//! An agent's conversation as a phone reads it (its Chat tab): its transcript — Claude Code's,
//! Command Code's, or Codex's rollout — read again as turns, each your prompt then what the agent answered and did:
//! its text (Markdown), its tool calls in a line each, a file's edit as a short diff, a command's
//! output as its last lines. Unlike the terminal's history, never cut at a width nor mixed with the
//! CLI's frame. The phone already watches the terminal that shows all of it: this is the same
//! conversation, readable. Bounded: its last turns, each part cut; the transcript read from its
//! end, further back as more turns are asked, and kept read: only what was written since is read
//! again.
//! Besides: its thinking (when the CLI keeps its words), how long each call took, what each turn
//! consumed, a subagent's own calls (from its transcript), its plan as last written, and its
//! background tasks (Claude Code's commands, subagents and monitors run in the background): from
//! their launch to the notification of their end.

use std::{
    collections::{HashMap, HashSet},
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
    time::Instant,
    path::Path,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::{Context, Result};

use crate::breakdown::{codex_what, Names};
use prism_core::agents::transcript::{ClaudeUsage, CodexInfo};

mod fold;
mod formats;
mod subagents;
#[cfg(test)]
mod tests;

use fold::*;
pub use formats::pasted_image;
pub(crate) use formats::reminders;
use formats::*;
use subagents::*;

/// The end of the transcript read first; 4 times as much further back each time more turns are
/// asked than it holds, up to `MAX_BYTES` (the whole of any transcript seen: 85 MB for days of work).
const TAIL_BYTES: u64 = 8 * 1024 * 1024;
const MAX_BYTES: u64 = 1024 * 1024 * 1024;
/// Transcripts kept read: the Chats open (side by side, the phone's), the least used left first.
const KEPT: usize = 8;
/// And their bytes read, at most (a fold weighs 1–2 times what it read: 100–170 MB for all of a
/// 94 MB transcript): beyond, those not asked for in `IDLE` leave, the least used first; a Chat
/// open is read every few seconds, so never read again whole because another one grew.
const KEPT_BYTES: u64 = 256 << 20;
const IDLE: std::time::Duration = std::time::Duration::from_secs(30);
/// An answer's text, cut at this many characters.
const TEXT_CHARS: usize = 12_000;
/// A reminder's text, cut.
const REMINDER_CHARS: usize = 4_000;
/// A command's output: its last lines, and characters.
const OUTPUT_LINES: usize = 16;
/// A subagent's calls shown, its last ones; and its last words, cut.
const SUB_STEPS: usize = 8;
const SUB_CHARS: usize = 400;
const OUTPUT_CHARS: usize = 2_000;
/// An edit's diff, its first lines.
const DIFF_LINES: usize = 40;
/// Background tasks over shown, besides those running: the last ones.
const BACKGROUND_ENDED: usize = 5;

#[derive(Debug, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentConversation"))]
pub struct Conversation {
    /// The last turns, oldest first.
    pub turns: Vec<Turn>,
    /// Turns read in all, those left out included.
    pub total: u32,
    /// The transcript was read from its end only: its first turn read may lack its start.
    pub partial: bool,
    /// Its plan as last written (Claude Code's `TodoWrite` or `TaskCreate`/`TaskUpdate`, Codex's
    /// `update_plan`); none when it keeps none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<PlanStep>>", optional))]
    pub plan: Vec<PlanStep>,
    /// Its background tasks (Claude Code): those running, then the last ones over, oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<Background>>", optional))]
    pub background: Vec<Background>,
    /// What its CLI shows it doing right now, read off its screen; absent between turns.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub live: Option<crate::live::Live>,
    /// The model of its last reply (Claude Code) or turn (Codex's `turn_context`), as the CLI names it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub model: Option<String>,
    /// The turns left out of `turns` because the reader holds them unchanged (it gave their
    /// `sig`), by number: with those sent, every turn read, in the order of their numbers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<u32>>", optional))]
    pub kept: Vec<u32>,
}

impl Conversation {
    /// The question it asks or the plan it proposes right now (Command Code's, from its mod: its
    /// transcript is written once they are answered), a turn of its own unless the transcript
    /// already has its call.
    pub(crate) fn waiting(&mut self, waiting: crate::status::Waiting) {
        use crate::status::Waiting;
        let tools = || self.turns.iter().flat_map(|t| &t.entries).filter_map(|e| match e {
            Entry::Tool(t) => Some(t),
            _ => None,
        });
        let tool = match waiting {
            Waiting::Question(ask) => {
                if tools().any(|t| t.ask.as_ref().is_some_and(|a| a.call == ask.call)) {
                    return;
                }
                Tool { tool: "AskUserQuestion".into(), ask: Some(ask), ..Tool::default() }
            }
            Waiting::Plan { call, text } => {
                if tools().any(|t| t.proposal.as_ref().is_some_and(|p| p.call == call)) {
                    return;
                }
                let version = tools().filter(|t| t.proposal.is_some()).count() as u32 + 1;
                let proposal = crate::plan::Proposal { call, text: cut(text.trim(), TEXT_CHARS), version, verdict: "waiting", feedback: None, file: None };
                Tool { tool: "ExitPlanMode".into(), proposal: Some(proposal), ..Tool::default() }
            }
        };
        let n = self.turns.last().map_or(1, |t| t.n + 1);
        let tool = Tool { kind: kind_of(&tool.tool), state: "running", ..tool };
        self.turns.push(Turn { n, entries: vec![Entry::Tool(tool)], ..Turn::default() });
        self.total += 1;
    }

    /// Signs each turn not signed yet (`read` keeps those of unchanged turns), then leaves out
    /// those whose signature the reader holds (`have`): a Chat read every second while its agent
    /// works gets the turn that moved, not all of them again (16 MB a second with all of a long
    /// session's turns, measured).
    pub fn only_new(&mut self, have: &[String]) {
        for t in self.turns.iter_mut().filter(|t| t.sig.is_empty()) {
            t.sig = sign(t);
        }
        if have.is_empty() {
            return;
        }
        let have: HashSet<&str> = have.iter().map(String::as_str).collect();
        let (kept, sent): (Vec<Turn>, Vec<Turn>) = std::mem::take(&mut self.turns).into_iter().partition(|t| have.contains(t.sig.as_str()));
        self.kept = kept.into_iter().map(|t| t.n).collect();
        self.turns = sent;
    }
}

/// A turn's fingerprint, its number included: read again from further back, a transcript's turns
/// are numbered anew, and none is taken for the one that bore its number before.
fn sign(t: &Turn) -> String {
    use std::hash::Hasher;
    struct Into<'a>(&'a mut std::collections::hash_map::DefaultHasher);
    impl std::io::Write for Into<'_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.write(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let _ = serde_json::to_writer(Into(&mut h), t);
    format!("{:016x}", h.finish())
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "PlanStep"))]
pub struct PlanStep {
    pub text: String,
    /// `pending`, `in_progress`, `completed`.
    #[cfg_attr(feature = "ts", ts(type = "'pending' | 'in_progress' | 'completed'"))]
    pub status: String,
    /// Its number (`TaskUpdate`'s `taskId`).
    #[serde(skip)]
    pub id: String,
}

