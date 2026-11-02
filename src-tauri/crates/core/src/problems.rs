//! Prism's recent problems, kept to be shown: the warnings and errors it logs (a pack's key
//! ignored, a block that does not start, a command that fails…), which otherwise only reach
//! `prism.log`, where nobody looks. The window lists them from the status bar (`Snapshot.problems`).
//! A problem met again is counted on its line rather than added; the oldest go past `KEPT`.

use std::{collections::VecDeque, fmt::Write as _, sync::Mutex};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::watch;
use tracing::{field::Field, Event, Level, Subscriber};
use tracing_subscriber::{layer::Context, Layer};

/// The problems kept, most recent first.
const KEPT: usize = 50;
/// A message beyond this length is cut (a whole stderr, a body).
const MAX_CHARS: usize = 600;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Problem {
    /// `error` or `warn`.
    pub level: String,
    /// Where it was logged: the module, without its crate's `prism_` (`collectors::gitlab`).
    pub target: String,
    pub message: String,
    /// The last time it was logged, and how many times since launch.
    pub at: DateTime<Utc>,
    pub count: u32,
}

static PROBLEMS: Mutex<VecDeque<Problem>> = Mutex::new(VecDeque::new());
static CHANGED: std::sync::LazyLock<watch::Sender<()>> = std::sync::LazyLock::new(|| watch::channel(()).0);

/// The problems kept, most recent first.
pub fn list() -> Vec<Problem> {
    PROBLEMS.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
}

/// Changes on each problem logged.
pub fn changes() -> watch::Receiver<()> {
    CHANGED.subscribe()
}

/// A problem met again (a source failing each round, offline) updates its line's time and count
/// in the snapshot at most this often: a new problem, or one gone, at once.
const REPEATS_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// The same problems, maybe in another order or with other counts.
fn same_problems(a: &[Problem], b: &[Problem]) -> bool {
    let key = |p: &Problem| (p.level.clone(), p.message.clone());
    let set = |l: &[Problem]| l.iter().map(key).collect::<std::collections::HashSet<_>>();
    a.len() == b.len() && set(a) == set(b)
}

/// Keeps `Snapshot.problems` up to date, a burst of problems in one change.
pub async fn run(hub: crate::state::Hub) {
    let mut rx = changes();
    let mut pushed = std::time::Instant::now();
    loop {
        let list = list();
        hub.update(|s| {
            let changed = if same_problems(&s.problems, &list) { s.problems != list && pushed.elapsed() >= REPEATS_EVERY } else { true };
            if changed {
                s.problems = list;
                pushed = std::time::Instant::now();
            }
            changed
        });
        if rx.changed().await.is_err() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        rx.mark_unchanged();
    }
}

/// Forgets them all (the panel's Clear).
pub fn clear() {
    PROBLEMS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    CHANGED.send_replace(());
}

fn record(level: &Level, target: &str, message: String) {
    let level = if *level == Level::ERROR { "error" } else { "warn" };
    let target = target.strip_prefix("prism_").unwrap_or(target).to_string();
    let message: String = message.chars().take(MAX_CHARS).collect();
    {
        let mut list = PROBLEMS.lock().unwrap_or_else(|e| e.into_inner());
        let count = match list.iter().position(|p| p.level == level && p.message == message) {
            Some(i) => list.remove(i).map_or(1, |p| p.count.saturating_add(1)),
            None => 1,
        };
        list.push_front(Problem { level: level.into(), target, message, at: Utc::now(), count });
        list.truncate(KEPT);
    }
    CHANGED.send_replace(());
}

/// The layer that keeps Prism's own warnings and errors (`logs::init`), at the level the logs
/// are written at: under `[logs] level = "error"`, the warnings are not kept either.
pub struct Keep;

impl<S: Subscriber> Layer<S> for Keep {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        if *meta.level() > Level::WARN || !meta.target().starts_with("prism") {
            return;
        }
        let mut text = Text::default();
        event.record(&mut text);
        record(meta.level(), meta.target(), text.0);
    }
}

/// An event's message, then its other fields as `key=value`.
#[derive(Default)]
struct Text(String);

impl tracing::field::Visit for Text {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let rest = std::mem::take(&mut self.0);
            let _ = write!(self.0, "{value:?}");
            if !rest.is_empty() {
                let _ = write!(self.0, " {rest}");
            }
        } else {
            let sep = if self.0.is_empty() { "" } else { " " };
            let _ = write!(self.0, "{sep}{}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.record_debug(field, &format_args!("{value}"));
        } else {
            self.record_debug(field, &value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn keeps_warnings_and_errors_counted() {
        let subscriber = tracing_subscriber::registry().with(Keep);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("not kept");
            tracing::warn!("packs: [x] is personal, ignored in a pack");
            tracing::error!(repo = "core", "boom");
            tracing::warn!("packs: [x] is personal, ignored in a pack");
        });
        let list = list();
        let ours: Vec<_> = list.iter().filter(|p| p.target.starts_with("core::problems")).collect();
        assert_eq!(ours.len(), 2, "{ours:?}");
        assert_eq!((ours[0].level.as_str(), ours[0].message.as_str(), ours[0].count), ("warn", "packs: [x] is personal, ignored in a pack", 2));
        assert_eq!((ours[1].level.as_str(), ours[1].message.as_str()), ("error", "boom repo=\"core\""));
    }

    /// A problem met again changes the snapshot only now and then; a new one at once.
    #[test]
    fn a_repeat_is_the_same_problems() {
        let p = |m: &str, count| Problem { level: "warn".into(), target: "t".into(), message: m.into(), at: Utc::now(), count };
        assert!(same_problems(&[p("a", 1), p("b", 1)], &[p("b", 2), p("a", 1)]));
        assert!(!same_problems(&[p("a", 1)], &[p("a", 1), p("b", 1)]));
        assert!(!same_problems(&[p("a", 1)], &[p("c", 1)]));
    }
}
