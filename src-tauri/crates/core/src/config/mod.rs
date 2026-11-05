//! The config: `config.toml` over its packs, one table per feature (see `settings` for
//! Settings' tree). The tables live by domain in this module's files.

use prism_base::{i18n::tr, tr};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

mod ui;
mod repos;
mod tracker;
mod forge;
mod cluster;
mod integrations;
mod migrate;
mod remote;

pub use ui::*;
pub use repos::*;
pub use tracker::*;
pub use forge::*;
pub use cluster::*;
pub use integrations::*;
pub use migrate::*;
pub use remote::*;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// The merged tables as read (packs, then `config.toml`): the blocks read theirs there (`blocks`).
    #[serde(skip)]
    pub table: toml::Table,
    /// Packs this file builds on (`packs/<name>.toml`), read first, in order: see `packs`.
    #[serde(default)]
    pub packs: Vec<String>,
    /// The profile this file belongs to: its name and color (see `profile`).
    #[serde(default)]
    pub profile: ProfileConfig,
    /// The demo profile: sample data in every block, no source read (see `demo`).
    #[serde(default)]
    pub demo: DemoConfig,
    /// Access to the clusters: the switch of the Kubernetes collectors, and their default context.
    /// `[runners]`' keys move here (see [`migrate`]).
    #[serde(default)]
    pub clusters: ClustersConfig,
    /// The pods followed (`[[pods]]`): a cluster's namespaces each. The former `[runners]
    /// namespace` is one (see [`migrate`]).
    #[serde(default)]
    pub pods: Vec<PodsTarget>,
    /// The unread and flagged messages of the Microsoft 365 or Gmail inbox (`[mail]`).
    #[serde(default)]
    pub mail: MailConfig,
    /// The Google OAuth client Gmail and Google Calendar sign in with (`[google]`).
    #[serde(default)]
    pub google: GoogleConfig,
    #[serde(default)]
    pub worktrees: WorktreesConfig,
    /// The forge holding the repositories, GitLab (default) or GitHub, and the access to it and
    /// through it to the repository: its switch also turns off everything that reads the local repository.
    #[serde(default)]
    pub forge: ForgeConfig,
    /// What GitLab alone has: its instance, CI health, merge queue (a block may read more keys
    /// there).
    #[serde(default)]
    pub gitlab: GitlabConfig,
    /// ClickUp: tickets, sprint, time. Its switch turns off everything that reads it.
    #[serde(default)]
    pub tracker: TrackerConfig,
    #[serde(default)]
    pub clickup: ClickupConfig,
    #[serde(default)]
    pub obsidian: ObsidianConfig,
    #[serde(default)]
    pub jira: JiraConfig,
    #[serde(default)]
    pub calendar: CalendarConfig,
    #[serde(default)]
    pub vulnerabilities: VulnerabilitiesConfig,
    /// "Renovate" block: the Dependency Dashboard issue and the schedule running Renovate.
    #[serde(default)]
    pub renovate: RenovateConfig,
    /// "Environments" block: one item per cluster, read-only.
    #[serde(default)]
    pub environments: Vec<EnvironmentConfig>,
    /// ArgoCD instances; without the section, those found on the default context's cluster (`argocd*`
    /// namespaces).
    #[serde(default)]
    pub argocd: Vec<ArgoConfig>,
    /// How ArgoCD Applications are read: discovery, and the layout of their Helm values paths.
    #[serde(default)]
    pub argocd_layout: ArgoLayoutConfig,
    /// "Audio" block of the status bar: live streams and podcasts (RSS).
    #[serde(default)]
    pub audio: AudioConfig,
    /// The repositories: Repos menu, Claude actions in their folder, their MRs, scan of the base
    /// images. At most one is primary, the one bare MR references name (`[forge] bare_mr_repo`, or
    /// `primary = true`). `Config::repositories` lists them, the primary one first.
    #[serde(default)]
    pub repos: Vec<RepoConfig>,
    /// The working folders (`[[workspaces]]`): new projects and clones go in one of them.
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    /// Applications to open a repository in from the Repos menu.
    #[serde(default)]
    pub open_with: OpenWithConfig,
    /// Prism console.
    #[serde(default)]
    pub console: ConsoleConfig,
    /// The command palette (`[palette]`).
    #[serde(default)]
    pub palette: PaletteConfig,
    /// Settings › Cleanup (`[cleanup]`).
    #[serde(default)]
    pub cleanup: CleanupConfig,
    /// The editor of an agent's Files tab (`[files]`).
    #[serde(default)]
    pub files: FilesConfig,
    /// Quick capture of notes, by shortcut.
    #[serde(default)]
    pub notes: NotesConfig,
    /// Theme and playful touches, one per feature.
    #[serde(default)]
    pub appearance: AppearanceConfig,
    /// Rail pages (Dashboard, CI…), in order; empty: the app's own.
    #[serde(default)]
    pub views: Vec<ViewConfig>,
    /// Templates of the pages wizard, from the packs (a team's, or Prism's starter pack): Prism
    /// itself has none.
    #[serde(default)]
    pub templates: Vec<TemplateConfig>,
    /// Thresholds of the Alerts block's lines and badges, one per rule.
    #[serde(default)]
    pub alerts: AlertsConfig,
    /// The log file's level (`[logs]`), read at launch by `logs::init`, from config.toml alone.
    #[serde(default)]
    pub logs: LogsConfig,
    /// Agents: coding agent CLIs in Prism's terminals, and each CLI's own keys (`[agents.claude]`…;
    /// `[sessions]` and `[usage]` move to `[agents.claude]`, see [`migrate`]).
    #[serde(default)]
    pub agents: AgentsConfig,
    /// Prism's own updates (`[updates]`): where its releases are published, and their check.
    #[serde(default)]
    pub updates: UpdatesConfig,
    /// Feedback to the repositories Prism and its packs come from (`[feedback]`): on by default.
    #[serde(default)]
    pub feedback: FeedbackConfig,
    /// Prism from a phone (`[remote]`): off by default.
    #[serde(default)]
    pub remote: RemoteConfig,
    /// Other computers' Prism, opened here (`[[machines]]`).
    #[serde(default)]
    pub machines: Vec<MachineConfig>,
}

/// `[demo]`: on, Prism shows a fictitious team's work and reads nothing (no GitLab, ClickUp,
/// cluster nor Claude session of the machine). Personal: a pack does not set it.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct DemoConfig {
    pub enabled: bool,
}

/// A profile's name and color (`[profile]`), shown in the rail and the window title. Personal:
/// a pack does not set it.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ProfileConfig {
    pub name: String,
    /// A color of the app (`accent`, `gitlab`, `clickup`, `k8s`, `claude`, `data`…) or a CSS color;
    /// empty: the accent.
    pub color: String,
}

