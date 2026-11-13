//! Agents and Claude sessions, and the plan usage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};


/// An MR linked by hand to an agent: its repository (`None`: the primary one) and its number.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MrLink {
    pub repo: Option<String>,
    pub iid: u64,
}

/// What an agent was launched with, kept as it was then (a title or tags changed since stay the
/// agent's own): its Launch tab shows it, and launches the same again from it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LaunchSpec {
    pub title: String,
    /// The folder asked for (a repository, an existing worktree); absent with a new worktree.
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    /// The new worktree asked for.
    #[serde(default)]
    pub worktree: Option<WorktreeRequest>,
    /// `acceptEdits`, `plan`…; absent, the mode from the CLI's settings.
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// The CLI picked in the launcher; absent: the default, resolved then (the agent's `provider`).
    #[serde(default)]
    pub provider: Option<crate::agents::provider::Provider>,
    /// The CLI's account it runs under (`[[agents.accounts]]`, its name), kept on resume and
    /// changed by Continue with…; absent: the CLI's usual folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub account: Option<String>,
    /// The model picked in the launcher, kept on resume; absent: the CLI's (`[agents.<cli>] model`).
    #[serde(default)]
    pub model: Option<String>,
    /// The reasoning effort picked in the launcher or the agent's header, kept on resume; absent:
    /// the CLI's (`[agents.<cli>] effort`).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub effort: Option<String>,
    /// Its "Then" steps, as set at launch (not how they went).
    #[serde(default)]
    pub then: Vec<LaunchStep>,
    /// The action that opened the launcher (`Implement`, `Review`…); absent: a blank launch.
    #[serde(default)]
    pub from: Option<String>,
    /// Prism's MCP server and skill given to it whatever `[agents] prism_mcp` says ("Configure
    /// Prism"), kept on resume.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<bool>"))]
    pub prism_mcp: bool,
}

/// A new worktree as asked at launch: its repository, its folder name, the existing branch to
/// check out, or the branch its new one starts from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WorktreeRequest {
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub base: Option<String>,
    /// Other repositories (`[[repos]] name`) given a worktree of the same name and branch: the
    /// agent works in all of them at once (its CLI's `--add-dir`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub also: Vec<String>,
}

/// What a "Then" step does: run a task, prompt the agent, push and open a draft MR, launch a
/// review agent, notify, launch what it saw in passing (`text`: the proposal's id).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum StepKind {
    Task,
    Prompt,
    Mr,
    Agent,
    Notify,
    Aside,
}

/// When a "Then" step runs: once the one before finished, only if it succeeded, after your go.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum StepWhen {
    Done,
    Ok,
    Ask,
}

/// A "Then" step of the launcher, and when it runs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LaunchStep {
    pub kind: StepKind,
    pub when: StepWhen,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub text: Option<String>,
}

/// An agent's chain: its "Then" steps, run by Prism once the agent has done its work
/// (`prism_agents::chains`). In `chains.json`, as the window kept it before.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Chain {
    pub steps: Vec<LaunchStep>,
    pub runs: Vec<StepRun>,
    /// The step at hand; -1: the agent's own work, before the first step.
    pub current: i32,
    /// Waiting for the agent: its first work, a prompt step, or fixing a failed task.
    pub waiting: Option<ChainWait>,
    /// The agent was seen working since it was last waited for: its next pause ends that wait.
    pub saw_running: bool,
    /// The task at hand already went back to the agent once.
    pub retried: bool,
    pub stopped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub mr: Option<ChainMr>,
}

impl Chain {
    pub fn new(steps: Vec<LaunchStep>) -> Self {
        let runs = steps.iter().map(|_| StepRun::of(RunState::Pending)).collect();
        Chain { steps, runs, current: -1, waiting: Some(ChainWait::Work), saw_running: false, retried: false, stopped: false, mr: None }
    }

