//! The subcommands of `prism` and `prism-server`, once: their arguments, their help, which binary
//! has them. Parsed with clap; its errors reworded `<binary>: <message>`, then the usage.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};

/// The binary running: the app (a window) or the server (none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bin {
    App,
    Server,
}

impl Bin {
    pub fn name(self) -> &'static str {
        match self {
            Bin::App => "prism",
            Bin::Server => "prism-server",
        }
    }
}

/// What `--version` says: the binary's version, then its build (its commit).
#[derive(Debug, Clone, Copy)]
pub struct Info {
    pub bin: Bin,
    pub version: &'static str,
    pub build: &'static str,
}

impl Info {
    pub fn version_line(&self) -> String {
        if self.build.is_empty() { self.version.to_string() } else { format!("{} ({})", self.version, self.build) }
    }
}

#[derive(Debug, Parser)]
#[command(max_term_width = 100)]
pub struct Cli {
    /// The profile: `default`, or one of ~/.config/prism/profiles (otherwise PRISM_PROFILE, then the last one opened)
    #[arg(long, global = true, value_name = "ID")]
    pub profile: Option<String>,
    #[command(subcommand)]
    pub command: Option<Cmd>,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Run Prism without a window, until stopped (a systemd service)
    #[command(after_help = "Example:\n  prism-server serve")]
    Serve,
    /// Pair a phone from this terminal: its QR code here, the code it shows confirmed here
    #[command(after_help = "Example:\n  prism pair")]
    Pair,
    /// The other computers that open this Prism in their window
    #[command(subcommand)]
    Peer(Peer),
    /// Restart the profile's Prism, rereading config.toml and its packs (started if not running)
    #[command(after_help = "Example:\n  prism --profile perso restart")]
    Restart,
    /// The packs of config.toml, as in Settings › General › Packs
    #[command(subcommand)]
    Pack(Pack),
    /// The modules of the profile, as in Settings › General › Packs › Modules
    #[command(subcommand)]
    Modules(Modules),
    /// Prism's MCP server for an agent CLI (on stdin/stdout), or its installation into one
    Mcp(Mcp),
    /// Store a token in the keychain: typed hidden on a terminal, read from stdin otherwise
    #[command(after_help = "Examples:\n  prism token clickup\n  pbpaste | prism token jira")]
    Token {
        /// Whose
        service: Service,
    },
    /// Print the completion script of a shell
    Completions {
        shell: clap_complete::Shell,
    },
    /// An agent's hook: its event read on stdin, written to the agent's folder (internal)
    #[command(hide = true)]
    Hook {
        dir: PathBuf,
        /// What some CLIs add after the command: ignored.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        rest: Vec<OsString>,
    },
    /// Start the profile's Prism if it should run and does not (the keep-running job; internal)
    #[command(hide = true)]
    Watch,
    /// The profile's Prism without a window, shown in a window of the app (internal)
    #[command(hide = true)]
    Engine,
    /// Hold the agents' sessions where there is no tmux, beyond the window (internal)
    #[command(hide = true)]
    Sessions,
    /// `token clickup`'s former name
    #[command(name = "clickup-token", hide = true)]
    ClickupToken,
    /// `token jira`'s former name
    #[command(name = "jira-token", hide = true)]
    JiraToken,
    /// `token google`'s former name
    #[command(name = "google-secret", hide = true)]
    GoogleSecret,
}

#[derive(Debug, Subcommand)]
pub enum Peer {
    /// Accept a computer by the code it is given here
    Pair,
    /// Add a computer by name: its token printed, once
    #[command(after_help = "Example:\n  prism peer add laptop")]
    Add { name: String },
    /// Revoke a computer: its windows close within ten seconds
    Remove { name: String },
    /// The computers added, with their date
    List,
}

#[derive(Debug, Subcommand)]
pub enum Pack {
    /// Add a pack: a file, a repository's file at a ref, or a starter
    #[command(after_help = "Examples:\n  prism pack import ~/team.toml\n  prism pack import glab:group/project:prism/pack.toml@main\n  prism pack import gh:owner/repo:pack.toml@v2\n  prism pack import prism:starter")]
    Import {
        /// A file, glab:group/project:path@ref, gh:owner/repo:path@ref, or prism:starter
        source: String,
    },
    /// Update a repository's pack, from its ref or the one given
    #[command(after_help = "Example:\n  prism pack update team v3")]
    Update {
        name: String,
        /// A branch, a tag or a commit
        #[arg(value_name = "REF")]
        reference: Option<String>,
    },
    /// Remove a pack
    Remove { name: String },
}

