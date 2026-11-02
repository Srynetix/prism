//! What an agent's conversation consumed (Claude Code, Codex): the usage figures of every reply of
//! its transcripts (`message.usage`: input, output, cache writes and reads; Codex's rollout, the
//! `token_count` after each request), summed, never their text.
//! Read incrementally, as the transcript grows; a reply split over several records (one per
//! content block) counts once, by its message id. Its cost is estimated at the model's public API
//! price: on a subscription, a figure of comparison rather than a bill.

use std::{
    collections::{HashMap, HashSet},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use serde::Serialize;

use chrono::{DateTime, Utc};

use super::transcript::{ClaudeRecord, CodexLine, claude_interruption};

/// Replies kept for the per-reply bars.
const RECENT: usize = 40;

/// An agent's consumption, summed over its transcripts.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct AgentUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub replies: u32,
    /// Estimated at the models' API prices, in dollars; absent when a model's price is unknown.
    pub cost: Option<f64>,
    /// The model of the last reply.
    pub model: Option<String>,
    /// The tokens of each of the last replies (input of all kinds and output), oldest first.
    pub recent: Vec<u64>,
}

impl AgentUsage {
    /// Input of all kinds and output.
    pub fn tokens(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }

    /// `other`'s figures added (another stretch of the agent's).
    pub fn add(&mut self, other: &AgentUsage) {
        (self.input, self.output, self.cache_read, self.cache_write) = (self.input + other.input, self.output + other.output, self.cache_read + other.cache_read, self.cache_write + other.cache_write);
        self.replies += other.replies;
        self.cost = match (self.cost, other.cost) {
            (Some(a), Some(b)) => Some(a + b),
            (a, b) => a.or(b),
        };
    }

    /// What is left once `before`'s figures are taken out: a session resumed counts again what it
    /// held then. Its model and recent replies stay.
    pub fn minus(&self, before: &AgentUsage) -> AgentUsage {
        AgentUsage {
            input: self.input.saturating_sub(before.input),
            output: self.output.saturating_sub(before.output),
            cache_read: self.cache_read.saturating_sub(before.cache_read),
            cache_write: self.cache_write.saturating_sub(before.cache_write),
            replies: self.replies.saturating_sub(before.replies),
            cost: self.cost.map(|c| (c - before.cost.unwrap_or(0.0)).max(0.0)),
            model: self.model.clone(),
            recent: self.recent.clone(),
        }
    }
}

/// The transcripts read so far, and what they summed to.
#[derive(Debug, Default)]
pub struct UsageFold {
    files: HashMap<PathBuf, (u64, Vec<u8>)>,
    seen: HashSet<String>,
    pub usage: AgentUsage,
    /// The transcript being read (the conversation's current one).
    pub located: Option<PathBuf>,
    /// When the transcript was last looked for, not found: searched again only after a while.
    pub searched: Option<std::time::Instant>,
    /// A reply of a model without a known price: the cost is no longer complete.
    unpriced: bool,
    /// Codex's rollout: the model of its last `turn_context`, and the session's running total, to
    /// count each `token_count` once (it is sent again unchanged).
    codex_model: String,
    codex_total: u64,
    /// Claude Code: when its last turn was interrupted (Esc), which its hooks do not say.
    pub interrupted: Option<DateTime<Utc>>,
}

impl UsageFold {
    /// Reads what was added to `path` since last time.
    pub fn read(&mut self, path: &Path) {
        let Ok(mut f) = std::fs::File::open(path) else { return };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let (offset, partial) = self.files.entry(path.to_path_buf()).or_default();
        if len <= *offset || f.seek(SeekFrom::Start(*offset)).is_err() {
            return;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return;
        }
        *offset += buf.len() as u64;
        // Only the unfinished last line is kept: a buffer drained in place kept its capacity, the
        // size of the whole transcript at its first read, for as long as the agent was listed.
        let mut data = std::mem::take(partial);
        data.append(&mut buf);
        let Some(last) = data.iter().rposition(|b| *b == b'\n') else {
            *partial = data;
            return;
        };
        *partial = data[last + 1..].to_vec();
        for line in data[..last].split(|b| *b == b'\n') {
            self.add(line);
        }
    }