/// A command, a subagent or a monitor Claude Code runs in the background (`run_in_background`,
/// `Monitor`), followed by its id until the notification of its end.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "BackgroundTask"))]
pub struct Background {
    /// `command`, `agent`, `monitor`.
    #[cfg_attr(feature = "ts", ts(type = "'command' | 'agent' | 'monitor'"))]
    pub kind: &'static str,
    /// Its description, failing that its command.
    pub what: Option<String>,
    /// `running`, `completed`, `failed`, `stopped`.
    #[cfg_attr(feature = "ts", ts(type = "'running' | 'completed' | 'failed' | 'stopped'"))]
    pub state: &'static str,
    /// When it started, and ended (ISO 8601).
    pub at: Option<String>,
    pub ended: Option<String>,
    /// What Claude Code said of its end, or a monitor's last event.
    pub summary: Option<String>,
    /// A monitor's events so far.
    pub events: u32,
    #[serde(skip)]
    pub id: String,
    /// The call that launched it.
    #[serde(skip)]
    pub call: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ConversationTurn"))]
pub struct Turn {
    /// Counted from the first turn read, from 1.
    pub n: u32,
    /// Your prompt; empty when the turn's start was not read (or the conversation was resumed).
    pub prompt: String,
    /// When it was sent, and when its last entry was written (ISO 8601).
    pub at: Option<String>,
    pub last: Option<String>,
    pub entries: Vec<Entry>,
    /// The tokens its replies used (input of all kinds and output), and their estimated cost in
    /// dollars (none for a model without a known price).
    pub tokens: u64,
    pub cost: Option<f64>,
    /// The model that answered it (its last reply's).
    pub model: Option<String>,
    /// The images pasted in its prompt, kept whole in the transcript: its Chat asks for each one
    /// (`pasted_image`) rather than getting them with every read.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub pasted: Option<Pasted>,
    /// Its fingerprint (see `Conversation::only_new`), empty while it is being signed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub sig: String,
}

/// Where a prompt's pasted images are: the transcript line it is on (its first byte), and how many.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "PastedImages"))]
pub struct Pasted {
    pub line: u64,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
// A few hundred entries at most, read and sent: a tool kept inline costs nothing that matters.
#[allow(clippy::large_enum_variant)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ConversationEntry"))]
pub enum Entry {
    /// The agent's words, Markdown.
    Text { text: String },
    Tool(Tool),
    /// What happened besides: an interruption, a compaction.
    Note { text: String },
    /// Its thinking: its words when the CLI keeps them (often it keeps none), and how long since
    /// the step before.
    Thinking { text: String, ms: Option<u64> },
    /// What Claude Code sent it besides your words, out of the conversation shown: its harness's
    /// reminders (`<system-reminder>` blocks, a file changed, the memory read), a hook's context.
    /// `label`: the hook, or the file.
    Reminder {
        #[cfg_attr(feature = "ts", ts(type = "'harness' | 'hook'"))]
        source: &'static str,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        label: Option<String>,
    },
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ConversationTool"))]
pub struct Tool {
    pub tool: String,
    /// `read`, `write`, `command`, `web`, `agent`, `other`.
    #[cfg_attr(feature = "ts", ts(type = "'read' | 'write' | 'command' | 'web' | 'agent' | 'other'"))]
    pub kind: &'static str,
    /// The file, command, pattern, URL or task it was about.
    pub what: Option<String>,
    /// `running`, `ok`, `failed`.
    #[cfg_attr(feature = "ts", ts(type = "'running' | 'ok' | 'failed'"))]
    pub state: &'static str,
    /// A command's output (its last lines) or a failure's message.
    pub output: Option<String>,
    /// An edit: its lines, each led by `+`, `-` or ` `, the first ones; and how many it adds and removes.
    pub diff: Vec<String>,
    pub added: u32,
    pub removed: u32,
    /// A question asked (`AskUserQuestion`): its choices, and once answered, its answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub ask: Option<crate::question::Ask>,
    /// A plan proposed (`ExitPlanMode`): its text, and what became of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub proposal: Option<crate::plan::Proposal>,
    /// A call of Prism's « in passing » tools (`suggest_task`, `dismiss_task`): its Chat shows the
    /// task's card, its id in `output`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub aside: bool,
    /// A call proposing a change of Prism (`prism_propose_config`, `prism_propose_layout`,
    /// `prism_create_custom_block`, `prism_wait_proposal`): its Chat shows the proposal's card while
    /// it waits, then its answer (`output`: `{"status", "reason"?, "errors"?}`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub propose: bool,
    /// A proposal of Prism's assistant (`assistant_propose_*`, `assistant_wait`): what it proposes
    /// (`ticket`, `launch`, `note`, `repo`, `memory`, `wait`); its Chat shows its card while it
    /// waits, then what became of it (`output`: `{"status", "key"?, "agent"?, "reason"?}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "'ticket' | 'launch' | 'note' | 'repo' | 'memory' | 'wait'"))]
    pub assistant: Option<String>,
    /// When it was called (ISO 8601), and once done, how long it took.
    pub at: Option<String>,
    pub ms: Option<u64>,
    /// The reminders Claude Code added to its result (`<system-reminder>`), out of its output.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<String>>", optional))]
    pub reminders: Vec<String>,
    /// A subagent (`Agent`, `Task`): what it did, from its own transcript.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub sub: Option<Sub>,
    /// A subagent's type (`subagent_type`: `Explore`, `Plan`…), none for the general one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub agent: Option<String>,
    /// A subagent's prompt, which finds its transcript (failing its call's id, in its `.meta.json`).
    #[serde(skip)]
    pub prompt: Option<String>,
    #[serde(skip)]
    pub call: Option<String>,
    /// A skill run as a subagent (forked), or a subagent woken by a message (`SendMessage`): its
    /// agent's id, which names its transcript (`agent-<id>.jsonl`, whose `.meta.json` has no call's
    /// id, or that of the call that first launched it).
    #[serde(skip)]
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "SubagentRun"))]
pub struct Sub {
    /// Its last calls, and how many it made.
    pub steps: Vec<SubStep>,
    pub count: u32,
    /// Its last words.
    pub said: Option<String>,
    /// Still at work: its call under way, or, launched in the background (Claude Code runs them
    /// so by default), its task not over.
    pub running: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "SubagentStep"))]
