//! What an agent's context is made of (the context gauge's detail), read again from its transcript
//! on demand: Claude Code's, or Codex's rollout. The base — system prompt, tools, MCP, skills,
//! CLAUDE.md or AGENTS.md: all the CLI sends before your first word — is measured, the first
//! request's tokens less that first prompt; the conversation since the last compaction is split by
//! the size of each message (about 4 characters a token, an image about 1600), scaled to the
//! tokens measured at the last reply. Only sizes and names leave the transcript: a file read, a
//! command run, a URL fetched — never a prompt, an answer, a file's content nor a command's output.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::Instant,
};

use serde::Serialize;
use serde_json::Value;

use crate::context::{self, ContextUse};

/// Tokens of an image in a message, about (a 1092×1092 picture is 1590).
const IMAGE: u64 = 1600;
/// The heaviest items listed.
const HEAVIEST: usize = 6;
/// A command or a pattern, cut at this many characters.
const WHAT_CHARS: usize = 90;
/// Items kept between two reads before only the heaviest are: the heaviest stay the heaviest.
const ITEMS_KEPT: usize = 64;
/// Transcripts whose reading is kept (the least recently asked goes).
const KEPT: usize = 8;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentContextDetail"))]
pub struct Breakdown {
    /// Tokens sent at the last reply, out of the window (unknown: absent).
    pub used: u64,
    pub size: Option<u64>,
    /// All the CLI sends before your first word, measured; absent when the transcript does not
    /// start with the conversation.
    pub base: Option<u64>,
    /// The conversation since the last compaction, by kind (`summary`, `prompts`, `replies`,
    /// `calls`, `results`, `images`, `reminders`), in that order, the empty ones left out.
    pub parts: Vec<Part>,
    pub heaviest: Vec<Heavy>,
    /// Compactions of the conversation, and when the last one happened.
    pub compactions: u32,
    pub compacted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ContextPart"))]
pub struct Part {
    #[cfg_attr(feature = "ts", ts(type = "'summary' | 'prompts' | 'replies' | 'calls' | 'results' | 'images' | 'reminders'"))]
    pub kind: &'static str,
    pub tokens: u64,
    /// For tool calls and results: by tool, the heaviest first.
    pub tools: Vec<ToolShare>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ToolShare"))]
pub struct ToolShare {
    pub tool: String,
    pub tokens: u64,
    pub count: u32,
}

/// A tool's call or result, among the heaviest of the context.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "HeavyItem"))]
pub struct Heavy {
    pub tool: String,
    /// The file, command, pattern, URL or task it was about.
    pub what: Option<String>,
    pub tokens: u64,
    /// The prompt of yours it answered, counted from the start of the conversation.
    pub turn: u32,
    /// Its call (a file written whole, a long script), rather than its result.
    pub call: bool,
}

const KINDS: [&str; 7] = ["summary", "prompts", "replies", "calls", "results", "images", "reminders"];

/// What a call was about, kept until its result comes.
struct Call {
    tool: String,
    what: Option<String>,
    turn: u32,
}

/// The conversation as read so far: estimates, before scaling.
#[derive(Default)]
struct Fold {
    base: Option<u64>,
    parts: HashMap<&'static str, u64>,
    tools: HashMap<(&'static str, String), (u64, u32)>,
    items: Vec<Heavy>,
    calls: HashMap<String, Call>,
    turn: u32,
    compactions: u32,
    compacted_at: Option<String>,
}

impl Fold {
    fn add(&mut self, kind: &'static str, tokens: u64) {
        *self.parts.entry(kind).or_default() += tokens;
    }

    fn call(&mut self, id: Option<&str>, tool: &str, what: Option<String>, tokens: u64) {
        self.add("calls", tokens);
        self.share("calls", tool, tokens);
        self.push(Heavy { tool: tool.to_string(), what: what.clone(), tokens, turn: self.turn, call: true });
        if let Some(id) = id {
            self.calls.insert(id.to_string(), Call { tool: tool.to_string(), what, turn: self.turn });
        }
    }

    fn result(&mut self, id: Option<&str>, tokens: u64) {
        let (tool, what, turn) = match id.and_then(|id| self.calls.get(id)) {
            Some(c) => (c.tool.clone(), c.what.clone(), c.turn),
            None => ("tool".to_string(), None, self.turn),
        };
        self.add("results", tokens);
        self.share("results", &tool, tokens);
        self.push(Heavy { tool, what, tokens, turn, call: false });
    }