#[derive(Debug, Subcommand)]
pub enum Modules {
    /// The modules in modules/: their version, where they come from, trusted or not
    List,
    /// Install a module without a pack: a folder or .zip, or a repository's folder at a ref
    #[command(after_help = "Examples:\n  prism modules install ~/code/hello/dist/hello\n  prism modules install gh:owner/repo:dist/hello@v1\n  prism modules install glab:group/project:modules/hello.zip@main\n\nIt waits for your trust in Settings › General › Packs › Modules.")]
    Install {
        /// A folder or .zip, glab:group/project:path@ref or gh:owner/repo:path@ref
        source: String,
    },
    /// Read a module again from its pack or its source, and put it back whole
    Reinstall { id: String },
    /// Remove a module (a pack's stays out of that pack's updates until put back)
    Remove { id: String },
    /// Put back a pack's module you removed
    Restore { id: String },
    /// Let a module that stopped Prism run again (its back end restarts with Prism, or from Settings)
    Reset { id: String },
}

#[derive(Debug, Args)]
#[command(after_help = "Examples:\n  prism mcp install claude\n  prism mcp status")]
pub struct Mcp {
    #[command(subcommand)]
    pub action: Option<McpAction>,
}

#[derive(Debug, Subcommand)]
pub enum McpAction {
    /// Add Prism's MCP server and skill to an agent CLI's user config
    Install { cli: AgentCli },
    /// Take them out
    Uninstall { cli: AgentCli },
    /// Where they are installed
    Status,
    /// The « in passing » mode: two tools only (internal)
    #[command(hide = true)]
    Aside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AgentCli {
    Claude,
    Codex,
    Opencode,
}

impl AgentCli {
    pub fn name(self) -> &'static str {
        match self {
            AgentCli::Claude => "claude",
            AgentCli::Codex => "codex",
            AgentCli::Opencode => "opencode",
        }
    }
}

/// The services whose token goes to the keychain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Service {
    /// ClickUp's personal token
    Clickup,
    /// Jira's API token
    Jira,
    /// The Google OAuth client's secret
    Google,
}

/// The subcommands one binary has not: hidden from its help, refused if typed.
fn absent(bin: Bin) -> &'static [&'static str] {
    match bin {
        Bin::App => &["serve"],
        Bin::Server => &["engine"],
    }
}

/// The command line of `info.bin`: its name, version and help.
pub fn command(info: &Info) -> clap::Command {
    let bin = info.bin.name();
    let about = match info.bin {
        Bin::App => "Prism: your work at a glance, and the agents that do it. Without a subcommand, opens the window (or brings it to the front).",
        Bin::Server => "Prism without a window, for a computer reached from a phone. `serve` runs it.",
    };
    let mut cmd = Cli::command().name(bin).bin_name(bin).version(info.version_line()).about(about).after_help(format!("`{bin} help <command>` or `{bin} <command> --help`: a command's arguments and an example."));
    for name in absent(info.bin) {
        cmd = cmd.mut_subcommand(*name, |s| s.hide(true));
    }
    cmd = cmd.mut_subcommand("completions", |s| s.after_help(format!("Examples:\n  {bin} completions zsh > ~/.zfunc/_{bin}\n  {bin} completions bash > ~/.local/share/bash-completion/completions/{bin}\n  {bin} completions fish > ~/.config/fish/completions/{bin}.fish")));
    named(cmd, bin)
}

/// The examples' `prism …` as this binary's.
fn named(cmd: clap::Command, bin: &str) -> clap::Command {
    let mut cmd = match cmd.get_after_help().map(ToString::to_string) {
        Some(text) if bin != "prism" => {
            let text = text.lines().map(|l| match l.strip_prefix("  prism ") {
                Some(rest) => format!("  {bin} {rest}"),
                None => l.to_string(),
            });
            let text = text.collect::<Vec<_>>().join("\n");
            cmd.after_help(text)
        }
        _ => cmd,
    };
    let names: Vec<String> = cmd.get_subcommands().map(|s| s.get_name().to_string()).collect();
    for name in names {
        cmd = cmd.mut_subcommand(name, |s| named(s, bin));
    }
    cmd
}

