//! The coding agent CLI an agent runs (`[agents] provider`): Claude Code, Command Code, OpenCode
//! or Codex. All are terminal apps that Prism hosts in tmux the same way; they differ in their
//! command line and in how Prism follows them. Claude Code calls `prism hook` from the hooks of a
//! settings file (`--settings`), Codex from hooks given on its command line (`-c hooks.<event>=…`,
//! read from `codex-hooks.json`); Command Code loads a mod (`--mod`, `command_code_mod.ts`) and
//! OpenCode a plugin (`OPENCODE_CONFIG_CONTENT`, `opencode_plugin.ts`) that write the same log
//! lines themselves. All stop with two Ctrl-C; Claude Code and Command Code ask permissions with
//! the same keys (`1`: yes) and interrupt with Esc, OpenCode with Enter and Esc twice, Codex with
//! `y` and Esc.

use prism_base::i18n::tr;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "AgentProvider"))]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    #[default]
    Claude,
    CommandCode,
    #[serde(rename = "opencode")]
    OpenCode,
    Codex,
}

/// Prism's mod for Command Code, written next to the hooks settings on every launch.
pub const COMMAND_CODE_MOD: &str = include_str!("command_code_mod.ts");
/// Prism's plugin for OpenCode, written next to them on every launch.
pub const OPENCODE_PLUGIN: &str = include_str!("opencode_plugin.ts");

/// What a CLI can do, as the window needs it (`Provider::spec`): written out with the bindings
/// (`web/src/bindings/provider-specs.ts`), so that the window asks this rather than its name.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProviderSpec {
    pub provider: Provider,
    /// Its usual executable (`program`, the computer's).
    pub program: String,
    /// It creates the agent's worktree itself (`delegates_worktree`).
    pub delegates_worktree: bool,
    /// Prism reads its transcript (`has_transcript`), its conversation (`has_conversation`), its plan's usage (`has_plan`).
    pub transcript: bool,
    pub conversation: bool,
    pub plan: bool,
    /// It takes more folders (`takes_dirs`), a reasoning effort (`takes_effort`).
    pub takes_dirs: bool,
    pub takes_effort: bool,
    /// Its effort levels are its model's (`models::efforts`: Codex's catalog).
    pub efforts_by_model: bool,
    /// Its conversation picked up again takes a first prompt (`resume_with`): Continue with… back on it.
    pub resumes_with_prompt: bool,
}

impl Provider {
    pub const ALL: [Provider; 4] = [Self::Claude, Self::CommandCode, Self::OpenCode, Self::Codex];