    /// An item; past `ITEMS_KEPT`, only the heaviest stay (scaling keeps their order).
    fn push(&mut self, item: Heavy) {
        self.items.push(item);
        if self.items.len() > ITEMS_KEPT {
            self.items.sort_by_key(|a| std::cmp::Reverse(a.tokens));
            self.items.truncate(HEAVIEST);
        }
    }

    fn share(&mut self, kind: &'static str, tool: &str, tokens: u64) {
        let e = self.tools.entry((kind, tool.to_string())).or_default();
        e.0 += tokens;
        e.1 += 1;
    }

    /// The first request measured: what was counted so far, but your prompt, is the base's.
    fn measure_base(&mut self, first_request: u64) {
        if self.base.is_some() || first_request == 0 {
            return;
        }
        let prompt = self.parts.get("prompts").copied().unwrap_or(0) + self.parts.get("images").copied().unwrap_or(0);
        self.base = Some(first_request.saturating_sub(prompt));
        self.parts.retain(|k, _| *k == "prompts" || *k == "images");
    }

    /// A compaction: the conversation so far gives way to its summary; the base stays.
    fn compact(&mut self, at: Option<String>) {
        let base = self.base;
        let (turn, compactions) = (self.turn, self.compactions + 1);
        *self = Fold { base, turn, compactions, compacted_at: at, ..Fold::default() };
    }

    /// Scaled to the tokens measured at the last reply; the fold stays, to read on from.
    fn finish(&self, used: ContextUse) -> Breakdown {
        let estimated: u64 = self.parts.values().sum();
        let target = used.used.saturating_sub(self.base.unwrap_or(0));
        let factor = if estimated > 0 { target as f64 / estimated as f64 } else { 0.0 };
        let scale = |n: u64| (n as f64 * factor).round() as u64;
        let parts = KINDS
            .iter()
            .filter_map(|k| {
                let tokens = scale(self.parts.get(k).copied().unwrap_or(0));
                let mut tools: Vec<ToolShare> = self.tools.iter().filter(|((kind, _), _)| kind == k).map(|((_, tool), (n, count))| ToolShare { tool: tool.clone(), tokens: scale(*n), count: *count }).collect();
                tools.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.tool.cmp(&b.tool)));
                (tokens > 0).then_some(Part { kind: k, tokens, tools })
            })
            .collect();
        let mut heaviest = self.items.clone();
        heaviest.sort_by_key(|a| std::cmp::Reverse(a.tokens));
        heaviest.truncate(HEAVIEST);
        for i in &mut heaviest {
            i.tokens = scale(i.tokens);
        }
        Breakdown { used: used.used, size: used.size, base: self.base, parts, heaviest, compactions: self.compactions, compacted_at: self.compacted_at.clone() }
    }
}

/// A transcript read so far: up to `end` (past its last whole line), folded, for its folder's names.
struct Reading {
    end: u64,
    prefix: Option<String>,
    fold: Fold,
    used: Instant,
}

static READ: LazyLock<Mutex<HashMap<PathBuf, Reading>>> = LazyLock::new(Default::default);

/// The breakdown of a transcript; absent when it has no reply yet. `cwd`: the agent's folder, cut
/// from the paths named. Read whole once, then only what was written since (the popover asks again
/// at each reply: 174 ms each time for an 88 MB transcript, measured); afresh when it got shorter.
pub fn read(transcript: &Path, cwd: Option<&Path>) -> Option<Breakdown> {
    let used = context::read(transcript)?;
    let mut file = std::fs::File::open(transcript).ok()?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let prefix = cwd.map(|c| format!("{}/", c.display()));
    // Out of the map while it is read: another transcript's detail does not wait for this one.
    let kept = READ.lock().unwrap().remove(transcript).filter(|r| r.end <= len && r.prefix == prefix);
    let mut reading = kept.unwrap_or_else(|| Reading { end: 0, prefix: prefix.clone(), fold: Fold::default(), used: Instant::now() });
    if file.seek(SeekFrom::Start(reading.end)).is_ok() {
        let names = Names { prefix };
        let mut lines = BufReader::new(file);
        let mut line = Vec::new();
        loop {
            line.clear();
            match lines.read_until(b'\n', &mut line) {
                // A line being written: read with the next ones.
                Ok(n) if n > 0 && line.ends_with(b"\n") => reading.end += n as u64,
                _ => break,
            }
            let Ok(r) = serde_json::from_slice::<Value>(&line) else { continue };
            if r.get("payload").is_some() {
                codex(&mut reading.fold, &r);
            } else {
                claude(&mut reading.fold, &r, &names);
            }
        }
    }
    let b = reading.fold.finish(used);
    reading.used = Instant::now();
    let mut map = READ.lock().unwrap();
    map.insert(transcript.to_path_buf(), reading);
    if map.len() > KEPT {
        let oldest = map.iter().min_by_key(|(_, r)| r.used).map(|(p, _)| p.clone());
        oldest.map(|p| map.remove(&p));
    }
    Some(b)
}

/// About 4 characters a token.
fn est(s: &str) -> u64 {
    (s.chars().count() as u64).div_ceil(4)
}

/// The text of every string inside a value, measured.
fn est_value(v: &Value) -> u64 {
    match v {
        Value::String(s) => est(s),
        Value::Array(a) => a.iter().map(est_value).sum(),
        Value::Object(o) => o.values().map(est_value).sum(),
        _ => 0,
    }
}

/// A tool result's content: text, or blocks of text and images.
/// A result's text, its blocks of text joined.
fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

fn est_content(v: &Value) -> u64 {
    match v {
        Value::String(s) => est(s),
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match b.get("type").and_then(Value::as_str) {
                Some("image" | "input_image") => IMAGE,
                _ => b.get("text").and_then(Value::as_str).map_or(0, est),
            })
            .sum(),
        _ => 0,
    }
}

