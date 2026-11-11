//! The agents (`[agents]`): the Agents view, its launcher, the agents' worktrees, « in passing »,
//! overlaps, quick answers and actions, routines, each CLI's program and options, the kinds.

use super::*;

/// Agents (`[agents]`): the Agents view, where Claude actions go, and the agents' worktrees.
/// Sent as is to the frontend, and applied on save.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// Agents launched on their own at fixed times (`[[agents.routines]]`): a quick action (or one
    /// of Prism's, `builtin:`) run by a cron or at a pace, its result waiting in "To review" or
    /// going to the Digest.
    pub routines: Vec<Routine>,
    /// Two agents (or worktrees) of a repository changing the same lines or the same files, told
    /// before the merge (`[agents.overlaps]`).
    pub overlaps: OverlapsConfig,
    /// What an agent sees in passing, out of its task: proposed with `suggest_task`, kept in a
    /// list, launched, chained or made a ticket in a click (`[agents.aside]`).
    pub aside: AsideConfig,
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
    /// In an agent's Chat (Claude Code), what Claude Code sent it besides your words — its
    /// reminders (`<system-reminder>`), a hook's context, a file changed under it: `marker`, out of
    /// the conversation, a ⓘ at the end of the turn (or on the call) showing them on a click;
    /// `hidden`, never shown; `shown`, unfolded where they came.
    /// Any other value reads as `marker`.
    #[cfg_attr(feature = "ts", ts(type = "'marker' | 'hidden' | 'shown'"))]
    #[serde(deserialize_with = "chat_reminders")]
    pub chat_reminders: String,
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
    /// Prism's MCP server (`prism mcp`) and its "prism" skill given to every agent Prism launches:
    /// they read Prism's docs, schema and config, and propose changes. Off: only the agents of
    /// "Configure Prism" get them.
    pub prism_mcp: bool,
}

/// « In passing » (`[agents.aside]`): the tasks an agent flags out of its own (`suggest_task` of
/// Prism's MCP server), as Claude Desktop's `spawn_task`, kept in a list until dealt with.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct AsideConfig {
    /// The `suggest_task` / `dismiss_task` tools given to the agents Prism launches (Claude Code,
    /// Codex, OpenCode).
    pub enabled: bool,
    /// The `spawn_task` calls of the Claude Desktop sessions fill the list too.
    pub desktop: bool,
    /// The main button of a proposal: `launch` (the launcher, prefilled), `then` (after its agent,
    /// in its chain) or `ticket`.
    #[cfg_attr(feature = "ts", ts(type = "'launch' | 'then' | 'ticket'"))]
    pub default_action: String,
    /// Launch creates a worktree (off: the same folder as the agent that saw it).
    pub worktree: bool,
    /// Open proposals older than this many days fold under « Older » (never removed on their own).
    pub stale_days: u32,
    /// A count of the open proposals in the status bar.
    pub statusbar: bool,
}

impl Default for AsideConfig {
    fn default() -> Self {
        Self { enabled: true, desktop: true, default_action: "launch".into(), worktree: true, stale_days: 14, statusbar: false }
    }
}

/// Overlaps between the agents and worktrees of a repository (`[agents.overlaps]`): each one's
/// changes against the merge-base of its base, the uncommitted ones included, compared two by two.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct OverlapsConfig {
    pub enabled: bool,
    /// From which level a chip shows: `lines` (the same lines only) or `file` (the same file too).
    #[cfg_attr(feature = "ts", ts(type = "'lines' | 'file'"))]
    pub level: String,
    /// From which level an alert goes: `lines`, `file`, or `off`.
    #[cfg_attr(feature = "ts", ts(type = "'lines' | 'file' | 'off'"))]
    pub alert: String,
    /// Files never compared, as globs (`*` within a folder, `**` across folders; without a `/`, the
    /// file's name): lockfiles and generated files are computed again, not merged.
    pub ignore: Vec<String>,
}

impl Default for OverlapsConfig {
    fn default() -> Self {
        let ignore = ["*.lock", "pnpm-lock.yaml", "package-lock.json", "yarn.lock", "go.sum", "**/bindings/**"];
        Self { enabled: true, level: "file".into(), alert: "lines".into(), ignore: ignore.map(String::from).to_vec() }
    }
}

impl OverlapsConfig {
    /// `path` (relative to the repository's top) is left out of the comparison.
    pub fn ignored(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        self.ignore.iter().any(|g| if g.contains('/') { path_glob(g.trim_start_matches("./"), path) } else { path_glob(g, name) })
    }
}

/// `*` and `?` within a path segment, `**` across segments (none included).
fn path_glob(pattern: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match p {
            [] => t.is_empty(),
            [b'*', b'*', b'/', rest @ ..] => go(rest, t) || t.iter().enumerate().any(|(i, c)| *c == b'/' && go(rest, &t[i + 1..])),
            [b'*', b'*'] => true,
            [b'*', rest @ ..] => (0..=t.len()).take_while(|i| *i == 0 || t[i - 1] != b'/').any(|i| go(rest, &t[i..])),
            [b'?', rest @ ..] => t.first().is_some_and(|c| *c != b'/') && go(rest, &t[1..]),
            [c, rest @ ..] => t.first() == Some(c) && go(rest, &t[1..]),
        }
    }
    go(pattern.as_bytes(), text.as_bytes())
}

