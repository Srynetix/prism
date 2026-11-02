//! Subagents' own transcripts (`<session>/subagents/agent-*.jsonl`): each one's calls and last
//! words, folded into the call that launched it.

use super::*;

/// Each subagent's calls and last words, from its own transcript (Claude Code keeps them in
/// `<session>/subagents/agent-*.jsonl`), found by the prompt it was given; a forked skill's, by its
/// agent's id.
pub(super) fn subagents(transcript: &Path, turns: &mut [Turn], names: &Names, background: &[Background]) {
    let mut wanted: Vec<&mut Tool> = turns
        .iter_mut()
        .flat_map(|t| t.entries.iter_mut())
        .filter_map(|e| match e {
            Entry::Tool(t) if t.kind == "agent" && (t.prompt.is_some() || t.forked.is_some()) => Some(t),
            _ => None,
        })
        .collect();
    if wanted.is_empty() {
        return;
    }
    let Ok(dir) = std::fs::read_dir(transcript.with_extension("").join("subagents")) else { return };
    let mut subs = SUBS.lock().unwrap();
    if subs.len() > SUBS_KEPT {
        subs.clear();
    }
    for path in dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")) {
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let seen = subs.entry(path.clone()).or_insert_with(|| SubRead { prompt: first_prompt(&path), call: meta_call(&path), fold: SubFold::default() });
        let by_call = |t: &&mut Tool| seen.call.is_some() && t.call == seen.call;
        let by_prompt = |t: &&mut Tool| seen.prompt.as_deref().is_some_and(|p| t.prompt.as_deref().map(str::trim) == Some(p.trim()));
        let stem = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.strip_prefix("agent-"));
        let by_id = |t: &&mut Tool| stem.is_some() && t.forked.as_deref() == stem;
        let free = |t: &&mut Tool| t.sub.is_none();
        let Some(n) = wanted
            .iter()
            .position(|t| free(t) && by_id(t))
            .or_else(|| wanted.iter().position(|t| free(t) && by_call(t)))
            .or_else(|| wanted.iter().position(|t| free(t) && t.forked.is_none() && by_prompt(t)))
        else {
            continue;
        };
        let tool = &mut wanted[n];
        // Only what it wrote since is read.
        seen.fold.catch_up(&path, len, names);
        let mut sub = seen.fold.sub();
        sub.running = tool.state == "running" || background.iter().any(|b| b.state == "running" && tool.call.as_deref() == Some(b.call.as_str()));
        tool.sub = Some(sub);
    }
}

/// A subagent's transcript read: its prompt (its first line, which never changes), and what it
/// did so far.
pub(super) struct SubRead {
    pub(super) prompt: Option<String>,
    pub(super) call: Option<String>,
    pub(super) fold: SubFold,
}
pub(super) static SUBS: LazyLock<Mutex<HashMap<PathBuf, SubRead>>> = LazyLock::new(Default::default);
/// Beyond, the subagents read are forgotten (each is a few kilobytes).
pub(super) const SUBS_KEPT: usize = 2000;

/// The prompt a subagent was given: its transcript's first line.
pub(super) fn first_prompt(path: &Path) -> Option<String> {
    use std::io::BufRead;
    let mut line = String::new();
    std::io::BufReader::new(std::fs::File::open(path).ok()?).read_line(&mut line).ok()?;
    let r: Value = serde_json::from_str(&line).ok()?;
    Some(match r.pointer("/message/content")? {
        Value::String(s) => s.clone(),
        other => content_text(other),
    })
}

/// The call that launched a subagent, as Claude Code notes it next to its transcript
/// (`agent-<id>.meta.json`, `toolUseId`).
pub(super) fn meta_call(path: &Path) -> Option<String> {
    let meta: Value = serde_json::from_slice(&std::fs::read(path.with_extension("meta.json")).ok()?).ok()?;
    meta.get("toolUseId").and_then(Value::as_str).map(String::from)
}

