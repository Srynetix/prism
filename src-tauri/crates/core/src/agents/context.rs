//! An agent's context window: how much of it the conversation fills. For Claude Code, read by
//! `prism hook` from the end of its transcript (`transcript_path`): only the usage figures of the
//! last reply and the model asked for, never the text. For Command Code, its mod logs the tokens
//! of each request and the model; the window of the default model comes from `cmd status --json`
//! (see `command_code_window`), its mods are not told it. For OpenCode, its plugin logs them the
//! same way; the window comes from the catalog of models OpenCode keeps (`opencode_windows`).
//! For Codex, read like Claude Code's from its transcript (its rollout): the `token_count` it
//! writes after each request carries both the tokens and the window.

use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use serde::{Deserialize, Serialize};

use super::transcript::{ClaudeRecord, CodexLine};

/// Size read at the end of the transcript to find the last reply.
const TAIL_BYTES: u64 = 256 * 1024;
/// Claude's context window, and the one of a model asked for with `[1m]` (`claude-opus-5-5[1m]`).
const WINDOW: u64 = 200_000;
const WINDOW_1M: u64 = 1_000_000;

/// The context of the last request: tokens sent (cached or not) out of the model's window,
/// unknown for a Command Code model other than its default one.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ContextUse {
    pub used: u64,
    #[serde(default)]
    pub size: Option<u64>,
}

/// The context of the transcript's last reply of the main conversation (not a subagent's, nor
/// an error Claude Code writes itself); absent when the end of the file has none.
pub fn read(transcript: &Path) -> Option<ContextUse> {
    let mut f = std::fs::File::open(transcript).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    buf.split(|b| *b == b'\n').rev().find_map(of_line)
}

fn of_line(line: &[u8]) -> Option<ContextUse> {
    if let Ok(c) = serde_json::from_slice::<CodexLine>(line) {
        let info = c.token_count()?;
        let used = info.last_token_usage.input_tokens;
        return (used > 0).then_some(ContextUse { used, size: info.model_context_window });
    }
    let r: ClaudeRecord = serde_json::from_slice(line).ok()?;
    if r.is_sidechain {
        return None;
    }
    let requested = r.requested_model.clone();
    let reply = r.reply()?;
    let used = reply.usage?.context();
    if used == 0 {
        return None;
    }
    // A 1M window when asked for (`[1m]`), when it is the model's own (claude-opus-5-5, asked for
    // without the suffix), and for any context beyond 200k, whatever the model's name.
    let long = used > WINDOW
        || requested.as_deref().is_some_and(|m| m.ends_with("[1m]"))
        || requested.as_deref().or(reply.model.as_deref()).is_some_and(native_1m);
    Some(ContextUse { used, size: Some(if long { WINDOW_1M } else { WINDOW }) })
}

/// Whether the model's own window is 1M: Opus and Sonnet from 4.6, Haiku from 5, Fable and
/// Mythos. Older models (`claude-haiku-4-5`, `claude-opus-4-5-20251101`, `claude-3-…`) and
/// unknown names keep 200k.
fn native_1m(model: &str) -> bool {
    let model = model.strip_suffix("[1m]").unwrap_or(model);
    let mut parts = model.strip_prefix("claude-").unwrap_or(model).split('-');
    let family = parts.next().unwrap_or_default();
    let major: u32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    // A dated snapshot (`-20251101`) has no minor version.
    let minor: u32 = parts.next().filter(|v| v.len() < 8).and_then(|v| v.parse().ok()).unwrap_or(0);
    match family {
        "fable" | "mythos" => true,
        "opus" | "sonnet" => (major, minor) >= (4, 6),
        "haiku" => major >= 5,
        _ => false,
    }
}

/// The model Command Code runs by default, and its window: `cmd status --json`.
pub async fn command_code_window(program: &str) -> Option<(String, u64)> {
    #[derive(Deserialize)]
    struct Status {
        model: String,
        context_window: u64,
    }
    let out = crate::platform::command(program).args(["status", "--json"]).stdin(std::process::Stdio::null()).kill_on_drop(true).output().await.ok()?;
    let s: Status = serde_json::from_slice(&out.stdout).ok().filter(|_| out.status.success())?;
    Some((s.model, s.context_window))
}

/// The windows of the models OpenCode knows, by `<provider>/<model>`: the catalog it caches
/// (`~/.cache/opencode/models.json`, `{provider: {models: {model: {limit: {context}}}}}`). The
/// models of a provider configured by hand are not in it.
pub fn opencode_windows() -> Option<std::collections::HashMap<String, u64>> {
    let cache = std::env::var_os("XDG_CACHE_HOME").map(std::path::PathBuf::from).filter(|p| p.is_absolute()).or_else(|| dirs::home_dir().map(|h| h.join(".cache")))?;
    let raw = std::fs::read(cache.join("opencode").join("models.json")).ok()?;
    Some(parse_opencode_models(&serde_json::from_slice(&raw).ok()?))
}

