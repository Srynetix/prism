use std::{collections::HashMap, path::{Path, PathBuf}};

use anyhow::{bail, Context, Result};
use prism_base::tr;

/// Prism's own tmux server (`-L prism`), separate from the user's: its agents
/// outlive Prism there (quitting the app, `just deploy`), and Prism reattaches on return.
/// Prefix of the agents' tmux sessions, followed by the Claude session identifier.
pub use prism_core::agents::PREFIX;
/// An agent's shells (the Shell panel) are addressed as `shell:<agent>:<n>`; their sessions,
/// `prismsh-<agent>-<n>`, have a prefix of their own, so that they never count as agents.
pub const SHELL: &str = "shell:";
const SHELL_PREFIX: &str = "prismsh-";

/// The command line each shell was last given by Prism (a task run from the Tasks panel or this
/// block), by terminal key, and when: a server is named by it rather than by its process.
static SHELL_COMMANDS: std::sync::LazyLock<std::sync::Mutex<HashMap<String, (String, std::time::Instant)>>> = std::sync::LazyLock::new(Default::default);

/// Records the line Prism types into a shell (`Agents::run_in_shell`).
pub fn ran_in_shell(key: &str, command: &str) {
    SHELL_COMMANDS.lock().unwrap().insert(key.to_string(), (command.to_string(), std::time::Instant::now()));
}

/// The lines recorded by `ran_in_shell`, by terminal key, with when each was typed.
pub fn shell_commands() -> HashMap<String, (String, std::time::Instant)> {
    SHELL_COMMANDS.lock().unwrap().clone()
}

/// Forgets the line recorded for a shell (back at its prompt).
pub fn forget_shell_command(key: &str) {
    SHELL_COMMANDS.lock().unwrap().remove(key);
}

/// Config loaded at server startup, instead of `~/.tmux.conf`: tmux stays invisible.
/// No prefix key (Ctrl-B belongs to Claude), Esc without delay (to interrupt Claude),
/// the wheel scrolls the history, and a finished agent keeps its last screen. Focus
/// events go through: Claude Code expects them (without them, it advises enabling them).
/// A mouse selection goes to the clipboard: `set-clipboard external` sends it as OSC 52
/// (tmux knows `xterm*` terminals can receive it), which Prism's terminal handles; a program's own
/// OSC 52 is ignored (`on` passed it on): a commit message or a log printed in a pane cannot fill
/// the clipboard behind the user's back. A drag, a
/// double-click (a word) or a triple-click (a line) copies, then leaves copy mode at the bottom of
/// the pane; in the scrolled history, it keeps the selection in sight, and stays there: a click
/// clears it, a click without one at the bottom leaves, and Prism takes the pane out of it before
/// the next key (`leave_copy_mode`; the terminal says so meanwhile, `in_copy_mode`). The right button no longer opens tmux's menus (split, kill pane…):
/// Prism's terminal shows its own. OSC 8 links (Claude Code's) reach the terminal.
const CONF: &str = "\
set -g status off
set -g prefix None
unbind-key C-b
set -g escape-time 0
set -g history-limit 20000
set -g mouse on
set -g remain-on-exit on
set -g window-size latest
set -g default-terminal tmux-256color
set -as terminal-features ',xterm-256color:RGB'
set -as terminal-features ',xterm-256color:hyperlinks'
set -g set-clipboard external
set -g focus-events on
unbind -n MouseDown3Pane
unbind -n M-MouseDown3Pane
unbind -n MouseDown3Status
unbind -n MouseDown3StatusLeft
unbind -n MouseDown3StatusRight
unbind -n M-MouseDown3Status
unbind -n M-MouseDown3StatusLeft
bind -n DoubleClick1Pane select-pane -t = \\; if -F '#{||:#{pane_in_mode},#{mouse_any_flag}}' { send -M } { copy-mode -H ; send -X select-word ; send -X copy-pipe-and-cancel }
bind -n TripleClick1Pane select-pane -t = \\; if -F '#{||:#{pane_in_mode},#{mouse_any_flag}}' { send -M } { copy-mode -H ; send -X select-line ; send -X copy-pipe-and-cancel }
bind -T copy-mode MouseDown1Pane select-pane \\; if -F '#{||:#{selection_present},#{scroll_position}}' { send -X clear-selection } { send -X cancel }
bind -T copy-mode MouseDragEnd1Pane if -F '#{scroll_position}' { send -X copy-pipe-no-clear } { send -X copy-pipe-and-cancel }
bind -T copy-mode DoubleClick1Pane select-pane \\; send -X select-word \\; if -F '#{scroll_position}' { send -X copy-pipe-no-clear } { send -X copy-pipe-and-cancel }
bind -T copy-mode TripleClick1Pane select-pane \\; send -X select-line \\; if -F '#{scroll_position}' { send -X copy-pipe-no-clear } { send -X copy-pipe-and-cancel }
bind -T copy-mode-vi MouseDown1Pane select-pane \\; if -F '#{||:#{selection_present},#{scroll_position}}' { send -X clear-selection } { send -X cancel }
bind -T copy-mode-vi MouseDragEnd1Pane if -F '#{scroll_position}' { send -X copy-pipe-no-clear } { send -X copy-pipe-and-cancel }
bind -T copy-mode-vi DoubleClick1Pane select-pane \\; send -X select-word \\; if -F '#{scroll_position}' { send -X copy-pipe-no-clear } { send -X copy-pipe-and-cancel }
bind -T copy-mode-vi TripleClick1Pane select-pane \\; send -X select-line \\; if -F '#{scroll_position}' { send -X copy-pipe-no-clear } { send -X copy-pipe-and-cancel }
";

