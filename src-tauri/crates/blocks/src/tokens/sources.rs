//! Where each coding tool keeps its usage figures, and how they read as replies: Claude Code's and
//! Command Code's transcripts and Codex's rollouts (JSONL, read as they grow), OpenCode's database
//! (SQLite, read-only, the messages changed since). Only figures, models, times and folders are
//! read, never what was said.

use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use prism_core::agents::transcript::{ClaudeRecord, CodexLine};

/// A tool's reply, its tokens in four kinds: input outside the cache, output (reasoning
/// included), cache reads and writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    /// Its id in the tool, when it has one: a reply met twice counts once.
    pub id: Option<String>,
    pub at: DateTime<Utc>,
    pub tool: &'static str,
    pub model: String,
    /// The folder the session ran in, by its name.
    pub project: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Dollars: the tool's own figure, or none (estimated at the API prices for Claude Code).
    pub cost: Option<f64>,
    /// Claude Code's cache writes kept an hour, for the estimate.
    pub long_writes: u64,
}

pub const CLAUDE: &str = "claude";
pub const CODEX: &str = "codex";
pub const COMMAND_CODE: &str = "command-code";
pub const OPENCODE: &str = "opencode";
pub const ALL: [&str; 4] = [CLAUDE, CODEX, COMMAND_CODE, OPENCODE];

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// `~/.claude/projects`, or under `CLAUDE_CONFIG_DIR`.
pub fn claude_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).or_else(|| home().map(|h| h.join(".claude"))).map(|r| r.join("projects"))
}

/// `~/.codex/sessions`, or under `CODEX_HOME`.
pub fn codex_dir() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".codex"))).map(|r| r.join("sessions"))
}

pub fn command_code_dir() -> Option<PathBuf> {
    home().map(|h| h.join(".commandcode").join("projects"))
}

/// `~/.local/share/opencode/opencode.db`, or under `XDG_DATA_HOME`.
pub fn opencode_db() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".local/share"))).map(|d| d.join("opencode").join("opencode.db"))
}

/// The folder's name: the project a session ran in.
pub fn project_of(cwd: Option<&str>) -> String {
    cwd.and_then(|c| Path::new(c).file_name()).map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| "other".into())
}

/// The `.jsonl` files under `root`, down to `depth` folders.
pub fn jsonl_files(root: &Path, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return out };
    for e in entries.flatten() {
        let path = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() && depth > 0 => out.extend(jsonl_files(&path, depth - 1)),
            Ok(t) if t.is_file() && path.extension().is_some_and(|x| x == "jsonl") => out.push(path),
            _ => {}
        }
    }
    out
}

/// Files read as they grow: where each was left, and its unfinished last line.
#[derive(Default)]
pub struct Tail {
    files: HashMap<PathBuf, (u64, Vec<u8>)>,
}

