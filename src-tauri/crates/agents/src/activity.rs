//! An agent's activity (the Activity tab): its hook log read again, as a timeline of what it did —
//! files read and written, commands run and how long they took, permissions it waited for — a strip
//! of the time spent on each, and the files it touched. Metadata only, as the log holds: never a
//! prompt, an answer, a file's content nor a command's output.

use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use serde::Serialize;

use super::hook::Line;

/// The end of the log read at most: an agent of several days keeps its last hours.
const TAIL_BYTES: u64 = 4 * 1024 * 1024;
/// The timeline's length, newest first.
const MAX_ITEMS: usize = 400;

#[derive(Debug, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentActivity"))]
pub struct Activity {
    /// Newest first.
    pub items: Vec<Item>,
    pub summary: Summary,
    /// The time spent, in order: on reading, writing, commands, the web, subagents, and waiting for you.
    pub strip: Vec<Span>,
    /// Files read or written, the most touched first.
    pub files: Vec<FileUse>,
    /// The log's first and last event read.
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ActivityItem"))]
pub struct Item {
    pub t: i64,
    /// `read`, `write`, `command`, `web`, `agent`, `other` (a tool), `wait` (a permission), `prompt`
    /// (yours), `stop` (its turn over).
    #[cfg_attr(feature = "ts", ts(type = "'read' | 'write' | 'command' | 'web' | 'agent' | 'other' | 'wait' | 'prompt' | 'stop'"))]
    pub kind: &'static str,
    pub tool: Option<String>,
    pub what: Option<String>,
    pub path: Option<String>,
    /// How long it took (a tool: from its start to its outcome; a wait: until answered); absent: running.
    pub ms: Option<i64>,
    pub failed: bool,
}

#[derive(Debug, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ActivitySummary"))]
pub struct Summary {
    /// Distinct files read and written.
    pub read: usize,
    pub written: usize,
    pub commands: usize,
    pub failed: usize,
    /// Permissions it waited for, and for how long in all.
    pub waits: usize,
    pub waited_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ActivitySpan"))]
pub struct Span {
    pub from: i64,
    pub to: i64,
    pub kind: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "FileUse"))]
pub struct FileUse {
    pub path: String,
    pub reads: u32,
    pub writes: u32,
}

/// The kind of a tool call.
fn kind_of(tool: Option<&str>) -> &'static str {
    match tool.unwrap_or_default() {
        "Read" | "Grep" | "Glob" | "LS" | "read" | "grep" | "glob" | "list" => "read",
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" | "write" | "edit" | "patch" => "write",
        "Bash" | "BashOutput" | "KillShell" | "bash" => "command",
        "WebFetch" | "WebSearch" | "webfetch" => "web",
        "Task" | "Agent" | "task" => "agent",
        _ => "other",
    }
}

/// The log's last `TAIL_BYTES`, its first partial line dropped.
pub fn read(path: &Path) -> Activity {
    let Ok(mut f) = std::fs::File::open(path) else { return Activity::default() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(TAIL_BYTES);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return Activity::default();
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return Activity::default();
    }
    let lines: Vec<Line> = buf.split(|b| *b == b'\n').skip(usize::from(start > 0)).filter_map(|l| serde_json::from_slice(l).ok()).collect();
    fold(&lines)
}