/// Agents (`[agents]`): the Agents view, where Claude actions go, and the agents' worktrees.
/// Sent as is to the frontend, and applied on save.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct AgentsConfig {
    /// The Agents view; without it, Claude actions open Claude Desktop.
    pub enabled: bool,
    /// The coding agent CLI new agents run, and whose plan usage the status bar shows:
    /// `claude` (Claude Code), `command-code` (Command Code), `opencode` (OpenCode, no plan usage) or `codex` (Codex).
    /// An agent keeps its own when it resumes.
    pub provider: crate::agents::provider::Provider,
    /// Where the menus' Claude actions go: `agent` (Prism launcher) or `desktop`.
    #[cfg_attr(feature = "ts", ts(type = "'agent' | 'desktop'"))]
    pub target: String,
    /// Folder of new worktrees; absent: `<repo>.worktrees`, next to the main repository.
    pub worktrees_dir: Option<PathBuf>,
    /// Prefix of the agents' new branches, whatever their CLI.
    pub branch_prefix: String,
    /// Permission mode offered by the launcher: `default` (the one from the CLI's settings),
    /// `acceptEdits` or `plan`.
    /// Any other (`auto`, which approves on its own; a pack can set this key) reads as `default`.
    #[cfg_attr(feature = "ts", ts(type = "'default' | 'acceptEdits' | 'plan'"))]
    #[serde(deserialize_with = "launcher_mode")]
    pub permission_mode: String,
    /// Terminal text size, in points.
    pub font_size: f32,
    /// Text size of an agent's Chat on the computer (its conversation), in points; the rest of the Chat
    /// (its steps, its cards) scaled with it.
    pub chat_font_size: f32,
    /// Lines kept by an agent's terminal, on top of tmux's history.
    pub scrollback: u32,
    /// Kinds of work an agent does (`[[agents.kinds]]`, formerly `categories`), one per agent:
    /// guessed at launch from its command, changed in its header; in the order of the Agents
    /// view's groups when it groups them by kind. Their colours follow this order.
    pub kinds: Vec<AgentKind>,
    /// Tags suggested when tagging an agent (agent header), on top of those already in use; an
    /// agent may carry others.
    pub tags: Vec<String>,
    /// Language of the Claude actions' prompts: `fr` or `en`.
    #[cfg_attr(feature = "ts", ts(type = "'fr' | 'en'"))]
    pub prompt_language: String,
    /// How deep the Tasks panel looks for task files under the agent's folder: 0, the folder
    /// alone; 1, its subfolders too; and so on.
    pub tasks_depth: u8,
    /// An agent launched to fix a failed pipeline from Alerts gets the end of each failed job's
    /// log in its prompt, this many lines, read by Prism. 0: none, the agent reads them itself.
    pub fix_log_lines: u32,
    /// Claude Code's own keys (`[agents.claude]`), used wherever it runs: by default or for a repository.
    pub claude: ClaudeConfig,
    /// Command Code's (`[agents.command-code]`).
    #[serde(rename = "command-code")]
    #[cfg_attr(feature = "ts", ts(rename = "command-code"))]
    pub command_code: CliConfig,
    /// OpenCode's (`[agents.opencode]`).
    pub opencode: CliConfig,
    /// Codex's (`[agents.codex]`).
    pub codex: CliConfig,
    /// One-click answers to an agent at its prompt (`[agents.quick_answers]`).
    pub quick_answers: QuickAnswersConfig,
    /// Agents launched in one click on a repository (`[[agents.quick_actions]]`), the same job each
    /// time: from its project in the Agents view, the palette and the phone.
    pub quick_actions: Vec<QuickAction>,
    /// Prism's shortcuts an agent's terminal keeps for itself (`Cmd+K` clears it): the others go up
    /// to Prism, as everywhere else.
    pub terminal_keys: Vec<String>,
    /// How an agent is read on the computer, until its own Terminal | Chat says otherwise: `chat` (its
    /// conversation, from its transcript: Claude Code, Codex) or `terminal`. The others: their terminal.
    #[cfg_attr(feature = "ts", ts(type = "'chat' | 'terminal'"))]
    pub view: String,
    /// In an agent's Chat, the calls and thoughts between two of its answers folded into one line
    /// (its counts: files edited, commands, failures), unfolded on a click; the turn under way
    /// shows its last ones. Off: one line per call.
    pub fold_steps: bool,
    /// An agent's header on the computer: `read`, its ticket, MR, branch, kind and tags on a
    /// quiet line, changed behind Edit; `edit`, their fields always there.
    #[cfg_attr(feature = "ts", ts(type = "'read' | 'edit'"))]
    pub header_meta: String,
    /// In an agent's Chat, a long answer of its commented on passage by passage, the comments sent
    /// as one prompt, as its plans are (plan mode's, always). Off: its answers read only.
    pub comment_answers: bool,
    /// An agent whose CLI's plan is used up is offered another CLI to continue with (a banner in
    /// its view, an alert, the phone).
    pub suggest_switch: bool,
    /// How the CLI an agent continues with is told what was done: `prism` (a relay Prism writes
    /// from the transcript and git), `agent` (a summary the former agent writes), `task` (the task only).
    #[cfg_attr(feature = "ts", ts(type = "'prism' | 'agent' | 'task'"))]
    pub handoff: String,
    /// The last turns Prism's relay recalls.
    pub handoff_turns: u32,
    /// The relay shown, to edit, before it goes. Off: sent as written, in one click.
    pub handoff_review: bool,
}

/// Quick answers (`[agents.quick_answers]`): one click sends an agent at its prompt a text ("OK",
/// "Do it"), from its view, the Agents block, its alert and the phone.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct QuickAnswersConfig {
    pub enabled: bool,
    /// In this order; empty: Prism's, in `prompt_language`. A pack's list is replaced whole by config.toml's.
    pub answers: Vec<QuickAnswer>,
}

impl Default for QuickAnswersConfig {
    fn default() -> Self {
        Self { enabled: true, answers: Vec::new() }
    }
}

impl QuickAnswersConfig {
    /// The answers offered: the config's, otherwise Prism's in `language` (`fr`, else English).
    /// The window shows the same (quick-answers.ts): a phone's answer is checked against this list.
    pub fn list(&self, language: &str) -> Vec<QuickAnswer> {
        if !self.answers.is_empty() {
            return self.answers.clone();
        }
        let q = |label: &str, text: &str, submit: bool| QuickAnswer { label: label.into(), text: text.into(), submit };
        match language {
            "fr" => vec![
                q("OK", "OK", true),
                q("Fais-le", "Vas-y, fais-le.", true),
                q("Continue", "Continue.", true),
                q("Non, attends", "Non, attends : ", false),
                q("Commit", "Commit avec un message Conventional.", true),
            ],
            _ => vec![
                q("OK", "OK", true),
                q("Do it", "Go ahead, do it.", true),
                q("Continue", "Continue.", true),
                q("No, wait", "No, wait: ", false),
                q("Commit", "Commit with a Conventional message.", true),
            ],
        }
    }
}

/// An answer: its button, and the text sent (`{ticket}`, `{branch}`, `{mr}`, `{title}`, `{folder}`:
/// the agent's; an answer whose variable the agent lacks is not offered to it).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct QuickAnswer {
    pub label: String,
    pub text: String,
    /// Submitted (Enter); false: typed into its input, to finish.
    #[serde(default = "default_true")]
    pub submit: bool,
}

impl AgentsConfig {
    /// How a CLI is launched: its binary and extra arguments.
    pub fn cli(&self, provider: crate::agents::provider::Provider) -> &CliConfig {
        use crate::agents::provider::Provider;
        match provider {
            Provider::Claude => &self.claude.cli,
            Provider::CommandCode => &self.command_code,
            Provider::OpenCode => &self.opencode,
            Provider::Codex => &self.codex,
        }
    }
}

/// An agent launched in one click (`[[agents.quick_actions]]`): its prompt, in a repository.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct QuickAction {
    /// Its button, and the agent's title.
    pub name: String,
    /// What the agent is asked; `{repo}` and `{base}` (its default branch, `main`): the repository's.
    pub prompt: String,
    /// The repositories (`[[repos]] name`) it is offered on; empty: every one.
    pub repos: Vec<String>,
    /// In a new worktree, where the repository makes them; otherwise in its clone, as it is.
    pub worktree: bool,
    /// `default`, `acceptEdits` or `plan`; absent: the mode from the CLI's settings.
    #[cfg_attr(feature = "ts", ts(type = "'default' | 'acceptEdits' | 'plan' | null"))]
    #[serde(deserialize_with = "quick_action_mode")]
    pub mode: Option<String>,
    /// One of the `[[agents.kinds]]`; absent: the one its prompt's command names.
    pub kind: Option<String>,
}

