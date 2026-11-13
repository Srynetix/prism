//! The alerts, computed by the core from the snapshot shortly after it changes (`run`, woken by
//! `Hub::update`, the changes of a moment taken together), and once a minute for what depends on
//! the time: `Snapshot.alerts`, which the window, a phone, the routines and the Changes block read.
//! Prism's rules are here (`rules`, `worktrees`), a block's own in its `Block::alerts`; the window
//! adds those of the modules' blocks that only say theirs in TypeScript, and tells the
//! core (`Hub::set_external_alerts`) for the routines and the Changes block. Its texts follow
//! `[appearance] language`.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::{Snapshot, StatusOption};

mod rules;
mod worktrees;
#[cfg(test)]
mod tests;

pub use rules::access_alert;

/// How bad: broken (`ko`), to look at (`warn`), to know (`info`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Ko,
    Warn,
    Info,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ko => "ko",
            Self::Warn => "warn",
            Self::Info => "info",
        }
    }
}

/// What raised an alert (`Alert.rule`), as `[alerts] band_*` names them.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum AlertRule {
    Question,
    Permission,
    Proposal,
    YourTurn,
    AgentCost,
    AgentLoop,
    AgentLimit,
    Review,
    OpenThreads,
    MrCi,
    Conflicts,
    RebaseFailed,
    NeedRebase,
    OnlyCi,
    NoReviewer,
    NoTicket,
    Sync,
    Queue,
    BranchCi,
    Infra,
    Argo,
    Source,
    Block,
    Worktree,
    Overlap,
}

/// A notification when broken (`ko`): one per `key`, the object, not its changing detail.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "AlertNotification"))]
pub struct Notification {
    pub key: String,
    pub title: String,
}

/// An alert's object, opened in the app: a ticket's preview, an MR's pane, a pipeline's logs, the ArgoCD drawer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum OpenTarget {
    Ticket {
        id: String,
    },
    Mr {
        iid: u64,
        #[cfg_attr(feature = "ts", ts(optional = nullable))]
        repo: Option<String>,
    },
    Pipeline {
        pipeline: u64,
        label: String,
        #[cfg_attr(feature = "ts", ts(optional = nullable))]
        #[serde(skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    Argo {
        cluster: String,
        app: String,
    },
}

/// The agent action a `claude` fix runs on an MR (`ClaudeActionId` in the window).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum FixAction {
    Review,
    FixPipeline,
    Rebase,
    ReviewFix,
}

/// A side of an overlap.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "OverlapSide"))]
#[serde(rename_all = "lowercase")]
pub enum Side {
    A,
    B,
}

/// Fix of an alert, as data, offered in one click in Alerts (the window executes it). `status`:
/// change the status of a ticket; `rebase`: rebase by the forge; `queue`: into the merge queue;
/// `retry` / `logs`: retry the failed jobs or open a pipeline's logs; `mr`: open an MR's pane;
/// `claude`: the agent action of that name for the MR; `fix-branch`: an agent fixing a protected
/// branch's pipeline; `investigate`: an agent diagnosing a workload; `dev-server`: the server's own
/// agent, told why it stopped; `switch`: another CLI to continue an agent stopped at its plan's
/// limit; `block`: a command of a block's Rust side; `changes`: the Changes timeline around the
/// application's crash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Fix {
    Status {
        task_id: String,
        ticket: String,
        status: String,
        statuses: Vec<StatusOption>,
    },
    Rebase {
        iid: u64,
        #[cfg_attr(feature = "ts", ts(optional = nullable))]
        repo: Option<String>,
    },
    Mr {
        iid: u64,
        #[cfg_attr(feature = "ts", ts(optional = nullable))]
        repo: Option<String>,
    },
    Queue {
        iid: u64,
        #[cfg_attr(feature = "ts", ts(optional = nullable))]
        repo: Option<String>,
    },
    Automerge {
        iid: u64,
        #[cfg_attr(feature = "ts", ts(optional = nullable))]
        repo: Option<String>,
    },
    Retry {
        pipeline: u64,
        label: String,
        #[cfg_attr(feature = "ts", ts(optional = nullable))]
        #[serde(skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    Logs {
        pipeline: u64,
        label: String,
        #[cfg_attr(feature = "ts", ts(optional = nullable))]
        #[serde(skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    Claude {
        iid: u64,
        #[cfg_attr(feature = "ts", ts(optional = nullable))]
        repo: Option<String>,
        action: FixAction,
    },
    Podlogs {
        context: String,
        namespace: String,
        deployment: String,
        label: String,
    },
    Settings {
        section: String,
        label: String,
    },
    FixBranch {
        pipeline: u64,
    },
    Investigate {
        workload: String,
        namespace: String,
        context: String,
        env: String,
        issue: String,
    },
    DevServer {
        agent: String,
        command: String,
        #[serde(rename = "where")]
        #[cfg_attr(feature = "ts", ts(rename = "where"))]
        place: String,
        tail: String,
    },
    Switch {
        agent: String,
    },
    Overlap {
        id: String,
        side: Side,
    },
    Changes {
        env: Option<String>,
        app: String,
    },
    Block {
        block: String,
        command: String,
        #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
        args: Value,
        label: String,
        title: String,
    },
    Proposal {
        id: u64,
    },
}