/// Quick answers (`[agents.quick_answers]`): one click sends an agent at its prompt a text ("OK",
/// "Do it"), from its view, the Agents block, its alert and the phone.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// The model it runs (`opus`, in the CLI's own names); absent: the CLI's (`[agents.<cli>] model`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub model: Option<String>,
    /// The reasoning effort (`low`… `xhigh`); absent: the CLI's.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub effort: Option<String>,
}

/// An agent launched on its own (`[[agents.routines]]`): a quick action, or a prompt of its own,
/// at the times a cron names or at a pace. Read as it is: what does not hold (both `cron` and `every`, a cron that
/// does not read, an action unknown) shows in Problems, and that routine does not run.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct Routine {
    /// Its name, unique: what it is known by (its trust, its runs).
    pub name: String,
    /// A `[[agents.quick_actions]]` by its name, or one of Prism's: `builtin:alerts-triage`,
    /// `builtin:retro-draft`. Or `prompt`, not both.
    pub action: String,
    /// What the agent is asked, written for this routine only (`{repo}`, `{base}`, `{since}`,
    /// `{alerts}` replaced); or `action`, not both. With it, the routine says itself what a quick
    /// action would: `worktree`, `mode`, `kind`, `model`, `effort`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub prompt: Option<String>,
    /// With `prompt`: in a new worktree of each repository (`repos` needed).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub worktree: Option<bool>,
    /// With `prompt`: `default`, `acceptEdits` or `plan`; absent: the CLI's.
    #[cfg_attr(feature = "ts", ts(type = "'default' | 'acceptEdits' | 'plan' | null"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub mode: Option<String>,
    /// With `prompt`: one of the `[[agents.kinds]]`; absent: the default one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub kind: Option<String>,
    /// With `prompt`: the model; absent: the CLI's.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub model: Option<String>,
    /// With `prompt`: the reasoning effort; absent: the CLI's.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub effort: Option<String>,
    /// The repositories it runs on, one run each; empty: its quick action's (with `prompt`: once,
    /// in the first `[[workspaces]]` dir, otherwise the home folder).
    pub repos: Vec<String>,
    /// Five fields (`0 3 * * 1-5`); or `every`, not both.
    pub cron: Option<String>,
    /// A pace (`30m`, `6h`, `1d`); or `cron`, not both.
    pub every: Option<String>,
    /// The cron's time zone (`Europe/Paris`); absent: the computer's.
    pub time_zone: Option<String>,
    /// Where its result goes: `review` (the "To review" pile) or `digest` (the next summary).
    /// Blocked or failed, it goes to "To review" whatever this says.
    #[cfg_attr(feature = "ts", ts(type = "'review' | 'digest'"))]
    pub deliver: String,
    /// Its time passed while the computer slept or Prism was closed: `once`, run once on waking
    /// (less than 12 hours late), or `skip`.
    #[cfg_attr(feature = "ts", ts(type = "'skip' | 'once'"))]
    pub missed: String,
    /// Longer than this, the agent is interrupted, then stopped: "too long".
    pub max_minutes: u32,
    pub enabled: bool,
}

impl Default for Routine {
    fn default() -> Self {
        Self { name: String::new(), action: String::new(), prompt: None, worktree: None, mode: None, kind: None, model: None, effort: None, repos: Vec::new(), cron: None, every: None, time_zone: None, deliver: "review".into(), missed: "once".into(), max_minutes: 60, enabled: true }
    }
}

/// Prism's own actions a routine may run, without a repository.
pub const ROUTINE_BUILTINS: [&str; 2] = ["builtin:alerts-triage", "builtin:retro-draft"];

/// A text key set: not absent, not blank.
fn set(v: &Option<String>) -> Option<String> {
    v.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(String::from)
}

impl Routine {
    /// Its own prompt, when it has one (not blank).
    pub fn own_prompt(&self) -> Option<&str> {
        self.prompt.as_deref().filter(|p| !p.trim().is_empty())
    }