    fn add(&mut self, line: &[u8]) {
        if let Ok(c) = serde_json::from_slice::<CodexLine>(line) {
            self.add_codex(c);
            return;
        }
        if let Some(at) = claude_interruption(line) {
            self.interrupted = Some(at);
            return;
        }
        let Some(msg) = serde_json::from_slice::<ClaudeRecord>(line).ok().and_then(ClaudeRecord::reply) else { return };
        if let Some(id) = msg.id && !self.seen.insert(id) {
            return;
        }
        let Some(u) = msg.usage else { return };
        let long_writes = u.long_writes();
        self.reply(msg.model.unwrap_or_default(), u.input_tokens, u.output_tokens, u.cache_read_input_tokens, u.cache_creation_input_tokens, long_writes);
    }

    /// A line of Codex's rollout: its model from `turn_context`, a reply from each new
    /// `token_count` (`last_token_usage`, whose input counts the cached part).
    fn add_codex(&mut self, c: CodexLine) {
        if c.kind == "turn_context" {
            if let Some(m) = c.payload.model {
                self.codex_model = m;
            }
            return;
        }
        let Some(info) = c.token_count() else { return };
        if info.total_token_usage.total_tokens <= self.codex_total {
            return;
        }
        self.codex_total = info.total_token_usage.total_tokens;
        let u = info.last_token_usage;
        let cached = u.cached();
        let model = if self.codex_model.is_empty() { "codex".to_string() } else { self.codex_model.clone() };
        self.reply(model, u.input_tokens - cached, u.output_tokens, cached, u.cache_write_input_tokens, 0);
    }

    fn reply(&mut self, model: String, input: u64, output: u64, cache_read: u64, cache_write: u64, long_writes: u64) {
        let s = &mut self.usage;
        s.input += input;
        s.output += output;
        s.cache_read += cache_read;
        s.cache_write += cache_write;
        s.replies += 1;
        s.recent.push(input + output + cache_read + cache_write);
        if s.recent.len() > RECENT {
            s.recent.remove(0);
        }
        match cost(&model, input, output, cache_read, cache_write, long_writes) {
            Some(cost) => {
                if !self.unpriced {
                    s.cost = Some(s.cost.unwrap_or(0.0) + cost);
                }
            }
            None => {
                self.unpriced = true;
                s.cost = None;
            }
        }
        s.model = Some(model);
    }
}

/// Dollars per million tokens: input, output, cache read (a cache write costs 1.25× the input,
/// 2× for an hour). The public API prices of October 2026: Anthropic's, and OpenAI's for Codex
/// (standard tier, prompts under 272k tokens; GPT-6 Sol's and Codex's reviewer's not found).
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
}

/// A reply's cost in dollars at the model's API price; `long_writes`: those of the cache writes kept
/// an hour (2× the input rather than 1.25×). `None` for a model without a known price.
pub fn cost(model: &str, input: u64, output: u64, cache_read: u64, cache_write: u64, long_writes: u64) -> Option<f64> {
    let p = price(model)?;
    let m = 1e-6;
    let long_writes = long_writes.min(cache_write);
    Some(
        input as f64 * p.input * m
            + output as f64 * p.output * m
            + cache_read as f64 * p.cache_read * m
            + (cache_write - long_writes) as f64 * p.input * 1.25 * m
            + long_writes as f64 * p.input * 2.0 * m,
    )
}

