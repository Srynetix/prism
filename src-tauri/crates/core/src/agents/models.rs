//! The models an agent CLI offers, for Settings' model field (`[agents.<cli>] model`): Claude
//! Code's aliases and current models (it keeps no list), Codex's catalog (`models_cache.json`,
//! those it lists), Command Code's `cmd --list-models`, OpenCode's cached catalog. Their names
//! only; any other name may be typed.

use super::{context, provider::Provider};

/// Claude Code's aliases (`opus` follows the latest Opus), then its models by name.
const CLAUDE: [&str; 9] = ["opus", "sonnet", "haiku", "opusplan", "opus[1m]", "claude-fable-5-1", "claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"];

/// A name a command line may carry after `--model`: no option, no space.
pub fn valid(model: &str) -> bool {
    !model.is_empty() && !model.starts_with('-') && model.len() <= 200 && !model.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// A reasoning effort a command line may carry: a level's name (`low`, `xhigh`…), nothing else.
pub fn valid_effort(effort: &str) -> bool {
    !effort.is_empty() && effort.len() <= 20 && effort.chars().all(|c| c.is_ascii_lowercase())
}

/// The reasoning efforts `provider` takes, lowest first, as the CLI tells them: Claude Code's
/// `--help` (`--effort <level> … (low, medium, high, xhigh, max)`), the levels Codex's catalog
/// gives `model` (failing that, its first listed model). Empty for another CLI, or unread.
pub async fn efforts(provider: Provider, program: &str, model: Option<&str>) -> Vec<String> {
    match provider {
        Provider::Claude => {
            let run = crate::platform::command(program).arg("--help").stdin(std::process::Stdio::null()).kill_on_drop(true).output();
            match tokio::time::timeout(std::time::Duration::from_secs(10), run).await {
                Ok(Ok(out)) => parse_claude_efforts(&String::from_utf8_lossy(&out.stdout)),
                _ => Vec::new(),
            }
        }
        Provider::Codex => {
            let model = model.map(String::from);
            tokio::task::spawn_blocking(move || {
                let root = std::env::var_os("CODEX_HOME").map(std::path::PathBuf::from).or_else(|| dirs::home_dir().map(|h| h.join(".codex")));
                let raw = root.and_then(|r| std::fs::read(r.join("models_cache.json")).ok()).unwrap_or_default();
                parse_codex_efforts(&raw, model.as_deref())
            })
            .await
            .unwrap_or_default()
        }
        Provider::CommandCode | Provider::OpenCode => Vec::new(),
    }
}

/// The levels in parentheses of `--effort`'s help, which may wrap onto the next lines.
fn parse_claude_efforts(help: &str) -> Vec<String> {
    let Some(at) = help.find("--effort") else { return Vec::new() };
    let rest = &help[at..];
    let Some(open) = rest.find('(').filter(|o| *o < 400) else { return Vec::new() };
    let Some(close) = rest[open..].find(')') else { return Vec::new() };
    rest[open + 1..open + close].split(',').map(|e| e.split_whitespace().collect::<String>()).filter(|e| valid_effort(e)).collect()
}

fn parse_codex_efforts(raw: &[u8], model: Option<&str>) -> Vec<String> {
    let catalog: serde_json::Value = serde_json::from_slice(raw).unwrap_or_default();
    let models: Vec<&serde_json::Value> = catalog.get("models").and_then(|m| m.as_array()).into_iter().flatten().collect();
    let slug = |m: &serde_json::Value| m.get("slug").and_then(|s| s.as_str()).map(String::from);
    let found = model.and_then(|name| models.iter().find(|m| slug(m).as_deref() == Some(name))).or_else(|| models.iter().find(|m| m.get("visibility").and_then(|v| v.as_str()) != Some("hide")));
    let levels = found.and_then(|m| m.get("supported_reasoning_levels")).and_then(|l| l.as_array()).into_iter().flatten();
    levels.filter_map(|l| l.get("effort").and_then(|e| e.as_str())).filter(|e| valid_effort(e)).map(String::from).collect()
}

/// The models `provider` offers; `program`, its executable (Command Code is asked). Empty when
/// its list cannot be read (not installed, no catalog yet).
pub async fn list(provider: Provider, program: &str) -> Vec<String> {
    match provider {
        Provider::Claude => CLAUDE.iter().map(|m| m.to_string()).collect(),
        Provider::Codex => tokio::task::spawn_blocking(codex).await.unwrap_or_default(),
        Provider::CommandCode => command_code(program).await,
        Provider::OpenCode => tokio::task::spawn_blocking(|| {
            let mut models: Vec<String> = context::opencode_windows().unwrap_or_default().into_keys().collect();
            models.sort();
            models
        })
        .await
        .unwrap_or_default(),
    }
}

/// Codex's catalog, in its order: the models it lists (not those it hides).
fn codex() -> Vec<String> {
    let root = std::env::var_os("CODEX_HOME").map(std::path::PathBuf::from).or_else(|| dirs::home_dir().map(|h| h.join(".codex")));
    let Some(raw) = root.and_then(|r| std::fs::read(r.join("models_cache.json")).ok()) else { return Vec::new() };
    parse_codex(&raw)
}

fn parse_codex(raw: &[u8]) -> Vec<String> {
    let catalog: serde_json::Value = serde_json::from_slice(raw).unwrap_or_default();
    let models = catalog.get("models").and_then(|m| m.as_array()).into_iter().flatten();
    models.filter(|m| m.get("visibility").and_then(|v| v.as_str()) != Some("hide")).filter_map(|m| m.get("slug").and_then(|s| s.as_str())).map(String::from).collect()
}

async fn command_code(program: &str) -> Vec<String> {
    let run = crate::platform::command(program).arg("--list-models").stdin(std::process::Stdio::null()).kill_on_drop(true).output();
    match tokio::time::timeout(std::time::Duration::from_secs(20), run).await {
        Ok(Ok(out)) if out.status.success() => parse_command_code(&String::from_utf8_lossy(&out.stdout)),
        _ => Vec::new(),
    }
}

/// `cmd --list-models`: a model per line, its name then its description after two spaces or
/// more; the headings (`Open Source`) have one.
fn parse_command_code(out: &str) -> Vec<String> {
    out.lines()
        .map(strip_ansi)
        .filter_map(|l| {
            let (name, rest) = l.trim_start().split_once("  ")?;
            (!rest.trim().is_empty() && valid(name) && !name.contains('·')).then(|| name.to_string())
        })
        .collect()
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // `ESC [ … letter`.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CliConfig;

    #[test]
    fn reads_codex_and_command_code_lists() {
        let codex = br#"{"models":[{"slug":"gpt-6.1-sol","visibility":"list"},{"slug":"gpt-reserve","visibility":"hide"},{"slug":"gpt-6-luna","visibility":"list"}]}"#;
        assert_eq!(parse_codex(codex), ["gpt-6.1-sol", "gpt-6-luna"]);
        let cmd = "Available models  ·  87 models\n\nOpen Source\n\n\u{1b}[1mdeepseek/deepseek-v4-pro\u{1b}[0m               hybrid reasoning\nmoonshotai/kimi-k3                     long-horizon coding (default)\n";
        assert_eq!(parse_command_code(cmd), ["deepseek/deepseek-v4-pro", "moonshotai/kimi-k3"]);
    }

    #[test]
    fn reads_the_efforts_each_cli_takes() {
        let help = "  --effort <level>        Effort level for the current session\n                          (low, medium, high, xhigh,\n                          max)\n  --model <model>";
        assert_eq!(parse_claude_efforts(help), ["low", "medium", "high", "xhigh", "max"]);
        assert!(parse_claude_efforts("  --model <model>  (opus)").is_empty());
        let codex = br#"{"models":[{"slug":"a","visibility":"list","supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}]},{"slug":"b","supported_reasoning_levels":[{"effort":"minimal"},{"effort":"xhigh"}]}]}"#;
        assert_eq!(parse_codex_efforts(codex, Some("b")), ["minimal", "xhigh"]);
        assert_eq!(parse_codex_efforts(codex, None), ["low", "high"]);
        assert_eq!(parse_codex_efforts(codex, Some("unknown")), ["low", "high"]);
    }

    #[test]
    fn puts_the_model_on_the_command_line() {
        let cli = CliConfig { model: Some("gpt-6.1-sol".into()), args: vec!["--verbose".into()], ..Default::default() };
        let argv = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(Provider::Claude.with_cli(argv(&["claude", "--settings", "x"]), &cli), argv(&["claude", "--model", "gpt-6.1-sol", "--verbose", "--settings", "x"]));
        assert_eq!(Provider::Codex.with_cli(argv(&["codex", "resume", "--last"]), &cli), argv(&["codex", "resume", "--model", "gpt-6.1-sol", "--verbose", "--last"]));
        // The effort, in each CLI's own way; none for a CLI that has none, nor a level that is not one.
        let effort = |e: &str| CliConfig { effort: Some(e.into()), ..Default::default() };
        assert_eq!(Provider::Claude.with_cli(argv(&["claude"]), &effort("xhigh")), argv(&["claude", "--effort", "xhigh"]));
        assert_eq!(Provider::Codex.with_cli(argv(&["codex", "resume", "--last"]), &effort("high")), argv(&["codex", "resume", "-c", "model_reasoning_effort=\"high\"", "--last"]));
        assert_eq!(Provider::OpenCode.with_cli(argv(&["opencode"]), &effort("high")), argv(&["opencode"]));
        assert_eq!(Provider::Claude.with_cli(argv(&["claude"]), &effort("--danger")), argv(&["claude"]));
        // Not a model: left out.
        let odd = CliConfig { model: Some("--dangerously-skip-permissions".into()), ..Default::default() };
        assert_eq!(Provider::Claude.with_cli(argv(&["claude"]), &odd), argv(&["claude"]));
        let blank = CliConfig { model: Some("  ".into()), ..Default::default() };
        assert_eq!(Provider::Claude.with_cli(argv(&["claude"]), &blank), argv(&["claude"]));
    }
}
