//! The component's side of the world: what Prism calls (`configure`, `start`, `tick`, `call`,
//! `hints`, `images`), answered by the blocks.
//!
//! A block's collector is a future (its loop, spawned in `start`). Prism ticks a part when due;
//! the futures of that part are polled then, each up to its next `pause`, which tells which part
//! it belongs to (its context's source) and when it is due again. A future not yet polled belongs
//! to the block that spawned it, and runs on the first tick of one of its parts.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use serde_json::{json, Value};

use crate::bindings::prism::module::types::{Configured, Published, Tick};
use crate::block::{Block, BlockCtx, BlockEnv, Stages};
use crate::types::Conventions;

/// A future pending without a pause (it waited on something Prism does not give): tried again
/// this much later.
const STALLED: Duration = Duration::from_secs(60);

type Task = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Running {
    future: Task,
    block: &'static str,
    /// The part its last pause was under; none until then.
    part: Option<&'static str>,
    /// When it is due again (ms since the epoch).
    at: i64,
    aborted: Arc<AtomicBool>,
}

#[derive(Default)]
struct State {
    config: toml::Table,
    tasks: Vec<Running>,
    /// Spawned while something ran: joined to `tasks` once it is done.
    spawned: Vec<Running>,
    /// The block whose `start` is running (what it spawns is its).
    starting: Option<&'static str>,
    published: BTreeMap<&'static str, Value>,
    /// Each source's error as it stands (none: it answered).
    errors: HashMap<&'static str, Option<String>>,
    /// Sources whose error changed since the last tick.
    changed: Vec<&'static str>,
    paused: Option<(&'static str, Duration)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn publish(block: &'static str, state: Value) {
    with(|s| s.published.insert(block, state));
}

pub(crate) fn set_error(source: &'static str, err: Option<String>) {
    with(|s| {
        if s.errors.get(source) != Some(&err) {
            s.errors.insert(source, err);
            if !s.changed.contains(&source) {
                s.changed.push(source);
            }
        }
    });
}

pub(crate) fn paused(source: &'static str, every: Duration) {
    with(|s| s.paused = Some((source, every)));
}

/// A future spawned by a block: polled when Prism ticks its part.
pub(crate) fn spawn(future: Task) -> Arc<AtomicBool> {
    let aborted = Arc::new(AtomicBool::new(false));
    with(|s| {
        let block = s.starting.unwrap_or("");
        s.spawned.push(Running { future, block, part: None, at: now_ms(), aborted: aborted.clone() });
    });
    aborted
}

fn find(blocks: &'static [&'static dyn Block], id: &str) -> Result<&'static dyn Block, String> {
    blocks.iter().copied().find(|b| b.id() == id).ok_or_else(|| format!("no block {id} in this module"))
}

/// The block a part belongs to: the one with that id or declaring it, or the first.
fn owner(blocks: &'static [&'static dyn Block], part: &str) -> &'static str {
    blocks.iter().find(|b| b.id() == part || b.parts().contains(&part)).or(blocks.first()).map_or("", |b| b.id())
}

/// `future` run to its end now: what Prism gives answers at once.
fn block_on<T>(future: impl Future<Output = T>) -> Result<T, String> {
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..1000 {
        if let Poll::Ready(v) = future.as_mut().poll(&mut cx) {
            return Ok(v);
        }
    }
    Err("it waited on something Prism does not give a module (a timer, a thread)".into())
}

pub fn configure(blocks: &'static [&'static dyn Block], config: String, lang: String, demo: bool) -> Result<Configured, String> {
    let table: toml::Table = toml::from_str(&config).map_err(|e| format!("config: {e}"))?;
    let lang = crate::Lang::of(&lang);
    crate::i18n::set(blocks.iter().flat_map(|b| b.messages(lang).iter().copied()));
    with(|s| *s = State { config: table.clone(), ..Default::default() });
    for b in blocks {
        b.configure(&table).map_err(|e| format!("[{}]: {e:#}", b.id()))?;
    }
    let parts: Vec<String> = blocks.iter().flat_map(|b| std::iter::once(b.id()).chain(b.parts().iter().copied())).map(String::from).collect();
    let stages = blocks.iter().find_map(|b| b.stages()).map(|p| serde_json::to_string(&p.stages().0).unwrap_or_default());
    let demo = if demo {
        let now = chrono::Utc::now();
        blocks.iter().filter_map(|b| Some((b.id().to_string(), b.demo(now)?.to_string()))).collect()
    } else {
        Vec::new()
    };
    Ok(Configured { parts, stages, demo })
}

pub fn start(blocks: &'static [&'static dyn Block], env: String) -> Result<(), String> {
    let value: Value = serde_json::from_str(&env).map_err(|e| format!("env: {e}"))?;
    let block = find(blocks, value["block"].as_str().unwrap_or_default())?;
    if let Ok(c) = serde_json::from_value::<Conventions>(value["conventions"].clone()) {
        crate::host::set_conventions(c);
    }
    let env: BlockEnv = serde_json::from_value(value).map_err(|e| format!("env: {e}"))?;
    let config = with(|s| {
        s.starting = Some(block.id());
        // Started again (a new config): its former loops stop.
        s.tasks.retain(|t| t.block != block.id());
        s.config.clone()
    });
    let started = block.start(&config, BlockCtx::new(block.id(), Arc::new(env)));
    with(|s| {
        s.starting = None;
        let spawned = std::mem::take(&mut s.spawned);
        s.tasks.extend(spawned);
    });
    started.map_err(|e| format!("{e:#}"))
}