pub fn price(model: &str) -> Option<Price> {
    // `claude-opus-5-5[1m]`, `claude-sonnet-4-6-20260101`: the family and version only.
    let m = model.trim_start_matches("anthropic.").split('[').next().unwrap_or(model);
    let p = |input: f64, output: f64, cache_read: f64| Some(Price { input, output, cache_read });
    let is = |prefix: &str| m == prefix || m.starts_with(&format!("{prefix}-2"));
    if is("claude-fable-5-1") || is("claude-mythos-5-1") {
        p(10.0, 50.0, 0.25)
    } else if is("claude-fable-5") || is("claude-mythos-5") {
        p(10.0, 50.0, 1.0)
    } else if is("claude-opus-5-5") {
        p(4.0, 20.0, 0.20)
    } else if is("claude-opus-5") || is("claude-opus-4-8") || is("claude-opus-4-7") || is("claude-opus-4-6") {
        p(5.0, 25.0, 0.50)
    } else if is("claude-sonnet-5-5") || is("claude-sonnet-5") {
        p(2.0, 10.0, 0.20)
    } else if is("claude-sonnet-4-6") {
        p(3.0, 15.0, 0.30)
    } else if is("claude-haiku-5-5") {
        p(0.10, 0.50, 0.01)
    } else if is("claude-haiku-4-5") {
        p(1.0, 5.0, 0.10)
    } else if is("gpt-6-astra") {
        p(10.0, 50.0, 1.0)
    } else if is("gpt-6.1-sol") {
        p(2.0, 10.0, 0.10)
    } else if is("gpt-6-luna") {
        p(0.10, 0.50, 0.01)
    } else if is("gpt-5.6-sol") {
        p(4.0, 20.0, 0.40)
    } else if is("gpt-5.6-terra") {
        p(2.0, 12.0, 0.20)
    } else if is("gpt-5.6-luna") {
        p(0.20, 1.20, 0.02)
    } else if is("gpt-5-mini") || is("gpt-5.1-codex-mini") {
        p(0.25, 2.0, 0.025)
    } else if is("gpt-5") || is("gpt-5-codex") || is("gpt-5.1") || is("gpt-5.1-codex") {
        p(1.25, 10.0, 0.125)
    } else {
        None
    }
}

/// An agent's transcript when its hooks did not name it: `<id>.jsonl` in one of Claude Code's
/// project folders.
pub fn find_transcript(session: &str) -> Option<PathBuf> {
    let projects = dirs::home_dir()?.join(".claude/projects");
    std::fs::read_dir(projects).ok()?.flatten().map(|d| d.path().join(format!("{session}.jsonl"))).find(|p| p.is_file())
}