pub fn fold(lines: &[Line]) -> Activity {
    let mut items: Vec<Item> = Vec::new();
    // Tool calls started, by id (Claude Code) or by tool (the CLIs without ids).
    let mut started: HashMap<String, usize> = HashMap::new();
    let mut waiting: Option<usize> = None;
    let key = |l: &Line| l.id.clone().unwrap_or_else(|| format!("tool:{}", l.tool.as_deref().unwrap_or_default()));
    for l in lines {
        // Any event after a permission request answers it (the tool runs, or the turn ends).
        if l.event != "PermissionRequest" && l.event != "Notification" && let Some(i) = waiting.take() {
            items[i].ms = Some((l.t - items[i].t).max(0));
        }
        match l.event.as_str() {
            "PreToolUse" => {
                started.insert(key(l), items.len());
                items.push(Item { t: l.t, kind: kind_of(l.tool.as_deref()), tool: l.tool.clone(), what: l.cmd.clone().or_else(|| l.what.clone()), path: l.path.clone(), ms: None, failed: false });
            }
            "PostToolUse" | "PostToolUseFailure" => {
                let failed = l.event == "PostToolUseFailure";
                match started.remove(&key(l)) {
                    Some(i) => {
                        items[i].ms = Some((l.t - items[i].t).max(0));
                        items[i].failed = failed;
                    }
                    None => items.push(Item { t: l.t, kind: kind_of(l.tool.as_deref()), tool: l.tool.clone(), what: l.cmd.clone().or_else(|| l.what.clone()), path: l.path.clone(), ms: Some(0), failed }),
                }
            }
            "PermissionRequest" => {
                waiting = Some(items.len());
                items.push(Item { t: l.t, kind: "wait", tool: l.tool.clone(), what: l.cmd.clone().or_else(|| l.what.clone()), path: None, ms: None, failed: false });
            }
            "UserPromptSubmit" => items.push(Item { t: l.t, kind: "prompt", tool: None, what: None, path: None, ms: Some(0), failed: false }),
            "Stop" if l.background.unwrap_or(0) == 0 => items.push(Item { t: l.t, kind: "stop", tool: None, what: None, path: None, ms: Some(0), failed: false }),
            _ => {}
        }
    }

    let mut summary = Summary::default();
    let mut files: HashMap<String, FileUse> = HashMap::new();
    for it in &items {
        match it.kind {
            "command" => {
                summary.commands += 1;
                summary.failed += usize::from(it.failed);
            }
            "wait" => {
                summary.waits += 1;
                summary.waited_ms += it.ms.unwrap_or(0);
            }
            _ => {}
        }
        if let (Some(path), "read" | "write") = (&it.path, it.kind) {
            let f = files.entry(path.clone()).or_insert_with(|| FileUse { path: path.clone(), reads: 0, writes: 0 });
            if it.kind == "read" { f.reads += 1 } else { f.writes += 1 }
        }
    }
    summary.read = files.values().filter(|f| f.reads > 0).count();
    summary.written = files.values().filter(|f| f.writes > 0).count();
    let mut files: Vec<FileUse> = files.into_values().collect();
    files.sort_by(|a, b| (b.writes * 2 + b.reads).cmp(&(a.writes * 2 + a.reads)).then_with(|| a.path.cmp(&b.path)));

    let to = lines.last().map(|l| l.t);
    let strip = items
        .iter()
        .filter(|it| !matches!(it.kind, "prompt" | "stop"))
        .filter_map(|it| {
            let end = it.ms.map(|ms| it.t + ms).or(to)?;
            (end > it.t).then_some(Span { from: it.t, to: end, kind: it.kind })
        })
        .collect();
    items.reverse();
    items.truncate(MAX_ITEMS);
    Activity { items, summary, strip, files, from: lines.first().map(|l| l.t), to }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(t: i64, event: &str, tool: Option<&str>, id: Option<&str>) -> Line {
        Line { t, event: event.into(), tool: tool.map(String::from), id: id.map(String::from), ..Default::default() }
    }

    #[test]
    fn pairs_tools_counts_failures_and_waits() {
        let mut read = line(0, "PreToolUse", Some("Read"), Some("a"));
        read.path = Some("src/vat.ts".into());
        let mut bash = line(2_000, "PreToolUse", Some("Bash"), Some("b"));
        bash.cmd = Some("npm test".into());
        let lines = vec![
            line(-1_000, "UserPromptSubmit", None, None),
            read,
            line(500, "PostToolUse", Some("Read"), Some("a")),
            line(1_000, "PermissionRequest", Some("Bash"), Some("b")),
            bash,
            line(9_000, "PostToolUseFailure", Some("Bash"), Some("b")),
            line(9_500, "Stop", None, None),
        ];
        let a = fold(&lines);
        assert_eq!(a.summary, Summary { read: 1, written: 0, commands: 1, failed: 1, waits: 1, waited_ms: 1_000 });
        let cmd = a.items.iter().find(|i| i.kind == "command").unwrap();
        assert_eq!((cmd.ms, cmd.failed, cmd.what.as_deref()), (Some(7_000), true, Some("npm test")));
        assert_eq!(a.items.first().unwrap().kind, "stop");
        assert_eq!(a.files, vec![FileUse { path: "src/vat.ts".into(), reads: 1, writes: 0 }]);
    }

    #[test]
    fn a_tool_still_running_has_no_duration() {
        let a = fold(&[line(0, "PreToolUse", Some("Bash"), Some("x")), line(5_000, "Notification", None, None)]);
        assert_eq!(a.items[0].ms, None);
        assert_eq!(a.strip, vec![Span { from: 0, to: 5_000, kind: "command" }]);
    }
}