/// How a coding agent CLI is launched (`[agents.<cli>]`): by default, its usual name from the PATH.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct CliConfig {
    /// The executable: a name looked up in the PATH, or a path (`~` allowed). Absent: the CLI's usual name.
    pub bin: Option<String>,
    /// The model new agents and resumes run (`--model`, in the CLI's own names: `opus`,
    /// `gpt-6.1-sol`, `anthropic/claude-sonnet-5-5`…). Absent: the CLI's default.
    pub model: Option<String>,
    /// The reasoning effort new agents and resumes run with (`low`, `medium`, `high`, `xhigh`…, in
    /// the CLI's own levels): Claude Code's `--effort`, Codex's `model_reasoning_effort`. Absent,
    /// or another CLI: the CLI's default.
    pub effort: Option<String>,
    /// Arguments added to every launch and resume, right after the executable (`--verbose`…).
    pub args: Vec<String>,
}

/// Claude Code's own keys (`[agents.claude]`): its launch, Claude Desktop's sessions, the
/// conversations' cost and the plan usage's token. Formerly `[agents]`', `[sessions]`' and
/// `[usage]`'s: still read there (`migrate`).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct ClaudeConfig {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub cli: CliConfig,
    /// An agent whose conversation is estimated beyond this many dollars (API prices) is flagged in
    /// Alerts. 0: never.
    pub cost_alert: f64,
    /// Claude Desktop sessions listed in the Agents view (and counted, with `count_desktop`);
    /// only while Claude Code is the default CLI.
    pub show_desktop: bool,
    /// Claude Desktop sessions also count in Claude's figures (header, status
    /// bar, menu bar, Alerts, notifications); otherwise only Prism's agents.
    pub count_desktop: bool,
    /// Metadata of the desktop's Claude Code sessions.
    pub sessions_dir: PathBuf,
    /// Claude Code transcripts, to tell whether a session is working.
    pub transcripts: PathBuf,
    /// How often the desktop's sessions are read, in seconds.
    pub sessions_poll: u64,
    /// Claude Code token expired: run `claude doctor`, which refreshes it without calling the model.
    pub refresh_token: bool,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        let s = SessionsConfig::default();
        Self { cli: CliConfig::default(), cost_alert: 0.0, show_desktop: true, count_desktop: false, sessions_dir: s.dir, transcripts: s.transcripts, sessions_poll: s.poll_seconds, refresh_token: true }
    }
}

impl ClaudeConfig {
    /// What the desktop sessions' collector reads.
    pub fn sessions(&self) -> SessionsConfig {
        SessionsConfig { dir: self.sessions_dir.clone(), transcripts: self.transcripts.clone(), poll_seconds: self.sessions_poll }
    }
}

/// A kind of work of the agents (`[[agents.kinds]]`).
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct AgentKind {
    pub name: String,
    /// The commands that make an agent of this kind when its prompt starts with one: their names,
    /// separated by commas or spaces, with or without their `/` or plugin (`refine` matches
    /// `/refine` and `/acme-flow:refine`). Empty: chosen by hand only.
    pub when: String,
}

impl AgentsConfig {
    /// The kind of an agent launched with this prompt: the first whose `when` names the command
    /// it starts with (`/plugin:command`), if any.
    pub fn kind_of(&self, prompt: &str) -> Option<&str> {
        let command = prompt.trim_start().strip_prefix('/')?.split_whitespace().next()?;
        let name = command.rsplit(':').next().unwrap_or(command);
        self.kinds
            .iter()
            .find(|k| k.when.split([',', ' ']).map(|w| w.trim().trim_start_matches('/')).any(|w| !w.is_empty() && (w == command || w == name)))
            .map(|k| k.name.as_str())
    }
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: Default::default(),
            target: "agent".into(),
            worktrees_dir: None,
            branch_prefix: "agent/".into(),
            permission_mode: "default".into(),
            font_size: 12.5,
            chat_font_size: 13.5,
            scrollback: 5000,
            kinds: [("Refinement", "refine"), ("Development", "implement"), ("Review", "review")].map(|(name, when)| AgentKind { name: name.into(), when: when.into() }).to_vec(),
            tags: Vec::new(),
            prompt_language: "en".into(),
            tasks_depth: 3,
            fix_log_lines: 60,
            claude: ClaudeConfig::default(),
            command_code: CliConfig::default(),
            opencode: CliConfig::default(),
            codex: CliConfig::default(),
            quick_answers: QuickAnswersConfig::default(),
            quick_actions: Vec::new(),
            terminal_keys: vec!["Cmd+K".into()],
            view: "chat".into(),
            fold_steps: true,
            header_meta: "read".into(),
            comment_answers: true,
            suggest_switch: true,
            handoff: "prism".into(),
            handoff_turns: 8,
            handoff_review: true,
        }
    }
}

/// Prism's logs (`[logs]`): standard error and the profile's `prism.log`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LogsConfig {
    /// `error`, `warn`, `info`, `debug` or `trace`; `RUST_LOG` takes precedence.
    pub level: String,
}

impl Default for LogsConfig {
    fn default() -> Self {
        Self { level: "info".into() }
    }
}

/// Prism's own updates (`[updates]`): the releases of a repository, read with glab or gh. A
/// team's pack names its own, whose builds carry its blocks; the open-source Prism, its public one.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct UpdatesConfig {
    /// Checks at launch and every few hours, and says so in the status bar.
    pub enabled: bool,
    /// `glab:group/project` or `gh:owner/repo`; empty: no updates.
    pub source: String,
    /// Reads the packs' sources too, at the same times: a pack changed there shows in the status bar.
    pub packs: bool,
}

/// `[agents] permission_mode`: one the launcher offers, or `default`.
fn launcher_mode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let mode = String::deserialize(d)?;
    Ok(if matches!(mode.as_str(), "default" | "acceptEdits" | "plan") {
        mode
    } else {
        tracing::warn!("[agents] permission_mode {mode:?}: not one the launcher offers, default instead");
        "default".into()
    })
}

/// A quick action's `mode`: one the launcher offers, otherwise none (the CLI's own), said in the log.
fn quick_action_mode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let mode = Option::<String>::deserialize(d)?;
    Ok(mode.filter(|m| {
        let ok = matches!(m.as_str(), "default" | "acceptEdits" | "plan");
        if !ok {
            tracing::warn!("[[agents.quick_actions]] mode {m:?}: not one the launcher offers, the CLI's own instead");
        }
        ok
    }))
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self { enabled: true, source: String::new(), packs: true }
    }
}

/// Feedback (`[feedback]`): issues opened from Prism on the repository it comes from (`repo`, or
/// `[updates] source`) or on a pack's (its `[pack] source`), with glab or gh.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct FeedbackConfig {
    /// Send feedback in the console, the menus and the status bar.
    pub enabled: bool,
    /// Reads the state of the issues opened from Prism with the updates: a shipped one shows in the notes.
    pub follow: bool,
    /// Where Prism's own feedback goes, `glab:group/project` or `gh:owner/repo`; empty: `[updates] source`.
    pub repo: String,
    /// The label of each kind (`feature`, `bug`, `idea`); an empty one: none.
    pub labels: std::collections::BTreeMap<String, String>,
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        let labels = [("feature", "enhancement"), ("bug", "bug"), ("idea", "")].map(|(k, v)| (k.to_string(), v.to_string()));
        Self { enabled: true, follow: true, repo: String::new(), labels: labels.into_iter().collect() }
    }
}

fn default_true() -> bool {
    true
}

/// `~/.config/prism/config.toml`.
pub fn default_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".config/prism/config.toml")
}

/// The default values of each key, as JSON: a config containing only the required
/// keys, empty (they have no default).
pub fn defaults() -> serde_json::Value {
    const REQUIRED: &str = "[runners]\nkube_context = \"\"\nnamespace = \"\"\n[worktrees]\nrepo = \"\"\n[gitlab]\nproject = \"\"\n[clickup]\n";
    let mut defaults = toml::from_str::<Config>(REQUIRED).ok().and_then(|c| serde_json::to_value(c).ok()).unwrap_or_default();
    // The blocks' own tables (`[docker]`, `[data]`…), as read from an empty config.
    crate::blocks::settings(&mut defaults, &toml::Table::new());
    defaults
}

