//! Reads the coding tools' usage (`sources`): whole at launch, then what was added since, every
//! `poll_seconds`. A reply split over several records, or copied into a resumed session, counts
//! once (by its id). The daily totals are merged into the history (`tokens.json`), a day keeping
//! the larger of the two: the transcripts the tools delete after a while never lower it.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};
use prism_base::tr;

use chrono::{Local, Utc};
use tokio::sync::Notify;

use super::{
    sources::{self, claude_dir, codex_dir, command_code_dir, jsonl_files, opencode_db, CodexFile, Reply, Tail, CLAUDE, CODEX, COMMAND_CODE, OPENCODE},
    Day, TokensConfig, TokensState,
};
use crate::{agents::usage, BlockCtx};

static REFRESH: LazyLock<Notify> = LazyLock::new(Notify::new);

/// What was read so far, kept across the collectors' reloads.
static SCAN: LazyLock<Arc<tokio::sync::Mutex<Option<Scan>>>> = LazyLock::new(Default::default);
/// The history as last published, for the days the snapshot leaves out (`days_before`).
static HISTORY: LazyLock<std::sync::Mutex<BTreeMap<String, Day>>> = LazyLock::new(Default::default);
/// Days the snapshot carries, today's included: the current month and the one before (what a
/// month is compared with), the weeks before. A year of days was most of the snapshot (382 KB
/// of 489), sent whenever today's changed; the older ones are asked when the block goes back.
pub const RECENT_DAYS: i64 = 70;

/// The history's days before `to` (`YYYY-MM-DD`, excluded), oldest first.
pub fn days_before(to: &str) -> Vec<Day> {
    HISTORY.lock().unwrap().range(..to.to_string()).map(|(_, d)| d.clone()).collect()
}

/// What the snapshot gets of the history: its recent days, and how far back it goes.
fn publish(ctx: &BlockCtx, history: &BTreeMap<String, Day>, read_at: Option<chrono::DateTime<Utc>>) {
    *HISTORY.lock().unwrap() = history.clone();
    ctx.publish(&shown(history, &recent_from(), read_at));
}

/// The first day the snapshot carries.
fn recent_from() -> String {
    (Local::now().date_naive() - chrono::Duration::days(RECENT_DAYS - 1)).format("%Y-%m-%d").to_string()
}

fn shown(history: &BTreeMap<String, Day>, from: &str, read_at: Option<chrono::DateTime<Utc>>) -> TokensState {
    let days: Vec<Day> = history.range(from.to_string()..).map(|(_, d)| d.clone()).collect();
    let oldest = history.keys().next().filter(|first| first.as_str() < from).cloned();
    TokensState { days, oldest, read_at }
}

/// ↻ in the block: the next pass now.
pub fn refresh_now() {
    REFRESH.notify_one();
}

pub async fn run(cfg: TokensConfig, ctx: BlockCtx) {
    let path = ctx.data_dir().join("tokens.json");
    let mut history = load(&path);
    publish(&ctx, &history, None);
    let generation = prism_base::rt::Generation::current();
    let poll = Duration::from_secs(cfg.poll_seconds.max(60));
    loop {
        // Files are read on a blocking thread: the first pass reads every transcript. The scan
        // outlives a reload (only what was added since is read again), unless the tools read
        // change; the new generation waits for the old one's pass, which stops between files.
        let held = SCAN.clone().lock_owned().await;
        let (tools, generation) = (cfg.tools(), generation.clone());
        let fresh = match tokio::task::spawn_blocking(move || {
            let mut held = held;
            let scan = match held.as_mut() {
                Some(scan) if scan.tools == tools => scan,
                _ => held.insert(Scan::new(tools)),
            };
            scan.generation = generation;
            scan.pass();
            days_of(&scan.days)
        })
        .await
        {
            Ok(fresh) => fresh,
            Err(e) => {
                ctx.set_error(Some(tr!("reading the transcripts: {err}", err = e)));
                return;
            }
        };
        let keep_from = (Local::now().date_naive() - chrono::Duration::days(i64::from(cfg.keep_days))).format("%Y-%m-%d").to_string();
        if merge(&mut history, fresh, &keep_from)
            && let Err(e) = save(&path, &history)
        {
            tracing::warn!("tokens: saving {}: {e:#}", path.display());
        }
        ctx.set_error(None);
        publish(&ctx, &history, Some(Utc::now()));
        tokio::select! {
            _ = tokio::time::sleep(poll) => {}
            _ = REFRESH.notified() => {}
        }
    }
}