/// Something to know or to do, in Alerts, the status bar, the notifications and on a phone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Alert {
    /// `Source`, `Infra`, `CI`, `Sync`, `Review`, `MR`, `Claude`, `ArgoCD`, `Worktrees`; a block's
    /// alerts take its label.
    pub kind: String,
    /// The block with a source of its own that raised it.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// The block's own item that raised it (a custom block's id), or a source's error the source
    /// (`Snapshot.errors`' key): a phone is not told those of a custom block kept on the computer,
    /// nor the mail's when `[remote] mail` keeps it there.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification: Option<Notification>,
    pub severity: Severity,
    pub text: String,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// What "Open" shows in the app (a drawer) instead of `url`, which ⌘-click keeps.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open: Option<OpenTarget>,
    /// `false`: shown in Alerts, but without a notification (pending jobs…).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify: Option<bool>,
    /// Fixes offered in one click in Alerts.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixes: Option<Vec<Fix>>,
    /// The agent the alert waits on: its quick answers in the row.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// The rule that raised it: its band (`[alerts] band_now`, `band_later`, `band_hidden`).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<AlertRule>,
    /// Since when it holds, when the data says it; otherwise Prism counts from when it first saw it.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    /// Its few words beside the others of its object (the same MR, the same ticket), grouped.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
}

impl Alert {
    pub fn new(kind: impl Into<String>, severity: Severity, text: impl Into<String>) -> Self {
        Alert { kind: kind.into(), block: None, item: None, notification: None, severity, text: text.into(), url: None, open: None, notify: None, fixes: None, answer: None, rule: None, since: None, short: None }
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn open(mut self, open: OpenTarget) -> Self {
        self.open = Some(open);
        self
    }

    pub fn notify(mut self, notify: bool) -> Self {
        self.notify = Some(notify);
        self
    }

    pub fn notification(mut self, key: impl Into<String>, title: impl Into<String>) -> Self {
        self.notification = Some(Notification { key: key.into(), title: title.into() });
        self
    }

    pub fn fixes(mut self, fixes: Vec<Fix>) -> Self {
        self.fixes = Some(fixes);
        self
    }

    pub fn rule(mut self, rule: AlertRule) -> Self {
        self.rule = Some(rule);
        self
    }

    pub fn since(mut self, since: Option<DateTime<Utc>>) -> Self {
        self.since = since;
        self
    }

    pub fn short(mut self, short: impl Into<String>) -> Self {
        self.short = Some(short.into());
        self
    }

    pub fn item(mut self, item: impl Into<String>) -> Self {
        self.item = Some(item.into());
        self
    }
}

/// What the alerts read of the stores the window and the phones share (`prefs`): the agents and
/// worktrees archived, the overlaps ignored, the routines' agents opened.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prefs {
    /// `agents.archived`: an agent's (or a Desktop session's) id → its last activity when archived.
    pub agents_archived: HashMap<String, String>,
    /// `worktrees.archived`: a worktree's path → its state when archived (`head|changes`).
    pub worktrees_archived: HashMap<String, String>,
    /// `overlaps.ignored`: an overlap's id → its key when ignored.
    pub overlaps_ignored: HashMap<String, String>,
    /// `routines.opened`: the routines' agents opened from To review, back in the lists.
    pub routines_opened: HashSet<String>,
}