pub fn load(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| tr!("reading {path}", path = path.display()))?;
    let (table, _, _) = crate::packs::layered(path, &raw)?;
    let mut cfg: Config = toml::Value::Table(table.clone()).try_into().context(tr("invalid config.toml"))?;
    cfg.table = table;
    // Before the repositories: a clone in a working folder takes its forge.
    resolve_workspaces(&mut cfg.workspaces)?;
    resolve_repos(&mut cfg)?;
    for p in cfg.obsidian.prefixes() {
        check_prefix(&p)?;
    }
    let claude = &mut cfg.agents.claude;
    claude.sessions_dir = expand_home(&claude.sessions_dir);
    claude.transcripts = expand_home(&claude.transcripts);
    for cli in [&mut cfg.agents.claude.cli, &mut cfg.agents.command_code, &mut cfg.agents.opencode, &mut cfg.agents.codex] {
        cli.bin = cli.bin.take().map(|b| b.trim().to_string()).filter(|b| !b.is_empty()).map(|b| if b.starts_with('~') { expand_home(Path::new(&b)).display().to_string() } else { b });
    }
    cfg.agents.worktrees_dir = cfg.agents.worktrees_dir.as_deref().map(expand_home);
    Ok(cfg)
}

/// The working folders, read: those without a folder dropped, folders expanded, each named (its
/// folder's name by default), names unique.
fn resolve_workspaces(list: &mut Vec<Workspace>) -> Result<()> {
    list.retain(|w| !w.dir.as_os_str().is_empty());
    for w in list.iter_mut() {
        w.dir = expand_home(&w.dir);
        w.name = w.name.trim().to_string();
        if w.name.is_empty() {
            w.name = w.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        }
        w.namespace = w.namespace.trim().trim_matches('/').to_string();
    }
    for (i, w) in list.iter().enumerate() {
        anyhow::ensure!(!list[..i].iter().any(|o| o.name.eq_ignore_ascii_case(&w.name)), tr!("[[workspaces]]: two are named {name}", name = w.name));
    }
    Ok(())
}

/// The repositories, read: paths expanded, empty fields cleared, names unique, at most one primary
/// repository, named by `[forge] bare_mr_repo`, `[[repos]] primary` or the former `[gitlab] project`
/// and `[worktrees] repo`; the repositories the blocks name exist. Nothing is rewritten: Settings
/// shows the file as it is.
fn resolve_repos(cfg: &mut Config) -> Result<()> {
    cfg.worktrees.repo = expand_home(&cfg.worktrees.repo);
    cfg.gitlab.project = cfg.gitlab.project.trim().to_string();
    for r in &mut cfg.repos {
        // An empty path or project (field cleared in Settings) means none.
        r.path = r.path.as_deref().filter(|p| !p.as_os_str().is_empty()).map(expand_home);
        r.project = r.project.take().map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        // No forge of its own: its working folder's, if its clone is in one.
        if r.forge.is_none() {
            r.forge = r.path.as_deref().and_then(|p| cfg.workspaces.iter().find(|w| p.starts_with(&w.dir))).and_then(|w| w.forge);
        }
        // On another forge than the profile's: only its MRs are read there, by that forge's collector.
        r.forge = r.forge.filter(|f| *f != cfg.forge.provider);
        if let Some(f) = r.forge {
            anyhow::ensure!(!r.primary, tr!("[[repos]] {name}: on {forge}, not this profile's forge: it cannot be the primary repository", name = r.name, forge = f.label()));
            (r.pipelines, r.merge_queue) = (false, false);
        }
    }
    // `[forge] bare_mr_repo` names the primary repository: over a pack's `primary = true`.
    cfg.forge.bare_mr_repo = cfg.forge.bare_mr_repo.trim().to_string();
    if !cfg.forge.bare_mr_repo.is_empty() {
        let name = cfg.forge.bare_mr_repo.clone();
        let entry = cfg.repos.iter().find(|r| r.name == name).ok_or_else(|| anyhow::anyhow!(tr!("[forge] bare_mr_repo: no repository is named {name} ([[repos]])", name = name)))?;
        if let Some(f) = entry.forge {
            anyhow::bail!(tr!("[forge] bare_mr_repo: {name} is on {forge}, not this profile's forge", name = name, forge = f.label()));
        }
        for r in &mut cfg.repos {
            r.primary = r.name == name;
        }
    }
    let declared = cfg.repos.iter().filter(|r| r.primary).count();
    anyhow::ensure!(declared <= 1, tr!("[[repos]]: {declared} repositories have primary = true, at most one can", declared = declared));
    // A pack can move to `[[repos]] primary` before everyone's config.toml: the former `[gitlab]
    // project` and `[worktrees] repo` still fill in the primary entry, when they agree with it.
    let (project, repo) = (cfg.gitlab.project.clone(), cfg.worktrees.repo.clone());
    if let Some(p) = cfg.repos.iter_mut().find(|r| r.primary) {
        if !project.is_empty() {
            anyhow::ensure!(
                p.project.get_or_insert_with(|| project.clone()) == &project,
                tr!("[gitlab] project ({project}) and the primary [[repos]] entry {name} name two projects: keep one", project = project, name = p.name)
            );
        }
        if !repo.as_os_str().is_empty() {
            anyhow::ensure!(
                p.path.get_or_insert_with(|| repo.clone()) == &repo,
                tr!("[worktrees] repo ({repo}) and the primary [[repos]] entry {name} name two clones: keep one", repo = repo.display(), name = p.name)
            );
        }
    }
    let mut names = std::collections::HashSet::new();
    for r in cfg.repositories() {
        anyhow::ensure!(!r.name.trim().is_empty(), tr(tr(tr(tr("[[repos]]: a repository has no name")))));
        anyhow::ensure!(names.insert(r.name.clone()), tr!("[[repos]]: two repositories are named {name}", name = r.name));
    }
    // The repositories the blocks read (a pack's are checked as it starts, `Config::block_repos`).
    cfg.forge.ci_health_repo = cfg.forge.ci_health_repo.trim().to_string();
    cfg.renovate.repo = cfg.renovate.repo.trim().to_string();
    cfg.vulnerabilities.repos = cfg.vulnerabilities.repos.iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
    let named = [("[forge] ci_health_repo", &cfg.forge.ci_health_repo), ("[renovate] repo", &cfg.renovate.repo)].into_iter().filter(|(_, n)| !n.is_empty()).chain(cfg.vulnerabilities.repos.iter().map(|n| ("[vulnerabilities] repos", n)));
    for (key, name) in named {
        anyhow::ensure!(names.contains(name), tr!("{key}: no repository is named {name} ([[repos]])", key = key, name = name));
    }
    Ok(())
}

impl Config {
    /// The primary repository: the `[[repos]]` entry with `primary = true`, or the former `[gitlab]
    /// project` and `[worktrees] repo`, named after the project.
    pub fn primary(&self) -> Option<RepoConfig> {
        self.repos.iter().find(|r| r.primary).cloned().or_else(|| self.legacy_primary())
    }