fn load(path: &Path) -> BTreeMap<String, Day> {
    let days: Vec<Day> = std::fs::read(path).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
    days.into_iter().map(|d| (d.date.clone(), d)).collect()
}

fn save(path: &Path, history: &BTreeMap<String, Day>) -> anyhow::Result<()> {
    let days: Vec<&Day> = history.values().collect();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&days)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// The days read into the history: a day keeps whichever count is larger (the transcripts only
/// grow, until Claude Code deletes the old ones); those before `keep_from` leave. True if it changed.
fn merge(history: &mut BTreeMap<String, Day>, fresh: BTreeMap<String, Day>, keep_from: &str) -> bool {
    let mut changed = false;
    for (date, day) in fresh {
        // A day read again with its tools apart, or with a cost found since (a price added),
        // replaces the same day kept without.
        let keep = history.get(&date).is_some_and(|old| old.total() >= day.total() && old.replies >= day.replies && (!old.by_tool.is_empty() || day.by_tool.is_empty()) && old.unpriced <= day.unpriced);
        if !keep {
            history.insert(date, day);
            changed = true;
        }
    }
    let before = history.len();
    history.retain(|date, _| date.as_str() >= keep_from);
    changed || history.len() != before
}

/// What the sources gave so far: where each file was left, each Codex rollout's state, the
/// replies already counted, and the days they sum to.
struct Scan {
    tools: Vec<&'static str>,
    tail: Tail,
    codex: HashMap<PathBuf, CodexFile>,
    projects: HashMap<String, String>,
    /// OpenCode's messages read up to this change (milliseconds).
    opencode_since: i64,
    seen: HashSet<String>,
    days: BTreeMap<String, Acc>,
    /// The collectors' generation it reads for: once stopped, the pass stops.
    generation: Option<prism_base::rt::Generation>,
}

/// Where each source is; `root`: where a Command Code folder name is looked up (`/`).
struct Places {
    claude: Option<PathBuf>,
    codex: Option<PathBuf>,
    command_code: Option<PathBuf>,
    opencode: Option<PathBuf>,
    root: PathBuf,
}

/// A day being summed: its figures, its models', projects' and tools' tokens, and the same tool by tool.
#[derive(Default, Clone)]
struct Acc {
    day: Day,
    models: HashMap<String, u64>,
    projects: HashMap<String, u64>,
    tools: HashMap<String, u64>,
    by_tool: HashMap<String, Acc>,
}

impl Acc {
    fn add(&mut self, date: &str, r: &Reply, tokens: u64, cost: Option<f64>, model: &str) {
        let d = &mut self.day;
        d.date = date.to_string();
        d.input += r.input;
        d.output += r.output;
        d.cache_read += r.cache_read;
        d.cache_write += r.cache_write;
        d.replies += 1;
        match cost {
            Some(c) => d.cost += c,
            None => d.unpriced += tokens,
        }
        *self.models.entry(model.to_string()).or_default() += tokens;
        *self.projects.entry(r.project.clone()).or_default() += tokens;
        *self.tools.entry(r.tool.to_string()).or_default() += tokens;
    }
}

impl Scan {
    fn new(tools: Vec<&'static str>) -> Self {
        Self { tools, tail: Tail::default(), codex: HashMap::new(), projects: HashMap::new(), opencode_since: 0, seen: HashSet::new(), days: BTreeMap::new(), generation: None }
    }

    fn on(&self, tool: &str) -> bool {
        self.tools.contains(&tool)
    }

    fn stopped(&self) -> bool {
        self.generation.as_ref().is_some_and(|g| g.cancelled())
    }

    /// One read of every source on, where it is on this computer: what was added since the last one.
    fn pass(&mut self) {
        let at = Places { claude: claude_dir(), codex: codex_dir(), command_code: command_code_dir(), opencode: opencode_db(), root: PathBuf::from("/") };
        self.pass_in(&at);
    }