impl Tail {
    /// The complete lines added to `path` since last time (all of them the first time).
    pub fn lines(&mut self, path: &Path, mut each: impl FnMut(&[u8])) {
        let Ok(mut f) = std::fs::File::open(path) else { return };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let (offset, partial) = self.files.entry(path.to_path_buf()).or_default();
        // Rewritten shorter: read again from the start (what was counted is not counted twice).
        if len < *offset {
            (*offset, *partial) = (0, Vec::new());
        }
        if len == *offset || f.seek(SeekFrom::Start(*offset)).is_err() {
            return;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return;
        }
        *offset += buf.len() as u64;
        // Only the unfinished line is kept, in a buffer of its size: draining in place kept the
        // whole first read's capacity, a transcript's size for each file, as long as Prism ran.
        let data = if partial.is_empty() { buf } else { [std::mem::take(partial), buf].concat() };
        let Some(last) = data.iter().rposition(|b| *b == b'\n') else {
            *partial = data;
            return;
        };
        for line in data[..last].split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            each(line);
        }
        *partial = data[last + 1..].to_vec();
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/* ---------- Claude Code ---------- */

/// A line of a Claude Code transcript: an assistant record with its `message.usage`.
pub fn claude(line: &[u8]) -> Option<Reply> {
    // Most lines are not replies: skipped before parsing them.
    if !contains(line, b"\"usage\"") {
        return None;
    }
    let r: ClaudeRecord = serde_json::from_slice(line).ok()?;
    let (at, cwd) = (r.timestamp?, r.cwd.clone());
    let msg = r.reply()?;
    let u = msg.usage?;
    Some(Reply {
        id: msg.id,
        at,
        tool: CLAUDE,
        model: msg.model.unwrap_or_default(),
        project: project_of(cwd.as_deref()),
        input: u.input_tokens,
        output: u.output_tokens,
        cache_read: u.cache_read_input_tokens,
        cache_write: u.cache_creation_input_tokens,
        cost: None,
        long_writes: u.long_writes(),
    })
}

/* ---------- Codex ---------- */

/// What a rollout said so far: its model and folder (`session_meta`, `turn_context`), and the
/// session's running total, to count each `token_count` once (it is sent again unchanged).
#[derive(Default)]
pub struct CodexFile {
    model: String,
    cwd: Option<String>,
    total: u64,
}

/// A line of a Codex rollout: a turn's `token_count` (`last_token_usage`) is a reply; its input
/// counts the cached part, taken out as cache reads.
pub fn codex(line: &[u8], file: &mut CodexFile) -> Option<Reply> {
    if !(contains(line, b"token_count") || contains(line, b"turn_context") || contains(line, b"session_meta")) {
        return None;
    }
    let l: CodexLine = serde_json::from_slice(line).ok()?;
    if l.kind == "session_meta" || l.kind == "turn_context" {
        if let Some(cwd) = l.payload.cwd {
            file.cwd = Some(cwd);
        }
        if let Some(model) = l.payload.model {
            file.model = model;
        }
        return None;
    }
    let at = l.timestamp;
    let info = l.token_count()?;
    if info.total_token_usage.total_tokens <= file.total {
        return None;
    }
    file.total = info.total_token_usage.total_tokens;
    let last = info.last_token_usage;
    let cached = last.cached();
    Some(Reply {
        id: None,
        at: at?,
        tool: CODEX,
        model: if file.model.is_empty() { "codex".into() } else { file.model.clone() },
        project: project_of(file.cwd.as_deref()),
        input: last.input_tokens - cached,
        output: last.output_tokens,
        cache_read: cached,
        cache_write: last.cache_write_input_tokens,
        cost: None,
        long_writes: 0,
    })
}

/* ---------- Command Code ---------- */

/// A line of a Command Code transcript with its `usage` (camelCase, its own cost in dollars).
/// Its folder is the project folder's name, `users-me-work-shop` for `/Users/me/work/shop`.
pub fn command_code(line: &[u8], project: &str) -> Option<Reply> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Usage {
        #[serde(default)]
        input_tokens: u64,
        #[serde(default)]
        output_tokens: u64,
        #[serde(default)]
        cache_read_tokens: u64,
        #[serde(default)]
        cache_write_tokens: u64,
        cost_usd: Option<f64>,
    }
    #[derive(Deserialize)]
    struct Line {
        id: Option<String>,
        timestamp: Option<DateTime<Utc>>,
        model: Option<String>,
        usage: Option<Usage>,
    }
    if !contains(line, b"\"usage\"") {
        return None;
    }
    let l: Line = serde_json::from_slice(line).ok()?;
    let u = l.usage?;
    Some(Reply {
        id: l.id,
        at: l.timestamp?,
        tool: COMMAND_CODE,
        model: l.model.unwrap_or_else(|| "command-code".into()),
        project: project.to_string(),
        input: u.input_tokens,
        output: u.output_tokens,
        cache_read: u.cache_read_tokens,
        cache_write: u.cache_write_tokens,
        cost: u.cost_usd,
        long_writes: 0,
    })
}

/// A Command Code project folder's name back to the folder's own: its path lowercased, `/` and
/// any other sign as `-`, found again on disk segment by segment; failing that, what follows the
/// home folder's part.
pub fn command_code_project(encoded: &str, root: &Path, cache: &mut HashMap<String, String>) -> String {
    if let Some(known) = cache.get(encoded) {
        return known.clone();
    }
    let norm = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect::<String>();
    fn walk(dir: &Path, rest: &str, norm: &dyn Fn(&str) -> String, depth: usize) -> Option<PathBuf> {
        if rest.is_empty() {
            return Some(dir.to_path_buf());
        }
        if depth == 0 {
            return None;
        }
        let mut entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).collect();
        // The longest names first: `poc-synth` before `poc`.
        entries.sort_by_key(|e| std::cmp::Reverse(e.file_name().len()));
        for e in entries {
            let name = norm(&e.file_name().to_string_lossy());
            if let Some(after) = rest.strip_prefix(&name) {
                if after.is_empty() {
                    return Some(e.path());
                }
                if let Some(after) = after.strip_prefix('-')
                    && let Some(found) = walk(&e.path(), after, norm, depth - 1)
                {
                    return Some(found);
                }
            }
        }
        None
    }
    let found = walk(root, encoded, &norm, 12).and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    let fallback = || {
        let home = home().map(|h| norm(&h.to_string_lossy()).trim_start_matches('-').to_string()).unwrap_or_default();
        encoded.strip_prefix(&format!("{home}-")).unwrap_or(encoded).to_string()
    };
    let name = found.unwrap_or_else(fallback);
    cache.insert(encoded.to_string(), name.clone());
    name
}