    /// When it runs, and what it runs: the quick action named, or the one its own prompt makes;
    /// none for a builtin. What does not hold, in a few words.
    pub fn check<'a>(&self, actions: &'a [QuickAction]) -> std::result::Result<(crate::schedule::When, Option<std::borrow::Cow<'a, QuickAction>>), String> {
        if self.name.trim().is_empty() {
            return Err(tr("a routine has no name").into());
        }
        let when = crate::schedule::When::of(self.cron.as_deref(), self.every.as_deref(), self.time_zone.as_deref()).map_err(|e| format!("{e:#}"))?;
        if !matches!(self.deliver.as_str(), "review" | "digest") {
            return Err(tr!("deliver {value}: review or digest", value = self.deliver));
        }
        if !matches!(self.missed.as_str(), "skip" | "once") {
            return Err(tr!("missed {value}: skip or once", value = self.missed));
        }
        if self.max_minutes == 0 {
            return Err(tr("max_minutes: a minute at least").into());
        }
        let action = self.action.trim();
        if let Some(prompt) = self.own_prompt() {
            if !action.is_empty() {
                return Err(tr("action and prompt: one of the two, not both").into());
            }
            // Blank: the CLI's, as for a quick action.
            let mode = set(&self.mode);
            if let Some(m) = mode.as_deref().filter(|m| !matches!(*m, "default" | "acceptEdits" | "plan")) {
                return Err(tr!("mode {value}: default, acceptEdits or plan", value = m));
            }
            let worktree = self.worktree.unwrap_or(false);
            if worktree && self.repos.is_empty() {
                return Err(tr("worktree = true needs its repos").into());
            }
            // Without a repository, nothing to put in their place.
            if self.repos.is_empty()
                && let Some(slot) = ["{repo}", "{base}"].into_iter().find(|s| prompt.contains(s))
            {
                return Err(tr!("{slot} needs repos: without them the routine runs in a working folder", slot = slot));
            }
            let own = QuickAction {
                name: self.name.trim().to_string(),
                prompt: prompt.to_string(),
                repos: self.repos.clone(),
                worktree,
                mode,
                kind: set(&self.kind),
                model: set(&self.model),
                effort: set(&self.effort),
            };
            return Ok((when, Some(std::borrow::Cow::Owned(own))));
        }
        if action.is_empty() {
            return Err(tr("an action or a prompt: one of the two").into());
        }
        // Beside an action, what is launched is read in one place only: its quick action.
        // Blank (`model = ""`): the CLI's, said nowhere: not a second place.
        let beside = [("worktree", self.worktree == Some(true)), ("mode", set(&self.mode).is_some()), ("kind", set(&self.kind).is_some()), ("model", set(&self.model).is_some()), ("effort", set(&self.effort).is_some())];
        if let Some((key, _)) = beside.iter().find(|(_, set)| *set) {
            return Err(tr!("{key}: with prompt only, its quick action says it beside action", key = *key));
        }
        if ROUTINE_BUILTINS.contains(&action) {
            return Ok((when, None));
        }
        let qa = actions.iter().find(|a| a.name == action).ok_or_else(|| tr!("no quick action named {name}", name = action))?;
        if qa.prompt.trim().is_empty() {
            return Err(tr!("quick action {name} has no prompt", name = action));
        }
        Ok((when, Some(std::borrow::Cow::Borrowed(qa))))
    }

    /// The repositories it runs on: its own, otherwise its quick action's; none for a builtin.
    pub fn targets(&self, qa: Option<&QuickAction>) -> Vec<String> {
        match qa {
            None => Vec::new(),
            Some(_) if !self.repos.is_empty() => self.repos.clone(),
            Some(qa) => qa.repos.clone(),
        }
    }

    /// Its runs at each time: one per repository, or one without (a builtin, a prompt of its own
    /// with no repos: in the working folder).
    pub fn runs(&self, qa: Option<&QuickAction>) -> Vec<Option<String>> {
        if qa.is_none() || (self.own_prompt().is_some() && self.repos.is_empty()) {
            return vec![None];
        }
        self.targets(qa).into_iter().map(Some).collect()
    }
}

/// How a coding agent CLI is launched (`[agents.<cli>]`): by default, its usual name from the PATH.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
            routines: Vec::new(),
            overlaps: OverlapsConfig::default(),
            aside: AsideConfig::default(),
            terminal_keys: vec!["Cmd+K".into()],
            view: "chat".into(),
            fold_steps: true,
            chat_reminders: "marker".into(),
            header_meta: "read".into(),
            comment_answers: true,
            suggest_switch: true,
            handoff: "prism".into(),
            handoff_turns: 8,
            handoff_review: true,
            prism_mcp: false,
        }
    }
}

/// `[agents] permission_mode`: one the launcher offers, or `default`.
/// `[agents] chat_reminders`: marker, hidden or shown, else `marker`.
fn chat_reminders<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let mode = String::deserialize(d)?;
    Ok(if matches!(mode.as_str(), "marker" | "hidden" | "shown") {
        mode
    } else {
        tracing::warn!("[agents] chat_reminders {mode:?}: neither marker, hidden nor shown, marker instead");
        "marker".into()
    })
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_chat_reminders_reads_as_marker() {
        let read = |v: &str| toml::from_str::<AgentsConfig>(&format!("chat_reminders = \"{v}\"")).unwrap().chat_reminders;
        assert_eq!((read("shown"), read("hidden"), read("show"), read("Marker")), ("shown".into(), "hidden".into(), "marker".into(), "marker".into()));
    }
}