    /// The variable that gives it a folder of its own (an account, `[[agents.accounts]]`):
    /// `CLAUDE_CONFIG_DIR`, `CODEX_HOME`; none for the CLIs without accounts.
    pub fn home_var(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("CLAUDE_CONFIG_DIR"),
            Self::Codex => Some("CODEX_HOME"),
            Self::CommandCode | Self::OpenCode => None,
        }
    }

    /// The folder of an account (`dir`) as its variable should give it: none when it is the CLI's
    /// usual one already (`home(None)`), which it then runs in as before. Claude Code keeps the
    /// login of a folder given by `CLAUDE_CONFIG_DIR` apart (another keychain entry): set to
    /// `~/.claude`, it would ask to log in again.
    pub fn own_home(self, dir: &Path) -> Option<PathBuf> {
        let usual = self.home(None)?;
        (dir != usual).then(|| dir.to_path_buf())
    }

    /// Its folder (login, settings, sessions): an account's (`dir`), else the one its variable
    /// names in Prism's environment, else its usual one (`~/.claude`, `~/.codex`).
    pub fn home(self, dir: Option<&Path>) -> Option<PathBuf> {
        let var = self.home_var()?;
        dir.map(Path::to_path_buf).or_else(|| std::env::var_os(var).map(PathBuf::from)).or_else(|| {
            let usual = if self == Self::Claude { ".claude" } else { ".codex" };
            dirs::home_dir().map(|h| h.join(usual))
        })
    }

    /// What it can do (`ProviderSpec`).
    pub fn spec(self) -> ProviderSpec {
        ProviderSpec {
            provider: self,
            program: self.program().into(),
            delegates_worktree: self.delegates_worktree(),
            transcript: self.has_transcript(),
            conversation: self.has_conversation(),
            plan: self.has_plan(),
            takes_dirs: self.takes_dirs(),
            takes_effort: self.takes_effort(),
            efforts_by_model: self == Self::Codex,
            resumes_with_prompt: self.resume_with("s", Path::new("w"), None, "p").is_some(),
        }
    }

    /// The file, in the agents' folder, that makes the CLI report to Prism: hooks settings, mod,
    /// plugin, or Codex's hooks.
    pub fn wiring_file(self) -> &'static str {
        match self {
            Self::Claude => "hooks.json",
            Self::CommandCode => "command-code-mod.ts",
            Self::OpenCode => "opencode-plugin.ts",
            Self::Codex => "codex-hooks.json",
        }
    }

    /// The CLI's executable; on Windows, `cmd` is the system shell and Command Code is `cmdc`.
    pub fn program(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::CommandCode if cfg!(windows) => "cmdc",
            Self::CommandCode => "cmd",
            Self::OpenCode => "opencode",
            Self::Codex => "codex",
        }
    }

    /// Where the CLI keeps its sessions' scratchpads (`files::scratchpad`): Claude Code and Command
    /// Code write the pages of a session (its artifacts) in `<dir>/<project>/<session>/scratchpad`;
    /// OpenCode and Codex have no scratchpad.
    pub fn tmp_dir(self) -> Option<std::path::PathBuf> {
        crate::platform::cli_tmp_dir(match self {
            Self::Claude => "claude",
            Self::CommandCode => "commandcode",
            Self::OpenCode | Self::Codex => return None,
        })
    }

    /// Prism reads the agent's conversation from a log the CLI writes (Claude Code's transcript,
    /// Codex's rollout): its Chat, context detail, usage and slash commands.
    pub fn has_transcript(self) -> bool {
        matches!(self, Self::Claude | Self::Codex)
    }

    /// Prism reads its conversation (the Chat tab) from its transcript: those of `has_transcript`,
    /// and Command Code's (`~/.commandcode/projects/<folder>/<session>.jsonl`), which only the Chat reads.
    pub fn has_conversation(self) -> bool {
        self.has_transcript() || self == Self::CommandCode
    }

    /// The CLI runs on a plan whose usage Prism reads; OpenCode has none.
    pub fn has_plan(self) -> bool {
        self != Self::OpenCode
    }

    /// Claude Code's transcript can be found by the session's id when its hooks did not name it
    /// (`usage::find_transcript`); Codex's rollout is known only from them.
    pub fn finds_transcript(self) -> bool {
        self == Self::Claude
    }

    /// Claude Code creates the agent's worktree through Prism's `WorktreeCreate` hook
    /// (`--worktree`); for the others, Prism creates it before launching them there.
    pub fn delegates_worktree(self) -> bool {
        self == Self::Claude
    }

    /// The launch command line. `wiring`: the hooks settings (Claude Code), the mod (Command Code) or
    /// Codex's hooks (`codex_hooks`), OpenCode's plugin going in its environment (`env`); `mode`: one of Prism's permission modes,
    /// Claude Code's names; `worktree`: the name to request, only when the CLI creates it.
    pub fn launch(self, id: &str, title: &str, wiring: &Path, mode: Option<&str>, worktree: Option<&str>, prompt: Option<&str>) -> Result<Vec<String>> {
        let wiring = wiring.display().to_string();
        // A value, never an option of the CLI's (a phone renames an agent freely).
        let title = Some(title.trim_start_matches(|c: char| c == '-' || c.is_whitespace())).filter(|t| !t.is_empty()).unwrap_or("Agent");
        let mut argv = match self {
            Self::Claude => vec![self.program().to_string(), "--session-id".into(), id.into(), "-n".into(), title.into(), "--settings".into(), wiring],
            // Command Code picks its own session id: the mod reports it at start (`SessionStart`).
            Self::CommandCode => vec![self.program().to_string(), "-n".into(), title.into(), "--mod".into(), wiring],
            // OpenCode too, through the plugin; it has no title of its own to set at launch.
            Self::OpenCode => {
                let mut argv = vec![self.program().to_string()];
                match mode.filter(|m| *m != "default").map(|m| self.permission_mode(m)).transpose()? {
                    Some("plan") => argv.extend(["--agent".into(), "plan".into()]),
                    Some("auto") => argv.push("--auto".into()),
                    _ => {}
                }
                // Attached (`--prompt=…`): a prompt starting with `-` stays the prompt.
                argv.extend(prompt.map(|p| format!("--prompt={p}")));
                return Ok(argv);
            }
            // Codex too, reported by its hooks at start; it has no title of its own to set at launch.
            Self::Codex => {
                let mut argv = vec![self.program().to_string()];
                argv.extend(codex_hooks(Path::new(&wiring)));
                match mode.filter(|m| *m != "default").map(|m| self.permission_mode(m)).transpose()? {
                    Some("auto") => argv.push("--approve-for-me".into()),
                    Some(sandbox) => argv.extend(["--sandbox".into(), sandbox.into()]),
                    None => {}
                }
                argv.extend(prompt.into_iter().flat_map(|p| ["--".to_string(), p.to_string()]));
                return Ok(argv);
            }
        };
        if let Some(mode) = mode.filter(|m| *m != "default") {
            argv.extend(["--permission-mode".into(), self.permission_mode(mode)?.into()]);
        }
        if let Some(name) = worktree {
            argv.extend(["--worktree".into(), name.into()]);
        }
        // After `--`: a prompt starting with `-` (`--dangerously-skip-permissions`) is no option.
        argv.extend(prompt.into_iter().flat_map(|p| ["--".to_string(), p.to_string()]));
        Ok(argv)
    }

    /// It can be given more folders than the one it runs in (`--add-dir`): an agent in the worktrees
    /// of several repositories at once.
    pub fn takes_dirs(self) -> bool {
        matches!(self, Self::Claude | Self::Codex)
    }

    /// A launch or resume command line given `dirs` too (`--add-dir`, right after the executable,
    /// after `resume` for Codex, whose subcommand comes first); a CLI that takes none, unchanged.
    /// Claude Code's `--add-dir` takes several values: an option always follows it, never the prompt.
    pub fn with_dirs(self, mut argv: Vec<String>, dirs: &[PathBuf]) -> Vec<String> {
        if !self.takes_dirs() || dirs.is_empty() {
            return argv;
        }
        let at = if self == Self::Codex && argv.get(1).is_some_and(|a| a == "resume") { 2 } else { 1 };
        let added: Vec<String> = dirs.iter().flat_map(|d| ["--add-dir".to_string(), d.display().to_string()]).collect();
        argv.splice(at.min(argv.len())..at.min(argv.len()), added);
        argv
    }

    /// Its reasoning effort can be chosen (`[agents.<cli>] effort`, the launcher, the agent's header).
    pub fn takes_effort(self) -> bool {
        matches!(self, Self::Claude | Self::Codex)
    }

    /// A launch or resume command line with the CLI's own executable, model, effort and extra
    /// arguments (`[agents.<cli>]`): `--model` (every CLI takes it), the effort (Claude Code's
    /// `--effort`, Codex's `model_reasoning_effort`), then the arguments right after the
    /// executable, after `resume` for Codex, whose subcommand comes first. A model or an effort
    /// that is not one (empty, an option, spaces) is left out.
    pub fn with_cli(self, mut argv: Vec<String>, cli: &crate::config::CliConfig) -> Vec<String> {
        if let (Some(bin), Some(first)) = (cli.bin.as_deref(), argv.first_mut()) {
            *first = bin.to_string();
        }
        let at = if self == Self::Codex && argv.get(1).is_some_and(|a| a == "resume") { 2 } else { 1 };
        let model = cli.model.as_deref().map(str::trim).filter(|m| crate::agents::models::valid(m));
        let effort = cli.effort.as_deref().map(str::trim).filter(|e| crate::agents::models::valid_effort(e));
        let effort: Vec<String> = match (self, effort) {
            (Self::Claude, Some(e)) => vec!["--effort".into(), e.into()],
            (Self::Codex, Some(e)) => vec!["-c".into(), format!("model_reasoning_effort=\"{e}\"")],
            _ => Vec::new(),
        };
        let added = model.into_iter().flat_map(|m| ["--model".to_string(), m.to_string()]).chain(effort).chain(cli.args.iter().cloned());
        argv.splice(at.min(argv.len())..at.min(argv.len()), added);
        argv
    }

    /// Picks up the agent's conversation: Claude Code by Prism's id, or the one started since by a
    /// `/clear` (`session`, reported by the hooks); Command Code, OpenCode and Codex by their own
    /// (reported by the mod, the plugin, the hooks), or failing that the last conversation of the
    /// folder. In the permission `mode` it was launched in: an agent in plan mode stays in it.
    pub fn resume(self, id: &str, session: Option<&str>, wiring: &Path, mode: Option<&str>) -> Vec<String> {
        let wiring = wiring.display().to_string();
        // A session id reaches the command line: never an option (`--dangerously-…`).
        let session = session.filter(|s| valid_session(s));
        let mut argv = match (self, session) {
            (Self::Claude, session) => vec![self.program().into(), "--resume".into(), session.unwrap_or(id).into(), "--settings".into(), wiring],
            (Self::CommandCode, Some(session)) => vec![self.program().into(), "--mod".into(), wiring, "--session".into(), session.into()],
            (Self::CommandCode, None) => vec![self.program().into(), "--mod".into(), wiring, "--continue".into()],
            (Self::OpenCode, Some(session)) => vec![self.program().into(), "--session".into(), session.into()],
            (Self::OpenCode, None) => vec![self.program().into(), "--continue".into()],
            (Self::Codex, session) => {
                let mut argv = vec![self.program().to_string(), "resume".into()];
                argv.extend(codex_hooks(Path::new(&wiring)));
                argv.extend(self.mode_args(mode));
                argv.push(session.map_or("--last", |s| s).into());
                return argv;
            }
        };
        argv.extend(self.mode_args(mode));
        argv
    }

    /// Picks `session` up again with a first prompt (an agent back on a CLI it ran before), in the
    /// permission `mode` asked: Claude Code and Codex take one on resume; none for the others,
    /// which start a new conversation.
    pub fn resume_with(self, session: &str, wiring: &Path, mode: Option<&str>, prompt: &str) -> Option<Vec<String>> {
        if !valid_session(session) {
            return None;
        }
        let wiring = wiring.display().to_string();
        // After `--`: a prompt starting with `-` is no option.
        match self {
            Self::Claude => {
                let mut argv = vec![self.program().into(), "--resume".into(), session.into(), "--settings".into(), wiring];
                argv.extend(self.mode_args(mode));
                argv.extend(["--".into(), prompt.into()]);
                Some(argv)
            }
            Self::Codex => {
                let mut argv = vec![self.program().to_string(), "resume".into()];
                argv.extend(codex_hooks(Path::new(&wiring)));
                argv.extend(self.mode_args(mode));
                argv.extend([session.to_string(), "--".into(), prompt.into()]);
                Some(argv)
            }
            Self::CommandCode | Self::OpenCode => None,
        }
    }

    /// A permission mode's options, as `launch` gives them (none for the default, nor for a mode
    /// the CLI does not have).
    fn mode_args(self, mode: Option<&str>) -> Vec<String> {
        let Some(mode) = mode.filter(|m| *m != "default").and_then(|m| self.permission_mode(m).ok()) else { return Vec::new() };
        match (self, mode) {
            (Self::OpenCode, "plan") => vec!["--agent".into(), "plan".into()],
            (Self::OpenCode, "auto") => vec!["--auto".into()],
            (Self::OpenCode, _) => Vec::new(),
            (Self::Codex, "auto") => vec!["--approve-for-me".into()],
            (Self::Codex, sandbox) => vec!["--sandbox".into(), sandbox.into()],
            (_, mode) => vec!["--permission-mode".into(), mode.into()],
        }
    }

    /// The options that give it Prism's MCP server (`whole`: every tool and the skill, `[agents]
    /// prism_mcp`; otherwise « in passing » alone, `[agents.aside]`), files read from `work`
    /// (`write_mcp`). OpenCode gets it in its environment (`env`), Command Code not at all.
    pub fn mcp_args(self, work: &Path, whole: bool) -> Vec<String> {
        use crate::mcp;
        match (self, whole) {
            (Self::Claude, true) => vec!["--plugin-dir".into(), mcp::claude_plugin_dir(work).display().to_string()],
            (Self::Claude, false) => vec!["--mcp-config".into(), mcp::claude_aside_file(work).display().to_string()],
            (Self::Codex, true) => mcp::codex_args(&mcp::command()),
            (Self::Codex, false) => mcp::codex_args(&mcp::aside_command()),
            (Self::OpenCode | Self::CommandCode, _) => Vec::new(),
        }
    }

    /// The « in passing » tools allowed without asking (`suggest_task` only records a proposal:
    /// never a permission prompt that would stop the agent); a CLI that asks for none, nothing.
    pub fn aside_allowed(self) -> Vec<String> {
        if self != Self::Claude {
            return Vec::new();
        }
        // Its tools as named by `--mcp-config`, and by the plugin of the whole server (`plugin_<plugin>_<server>`).
        let s = crate::mcp::SERVER;
        vec![format!("--allowedTools=mcp__{s}__suggest_task,mcp__{s}__dismiss_task,mcp__plugin_{s}_{s}__suggest_task,mcp__plugin_{s}_{s}__dismiss_task,mcp__{s}__done_task,mcp__plugin_{s}_{s}__done_task")]
    }

    /// Prism's assistant's options: Prism's tools allowed without asking, those that write, run
    /// or read files taken away (Claude Code). The others: Codex runs it in its read-only sandbox
    /// (its permission mode, `plan`); OpenCode and Command Code take no such option.
    pub fn assistant_args(self) -> Vec<String> {
        if self != Self::Claude {
            return Vec::new();
        }
        let s = crate::mcp::SERVER;
        vec![format!("--allowedTools=mcp__{s},mcp__plugin_{s}_{s}"), format!("--disallowedTools={}", ASSISTANT_DENIED.join(","))]
    }

    /// The files `mcp_args` reads, written in `work` for the executable as it is now (its path
    /// follows Prism's): Claude Code's plugin and its « in passing » `--mcp-config`. The others need none.
    /// Each written whatever became of the other: one failed, the other still serves.
    pub fn write_mcp(self, work: &Path) -> anyhow::Result<()> {
        if self == Self::Claude {
            let plugin = crate::mcp::write_claude_plugin(work, &crate::mcp::command());
            let aside = crate::mcp::write_claude_aside(work, &crate::mcp::aside_command());
            plugin?;
            aside?;
        }
        Ok(())
    }

    /// What the CLI's environment adds for Prism: OpenCode's plugin (`wiring`), merged into the
    /// user's own config, and, for `acceptEdits`, edits allowed whatever that config says (OpenCode
    /// allows them by default).
    /// `mcp`: Prism's MCP server's command line, given to it too (`[agents] prism_mcp`).
    pub fn env(self, wiring: &Path, mode: Option<&str>, mcp: Option<&[String]>) -> Vec<String> {
        if self != Self::OpenCode {
            return Vec::new();
        }
        let mut config = serde_json::json!({ "plugin": [format!("file://{}", wiring.display())] });
        if mode == Some("acceptEdits") {
            config["permission"] = serde_json::json!({ "edit": "allow" });
        }
        if let Some(command) = mcp {
            config["mcp"] = serde_json::json!({ crate::mcp::SERVER: crate::mcp::opencode_server(command) });
        }
        vec![format!("OPENCODE_CONFIG_CONTENT={config}")]
    }

    /// The keys that grant once the permission asked: `1. Yes`, OpenCode's preselected `Allow once`,
    /// Codex's `Yes, proceed (y)`.
    pub fn allow_keys(self) -> &'static [&'static str] {
        match self {
            Self::OpenCode => &["Enter"],
            Self::Codex => &["y"],
            _ => &["1"],
        }
    }

    /// The keys that interrupt the current turn: Esc, twice for OpenCode (within 5 s).
    pub fn interrupt_keys(self) -> &'static [&'static str] {
        match self {
            Self::OpenCode => &["Escape", "Escape"],
            _ => &["Escape"],
        }
    }

    /// One of Prism's permission modes (Claude Code's names), in the CLI's words.
    pub fn permission_mode(self, mode: &str) -> Result<&'static str> {
        Ok(match (self, mode) {
            (Self::Claude, "acceptEdits") => "acceptEdits",
            (Self::Claude, "plan") => "plan",
            (Self::Claude, "auto") => "auto",
            (Self::CommandCode, "acceptEdits") => "accept-edits",
            (Self::CommandCode, "plan") => "plan",
            (Self::CommandCode, "auto") => bail!(tr("Command Code has no auto permission mode")),
            // Edits through the environment (`env`), plan as an agent, auto as `--auto`.
            (Self::OpenCode, "acceptEdits") => "acceptEdits",
            (Self::OpenCode, "plan") => "plan",
            (Self::OpenCode, "auto") => "auto",
            // Codex has no plan mode to start in: a read-only sandbox, where it can only read and
            // propose; edits in the workspace without asking, its workspace-write sandbox; auto, its
            // automatic review of approval requests (`--approve-for-me`).
            (Self::Codex, "acceptEdits") => "workspace-write",
            (Self::Codex, "plan") => "read-only",
            (Self::Codex, "auto") => "auto",
            _ => bail!("unknown permission mode {mode:?}"),
        })
    }
}

