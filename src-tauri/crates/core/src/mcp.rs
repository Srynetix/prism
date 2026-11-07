//! Prism's MCP server for agents (`prism mcp`), as the rest of Prism knows it: the "prism" skill
//! it brings, the local socket it reaches the running Prism by, how an agent CLI is given it, and
//! what the window shows of it (`Snapshot.mcp`: the agents connected, the proposals waiting).
//! The server itself and the socket's other end are in `prism-commands` (`mcp`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;

/// The "prism" skill: what Prism is, its words, which tool to call when. Also the MCP server's
/// instructions, for a CLI that reads no skill.
pub const SKILL: &str = include_str!("../../../../skills/prism/SKILL.md");

/// The skill without its front matter (`---` … `---`).
pub fn skill_body() -> &'static str {
    SKILL.strip_prefix("---").and_then(|rest| rest.split_once("\n---\n")).map_or(SKILL, |(_, body)| body.trim_start())
}

/// The running Prism's end of `prism mcp`: a Unix socket of the profile's data folder, the
/// owner's only (0600, in a 0700 folder). A socket's path is short (104 bytes on macOS): beyond, one
/// named after it in the user's private runtime folder (`$XDG_RUNTIME_DIR`, macOS's `$TMPDIR`).
pub fn socket_path() -> PathBuf {
    socket_for(&crate::profile::data_dir(&crate::profile::data_base()).join("mcp.sock"))
}

/// On Windows, the named pipe in its place: named after the profile's data folder, as a long
/// socket path is (`socket_for`), so that each profile has its own.
pub fn pipe_name() -> String {
    pipe_for("mcp")
}

/// `prism sessions`' socket (`prism_agents::tmux::daemon`), beside `socket_path`: where Prism
/// holds the agents' sessions itself, they outlive the window.
pub fn sessions_socket_path() -> PathBuf {
    socket_for(&crate::profile::data_dir(&crate::profile::data_base()).join("sessions.sock"))
}

/// On Windows, `sessions_socket_path`'s named pipe.
pub fn sessions_pipe_name() -> String {
    pipe_for("sessions")
}

/// The profile's pipe `kind`: named after the socket it stands in for.
fn pipe_for(kind: &str) -> String {
    format!(r"\\.\pipe\prism-{kind}-{:016x}", fnv(&crate::profile::data_dir(&crate::profile::data_base()).join(format!("{kind}.sock"))))
}

/// FNV-1a: the same short name for the same path, in both processes.
fn fnv(path: &Path) -> u64 {
    path.as_os_str().as_encoded_bytes().iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3))
}

fn socket_for(preferred: &Path) -> PathBuf {
    if preferred.as_os_str().len() < 100 {
        return preferred.to_path_buf();
    }
    let hash = fnv(preferred);
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(std::env::temp_dir);
    // A folder of this account's in it (`/tmp` is everyone's): checked as it is used (`private_dir`).
    let stem = preferred.file_stem().and_then(|s| s.to_str()).unwrap_or("mcp");
    base.join(format!("prism-{}", uid())).join(format!("{stem}-{hash:016x}.sock"))
}

#[cfg(unix)]
fn uid() -> u32 {
    prism_sys::process::uid()
}

#[cfg(not(unix))]
fn uid() -> u32 {
    0
}

/// The socket's folder, made if missing (0700), and refused unless it is a real folder of this
/// account's that no one else may enter: another account cannot have put it there first.
pub fn private_dir(socket: &Path) -> Result<()> {
    let dir = socket.parent().context("the socket's folder")?;
    #[cfg(not(unix))]
    let _ = dir;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        if !dir.exists() {
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        }
        let meta = std::fs::symlink_metadata(dir)?;
        // The profile's data folder sits in the home (private already); the fallback in a shared one.
        let shared = !dir.starts_with(crate::profile::data_base());
        anyhow::ensure!(meta.is_dir() && meta.uid() == uid() && (!shared || meta.mode() & 0o077 == 0), "{}: not a private folder of this account", dir.display());
    }
    Ok(())
}