/// What the arguments ask.
#[derive(Debug)]
pub enum Parsed {
    /// Run this.
    Run(Cli),
    /// Done already (help, version, an error said): this exit code.
    Exit(i32),
}

/// Arguments macOS adds to an app it opens: not the user's.
fn from_the_system(a: &OsString) -> bool {
    a.to_str().is_some_and(|a| a.starts_with("-psn_"))
}

/// `args` (the binary's name first) parsed for `info.bin`; help, version and errors printed.
pub fn parse(info: &Info, args: Vec<OsString>) -> Parsed {
    let args: Vec<OsString> = args.into_iter().enumerate().filter(|(i, a)| *i == 0 || !from_the_system(a)).map(|(_, a)| a).collect();
    let mut cmd = command(info);
    // Another binary's subcommand, even only its help asked (`prism help serve`, `prism serve -h`):
    // said to be the other's, never shown as this one's.
    let mut words: Vec<&str> = Vec::new();
    let mut rest = args.iter().skip(1).filter_map(|a| a.to_str());
    while let Some(a) = rest.next() {
        if a == "--profile" {
            rest.next();
        } else if !a.starts_with('-') {
            words.push(a);
        }
    }
    let named = match words.as_slice() {
        ["help", w, ..] => Some(*w),
        [w, ..] => Some(*w),
        [] => None,
    };
    let other_s = named.filter(|w| absent(info.bin).contains(w));
    let fail = |e: clap::Error| {
        use clap::error::ErrorKind::*;
        match e.kind() {
            DisplayHelp | DisplayVersion => {
                let _ = e.print();
                Parsed::Exit(0)
            }
            DisplayHelpOnMissingArgumentOrSubcommand => {
                let _ = e.print();
                Parsed::Exit(2)
            }
            _ => {
                eprint!("{}", reworded(info.bin, &e.render().to_string()));
                Parsed::Exit(2)
            }
        }
    };
    if let Some(name) = other_s {
        let other = if info.bin == Bin::App { "prism-server" } else { "prism" };
        return fail(cmd.error(clap::error::ErrorKind::InvalidSubcommand, format!("`{name}` is {other}'s, not {}'s: run `{other} {name}`", info.bin.name())));
    }
    let matches = match cmd.try_get_matches_from_mut(args) {
        Ok(m) => m,
        Err(e) => return fail(e),
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => return fail(e.format(&mut cmd)),
    };
    if let Some(name) = matches.subcommand_name().filter(|n| absent(info.bin).contains(n)) {
        let other = if info.bin == Bin::App { "prism-server" } else { "prism" };
        return fail(cmd.error(clap::error::ErrorKind::InvalidSubcommand, format!("`{name}` is {other}'s, not {}'s: run `{other} {name}`", info.bin.name())));
    }
    Parsed::Run(cli)
}

/// clap's `error: …` as `<bin>: …`.
fn reworded(bin: Bin, text: &str) -> String {
    match text.strip_prefix("error: ") {
        Some(rest) => format!("{}: {rest}", bin.name()),
        None => text.to_string(),
    }
}