/// A shell's session as `all_shells` reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellSession {
    /// An agent's id, or a worktree's or pod's fingerprint.
    pub owner: String,
    pub n: u32,
    /// The folder it started in.
    pub path: String,
    /// Unix seconds.
    pub created: i64,
    pub command: String,
    /// `@prism-pod`: the pod it reaches (JSON), empty for any other shell.
    pub pod: String,
}

/// Terminal state of an agent's tmux session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pane {
    Alive,
    /// Claude has exited; the screen stays displayed (`remain-on-exit`).
    Exited(i32),
}

/// What `send-keys` would type, as bytes: its key names (`Enter`, `Escape`, `C-c`, the arrows) or,
/// after `-l`, its text. None for a key it does not know.
fn key_bytes(keys: &[&str]) -> Option<String> {
    let mut out = String::new();
    let mut literal = false;
    for k in keys {
        if *k == "-l" {
            literal = true;
            continue;
        }
        if literal {
            out.push_str(k);
            continue;
        }
        let b = match *k {
            "Enter" => "\r".to_string(),
            "Escape" => "\x1b".to_string(),
            "Tab" => "\t".to_string(),
            "BTab" => "\x1b[Z".to_string(),
            "Space" => " ".to_string(),
            "BSpace" => "\x7f".to_string(),
            "Up" => "\x1b[A".to_string(),
            "Down" => "\x1b[B".to_string(),
            "Right" => "\x1b[C".to_string(),
            "Left" => "\x1b[D".to_string(),
            k if k.len() == 3 && k.starts_with("C-") && k.as_bytes()[2].is_ascii_alphabetic() => ((k.as_bytes()[2].to_ascii_lowercase() - b'a' + 1) as char).to_string(),
            k if k.chars().count() == 1 => k.to_string(),
            _ => return None,
        };
        out.push_str(&b);
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
pub mod fake;

/// A text as it may be pasted: its lines and tabs, no other control character. Text from
/// elsewhere (a task's output, a quoted file, a phone) could carry the end of the bracketed paste
/// (`ESC [201~`) and keys after it, which tmux before 3.7 passed on as typed.
pub fn pasteable(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n").chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\t')).collect()
}

pub struct Tmux {
    conf: PathBuf,
    socket: String,
    /// The tests' server in memory, in place of the binary.
    #[cfg(test)]
    fake: Option<std::sync::Arc<fake::Server>>,
}

impl Tmux {
    pub fn new(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let conf = dir.join("tmux.conf");
        std::fs::write(&conf, CONF).with_context(|| format!("writing {}", conf.display()))?;
        Ok(Self {
            conf,
            socket: crate::profile::tmux_socket(),
            #[cfg(test)]
            fake: None,
        })
    }

    /// A tmux whose commands go to `server`.
    #[cfg(test)]
    pub fn fake(dir: &Path, server: std::sync::Arc<fake::Server>) -> Result<Self> {
        Ok(Self { fake: Some(server), ..Self::new(dir)? })
    }

    /// The tmux session of an agent, or of one of its shells (`shell:<agent>:<n>`).
    pub fn session(key: &str) -> String {
        match key.strip_prefix(SHELL) {
            Some(shell) => format!("{SHELL_PREFIX}{}", shell.replace(':', "-")),
            None => format!("{PREFIX}{key}"),
        }
    }

    /// The agent's shells still open, by number, with the program in their foreground.
    pub async fn shells(&self, id: &str) -> Result<Vec<(u32, String)>> {
        let out = match self.run(&["list-panes", "-a", "-F", "#{session_name}\t#{pane_current_command}"]).await {
            Ok(out) => out,
            Err(e) if is_no_server(&e) => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let prefix = format!("{SHELL_PREFIX}{id}-");
        let mut shells: Vec<(u32, String)> = out
            .lines()
            .filter_map(|l| {
                let (name, command) = l.split_once('\t')?;
                Some((name.strip_prefix(&prefix)?.parse().ok()?, command.to_string()))
            })
            .collect();
        shells.sort();
        Ok(shells)
    }

    /// Every shell of the server with its owner (`prismsh-<owner>-<n>`), whoever owns it: the folder
    /// it started in, when (Unix seconds), the program in its foreground, its `@prism-pod`.
    pub async fn all_shells(&self) -> Result<Vec<ShellSession>> {
        let out = match self.run(&["list-panes", "-a", "-F", "#{session_name}\t#{session_path}\t#{session_created}\t#{pane_current_command}\t#{@prism-pod}"]).await {
            Ok(out) => out,
            Err(e) if is_no_server(&e) => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        Ok(parse_shells(&out))
    }

    /// Sets a user option (`@<name>`) on a session: what Prism notes about it, read back by `all_shells`.
    pub async fn set_note(&self, key: &str, name: &str, value: &str) -> Result<()> {
        self.run(&["set-option", "-t", &Self::session(key), &format!("@{name}"), value]).await.map(drop)
    }

    /// Common arguments: Prism's server, its config, and forced UTF-8 (an app launched from
    /// the Finder has no `LANG`, and tmux would replace accents with `_`).
    pub fn base_args(&self) -> Vec<String> {
        vec!["-u".into(), "-L".into(), self.socket.clone(), "-f".into(), self.conf.display().to_string()]
    }

    async fn run(&self, args: &[&str]) -> Result<String> {
        #[cfg(test)]
        if let Some(server) = &self.fake {
            return server.run(args);
        }
        let out = crate::platform::command("tmux")
            .args(self.base_args())
            .args(args)
            .output()
            .await
            .with_context(|| tr!("tmux not found ({hint})", hint = crate::platform::install_hint("tmux")))?;
        if !out.status.success() {
            bail!("tmux {}: {}", args.first().unwrap_or(&""), String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// `run` with `input` on tmux's standard input: a buffer's text (`load-buffer -`), whole. In
    /// its arguments, tmux refuses a command over ~16 KB ("command too long"): a long prompt.
    async fn run_input(&self, args: &[&str], input: &str) -> Result<String> {
        #[cfg(test)]
        if let Some(server) = &self.fake {
            return server.run(args);
        }
        use std::process::Stdio;
        use tokio::io::AsyncWriteExt;
        let mut child = crate::platform::command("tmux")
            .args(self.base_args())
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| tr!("tmux not found ({hint})", hint = crate::platform::install_hint("tmux")))?;
        let mut stdin = child.stdin.take().context("tmux's stdin")?;
        stdin.write_all(input.as_bytes()).await?;
        drop(stdin);
        let out = child.wait_with_output().await?;
        if !out.status.success() {
            bail!("tmux {}: {}", args.first().unwrap_or(&""), String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Reloads the config in an already running server (it outlives Prism, and so its
    /// updates); with no server, nothing to do: the next one will read it at startup.
    /// Applies the config to a server already running (agents outlive Prism), and gives it Prism's
    /// locale (see `shell_env::ensure_utf8_locale`) and profile (`PRISM_PROFILE`, which a server
    /// started by a Prism that had inherited another profile's got wrong): its next panes get
    /// them; those already open keep theirs until restarted.
    pub async fn reload_conf(&self) -> Result<()> {
        let conf = self.conf.display().to_string();
        match self.run(&["source-file", &conf]).await {
            Err(e) if !is_no_server(&e) => return Err(e),
            Err(_) => return Ok(()),
            Ok(_) => {}
        }
        self.run(&["set-environment", "-g", "PRISM_PROFILE", crate::profile::active()]).await?;
        if let Ok(lang) = std::env::var("LANG") {
            self.run(&["set-environment", "-g", "LANG", &lang]).await?;
            for var in ["LC_ALL", "LC_CTYPE"] {
                let _ = self.run(&["set-environment", "-gu", var]).await;
            }
        }
        Ok(())
    }

    /// New detached session that runs `argv` directly (no shell: no quotes
    /// to escape in the prompt), in `cwd`, with `env` on top of the server's environment.
    pub async fn new_session(&self, id: &str, cwd: &Path, env: &[String], argv: &[String]) -> Result<()> {
        let name = Self::session(id);
        let cwd = cwd.display().to_string();
        let mut args = vec!["new-session", "-d", "-s", &name, "-x", "120", "-y", "40", "-c", &cwd, "-e", "COLORTERM=truecolor"];
        args.extend(env.iter().flat_map(|e| ["-e", e.as_str()]));
        args.push("--");
        args.extend(argv.iter().map(String::as_str));
        #[cfg(test)]
        if let Some(server) = &self.fake {
            return server.run(&args).map(drop);
        }
        // macOS: the first session starts the server, which outlives Prism with the agents: it
        // starts responsible for itself, so that the system does not count it as Prism's.
        #[cfg(target_os = "macos")]
        {
            let all: Vec<String> = self.base_args().into_iter().chain(args.iter().map(|a| a.to_string())).collect();
            let (code, err) = tokio::task::spawn_blocking(move || crate::platform::run_independent("tmux", &all)).await??;
            if code != 0 {
                bail!("tmux new-session: {}", err.trim());
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        self.run(&args).await.map(drop)
    }

    /// Replaces the session's process (`-k`: even if it is still running) with `argv`, in `cwd`.
    /// The session and the terminals attached to it stay: they see Claude start again.
    pub async fn respawn(&self, id: &str, cwd: &Path, env: &[String], argv: &[String]) -> Result<()> {
        let name = Self::session(id);
        let cwd = cwd.display().to_string();
        let mut args = vec!["respawn-pane", "-k", "-t", &name, "-c", &cwd, "-e", "COLORTERM=truecolor"];
        args.extend(env.iter().flat_map(|e| ["-e", e.as_str()]));
        args.push("--");
        args.extend(argv.iter().map(String::as_str));
        self.run(&args).await.map(drop)
    }

    /// Agent sessions of the server, by identifier. With no server (no agent), nothing.
    pub async fn panes(&self) -> Result<HashMap<String, Pane>> {
        let out = match self.run(&["list-panes", "-a", "-F", "#{session_name}\t#{pane_dead}\t#{pane_dead_status}"]).await {
            Ok(out) => out,
            Err(e) if is_no_server(&e) => return Ok(HashMap::new()),
            Err(e) => return Err(e),
        };
        Ok(parse_panes(&out))
    }

    /// The state of one session's pane and the program in the foreground there; `None`: no such session.
    pub async fn pane(&self, key: &str) -> Result<Option<(Pane, String)>> {
        let out = match self.run(&["list-panes", "-t", &Self::session(key), "-F", "#{pane_dead}\t#{pane_dead_status}\t#{pane_current_command}"]).await {
            Ok(out) => out,
            // `no current target`: its session gone while the server exits.
            Err(e) if is_no_server(&e) || ["can't find", "no current target"].iter().any(|m| e.to_string().contains(m)) => return Ok(None),
            Err(e) => return Err(e),
        };
        let mut parts = out.lines().next().unwrap_or_default().split('\t');
        let dead = parts.next() == Some("1");
        let status = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let command = parts.next().unwrap_or_default().to_string();
        Ok(Some((if dead { Pane::Exited(status) } else { Pane::Alive }, command)))
    }

    /// Unlike an agent's, a shell's session ends with its shell (`exit`): no last screen to keep.
    pub async fn end_with_program(&self, key: &str) -> Result<()> {
        self.run(&["set-option", "-t", &Self::session(key), "remain-on-exit", "off"]).await.map(drop)
    }

    pub async fn send_keys(&self, id: &str, keys: &[&str]) -> Result<()> {
        let name = Self::session(id);
        let mut args = vec!["send-keys", "-t", &name];
        args.extend(keys);
        match self.run(&args).await {
            // A phone watching (a read-only client, `watch_argv`): tmux 3.5+ refuses `send-keys`
            // then, whatever the target. The same bytes, pasted raw (`-S`), still reach the program.
            Err(e) if e.to_string().contains("client is read-only") => {
                let bytes = key_bytes(keys).with_context(|| format!("{e:#}"))?;
                let buffer = format!("prism-keys-{id}");
                self.run(&["set-buffer", "-b", &buffer, "--", &bytes]).await?;
                self.run(&["paste-buffer", "-S", "-d", "-b", &buffer, "-t", &name]).await.map(drop)
            }
            r => r.map(drop),
        }
    }

    /// Pastes `text` in the pane as one block (bracketed paste when the program asks for it): a
    /// multi-line prompt arrives whole instead of being sent at its first line break.
    pub async fn paste(&self, id: &str, text: &str) -> Result<()> {
        let name = Self::session(id);
        let buffer = format!("prism-{id}");
        // On its stdin, not in its arguments: a long prompt too.
        self.run_input(&["load-buffer", "-b", &buffer, "-"], &pasteable(text)).await?;
        self.run(&["paste-buffer", "-p", "-d", "-b", &buffer, "-t", &name]).await.map(drop)
    }

    /// Takes the pane out of copy mode (a selection kept in sight, the history scrolled), if it is
    /// in it: the next key goes to the program, not to the selection.
    pub async fn leave_copy_mode(&self, id: &str) -> Result<()> {
        let name = Self::session(id);
        let cancel = format!("send-keys -t {} -X cancel", crate::hook::quote(&name));
        self.run(&["if-shell", "-F", "-t", &name, "#{pane_in_mode}", &cancel]).await.map(drop)
    }

    /// The pane is in copy mode: its view stays still until it leaves it.
    pub async fn in_copy_mode(&self, id: &str) -> Result<bool> {
        Ok(self.run(&["display-message", "-p", "-t", &Self::session(id), "#{pane_in_mode}"]).await?.trim() == "1")
    }

    pub async fn kill(&self, id: &str) -> Result<()> {
        match self.run(&["kill-session", "-t", &Self::session(id)]).await {
            Err(e) if !is_no_server(&e) && !e.to_string().contains("can't find session") => Err(e),
            _ => Ok(()),
        }
    }

    /// Is tmux in the PATH?
    pub async fn installed() -> bool {
        crate::platform::command("tmux").arg("-V").output().await.is_ok_and(|o| o.status.success())
    }

    /// Command that attaches a read-only client to the agent's session, for a phone watching. It
    /// never types. `sized`: its size counts (`-f read-only`), and as the latest client it gives
    /// the window the phone's size, until the desk types or it detaches (`window-size latest`);
    /// otherwise `-r` (`read-only,ignore-size`) leaves the window as it is.
    pub fn watch_argv(&self, id: &str, sized: bool) -> Vec<String> {
        let mut argv = vec!["tmux".to_string()];
        argv.extend(self.base_args());
        let flags: &[&str] = if sized { &["-f", "read-only"] } else { &["-r"] };
        argv.push("attach-session".into());
        argv.extend(flags.iter().map(|f| f.to_string()));
        argv.extend(["-t".into(), Self::session(id)]);
        argv
    }

    /// The pane's history and screen as text, with its colours (`-e`), wrapped lines joined: the
    /// last `lines` of it. Without `screen`, the history only: what scrolled off the screen.
    pub async fn history(&self, id: &str, lines: u32, screen: bool) -> Result<String> {
        let start = format!("-{lines}");
        let session = Self::session(id);
        let mut args = vec!["capture-pane", "-p", "-e", "-J", "-t", &session, "-S", &start];
        if !screen {
            args.extend(["-E", "-1"]);
        }
        self.run(&args).await
    }

    /// The pane's visible screen as text, with its colours (`-e`).
    pub async fn screen(&self, id: &str) -> Result<String> {
        self.run(&["capture-pane", "-p", "-e", "-t", &Self::session(id)]).await
    }

    /// Several agents' screens, in one tmux call (a process each was 6.5 ms, one after the other
    /// every refresh for each agent at its prompt): `display-message` names the next screen, then
    /// `capture-pane` gives it. tmux stops at a command that fails (a session gone meanwhile):
    /// those not reached are asked one by one. The ids without a screen are left out.
    pub async fn screens(&self, ids: &[String]) -> HashMap<String, String> {
        let mut out = HashMap::new();
        if ids.is_empty() {
            return out;
        }
        let sessions: Vec<String> = ids.iter().map(|id| Self::session(id)).collect();
        let marks: Vec<String> = ids.iter().map(|id| format!("{SCREEN_MARK}{id}")).collect();
        let mut args: Vec<&str> = Vec::new();
        for (session, mark) in sessions.iter().zip(&marks) {
            if !args.is_empty() {
                args.push(";");
            }
            args.extend(["display-message", "-p", mark.as_str(), ";", "capture-pane", "-p", "-e", "-t", session.as_str()]);
        }
        // The tests' fake server takes one command a call: one by one there.
        #[cfg(test)]
        if self.fake.is_some() {
            args.clear();
        }
        if !args.is_empty()
            && let Ok(text) = self.run(&args).await
        {
            out = split_screens(&text);
        }
        let missing: Vec<&String> = ids.iter().filter(|id| !out.contains_key(*id)).collect();
        for id in missing {
            if let Ok(s) = self.screen(id).await {
                out.insert(id.clone(), s);
            }
        }
        out
    }

    /// The agent's window, in columns and rows.
    pub async fn window_size(&self, id: &str) -> Result<(u16, u16)> {
        let out = self.run(&["display-message", "-p", "-t", &Self::session(id), "#{window_width} #{window_height}"]).await?;
        let mut it = out.split_whitespace().map(|n| n.parse::<u16>().unwrap_or(0));
        Ok((it.next().unwrap_or(80).max(20), it.next().unwrap_or(24).max(5)))
    }

    /// Command that attaches a terminal to the agent's session.
    pub fn attach_argv(&self, id: &str) -> Vec<String> {
        let mut argv = vec!["tmux".to_string()];
        argv.extend(self.base_args());
        argv.extend(["attach-session".into(), "-t".into(), Self::session(id)]);
        argv
    }
}

/// No server to answer: none running, or the one asked stopping meanwhile (its last session
/// gone, tmux answers one of the last two while it exits).
fn is_no_server(e: &anyhow::Error) -> bool {
    let msg = e.to_string();
    ["no server running", "error connecting", "server exited unexpectedly", "lost server"].iter().any(|m| msg.contains(m))
}

fn parse_shells(out: &str) -> Vec<ShellSession> {
    out.lines()
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let (owner, n) = parts.next()?.strip_prefix(SHELL_PREFIX)?.rsplit_once('-')?;
            let (path, created, command, pod) = (parts.next()?, parts.next()?, parts.next()?, parts.next().unwrap_or_default());
            Some(ShellSession { owner: owner.to_string(), n: n.parse().ok()?, path: path.to_string(), created: created.parse().unwrap_or(0), command: command.to_string(), pod: pod.to_string() })
        })
        .collect()
}

fn parse_panes(out: &str) -> HashMap<String, Pane> {
    out.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let id = parts.next()?.strip_prefix(PREFIX)?.to_string();
            let dead = parts.next()? == "1";
            let status = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            Some((id, if dead { Pane::Exited(status) } else { Pane::Alive }))
        })
        .collect()
}

/// What names the next screen in `screens`' output: no screen line starts with it.
const SCREEN_MARK: &str = "\u{1}prism-screen ";

/// `screens`' output, by agent: each marker line, then that screen's lines up to the next one.
fn split_screens(text: &str) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    let mut current: Option<String> = None;
    for line in text.split_inclusive('\n') {
        if let Some(id) = line.strip_prefix(SCREEN_MARK) {
            let id = id.trim_end_matches('\n').to_string();
            out.insert(id.clone(), String::new());
            current = Some(id);
        } else if let Some(screen) = current.as_ref().and_then(|id| out.get_mut(id)) {
            screen.push_str(line);
        }
    }
    out
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    #[test]
    fn splits_the_screens_of_one_call() {
        let text = format!("{SCREEN_MARK}a\n> hi\n\n  ? for shortcuts\n{SCREEN_MARK}b\nline\n");
        let screens = split_screens(&text);
        assert_eq!(screens["a"], "> hi\n\n  ? for shortcuts\n");
        assert_eq!(screens["b"], "line\n");
        assert!(split_screens("").is_empty());
    }

    /// Shells only, by owner and number (an owner's own dashes kept); a pod's note read whole.
    #[test]
    fn reads_every_shell() {
        let out = "prism-abc\t/w\t1\tclaude\t\nprismsh-wt00ff-2\t/w/a-b\t1700000000\tzsh\t\nprismsh-kp11-1\t/Users/me\t5\tkubectl\t{\"pod\":\"x\"}\nprismsh-0a-b-c-3\t/x\t7\tnpm\t\n";
        let shells = parse_shells(out);
        assert_eq!(shells.len(), 3);
        assert_eq!((shells[0].owner.as_str(), shells[0].n, shells[0].path.as_str(), shells[0].created), ("wt00ff", 2, "/w/a-b", 1_700_000_000));
        assert_eq!((shells[1].command.as_str(), shells[1].pod.as_str()), ("kubectl", "{\"pod\":\"x\"}"));
        assert_eq!((shells[2].owner.as_str(), shells[2].n), ("0a-b-c", 3));
    }

    /// No key hides in a pasted text: the bracketed paste's end and what follows it stay text.
    #[test]
    fn pastes_no_control_character() {
        assert_eq!(pasteable("ok\x1b[201~\x03rm -rf ~\r\nnext\tcol\u{9b}x"), "ok[201~rm -rf ~\nnext\tcolx");
    }

    /// Throwaway tmux server, so as never to touch the real agents during tests. Dropped, it
    /// stops its server and removes its socket and folder, even when the test fails.
    pub struct Scratch {
        pub tmux: Tmux,
        pub dir: PathBuf,
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::process::Command::new("tmux").args(["-L", &self.tmux.socket, "kill-server"]).output();
            // tmux leaves its socket behind: `$TMUX_TMPDIR/tmux-<uid>/<name>`.
            #[cfg(unix)]
            let _ = std::fs::remove_file(std::env::var_os("TMUX_TMPDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp")).join(format!("tmux-{}", prism_sys::process::uid())).join(&self.tmux.socket));
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    pub fn scratch(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("prism-tmux-{name}-{}", std::process::id()));
        let mut tmux = Tmux::new(&dir).unwrap();
        tmux.socket = format!("prism-test-{name}-{}", std::process::id());
        // A session with no program runs the default shell: `/bin/sh`, not the user's login shell,
        // whose dotfiles (completion, prompt, plugins) took more than 5 s to start under a full test run.
        let conf = std::fs::read_to_string(&tmux.conf).unwrap();
        std::fs::write(&tmux.conf, format!("{conf}set -g default-shell /bin/sh\n")).unwrap();
        Scratch { tmux, dir }
    }

    #[test]
    fn a_server_exiting_is_no_server() {
        for msg in ["tmux list-panes: no server running on /tmp/tmux-501/prism", "tmux list-panes: server exited unexpectedly", "tmux list-panes: lost server"] {
            assert!(is_no_server(&anyhow::anyhow!("{msg}")), "{msg}");
        }
        assert!(!is_no_server(&anyhow::anyhow!("tmux send-keys: can't find pane")));
    }

    #[test]
    fn keeps_only_agent_sessions() {
        let panes = parse_panes("prism-abc\t0\t\nprism-def\t1\t130\nwork\t0\t\nprismsh-abc\t0\t\n");
        assert_eq!(panes.len(), 2);
        assert_eq!(panes["abc"], Pane::Alive);
        assert_eq!(panes["def"], Pane::Exited(130));
    }

    /// Restarting replaces the process in place: a dead pane comes back to life, without a new session.
    #[tokio::test]
    async fn respawn_keeps_the_session() {
        if std::process::Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let s = scratch("respawn");
        let (tmux, dir) = (&s.tmux, &s.dir);
        tmux.new_session("a1", dir, &[], &["sh".into(), "-c".into(), "exit 3".into()]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(tmux.panes().await.unwrap().get("a1"), Some(&Pane::Exited(3)));
        tmux.respawn("a1", dir, &[], &["sleep".into(), "30".into()]).await.unwrap();
        let panes = tmux.panes().await.unwrap();
        assert_eq!((panes.len(), panes.get("a1")), (1, Some(&Pane::Alive)));
        // Even when alive (`-k`).
        tmux.respawn("a1", dir, &[], &["sleep".into(), "31".into()]).await.unwrap();
        assert_eq!(tmux.panes().await.unwrap().get("a1"), Some(&Pane::Alive));
        tmux.kill("a1").await.unwrap();
    }

    /// A shell: the default one when no program is given, its foreground program seen, and its
    /// session gone with an `exit`.
    #[tokio::test]
    async fn shell_session_follows_its_shell() {
        if std::process::Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let s = scratch("shell");
        let (tmux, dir) = (&s.tmux, &s.dir);
        let key = format!("{SHELL}a1:2");
        tmux.new_session(&key, dir, &[], &[]).await.unwrap();
        tmux.end_with_program(&key).await.unwrap();
        assert!(tmux.panes().await.unwrap().is_empty(), "a shell is not an agent");
        assert_eq!(tmux.shells("a1").await.unwrap().iter().map(|(n, _)| *n).collect::<Vec<_>>(), [2]);
        let foreground = || async { tmux.pane(&key).await.unwrap().map(|(_, cmd)| cmd) };
        let wait_for = |want: Option<&'static str>| async move {
            // Returns as soon as it is there: the margin only costs a loaded machine.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                let now = foreground().await;
                if now.as_deref() == want || std::time::Instant::now() > deadline {
                    return now;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        };
        tmux.send_keys(&key, &["-l", "sleep 30"]).await.unwrap();
        tmux.send_keys(&key, &["Enter"]).await.unwrap();
        assert_eq!(wait_for(Some("sleep")).await.as_deref(), Some("sleep"));
        tmux.send_keys(&key, &["C-c"]).await.unwrap();
        tmux.send_keys(&key, &["-l", "exit"]).await.unwrap();
        tmux.send_keys(&key, &["Enter"]).await.unwrap();
        assert_eq!(wait_for(None).await, None);
    }

    /// A phone's client sees the screen, but neither types into the program nor resizes its
    /// window; the desk's client does both.
    /// A prompt over tmux's ~16 KB command limit is pasted whole.
    #[tokio::test]
    async fn pastes_a_long_text() {
        if std::process::Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let s = scratch("long");
        let (tmux, dir) = (&s.tmux, &s.dir);
        let typed = dir.join("typed.txt");
        tmux.new_session("a1", dir, &[], &["sh".into(), "-c".into(), format!("cat > {}", typed.display())]).await.unwrap();
        // Short lines: the terminal's line discipline cuts a line past ~1 KB, tmux has nothing to do with it.
        let text: String = (0..1000).map(|i| format!("line {i:04} of a long prompt, pasted whole\n")).collect();
        assert!(text.len() > 40_000);
        tmux.paste("a1", &text).await.unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::fs::read_to_string(&typed).unwrap_or_default().len() < text.len() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert_eq!(std::fs::read_to_string(&typed).unwrap(), text);
        tmux.kill("a1").await.unwrap();
    }

    #[tokio::test]
    async fn a_watching_client_reads_only() {
        if std::process::Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let s = scratch("watch");
        let (tmux, dir) = (&s.tmux, &s.dir);
        let typed = dir.join("typed.txt");
        let script = format!("echo ready; cat > {}", typed.display());
        tmux.new_session("a1", dir, &[], &["sh".into(), "-c".into(), script]).await.unwrap();
        let before = tmux.window_size("a1").await.unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
        let sink = seen.clone();
        // At the window's size, as `Agents::watch` opens it: alone, tmux would size the window to it.
        let mut phone = crate::pty::Attachment::spawn("phone", &tmux.watch_argv("a1", false), before.0, before.1, 1, move |b| sink.lock().unwrap().extend(b), || {}).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !String::from_utf8_lossy(&seen.lock().unwrap()).contains("ready") && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(String::from_utf8_lossy(&seen.lock().unwrap()).contains("ready"), "the phone sees the screen");
        phone.write(b"from the phone\r").unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert_eq!(tmux.window_size("a1").await.unwrap(), before, "the window keeps the desk's size");
        assert_eq!(std::fs::read_to_string(&typed).unwrap_or_default(), "", "nothing typed reaches the program");
        drop(phone);
        // The desk's client, for comparison: what it types arrives.
        let mut desk = crate::pty::Attachment::spawn("desk", &tmux.attach_argv("a1"), 80, 24, 2, |_| {}, || {}).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        desk.write(b"from the desk\r").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !std::fs::read_to_string(&typed).unwrap_or_default().contains("from the desk") && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(std::fs::read_to_string(&typed).unwrap_or_default(), "from the desk\n");
        drop(desk);
        tmux.kill("a1").await.unwrap();
    }

    /// A phone watching at its own size: the window takes it while the phone is the latest client,
    /// nothing it sends reaches the program, and the desk gets its size back when it types.
    #[tokio::test]
    async fn a_sized_watching_client_resizes_but_reads_only() {
        if std::process::Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let s = scratch("sized");
        let (tmux, dir) = (&s.tmux, &s.dir);
        let typed = dir.join("typed.txt");
        let script = format!("echo ready; cat > {}", typed.display());
        tmux.new_session("a1", dir, &[], &["sh".into(), "-c".into(), script]).await.unwrap();
        let mut desk = crate::pty::Attachment::spawn("desk", &tmux.attach_argv("a1"), 120, 40, 1, |_| {}, || {}).unwrap();
        let settle = |want: (u16, u16)| async move {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while tmux.window_size("a1").await.unwrap() != want && std::time::Instant::now() < deadline {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            tmux.window_size("a1").await.unwrap()
        };
        assert_eq!(settle((120, 40)).await, (120, 40), "the desk's size");
        let mut phone = crate::pty::Attachment::spawn("phone", &tmux.watch_argv("a1", true), 50, 30, 2, |_| {}, || {}).unwrap();
        assert_eq!(settle((50, 30)).await, (50, 30), "the phone's size while it watches");
        phone.write(b"from the phone\r").unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert_eq!(std::fs::read_to_string(&typed).unwrap_or_default(), "", "nothing typed reaches the program");
        desk.write(b"from the desk\r").unwrap();
        assert_eq!(settle((120, 40)).await, (120, 40), "the desk types: its size again");
        drop(phone);
        drop(desk);
        tmux.kill("a1").await.unwrap();
    }

    #[test]
    fn keys_as_bytes() {
        assert_eq!(key_bytes(&["Enter"]).as_deref(), Some("\r"));
        assert_eq!(key_bytes(&["Escape", "Escape"]).as_deref(), Some("\x1b\x1b"));
        assert_eq!(key_bytes(&["C-c"]).as_deref(), Some("\x03"));
        assert_eq!(key_bytes(&["1"]).as_deref(), Some("1"));
        assert_eq!(key_bytes(&["-l", "/compact"]).as_deref(), Some("/compact"));
        assert_eq!(key_bytes(&["Up", "BTab"]).as_deref(), Some("\x1b[A\x1b[Z"));
        assert_eq!(key_bytes(&["F5"]), None);
    }

    /// A phone watching (a read-only client): tmux refuses `send-keys`, yet the text and its Enter
    /// reach the program — the phone's answer box, its quick answers, its 1 2 3 and Esc.
    #[tokio::test]
    async fn keys_reach_the_program_while_a_phone_watches() {
        if std::process::Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let s = scratch("watched");
        let (tmux, dir) = (&s.tmux, &s.dir);
        let typed = dir.join("typed.txt");
        tmux.new_session("a1", dir, &[], &["sh".into(), "-c".into(), format!("echo ready; cat > {}", typed.display())]).await.unwrap();
        let (cols, rows) = tmux.window_size("a1").await.unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
        let sink = seen.clone();
        let phone = crate::pty::Attachment::spawn("phone", &tmux.watch_argv("a1", true), cols, rows, 1, move |b| sink.lock().unwrap().extend(b), || {}).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !String::from_utf8_lossy(&seen.lock().unwrap()).contains("ready") && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        tmux.paste("a1", "from the box").await.unwrap();
        tmux.send_keys("a1", &["Enter"]).await.unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !std::fs::read_to_string(&typed).unwrap_or_default().contains('\n') && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(std::fs::read_to_string(&typed).unwrap_or_default(), "from the box\n");
        drop(phone);
        tmux.kill("a1").await.unwrap();
    }

    /// A text from a phone may start with a dash: it is pasted as is, not read as tmux's option.
    #[tokio::test]
    async fn a_pasted_text_may_start_with_a_dash() {
        if std::process::Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let s = scratch("dash");
        let (tmux, dir) = (&s.tmux, &s.dir);
        let typed = dir.join("typed.txt");
        tmux.new_session("a1", dir, &[], &["sh".into(), "-c".into(), format!("cat > {}", typed.display())]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        tmux.paste("a1", "-b oops").await.unwrap();
        tmux.send_keys("a1", &["Enter"]).await.unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !std::fs::read_to_string(&typed).unwrap_or_default().contains("oops") && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(std::fs::read_to_string(&typed).unwrap_or_default(), "-b oops\n");
        tmux.kill("a1").await.unwrap();
    }
}