pub struct SubStep {
    pub tool: String,
    pub what: Option<String>,
    /// `running`, `ok`, `failed`.
    #[cfg_attr(feature = "ts", ts(type = "'running' | 'ok' | 'failed'"))]
    pub state: &'static str,
}

/// A plan waiting for its answer whose file was written before what was read: read from the file.
fn waiting_plan(turns: &mut [Turn]) {
    let waiting = turns.iter_mut().flat_map(|t| t.entries.iter_mut()).filter_map(|e| match e {
        Entry::Tool(Tool { proposal: Some(p), .. }) if p.verdict == "waiting" && p.text.is_empty() => Some(p),
        _ => None,
    });
    for p in waiting {
        if let Some(text) = p.file.as_deref().and_then(|f| std::fs::read_to_string(f).ok()) {
            p.text = cut(text.trim(), TEXT_CHARS);
        }
    }
}

/// The kind of a tool call (Claude Code's names, Codex's).
fn kind_of(tool: &str) -> &'static str {
    match tool {
        "Read" | "Grep" | "Glob" | "LS" | "view_image" => "read",
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" | "apply_patch" => "write",
        "Bash" | "BashOutput" | "KillShell" | "shell" | "exec" | "exec_command" | "local_shell" | "write_stdin" => "command",
        "WebFetch" | "WebSearch" | "web_search" => "web",
        "Task" | "Agent" => "agent",
        _ => "other",
    }
}