/// The socket at `path` is this account's: `prism mcp` never talks to another's.
#[cfg(unix)]
pub fn owned_socket(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    private_dir(path)?;
    let meta = std::fs::symlink_metadata(path).with_context(|| format!("no {}", path.display()))?;
    anyhow::ensure!(meta.uid() == uid(), "{}: another account's socket", path.display());
    Ok(())
}

/// The command line that starts `prism mcp` for the active profile: this executable (the app's,
/// or the server's), the profile named.
pub fn command() -> Vec<String> {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "prism".into());
    vec![exe, "--profile".into(), crate::profile::active().into(), "mcp".into()]
}

/// `command` in its « in passing » mode (`prism mcp aside`): `suggest_task` and `dismiss_task`
/// only, for the agents Prism launches without the whole server (`[agents.aside]`).
pub fn aside_command() -> Vec<String> {
    let mut c = command();
    c.push("aside".into());
    c
}

/// Claude Code's `--mcp-config` file of the « in passing » server, in `dir` (written again each
/// time, as the plugin). Its path.
pub fn write_claude_aside(dir: &Path, command: &[String]) -> Result<PathBuf> {
    let file = claude_aside_file(dir);
    std::fs::write(&file, serde_json::to_string_pretty(&claude_servers(command))?)?;
    owner_only(&file)?;
    Ok(file)
}

/// Where `write_claude_aside` writes it in `dir`.
pub fn claude_aside_file(dir: &Path) -> PathBuf {
    dir.join("prism-aside-mcp.json")
}

/// The name an agent CLI knows the server by.
pub const SERVER: &str = "prism";

/// The plugin Claude Code is given (`--plugin-dir`): the MCP server and the skill, in `dir`
/// (written again each time, the executable's path may have changed). Its folder.
pub fn write_claude_plugin(dir: &Path, command: &[String]) -> Result<PathBuf> {
    let root = claude_plugin_dir(dir);
    std::fs::create_dir_all(root.join(".claude-plugin"))?;
    std::fs::create_dir_all(root.join("skills/prism"))?;
    let manifest = serde_json::json!({ "name": "prism", "description": "Prism's MCP server and its skill: understand and configure Prism.", "version": crate::packs::PRISM });
    std::fs::write(root.join(".claude-plugin/plugin.json"), serde_json::to_string_pretty(&manifest)?)?;
    std::fs::write(root.join(".mcp.json"), serde_json::to_string_pretty(&claude_servers(command))?)?;
    std::fs::write(root.join("skills/prism/SKILL.md"), SKILL)?;
    Ok(root)
}

/// Where `write_claude_plugin` writes it in `dir`.
pub fn claude_plugin_dir(dir: &Path) -> PathBuf {
    dir.join("prism-mcp-plugin")
}

/// `{"mcpServers": {"prism": …}}`, Claude Code's shape.
pub fn claude_servers(command: &[String]) -> serde_json::Value {
    let (program, args) = command.split_first().map_or(("prism", &[][..]), |(p, a)| (p.as_str(), a));
    serde_json::json!({ "mcpServers": { SERVER: { "type": "stdio", "command": program, "args": args } } })
}

/// Codex's options for it: `-c mcp_servers.prism.command=…`, `-c mcp_servers.prism.args=[…]`.
pub fn codex_args(command: &[String]) -> Vec<String> {
    let (program, args) = command.split_first().map_or(("prism", &[][..]), |(p, a)| (p.as_str(), a));
    let quoted = |s: &str| toml::Value::String(s.to_string()).to_string();
    let list = format!("[{}]", args.iter().map(|a| quoted(a)).collect::<Vec<_>>().join(", "));
    // A proposal waits for the user: longer than Codex's 60 s by default.
    vec!["-c".into(), format!("mcp_servers.{SERVER}.command={}", quoted(program)), "-c".into(), format!("mcp_servers.{SERVER}.args={list}"), "-c".into(), format!("mcp_servers.{SERVER}.tool_timeout_sec=300")]
}

/// OpenCode's `mcp` entry for it (`OPENCODE_CONFIG_CONTENT`, or its config file).
pub fn opencode_server(command: &[String]) -> serde_json::Value {
    serde_json::json!({ "type": "local", "command": command, "enabled": true })
}

