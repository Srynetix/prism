//! `prism mcp install <cli>`: Prism's MCP server added to an agent CLI's user config, outside
//! Prism, and the "prism" skill copied where that CLI reads its skills; `uninstall` takes both out.
//! Claude Code's config is changed through its own command (`claude mcp add`), the others' files
//! directly (Codex's TOML, OpenCode's JSON).

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::Serialize;

/// The CLIs it installs into.
pub const CLIS: &[&str] = &["claude", "codex", "opencode"];

/// How long Codex waits for a tool's answer: a proposal waits for the user (`host::WAIT` is less).
const CODEX_TOOL_TIMEOUT: i64 = 300;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "McpInstall"))]
pub struct Installed {
    pub cli: String,
    /// Its config names Prism's server.
    pub server: bool,
    /// The skill is in its skills' folder.
    pub skill: bool,
    /// The CLI's config folder exists (it is installed, or was run once).
    pub present: bool,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

fn skill_dir(cli: &str) -> PathBuf {
    match cli {
        "claude" => home().join(".claude/skills/prism"),
        "codex" => home().join(".codex/skills/prism"),
        _ => opencode_dir().join("skill/prism"),
    }
}

fn opencode_dir() -> PathBuf {
    dirs::config_dir().filter(|_| cfg!(not(target_os = "macos"))).unwrap_or_else(|| home().join(".config")).join("opencode")
}

fn codex_config() -> PathBuf {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".codex")).join("config.toml")
}

/// Where each CLI stands.
pub fn status() -> Vec<Installed> {
    CLIS.iter()
        .map(|cli| {
            let (server, present) = match *cli {
                "claude" => {
                    let file = home().join(".claude.json");
                    let has = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).is_some_and(|v| v.pointer(&format!("/mcpServers/{}", prism_core::mcp::SERVER)).is_some());
                    (has, home().join(".claude").is_dir())
                }
                "codex" => {
                    let file = codex_config();
                    let has = std::fs::read_to_string(&file).ok().and_then(|t| t.parse::<toml::Table>().ok()).is_some_and(|t| t.get("mcp_servers").and_then(|m| m.get(prism_core::mcp::SERVER)).is_some());
                    (has, file.parent().is_some_and(|d| d.is_dir()))
                }
                _ => {
                    let has = opencode_file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).is_some_and(|v| v.pointer(&format!("/mcp/{}", prism_core::mcp::SERVER)).is_some());
                    (has, opencode_dir().is_dir())
                }
            };
            Installed { cli: cli.to_string(), server, skill: skill_dir(cli).join("SKILL.md").is_file(), present }
        })
        .collect()
}

fn opencode_file() -> Option<PathBuf> {
    ["opencode.json", "opencode.jsonc"].iter().map(|f| opencode_dir().join(f)).find(|p| p.is_file())
}

/// Adds the server and the skill to `cli`; what was done.
pub fn install(cli: &str) -> Result<String> {
    let command = prism_core::mcp::command();
    let (program, args) = command.split_first().context("command")?;
    match cli {
        "claude" => {
            // Added again: the executable's path may have changed (an update moved it).
            let _ = std::process::Command::new("claude").args(["mcp", "remove", "--scope", "user", prism_core::mcp::SERVER]).output();
            let out = std::process::Command::new("claude").args(["mcp", "add", "--scope", "user", prism_core::mcp::SERVER, "--"]).arg(program).args(args).output().context("claude not found in the PATH")?;
            if !out.status.success() {
                bail!("claude mcp add: {}", String::from_utf8_lossy(&out.stderr).trim());
            }
        }
        "codex" => {
            let file = codex_config();
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let mut doc: toml_edit::DocumentMut = text.parse().with_context(|| format!("{}: not valid TOML", file.display()))?;
            let mut entry = toml_edit::Table::new();
            entry.insert("command", toml_edit::value(program.as_str()));
            entry.insert("args", toml_edit::value(args.iter().map(String::as_str).collect::<toml_edit::Array>()));
            entry.insert("tool_timeout_sec", toml_edit::value(CODEX_TOOL_TIMEOUT));
            let servers = doc.entry("mcp_servers").or_insert_with(|| {
                let mut t = toml_edit::Table::new();
                t.set_implicit(true);
                toml_edit::Item::Table(t)
            });
            servers.as_table_mut().context("mcp_servers: not a table")?.insert(prism_core::mcp::SERVER, toml_edit::Item::Table(entry));
            write(&file, &doc.to_string())?;
        }
        "opencode" => {
            let file = opencode_file().unwrap_or_else(|| opencode_dir().join("opencode.json"));
            let text = std::fs::read_to_string(&file).unwrap_or_else(|_| "{}".into());
            let mut v: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("{}: comments or not JSON; add under \"mcp\": \"prism\": {}", file.display(), prism_core::mcp::opencode_server(&command)))?;
            if !v["mcp"].is_object() {
                v["mcp"] = serde_json::json!({});
            }
            v["mcp"][prism_core::mcp::SERVER] = prism_core::mcp::opencode_server(&command);
            write(&file, &serde_json::to_string_pretty(&v)?)?;
        }
        other => bail!("no CLI {other}: {}", CLIS.join(", ")),
    }
    let dir = skill_dir(cli);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("SKILL.md"), prism_core::mcp::SKILL)?;
    Ok(format!("Prism's MCP server added to {cli} (profile {}), the skill in {}", prism_core::profile::active(), dir.display()))
}

/// Takes the server and the skill out of `cli`.
pub fn uninstall(cli: &str) -> Result<String> {
    match cli {
        "claude" => {
            let _ = std::process::Command::new("claude").args(["mcp", "remove", "--scope", "user", prism_core::mcp::SERVER]).output().context("claude not found in the PATH")?;
        }
        "codex" => {
            let file = codex_config();
            if let Ok(text) = std::fs::read_to_string(&file) {
                let mut doc: toml_edit::DocumentMut = text.parse().with_context(|| format!("{}: not valid TOML", file.display()))?;
                if let Some(servers) = doc.get_mut("mcp_servers").and_then(|s| s.as_table_mut()) {
                    servers.remove(prism_core::mcp::SERVER);
                }
                write(&file, &doc.to_string())?;
            }
        }
        "opencode" => {
            if let Some(file) = opencode_file() {
                let text = std::fs::read_to_string(&file)?;
                let mut v: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("{}: comments or not JSON; remove \"mcp\" › \"prism\" by hand", file.display()))?;
                if let Some(m) = v.get_mut("mcp").and_then(|m| m.as_object_mut()) {
                    m.remove(prism_core::mcp::SERVER);
                }
                write(&file, &serde_json::to_string_pretty(&v)?)?;
            }
        }
        other => bail!("no CLI {other}: {}", CLIS.join(", ")),
    }
    let _ = std::fs::remove_dir_all(skill_dir(cli));
    Ok(format!("Prism's MCP server and skill removed from {cli}"))
}

/// A CLI's config written whole, its former text kept beside it (`.bak`).
fn write(file: &std::path::Path, text: &str) -> Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if file.is_file() {
        std::fs::copy(file, file.with_extension(format!("{}.bak", file.extension().and_then(|e| e.to_str()).unwrap_or("")))).context("backup")?;
    }
    std::fs::write(file, text).with_context(|| file.display().to_string())
}