fn parse_opencode_models(catalog: &serde_json::Value) -> std::collections::HashMap<String, u64> {
    let mut windows = std::collections::HashMap::new();
    for (provider, p) in catalog.as_object().into_iter().flatten() {
        for (model, m) in p.get("models").and_then(|m| m.as_object()).into_iter().flatten() {
            if let Some(size) = m.pointer("/limit/context").and_then(|c| c.as_u64()).filter(|c| *c > 0) {
                windows.insert(format!("{provider}/{model}"), size);
            }
        }
    }
    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPLY: &str = r#"{"type":"assistant","isSidechain":false,"requestedModel":"claude-opus-5-5[1m]","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"secret"}],"usage":{"input_tokens":2,"cache_creation_input_tokens":892,"cache_read_input_tokens":311479,"output_tokens":137}}}"#;

    #[test]
    fn reads_the_last_reply_of_the_conversation() {
        assert_eq!(of_line(REPLY.as_bytes()), Some(ContextUse { used: 312_373, size: Some(1_000_000) }));
        let plain = REPLY.replace("[1m]", "");
        assert_eq!(of_line(plain.as_bytes()).and_then(|c| c.size), Some(1_000_000), "beyond 200k, a 1M window even unasked");
        let small = plain.replace("311479", "99000");
        assert_eq!(of_line(small.as_bytes()).and_then(|c| c.size), Some(1_000_000), "claude-opus-5-5's own window is 1M");
        let old = small.replace("claude-opus-5-5", "claude-haiku-4-5");
        assert_eq!(of_line(old.as_bytes()).and_then(|c| c.size), Some(200_000));
        let older = REPLY.replace(r#""requestedModel":"claude-opus-5-5[1m]","#, "");
        assert_eq!(of_line(older.as_bytes()).and_then(|c| c.size), Some(1_000_000), "beyond 200k without requestedModel: 1M");
        assert_eq!(of_line(REPLY.replace(r#""isSidechain":false"#, r#""isSidechain":true"#).as_bytes()), None);
        assert_eq!(of_line(REPLY.replace(r#""model":"claude-opus-5-5""#, r#""model":"<synthetic>""#).as_bytes()), None);
    }

    #[test]
    fn knows_the_models_with_a_1m_window() {
        for m in ["claude-opus-5-5", "claude-opus-4-6", "claude-opus-4-8", "claude-sonnet-4-6", "claude-sonnet-5", "claude-haiku-5-5", "claude-fable-5-1", "claude-mythos-5"] {
            assert!(native_1m(m), "{m}");
        }
        for m in ["claude-haiku-4-5", "claude-opus-4-5-20251101", "claude-sonnet-4-20250514", "claude-opus-4-1", "claude-3-7-sonnet-20250219", "gpt-6.1-sol", ""] {
            assert!(!native_1m(m), "{m}");
        }
    }

    /// Codex: its last `token_count`, the rest of its rollout ignored.
    #[test]
    fn reads_codex_rollouts() {
        let path = std::env::temp_dir().join(format!("prism-codex-rollout-{}.jsonl", std::process::id()));
        let lines = [
            r#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":15594,"cached_input_tokens":12160,"output_tokens":44},"model_context_window":475000},"rate_limits":{}}}"#,
            r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#,
            r#"{"type":"response_item","payload":{"type":"message","content":"secret"}}"#,
            r#"{"type":"event_msg","payload":{"type":"task_complete"}}"#,
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        assert_eq!(read(&path), Some(ContextUse { used: 15594, size: Some(475_000) }));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reads_opencode_windows_from_its_catalog() {
        let catalog = serde_json::json!({
            "anthropic": { "id": "anthropic", "models": { "claude-sonnet-5-5": { "limit": { "context": 1_000_000, "output": 64_000 } } } },
            "deepinfra": { "models": { "tencent/Hy3": { "limit": { "context": 262_144 } }, "nolimit": {} } },
        });
        let windows = parse_opencode_models(&catalog);
        assert_eq!(windows.get("anthropic/claude-sonnet-5-5"), Some(&1_000_000));
        assert_eq!(windows.get("deepinfra/tencent/Hy3"), Some(&262_144));
        assert_eq!(windows.len(), 2);
    }

    #[test]
    fn skips_what_follows_the_reply() {
        let path = std::env::temp_dir().join(format!("prism-context-{}.jsonl", std::process::id()));
        let tool = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"x"}]}}"#;
        std::fs::write(&path, format!("{REPLY}\n{tool}\n{{\"type\":\"system\",\"subtype\":\"turn_duration\"}}\n")).unwrap();
        assert_eq!(read(&path).map(|c| c.used), Some(312_373));
        std::fs::write(&path, format!("{tool}\n")).unwrap();
        assert_eq!(read(&path), None);
        let _ = std::fs::remove_file(path);
    }
}