pub fn tick(blocks: &'static [&'static dyn Block], part: String) -> Tick {
    let now = now_ms();
    let block = owner(blocks, &part);
    // The futures due: this part's, and those of its block not yet run.
    let due: Vec<Running> = with(|s| {
        let (due, rest): (Vec<Running>, Vec<Running>) = std::mem::take(&mut s.tasks).into_iter().partition(|t| (t.part == Some(part.as_str()) && t.at <= now) || (t.part.is_none() && t.block == block));
        s.tasks = rest;
        due
    });
    let mut cx = Context::from_waker(Waker::noop());
    for mut t in due {
        if t.aborted.load(Ordering::Relaxed) {
            continue;
        }
        with(|s| {
            s.paused = None;
            s.starting = Some(t.block);
        });
        let done = t.future.as_mut().poll(&mut cx).is_ready();
        let paused = with(|s| {
            s.starting = None;
            s.paused.take()
        });
        if done {
            continue;
        }
        match paused {
            Some((source, every)) => {
                t.part = Some(source);
                t.at = now_ms() + every.as_millis() as i64;
            }
            None => t.at = now_ms() + STALLED.as_millis() as i64,
        }
        with(|s| s.tasks.push(t));
    }
    with(|s| {
        let spawned = std::mem::take(&mut s.spawned);
        s.tasks.extend(spawned);
    });
    let now = chrono::Utc::now();
    let published: Vec<Published> = with(|s| std::mem::take(&mut s.published))
        .into_iter()
        .map(|(id, state)| {
            let b = blocks.iter().find(|b| b.id() == id);
            let alerts = b.map(|b| b.alerts(&state)).filter(|a| !a.is_empty()).map(|a| serde_json::to_string(&a).unwrap_or_default());
            let changes = b.map(|b| b.changes(&state, now)).filter(|c| !c.is_empty()).map(|c| serde_json::to_string(&c).unwrap_or_default());
            Published { block: id.to_string(), state: state.to_string(), alerts, changes }
        })
        .collect();
    let (error, errors, next) = with(|s| {
        let error = s.errors.get(part.as_str()).cloned().flatten();
        let changed = std::mem::take(&mut s.changed);
        let errors = changed.into_iter().filter(|c| *c != part).map(|c| (c.to_string(), s.errors.get(c).cloned().flatten())).collect();
        // When this part is due again: its soonest future; none left, never (until a new start).
        let mine = s.tasks.iter().filter(|t| t.part == Some(part.as_str()) || (t.part.is_none() && t.block == block));
        let next = mine.map(|t| t.at).min().map_or(0, |at| (((at - now_ms()).max(0) + 999) / 1000).max(1) as u32);
        (error, errors, next)
    });
    Tick { published, error, errors, next }
}

pub fn call(blocks: &'static [&'static dyn Block], block: String, command: String, args: String, demo: bool) -> Result<String, String> {
    let b = find(blocks, &block)?;
    let args: Value = if args.is_empty() { Value::Null } else { serde_json::from_str(&args).map_err(|e| format!("args: {e}"))? };
    let answer = if demo {
        b.demo_call(&command, &args).unwrap_or_else(|| Err(anyhow::anyhow!("The demo profile changes nothing: its data is a sample")))
    } else {
        block_on(b.call(command, args))?
    };
    answer.map(|v| v.to_string()).map_err(|e| format!("{e:#}"))
}

pub fn hints(blocks: &'static [&'static dyn Block], repo: String) -> Result<String, String> {
    let provider = blocks.iter().find_map(|b| b.stages()).ok_or("no block of this module declares stages")?;
    let hints = block_on(provider.hints(std::path::Path::new(&repo)))?.map_err(|e| format!("{e:#}"))?;
    let expected: Vec<Value> = hints.expected.into_iter().map(|((app, env), version)| json!({ "app": app, "env": env, "version": version })).collect();
    Ok(json!({ "apps": hints.apps, "expected": expected }).to_string())
}

pub fn images(blocks: &'static [&'static dyn Block], repo: String, project: String, local: String) -> Result<String, String> {
    let provider = blocks.iter().find_map(|b| b.images()).ok_or("no block of this module names images")?;
    let local: Vec<crate::block::LocalRepo> = serde_json::from_str(&local).map_err(|e| format!("local: {e}"))?;
    let targets = block_on(provider.images(std::path::Path::new(&repo), &project, &local))?.map_err(|e| format!("{e:#}"))?;
    serde_json::to_string(&targets).map_err(|e| e.to_string())
}

/// The stages the module declares, for a test.
#[allow(dead_code)]
pub(crate) fn stages_of(blocks: &'static [&'static dyn Block]) -> Option<Stages> {
    blocks.iter().find_map(|b| b.stages()).map(|p| p.stages())
}

/// What `spawn` returns: a handle to stop the future.
pub struct JoinHandle<T> {
    pub(crate) aborted: Arc<AtomicBool>,
    pub(crate) _output: std::marker::PhantomData<fn() -> T>,
}

impl<T> JoinHandle<T> {
    /// Stops it: it is not polled again.
    pub fn abort(&self) {
        self.aborted.store(true, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.aborted.load(Ordering::Relaxed)
    }
}

/// A value kept across starts and set again on each (rather than a `OnceLock`).
pub struct Swap<T: 'static>(Mutex<Option<&'static T>>);

impl<T: Send + Sync + 'static> Swap<T> {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub fn set(&self, value: T) -> &'static T {
        let value = Box::leak(Box::new(value));
        *self.0.lock().unwrap() = Some(value);
        value
    }

    pub fn get(&self) -> Option<&'static T> {
        *self.0.lock().unwrap()
    }
}

impl<T: Send + Sync + 'static> Default for Swap<T> {
    fn default() -> Self {
        Self::new()
    }
}