    /// The former `[gitlab] project` and `[worktrees] repo`, when no `[[repos]]` entry is primary.
    fn legacy_primary(&self) -> Option<RepoConfig> {
        let (project, repo) = (&self.gitlab.project, &self.worktrees.repo);
        if self.repos.iter().any(|r| r.primary) || (project.is_empty() && repo.as_os_str().is_empty()) {
            return None;
        }
        let name = project.rsplit('/').next().filter(|n| !n.is_empty())
            .or_else(|| repo.file_name().and_then(|n| n.to_str()))
            .unwrap_or("main");
        Some(RepoConfig {
            name: name.to_string(),
            path: Some(repo.clone()).filter(|p| !p.as_os_str().is_empty()),
            project: Some(project.clone()).filter(|p| !p.is_empty()),
            merge_requests: true,
            primary: true,
            // As before `[[repos]] primary`: worktrees, merge queue and tickets were the primary repository's.
            worktrees: true,
            merge_queue: true,
            ticket_required: true,
            pipelines: true,
            main_branch: None,
            release_prefix: None,
            base_branch: None,
            agent: None,
            forge: None,
        })
    }

    /// A repository's main branch, release prefix and worktree base: its own, otherwise `[forge]`'s
    /// and `[worktrees]`'s, the same for every repository (an empty main branch: the remote's
    /// default one).
    pub fn branches(&self, r: &RepoConfig) -> (String, String, String) {
        (
            r.main_branch.clone().unwrap_or_else(|| self.forge.main_branch.clone()),
            r.release_prefix.clone().unwrap_or_else(|| self.forge.release_prefix.clone()),
            r.base_branch.clone().unwrap_or_else(|| self.worktrees.base_branch.clone()),
        )
    }

    /// Main branch and release prefix of the primary repository, otherwise `[forge]`'s.
    pub fn primary_branches(&self) -> (String, String) {
        let (main, release, _) = self.primary().map_or_else(|| (self.forge.main_branch.clone(), self.forge.release_prefix.clone(), String::new()), |p| self.branches(&p));
        (main, release)
    }

    /// A repository's project with its branches, `repo` absent for the primary one (as on its MRs).
    pub fn pipeline_repo(&self, r: &RepoConfig) -> Option<PipelineRepo> {
        let project = r.project.clone()?;
        let (main_branch, release_prefix, _) = self.branches(r);
        Some(PipelineRepo { repo: (!r.primary).then(|| r.name.clone()), name: r.name.clone(), project, main_branch, release_prefix })
    }

    /// The repositories whose protected branches' pipelines are followed, the primary one first.
    pub fn pipeline_repos(&self) -> Vec<PipelineRepo> {
        self.repositories().iter().filter(|r| r.pipelines).filter_map(|r| self.pipeline_repo(r)).collect()
    }

    /// The repositories whose worktrees Prism lists and creates, the primary one first.
    pub fn worktree_repos(&self) -> Vec<WorktreeRepo> {
        self.repositories()
            .into_iter()
            .filter(RepoConfig::has_worktrees)
            .map(|r| {
                let (main_branch, release_prefix, base_branch) = self.branches(&r);
                WorktreeRepo { path: r.dir().to_path_buf(), name: r.name, main_branch, release_prefix, base_branch }
            })
            .collect()
    }

    /// The repositories `names` names (`[[repos]]` names), in that order; none named: the primary
    /// one, if any. A name no repository has is left out.
    pub fn repos_named(&self, names: &[String]) -> Vec<RepoConfig> {
        if names.is_empty() {
            return self.primary().into_iter().collect();
        }
        let all = self.repositories();
        names.iter().filter_map(|n| all.iter().find(|r| &r.name == n).cloned()).collect()
    }

    /// The repository CI health reads (`[forge] ci_health_repo`, otherwise the primary one).
    pub fn ci_health_repo(&self) -> Option<RepoConfig> {
        self.repos_named(&names(&self.forge.ci_health_repo)).into_iter().next()
    }

    /// The repository whose project runs Renovate (`[renovate] repo`, otherwise the primary one).
    pub fn renovate_repo(&self) -> Option<RepoConfig> {
        self.repos_named(&names(&self.renovate.repo)).into_iter().next()
    }

    /// The repositories whose dependencies Vulnerabilities scans (`[vulnerabilities] repos`,
    /// otherwise the primary one).
    pub fn vulnerability_repos(&self) -> Vec<RepoConfig> {
        self.repos_named(&self.vulnerabilities.repos)
    }

    /// The repositories a block reads (`Block::repos`): its table's `repo` or `repos` (`[shipping]
    /// repo = "app"`), otherwise the primary one; and the names no repository has.
    pub fn block_repos(&self, block: &str) -> (Vec<RepoConfig>, Vec<String>) {
        let table = self.table.get(block).and_then(toml::Value::as_table);
        let listed: Vec<String> = match (table.and_then(|t| t.get("repos")), table.and_then(|t| t.get("repo"))) {
            (Some(toml::Value::Array(list)), _) => list.iter().filter_map(toml::Value::as_str).map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect(),
            (_, Some(toml::Value::String(name))) => names(name),
            _ => Vec::new(),
        };
        let found = self.repos_named(&listed);
        let missing = listed.into_iter().filter(|n| !found.iter().any(|r| &r.name == n)).collect();
        (found, missing)
    }

    /// The local clones with an agent CLI of their own (`[[repos]] agent`).
    pub fn repo_agents(&self) -> Vec<(PathBuf, crate::agents::provider::Provider)> {
        self.repositories().into_iter().filter_map(|r| Some((r.path?, r.agent?))).collect()
    }

    /// Every repository, the primary one first.
    pub fn repositories(&self) -> Vec<RepoConfig> {
        self.primary().into_iter().chain(self.repos.iter().filter(|r| !r.primary).cloned()).collect()
    }

    /// `[gitlab]` with the primary repository's project (empty without one).
    pub fn gitlab_of_primary(&self) -> GitlabConfig {
        GitlabConfig { project: self.primary().and_then(|r| r.project).unwrap_or_default(), ..self.gitlab.clone() }
    }
}

/// A name typed in a key, as a list: none when empty.
fn names(name: &str) -> Vec<String> {
    Some(name.trim()).filter(|n| !n.is_empty()).map(String::from).into_iter().collect()
}

pub fn expand_home(p: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => dirs::home_dir().unwrap_or_default().join(rest),
        Err(_) => p.to_path_buf(),
    }
}