/// The table of the guide's CLI section: each public subcommand (and theirs), what it does, which
/// binary has it. Kept in docs/guide.md by a test.
#[cfg(test)]
pub fn guide_table() -> String {
    let app = command(&Info { bin: Bin::App, version: "", build: "" });
    let mut lines = vec!["| Command | What it does | `prism` | `prism-server` |".to_string(), "|---|---|:-:|:-:|".to_string()];
    fn walk(c: &clap::Command, prefix: &str, lines: &mut Vec<String>) {
        for s in c.get_subcommands().filter(|s| s.get_name() != "help") {
            let name = format!("{prefix}{}", s.get_name());
            let hidden = s.is_hide_set() && !(prefix.is_empty() && absent(Bin::App).contains(&s.get_name()));
            if hidden {
                continue;
            }
            let args: String = s.get_positionals().filter(|a| !a.is_hide_set()).map(|a| {
                let v = a.get_value_names().and_then(|v| v.first()).map_or_else(|| a.get_id().to_string().to_uppercase(), ToString::to_string);
                let shown = match a.get_possible_values().iter().map(|p| p.get_name().to_string()).collect::<Vec<_>>() {
                    // `\|`: a bar inside a table's cell, even in code.
                    p if !p.is_empty() && p.len() <= 5 => p.join("\\|"),
                    _ => v.to_lowercase(),
                };
                if a.is_required_set() { format!(" <{shown}>") } else { format!(" [{shown}]") }
            }).collect();
            if s.is_subcommand_required_set() {
                walk(s, &format!("{name} "), lines);
                continue;
            }
            let about = s.get_about().map(|a| a.to_string()).unwrap_or_default();
            let only_server = prefix.is_empty() && absent(Bin::App).contains(&s.get_name());
            lines.push(format!("| `{name}{args}` | {about} | {} | ✓ |", if only_server { "" } else { "✓" }));
            if s.has_subcommands() {
                walk(s, &format!("{name} "), lines);
            }
        }
    }
    walk(&app, "", &mut lines);
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: Info = Info { bin: Bin::App, version: "0.11.0", build: "46bc0f2" };
    const SERVER: Info = Info { bin: Bin::Server, version: "0.11.0", build: "" };

    fn parse_of(info: &Info, list: &[&str]) -> Parsed {
        parse(info, std::iter::once(info.bin.name()).chain(list.iter().copied()).map(Into::into).collect())
    }

    fn exit_of(info: &Info, list: &[&str]) -> Option<i32> {
        match parse_of(info, list) {
            Parsed::Exit(c) => Some(c),
            Parsed::Run(_) => None,
        }
    }

    fn cmd_of(info: &Info, list: &[&str]) -> Option<Cmd> {
        match parse_of(info, list) {
            Parsed::Run(c) => c.command,
            Parsed::Exit(c) => panic!("exit {c} for {list:?}"),
        }
    }

    #[test]
    fn the_definition_holds() {
        command(&APP).debug_assert();
        command(&SERVER).debug_assert();
    }

    #[test]
    fn help_and_version_end_with_zero() {
        for a in [&["--help"][..], &["-h"], &["help"], &["help", "pack"], &["pack", "--help"], &["--version"], &["-V"], &["token", "--help"]] {
            assert_eq!(exit_of(&APP, a), Some(0), "{a:?}");
            assert_eq!(exit_of(&SERVER, a), Some(0), "{a:?}");
        }
    }

    #[test]
    fn a_wrong_use_ends_with_two() {
        for a in [&["nonsense"][..], &["--nope"], &["restart", "foo"], &["pack", "nonsense"], &["hook"], &["token", "slack"], &["--profile"], &["serve"], &["help", "serve"], &["serve", "--help"]] {
            assert_eq!(exit_of(&APP, a), Some(2), "{a:?}");
        }
        assert_eq!(exit_of(&SERVER, &["restart", "foo"]), Some(2));
        // A command missing its subcommand: its help, as a wrong use.
        assert_eq!(exit_of(&APP, &["pack"]), Some(2));
    }

    #[test]
    fn no_subcommand_opens_the_window() {
        assert!(cmd_of(&APP, &[]).is_none());
        assert_eq!(exit_of(&APP, &["--profile", "serve", "restart", "--help"]), Some(0), "a profile named serve");
        assert!(cmd_of(&APP, &["-psn_0_12345"]).is_none(), "macOS's own argument");
        match parse_of(&APP, &["--profile", "perso"]) {
            Parsed::Run(c) => assert_eq!((c.profile.as_deref(), c.command.is_none()), (Some("perso"), true)),
            Parsed::Exit(c) => panic!("{c}"),
        }
    }

    #[test]
    fn subcommands_and_their_arguments() {
        assert!(matches!(cmd_of(&SERVER, &["serve"]), Some(Cmd::Serve)));
        assert!(matches!(cmd_of(&APP, &["engine"]), Some(Cmd::Engine)), "a profile's Prism for a window of the app");
        assert_eq!(exit_of(&SERVER, &["engine"]), Some(2), "the app's alone");
        assert!(matches!(cmd_of(&APP, &["pack", "update", "team"]), Some(Cmd::Pack(Pack::Update { name, reference: None })) if name == "team"));
        assert!(matches!(cmd_of(&APP, &["pack", "update", "team", "v3"]), Some(Cmd::Pack(Pack::Update { reference: Some(r), .. })) if r == "v3"));
        assert!(matches!(cmd_of(&APP, &["modules", "install", "gh:o/r:dist/hello@v1"]), Some(Cmd::Modules(Modules::Install { source })) if source == "gh:o/r:dist/hello@v1"));
        assert!(matches!(cmd_of(&APP, &["modules", "reset", "delivery"]), Some(Cmd::Modules(Modules::Reset { id })) if id == "delivery"));
        assert!(matches!(cmd_of(&APP, &["mcp"]), Some(Cmd::Mcp(Mcp { action: None }))));
        assert!(matches!(cmd_of(&APP, &["mcp", "aside"]), Some(Cmd::Mcp(Mcp { action: Some(McpAction::Aside) }))));
        assert!(matches!(cmd_of(&APP, &["mcp", "install", "codex"]), Some(Cmd::Mcp(Mcp { action: Some(McpAction::Install { cli: AgentCli::Codex }) }))));
        assert!(matches!(cmd_of(&APP, &["token", "google"]), Some(Cmd::Token { service: Service::Google })));
        assert!(matches!(cmd_of(&APP, &["jira-token"]), Some(Cmd::JiraToken)));
        assert!(matches!(cmd_of(&APP, &["watch"]), Some(Cmd::Watch)));
        // A hook: its folder, and whatever a CLI adds after it.
        assert!(matches!(cmd_of(&APP, &["hook", "/tmp/a b", "{\"type\":\"x\"}", "--flag"]), Some(Cmd::Hook { dir, rest }) if dir == std::path::Path::new("/tmp/a b") && rest.len() == 2));
        // The profile anywhere, in both spellings.
        for a in [&["--profile", "perso", "restart"][..], &["restart", "--profile", "perso"], &["--profile=perso", "restart"]] {
            match parse_of(&APP, a) {
                Parsed::Run(c) => assert_eq!(c.profile.as_deref(), Some("perso"), "{a:?}"),
                Parsed::Exit(c) => panic!("{c} {a:?}"),
            }
        }
    }

    #[test]
    fn help_names_the_binary_and_hides_the_internal() {
        let help = command(&SERVER).render_help().to_string();
        assert!(help.contains("Usage: prism-server"), "{help}");
        assert!(help.contains("serve") && help.contains("token"), "{help}");
        for hidden in ["hook", "watch", "sessions", "clickup-token", "jira-token", "google-secret"] {
            assert!(!help.contains(hidden), "{hidden} in {help}");
        }
        let app = command(&APP).render_help().to_string();
        assert!(app.contains("Usage: prism ") && !app.contains("  serve"), "{app}");
        assert_eq!(command(&APP).render_version().trim(), "prism 0.11.0 (46bc0f2)");
        let mut server = command(&SERVER);
        let update = server.find_subcommand_mut("pack").unwrap().find_subcommand_mut("update").unwrap().render_help().to_string();
        assert!(update.contains("  prism-server pack update team v3"), "{update}");
        assert_eq!(reworded(Bin::Server, "error: unrecognized subcommand 'x'\n"), "prism-server: unrecognized subcommand 'x'\n");
    }

    /// docs/guide.md's CLI section is the table of this code (`PRISM_WRITE_GUIDE=1` writes it).
    #[test]
    fn the_guide_lists_the_subcommands() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/guide.md");
        let guide = std::fs::read_to_string(&path).unwrap();
        let (start, end) = ("<!-- cli:start (generated: PRISM_WRITE_GUIDE=1 cargo test -p prism-commands guide) -->\n", "\n<!-- cli:end -->");
        let (a, b) = (guide.find(start).expect("the CLI section's start marker") + start.len(), guide.find(end).expect("its end marker"));
        let table = guide_table();
        if std::env::var_os("PRISM_WRITE_GUIDE").is_some() {
            std::fs::write(&path, format!("{}{table}{}", &guide[..a], &guide[b..])).unwrap();
            return;
        }
        assert_eq!(&guide[a..b], table, "docs/guide.md's CLI table is stale: PRISM_WRITE_GUIDE=1 cargo test -p prism-commands guide");
    }
}
