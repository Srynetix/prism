//! The lines of the agents' transcripts that carry tokens, read the same way by everything that
//! counts them (an agent's usage and context gauge, the Tokens block, its Chat's turn costs):
//! Claude Code's replies (`message.usage`) and Codex's rollout (`turn_context`, `token_count`),
//! never their text — but for the marker Claude Code writes itself when a turn is interrupted.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/* ---------- Claude Code ---------- */

/// A record of a Claude Code transcript, as far as its tokens go.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeRecord {
    #[serde(rename = "type")]
    pub kind: String,
    /// A subagent's, in the conversation's own file.
    #[serde(default)]
    pub is_sidechain: bool,
    /// The model asked for (`claude-opus-5-5[1m]`), which says the window.
    pub requested_model: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub cwd: Option<String>,
    pub message: Option<ClaudeMessage>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeMessage {
    /// A reply split over several records (one per content block) has one id: it counts once.
    pub id: Option<String>,
    pub model: Option<String>,
    pub usage: Option<ClaudeUsage>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ClaudeUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation: Option<CacheCreation>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CacheCreation {
    #[serde(default)]
    pub ephemeral_1h_input_tokens: u64,
}

/// The marker Claude Code writes in the conversation when its turn is interrupted (Esc), alone or
/// followed by ` for tool use]`.
const INTERRUPTED: &str = "[Request interrupted by user";

/// When the turn was interrupted, if the record says so: the user's text (not a tool's result) is
/// Claude Code's marker. Claude Code tells its hooks nothing then, only its transcript. Lines
/// without the marker are not parsed.
pub fn claude_interruption(line: &[u8]) -> Option<DateTime<Utc>> {
    if !std::str::from_utf8(line).is_ok_and(|s| s.contains(INTERRUPTED)) {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(line).ok()?;
    if v["type"] != "user" || v["isSidechain"] == true {
        return None;
    }
    let marker = |t: &serde_json::Value| t.as_str().is_some_and(|t| t.trim_start().starts_with(INTERRUPTED));
    let content = &v["message"]["content"];
    let marked = marker(content) || content.as_array().is_some_and(|blocks| blocks.iter().any(|b| b["type"] == "text" && marker(&b["text"])));
    marked.then(|| v["timestamp"].as_str()?.parse().ok()).flatten()
}

impl ClaudeRecord {
    /// The reply it is: an assistant record with its message, not one Claude Code writes itself
    /// (`<synthetic>`, an error).
    pub fn reply(self) -> Option<ClaudeMessage> {
        let msg = self.message.filter(|_| self.kind == "assistant")?;
        (msg.model.as_deref() != Some("<synthetic>")).then_some(msg)
    }
}

impl ClaudeUsage {
    /// Cache writes kept an hour, priced higher: among the cache writes.
    pub fn long_writes(&self) -> u64 {
        self.cache_creation.as_ref().map_or(0, |c| c.ephemeral_1h_input_tokens).min(self.cache_creation_input_tokens)
    }

    /// The tokens sent with the request, cached or not: the context it filled.
    pub fn context(&self) -> u64 {
        self.input_tokens + self.cache_creation_input_tokens + self.cache_read_input_tokens
    }

    /// Every token of the reply, input of all kinds and output.
    pub fn total(&self) -> u64 {
        self.context() + self.output_tokens
    }

    /// Its cost at `model`'s API price; none for a model without a known price.
    pub fn cost(&self, model: &str) -> Option<f64> {
        super::usage::cost(model, self.input_tokens, self.output_tokens, self.cache_read_input_tokens, self.cache_creation_input_tokens, self.long_writes())
    }
}

/* ---------- Codex ---------- */

/// A line of Codex's rollout that tells its folder and model (`session_meta`, `turn_context`) or a
/// request's tokens (`event_msg` of type `token_count`). Claude Code's records have no `payload`.
#[derive(Debug, Deserialize)]
pub struct CodexLine {
    #[serde(rename = "type")]
    pub kind: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub payload: CodexPayload,
}

#[derive(Debug, Deserialize)]
pub struct CodexPayload {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub info: Option<CodexInfo>,
}

#[derive(Debug, Deserialize)]
pub struct CodexInfo {
    /// The last request's: its input counts the cached part.
    pub last_token_usage: CodexUsage,
    /// The session's running total: a `token_count` sent again unchanged counts once.
    #[serde(default)]
    pub total_token_usage: CodexUsage,
    pub model_context_window: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CodexUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub cache_write_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

impl CodexLine {
    /// The request's tokens, if it is a `token_count` that has them.
    pub fn token_count(self) -> Option<CodexInfo> {
        (self.kind == "event_msg" && self.payload.kind.as_deref() == Some("token_count")).then_some(self.payload.info).flatten()
    }
}

impl CodexUsage {
    /// The cached part of the input.
    pub fn cached(&self) -> u64 {
        self.cached_input_tokens.min(self.input_tokens)
    }

    /// Its cost at `model`'s API price, the cached input as cache reads.
    pub fn cost(&self, model: &str) -> Option<f64> {
        let cached = self.cached();
        super::usage::cost(model, self.input_tokens - cached, self.output_tokens, cached, self.cache_write_input_tokens, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reply's figures; Claude Code's own records and Codex's lines are not replies.
    #[test]
    fn reads_the_lines_that_carry_tokens() {
        let r: ClaudeRecord = serde_json::from_str(r#"{"type":"assistant","isSidechain":false,"message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":100,"cache_creation_input_tokens":20,"cache_creation":{"ephemeral_1h_input_tokens":50}}}}"#).unwrap();
        let u = r.reply().unwrap().usage.unwrap();
        assert_eq!((u.context(), u.total(), u.long_writes()), (130, 135, 20), "an hour's writes among the writes");
        let synthetic: ClaudeRecord = serde_json::from_str(r#"{"type":"assistant","message":{"model":"<synthetic>"}}"#).unwrap();
        assert!(synthetic.reply().is_none());
        assert!(serde_json::from_str::<CodexLine>(r#"{"type":"assistant","message":{}}"#).is_err(), "no payload: Claude Code's");

        let c: CodexLine = serde_json::from_str(r#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1000,"cached_input_tokens":2000,"output_tokens":7},"model_context_window":272000}}}"#).unwrap();
        let info = c.token_count().unwrap();
        assert_eq!((info.last_token_usage.cached(), info.total_token_usage.total_tokens, info.model_context_window), (1000, 0, Some(272_000)));
        let empty: CodexLine = serde_json::from_str(r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#).unwrap();
        assert!(empty.token_count().is_none());
    }
}