impl Prefs {
    /// From the shared store `prefs`, as the window and the phones write it.
    pub fn of(value: &Value) -> Self {
        let map = |key: &str| -> HashMap<String, String> { value.get(key).and_then(Value::as_object).map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect()).unwrap_or_default() };
        Prefs {
            agents_archived: map("agents.archived"),
            worktrees_archived: map("worktrees.archived"),
            overlaps_ignored: map("overlaps.ignored"),
            routines_opened: value.get("routines.opened").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default(),
        }
    }
}

/// Every alert of `s` at `now`: Prism's rules, then the blocks' (`Block::alerts`), each with its rule.
pub fn compute(s: &Snapshot, prefs: &Prefs, now: DateTime<Utc>) -> Vec<Alert> {
    let x = rules::Ctx::new(s, prefs, now);
    let mut out = Vec::new();
    ruled(&mut out, AlertRule::Source, x.source_alerts());
    ruled(&mut out, AlertRule::Infra, x.infra_alerts());
    ruled(&mut out, AlertRule::Block, block_alerts(s));
    ruled(&mut out, AlertRule::Argo, x.argo_alerts());
    ruled(&mut out, AlertRule::BranchCi, x.ci_alerts());
    let mut sync = x.sync_alerts();
    sync.extend(x.draft_alerts());
    sync.extend(x.in_review_alerts());
    sync.extend(x.approved_alerts());
    ruled(&mut out, AlertRule::Sync, sync);
    ruled(&mut out, AlertRule::Queue, x.queue_alerts());
    out.extend(x.session_alerts());
    ruled(&mut out, AlertRule::Proposal, x.proposal_alerts());
    ruled(&mut out, AlertRule::Overlap, x.overlap_alerts());
    ruled(&mut out, AlertRule::Worktree, x.worktree_alerts());
    ruled(&mut out, AlertRule::Review, x.review_alerts());
    out.extend(x.my_mr_alerts());
    out
}

/// A producer's rule, on those of its alerts that name none of their own.
fn ruled(out: &mut Vec<Alert>, rule: AlertRule, list: Vec<Alert>) {
    out.extend(list.into_iter().map(|mut a| {
        a.rule.get_or_insert(rule);
        a
    }));
}

/// The alerts of the blocks with a source of their own, from their state: `block` set to the block's id.
fn block_alerts(s: &Snapshot) -> Vec<Alert> {
    crate::blocks::all()
        .iter()
        .filter_map(|b| s.blocks.get(b.id()).filter(|v| !v.is_null()).map(|state| (b, state)))
        .flat_map(|(b, state)| {
            b.alerts(state).into_iter().map(|mut a| {
                a.block = Some(b.id().to_string());
                a
            })
        })
        .collect()
}

/// The changes of a moment taken together before the alerts are computed again.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(250);
/// What depends on the time (a review waiting for hours, a job queued, a turn over a while ago)
/// comes and goes without a change: computed again this often.
const EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// The alerts computed while Prism runs, away from the snapshot's lock: shortly after a change
/// (`Hub::update` wakes it), and once a minute. Their own publication wakes nothing.
pub async fn run(hub: crate::state::Hub) {
    let wake = hub.alerts_wake();
    loop {
        tokio::select! {
            _ = wake.notified() => tokio::time::sleep(SETTLE).await,
            _ = tokio::time::sleep(EVERY) => {}
        }
        let hub = hub.clone();
        if let Err(e) = tokio::task::spawn_blocking(move || hub.refresh_alerts()).await {
            tracing::warn!("alerts: {e}");
        }
    }
}

/* ---------- Numbers as the window writes them ---------- */

/// `x` with `digits` decimals, a tie rounded up as JavaScript's `toFixed` does.
pub fn to_fixed(x: f64, digits: usize) -> String {
    let m = 10f64.powi(digits as i32);
    let y = x * m;
    if (y - y.trunc()).abs() == 0.5 {
        return format!("{:.*}", digits, (y.trunc() + y.signum()) / m);
    }
    format!("{x:.digits$}")
}

/// `Math.round`: a half up.
pub fn round(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

/// A number as JavaScript prints it: `35`, `35.5`.
pub fn js_number(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
}