/// Command Code's transcript of session `session`, in whichever project folder it is.
pub fn find_command_code_transcript(session: &str) -> Option<PathBuf> {
    let projects = dirs::home_dir()?.join(".commandcode/projects");
    std::fs::read_dir(projects).ok()?.flatten().map(|d| d.path().join(format!("{session}.jsonl"))).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    #[test]
    fn stretches_add_and_a_resumed_one_counts_once() {
        let a = super::AgentUsage { input: 100, output: 10, cache_read: 1000, cache_write: 50, replies: 3, cost: Some(1.5), ..Default::default() };
        let b = super::AgentUsage { input: 20, output: 2, replies: 1, cost: Some(0.5), ..Default::default() };
        let mut sum = a.clone();
        sum.add(&b);
        assert_eq!((sum.tokens(), sum.replies, sum.cost), (1182, 4, Some(2.0)));
        // Back on `a`'s session: its transcript holds `a` again, which is its earlier stretch's.
        let resumed = super::AgentUsage { input: 130, output: 15, cache_read: 1000, cache_write: 50, replies: 5, cost: Some(1.9), model: Some("m".into()), ..Default::default() };
        let now = resumed.minus(&a);
        assert_eq!((now.input, now.output, now.cache_read, now.replies, now.model.as_deref()), (30, 5, 0, 2, Some("m")));
        assert!((now.cost.unwrap() - 0.4).abs() < 1e-9);
    }

    use super::*;

    fn reply(id: &str, model: &str, input: u64, output: u64, read: u64, write: u64) -> String {
        serde_json::json!({ "type": "assistant", "message": { "id": id, "model": model, "usage": { "input_tokens": input, "output_tokens": output, "cache_read_input_tokens": read, "cache_creation_input_tokens": write } } }).to_string()
    }

    #[test]
    fn sums_each_reply_once_and_prices_it() {
        let dir = std::env::temp_dir().join(format!("prism-usage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        // A reply written as two records (two content blocks), then a second one.
        let lines = [reply("m1", "claude-opus-5-5", 1_000_000, 0, 0, 0), reply("m1", "claude-opus-5-5", 1_000_000, 0, 0, 0), r#"{"type":"user","message":{"content":"hi"}}"#.into()];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let mut f = UsageFold::default();
        f.read(&path);
        assert_eq!((f.usage.input, f.usage.replies), (1_000_000, 1));
        assert!((f.usage.cost.unwrap() - 4.0).abs() < 1e-9);
        // Appended later: read from where it stopped.
        let more = reply("m2", "claude-opus-5-5", 0, 1_000_000, 1_000_000, 0);
        std::fs::write(&path, lines.join("\n") + "\n" + &more + "\n").unwrap();
        f.read(&path);
        assert_eq!((f.usage.output, f.usage.cache_read, f.usage.replies), (1_000_000, 1_000_000, 2));
        assert!((f.usage.cost.unwrap() - 24.2).abs() < 1e-9);
        // A model without a known price: tokens only.
        let odd = reply("m3", "some-other-model", 10, 10, 0, 0);
        std::fs::write(&path, lines.join("\n") + "\n" + &more + "\n" + &odd + "\n").unwrap();
        f.read(&path);
        assert_eq!(f.usage.cost, None);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Claude Code's interruption marker, alone or after a refused tool; never a tool's output
    /// that happens to quote it, nor a subagent's.
    #[test]
    fn notes_the_last_interruption() {
        let mut f = UsageFold::default();
        let at = "2026-10-21T14:02:56.000Z";
        f.add(serde_json::json!({ "type": "user", "timestamp": at, "message": { "content": [{ "type": "tool_result", "content": "[Request interrupted by user]" }] } }).to_string().as_bytes());
        f.add(serde_json::json!({ "type": "user", "isSidechain": true, "timestamp": at, "message": { "content": "[Request interrupted by user]" } }).to_string().as_bytes());
        assert_eq!(f.interrupted, None);
        f.add(serde_json::json!({ "type": "user", "timestamp": at, "message": { "content": [{ "type": "text", "text": "[Request interrupted by user for tool use]" }] } }).to_string().as_bytes());
        assert_eq!(f.interrupted, Some(at.parse().unwrap()));
    }

    #[test]
    fn prices_by_family() {
        assert_eq!(price("claude-opus-5-5[1m]").unwrap().input, 4.0);
        assert_eq!(price("claude-sonnet-4-6").unwrap().output, 15.0);
        assert_eq!(price("claude-opus-5").unwrap().input, 5.0);
        assert!(price("claude-opus-5-9").is_none());
        assert_eq!(price("gpt-6.1-sol").unwrap().output, 10.0);
        assert_eq!(price("gpt-5-codex").unwrap().input, 1.25);
        assert_eq!(price("gpt-6-astra").unwrap().cache_read, 1.0);
        assert_eq!(price("gpt-5.6-terra").unwrap().output, 12.0);
        assert!(price("gpt-6.1-luna").is_none() && price("gpt-6-sol").is_none());
    }

    /// Codex's rollout: each new `token_count` a reply, its cached input taken out, at its model's price.
    #[test]
    fn reads_a_codex_rollout() {
        let dir = std::env::temp_dir().join(format!("prism-usage-codex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rollout.jsonl");
        let count = |input: u64, cached: u64, output: u64, total: u64| serde_json::json!({ "type": "event_msg", "payload": { "type": "token_count", "info": { "last_token_usage": { "input_tokens": input, "cached_input_tokens": cached, "output_tokens": output, "total_tokens": input + output }, "total_token_usage": { "total_tokens": total } } } }).to_string();
        let lines = [
            r#"{"type":"session_meta","payload":{"cwd":"/x"}}"#.to_string(),
            r#"{"type":"turn_context","payload":{"model":"gpt-6.1-sol","cwd":"/x"}}"#.to_string(),
            count(1_000_000, 500_000, 100_000, 1_100_000),
            // Sent again unchanged: once.
            count(1_000_000, 500_000, 100_000, 1_100_000),
            r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#.to_string(),
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let mut f = UsageFold::default();
        f.read(&path);
        let u = &f.usage;
        assert_eq!((u.input, u.cache_read, u.output, u.replies, u.model.as_deref()), (500_000, 500_000, 100_000, 1, Some("gpt-6.1-sol")));
        // 0.5M × $2 + 0.5M × $0.10 + 0.1M × $10.
        assert!((u.cost.unwrap() - 2.05).abs() < 1e-9);
        let _ = std::fs::remove_dir_all(dir);
    }
}