    /// Stopped (its generation was), it reads no further file, but counts the ones it read: it
    /// stays where it is, for the next generation to go on from there.
    fn pass_in(&mut self, at: &Places) {
        let mut replies = Vec::new();
        if self.on(CLAUDE) && let Some(root) = &at.claude {
            for file in jsonl_files(root, 4) {
                if self.stopped() {
                    break;
                }
                self.tail.lines(&file, |l| replies.extend(sources::claude(l)));
            }
        }
        if self.on(CODEX) && !self.stopped() && let Some(root) = &at.codex {
            for file in jsonl_files(root, 4) {
                if self.stopped() {
                    break;
                }
                let state = self.codex.entry(file.clone()).or_default();
                self.tail.lines(&file, |l| replies.extend(sources::codex(l, state)));
            }
        }
        if self.on(COMMAND_CODE) && !self.stopped() && let Some(root) = &at.command_code {
            for file in jsonl_files(root, 1).into_iter().filter(|f| !f.to_string_lossy().ends_with(".checkpoints.jsonl")) {
                if self.stopped() {
                    break;
                }
                let folder = file.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let project = sources::command_code_project(&folder, &at.root, &mut self.projects);
                self.tail.lines(&file, |l| replies.extend(sources::command_code(l, &project)));
            }
        }
        if self.on(OPENCODE) && !self.stopped() && let Some(db) = at.opencode.as_ref().filter(|p| p.is_file()) {
            match sources::opencode(db, self.opencode_since) {
                Ok((found, since)) => {
                    replies.extend(found);
                    self.opencode_since = since;
                }
                Err(e) => tracing::warn!("tokens: reading OpenCode's database: {e:#}"),
            }
        }
        for r in replies {
            self.add(r);
        }
    }

    /// A reply into its day, once (a reply split over several records, or copied into a resumed
    /// session, has the same id). Claude Code's and Codex's cost is estimated at the API prices,
    /// the others' are their own.
    fn add(&mut self, r: Reply) {
        if let Some(id) = &r.id
            && !self.seen.insert(format!("{}:{id}", r.tool))
        {
            return;
        }
        let date = r.at.with_timezone(&Local).format("%Y-%m-%d").to_string();
        let tokens = r.input + r.output + r.cache_read + r.cache_write;
        let cost = if r.tool == CLAUDE || r.tool == CODEX { usage::cost(&r.model, r.input, r.output, r.cache_read, r.cache_write, r.long_writes) } else { r.cost };
        let model = family(&r.model);
        let acc = self.days.entry(date.clone()).or_default();
        acc.add(&date, &r, tokens, cost, &model);
        acc.by_tool.entry(r.tool.to_string()).or_default().add(&date, &r, tokens, cost, &model);
    }
}

impl From<Acc> for Day {
    fn from(a: Acc) -> Self {
        let ranked = |m: HashMap<String, u64>| {
            let mut v: Vec<(String, u64)> = m.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            v
        };
        let by_tool = a.by_tool.into_iter().map(|(t, acc)| (t, Day::from(acc))).collect();
        Day { models: ranked(a.models), projects: ranked(a.projects), tools: ranked(a.tools), by_tool, ..a.day }
    }
}

/// The days of a scan, as the history keeps them.
fn days_of(days: &BTreeMap<String, Acc>) -> BTreeMap<String, Day> {
    days.iter().map(|(k, a)| (k.clone(), Day::from(a.clone()))).collect()
}