pub(crate) struct Names {
    pub(crate) prefix: Option<String>,
}

impl Names {
    /// What a tool's call was about, from its input: never a file's content, only its path.
    pub(crate) fn what(&self, tool: &str, input: &Value) -> Option<String> {
        let field = |k: &str| input.get(k).and_then(Value::as_str);
        let raw = match tool {
            "Read" | "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => field("file_path").or(field("notebook_path")).map(|p| self.relative(p)),
            "Bash" => field("command").map(String::from),
            "Grep" | "Glob" => field("pattern").map(String::from),
            "LS" => field("path").map(|p| self.relative(p)),
            "WebFetch" => field("url").map(String::from),
            "WebSearch" => field("query").map(String::from),
            "Agent" | "Task" => field("description").map(String::from),
            "Skill" => field("skill").or(field("command")).map(String::from),
            "SendMessage" => field("summary").map(String::from),
            _ => None,
        }?;
        Some(short(&raw))
    }

    fn relative(&self, path: &str) -> String {
        self.prefix.as_deref().and_then(|p| path.strip_prefix(p)).unwrap_or(path).to_string()
    }
}

/// The first line, cut.
fn short(s: &str) -> String {
    let line = s.trim().lines().next().unwrap_or("").trim();
    if line.chars().count() > WHAT_CHARS { format!("{}…", line.chars().take(WHAT_CHARS).collect::<String>()) } else { line.to_string() }
}