/// What a subagent did, read as its transcript grows: its last calls (their outcome as it comes),
/// how many, its last words. Its end only the first time (`TAIL_BYTES`), as before.
#[derive(Default)]
pub(super) struct SubFold {
    /// Read up to there; the line being written kept apart.
    pub(super) end: u64,
    pub(super) partial: Vec<u8>,
    /// Its last calls, `first` the number of the oldest one kept; the calls waiting for their
    /// result, by id: their number.
    pub(super) steps: std::collections::VecDeque<SubStep>,
    pub(super) first: usize,
    pub(super) calls: HashMap<String, usize>,
    pub(super) count: u32,
    pub(super) said: Option<String>,
}

impl SubFold {
    pub(super) fn catch_up(&mut self, path: &Path, len: u64, names: &Names) {
        if len < self.end {
            // Written again shorter: afresh.
            *self = Self::default();
        }
        if len == self.end {
            return;
        }
        let Ok(mut f) = std::fs::File::open(path) else { return };
        let fresh = self.end == 0;
        let start = if fresh { len.saturating_sub(TAIL_BYTES) } else { self.end };
        let mut buf = Vec::new();
        if f.seek(SeekFrom::Start(start)).is_err() || f.read_to_end(&mut buf).is_err() {
            return;
        }
        self.end = start + buf.len() as u64;
        let data = if self.partial.is_empty() { buf } else { [std::mem::take(&mut self.partial), buf].concat() };
        let mut lines: Vec<&[u8]> = data.split(|b| *b == b'\n').collect();
        // From the middle of the file, the line it starts in is not whole.
        if fresh && start > 0 && !lines.is_empty() {
            lines.remove(0);
        }
        // The last one without its line break: kept for next time, unless it already reads whole.
        let last = lines.pop().unwrap_or_default();
        for line in lines {
            self.line(line, names);
        }
        if !last.is_empty() && !self.line(last, names) {
            self.partial = last.to_vec();
        }
    }

    /// Folds a line; false if it does not parse.
    pub(super) fn line(&mut self, line: &[u8], names: &Names) -> bool {
        let Ok(r) = serde_json::from_slice::<Value>(line) else { return false };
        for b in r.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
            match (r.get("type").and_then(Value::as_str), b.get("type").and_then(Value::as_str)) {
                (Some("assistant"), Some("text")) => {
                    let text = b.get("text").and_then(Value::as_str).unwrap_or_default().trim();
                    if !text.is_empty() {
                        self.said = Some(cut(text, SUB_CHARS));
                    }
                }
                (Some("assistant"), Some("tool_use")) => {
                    let name = b.get("name").and_then(Value::as_str).unwrap_or("tool");
                    let what = names.what(name, b.get("input").unwrap_or(&Value::Null));
                    if let Some(id) = b.get("id").and_then(Value::as_str) {
                        self.calls.insert(id.to_string(), self.first + self.steps.len());
                    }
                    self.steps.push_back(SubStep { tool: name.into(), what, state: "running" });
                    self.count += 1;
                    if self.steps.len() > SUB_STEPS {
                        self.steps.pop_front();
                        self.first += 1;
                        let first = self.first;
                        self.calls.retain(|_, n| *n >= first);
                    }
                }
                (Some("user"), Some("tool_result")) => {
                    let Some(n) = b.get("tool_use_id").and_then(Value::as_str).and_then(|id| self.calls.remove(id)) else { continue };
                    if let Some(step) = self.steps.get_mut(n - self.first) {
                        step.state = if b.get("is_error").and_then(Value::as_bool) == Some(true) { "failed" } else { "ok" };
                    }
                }
                _ => {}
            }
        }
        true
    }

    pub(super) fn sub(&self) -> Sub {
        Sub { steps: self.steps.iter().cloned().collect(), count: self.count, said: self.said.clone(), running: false }
    }
}