/* ---------- OpenCode ---------- */

/// OpenCode's assistant messages changed since `since` (milliseconds), completed ones only, and
/// the latest change read: the next read starts there. Read-only: OpenCode keeps writing to it.
pub fn opencode(db: &Path, since: i64) -> anyhow::Result<(Vec<Reply>, i64)> {
    use rusqlite::{Connection, OpenFlags};
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    conn.busy_timeout(std::time::Duration::from_secs(2))?;
    let mut stmt = conn.prepare("SELECT id, time_updated, data FROM message WHERE time_updated > ?1 ORDER BY time_updated")?;
    let mut rows = stmt.query([since])?;
    let (mut out, mut latest) = (Vec::new(), since);
    while let Some(row) = rows.next()? {
        let (id, updated, data): (String, i64, String) = (row.get(0)?, row.get(1)?, row.get(2)?);
        latest = latest.max(updated);
        if let Some(r) = opencode_message(&id, &data) {
            out.push(r);
        }
    }
    Ok((out, latest))
}

/// A message's `data`: an assistant reply once completed, with its tokens and cost.
pub fn opencode_message(id: &str, data: &str) -> Option<Reply> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Data {
        role: String,
        time: Option<Time>,
        model_id: Option<String>,
        path: Option<PathInfo>,
        cost: Option<f64>,
        tokens: Option<Tokens>,
    }
    #[derive(Deserialize)]
    struct Time {
        completed: Option<i64>,
    }
    #[derive(Deserialize)]
    struct PathInfo {
        cwd: Option<String>,
    }
    #[derive(Deserialize, Default)]
    struct Tokens {
        #[serde(default)]
        input: u64,
        #[serde(default)]
        output: u64,
        #[serde(default)]
        reasoning: u64,
        #[serde(default)]
        cache: Cache,
    }
    #[derive(Deserialize, Default)]
    struct Cache {
        #[serde(default)]
        read: u64,
        #[serde(default)]
        write: u64,
    }
    let d: Data = serde_json::from_str(data).ok()?;
    if d.role != "assistant" {
        return None;
    }
    let completed = d.time?.completed?;
    let t = d.tokens.unwrap_or_default();
    Some(Reply {
        id: Some(id.to_string()),
        at: DateTime::from_timestamp_millis(completed)?,
        tool: OPENCODE,
        model: d.model_id.unwrap_or_else(|| "opencode".into()),
        project: project_of(d.path.and_then(|p| p.cwd).as_deref()),
        input: t.input,
        output: t.output + t.reasoning,
        cache_read: t.cache.read,
        cache_write: t.cache.write,
        cost: d.cost,
        long_writes: 0,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::Tail;

    #[test]
    fn tail_gives_complete_lines_and_keeps_only_the_unfinished_one() {
        let path = std::env::temp_dir().join(format!("prism-tail-{}.jsonl", std::process::id()));
        std::fs::write(&path, b"a\nb\nhalf").unwrap();
        let (mut tail, mut got) = (Tail::default(), Vec::new());
        tail.lines(&path, |l| got.push(l.to_vec()));
        assert_eq!(got, [b"a".to_vec(), b"b".to_vec()]);
        let partial = &tail.files[&path].1;
        assert_eq!((partial.as_slice(), partial.capacity()), (b"half".as_slice(), 4));
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b" done\nc").unwrap();
        got.clear();
        tail.lines(&path, |l| got.push(l.to_vec()));
        assert_eq!(got, [b"half done".to_vec()]);
        assert_eq!(tail.files[&path].1, b"c");
        std::fs::remove_file(&path).ok();
    }
}