/// A transcript read so far: from `start` (a line's start) up to `end` (past its last line read),
/// folded; what is written after `end` is all there is to read next time.
struct Reading {
    start: u64,
    end: u64,
    fold: Fold,
}

/// One slot per transcript, with when it was last asked for: two readers of the same transcript
/// (the Chat, the Plan tab, a phone) wait for each other rather than both reading it again from
/// scratch, the last one in replacing the other's; other transcripts are read in parallel.
type Slot = Arc<Mutex<Option<Reading>>>;
static READ: LazyLock<Mutex<HashMap<PathBuf, Kept>>> = LazyLock::new(Default::default);

struct Kept {
    used: Instant,
    /// What its reading read, as last left there.
    bytes: u64,
    slot: Slot,
}

/// Leaves the least used transcripts out of `map`, `current` (being read) kept: past `KEPT`
/// whatever they are, past `KEPT_BYTES` those idle.
fn evict(map: &mut HashMap<PathBuf, Kept>, current: &Path) {
    loop {
        let total: u64 = map.values().map(|k| k.bytes).sum();
        let over_count = map.len() > KEPT;
        if !over_count && total <= KEPT_BYTES {
            return;
        }
        let oldest = map.iter().filter(|(p, k)| p.as_path() != current && (over_count || k.used.elapsed() >= IDLE)).min_by_key(|(_, k)| k.used).map(|(p, _)| p.clone());
        let Some(p) = oldest else { return };
        map.remove(&p);
    }
}

impl Reading {
    fn new(start: u64, prefix: Option<String>) -> Self {
        let fold = Fold::new(Names { prefix });
        Self { start, end: start, fold }
    }