    /// It still has something to do.
    pub fn active(&self) -> bool {
        !self.stopped && self.current < self.steps.len() as i32
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum ChainWait {
    Work,
    Prompt,
    Fixing,
}

/// The MR a chain's `mr` step opened.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ChainMr {
    pub iid: u64,
    pub url: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Pending,
    Held,
    Running,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StepRun {
    pub state: RunState,
    /// What it gave: an MR's link, a task's exit, why it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub url: Option<String>,
}

impl StepRun {
    pub fn of(state: RunState) -> Self {
        StepRun { state, detail: None, url: None }
    }

    pub fn with(state: RunState, detail: impl Into<String>) -> Self {
        StepRun { state, detail: Some(detail.into()), url: None }
    }
}

/// What an agent's CLI asks on its screen before its session starts (`Agent::gate`).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgentGate {
    pub kind: GateKind,
    /// The folder it names (a folder's trust), as its screen writes it.
    pub folder: Option<String>,
    /// Prism knows this screen's keys: it can answer it. Otherwise only its terminal does.
    pub answerable: bool,
    /// The folder of the repository's worktrees (`<repo>.worktrees`), when trusting it would cover
    /// this one and the next (Claude Code): « All worktrees of … ».
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum GateKind {
    /// Trust the folder (its settings, hooks and MCP servers run).
    Folder,
    /// Approve Prism's hooks (Codex): without them, no status.
    Hooks,
    /// Something else it asks before it starts (an update…).
    Other,
}

/// Agent launched by Prism in its tmux server (Agents view).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Agent {
    /// Prism's identifier, which also names the tmux session (and Claude Code's session id).
    pub id: String,
    /// The CLI it runs: its labels, keys and commands follow it.
    pub provider: crate::agents::provider::Provider,
    /// The CLI's account it runs under (`[[agents.accounts]]`); absent: the CLI's usual folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub account: Option<String>,
    pub title: String,
    /// Its kind (`[[agents.kinds]]`), which groups them.
    pub category: Option<String>,
    /// Free tags set in the Agents view.
    pub tags: Vec<String>,
    pub cwd: String,
    /// The agent's worktree, if it has one; and why it was kept when Claude exited.
    pub worktree: Option<String>,
    pub worktree_note: Option<String>,
    /// Worktrees created by a command of the agent (a skill), which stay linked to it; only
    /// those whose folder still exists.
    pub adopted: Vec<String>,
    /// The worktrees of the other repositories it was launched in too (`also`), while they exist.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub also: Vec<String>,
    pub branch: Option<String>,
    /// Its ticket: the one linked by hand, otherwise the one found in its title or branch.
    pub ticket: Option<String>,
    /// The ticket is the one linked by hand (Agents view), not a guess.
    pub ticket_linked: bool,
    /// The MR linked by hand (Agents view); otherwise the window finds its branch's.
    pub mr: Option<MrLink>,
    pub prompt: Option<String>,
    /// What it was launched with; absent for an agent launched before Prism kept it.
    pub launch: Option<LaunchSpec>,
    pub created_at: DateTime<Utc>,
    pub state: AgentState,
    /// Claude's exit code, once it has exited.
    pub exit_code: Option<i32>,
    /// From Claude Code's hooks, like the desktop sessions.
    pub status: SessionStatus,
    pub needs_action: Option<String>,
    /// Marked read (`read_marks`): its turn over, it waits for me no more (status `Idle`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub read: bool,
    /// At its prompt, the next prompt Claude Code suggests (dimmed in its input), read off its screen.
    pub suggestion: Option<String>,
    /// What its CLI asks before it starts, read off its screen (it sends no hook until answered):
    /// to trust its folder, to approve Prism's hooks. Its status is then `WaitingTool`.
    pub gate: Option<AgentGate>,
    pub last_activity: Option<DateTime<Utc>>,
    /// When it last moved — a prompt, a tool, a permission — not a mere notification: what a mark
    /// read (`read_marks`) is compared with. Kept for Prism, never sent.
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub moved_at: Option<DateTime<Utc>>,
    /// How much of its context window the conversation fills (Claude Code); absent before its
    /// first reply, and after a compaction until the next one.
    pub context: Option<crate::agents::context::ContextUse>,
    /// The same command failed this many times in a row, nothing written in between (3 or more):
    /// the agent is going in circles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub looping: Option<Looping>,
    /// It has proposed a plan (plan mode) in this conversation: its Plan tab is offered.
    #[serde(default)]
    pub planned: bool,
    /// What its conversation consumed so far (Claude Code, Codex): tokens, and their estimated cost.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::agents::usage::AgentUsage>,
    /// The model answering, as its CLI names it: its last reply's (Claude Code, Codex), or the one
    /// its hooks or plugin log (Command Code, OpenCode); absent before it answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The CLIs it ran before its current one (Continue with…), oldest first; its `usage` is the
    /// current one's only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<AgentLeg>>"))]
    pub legs: Vec<AgentLeg>,
}

/// A stretch of an agent on one CLI, over since another took it on (Continue with…).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgentLeg {
    pub provider: crate::agents::provider::Provider,
    /// The CLI's account it ran under (`[[agents.accounts]]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub account: Option<String>,
    /// Its CLI's session, to go back to it (Claude Code: the agent's id, or the one of a `/clear`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub session: Option<String>,
    /// Its conversation's file, for the Chat's earlier turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub transcript: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub model: Option<String>,
    pub started: DateTime<Utc>,
    pub ended: DateTime<Utc>,
    /// What it consumed on this stretch only (a session resumed later counts what it held before
    /// in its earlier stretch, not twice); none for a CLI whose conversation Prism does not read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub usage: Option<crate::agents::usage::AgentUsage>,
    /// Why it was handed over: `limit` (its plan used up) or `choice`.
    pub reason: String,
    /// How its successor was told: `prism` (Prism's relay), `agent` (its own summary), `task`.
    pub handoff: String,
    /// The words its successor started from.
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Looping {
    pub command: String,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Running,
    /// Claude has exited; its last screen stays displayed.
    Exited,
    /// No tmux session anymore (the computer restarted): the agent can resume its conversation.
    Gone,
    /// Declared, not started: its launch is ready, nothing runs and no worktree exists yet.
    Prepared,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Claude is working (transcript being written).
    Running,
    /// Its turn is over but background work (subagents, tasks, wake-ups) goes on and will
    /// relaunch it: still working, and a prompt can be sent meanwhile; not waiting for one.
    Background,
    /// Tool call with no follow-up for a while: permission to grant or long-running command.
    WaitingTool,
    /// Turn finished, Claude is waiting for an answer or a decision.
    NeedsYou,
    /// Turn finished, a deliverable is ready to review.
    ReviewReady,
    Completed,
    Idle,
}

/// Where a Claude session comes from: the desktop app, or an agent launched by Prism.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum SessionOrigin {
    #[default]
    Desktop,
    Agent,
}

