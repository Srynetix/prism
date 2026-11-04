use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use chrono::{DateTime, Utc};

use super::hook::Line;
use crate::provider::valid_session;
use crate::state::SessionStatus;

/// The hook log's line Prism writes when another CLI takes an agent on (`Agents::switch`).
pub const SWITCH: &str = "ProviderSwitch";

/// What an agent asking its question waits for.
pub const QUESTION: &str = "Answer its question";

/// What it waits for you on, its transcript not saying it yet (Command Code writes it once
/// answered): its question, or its plan.
#[derive(Debug, Clone, PartialEq)]
pub enum Waiting {
    Question(crate::question::Ask),
    Plan { call: String, text: String },
}

/// Status of an agent, deduced from its hook log as it is read.
#[derive(Debug, Clone, PartialEq)]
pub struct Fold {
    pub status: SessionStatus,
    /// What Claude is waiting for (a tool permission, an answer to its question); never the text
    /// of the conversation.
    pub needs_action: Option<String>,
    pub last_activity: Option<DateTime<Utc>>,
    /// The last line of the agent's own turn (prompt, tool call, permission, start): an interruption
    /// written later in its transcript ends that turn.
    turn_at: Option<DateTime<Utc>>,
    /// Worktree created by the hook, and why it was kept when Claude wanted to remove it.
    pub worktree: Option<String>,
    pub worktree_note: Option<String>,
    /// Worktrees created by a command of the agent (a skill): linked to it, never removed by it.
    pub adopted: Vec<String>,
    /// Turn finished, but background tasks will relaunch it (`Background`, or a subagent's
    /// permission): it is not up to me to answer.
    pub background: bool,
    /// The CLI's own session id, when it is not Prism's (Command Code, OpenCode, Claude Code
    /// after a `/clear`): to resume it.
    pub session: Option<String>,
    /// The context's size at the last reply measured, and the model (Command Code, OpenCode, whose
    /// window Prism looks up).
    pub context: Option<super::context::ContextUse>,
    pub model: Option<String>,
    /// The same command failing again and again, nothing written in between: the command and how
    /// many times in a row.
    pub failing: Option<(String, u32)>,
    /// The conversation's transcript (Claude Code), from its start.
    pub transcript: Option<String>,
    /// It has proposed a plan (`ExitPlanMode`) in this conversation: its Plan tab has something to show.
    pub planned: bool,
    /// The question it asks or the plan it proposes, while it waits for your answer (Command Code,
    /// whose transcript holds them once answered only): for its Chat.
    pub waiting: Option<Waiting>,
    /// Its permission answered (`PermissionReplied`), nothing since: the permission's notification,
    /// a few seconds late, asks nothing any more.
    replied: bool,
    /// Time spent working (`Running`) in the turns over, in milliseconds: its active time, which
    /// waiting for you or for a permission is not; and since when it works, in the turn under way.
    active_ms: u64,
    working_since: Option<i64>,
    /// The time of the line being applied (milliseconds), for the active time.
    now: i64,
    /// Bytes of the log already read, and incomplete line ending pending.
    offset: u64,
    partial: Vec<u8>,
}

/// The longest stretch counted as working: a turn whose end the log missed (Prism closed, a hook
/// lost) does not count the hours after it.
const LONGEST_STRETCH_MS: i64 = 3 * 3_600_000;

impl Default for Fold {
    fn default() -> Self {
        Self { status: SessionStatus::Idle, needs_action: None, last_activity: None, turn_at: None, worktree: None, worktree_note: None, adopted: Vec::new(), background: false, session: None, context: None, model: None, failing: None, transcript: None, planned: false, waiting: None, replied: false, active_ms: 0, working_since: None, now: 0, offset: 0, partial: Vec::new() }
    }
}