/// A single file, as `load` reads it (its former keys moved), without packs nor what `load`
/// resolves: for tests, here and in the upper crates. Panics on an invalid config.
#[doc(hidden)]
pub fn parsed(text: &str) -> Config {
    let mut table: toml::Table = text.parse().unwrap();
    migrate(&mut table);
    toml::Value::Table(table).try_into().unwrap()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// An agent's kind from its prompt's command: by its name, with or without its plugin.
    #[test]
    fn kind_of_the_launch_command() {
        let a = AgentsConfig::default();
        assert_eq!(a.kind_of("/acme-flow:refine ACME-1"), Some("Refinement"));
        assert_eq!(a.kind_of("  /implement"), Some("Development"));
        assert_eq!(a.kind_of("/acme-flow:review-fix 12"), None, "another command");
        assert_eq!(a.kind_of("review this MR"), None, "not a command");
        let a = AgentsConfig { kinds: vec![AgentKind { name: "Fix".into(), when: "/cve, acme-flow:backport".into() }], ..a };
        assert_eq!((a.kind_of("/acme-flow:cve x"), a.kind_of("/acme-flow:backport 1"), a.kind_of("/other:backport")), (Some("Fix"), Some("Fix"), None));
    }

    /// `[agents] categories` read as `[[agents.kinds]]`, `stages` dropped.
    #[test]
    fn former_categories_are_kinds() {
        let c = parsed("[agents]\ncategories = [\"Refinement\", \"Ops\"]\nstages = [\"Doing\"]\n");
        assert_eq!(c.agents.kinds, [AgentKind { name: "Refinement".into(), when: String::new() }, AgentKind { name: "Ops".into(), when: String::new() }]);
        let c = parsed("[agents]\nkinds = [\"A\"]\ncategories = [\"B\"]\n");
        assert_eq!(c.agents.kinds.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(), ["A"], "the new key wins");
    }

    /// `[[agents.quick_actions]]`: a prompt on some repositories, in their clone unless asked otherwise.
    #[test]
    fn quick_actions_read() {
        let c = parsed("[[agents.quick_actions]]\nname = \"Upstream + Trivy\"\nprompt = \"Update {base}.\"\nrepos = [\"api\"]\nmode = \"acceptEdits\"\n");
        assert_eq!(c.agents.quick_actions, [QuickAction { name: "Upstream + Trivy".into(), prompt: "Update {base}.".into(), repos: vec!["api".into()], worktree: false, mode: Some("acceptEdits".into()), kind: None }]);
        assert!(AgentsConfig::default().quick_actions.is_empty());
        let c = parsed("[[agents.quick_actions]]\nname = \"Scan\"\nmode = \"bypassPermissions\"\n");
        assert_eq!(c.agents.quick_actions[0].mode, None, "a mode the launcher does not offer is dropped");
    }

    /// A phone's quick answer is checked against this list: the window's defaults must be the same.
    #[test]
    fn default_quick_answers_match_the_window_s() {
        let ts = include_str!("../../../../../web/src/quick-answers.ts");
        for language in ["en", "fr"] {
            for q in QuickAnswersConfig::default().list(language) {
                let line = format!("{{ label: '{}', text: '{}', submit: {} }}", q.label, q.text, q.submit);
                assert!(ts.contains(&line), "quick-answers.ts lacks {line}");
            }
        }
    }

    /// A board's prefix is a pattern: it must compile, and keep the number the only group.
    #[test]
    fn board_prefixes_are_checked() {
        assert!(check_prefix("T").is_ok() && check_prefix("[ABCLM]").is_ok() && check_prefix("(?:KB|TH)").is_ok());
        assert!(check_prefix("[AB").is_err(), "does not compile");
        assert!(check_prefix("(KB|TH)").is_err(), "a capturing group");
        assert!(check_prefix("X?").is_err(), "matches an empty prefix");
    }

    /// The ticket prefix, branch aliases and flow statuses were `[clickup]`'s: still read there, and
    /// `[tracker]` wins when a file says both.
    #[test]
    fn former_clickup_keys_are_the_trackers() {
        let old = parsed("[clickup]\nticket_prefix = \"ACME\"\ndone_statuses = [\"shipped\"]\nsprint_folder = \"Mine\"\n");
        assert_eq!((old.tracker.ticket_prefix.as_str(), old.tracker.done_statuses.as_slice()), ("ACME", ["shipped".to_string()].as_slice()));
        assert_eq!(old.clickup.sprint_folder, "Mine", "what is ClickUp's stays");
        let both = parsed("[tracker]\nticket_prefix = \"NEW\"\n[clickup]\nticket_prefix = \"OLD\"\nbranch_aliases = [\"flow\"]\n");
        assert_eq!((both.tracker.ticket_prefix.as_str(), both.tracker.branch_aliases.as_slice()), ("NEW", ["flow".to_string()].as_slice()));
        let obsidian = parsed("[tracker]\nprovider = \"obsidian\"\n[obsidian]\nvault = \"~/Notes\"\n[[obsidian.boards]]\npath = \"Things/Board.md\"\nrepo = \"Things\"\n");
        assert_eq!((obsidian.tracker.provider, obsidian.obsidian.boards[0].repo.as_deref()), (crate::tracker::Provider::Obsidian, Some("Things")));
    }

    /// The switch, branches, merged window and polling were `[gitlab]`'s: still read there, and
    /// `[forge]` wins when a file says both.
    #[test]
    fn former_gitlab_keys_are_the_forges() {
        let old = parsed("[gitlab]\nenabled = false\nmain_branch = \"master\"\nrelease_prefix = \"rel/\"\nmerged_days = 3\npoll_seconds = 30\nhost = \"git.acme.io\"\n");
        assert_eq!((old.forge.enabled, old.forge.main_branch.as_str(), old.forge.release_prefix.as_str(), old.forge.merged_days, old.forge.poll_seconds), (false, "master", "rel/", 3, 30));
        assert_eq!(old.gitlab.host, "git.acme.io", "what is GitLab's stays");
        let both = parsed("[forge]\nmain_branch = \"trunk\"\n[gitlab]\nmain_branch = \"master\"\npoll_seconds = 30\n");
        assert_eq!((both.forge.main_branch.as_str(), both.forge.poll_seconds), ("trunk", 30));
        let new = parsed("[forge]\nprovider = \"github\"\nenabled = false\n");
        assert!(!new.forge.enabled && new.forge.main_branch.is_empty() && new.forge.poll_seconds == 15, "the remote's default branch");
    }

    /// Claude Code's keys were `[agents]`', `[sessions]`' and `[usage]`'s: still read there, and
    /// `[agents.claude]` wins when a file says both. Each CLI has its launch keys.
    #[test]
    fn former_claude_keys_are_claude_codes() {
        let old = parsed("[agents]\ncost_alert = 5\nshow_desktop = false\ncount_desktop = true\n[sessions]\ndir = \"/s\"\npoll_seconds = 9\n[usage]\nrefresh_token = false\n");
        let c = &old.agents.claude;
        assert_eq!((c.cost_alert, c.show_desktop, c.count_desktop, c.sessions_dir.as_path(), c.sessions_poll, c.refresh_token), (5.0, false, true, Path::new("/s"), 9, false));
        assert!(!old.table.contains_key("sessions") && !old.table.contains_key("usage"), "emptied, they go");
        let both = parsed("[agents]\ncost_alert = 5\n[agents.claude]\ncost_alert = 8\nbin = \"/opt/claude\"\nargs = [\"--model\", \"opus\"]\n[agents.command-code]\nbin = \"cmdx\"\n");
        assert_eq!((both.agents.claude.cost_alert, both.agents.claude.cli.bin.as_deref(), both.agents.claude.cli.args.len()), (8.0, Some("/opt/claude"), 2));
        assert_eq!(both.agents.cli(crate::agents::provider::Provider::CommandCode).bin.as_deref(), Some("cmdx"));
        let none = parsed("");
        assert!(none.agents.claude.show_desktop && none.agents.claude.refresh_token && none.agents.codex.bin.is_none());
    }

    /// GitLab unless the profile says otherwise.
    #[test]
    fn forge_provider() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.forge.provider, crate::forge::Provider::Gitlab);
        let cfg: Config = toml::from_str("[forge]\nprovider = \"github\"\n").unwrap();
        assert_eq!(cfg.forge.provider, crate::forge::Provider::Github);
        assert!(toml::from_str::<Config>("[forge]\nprovider = \"gitea\"\n").is_err());
    }

    /// Product profile: no GitLab, no repository, no clusters; only ClickUp is required.
    #[test]
    fn product_config_without_code_nor_cluster() {
        let cfg = parsed("[forge]\nenabled = false\n\n[runners]\nenabled = false\n\n[clickup]\n");
        assert!(!cfg.forge.enabled && !cfg.clusters.enabled);
        assert!(cfg.gitlab.project.is_empty() && cfg.worktrees.repo.as_os_str().is_empty());
        let full = parsed("[gitlab]\nproject = \"a/b\"\n\n[worktrees]\nrepo = \"~/app\"\n\n[runners]\nkube_context = \"k\"\nnamespace = \"n\"\n\n[clickup]\n");
        assert!(full.forge.enabled && full.clusters.enabled, "on unless switched off");
        assert_eq!(full.gitlab.merge_queue_label, "", "no merge queue unless declared");
        let bare: Config = toml::from_str("").unwrap();
        assert!(bare.clickup.enabled && bare.tracker.ticket_prefix.is_empty(), "every section optional");
    }


    fn resolved(text: &str) -> Result<Config> {
        let mut cfg = parsed(text);
        resolve_repos(&mut cfg).map(|_| cfg)
    }

    /// The former `[gitlab] project` and `[worktrees] repo` are the primary repository, first.
    #[test]
    fn legacy_sections_are_the_primary_repository() {
        let cfg = resolved("[gitlab]\nproject = \"g/app\"\n[worktrees]\nrepo = \"/dev/app\"\n[[repos]]\nname = \"helm\"\nproject = \"g/helm\"\nmerge_requests = true\n").unwrap();
        let p = cfg.primary().unwrap();
        assert_eq!((p.name.as_str(), p.project.as_deref(), p.path.as_deref()), ("app", Some("g/app"), Some(Path::new("/dev/app"))));
        let all = cfg.repositories();
        assert_eq!(all.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["app", "helm"]);
        assert_eq!(p.mr_project(), None, "the primary repository's MRs are read with its pipelines");
        assert_eq!(all[1].mr_project(), Some(("helm".into(), "g/helm".into())));
    }

    /// A `[[repos]]` entry can be the primary one: it goes first, and feeds `[gitlab]`.
    #[test]
    fn primary_repository_entry() {
        let cfg = resolved("[[repos]]\nname = \"infra\"\npath = \"/dev/infra\"\n[[repos]]\nname = \"app\"\npath = \"/dev/app\"\nproject = \"g/app\"\nprimary = true\n").unwrap();
        assert_eq!(cfg.repositories()[0].name, "app");
        assert!(cfg.gitlab.project.is_empty(), "the file is not rewritten");
        assert_eq!((cfg.gitlab_of_primary().project.as_str(), cfg.worktree_repos()), ("g/app", Vec::new()), "worktrees: only when asked");
    }

    /// No primary repository: Prism follows the others' MRs only.
    #[test]
    fn no_primary_repository() {
        let cfg = resolved("[[repos]]\nname = \"app\"\nproject = \"g/app\"\nmerge_requests = true\n").unwrap();
        assert!(cfg.primary().is_none() && cfg.gitlab_of_primary().project.is_empty() && cfg.worktree_repos().is_empty());
        assert_eq!(cfg.repositories()[0].mr_project(), Some(("app".into(), "g/app".into())));
    }

    /// Worktrees: the former primary repository, another one when asked; `[forge]` and `[worktrees]`
    /// branches for both (no main branch declared: the remote's default one), unless a repository
    /// names its own.
    #[test]
    fn worktree_repositories() {
        let cfg = resolved("[gitlab]\nproject = \"g/app\"\nrelease_prefix = \"rel/\"\n[worktrees]\nrepo = \"/dev/app\"\nbase_branch = \"develop\"\n[[repos]]\nname = \"helm\"\npath = \"/dev/helm\"\nworktrees = true\n[[repos]]\nname = \"infra\"\npath = \"/dev/infra\"\n").unwrap();
        let w = cfg.worktree_repos();
        assert_eq!(w.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["app", "helm"]);
        assert_eq!((w[0].main_branch.as_str(), w[0].release_prefix.as_str(), w[0].base_branch.as_str()), ("", "rel/", "develop"));
        assert_eq!((w[1].main_branch.as_str(), w[1].release_prefix.as_str(), w[1].base_branch.as_str()), ("", "rel/", "develop"));
        let own = resolved("[forge]\nmain_branch = \"trunk\"\n[[repos]]\nname = \"helm\"\npath = \"/dev/helm\"\nworktrees = true\nmain_branch = \"master\"\nrelease_prefix = \"\"\nbase_branch = \"auto\"\n").unwrap();
        let h = &own.worktree_repos()[0];
        assert_eq!((h.main_branch.as_str(), h.release_prefix.as_str(), h.base_branch.as_str()), ("master", "", "auto"), "its own over [forge]'s");
        let infos: Vec<RepoInfo> = cfg.repositories().iter().map(RepoConfig::info).collect();
        assert!(infos[0].merge_queue && infos[0].ticket_required && !infos[1].merge_queue && !infos[1].ticket_required);
    }

    /// A repository's agent CLI: only the clones that name one, the others follow `[agents] provider`.
    #[test]
    fn repository_agents() {
        use crate::agents::provider::Provider;
        let cfg = resolved("[agents]\nprovider = \"command-code\"\n[[repos]]\nname = \"app\"\npath = \"/dev/app\"\nprimary = true\nagent = \"claude\"\n[[repos]]\nname = \"helm\"\npath = \"/dev/helm\"\n[[repos]]\nname = \"mrs\"\nagent = \"claude\"\n").unwrap();
        assert_eq!(cfg.repo_agents(), [(PathBuf::from("/dev/app"), Provider::Claude)]);
        assert_eq!(cfg.repositories()[0].info().agent, Some(Provider::Claude));
        assert!(toml::from_str::<Config>("[[repos]]\nname = \"app\"\nagent = \"aider\"\n").is_err());
    }

    /// The pack names the primary repository, a config.toml not yet migrated keeps `[worktrees] repo`.
    #[test]
    fn former_sections_fill_in_the_primary_entry() {
        let cfg = resolved("[gitlab]\nproject = \"g/a\"\n[worktrees]\nrepo = \"/dev/a\"\n[[repos]]\nname = \"a\"\nproject = \"g/a\"\nprimary = true\n").unwrap();
        assert_eq!(cfg.repositories().len(), 1);
        assert_eq!(cfg.primary().unwrap().path.as_deref(), Some(Path::new("/dev/a")));
    }

    /// Pipelines: the former primary repository's, another one's when asked, both with `[forge]`
    /// branches (no main branch: GitLab's default one).
    #[test]
    fn pipeline_repositories() {
        let cfg = resolved("[gitlab]\nproject = \"g/app\"\nrelease_prefix = \"rel/\"\n[[repos]]\nname = \"helm\"\nproject = \"g/helm\"\npipelines = true\n[[repos]]\nname = \"infra\"\nproject = \"g/infra\"\n").unwrap();
        let p = cfg.pipeline_repos();
        assert_eq!(p.iter().map(|r| (r.repo.clone(), r.project.as_str(), r.main_branch.as_str(), r.release_prefix.as_str())).collect::<Vec<_>>(), [(None, "g/app", "", "rel/"), (Some("helm".into()), "g/helm", "", "rel/")]);
    }

    /// A clone in a working folder takes its forge, unless it names its own.
    #[test]
    fn working_folder_forge() {
        let cfg = resolved("[[workspaces]]\nname = \"Personal\"\ndir = \"/perso\"\nforge = \"github\"\n[[repos]]\nname = \"blog\"\npath = \"/perso/blog\"\nproject = \"me/blog\"\nmerge_requests = true\n[[repos]]\nname = \"mine\"\npath = \"/perso/mine\"\nproject = \"g/mine\"\nforge = \"gitlab\"\n[[repos]]\nname = \"app\"\npath = \"/dev/app\"\n").unwrap();
        let forge = |n: &str| cfg.repos.iter().find(|r| r.name == n).unwrap().forge;
        assert_eq!((forge("blog"), forge("mine"), forge("app")), (Some(crate::forge::Provider::Github), None, None));
    }

    /// `[forge] bare_mr_repo` names the primary repository, over a pack's `primary = true`; a name no
    /// repository has, or one on another forge, is refused.
    #[test]
    fn bare_mr_repository() {
        let cfg = resolved("[forge]\nbare_mr_repo = \"helm\"\n[[repos]]\nname = \"app\"\nproject = \"g/app\"\nprimary = true\n[[repos]]\nname = \"helm\"\nproject = \"g/helm\"\n").unwrap();
        assert_eq!(cfg.primary().unwrap().name, "helm");
        assert_eq!(cfg.repositories().iter().filter(|r| r.primary).count(), 1);
        assert!(resolved("[forge]\nbare_mr_repo = \"nope\"\n[[repos]]\nname = \"app\"\n").is_err());
        assert!(resolved("[forge]\nbare_mr_repo = \"mine\"\n[[repos]]\nname = \"mine\"\nproject = \"me/mine\"\nforge = \"github\"\n").is_err());
    }

    /// Each block reads its own repositories, otherwise the primary one; without either, none.
    #[test]
    fn block_repositories() {
        let toml = "[forge]\nci_health_repo = \"helm\"\n[renovate]\nrepo = \"infra\"\n[vulnerabilities]\nrepos = [\"app\", \"helm\"]\n[shipping]\nrepo = \"helm\"\n[queue]\nrepos = [\"gone\"]\n[[repos]]\nname = \"app\"\nproject = \"g/app\"\nprimary = true\n[[repos]]\nname = \"helm\"\nproject = \"g/helm\"\n[[repos]]\nname = \"infra\"\nproject = \"g/infra\"\n";
        let mut cfg = resolved(toml).unwrap();
        cfg.table = toml.parse().unwrap();
        let names = |list: Vec<RepoConfig>| list.into_iter().map(|r| r.name).collect::<Vec<_>>();
        assert_eq!((cfg.ci_health_repo().unwrap().name, cfg.renovate_repo().unwrap().name), ("helm".to_string(), "infra".to_string()));
        assert_eq!(names(cfg.vulnerability_repos()), ["app", "helm"]);
        assert_eq!(names(cfg.block_repos("shipping").0), ["helm"]);
        assert_eq!(names(cfg.block_repos("docker").0), ["app"], "none named: the primary one");
        let (none, missing) = cfg.block_repos("queue");
        assert!(none.is_empty() && missing == ["gone"]);
        assert!(resolved("[renovate]\nrepo = \"nope\"\n[[repos]]\nname = \"app\"\n").is_err());
        let alone = resolved("[[repos]]\nname = \"app\"\nproject = \"g/app\"\n").unwrap();
        assert!(alone.ci_health_repo().is_none() && alone.vulnerability_repos().is_empty());
    }

    #[test]
    fn ambiguous_repositories_are_refused() {
        assert!(resolved("[gitlab]\nproject = \"g/a\"\n[[repos]]\nname = \"b\"\nproject = \"g/b\"\nprimary = true\n").is_err(), "two primary projects");
        assert!(resolved("[worktrees]\nrepo = \"/x\"\n[[repos]]\nname = \"b\"\npath = \"/y\"\nprimary = true\n").is_err(), "two primary clones");
        assert!(resolved("[[repos]]\nname = \"a\"\nprimary = true\n[[repos]]\nname = \"b\"\nprimary = true\n").is_err(), "two primaries");
        assert!(resolved("[gitlab]\nproject = \"g/a\"\n[[repos]]\nname = \"a\"\n").is_err(), "same name");
    }

    /// On another forge: only its MRs, read there.
    #[test]
    fn a_repository_on_another_forge_is_only_its_mrs() {
        let cfg = resolved("[[repos]]\nname = \"mine\"\npath = \"/dev/mine\"\nproject = \"me/mine\"\nforge = \"github\"\nmerge_requests = true\npipelines = true\nmerge_queue = true\n[[repos]]\nname = \"app\"\nproject = \"g/app\"\nforge = \"gitlab\"\nmerge_requests = true\n").unwrap();
        let mine = cfg.repos.iter().find(|r| r.name == "mine").unwrap();
        assert_eq!((mine.forge, mine.merge_requests, mine.pipelines, mine.merge_queue), (Some(crate::forge::Provider::Github), true, false, false));
        assert_eq!((mine.info().project, mine.info().forge), (None, Some(crate::forge::Provider::Github)), "nothing the profile's forge acts on");
        assert_eq!(cfg.repos.iter().find(|r| r.name == "app").unwrap().forge, None, "the profile's forge, named");
        assert!(cfg.pipeline_repos().is_empty());
        assert_eq!(cfg.repositories().iter().filter_map(RepoConfig::mr_project).map(|r| r.0).collect::<Vec<_>>(), ["app"]);
        assert_eq!(cfg.repositories().iter().filter_map(RepoConfig::mr_project_elsewhere).collect::<Vec<_>>(), [("mine".to_string(), "me/mine".to_string())]);
        assert!(resolved("[[repos]]\nname = \"mine\"\nproject = \"me/mine\"\nforge = \"github\"\nprimary = true\n").is_err());
    }

    #[test]
    fn repo_without_path_is_only_its_merge_requests() {
        let cfg: Config = toml::from_str("[clickup]\n\n[[repos]]\nname = \"helm\"\nproject = \"g/helm\"\nmerge_requests = true\n").unwrap();
        let r = &cfg.repos[0];
        assert!(r.path.is_none());
        assert_eq!(r.mr_project(), Some(("helm".into(), "g/helm".into())));
    }

    #[test]
    fn runners_become_clusters_and_a_pods_target() {
        let cfg = parsed("[runners]\nenabled = false\nkube_context = \"ci\"\nnamespace = \"gitlab-runner\"\n");
        assert!(!cfg.clusters.enabled);
        assert_eq!(cfg.clusters.default_context, "ci");
        assert_eq!(cfg.pods.len(), 1);
        assert_eq!((cfg.pods[0].context_or(&cfg.clusters), cfg.pods[0].label(&cfg.clusters).as_str()), ("ci", "Runners"));
        assert_eq!(cfg.pods[0].namespaces, ["gitlab-runner"]);
        let own = parsed("[runners]\nnamespace = \"gitlab-runner\"\n\n[[pods]]\ncontext = \"prod\"\nnamespaces = [\"app-*\"]\n");
        assert_eq!((own.pods.len(), own.pods[0].context.as_str()), (1, "prod"), "the layer's own targets win");
        let new = parsed("[clusters]\ndefault_context = \"new\"\n\n[runners]\nkube_context = \"old\"\n");
        assert_eq!(new.clusters.default_context, "new", "[clusters] wins over the former key");
    }

    #[test]
    fn namespaces_match_their_patterns() {
        let t = |ns: &[&str]| PodsTarget { namespaces: ns.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        assert!(t(&[]).follows("anything"));
        assert!(t(&["app-*"]).follows("app-api") && !t(&["app-*"]).follows("api-app"));
        assert!(t(&["*-prod"]).follows("shop-prod") && !t(&["*-prod"]).follows("shop-preprod-x"));
        assert!(t(&["a*b*c"]).follows("a-b-c") && !t(&["a*b*c"]).follows("a-c"));
        assert!(t(&["ab*ba"]).follows("ab-ba") && !t(&["ab*ba"]).follows("aba"), "the ends do not overlap");
        assert_eq!(t(&["one", "two"]).exact_namespaces(), Some(vec!["one".to_string(), "two".to_string()]));
        assert_eq!(t(&["one", "app-*"]).exact_namespaces(), None);
        assert_eq!(t(&[]).exact_namespaces(), None);
    }

    #[test]
    fn environment_namespaces_take_patterns() {
        let e = |ns: &[&str]| EnvironmentConfig { env: "ops".into(), context: "c".into(), namespaces: ns.iter().map(|s| s.to_string()).collect() };
        let all: Vec<String> = ["apps", "argocd", "monitoring", "kube-system"].iter().map(|s| s.to_string()).collect();
        assert!(!e(&[]).reads("apps") && e(&[]).namespaces_in(&all).is_empty(), "empty reads nothing");
        assert!(e(&["*"]).reads("apps") && e(&["*"]).has_patterns());
        assert_eq!(e(&["*"]).namespaces_in(&all), all);
        assert_eq!(e(&["monitoring", "a*", "apps"]).namespaces_in(&all), ["monitoring", "apps", "argocd"], "config order, each once");
        assert_eq!(e(&["gone"]).namespaces_in(&[]), ["gone"], "a named one is read even if not listed");
        assert!(!e(&["apps"]).has_patterns() && !e(&["apps"]).reads("argocd"));
    }
}