/// A line of Claude Code's transcript.
fn claude(fold: &mut Fold, r: &Value, names: &Names) {
    if r.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return;
    }
    let flag = |k: &str| r.get(k).and_then(Value::as_bool) == Some(true);
    match r.get("type").and_then(Value::as_str) {
        Some("system") if r.get("subtype").and_then(Value::as_str) == Some("compact_boundary") => {
            fold.compact(r.get("timestamp").and_then(Value::as_str).map(String::from));
        }
        Some("user") => {
            let Some(content) = r.pointer("/message/content") else { return };
            if flag("isCompactSummary") {
                fold.add("summary", est_content(content));
                return;
            }
            let meta = flag("isMeta");
            let text_kind = if meta { "reminders" } else { "prompts" };
            // A prompt's reminders (`<system-reminder>`) count as such.
            let words = |fold: &mut Fold, s: &str| {
                let (rest, found) = crate::conversation::reminders(s);
                fold.add(text_kind, est(&rest));
                if !found.is_empty() {
                    fold.add("reminders", found.iter().map(|r| est(r)).sum());
                }
            };
            match content {
                Value::String(s) => {
                    if !meta {
                        fold.turn += 1;
                    }
                    words(fold, s);
                }
                Value::Array(blocks) => {
                    let mut prompted = false;
                    for b in blocks {
                        match b.get("type").and_then(Value::as_str) {
                            Some("tool_result") => {
                                // The reminders Claude Code adds to it are not the tool's.
                                let content = b.get("content");
                                let reminded: u64 = content.map(|c| crate::conversation::reminders(&text_of(c)).1.iter().map(|r| est(r)).sum()).unwrap_or(0);
                                let tokens = content.map_or(0, est_content);
                                fold.result(b.get("tool_use_id").and_then(Value::as_str), tokens.saturating_sub(reminded));
                                if reminded > 0 {
                                    fold.add("reminders", reminded);
                                }
                            }
                            Some("image") => fold.add("images", IMAGE),
                            Some("text") => {
                                prompted |= !meta;
                                words(fold, b.get("text").and_then(Value::as_str).unwrap_or_default());
                            }
                            _ => {}
                        }
                    }
                    if prompted {
                        fold.turn += 1;
                    }
                }
                _ => {}
            }
        }
        Some("assistant") => {
            let Some(msg) = r.get("message") else { return };
            if msg.get("model").and_then(Value::as_str) == Some("<synthetic>") {
                return;
            }
            if fold.base.is_none()
                && let Some(u) = msg.get("usage")
            {
                let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
                fold.measure_base(n("input_tokens") + n("cache_creation_input_tokens") + n("cache_read_input_tokens"));
            }
            for b in msg.get("content").and_then(Value::as_array).into_iter().flatten() {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => fold.add("replies", b.get("text").and_then(Value::as_str).map_or(0, est)),
                    Some("thinking") => fold.add("replies", b.get("thinking").and_then(Value::as_str).map_or(0, est)),
                    Some("tool_use") => {
                        let tool = b.get("name").and_then(Value::as_str).unwrap_or("tool");
                        let input = b.get("input").unwrap_or(&Value::Null);
                        fold.call(b.get("id").and_then(Value::as_str), tool, names.what(tool, input), est(tool) + est_value(input));
                    }
                    _ => {}
                }
            }
        }
        // What Claude Code adds to a turn by itself (reminders, a file edited meanwhile, a hook's
        // context); its own setup (the prompt, the environment) is the base's.
        Some("attachment") => {
            let Some(a) = r.get("attachment") else { return };
            if !matches!(a.get("type").and_then(Value::as_str), Some("prompt_snapshot" | "environment" | "deferred_tools_record" | "queued_command")) {
                fold.add("reminders", est_value(a).saturating_sub(est_value(a.get("type").unwrap_or(&Value::Null))));
            }
        }
        _ => {}
    }
}

/// A line of Codex's rollout.
fn codex(fold: &mut Fold, r: &Value) {
    let p = &r["payload"];
    let kind = p.get("type").and_then(Value::as_str);
    match r.get("type").and_then(Value::as_str) {
        Some("compacted") => {
            fold.compact(r.get("timestamp").and_then(Value::as_str).map(String::from));
            fold.add("summary", p.get("message").map_or(0, est_value));
        }
        Some("event_msg") if kind == Some("token_count") && fold.base.is_none() => {
            fold.measure_base(p.pointer("/info/last_token_usage/input_tokens").and_then(Value::as_u64).unwrap_or(0));
        }
        Some("response_item") => match kind {
            Some("message") => {
                let text: Vec<&str> = p.get("content").and_then(Value::as_array).into_iter().flatten().filter_map(|b| b.get("text").and_then(Value::as_str)).collect();
                let images = p.get("content").and_then(Value::as_array).into_iter().flatten().filter(|b| b.get("type").and_then(Value::as_str) == Some("input_image")).count() as u64;
                let tokens: u64 = text.iter().map(|t| est(t)).sum();
                match p.get("role").and_then(Value::as_str) {
                    Some("assistant") => fold.add("replies", tokens),
                    // Its context (`<environment_context>`, `<user_instructions>`…), not a prompt of yours.
                    Some("user") if !text.iter().all(|t| t.trim_start().starts_with('<')) => {
                        fold.turn += 1;
                        fold.add("prompts", tokens);
                        fold.add("images", images * IMAGE);
                    }
                    _ => fold.add("reminders", tokens),
                }
            }
            // Its summary, and its encrypted reasoning, about a token every 5 characters of it.
            Some("reasoning") => {
                let summary = p.get("summary").map_or(0, est_value);
                let encrypted = p.get("encrypted_content").and_then(Value::as_str).map_or(0, |s| s.len() as u64 / 5);
                fold.add("replies", summary + encrypted);
            }
            Some("function_call" | "custom_tool_call" | "local_shell_call") => {
                let tool = p.get("name").and_then(Value::as_str).unwrap_or("shell");
                let input = p.get("input").or(p.get("arguments")).or(p.get("action")).unwrap_or(&Value::Null);
                fold.call(p.get("call_id").and_then(Value::as_str), tool, codex_what(input), est(tool) + est_value(input));
            }
            Some("function_call_output" | "custom_tool_call_output") => {
                fold.result(p.get("call_id").and_then(Value::as_str), p.get("output").map_or(0, est_content));
            }
            _ => {}
        },
        _ => {}
    }
}

