//! Agents: coding agent sessions (Claude Code, Command Code, OpenCode or Codex, see `provider`) that Prism launches itself.
//! Each runs in a session of Prism's tmux server, which keeps it alive without Prism; the
//! Agents view attaches to it through a pseudo-terminal to display it.

pub mod activity;
mod attach;
pub mod breakdown;
pub mod chain;
mod chains;
pub mod conversation;
mod costs;
pub mod files;
pub mod hook;
mod input;
mod left;
mod lifecycle;
pub mod live;
mod meta;
mod pty;
pub mod plan;
pub mod question;
mod quick;
mod reading;
mod routines;
mod suggestion;
pub mod switch;

mod registry;
mod pod_shells;
mod shells;
pub mod slash;
mod status;
pub mod tasks;
mod term;
pub mod vcs;
mod windows;
mod workspace;
/// The core's side of the agents: their providers, their context and plan use.
pub use prism_core::agents::{context, provider, usage};
pub mod tmux;
/// Prefix of the agents' tmux sessions.
pub use tmux::PREFIX as SESSION_PREFIX;
/// Key prefix of an agent's shells, in the terminal commands (`shell:<agent>:<n>`).
pub use tmux::SHELL;
pub mod worktree;

use prism_base::{git, i18n::tr, platform, tr};
use prism_core::{config, forge, profile, state, ticket};
use prism_collectors as collectors;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use anyhow::{bail, ensure, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use crate::{
    config::{AgentsConfig, WorktreeRepo},
    state::{Agent, AgentState, ClaudeSession, Hub, SessionOrigin, SessionStatus},
};
use provider::Provider;
pub use chains::{defines_task, takes_prompt, Notice, Sent};
pub use routines::Inputs as RoutineInputs;
pub use lifecycle::LaunchPreview;
pub use pod_shells::PodRef;
pub use pty::Attachment;
use registry::{Record, Registry};
use status::Fold;
use tmux::{Pane, Tmux};

const SOURCE: &str = "agents";
const POLL: Duration = Duration::from_secs(2);
/// A transcript not found (no hook named it yet) is looked for again this often.
const SEARCH_AGAIN: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct Agents {
    inner: Arc<Inner>,
}

/// What the Agents view asks for when launching an agent.
#[derive(Debug, Deserialize)]
pub struct Launch {
    pub title: String,
    /// Folder to launch the agent in, any one (`~/…` allowed); ignored with a new worktree.
    #[serde(default, deserialize_with = "home_path")]
    pub cwd: Option<PathBuf>,
    pub prompt: Option<String>,
    pub worktree: Option<NewWorktree>,
    /// `acceptEdits`, `plan`…; absent, the mode from the CLI's settings.
    pub permission_mode: Option<String>,
    /// One of the `[[agents.kinds]]`; absent, the one its prompt's command names, if any.
    pub category: Option<String>,
    /// Its first tags (free, several).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Its CLI, picked in the launcher; absent, its repository's (`[[repos]] agent`), failing that
    /// `[agents] provider`.
    #[serde(default)]
    pub provider: Option<Provider>,
    /// Its model, picked in the launcher, kept on resume; absent, its CLI's (`[agents.<cli>] model`).
    #[serde(default)]
    pub model: Option<String>,
    /// Its reasoning effort, picked in the launcher, kept on resume; absent, its CLI's (`[agents.<cli>] effort`).
    #[serde(default)]
    pub effort: Option<String>,
    /// Its "Then" steps (`chains`), kept with the launch.
    #[serde(default)]
    pub then: Vec<state::LaunchStep>,
    /// The action that opened the launcher, kept with the launch.
    #[serde(default)]
    pub from: Option<String>,
}

impl From<state::LaunchSpec> for Launch {
    fn from(s: state::LaunchSpec) -> Self {
        Launch {
            title: s.title,
            cwd: s.cwd.map(PathBuf::from),
            prompt: s.prompt,
            worktree: s.worktree.map(|w| NewWorktree { repo: w.repo, name: w.name, branch: w.branch, base: w.base, also: w.also }),
            permission_mode: s.permission_mode,
            category: s.category,
            tags: s.tags,
            provider: s.provider,
            model: s.model,
            effort: s.effort,
            then: s.then,
            from: s.from,
        }
    }
}

/// A folder as the launcher gives it: `~/…` under the home folder.
fn home_path<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Option<PathBuf>, D::Error> {
    Ok(Option::<PathBuf>::deserialize(d)?.map(|p| config::expand_home(&p)))
}

impl Launch {
    /// The launch as asked, kept on the agent.
    fn spec(&self) -> state::LaunchSpec {
        let some = |s: &Option<String>| s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
        state::LaunchSpec {
            title: self.title.trim().to_string(),
            cwd: if self.worktree.is_some() { None } else { self.cwd.as_ref().map(|p| p.display().to_string()) },
            prompt: some(&self.prompt),
            worktree: self.worktree.as_ref().map(|w| state::WorktreeRequest { repo: w.repo.clone(), name: w.name.clone(), branch: w.branch.clone(), base: w.base.clone(), also: w.also.clone() }),
            permission_mode: some(&self.permission_mode),
            category: some(&self.category),
            tags: clean_tags(self.tags.clone()),
            provider: self.provider,
            model: some(&self.model),
            effort: some(&self.effort),
            then: self.then.clone(),
            from: some(&self.from),
        }
    }
}

/// A new worktree for the agent, next to its repository.
#[derive(Debug, Deserialize)]
pub struct NewWorktree {
    /// Its repository (`[[repos]] name`); absent, the first one with worktrees (the primary one).
    #[serde(default)]
    pub repo: Option<String>,
    /// Folder name; failing that, derived from the title.
    pub name: Option<String>,
    /// Existing branch to check out (an MR's); failing that, `<prefix><name>` from the base.
    pub branch: Option<String>,
    /// Branch on `origin` the new one starts from, picked in the launcher; absent, the repository's
    /// base (`[[repos]] base_branch`, `auto`: the latest release).
    #[serde(default)]
    pub base: Option<String>,
    /// Other repositories given a worktree of the same name and branch, the agent working in all
    /// of them (`--add-dir`): Claude Code and Codex only.
    #[serde(default)]
    pub also: Vec<String>,
}

/// Failures of the same command in a row that make an agent "going in circles".
const LOOPING: u32 = 3;

struct Inner {
    tmux: Tmux,
    /// Agents' working folder: tmux config, hooks settings, logs, intents.
    work: PathBuf,
    /// The repositories where agents create their worktrees, the primary one first; re-read on
    /// each change of the config (`set_repos`).
    worktrees: Mutex<Vec<WorktreeRepo>>,
    /// The clones whose agents run a CLI of their own (`[[repos]] agent`), rather than `[agents] provider`.
    repo_agents: Mutex<Vec<(PathBuf, Provider)>>,
    /// `[agents]`, re-read on every Settings save.
    settings: Mutex<AgentsConfig>,
    /// Hook logs, one per session (`<id>.jsonl`), written by each CLI's wiring
    /// (`Provider::wiring_file`, in `work`).
    events: PathBuf,
    folds: Mutex<HashMap<String, Fold>>,
    /// What each Claude Code agent's transcripts consumed, read as they grow.
    usages: Mutex<HashMap<String, usage::UsageFold>>,
    /// What every agent cost, closed ones too (`agent-costs.json`).
    costs: Mutex<costs::Book>,
    /// Held by a refresh from copying the folds to putting them back: a change made to a fold
    /// meanwhile would be overwritten.
    refreshing: tokio::sync::Mutex<()>,
    registry: Mutex<Registry>,
    attachments: Mutex<HashMap<String, Attachment>>,
    next_token: AtomicU64,
    hub: Hub,
    /// Wakes the state loop right away after a launch or a close.
    changed: Notify,
    command_code_window: Mutex<WindowCache>,
    opencode_windows: Mutex<OpenCodeWindows>,
    /// The chains and the inbox (`chains`).
    book: Mutex<chains::Book>,
    /// Wakes the chains' loop: a prompt queued, a queued one sent.
    chains_wake: Notify,
    /// The routines: their times, their runs, their trust (`routines`).
    routines: Mutex<routines::Book>,
    /// Wakes the routines' loop: the config changed.
    routines_wake: Notify,
    /// The routines' loop runs (not in the demo, whose snapshot shows sample ones).
    routines_live: std::sync::atomic::AtomicBool,
}

/// The windows of OpenCode's models (its catalog), and when it was last read.
#[derive(Default)]
struct OpenCodeWindows {
    known: HashMap<String, u64>,
    read: Option<std::time::Instant>,
}

/// Command Code's default model and its window (`cmd status --json`), and when it was last asked.
#[derive(Default)]
struct WindowCache {
    known: Option<(String, u64)>,
    asked: Option<std::time::Instant>,
}

impl Agents {
    /// `dir`: the app's data folder (registry and tmux config); `worktrees`: the config section
    /// of the same name, absent if it does not load; `repo_agents`: the clones with a CLI of their own.
    pub fn new(dir: PathBuf, hub: Hub, worktrees: Vec<WorktreeRepo>, repo_agents: Vec<(PathBuf, Provider)>) -> Result<Self> {
        let tmux = Tmux::new(&dir.join("agents"))?;
        Self::with_tmux(dir, hub, worktrees, repo_agents, tmux)
    }

    /// `new`, on a given tmux (the tests' in memory).
    fn with_tmux(dir: PathBuf, hub: Hub, worktrees: Vec<WorktreeRepo>, repo_agents: Vec<(PathBuf, Provider)>, tmux: Tmux) -> Result<Self> {
        let work = dir.join("agents");
        let registry = Registry::load(dir.join("agents.json"));
        let book = costs::Book::new(costs::load(&dir.join("agent-costs.json")));
        let events = work.join("events");
        // What nothing will come for: a month's attachments, and the notes of commands whose
        // end never arrived (an agent interrupted, Prism closed) — worktree paths, kept a day.
        attach::prune(&work.join("attach"));
        hook::prune_watch(&work.join("watch"));
        for provider in Provider::ALL {
            write_wiring(&work, provider)?;
        }
        let agents = Self {
            inner: Arc::new(Inner {
                tmux,
                work,
                worktrees: Mutex::new(worktrees),
                repo_agents: Mutex::new(repo_agents),
                settings: Mutex::default(),
                events,
                folds: Mutex::default(),
                usages: Mutex::default(),
                costs: Mutex::new(book),
                refreshing: tokio::sync::Mutex::default(),
                registry: Mutex::new(registry),
                attachments: Mutex::default(),
                next_token: AtomicU64::new(1),
                hub,
                changed: Notify::new(),
                command_code_window: Mutex::default(),
                opencode_windows: Mutex::default(),
                book: Mutex::new(chains::Book::load(&dir)),
                chains_wake: Notify::new(),
                routines: Mutex::new(routines::Book::load(&dir)),
                routines_wake: Notify::new(),
                routines_live: std::sync::atomic::AtomicBool::new(false),
            }),
        };
        agents.publish_left();
        Ok(agents)
    }

    /// Applies `[agents]`: at launch, then on every Settings save.
    /// The repositories of a config read again: the next launches create their worktrees and pick
    /// their CLI by them; the agents running stay as they are.
    pub fn set_repos(&self, worktrees: Vec<WorktreeRepo>, repo_agents: Vec<(PathBuf, Provider)>) {
        *self.inner.worktrees.lock().unwrap() = worktrees;
        *self.inner.repo_agents.lock().unwrap() = repo_agents;
    }

    pub fn configure(&self, cfg: AgentsConfig) {
        *self.inner.settings.lock().unwrap() = cfg.clone();
        self.inner.hub.update(|s| {
            let changed = s.agents_settings != cfg;
            s.agents_settings = cfg;
            changed
        });
        // The routines' times computed again from it, without a restart.
        self.publish_routines_now();
        self.inner.routines_wake.notify_one();
    }

    fn enabled(&self) -> bool {
        self.inner.settings.lock().unwrap().enabled
    }

    /// Keeps the snapshot's agent list up to date with the state of their tmux sessions.
    pub async fn run(self) {
        if let Err(e) = self.inner.tmux.reload_conf().await {
            tracing::warn!("agents: reloading the tmux config: {e:#}");
        }
        loop {
            self.refresh().await;
            tokio::select! {
                _ = tokio::time::sleep(POLL) => {}
                _ = self.inner.changed.notified() => {}
            }
        }
    }

    async fn refresh(&self) {
        let panes = match self.inner.tmux.panes().await {
            Ok(p) => p,
            // Without tmux (Windows, often) and with the agents off: nothing to report.
            Err(_) if !self.enabled() && !Tmux::installed().await => {
                self.inner.hub.set_error(SOURCE, None);
                return;
            }
            Err(e) => {
                tracing::warn!("agents: {e:#}");
                self.inner.hub.set_error(SOURCE, Some(format!("{e:#}")));
                return;
            }
        };
        let records = self.inner.registry.lock().unwrap().records.clone();
        // The logs and transcripts are read on a copy, off the async workers and without holding
        // the locks: the commands (`dir`, shells, diff…) read the folds meanwhile.
        let refreshing = self.inner.refreshing.lock().await;
        let mut folds = self.inner.folds.lock().unwrap().clone();
        let mut usages = std::mem::take(&mut *self.inner.usages.lock().unwrap());
        let events = self.inner.events.clone();
        let read = tokio::task::spawn_blocking(move || {
            folds.retain(|id, _| records.iter().any(|r| &r.id == id));
            usages.retain(|id, _| records.iter().any(|r| &r.id == id));
            let list: Vec<(Agent, Option<String>)> = records.into_iter().rev().map(|r| read_agent(r, &mut folds, &mut usages, &events, &panes)).collect();
            (list, folds, usages)
        })
        .await;
        let Ok((read, folds, usages)) = read else {
            tracing::error!("agents: reading the logs panicked");
            return;
        };
        let active: HashMap<String, u64> = folds.iter().map(|(id, f)| (id.clone(), f.active_ms())).collect();
        *self.inner.folds.lock().unwrap() = folds;
        *self.inner.usages.lock().unwrap() = usages;
        drop(refreshing);
        // The window of the CLIs that do not tell it (Command Code, OpenCode): from their caches.
        let list: Vec<Agent> = read
            .into_iter()
            .map(|(mut a, model)| {
                if let Some(c) = a.context.as_mut().filter(|c| c.size.is_none()) {
                    c.size = self.window(a.provider, model.as_deref());
                }
                a
            })
            .collect();
        // The prompt Claude Code suggests at its prompt, read off its screen.
        let mut list = list;
        // Every one's screen in a single tmux call.
        let ids: Vec<String> = list.iter().filter(|a| a.provider == Provider::Claude && quick::at_prompt(a.state, a.status)).map(|a| a.id.clone()).collect();
        let screens = self.inner.tmux.screens(&ids).await;
        for a in list.iter_mut().filter(|a| ids.contains(&a.id)) {
            a.suggestion = screens.get(&a.id).and_then(|s| suggestion::read(s));
        }
        // Agents also appear among the Claude sessions: Claude block, menu bar,
        // notifications and alerts see them like the desktop ones.
        let sessions: Vec<ClaudeSession> = list.iter().filter(|a| a.state != AgentState::Prepared).map(as_session).collect();
        // What each one cost so far, kept past its end.
        self.note_costs(&list, &active);
        self.inner.hub.update(|s| {
            let mine = s.sessions.iter().filter(|c| c.origin == SessionOrigin::Agent).cloned().collect::<Vec<_>>();
            let changed = s.agents != list || mine != sessions;
            s.agents = list;
            s.sessions.retain(|c| c.origin != SessionOrigin::Agent);
            s.sessions.extend(sessions);
            changed
        });
        self.inner.hub.set_error(SOURCE, None);
    }

    /// The agent's record, as registered.
    fn record(&self, id: &str) -> Result<Record> {
        self.inner.registry.lock().unwrap().get(id).cloned().context(tr("unknown agent"))
    }

    /// The CLI the agent runs.
    fn provider(&self, id: &str) -> Result<Provider> {
        Ok(self.inner.registry.lock().unwrap().get(id).context(tr("unknown agent"))?.provider)
    }

    fn update_registry(&self, f: impl FnOnce(&mut Registry)) -> Result<()> {
        let mut reg = self.inner.registry.lock().unwrap();
        f(&mut reg);
        let saved = reg.save();
        drop(reg);
        self.inner.changed.notify_one();
        saved
    }
}

/// The worktree the agent works in, if it exists: the one the hook created, otherwise the planned one.
/// A Claude Code conversation of this id in its usual place (`~/.claude/projects/<folder>/<id>.jsonl`,
/// or under `CLAUDE_CONFIG_DIR`): its hooks may not have said where it is (an agent from before).
fn claude_conversation_exists(id: &str) -> bool {
    let root = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude")));
    let Some(projects) = root.map(|r| r.join("projects")) else { return false };
    let file = format!("{id}.jsonl");
    std::fs::read_dir(projects).into_iter().flatten().flatten().any(|d| d.path().join(&file).is_file())
}

/// An agent as its record, its hook log and (Claude Code) its transcript tell it, and the model
/// it runs; the log and transcript are read from where they were left. Blocking: disk reads.
fn read_agent(r: Record, folds: &mut HashMap<String, Fold>, usages: &mut HashMap<String, usage::UsageFold>, events: &std::path::Path, panes: &HashMap<String, Pane>) -> (Agent, Option<String>) {
    let fold = folds.entry(r.id.clone()).or_default();
    fold.read(&events.join(format!("{}.jsonl", r.id)));
    // Claude Code's transcript, named by its hooks or found by the conversation's id; Codex's
    // rollout, named by its hooks.
    let usage = r.provider.has_transcript().then(|| {
        let u = usages.entry(r.id.clone()).or_default();
        if let Some(t) = fold.transcript.as_deref() {
            u.located = Some(t.into());
        } else if u.located.is_none() && !r.prepared && r.provider.finds_transcript() && u.searched.is_none_or(|t| t.elapsed() >= SEARCH_AGAIN) {
            // A folder read per project each time: not every tick while it is still missing.
            u.located = usage::find_transcript(fold.session.as_deref().unwrap_or(&r.id));
            u.searched = Some(std::time::Instant::now());
        }
        if let Some(path) = u.located.clone() {
            u.read(&path);
        }
        if let Some(at) = u.interrupted {
            fold.interrupted(at);
        }
        u.usage.clone()
    })
    // Back on a session it ran before: what that stretch consumed is counted there.
    .map(|u| match r.resumed.as_deref() {
        Some(session) => u.minus(&carried(&r.legs, r.provider, session)),
        None => u,
    })
    .filter(|u| u.replies > 0);
    // tmux has the last word: an exited Claude is no longer working, whatever the log says.
    let (state, exit_code, status) = match panes.get(&r.id) {
        _ if r.prepared => (AgentState::Prepared, None, SessionStatus::Idle),
        Some(Pane::Alive) => (AgentState::Running, None, fold.status),
        Some(Pane::Exited(code)) => (AgentState::Exited, Some(*code), SessionStatus::Completed),
        None => (AgentState::Gone, None, SessionStatus::Idle),
    };
    let needs_action = (state == AgentState::Running).then(|| fold.needs_action.clone()).flatten();
    let worktree = effective_worktree(&r, fold);
    let adopted = fold.adopted.iter().filter(|p| own_worktree(&r, Path::new(p))).cloned().collect();
    let branch = git_branch(worktree.as_deref().unwrap_or(&r.cwd));
    let agent = Agent {
        ticket: r.ticket.clone().or_else(|| ticket::find_in_session(&r.title, branch.as_deref())),
        ticket_linked: r.ticket.is_some(),
        mr: r.mr,
        worktree: worktree.map(|w| w.display().to_string()),
        worktree_note: fold.worktree_note.clone(),
        adopted,
        also: r.extra.iter().filter(|p| p.is_dir()).map(|p| p.display().to_string()).collect(),
        id: r.id,
        provider: r.provider,
        title: r.title,
        category: r.category,
        tags: r.tags,
        cwd: r.cwd.display().to_string(),
        branch,
        prompt: r.prompt,
        launch: r.launch,
        created_at: r.created_at,
        state,
        exit_code,
        status,
        needs_action,
        last_activity: fold.last_activity.or(Some(r.created_at)),
        context: fold.context,
        model: usage.as_ref().and_then(|u| u.model.clone()).or_else(|| fold.model.clone()),
        usage,
        suggestion: None,
        planned: fold.planned,
        looping: fold.failing.clone().filter(|(_, n)| *n >= LOOPING && state == AgentState::Running).map(|(command, count)| crate::state::Looping { command, count }),
        legs: r.legs,
    };
    (agent, fold.model.clone())
}

/// What the earlier stretches of `provider` on `session` consumed: a conversation resumed holds it.
fn carried(legs: &[crate::state::AgentLeg], provider: Provider, session: &str) -> usage::AgentUsage {
    let mut sum = usage::AgentUsage::default();
    for leg in legs.iter().filter(|l| l.provider == provider && l.session.as_deref() == Some(session)) {
        if let Some(u) = &leg.usage {
            sum.add(u);
        }
    }
    sum
}

/// A CLI's wiring file, owner-only: the hooks call back this very binary, so it follows the app
/// (development, release) wherever it is; and what the file says runs in every agent of that CLI.
fn write_wiring(work: &Path, provider: Provider) -> Result<()> {
    let exe = std::env::current_exe().context("locating the Prism executable")?;
    let wiring: Vec<u8> = match provider {
        Provider::Claude => serde_json::to_vec_pretty(&hook::hooks_settings(&exe, work))?,
        Provider::CommandCode => provider::COMMAND_CODE_MOD.into(),
        Provider::OpenCode => provider::OPENCODE_PLUGIN.into(),
        Provider::Codex => serde_json::to_vec_pretty(&hook::codex_overrides(&exe, work))?,
    };
    let file = work.join(provider.wiring_file());
    // Left alone when already right: Command Code watches its mod and reloads it, in every
    // running agent, at each write ("[mods] ✓ reloaded" in their terminals).
    if std::fs::read(&file).is_ok_and(|on_disk| on_disk == wiring) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
        }
        return Ok(());
    }
    let mut open = std::fs::OpenOptions::new();
    open.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        open.mode(0o600);
        if file.exists() {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    std::io::Write::write_all(&mut open.open(&file)?, &wiring)?;
    Ok(())
}