/// A CLI's session id as the agents' log reports it: letters, digits, `-` and `_`, never starting
/// with `-` (it goes on a resume's command line).
pub fn valid_session(id: &str) -> bool {
    (1..=128).contains(&id.len()) && !id.starts_with('-') && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Codex's hooks, written by Prism next to Claude Code's settings (`hook::codex_overrides`): the
/// `-c hooks.<event>=…` pairs of its command line. Unreadable, none: the agent runs unfollowed.
fn codex_hooks(file: &Path) -> Vec<String> {
    let pairs: Vec<String> = match std::fs::read(file) {
        Ok(raw) => serde_json::from_slice(&raw).unwrap_or_else(|e| {
            tracing::warn!("codex: {} unreadable, the agent runs unfollowed: {e}", file.display());
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    // Hooks only: any other key (`approval_policy`, `mcp_servers.…`) a program wrote there is not
    // Prism's, and goes on no command line.
    pairs.into_iter().filter(|p| p.starts_with("hooks.")).flat_map(|p| ["-c".to_string(), p]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prism's MCP server for each CLI, whole or « in passing » alone, from the one place.
    #[test]
    fn each_cli_gets_prism_s_mcp_its_own_way() {
        let work = Path::new("/w");
        assert_eq!(Provider::Claude.mcp_args(work, true)[..2], ["--plugin-dir".to_string(), "/w/prism-mcp-plugin".into()]);
        assert_eq!(Provider::Claude.mcp_args(work, false)[0], "--mcp-config");
        let codex = Provider::Codex.mcp_args(work, false).join(" ");
        assert!(codex.contains("mcp_servers.prism.args") && codex.contains("\"aside\""), "{codex}");
        assert!(Provider::OpenCode.mcp_args(work, true).is_empty() && Provider::CommandCode.mcp_args(work, true).is_empty());
        assert!(Provider::Claude.aside_allowed()[0].starts_with("--allowedTools=mcp__prism__suggest_task"));
        assert!(Provider::Codex.aside_allowed().is_empty());
    }

    /// What the window reads of each CLI (`provider-specs.ts`) says what its command lines do.
    #[test]
    fn a_spec_says_what_the_cli_does() {
        let can = |f: fn(&ProviderSpec) -> bool| Provider::ALL.into_iter().filter(|p| f(&p.spec())).collect::<Vec<_>>();
        assert_eq!(can(|s| s.resumes_with_prompt), [Provider::Claude, Provider::Codex]);
        assert_eq!(can(|s| s.takes_effort), [Provider::Claude, Provider::Codex]);
        assert_eq!(can(|s| s.efforts_by_model), [Provider::Codex]);
        assert_eq!(can(|s| s.delegates_worktree), [Provider::Claude]);
        assert_eq!(can(|s| !s.plan), [Provider::OpenCode]);
    }

    #[test]
    fn extra_folders_follow_the_executable() {
        let dirs = [PathBuf::from("/w/helm"), PathBuf::from("/w/dockerfiles")];
        let claude = Provider::Claude.with_dirs(vec!["claude".into(), "--session-id".into(), "x".into(), "--".into(), "go".into()], &dirs);
        assert_eq!(claude, ["claude", "--add-dir", "/w/helm", "--add-dir", "/w/dockerfiles", "--session-id", "x", "--", "go"]);
        let resume = Provider::Codex.with_dirs(vec!["codex".into(), "resume".into(), "--last".into()], &dirs[..1]);
        assert_eq!(resume, ["codex", "resume", "--add-dir", "/w/helm", "--last"]);
        let opencode = vec!["opencode".to_string()];
        assert_eq!(Provider::OpenCode.with_dirs(opencode.clone(), &dirs), opencode, "it takes none");
    }

    #[test]
    fn builds_each_command_line() {
        let wiring = Path::new("/data/agents/hooks.json");
        let claude = Provider::Claude.launch("ab-12", "Fix", wiring, Some("acceptEdits"), Some("fix"), Some("Go")).unwrap();
        assert_eq!(claude, ["claude", "--session-id", "ab-12", "-n", "Fix", "--settings", "/data/agents/hooks.json", "--permission-mode", "acceptEdits", "--worktree", "fix", "--", "Go"]);
        // A prompt is never an option: `--` before it (`--prompt=` for OpenCode).
        let flag = Some("--dangerously-skip-permissions");
        assert_eq!(Provider::Claude.launch("ab-12", "Fix", wiring, None, None, flag).unwrap()[7..], ["--", "--dangerously-skip-permissions"]);
        assert_eq!(Provider::OpenCode.launch("ab-12", "Fix", wiring, None, None, flag).unwrap(), ["opencode", "--prompt=--dangerously-skip-permissions"]);
        let module = Path::new("/data/agents/command-code-mod.ts");
        let cmd = Provider::CommandCode.launch("ab-12", "Fix", module, Some("acceptEdits"), None, None).unwrap();
        assert_eq!(&cmd[1..], ["-n", "Fix", "--mod", "/data/agents/command-code-mod.ts", "--permission-mode", "accept-edits"]);
        assert!(Provider::CommandCode.launch("ab-12", "Fix", module, Some("auto"), None, None).is_err());
        assert!(Provider::Claude.launch("ab-12", "Fix", wiring, Some("yolo"), None, None).is_err());
        assert_eq!(Provider::Claude.launch("ab-12", "Fix", wiring, Some("default"), None, None).unwrap().len(), 7);
    }

    #[test]
    fn adds_the_cli_own_executable_and_arguments() {
        let cli = crate::config::CliConfig { bin: Some("/opt/claude".into()), args: vec!["--model".into(), "opus".into()], ..Default::default() };
        let argv = Provider::Claude.with_cli(vec!["claude".into(), "--resume".into(), "ab-12".into()], &cli);
        assert_eq!(argv, ["/opt/claude", "--model", "opus", "--resume", "ab-12"]);
        let codex = crate::config::CliConfig { bin: None, args: vec!["--profile".into(), "work".into()], ..Default::default() };
        assert_eq!(Provider::Codex.with_cli(vec!["codex".into(), "resume".into(), "--last".into()], &codex), ["codex", "resume", "--profile", "work", "--last"]);
        assert_eq!(Provider::Codex.with_cli(vec!["codex".into(), "Go".into()], &codex), ["codex", "--profile", "work", "Go"]);
        assert_eq!(Provider::OpenCode.with_cli(vec!["opencode".into()], &Default::default()), ["opencode"]);
    }

    #[test]
    fn resumes_with_a_relay() {
        let hooks = Path::new("/h.json");
        assert_eq!(Provider::Claude.resume_with("ab-12", hooks, None, "-- go on").unwrap(), ["claude", "--resume", "ab-12", "--settings", "/h.json", "--", "-- go on"]);
        // Back in plan mode, as asked: an agent on untrusted input stays read-only on each CLI.
        assert_eq!(Provider::Claude.resume_with("ab-12", hooks, Some("plan"), "go").unwrap()[5..], ["--permission-mode", "plan", "--", "go"]);
        assert_eq!(Provider::Codex.resume_with("019a-ses", Path::new("/nowhere.json"), Some("plan"), "go").unwrap(), ["codex", "resume", "--sandbox", "read-only", "019a-ses", "--", "go"]);
        let codex = Provider::Codex.resume_with("019a-ses", Path::new("/nowhere.json"), None, "go on").unwrap();
        assert_eq!(codex, ["codex", "resume", "019a-ses", "--", "go on"]);
        // Model and effort go after `resume`, before the session.
        let cli = crate::config::CliConfig { model: Some("gpt-x".into()), ..Default::default() };
        assert_eq!(Provider::Codex.with_cli(codex, &cli)[..4], ["codex", "resume", "--model", "gpt-x"]);
        assert!(Provider::OpenCode.resume_with("ses_1", hooks, None, "go").is_none(), "a new conversation instead");
        assert!(Provider::Claude.resume_with("--x", hooks, None, "go").is_none());
    }

    #[test]
    fn resumes_by_the_cli_session() {
        let module = Path::new("/m.ts");
        assert_eq!(&Provider::CommandCode.resume("ab-12", Some("f761"), module, None)[1..], ["--mod", "/m.ts", "--session", "f761"]);
        assert_eq!(&Provider::CommandCode.resume("ab-12", None, module, None)[1..], ["--mod", "/m.ts", "--continue"]);
        assert_eq!(Provider::Claude.resume("ab-12", None, Path::new("/h.json"), None), ["claude", "--resume", "ab-12", "--settings", "/h.json"]);
        assert_eq!(Provider::Claude.resume("ab-12", None, Path::new("/h.json"), Some("plan"))[5..], ["--permission-mode", "plan"]);
        assert_eq!(Provider::OpenCode.resume("ab-12", Some("ses_1"), Path::new("/p.ts"), Some("plan"))[1..], ["--session", "ses_1", "--agent", "plan"]);
        assert_eq!(Provider::Claude.resume("ab-12", Some("cd-34"), Path::new("/h.json"), None)[2], "cd-34", "the conversation since a /clear");
        assert_eq!(serde_json::from_str::<Provider>(r#""command-code""#).unwrap(), Provider::CommandCode);
        assert_eq!(Provider::OpenCode.resume("ab-12", Some("ses_4f2a"), module, None), ["opencode", "--session", "ses_4f2a"]);
        assert_eq!(Provider::OpenCode.resume("ab-12", None, module, None), ["opencode", "--continue"]);
        // A session id the log could carry, made an option: the CLI's own way back instead.
        assert_eq!(Provider::Claude.resume("ab-12", Some("--dangerously-skip-permissions"), Path::new("/h.json"), None)[2], "ab-12");
        assert_eq!(Provider::OpenCode.resume("ab-12", Some("--x"), module, None), ["opencode", "--continue"]);
        assert!(!valid_session("a b") && !valid_session("") && !valid_session("--config=x") && valid_session("019a-ses_4f2a"));
        assert_eq!(serde_json::from_str::<Provider>(r#""opencode""#).unwrap(), Provider::OpenCode);
    }

    #[test]
    fn wires_opencode_through_its_environment() {
        let plugin = Path::new("/data/agents/opencode-plugin.ts");
        assert_eq!(Provider::OpenCode.launch("ab-12", "Fix", plugin, Some("plan"), None, Some("Go")).unwrap(), ["opencode", "--agent", "plan", "--prompt=Go"]);
        assert_eq!(Provider::OpenCode.launch("ab-12", "Fix", plugin, Some("auto"), None, None).unwrap(), ["opencode", "--auto"]);
        assert_eq!(Provider::OpenCode.launch("ab-12", "Fix", plugin, Some("acceptEdits"), None, None).unwrap(), ["opencode"]);
        assert!(Provider::OpenCode.launch("ab-12", "Fix", plugin, Some("yolo"), None, None).is_err());
        let env = Provider::OpenCode.env(plugin, Some("acceptEdits"), None);
        let config: serde_json::Value = serde_json::from_str(env[0].strip_prefix("OPENCODE_CONFIG_CONTENT=").unwrap()).unwrap();
        assert_eq!(config, serde_json::json!({ "plugin": ["file:///data/agents/opencode-plugin.ts"], "permission": { "edit": "allow" } }));
        assert_eq!(Provider::OpenCode.env(plugin, None, None).len(), 1);
        assert!(Provider::Claude.env(plugin, None, None).is_empty());
    }

    #[test]
    fn wires_codex_through_its_command_line() {
        let file = std::env::temp_dir().join(format!("prism-codex-hooks-{}.json", std::process::id()));
        // A key another program added to the file: not Prism's, left out.
        std::fs::write(&file, r#"["hooks.Stop=[]", "approval_policy=\"never\""]"#).unwrap();
        assert_eq!(Provider::Codex.launch("ab-12", "Fix", &file, Some("plan"), None, Some("Go")).unwrap(), ["codex", "-c", "hooks.Stop=[]", "--sandbox", "read-only", "--", "Go"]);
        assert_eq!(&Provider::Codex.launch("ab-12", "Fix", &file, Some("acceptEdits"), None, None).unwrap()[3..], ["--sandbox", "workspace-write"]);
        assert_eq!(&Provider::Codex.launch("ab-12", "Fix", &file, Some("auto"), None, None).unwrap()[3..], ["--approve-for-me"]);
        assert_eq!(Provider::Codex.launch("ab-12", "Fix", &file, Some("default"), None, None).unwrap().len(), 3);
        assert!(Provider::Codex.launch("ab-12", "Fix", &file, Some("yolo"), None, None).is_err());
        assert_eq!(Provider::Codex.resume("ab-12", Some("01a13bc2-2a63"), &file, None), ["codex", "resume", "-c", "hooks.Stop=[]", "01a13bc2-2a63"]);
        assert_eq!(Provider::Codex.resume("ab-12", None, &file, None)[4], "--last");
        assert_eq!(Provider::Codex.launch("ab-12", "Fix", Path::new("/missing.json"), None, None, None).unwrap(), ["codex"], "unreadable: unfollowed");
        assert!(Provider::Codex.env(&file, None, None).is_empty());
        assert_eq!(serde_json::from_str::<Provider>(r#""codex""#).unwrap(), Provider::Codex);
        let _ = std::fs::remove_file(file);
    }
}

/// Claude Code's tools the assistant goes without: it never writes, runs a command nor reads a
/// file itself, nor starts a subagent that would.
pub const ASSISTANT_DENIED: &[&str] = &["Bash", "Edit", "MultiEdit", "Write", "NotebookEdit", "Read", "Grep", "Glob", "LS", "Task", "Agent"];