/// What a Codex call ran: its script's first line, or a command given as a list.
pub(crate) fn codex_what(input: &Value) -> Option<String> {
    let s = input.as_str()?;
    match serde_json::from_str::<Value>(s) {
        Ok(Value::Object(o)) => match o.get("command") {
            Some(Value::Array(argv)) => Some(short(&argv.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "))),
            Some(Value::String(c)) => Some(short(c)),
            _ => None,
        },
        _ => Some(short(s)).filter(|w| !w.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, lines: &[Value]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("prism-breakdown-{name}-{}.jsonl", std::process::id()));
        std::fs::write(&path, lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n")).unwrap();
        path
    }

    fn assistant(input: u64, content: Value) -> Value {
        serde_json::json!({ "type": "assistant", "message": { "model": "claude-opus-5-5", "usage": { "input_tokens": 1, "cache_creation_input_tokens": 0, "cache_read_input_tokens": input - 1 }, "content": content } })
    }

    #[test]
    fn splits_claude_codes_context() {
        let prompt = "x".repeat(400);
        let output = "y".repeat(8000);
        let path = file("claude", &[
            serde_json::json!({ "type": "user", "message": { "content": prompt } }),
            assistant(20_100, serde_json::json!([{ "type": "tool_use", "id": "t1", "name": "Read", "input": { "file_path": "/repo/src/main.rs" } }])),
            serde_json::json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": output }] } }),
            serde_json::json!({ "type": "assistant", "isSidechain": true, "message": { "content": [{ "type": "text", "text": "z".repeat(90_000) }] } }),
            assistant(22_400, serde_json::json!([{ "type": "text", "text": "done" }])),
        ]);
        let b = read(&path, Some(Path::new("/repo"))).unwrap();
        assert_eq!((b.used, b.base), (22_400, Some(20_000)), "the first request, less the prompt's 100");
        let results = b.parts.iter().find(|p| p.kind == "results").unwrap();
        assert!((2200..=2350).contains(&results.tokens), "most of the 2400 measured past the base: {}", results.tokens);
        assert_eq!(b.parts.iter().map(|p| p.tokens).sum::<u64>() + 20_000, 22_400);
        assert_eq!(results.tools[0].tool, "Read");
        assert_eq!((b.heaviest[0].what.as_deref(), b.heaviest[0].turn, b.heaviest[0].call), (Some("src/main.rs"), 1, false));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reads_on_from_where_it_stopped() {
        let lines = [
            serde_json::json!({ "type": "user", "message": { "content": "x".repeat(400) } }),
            assistant(20_100, serde_json::json!([{ "type": "tool_use", "id": "t1", "name": "Read", "input": { "file_path": "/repo/a.rs" } }])),
            serde_json::json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": "y".repeat(8000) }] } }),
            assistant(22_400, serde_json::json!([{ "type": "tool_use", "id": "t2", "name": "Bash", "input": { "command": "cargo test" } }])),
            serde_json::json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "t2", "content": "z".repeat(2000) }] } }),
            assistant(23_000, serde_json::json!([{ "type": "text", "text": "done" }])),
        ];
        let text = |l: &[Value]| l.iter().map(|l| format!("{l}\n")).collect::<String>();
        let path = std::env::temp_dir().join(format!("prism-breakdown-on-{}.jsonl", std::process::id()));
        let fresh = std::env::temp_dir().join(format!("prism-breakdown-fresh-{}.jsonl", std::process::id()));
        std::fs::write(&path, text(&lines[..3])).unwrap();
        read(&path, Some(Path::new("/repo"))).unwrap();
        // Written since, a line still half written: read on, the half line with the next read.
        std::fs::write(&path, text(&lines) + "{\"type\":\"us").unwrap();
        std::fs::write(&fresh, text(&lines)).unwrap();
        let on = read(&path, Some(Path::new("/repo"))).unwrap();
        assert_eq!(on, read(&fresh, Some(Path::new("/repo"))).unwrap());
        assert_eq!(on.heaviest.len(), 4);
        // Written again shorter (a new conversation in the file): read afresh.
        std::fs::write(&path, text(&lines[..2])).unwrap();
        std::fs::write(&fresh, text(&lines[..2])).unwrap();
        assert_eq!(read(&path, Some(Path::new("/repo"))), read(&fresh, Some(Path::new("/repo"))));
        let _ = (std::fs::remove_file(path), std::fs::remove_file(fresh));
    }

    #[test]
    fn counts_a_prompts_and_a_results_reminders_as_such() {
        let reminder = format!("\n<system-reminder>{}</system-reminder>", "r".repeat(4000));
        let path = file("reminders", &[
            serde_json::json!({ "type": "user", "message": { "content": format!("go{reminder}") } }),
            assistant(20_100, serde_json::json!([{ "type": "tool_use", "id": "t1", "name": "Read", "input": { "file_path": "/repo/a.rs" } }])),
            serde_json::json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": format!("{}{reminder}", "y".repeat(400)) }] } }),
            assistant(23_000, serde_json::json!([{ "type": "text", "text": "done" }])),
        ]);
        let b = read(&path, None).unwrap();
        let part = |k: &str| b.parts.iter().find(|p| p.kind == k).map_or(0, |p| p.tokens);
        // 2 × 1000 tokens of reminders, a prompt of 1, a result of 100.
        assert!(part("prompts") < 10 && part("results") * 5 < part("reminders"), "{:?}", b.parts);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn counts_from_the_last_compaction() {
        let path = file("compact", &[
            serde_json::json!({ "type": "user", "message": { "content": "go" } }),
            assistant(10_001, serde_json::json!([{ "type": "text", "text": "a".repeat(40_000) }])),
            serde_json::json!({ "type": "system", "subtype": "compact_boundary", "timestamp": "2026-10-16T14:02:00Z" }),
            serde_json::json!({ "type": "user", "isCompactSummary": true, "message": { "content": "s".repeat(4000) } }),
            assistant(11_000, serde_json::json!([{ "type": "text", "text": "ok" }])),
        ]);
        let b = read(&path, None).unwrap();
        assert_eq!((b.base, b.compactions, b.compacted_at.as_deref()), (Some(10_000), 1, Some("2026-10-16T14:02:00Z")));
        assert_eq!(b.parts[0].kind, "summary");
        assert!(b.parts.iter().all(|p| p.kind != "prompts"), "what came before is the summary's");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn splits_codexs_context() {
        let path = file("codex", &[
            serde_json::json!({ "type": "response_item", "payload": { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "<environment_context>…</environment_context>" }] } }),
            serde_json::json!({ "type": "response_item", "payload": { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "Fix the test" }] } }),
            serde_json::json!({ "type": "event_msg", "payload": { "type": "token_count", "info": { "last_token_usage": { "input_tokens": 9_003 }, "model_context_window": 272_000 } } }),
            serde_json::json!({ "type": "response_item", "payload": { "type": "custom_tool_call", "name": "exec", "call_id": "c1", "input": "cargo test -p core\necho done" } }),
            serde_json::json!({ "type": "response_item", "payload": { "type": "custom_tool_call_output", "call_id": "c1", "output": [{ "type": "input_text", "text": "o".repeat(4000) }] } }),
            serde_json::json!({ "type": "event_msg", "payload": { "type": "token_count", "info": { "last_token_usage": { "input_tokens": 10_100 }, "model_context_window": 272_000 } } }),
        ]);
        let b = read(&path, None).unwrap();
        assert_eq!((b.used, b.size, b.base), (10_100, Some(272_000), Some(9_000)));
        assert_eq!((b.heaviest[0].tool.as_str(), b.heaviest[0].what.as_deref()), ("exec", Some("cargo test -p core")));
        let _ = std::fs::remove_file(path);
    }
}