fn effective_worktree(r: &Record, fold: &Fold) -> Option<PathBuf> {
    fold.worktree.as_ref().map(PathBuf::from).filter(|p| own_worktree(r, p)).or_else(|| r.worktree.clone()).filter(|p| p.is_dir())
}

/// A worktree its log names is the agent's: the one Prism prepared for it, or one Claude Code
/// made in its folder (`.claude/worktrees/<name>`). The log is a file any program of this user
/// can write: a path elsewhere (another agent's worktree, a home folder) would become the
/// agent's folder — served, resumed in, removed when it closes.
fn own_worktree(r: &Record, path: &Path) -> bool {
    let Ok(path) = path.canonicalize() else { return false };
    let canon = |p: &Path| p.canonicalize().ok();
    if r.worktree.as_deref().and_then(canon).is_some_and(|w| w == path) {
        return true;
    }
    [Some(r.cwd.as_path()), r.worktree.as_deref()].into_iter().flatten().filter_map(|base| canon(&base.join(".claude/worktrees"))).any(|root| path.parent() == Some(root.as_path()))
}

fn as_session(a: &Agent) -> ClaudeSession {
    ClaudeSession {
        id: a.id.clone(),
        title: a.title.clone(),
        folder: std::path::Path::new(&a.cwd).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        worktree: a.worktree.clone(),
        branch: a.branch.clone(),
        ticket: a.ticket.clone(),
        status: a.status,
        needs_action: a.needs_action.clone(),
        last_activity: a.last_activity,
        group: Some(a.category.clone().unwrap_or_else(|| "Prism agents".into())),
        group_order: None,
        order_in_group: None,
        origin: SessionOrigin::Agent,
    }
}

