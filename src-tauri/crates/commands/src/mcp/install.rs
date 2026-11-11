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
    prism_core::agents::provider::Provider::Codex.home(None).unwrap_or_else(|| home().join(".codex")).join("config.toml")
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
            write(&file, &codex_with(&text, program, args).with_context(|| format!("{}: not valid TOML", file.display()))?)?;
        }
        "opencode" => {
            let file = opencode_file().unwrap_or_else(|| opencode_dir().join("opencode.json"));
            let text = std::fs::read_to_string(&file).unwrap_or_else(|_| "{}".into());
            let with = opencode_with(&text, &command).with_context(|| format!("{}: comments or not JSON; add under \"mcp\": \"prism\": {}", file.display(), prism_core::mcp::opencode_server(&command)))?;
            write(&file, &with)?;
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
                write(&file, &codex_without(&text).with_context(|| format!("{}: not valid TOML", file.display()))?)?;
            }
        }
        "opencode" => {
            if let Some(file) = opencode_file() {
                let text = std::fs::read_to_string(&file)?;
                let without = opencode_without(&text).with_context(|| format!("{}: comments or not JSON; remove \"mcp\" › \"prism\" by hand", file.display()))?;
                write(&file, &without)?;
            }
        }
        other => bail!("no CLI {other}: {}", CLIS.join(", ")),
    }
    let _ = std::fs::remove_dir_all(skill_dir(cli));
    Ok(format!("Prism's MCP server and skill removed from {cli}"))
}

/// Codex's `config.toml` with Prism's server (`[mcp_servers.prism]`), the rest of it as it was.
fn codex_with(text: &str, program: &str, args: &[String]) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("not valid TOML")?;
    let mut entry = toml_edit::Table::new();
    entry.insert("command", toml_edit::value(program));
    entry.insert("args", toml_edit::value(args.iter().map(String::as_str).collect::<toml_edit::Array>()));
    entry.insert("tool_timeout_sec", toml_edit::value(CODEX_TOOL_TIMEOUT));
    let servers = doc.entry("mcp_servers").or_insert_with(|| {
        let mut t = toml_edit::Table::new();
        t.set_implicit(true);
        toml_edit::Item::Table(t)
    });
    servers.as_table_mut().context("mcp_servers: not a table")?.insert(prism_core::mcp::SERVER, toml_edit::Item::Table(entry));
    Ok(doc.to_string())
}

/// Codex's `config.toml` without Prism's server, the rest of it as it was.
fn codex_without(text: &str) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("not valid TOML")?;
    if let Some(servers) = doc.get_mut("mcp_servers").and_then(|s| s.as_table_mut()) {
        servers.remove(prism_core::mcp::SERVER);
    }
    Ok(doc.to_string())
}

/// OpenCode's `opencode.json` with Prism's server under `mcp`; a file with comments (JSONC) refused.
fn opencode_with(text: &str, command: &[String]) -> Result<String> {
    let mut v: serde_json::Value = serde_json::from_str(text)?;
    if !v["mcp"].is_object() {
        v["mcp"] = serde_json::json!({});
    }
    v["mcp"][prism_core::mcp::SERVER] = prism_core::mcp::opencode_server(command);
    Ok(serde_json::to_string_pretty(&v)?)
}

/// OpenCode's `opencode.json` without Prism's server.
fn opencode_without(text: &str) -> Result<String> {
    let mut v: serde_json::Value = serde_json::from_str(text)?;
    if let Some(m) = v.get_mut("mcp").and_then(|m| m.as_object_mut()) {
        m.remove(prism_core::mcp::SERVER);
    }
    Ok(serde_json::to_string_pretty(&v)?)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Codex's config: Prism's server added and taken out, the user's own tables and comments kept.
    #[test]
    fn codex_keeps_the_rest_of_its_config() {
        let mine = "# mine\nmodel = \"gpt\"\n\n[mcp_servers.other]\ncommand = \"x\"\n";
        let with = codex_with(mine, "/bin/prism", &["--profile".into(), "perso".into(), "mcp".into()]).unwrap();
        let t: toml::Table = with.parse().unwrap();
        assert_eq!(t["mcp_servers"]["prism"]["command"].as_str(), Some("/bin/prism"));
        assert_eq!(t["mcp_servers"]["prism"]["args"].as_array().unwrap().len(), 3);
        assert_eq!(t["mcp_servers"]["prism"]["tool_timeout_sec"].as_integer(), Some(CODEX_TOOL_TIMEOUT));
        assert!(with.starts_with("# mine") && t["mcp_servers"].get("other").is_some());
        let without = codex_without(&with).unwrap();
        assert!(without.parse::<toml::Table>().unwrap()["mcp_servers"].get("prism").is_none() && without.contains("[mcp_servers.other]"));
        assert!(codex_with("not = = toml", "p", &[]).is_err());
        assert_eq!(codex_without("model = \"gpt\"\n").unwrap(), "model = \"gpt\"\n", "nothing to take out");
    }

    /// OpenCode's: the same, its other servers kept; a file with comments refused rather than lost.
    #[test]
    fn opencode_keeps_its_other_servers() {
        let with = opencode_with("{\"mcp\": {\"other\": {\"type\": \"local\"}}, \"theme\": \"x\"}", &["/bin/prism".into(), "mcp".into()]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&with).unwrap();
        assert_eq!(v["mcp"]["prism"]["command"][0], "/bin/prism");
        assert!(v["mcp"]["other"].is_object() && v["theme"] == "x");
        let without: serde_json::Value = serde_json::from_str(&opencode_without(&with).unwrap()).unwrap();
        assert!(without["mcp"].get("prism").is_none() && without["mcp"]["other"].is_object());
        assert!(opencode_with("{} // a comment", &[]).is_err());
        assert!(opencode_with("{}", &["p".into()]).unwrap().contains("\"prism\""));
    }
}