/// Claude Code desktop session: metadata and status only, never the content.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ClaudeSession {
    pub id: String,
    pub title: String,
    pub folder: String,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub ticket: Option<String>,
    pub status: SessionStatus,
    pub needs_action: Option<String>,
    /// Marked read (`read_marks`): its turn over, it waits for me no more (status `Idle`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub read: bool,
    pub last_activity: Option<DateTime<Utc>>,
    /// Desktop sidebar group, its rank, and the session's rank within the group.
    pub group: Option<String>,
    pub group_order: Option<u32>,
    pub order_in_group: Option<u32>,
    pub origin: SessionOrigin,
    /// The repository a Desktop session works in (`[[repos]]`): its whole folder is never sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub repo: Option<String>,
    /// A Desktop session's transcript and whole folder, kept for Prism (« in passing » reads its
    /// `spawn_task` calls), never sent.
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub transcript: Option<String>,
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub path: Option<String>,
}

/// Archived desktop session that had a worktree: enough to tell what it was used for, and
/// that nobody works in it anymore.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ArchivedSession {
    pub id: String,
    pub title: String,
    pub worktree: String,
    pub branch: Option<String>,
    pub last_activity: Option<DateTime<Utc>>,
}

/// The last agent closed in a repository (the Agents view by project): where it was left and its
/// last words, so that a project picked up days later says where it stood. Reopen brings it back
/// with its conversation, in its folder if that still exists.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LeftSession {
    /// The repository (`[[repos]] name`) it worked in.
    pub repo: String,
    pub id: String,
    pub provider: crate::agents::provider::Provider,
    pub title: String,
    pub branch: Option<String>,
    /// Its worktree, while it still exists.
    pub worktree: Option<String>,
    pub cwd: String,
    pub closed_at: DateTime<Utc>,
    /// The end of its last reply, cut to a few lines.
    pub said: Option<String>,
    /// Its folder still exists: Reopen can resume its conversation there.
    pub resumable: bool,
}

/// What an agent cost, kept past its end (`agent-costs.json`): a ticket's actual time and cost
/// count its agents, closed ones too. Its figures as last seen; one per agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgentCost {
    pub id: String,
    /// Its ticket (linked by hand, or found in its title or branch); absent: unlinked.
    #[serde(default)]
    pub ticket: Option<String>,
    /// Its branch: an MR of it names the ticket when the agent did not.
    #[serde(default)]
    pub branch: Option<String>,
    pub title: String,
    /// Its kind (`[[agents.kinds]]`): Implement, Review-fix, Diagnose…
    #[serde(default)]
    pub kind: Option<String>,
    pub provider: crate::agents::provider::Provider,
    #[serde(default)]
    pub model: Option<String>,
    pub started: DateTime<Utc>,
    /// Its last activity seen.
    pub last: DateTime<Utc>,
    /// Time spent working (its turns, not waiting for you nor for a permission), in milliseconds.
    pub active_ms: u64,
    /// Tokens of every kind (input, output, cache reads and writes).
    pub tokens: u64,
    /// At the models' API prices, in dollars; absent when a model's price is unknown.
    #[serde(default)]
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UsageWindow {
    /// Consumed percentage of the window (0–100).
    pub utilization: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

/// Claude plan usage: 5 h rolling window, week, and per-model windows.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ClaudeUsage {
    pub five_hour: Option<UsageWindow>,
    pub seven_day: Option<UsageWindow>,
    pub others: Vec<(String, UsageWindow)>,
    pub fetched_at: DateTime<Utc>,
}

/// The plan usage of a CLI (or an account of it) other than `[agents] provider`'s default, read
/// while an agent of it runs.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProviderUsage {
    pub provider: crate::agents::provider::Provider,
    /// Its account (`[[agents.accounts]]`), each with its own plan; absent: the CLI's usual folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub account: Option<String>,
    /// Absent until its first reading.
    pub usage: Option<ClaudeUsage>,
    /// Its last reading failed: the last one kept stays shown.
    pub error: Option<String>,
}