/// `claude-opus-5-5[1m]`, `claude-sonnet-4-6-20260101` → the model without its window or date.
fn family(model: &str) -> String {
    let m = model.split('[').next().unwrap_or(model);
    let m = match m.rsplit_once('-') {
        Some((head, date)) if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => m,
    };
    if m.is_empty() { "unknown".into() } else { m.to_string() }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn the_snapshot_carries_the_recent_days_and_the_rest_on_demand() {
        let day = |date: &str| (date.to_string(), Day { date: date.into(), input: 1, replies: 1, ..Default::default() });
        let history: BTreeMap<String, Day> = [day("2025-11-02"), day("2026-08-01"), day("2026-08-12"), day("2026-10-19")].into_iter().collect();
        let s = shown(&history, "2026-08-12", None);
        assert_eq!(s.days.iter().map(|d| d.date.as_str()).collect::<Vec<_>>(), ["2026-08-12", "2026-10-19"]);
        assert_eq!(s.oldest.as_deref(), Some("2025-11-02"));
        // All of it recent: nothing older to say.
        assert_eq!(shown(&history, "2025-01-01", None).oldest, None);
        *HISTORY.lock().unwrap() = history;
        assert_eq!(days_before("2026-08-12").iter().map(|d| d.date.as_str()).collect::<Vec<_>>(), ["2025-11-02", "2026-08-01"]);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("prism-tokens-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn noon(d: u32) -> String {
        Local.with_ymd_and_hms(2026, 10, d, 12, 0, 0).unwrap().with_timezone(&Utc).to_rfc3339()
    }

    fn claude(id: &str, d: u32, model: &str, cwd: &str, input: u64, output: u64) -> String {
        serde_json::json!({ "type": "assistant", "timestamp": noon(d), "cwd": cwd, "message": { "id": id, "model": model, "usage": { "input_tokens": input, "output_tokens": output, "cache_read_input_tokens": 100, "cache_creation_input_tokens": 0 } } }).to_string()
    }

    fn places(dir: &Path) -> Places {
        Places { claude: Some(dir.join("claude")), codex: Some(dir.join("codex")), command_code: Some(dir.join("cc")), opencode: Some(dir.join("opencode.db")), root: dir.join("disk") }
    }

    #[test]
    fn a_pass_stopped_by_a_reload_counts_what_it_read_and_the_next_goes_on() {
        let dir = scratch("stopped");
        let project = dir.join("claude").join("-Users-me-shop");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("a.jsonl"), format!("{}\n", claude("m1", 13, "claude-opus-5-5", "/Users/me/shop", 10, 0))).unwrap();
        std::fs::write(project.join("b.jsonl"), format!("{}\n", claude("m2", 13, "claude-opus-5-5", "/Users/me/shop", 5, 0))).unwrap();
        let mut scan = Scan::new(sources::ALL.to_vec());
        let stopped = prism_base::rt::Generation::default();
        stopped.cancel();
        scan.generation = Some(stopped);
        scan.pass_in(&places(&dir));
        assert_eq!(days_of(&scan.days).get("2026-10-13").map(|d| d.input), None);
        // The next generation's pass: everything once.
        scan.generation = None;
        scan.pass_in(&places(&dir));
        scan.pass_in(&places(&dir));
        assert_eq!(days_of(&scan.days)["2026-10-13"].input, 15);
    }

    #[test]
    fn sums_each_reply_once_by_day() {
        let dir = scratch("claude");
        let project = dir.join("claude").join("-Users-me-shop");
        std::fs::create_dir_all(project.join("sub")).unwrap();
        let a = [
            claude("m1", 13, "claude-opus-5-5[1m]", "/Users/me/shop", 10, 20),
            // The same reply's second content block: once.
            claude("m1", 13, "claude-opus-5-5[1m]", "/Users/me/shop", 10, 20),
            r#"{"type":"user","message":{"content":"hello"}}"#.to_string(),
            claude("m2", 14, "claude-sonnet-4-6-20260101", "/Users/me/docs", 1, 2),
        ]
        .join("\n");
        std::fs::write(project.join("s1.jsonl"), format!("{a}\n")).unwrap();
        // A resumed session copies the earlier replies: not counted again.
        std::fs::write(project.join("sub").join("s2.jsonl"), format!("{}\n", claude("m2", 14, "claude-sonnet-4-6-20260101", "/Users/me/docs", 1, 2))).unwrap();
        let mut scan = Scan::new(sources::ALL.to_vec());
        scan.pass_in(&places(&dir));
        let days = days_of(&scan.days);
        let d13 = &days["2026-10-13"];
        assert_eq!((d13.input, d13.output, d13.cache_read, d13.replies), (10, 20, 100, 1));
        assert_eq!(d13.models, [("claude-opus-5-5".to_string(), 130)]);
        assert_eq!(d13.projects, [("shop".to_string(), 130)]);
        assert_eq!(d13.tools, [("claude".to_string(), 130)]);
        assert!(d13.cost > 0.0 && d13.unpriced == 0);
        assert_eq!(days["2026-10-14"].models[0].0, "claude-sonnet-4-6");

        // Appended since: only the new reply is read.
        let more = claude("m3", 14, "mystery-model", "/Users/me/docs", 5, 5);
        std::fs::write(project.join("s1.jsonl"), format!("{a}\n{more}\n")).unwrap();
        scan.pass_in(&places(&dir));
        let d14 = &days_of(&scan.days)["2026-10-14"];
        assert_eq!((d14.replies, d14.unpriced), (2, 110));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Codex, Command Code and OpenCode beside Claude Code, each in its tool; a tool turned off is not read.
    #[test]
    fn every_tool_into_the_same_days() {
        let dir = scratch("tools");
        // Codex: the model and folder come first; a token_count sent again unchanged counts once.
        let rollout = dir.join("codex").join("2026").join("10").join("13");
        std::fs::create_dir_all(&rollout).unwrap();
        let count = |input: u64, cached: u64, output: u64, total: u64| serde_json::json!({ "timestamp": noon(13), "type": "event_msg", "payload": { "type": "token_count", "info": { "last_token_usage": { "input_tokens": input, "cached_input_tokens": cached, "output_tokens": output, "total_tokens": input + output }, "total_token_usage": { "total_tokens": total } } } }).to_string();
        let lines = [
            serde_json::json!({ "timestamp": noon(13), "type": "session_meta", "payload": { "cwd": "/Users/me/api" } }).to_string(),
            serde_json::json!({ "timestamp": noon(13), "type": "turn_context", "payload": { "model": "gpt-5-codex", "cwd": "/Users/me/api" } }).to_string(),
            count(1000, 800, 50, 1050),
            count(1000, 800, 50, 1050),
            r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#.to_string(),
        ];
        std::fs::write(rollout.join("rollout-x.jsonl"), lines.join("\n") + "\n").unwrap();
        // Command Code: its folder name resolved on disk, its own cost.
        std::fs::create_dir_all(dir.join("disk").join("Work").join("poc-synth")).unwrap();
        let cc = dir.join("cc").join("work-poc-synth");
        std::fs::create_dir_all(&cc).unwrap();
        let reply = serde_json::json!({ "id": "c1", "type": "assistant", "timestamp": noon(13), "model": "claude-sonnet-5-5", "usage": { "inputTokens": 7, "outputTokens": 3, "cacheReadTokens": 90, "cacheWriteTokens": 0, "costUsd": 0.5 } }).to_string();
        std::fs::write(cc.join("s.jsonl"), format!("{reply}\n{reply}\n")).unwrap();
        std::fs::write(cc.join("s.checkpoints.jsonl"), format!("{}\n", reply.replace("c1", "c2"))).unwrap();
        // OpenCode: completed assistant messages only.
        let db = rusqlite::Connection::open(dir.join("opencode.db")).unwrap();
        db.execute_batch("CREATE TABLE message (id text PRIMARY KEY, session_id text, time_created integer, time_updated integer, data text)").unwrap();
        let done = Local.with_ymd_and_hms(2026, 10, 14, 9, 0, 0).unwrap().timestamp_millis();
        let msg = |role: &str, completed: Option<i64>| serde_json::json!({ "role": role, "time": { "created": done, "completed": completed }, "modelID": "gpt-5", "path": { "cwd": "/Users/me/site" }, "cost": 0.25, "tokens": { "input": 40, "output": 10, "reasoning": 5, "cache": { "read": 200, "write": 0 } } }).to_string();
        for (id, role, completed, updated) in [("o1", "assistant", Some(done), 5), ("o2", "user", None, 6), ("o3", "assistant", None, 7)] {
            db.execute("INSERT INTO message VALUES (?1, 's', 0, ?2, ?3)", rusqlite::params![id, updated, msg(role, completed)]).unwrap();
        }
        drop(db);

        let mut scan = Scan::new(sources::ALL.to_vec());
        scan.pass_in(&places(&dir));
        let days = days_of(&scan.days);
        let d13 = &days["2026-10-13"];
        assert_eq!(d13.tools, [("codex".to_string(), 1050), ("command-code".to_string(), 100)]);
        assert_eq!((d13.input, d13.cache_read, d13.output), (200 + 7, 800 + 90, 50 + 3));
        assert_eq!(d13.projects, [("api".to_string(), 1050), ("poc-synth".to_string(), 100)]);
        // Codex's at the API prices; Command Code's is its own.
        let codex_cost = (200.0 * 1.25 + 800.0 * 0.125 + 50.0 * 10.0) * 1e-6;
        assert_eq!(d13.unpriced, 0);
        assert!((d13.cost - (0.5 + codex_cost)).abs() < 1e-12);
        // Tool by tool: each its own figures, models and projects.
        let codex = &d13.by_tool["codex"];
        assert_eq!((codex.input, codex.cache_read, codex.output, codex.unpriced), (200, 800, 50, 0));
        assert!((codex.cost - codex_cost).abs() < 1e-12);
        assert_eq!((codex.models.clone(), codex.projects.clone()), (vec![("gpt-5-codex".to_string(), 1050)], vec![("api".to_string(), 1050)]));
        assert_eq!((d13.by_tool["command-code"].cost, d13.by_tool.len()), (0.5, 2));
        let d14 = &days["2026-10-14"];
        assert_eq!((d14.tools.clone(), d14.output, d14.cost, d14.projects[0].0.as_str()), (vec![("opencode".to_string(), 255)], 15, 0.25, "site"));

        // The message completed since: read on the next pass, by its change.
        let db = rusqlite::Connection::open(dir.join("opencode.db")).unwrap();
        db.execute("UPDATE message SET time_updated = 8, data = ?1 WHERE id = 'o3'", [msg("assistant", Some(done))]).unwrap();
        drop(db);
        scan.pass_in(&places(&dir));
        assert_eq!(days_of(&scan.days)["2026-10-14"].replies, 2);

        // Only Claude Code: nothing of the others.
        let mut claude_only = Scan::new(vec![CLAUDE]);
        claude_only.pass_in(&places(&dir));
        assert!(claude_only.days.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// This computer's own sessions, read once: totals per tool only (`--ignored --nocapture`).
    #[test]
    #[ignore]
    fn reads_this_computer() {
        let started = std::time::Instant::now();
        let mut scan = Scan::new(sources::ALL.to_vec());
        scan.pass();
        let days = days_of(&scan.days);
        let mut tools: BTreeMap<String, (u64, u32)> = BTreeMap::new();
        for d in days.values() {
            for (t, n) in &d.tools {
                tools.entry(t.clone()).or_default().0 += n;
            }
        }
        for d in days.values() {
            if let Some((t, _)) = d.tools.first() {
                tools.entry(t.clone()).or_default().1 += 1;
            }
        }
        println!("{} days, {:?} in {:.1}s", days.len(), tools, started.elapsed().as_secs_f32());
        println!("from {:?} to {:?}", days.keys().next(), days.keys().last());
    }

    #[test]
    fn the_history_keeps_the_larger_day_and_drops_the_old_ones() {
        let day = |date: &str, input: u64| Day { date: date.into(), input, replies: 1, ..Default::default() };
        let mut history: BTreeMap<String, Day> = [("2025-01-01", 5), ("2026-10-12", 50), ("2026-10-13", 10)].into_iter().map(|(d, n)| (d.to_string(), day(d, n))).collect();
        // The 12th's transcripts were deleted: fewer read than kept.
        let fresh: BTreeMap<String, Day> = [("2026-10-12", 20), ("2026-10-13", 30)].into_iter().map(|(d, n)| (d.to_string(), day(d, n))).collect();
        assert!(merge(&mut history, fresh, "2026-01-01"));
        assert_eq!(history.values().map(|d| (d.date.as_str(), d.input)).collect::<Vec<_>>(), [("2026-10-12", 50), ("2026-10-13", 30)]);
        // The same day read again with its tools apart: taken, though no larger.
        let mut split = day("2026-10-13", 30);
        split.by_tool.insert("claude".into(), day("2026-10-13", 30));
        assert!(merge(&mut history, [("2026-10-13".to_string(), split)].into(), "2026-01-01"));
        assert!(!merge(&mut history, [("2026-10-13".to_string(), day("2026-10-13", 30))].into(), "2026-01-01"));
        assert_eq!(history["2026-10-13"].by_tool.len(), 1);
        // Read again with a cost where it had none (a price added): taken too.
        let mut priced = history["2026-10-13"].clone();
        history.get_mut("2026-10-13").unwrap().unpriced = 30;
        priced.cost = 0.1;
        assert!(merge(&mut history, [("2026-10-13".to_string(), priced)].into(), "2026-01-01"));
        assert_eq!(history["2026-10-13"].unpriced, 0);
    }
}