/// What the window shows of `prism mcp`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct McpState {
    /// The `prism mcp` connected now, oldest first.
    pub clients: Vec<McpClient>,
    /// The proposals waiting for an answer, oldest first.
    pub proposals: Vec<Proposal>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct McpClient {
    pub id: u64,
    /// The CLI, as it introduced itself (`claude-code`, `codex`…).
    pub name: String,
    /// The Prism agent it runs in (`PRISM_AGENT`), if any.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub since: DateTime<Utc>,
    pub reads: u32,
    pub proposals: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "ProposalKind"))]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    /// A change of `config.toml`.
    Config,
    /// The pages (`[[views]]`).
    Layout,
    /// A new custom block (or a change of one): trusted as it is added.
    CustomBlock,
}

/// A change an agent proposes, waiting for the user's answer in Prism.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Proposal {
    pub id: u64,
    pub kind: ProposalKind,
    /// Who proposes it: the CLI, and its Prism agent.
    pub client: String,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The agent's reason, in its words.
    pub why: String,
    pub at: DateTime<Utc>,
    /// `config.toml` once applied, whole: what Apply writes (or the TOML panel opens).
    pub text: String,
    /// From the file as it was to `text`.
    pub diff: Vec<crate::textdiff::Line>,
    /// The blocks and tables it changes (`MRs`, `[[repos]]`…).
    pub touches: Vec<String>,
    /// The keys it sets that a pack sets too: `config.toml` overrides them.
    pub overrides: Vec<String>,
    /// `config.toml` changed since it was proposed: Apply would write it again on the new one.
    pub stale: bool,
    /// What a custom block ran in the sandbox before it was proposed, if it was.
    #[cfg_attr(feature = "ts", ts(optional, type = "unknown"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<serde_json::Value>,
    /// The custom block it adds or changes: its id.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// What the check let through but says (a name it does not know, a page naming no block).
    pub warnings: Vec<String>,
    /// The connection it came by: gone, the proposal goes too.
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(skip)]
    pub client_id: Option<u64>,
}

/// `path` names a file that may be written by the owner only.
pub fn owner_only(path: &Path) -> Result<()> {
    #[cfg(not(unix))]
    let _ = path;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).with_context(|| format!("{}: permissions", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_skill_has_its_front_matter_and_a_body() {
        assert!(super::SKILL.starts_with("---\nname: prism\n"));
        assert!(super::skill_body().starts_with("# Prism"));
    }

    #[test]
    fn a_long_socket_path_is_made_short() {
        let long = std::path::Path::new("/Users/someone/Library/Application Support/app.prism.desktop/profiles/a-rather-long-profile-name/mcp.sock");
        let short = super::socket_for(long);
        assert!(short.as_os_str().len() < 104 && short != long && short == super::socket_for(long));
        assert_eq!(super::socket_for(std::path::Path::new("/tmp/x/mcp.sock")), std::path::Path::new("/tmp/x/mcp.sock"));
        assert!(short.parent().unwrap().file_name().unwrap().to_string_lossy().starts_with("prism-"));
    }

    #[cfg(unix)]
    #[test]
    fn a_shared_folder_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("prism-mcp-test-{}", std::process::id()));
        let sock = dir.join("s.sock");
        let _ = std::fs::remove_dir_all(&dir);
        super::private_dir(&sock).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(super::private_dir(&sock).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_cli_is_given_the_same_command() {
        let cmd = ["/Applications/Prism.app/Contents/MacOS/prism".to_string(), "--profile".into(), "default".into(), "mcp".into()];
        let codex = super::codex_args(&cmd);
        assert_eq!(codex[1], r#"mcp_servers.prism.command="/Applications/Prism.app/Contents/MacOS/prism""#);
        assert_eq!(codex[3], r#"mcp_servers.prism.args=["--profile", "default", "mcp"]"#);
        assert_eq!(super::claude_servers(&cmd)["mcpServers"]["prism"]["args"][2], "mcp");
        assert_eq!(super::opencode_server(&cmd)["command"][0], cmd[0]);
    }
}