    /// Folds what was written since `end`: its full lines, and a last one without its line break
    /// once it reads whole (a line being written does not parse). From the middle of the file,
    /// the line it starts in is skipped.
    fn catch_up(&mut self, f: &mut std::fs::File) -> std::io::Result<()> {
        let mut buf = Vec::new();
        f.seek(SeekFrom::Start(self.end))?;
        f.read_to_end(&mut buf)?;
        let mut from = 0;
        if self.end == self.start && self.start > 0 {
            let Some(nl) = buf.iter().position(|b| *b == b'\n') else { return Ok(()) };
            from = nl + 1;
            self.start += from as u64;
        }
        let mut read = from;
        for (i, line) in buf[from..].split_inclusive(|b| *b == b'\n').scan(from, |at, l| Some((std::mem::replace(at, *at + l.len()), l))) {
            let whole = line.ends_with(b"\n");
            let Ok(r) = serde_json::from_slice::<Value>(line.strip_suffix(b"\n").unwrap_or(line)) else {
                if whole {
                    read = i + line.len();
                }
                continue;
            };
            self.fold.line = self.end + i as u64;
            if r.get("payload").is_some() {
                self.fold.codex(&r);
            } else if r.get("type").and_then(Value::as_str) == Some("message") {
                self.fold.command_code(&r);
            } else {
                self.fold.claude(&r);
            }
            read = i + line.len();
        }
        self.end += read as u64;
        Ok(())
    }
}

/// The transcript's last `turns` turns; `cwd`: the agent's folder, cut from the paths named.
/// Read once, then only what was written since; further back when more turns are asked than read.
pub fn read(transcript: &Path, cwd: Option<&Path>, turns: usize) -> Conversation {
    let Ok(mut f) = std::fs::File::open(transcript) else { return Conversation::default() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let prefix = cwd.map(|c| format!("{}/", c.display()));
    let slot = {
        let mut map = READ.lock().unwrap();
        let kept = map.entry(transcript.to_path_buf()).or_insert_with(|| Kept { used: Instant::now(), bytes: 0, slot: Slot::default() });
        kept.used = Instant::now();
        let slot = kept.slot.clone();
        evict(&mut map, transcript);
        slot
    };
    let mut held = slot.lock().unwrap();
    // Shorter than read (written again), or for another folder: read afresh.
    let mut kept = held.take().filter(|r| r.end <= len && r.fold.names.prefix == prefix);
    let mut window = TAIL_BYTES;
    let mut reading = loop {
        let mut r = kept.take().unwrap_or_else(|| Reading::new(len.saturating_sub(window), prefix.clone()));
        if r.catch_up(&mut f).is_err() {
            return Conversation::default();
        }
        let far = len - r.start.min(len);
        if r.start == 0 || r.fold.turns.len() > turns || far >= MAX_BYTES {
            break r;
        }
        window = far.max(TAIL_BYTES).saturating_mul(4).min(MAX_BYTES);
    };
    // The turns unchanged since the last read keep their signature (64–90 ms for 5000 turns).
    let range = reading.fold.turns.len().saturating_sub(turns.max(1))..reading.fold.turns.len();
    reading.fold.turns.sign(range.clone(), filled_elsewhere);
    let fold = &reading.fold;
    let total = fold.turns.len() as u32;
    let mut keep = fold.turns[range].to_vec();
    for t in keep.iter_mut().filter(|t| filled_elsewhere(t)) {
        t.sig.clear();
    }
    subagents(transcript, &mut keep, &fold.names, &fold.background);
    waiting_plan(&mut keep);
    let mut background = fold.background.clone();
    let ended = background.iter().filter(|b| b.state != "running").count();
    let mut over = ended.saturating_sub(BACKGROUND_ENDED);
    background.retain(|b| b.state == "running" || over == 0 || {
        over -= 1;
        false
    });
    let c = Conversation { turns: keep, total, partial: reading.start > 0, plan: fold.plan.clone(), background, live: None, model: fold.model.clone(), kept: Vec::new() };
    if let Some(k) = READ.lock().unwrap().get_mut(transcript) {
        k.bytes = reading.end - reading.start;
    }
    *held = Some(reading);
    c
}

/// A turn read again from elsewhere at each read, signed afresh each time: a subagent's calls
/// (`subagents`), a plan waiting whose text is in its file (`waiting_plan`).
fn filled_elsewhere(t: &Turn) -> bool {
    t.entries.iter().any(|e| match e {
        Entry::Tool(tool) => (tool.kind == "agent" && (tool.prompt.is_some() || tool.agent_id.is_some())) || tool.proposal.as_ref().is_some_and(|p| p.verdict == "waiting" && p.text.is_empty()),
        _ => false,
    })
}