/// One of an agent's shells, for the Shell panel's tabs.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentShellInfo"))]
pub struct Shell {
    /// Its terminal key, `shell:<agent>:<n>`.
    pub key: String,
    pub n: u32,
    /// The program in its foreground: the shell itself, or what it runs.
    pub command: String,
    pub busy: bool,
}

/// A shell with no agent still open (`open_shells`): a worktree's, or a pod's.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpenShell {
    /// Its terminal key, `shell:<owner>:<n>`.
    pub key: String,
    pub n: u32,
    /// The worktree it runs in; `None` for a pod's.
    pub folder: Option<String>,
    pub pod: Option<PodRef>,
    /// The program in its foreground.
    pub command: String,
    /// A worktree's shell running a program rather than at its prompt.
    pub busy: bool,
    /// A pod's shell whose `kubectl exec` has exited (pod gone, rights, no shell in the image).
    pub ended: bool,
    /// When it was opened, Unix seconds.
    pub started: i64,
}

/// `wt` and the FNV-1a fingerprint of a worktree's path: never an agent's id (a UUID, hex only).
fn folder_owner(path: &str) -> String {
    fingerprint("wt", path)
}

/// `kp` and the fingerprint of a pod's container (`pod_shells`): like a worktree, no agent.
fn pod_owner(context: &str, namespace: &str, pod: &str, container: Option<&str>) -> String {
    fingerprint("kp", &[context, namespace, pod, container.unwrap_or_default()].join("\0"))
}