impl Fold {
    /// Reads what has been added to the log since last time.
    pub fn read(&mut self, path: &Path) {
        let Ok(mut f) = std::fs::File::open(path) else { return };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        // Log recreated (agent relaunched after a cleanup): re-read from the start.
        if len < self.offset {
            *self = Self::default();
        }
        if len == self.offset || f.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return;
        }
        self.offset += buf.len() as u64;
        // Only the unfinished last line is kept, not a buffer the size of the first read.
        let mut data = std::mem::take(&mut self.partial);
        data.append(&mut buf);
        let Some(last) = data.iter().rposition(|b| *b == b'\n') else {
            self.partial = data;
            return;
        };
        self.partial = data[last + 1..].to_vec();
        for line in data[..last].split(|b| *b == b'\n') {
            if let Ok(line) = serde_json::from_slice::<Line>(line) {
                self.apply(&line);
            }
        }
    }

    pub fn apply(&mut self, l: &Line) {
        // The log is a file any program of this user can write: what goes on a command line or
        // names a folder is checked here (`valid_session`, `own_worktree`), not only by the hook.
        // Command Code reloading its mod (its file written again): the same session goes on, in
        // the same state — a permission it waits for still waits.
        if matches!(l.event.as_str(), "SessionStart" | "SessionEnd") && l.reason.as_deref() == Some("reload") {
            if let Some(session) = l.session.clone().filter(|s| valid_session(s)) {
                self.session = Some(session);
            }
            return;
        }
        // Another CLI takes the agent on (Continue with…): the former one's conversation is over
        // here, its session and transcript no longer the agent's.
        if l.event == SWITCH {
            (self.session, self.transcript, self.context, self.model, self.failing) = (None, None, None, None, None);
            (self.planned, self.background, self.replied) = (false, false, false);
            self.last_activity = DateTime::from_timestamp_millis(l.t).or(self.last_activity);
            self.now = self.now.max(l.t);
            self.set(SessionStatus::Idle, None);
            return;
        }
        self.last_activity = DateTime::from_timestamp_millis(l.t).or(self.last_activity);
        self.now = self.now.max(l.t);
        if !l.sub && matches!(l.event.as_str(), "SessionStart" | "UserPromptSubmit" | "PreToolUse" | "SessionEnd") {
            self.background = false;
        }
        // Not `PostToolUseFailure`: Claude Code sends it for the tool cut short by an interruption.
        if !l.sub && matches!(l.event.as_str(), "SessionStart" | "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PermissionRequest" | "PermissionReplied") {
            self.turn_at = DateTime::from_timestamp_millis(l.t).or(self.turn_at);
        }
        self.replied = match l.event.as_str() {
            "PermissionReplied" => true,
            "Notification" => self.replied,
            _ => false,
        };
        if l.context.is_some() && !l.sub {
            self.context = l.context;
            self.model = l.model.clone().or(self.model.take());
        }
        // A conversation's file, as its CLI names it: nothing a forged line could point elsewhere with.
        if let Some(t) = l.transcript.as_ref().filter(|t| {
            let p = std::path::Path::new(t);
            p.is_absolute() && p.extension().is_some_and(|e| e == "jsonl") && !p.components().any(|c| matches!(c, std::path::Component::ParentDir))
        }) {
            self.transcript = Some(t.clone());
        }
        if !l.sub && l.tool.as_deref() == Some("ExitPlanMode") {
            self.planned = true;
        }
        match (l.event.as_str(), l.tool.as_deref()) {
            // A failed command: the same as before makes the streak longer.
            ("PostToolUseFailure", _) => {
                if let Some(cmd) = l.cmd.clone().or_else(|| l.what.clone()).filter(|_| l.tool.as_deref() == Some("Bash")) {
                    let count = match &self.failing {
                        Some((c, n)) if *c == cmd => n + 1,
                        _ => 1,
                    };
                    self.failing = Some((cmd, count));
                }
            }
            // Something written, the command passing, or your prompt: the streak is over.
            ("PostToolUse", Some("Write" | "Edit" | "MultiEdit" | "NotebookEdit" | "apply_patch")) | ("UserPromptSubmit", _) => self.failing = None,
            ("PostToolUse", Some("Bash")) if self.failing.as_ref().is_some_and(|(c, _)| Some(c) == l.cmd.as_ref()) => self.failing = None,
            _ => {}
        }
        if l.sub && self.subagent(l) {
            return;
        }
        match l.event.as_str() {
            "SessionStart" => {
                // Compacted, cleared, or another conversation (`/clear`, `/new`): the former size no longer holds.
                if matches!(l.reason.as_deref(), Some("compact" | "clear")) || l.session.as_ref().is_some_and(|s| self.session.as_ref() != Some(s)) {
                    self.context = None;
                }
                if let Some(session) = l.session.clone().filter(|s| valid_session(s)) {
                    self.session = Some(session);
                }
                self.set(SessionStatus::Idle, None);
            }
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => self.set(SessionStatus::Running, None),
            // Its question (`AskUserQuestion`) goes through the permissions, but answers it: your turn,
            // not a permission to grant (Allow would pick its first choice).
            "PermissionRequest" if l.tool.as_deref() == Some("AskUserQuestion") => {
                self.set(SessionStatus::NeedsYou, Some(QUESTION.into()));
                self.waiting = l.ask.as_ref().and_then(|a| crate::question::parse(l.id.as_deref().unwrap_or_default(), a)).map(Waiting::Question);
            }
            "PermissionRequest" => {
                let tool = l.tool.as_deref().unwrap_or("a tool");
                let ask = match &l.what {
                    Some(what) => format!("Allow {tool}: {what}"),
                    None => format!("Allow {tool}"),
                };
                self.set(SessionStatus::WaitingTool, Some(ask));
                if l.tool.as_deref() == Some("ExitPlanMode") && let Some(text) = &l.plan {
                    self.waiting = Some(Waiting::Plan { call: l.id.clone().unwrap_or_default(), text: text.clone() });
                }
            }
            // The permission answered, the tool runs (or the turn ends, refused): OpenCode's plugin
            // says so, Prism too when it answers (Allow, a key of the menu) — Claude Code's hooks
            // say nothing before the tool is over (`PostToolUse`), minutes later for a long command.
            "PermissionReplied" if self.status == SessionStatus::WaitingTool => self.set(if self.background { SessionStatus::Background } else { SessionStatus::Running }, None),
            // The permission notification arrives a few seconds after the request: it is
            // only used if the request was not seen.
            "Notification" => match l.notification.as_deref() {
                Some("permission_prompt") if self.status != SessionStatus::WaitingTool && !self.replied && self.needs_action.as_deref() != Some(QUESTION) => self.set(SessionStatus::WaitingTool, Some("Permission requested".into())),
                Some("idle_prompt") if self.status == SessionStatus::Running && !self.background => self.set(SessionStatus::NeedsYou, None),
                _ => {}
            },
            // Background tasks in flight (a dev server, a subagent, a `/loop` wake-up): its turn is over,
            // at its prompt, where a prompt can be sent — not waiting for my answer: their return
            // relaunches it.
            "Stop" if l.background.unwrap_or(0) > 0 => {
                self.background = true;
                self.set(SessionStatus::Background, None);
            }
            // Codex: interrupted (Esc), it waits for you as after a reply.
            "Stop" | "Interrupt" => self.set(SessionStatus::NeedsYou, None),
            "SessionEnd" => self.set(SessionStatus::Completed, None),
            // Command Code or OpenCode compacted the conversation: its size comes with the next request.
            "Compacted" => self.context = None,
            "WorktreeCreate" => {
                self.worktree = l.what.clone();
                self.worktree_note = None;
            }
            "WorktreeAdopted" => {
                if let Some(path) = l.what.clone().filter(|p| !self.adopted.contains(p) && self.worktree.as_ref() != Some(p)) {
                    self.adopted.push(path);
                }
            }
            "WorktreeRemove" => match l.reason.as_deref() {
                Some("removed") => (self.worktree, self.worktree_note) = (None, None),
                reason => self.worktree_note = reason.map(|r| r.trim_start_matches("kept: ").to_string()),
            },
            _ => {}
        }
    }

    /// Claude Code interrupted (Esc, from its terminal or Prism) at `at`, as its transcript says: its
    /// hooks say nothing, not even `Stop`, so a turn still seen working (or waiting for a permission,
    /// whose menu Esc closes) is over, at its prompt, unless it went on after.
    pub fn interrupted(&mut self, at: DateTime<Utc>) {
        if matches!(self.status, SessionStatus::Running | SessionStatus::WaitingTool) && self.turn_at.is_none_or(|t| at > t) {
            self.now = self.now.max(at.timestamp_millis());
            self.set(SessionStatus::NeedsYou, None);
            self.last_activity = self.last_activity.max(Some(at));
        }
    }

    /// A subagent's call (Claude Code): working whether or not its parent is, it says nothing of
    /// whose turn it is — only its permission waits for you, and the calls after it end that wait.
    /// Returns whether the line is dealt with.
    fn subagent(&mut self, l: &Line) -> bool {
        if !matches!(l.event.as_str(), "PreToolUse" | "PostToolUse" | "PostToolUseFailure") {
            return false;
        }
        if self.status == SessionStatus::WaitingTool {
            self.set(if self.background { SessionStatus::Background } else { SessionStatus::Running }, None);
        }
        true
    }

    fn set(&mut self, status: SessionStatus, needs_action: Option<String>) {
        let (was, is) = (self.status == SessionStatus::Running, status == SessionStatus::Running);
        if was && !is && let Some(since) = self.working_since.take() {
            self.active_ms += (self.now - since).clamp(0, LONGEST_STRETCH_MS) as u64;
        }
        if is && !was {
            self.working_since = Some(self.now);
        }
        // Whatever comes next, it no longer waits on it; set again by the line that says so.
        self.waiting = None;
        self.status = status;
        self.needs_action = needs_action;
    }

    /// Its active time so far, in milliseconds: the turns over, and the one under way up to its
    /// last line.
    pub fn active_ms(&self) -> u64 {
        self.active_ms + self.working_since.map_or(0, |since| (self.now - since).clamp(0, LONGEST_STRETCH_MS) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(event: &str) -> Line {
        Line { t: 1_791_400_000_000, event: event.into(), ..Default::default() }
    }

    /// A turn that ends with background tasks in flight is over (at its prompt) but not waiting
    /// for my answer, even when Claude then reports it is idle; their return relaunches the turn.
    #[test]
    fn another_cli_forgets_the_former_session() {
        let mut f = Fold::default();
        f.apply(&Line { session: Some("claude-1".into()), transcript: Some("/t/claude-1.jsonl".into()), ..line("SessionStart") });
        f.apply(&Line { tool: Some("ExitPlanMode".into()), ..line("PreToolUse") });
        assert_eq!((f.session.as_deref(), f.planned, f.status), (Some("claude-1"), true, SessionStatus::Running));
        f.apply(&Line { what: Some("codex".into()), ..line(SWITCH) });
        assert_eq!((f.session.as_deref(), f.transcript.as_deref(), f.planned, f.status), (None, None, false, SessionStatus::Idle));
        // Its successor reports its own, read again from the log's start as well.
        f.apply(&Line { session: Some("019a-codex".into()), ..line("SessionStart") });
        assert_eq!(f.session.as_deref(), Some("019a-codex"));
    }

    #[test]
    fn background_work_is_not_my_turn() {
        let mut f = Fold::default();
        f.apply(&line("UserPromptSubmit"));
        f.apply(&Line { background: Some(2), ..line("Stop") });
        assert_eq!(f.status, SessionStatus::Background);
        f.apply(&Line { notification: Some("idle_prompt".into()), ..line("Notification") });
        assert_eq!(f.status, SessionStatus::Background);
        f.apply(&line("PreToolUse"));
        assert_eq!(f.status, SessionStatus::Running, "their return relaunches it");
        f.apply(&line("PreToolUse"));
        f.apply(&line("Stop"));
        assert_eq!(f.status, SessionStatus::NeedsYou);
    }

    /// Background subagents keep calling tools once their parent's turn is over: it stays in the
    /// background, and its idle notification does not make it wait for you (sequence seen with
    /// Claude Code 2.1.304). Only a subagent's permission waits for you, until its next call.
    #[test]
    fn a_subagent_working_is_not_my_turn() {
        let mut f = Fold::default();
        let sub = |event: &str, tool: &str| Line { tool: Some(tool.into()), sub: true, ..line(event) };
        f.apply(&line("UserPromptSubmit"));
        f.apply(&Line { tool: Some("Agent".into()), ..line("PreToolUse") });
        f.apply(&Line { background: Some(1), ..line("Stop") });
        f.apply(&sub("PreToolUse", "Bash"));
        f.apply(&sub("PostToolUse", "Bash"));
        assert_eq!(f.status, SessionStatus::Background);
        f.apply(&Line { notification: Some("idle_prompt".into()), ..line("Notification") });
        assert_eq!(f.status, SessionStatus::Background);
        f.apply(&Line { what: Some("Run the tests".into()), ..sub("PermissionRequest", "Bash") });
        assert_eq!((f.status, f.needs_action.as_deref()), (SessionStatus::WaitingTool, Some("Allow Bash: Run the tests")));
        f.apply(&sub("PostToolUse", "Bash"));
        assert_eq!((f.status, f.needs_action.as_deref()), (SessionStatus::Background, None));
        // Its failing commands still make a streak.
        f.apply(&Line { cmd: Some("npm test".into()), ..sub("PostToolUseFailure", "Bash") });
        assert_eq!(f.failing, Some(("npm test".into(), 1)));
        // Its return relaunches the parent, whose turn then ends for good.
        f.apply(&Line { tool: Some("Read".into()), ..line("PreToolUse") });
        assert_eq!(f.status, SessionStatus::Running);
        f.apply(&line("Stop"));
        assert_eq!(f.status, SessionStatus::NeedsYou);
        // A subagent in the foreground: its parent works until it returns.
        f.apply(&Line { tool: Some("Agent".into()), ..line("PreToolUse") });
        f.apply(&sub("PreToolUse", "Read"));
        assert_eq!(f.status, SessionStatus::Running);
    }

    /// Claude Code interrupted (Esc) sends no hook: the interruption read in its transcript ends the
    /// turn, unless the turn went on after it (a new prompt).
    #[test]
    fn an_interruption_ends_the_turn() {
        let at = |ms: i64| DateTime::from_timestamp_millis(ms).unwrap();
        let mut f = Fold::default();
        f.apply(&line("UserPromptSubmit"));
        f.apply(&Line { tool: Some("Bash".into()), ..line("PreToolUse") });
        // The tool cut short reports its failure after the interruption.
        f.apply(&Line { t: 1_791_400_002_000, tool: Some("Bash".into()), ..line("PostToolUseFailure") });
        f.interrupted(at(1_791_400_001_000));
        assert_eq!(f.status, SessionStatus::NeedsYou);
        f.apply(&Line { t: 1_791_400_003_000, ..line("UserPromptSubmit") });
        f.interrupted(at(1_791_400_001_000));
        assert_eq!(f.status, SessionStatus::Running, "an earlier interruption, the new prompt goes on");
        // Esc on a permission's menu.
        f.apply(&Line { t: 1_791_400_004_000, tool: Some("Bash".into()), ..line("PermissionRequest") });
        f.interrupted(at(1_791_400_005_000));
        assert_eq!((f.status, f.needs_action), (SessionStatus::NeedsYou, None));
    }

    /// A question is the agent waiting for you, not a permission; its notification changes nothing.
    #[test]
    fn remembers_a_plan_proposed() {
        let mut f = Fold::default();
        f.apply(&Line { tool: Some("ExitPlanMode".into()), sub: true, ..line("PreToolUse") });
        assert!(!f.planned, "a subagent's plan is not its own");
        f.apply(&Line { tool: Some("ExitPlanMode".into()), ..line("PreToolUse") });
        f.apply(&Line { tool: Some("Bash".into()), ..line("PreToolUse") });
        assert!(f.planned);
    }

    #[test]
    fn a_question_is_my_turn() {
        let mut f = Fold::default();
        f.apply(&Line { tool: Some("AskUserQuestion".into()), ..line("PreToolUse") });
        f.apply(&Line { tool: Some("AskUserQuestion".into()), ..line("PermissionRequest") });
        assert_eq!((f.status, f.needs_action.as_deref()), (SessionStatus::NeedsYou, Some(QUESTION)));
        f.apply(&Line { notification: Some("permission_prompt".into()), ..line("Notification") });
        assert_eq!(f.status, SessionStatus::NeedsYou);
        f.apply(&Line { tool: Some("AskUserQuestion".into()), ..line("PostToolUse") });
        assert_eq!(f.waiting, None, "Claude Code's question is in its transcript");
        assert_eq!((f.status, f.needs_action), (SessionStatus::Running, None));
    }

    /// The context's size follows the replies, and a compaction forgets it until the next one.
    #[test]
    fn follows_the_context_window() {
        use crate::context::ContextUse;
        let mut f = Fold::default();
        let full = ContextUse { used: 150_000, size: Some(200_000) };
        f.apply(&Line { context: Some(full), ..line("PostToolUse") });
        f.apply(&line("Notification"));
        assert_eq!(f.context, Some(full), "a line without a size keeps the last one");
        f.apply(&Line { reason: Some("compact".into()), ..line("SessionStart") });
        assert_eq!(f.context, None);
        // Command Code: its mod's figures, without the window, then its compaction.
        f.apply(&Line { context: Some(ContextUse { used: 67_000, size: None }), model: Some("meta/muse".into()), ..line("Context") });
        assert_eq!((f.context.map(|c| c.used), f.model.as_deref()), (Some(67_000), Some("meta/muse")));
        f.apply(&line("Compacted"));
        assert_eq!(f.context, None);
        // Another conversation in the same agent (OpenCode's /new), whatever its reason.
        f.apply(&Line { context: Some(full), ..line("Stop") });
        f.apply(&Line { reason: Some("startup".into()), session: Some("ses_2".into()), ..line("SessionStart") });
        assert_eq!(f.context, None);
    }

    /// A worktree created by a command of the agent is added to those linked to it, once.
    #[test]
    fn adopts_worktrees_created_by_a_command() {
        let mut f = Fold::default();
        let adopt = |p: &str| Line { what: Some(p.into()), ..line("WorktreeAdopted") };
        f.apply(&adopt("/dev/app.worktrees/abc-1"));
        f.apply(&adopt("/dev/app.worktrees/abc-1"));
        assert_eq!(f.adopted, ["/dev/app.worktrees/abc-1"]);
        assert_eq!(f.worktree, None, "its own worktree stays untouched");
    }

    #[test]
    fn follows_a_turn_with_a_permission() {
        let mut f = Fold::default();
        f.apply(&line("SessionStart"));
        assert_eq!(f.status, SessionStatus::Idle);
        f.apply(&line("UserPromptSubmit"));
        f.apply(&line("PreToolUse"));
        assert_eq!(f.status, SessionStatus::Running);
        f.apply(&Line { tool: Some("Bash".into()), what: Some("Run the tests".into()), ..line("PermissionRequest") });
        assert_eq!((f.status, f.needs_action.as_deref()), (SessionStatus::WaitingTool, Some("Allow Bash: Run the tests")));
        // The late notification does not replace the request summary.
        f.apply(&Line { notification: Some("permission_prompt".into()), ..line("Notification") });
        assert_eq!(f.needs_action.as_deref(), Some("Allow Bash: Run the tests"));
        f.apply(&line("PostToolUse"));
        assert_eq!((f.status, f.needs_action.as_deref()), (SessionStatus::Running, None));
        f.apply(&line("Stop"));
        assert_eq!(f.status, SessionStatus::NeedsYou);
        f.apply(&Line { notification: Some("idle_prompt".into()), ..line("Notification") });
        assert_eq!(f.status, SessionStatus::NeedsYou);
        f.apply(&line("SessionEnd"));
        assert_eq!(f.status, SessionStatus::Completed);
        assert!(f.last_activity.is_some());
    }

    /// The lines Command Code's mod writes for a turn where a write waits for permission
    /// (sequence seen with `cmd` 1.79): it keeps the CLI's session id, to resume it.
    #[test]
    fn keeps_command_codes_question_while_it_is_asked() {
        let mut f = Fold::default();
        let ask = serde_json::json!({ "questions": [{ "header": "Color", "question": "Which color?", "options": [{ "label": "Red" }, { "label": "Blue" }] }] });
        f.apply(&Line { tool: Some("ask_user_question".into()), ..line("PreToolUse") });
        f.apply(&Line { tool: Some("AskUserQuestion".into()), id: Some("call_1".into()), ask: Some(ask), ..line("PermissionRequest") });
        assert_eq!((f.status, f.needs_action.as_deref()), (SessionStatus::NeedsYou, Some(QUESTION)));
        let Some(Waiting::Question(a)) = &f.waiting else { panic!() };
        assert_eq!((a.call.as_str(), a.questions[0].options.len()), ("call_1", 2));
        f.apply(&Line { tool: Some("ask_user_question".into()), ..line("PostToolUse") });
        assert_eq!((f.status, f.waiting.is_none()), (SessionStatus::Running, true), "answered");
        f.apply(&Line { tool: Some("ExitPlanMode".into()), id: Some("call_2".into()), plan: Some("# Plan".into()), ..line("PermissionRequest") });
        assert_eq!((f.waiting.clone(), f.planned), (Some(Waiting::Plan { call: "call_2".into(), text: "# Plan".into() }), true));
        f.apply(&Line { tool: Some("exit_plan_mode".into()), ..line("PostToolUse") });
        assert_eq!(f.waiting, None);
    }

    #[test]
    fn follows_command_code_through_its_mod() {
        let mut f = Fold::default();
        f.apply(&Line { reason: Some("startup".into()), session: Some("d869d804-d93e".into()), ..line("SessionStart") });
        f.apply(&line("UserPromptSubmit"));
        f.apply(&Line { tool: Some("write_file".into()), what: Some("hello.txt".into()), ..line("PermissionRequest") });
        assert_eq!((f.status, f.needs_action.as_deref()), (SessionStatus::WaitingTool, Some("Allow write_file: hello.txt")));
        f.apply(&Line { reason: Some("reload".into()), ..line("SessionEnd") });
        f.apply(&Line { reason: Some("reload".into()), ..line("SessionStart") });
        assert_eq!(f.status, SessionStatus::WaitingTool, "its mod reloaded, the permission still waits");
        f.apply(&Line { tool: Some("write_file".into()), ..line("PreToolUse") });
        f.apply(&Line { tool: Some("write_file".into()), ..line("PostToolUse") });
        assert_eq!(f.status, SessionStatus::Running);
        f.apply(&line("Stop"));
        assert_eq!(f.status, SessionStatus::NeedsYou);
        f.apply(&Line { reason: Some("resume".into()), ..line("SessionStart") });
        assert_eq!(f.session.as_deref(), Some("d869d804-d93e"), "a start without an id keeps the known one");
        f.apply(&Line { reason: Some("shutdown".into()), ..line("SessionEnd") });
        assert_eq!(f.status, SessionStatus::Completed);
    }

    /// The lines OpenCode's plugin writes for a turn where an edit waits for permission: the
    /// permission is asked once the tool has started, and its answer resumes the turn.
    /// Claude Code: its permission answered from Prism, it works at once, while the tool runs;
    /// the permission's late notification does not bring the request back.
    #[test]
    fn a_permission_answered_from_prism_ends_the_wait() {
        let mut f = Fold::default();
        f.apply(&line("UserPromptSubmit"));
        f.apply(&Line { tool: Some("Bash".into()), ..line("PreToolUse") });
        f.apply(&Line { tool: Some("Bash".into()), what: Some("Run the tests".into()), ..line("PermissionRequest") });
        f.apply(&line("PermissionReplied"));
        assert_eq!((f.status, f.needs_action.as_deref()), (SessionStatus::Running, None));
        f.apply(&Line { notification: Some("permission_prompt".into()), ..line("Notification") });
        assert_eq!(f.status, SessionStatus::Running, "a late notification");
        f.apply(&Line { tool: Some("Bash".into()), ..line("PostToolUse") });
        f.apply(&Line { tool: Some("Bash".into()), ..line("PreToolUse") });
        f.apply(&Line { notification: Some("permission_prompt".into()), ..line("Notification") });
        assert_eq!(f.status, SessionStatus::WaitingTool, "the next one, unseen otherwise, still counts");
    }

    #[test]
    fn follows_opencode_through_its_plugin() {
        let mut f = Fold::default();
        f.apply(&Line { reason: Some("startup".into()), session: Some("ses_4f2a".into()), ..line("SessionStart") });
        f.apply(&line("UserPromptSubmit"));
        f.apply(&Line { tool: Some("edit".into()), ..line("PreToolUse") });
        f.apply(&Line { tool: Some("edit".into()), what: Some("main.rs".into()), ..line("PermissionRequest") });
        assert_eq!((f.status, f.needs_action.as_deref()), (SessionStatus::WaitingTool, Some("Allow edit: main.rs")));
        f.apply(&line("PermissionReplied"));
        assert_eq!((f.status, f.needs_action.as_deref()), (SessionStatus::Running, None));
        f.apply(&Line { tool: Some("edit".into()), ..line("PostToolUse") });
        f.apply(&line("Stop"));
        assert_eq!(f.status, SessionStatus::NeedsYou);
        f.apply(&line("PermissionReplied"));
        assert_eq!(f.status, SessionStatus::NeedsYou, "a late answer does not relaunch a finished turn");
        assert_eq!(f.session.as_deref(), Some("ses_4f2a"));
    }

    #[test]
    fn tracks_the_worktree() {
        let mut f = Fold::default();
        f.apply(&Line { what: Some("/w/abc-1".into()), ..line("WorktreeCreate") });
        assert_eq!(f.worktree.as_deref(), Some("/w/abc-1"));
        f.apply(&Line { reason: Some("kept: 2 uncommitted changes".into()), ..line("WorktreeRemove") });
        assert_eq!((f.worktree.as_deref(), f.worktree_note.as_deref()), (Some("/w/abc-1"), Some("2 uncommitted changes")));
        f.apply(&Line { reason: Some("removed".into()), ..line("WorktreeRemove") });
        assert_eq!((f.worktree.as_deref(), f.worktree_note.as_deref()), (None, None));
    }

    #[test]
    fn reads_the_journal_incrementally() {
        let dir = std::env::temp_dir().join(format!("prism-fold-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let mut f = Fold::default();
        std::fs::write(&path, "{\"t\":1,\"event\":\"UserPromptSubmit\"}\n{\"t\":2,\"event\":\"St").unwrap();
        f.read(&path);
        assert_eq!(f.status, SessionStatus::Running);
        // The cut line is completed on the next pass.
        std::fs::write(&path, "{\"t\":1,\"event\":\"UserPromptSubmit\"}\n{\"t\":2,\"event\":\"Stop\"}\n").unwrap();
        f.read(&path);
        assert_eq!(f.status, SessionStatus::NeedsYou);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The same command failing again and again makes a streak; a file written ends it.
    #[test]
    fn counts_the_same_command_failing() {
        let mut f = Fold::default();
        let fail = |cmd: &str| Line { event: "PostToolUseFailure".into(), tool: Some("Bash".into()), cmd: Some(cmd.into()), ..Default::default() };
        f.apply(&fail("npm test"));
        f.apply(&fail("npm test"));
        f.apply(&fail("npm test"));
        assert_eq!(f.failing, Some(("npm test".into(), 3)));
        f.apply(&fail("npm run lint"));
        assert_eq!(f.failing, Some(("npm run lint".into(), 1)));
        f.apply(&Line { event: "PostToolUse".into(), tool: Some("Edit".into()), ..Default::default() });
        assert_eq!(f.failing, None);
    }

    /// Only working counts as active: not the time at its prompt, nor waiting for a permission;
    /// the turn under way up to its last line.
    #[test]
    fn active_time_is_working_time_only() {
        let at = |t: i64, event: &str| Line { t, event: event.into(), ..Default::default() };
        let mut f = Fold::default();
        f.apply(&at(0, "SessionStart"));
        f.apply(&at(60_000, "UserPromptSubmit"));
        f.apply(&at(90_000, "PermissionRequest"));
        f.apply(&at(300_000, "PreToolUse"));
        f.apply(&at(330_000, "Stop"));
        assert_eq!(f.active_ms(), 30_000 + 30_000);
        f.apply(&at(900_000, "UserPromptSubmit"));
        f.apply(&at(960_000, "PostToolUse"));
        assert_eq!(f.active_ms(), 60_000 + 60_000, "the turn under way, up to its last line");
        f.interrupted(DateTime::from_timestamp_millis(1_000_000).unwrap());
        assert_eq!(f.active_ms(), 60_000 + 100_000);
    }
}