fn fingerprint(prefix: &str, text: &str) -> String {
    let hash = text.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
    format!("{prefix}{hash:016x}")
}

/// The owner of shells with no agent: a worktree's or a pod's.
fn is_folder_owner(id: &str) -> bool {
    id.len() == 18 && (id.starts_with("wt") || id.starts_with("kp")) && id[2..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn shell_key(id: &str, n: u32) -> String {
    format!("{}{id}:{n}", tmux::SHELL)
}

/// A shell at its prompt, rather than a program it runs (`pane_current_command`).
fn is_shell(command: &str) -> bool {
    matches!(command.trim_start_matches('-'), "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "mksh" | "tcsh" | "csh" | "nu" | "xonsh" | "elvish" | "pwsh")
}

/// Current branch of a folder, read from `HEAD` without running git (repository or worktree).
fn git_branch(dir: &std::path::Path) -> Option<String> {
    let dot = dir.join(".git");
    let git_dir = if dot.is_file() {
        let link = std::fs::read_to_string(&dot).ok()?;
        PathBuf::from(link.trim().strip_prefix("gitdir:")?.trim())
    } else {
        dot
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    head.trim().strip_prefix("ref: refs/heads/").map(String::from)
}

/// What `relabel` renames on the agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Label {
    Category,
    Tag,
}

/// `from` becomes `to` on the records carrying it (`to` empty: taken off).
fn relabel(records: &mut [Record], kind: Label, from: &str, to: &str) {
    let to = to.trim();
    for r in records {
        match kind {
            Label::Category => {
                if r.category.as_deref() == Some(from) {
                    r.category = (!to.is_empty()).then(|| to.to_string());
                }
            }
            Label::Tag => {
                if r.tags.iter().any(|t| t == from) {
                    r.tags = clean_tags(r.tags.iter().map(|t| if t == from { to.to_string() } else { t.clone() }).collect());
                }
            }
        }
    }
}

/// Tags trimmed, without empty ones nor duplicates, in their order.
fn clean_tags(tags: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t = t.trim().to_string();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

#[cfg(test)]
mod lifecycle_tests;

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn a_launch_is_kept_as_asked() {
        let launch: Launch = serde_json::from_value(serde_json::json!({
            "title": " SHOP-433 · Implement ", "cwd": "/repo", "prompt": "Implement SHOP-433.", "permission_mode": "acceptEdits",
            "worktree": { "repo": "shop", "branch": "fix/x" }, "tags": ["a", "a", " "], "provider": "codex",
            "then": [{ "kind": "task", "when": "done", "command": "npm test" }], "from": "Implement",
        }))
        .unwrap();
        let spec = launch.spec();
        assert_eq!(spec.title, "SHOP-433 · Implement");
        // With a new worktree, the folder sent along is not where it runs.
        assert_eq!(spec.cwd, None);
        assert_eq!(spec.worktree.unwrap().branch.as_deref(), Some("fix/x"));
        assert_eq!((spec.provider, spec.permission_mode.as_deref(), spec.from.as_deref()), (Some(Provider::Codex), Some("acceptEdits"), Some("Implement")));
        assert_eq!(spec.tags, ["a"]);
        assert_eq!(spec.then[0].command.as_deref(), Some("npm test"));
        // An older window sends neither steps nor origin.
        let bare: Launch = serde_json::from_value(serde_json::json!({ "title": "T", "cwd": "/repo" })).unwrap();
        assert_eq!((bare.spec().cwd.as_deref(), bare.spec().then.len()), (Some("/repo"), 0));
        // Any folder picked, under the home folder written `~/…`.
        let home: Launch = serde_json::from_value(serde_json::json!({ "title": "T", "cwd": "~/notes" })).unwrap();
        assert_eq!(home.cwd, Some(PathBuf::from(std::env::var("HOME").unwrap()).join("notes")));
    }

    /// A worktree's shells: one owner per path, the same after a restart, never taken for an agent.
    #[test]
    fn a_worktree_owns_its_shells() {
        let a = folder_owner("/Users/me/work/prism-feat");
        assert_eq!(a, folder_owner("/Users/me/work/prism-feat"));
        assert_ne!(a, folder_owner("/Users/me/work/prism-fix"));
        assert!(is_folder_owner(&a));
        assert!(!is_folder_owner("3f2b9c1e-7a4d-4e8b-9c0a-1b2c3d4e5f60"));
        // tmux session names: no `:` nor `.`, and `-` only before the shell's number.
        assert!(!a.contains([':', '.', '-']));
    }

    /// A pod's shells: one owner per container, never a worktree's nor an agent's.
    #[test]
    fn a_pod_owns_its_shells() {
        let p = pod_owner("prod", "shop", "api-7d9f-x2", None);
        assert_eq!(p, pod_owner("prod", "shop", "api-7d9f-x2", None));
        assert_ne!(p, pod_owner("prod", "shop", "api-7d9f-x2", Some("sidecar")));
        assert_ne!(p, pod_owner("prod", "shopapi-7d9f-x2", "", None), "fields kept apart");
        assert!(is_folder_owner(&p) && p.starts_with("kp"));
        assert!(!p.contains([':', '.', '-']));
    }

    /// A worktree its log names is taken only if it is the agent's: the one prepared for it, or
    /// one Claude Code made in its folder; never another folder.
    #[test]
    fn takes_only_its_own_worktree_from_its_log() {
        let dir = std::env::temp_dir().join(format!("prism-own-worktree-{}", std::process::id()));
        let (repo, planned, other) = (dir.join("repo"), dir.join("wt/fix"), dir.join("wt/other"));
        for d in [repo.join(".claude/worktrees/entered"), planned.clone(), other.clone()] {
            std::fs::create_dir_all(d).unwrap();
        }
        let r = Record { id: "a".into(), provider: Default::default(), title: "T".into(), cwd: repo.clone(), prompt: None, worktree: Some(planned.clone()), category: None, tags: Vec::new(), ticket: None, mr: None, launch: None, prepared: false, extra: Vec::new(), legs: Vec::new(), resumed: None, created_at: Utc::now() };
        assert!(own_worktree(&r, &planned));
        assert!(own_worktree(&r, &repo.join(".claude/worktrees/entered")), "one Claude Code made in its folder");
        assert!(!own_worktree(&r, &other), "another agent's");
        assert!(!own_worktree(&r, &dir), "a folder above");
        let mut fold = Fold::default();
        fold.worktree = Some(other.display().to_string());
        assert_eq!(effective_worktree(&r, &fold), Some(planned), "the forged one ignored");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Renaming a kind or a tag in Manage: the agents carrying it follow, the others stay.
    #[test]
    fn relabels_the_agents_carrying_it() {
        let record = |category: &str, tags: &[&str]| Record {
            id: "a".into(),
            provider: Default::default(),
            title: "T".into(),
            cwd: "/tmp".into(),
            prompt: None,
            worktree: None,
            category: Some(category.into()),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ticket: None,
            mr: None,
            launch: None,
            prepared: false,
            extra: Vec::new(),
            legs: Vec::new(),
            resumed: None,
            created_at: Utc::now(),
        };
        let mut records = vec![record("Review", &["api", "ui"]), record("Development", &["ui"])];
        relabel(&mut records, Label::Category, "Review", "Code review");
        relabel(&mut records, Label::Tag, "api", "ui");
        assert_eq!(records[0].category.as_deref(), Some("Code review"));
        assert_eq!(records[1].category.as_deref(), Some("Development"));
        assert_eq!(records[0].tags, ["ui"], "merged with the tag it already had");
        assert_eq!(clean_tags(vec![" a ".into(), "".into(), "b".into(), "a".into()]), ["a", "b"]);
    }

    /// An agent's full path, without Claude: tmux session, attached terminal, keystrokes,
    /// resizing, program exit. Skipped without tmux installed.
    #[tokio::test]
    async fn attaches_types_and_sees_exit() {
        if std::process::Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let s = tmux::tests::scratch("attach");
        let (tmux, dir) = (&s.tmux, &s.dir);
        let argv: Vec<String> = ["sh", "-c", "printf 'ready\\n'; read line; printf 'got:%s\\n' \"$line\"; exit 3"].map(String::from).to_vec();
        tmux.new_session("t1", dir, &[], &argv).await.unwrap();
        assert_eq!(tmux.panes().await.unwrap().get("t1"), Some(&Pane::Alive));

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let (end_tx, end_rx) = mpsc::channel::<()>();
        let mut att = Attachment::spawn("t1", &tmux.attach_argv("t1"), 100, 30, 1, move |b| { let _ = tx.send(b); }, move || { let _ = end_tx.send(()); }).unwrap();
        let mut seen = String::new();
        let until = |seen: &mut String, needle: &str| {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !seen.contains(needle) && std::time::Instant::now() < deadline {
                if let Ok(b) = rx.recv_timeout(Duration::from_millis(100)) {
                    seen.push_str(&String::from_utf8_lossy(&b));
                }
            }
            seen.contains(needle)
        };
        assert!(until(&mut seen, "ready"), "no prompt in {seen:?}");
        att.resize(80, 20).unwrap();
        att.write(b"hello\r").unwrap();
        assert!(until(&mut seen, "got:hello"), "no echo in {seen:?}");

        // Once the program has exited, the session stays on its last screen, with its code.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while tmux.panes().await.unwrap().get("t1") != Some(&Pane::Exited(3)) && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(tmux.panes().await.unwrap().get("t1"), Some(&Pane::Exited(3)));

        // Detaching the terminal does not stop the session; killing it ends the attachment.
        drop(att);
        assert!(end_rx.recv_timeout(Duration::from_secs(5)).is_ok());
        assert!(tmux.panes().await.unwrap().contains_key("t1"));
        tmux.kill("t1").await.unwrap();
        assert!(tmux.panes().await.unwrap().is_empty());
    }

    /// Detaching a terminal sends nothing to the program: portable-pty's writer, dropped, types
    /// `\n` and Ctrl-D into the tmux client, which passed them on (a dev server read the end
    /// of its input and quit). Several rounds: it was a race with the client's death.
    #[tokio::test]
    async fn detaching_sends_nothing_to_the_program() {
        if std::process::Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let s = tmux::tests::scratch("detach");
        let (tmux, dir) = (&s.tmux, &s.dir);
        let out = dir.join("typed");
        let argv: Vec<String> = ["sh", "-c", &format!("cat > '{}'", out.display())].map(String::from).to_vec();
        tmux.new_session("d1", dir, &[], &argv).await.unwrap();
        for round in 0..5 {
            let att = Attachment::spawn("d1", &tmux.attach_argv("d1"), 80, 24, round, |_| {}, || {}).unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
            drop(att);
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert_eq!(tmux.panes().await.unwrap().get("d1"), Some(&Pane::Alive), "cat read an end of input at round {round}");
        }
        assert_eq!(std::fs::read_to_string(&out).unwrap_or_default(), "", "something was typed");
        tmux.kill("d1").await.unwrap();
    }
}
