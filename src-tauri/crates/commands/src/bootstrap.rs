//! The collectors of the active profile's config: started at launch, then stopped and started
//! again whenever what they read of the config changes (`Collectors::reload`), without a restart
//! of Prism. A missing or invalid config does not prevent the app from opening: the error shows
//! in Alerts.

use prism_base::tr;
use std::collections::BTreeMap;

use prism_base::rt::Generation;

use crate::{agents, blocks, collectors, config, config_path, conventions, demo, forge, platform, profile, stages, state, tracker};

/// What the app state needs from the config, once the collectors are started.
pub struct Started {
    /// The audio sources (the default ones without a config).
    pub audio: config::AudioConfig,
    /// The reading of environment logs.
    pub pod_logs: collectors::environments::PodLogs,
    pub agents: AgentsSetup,
    pub forge: forge::Forge,
    pub tracker: collectors::tracker::Tracker,
    /// `[remote]`, applied once the app runs; none without a config.
    pub remote: Option<config::RemoteConfig>,
    /// The blocks were started, those that do not reload included (a config was read, outside
    /// the demo).
    blocks: bool,
}

/// What the agents start with.
#[derive(Default)]
pub struct AgentsSetup {
    /// The repositories where agents create their worktrees.
    pub worktrees: Vec<config::WorktreeRepo>,
    /// The repositories' agent providers.
    pub repos: Vec<(std::path::PathBuf, agents::provider::Provider)>,
    /// `[agents]`.
    pub cfg: config::AgentsConfig,
    /// The demo profile: sample agents, the real ones (tmux) do not run.
    pub demo: bool,
}

impl Started {
    /// Without a readable config: the defaults, GitLab and ClickUp, nothing started.
    fn without_config(team_id: collectors::clickup::TeamId) -> Self {
        Self {
            audio: config::AudioConfig::default(),
            pod_logs: Default::default(),
            agents: AgentsSetup::default(),
            forge: forge::Forge::new(forge::Actions::Gitlab(Default::default())),
            tracker: collectors::tracker::Tracker::Clickup(collectors::clickup::Actions::new(team_id)),
            remote: None,
            blocks: false,
        }
    }
}

/// The collectors running, from the config as last read, in parts (`Part`): when what a part
/// reads of the config changes, it alone stops and starts again from the new one (`reload`), what
/// it brought gone meanwhile; what they all read (the repositories, the conventions…) starts them
/// all again. The window, the agents, the remote and the blocks that do not reload run on untouched.
pub struct Collectors {
    hub: state::Hub,
    time: collectors::time::TimeService,
    team_id: collectors::clickup::TeamId,
    running: std::sync::Mutex<Running>,
}

#[derive(Default)]
struct Running {
    /// What all the parts read of the config; none without a readable config.
    common: Option<serde_json::Value>,
    /// Each part's generation (what it spawned, stopped together) and what it read of the config.
    parts: BTreeMap<Part, (Generation, serde_json::Value)>,
    /// The forges' actions, configured by the code part: kept while it runs on.
    forges: Forges,
    /// The tracker's actions, made by its part: kept while it runs on.
    tracker: Option<collectors::tracker::Tracker>,
    /// The merged config the blocks that do not reload started on; none before they did.
    blocks: Option<toml::Table>,
}

/// A part of the collectors, started again on its own when what it reads of the config changes:
/// a reload for `[docker]` no longer reads every transcript, fetches every repository and lists
/// every cluster again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    /// The forge and the repositories: worktrees, MRs and pipelines, CI health, Renovate,
    /// vulnerabilities, the Repos menu.
    Code,
    /// Pods, environments, ArgoCD.
    Cluster,
    /// The tracker and its time tracking.
    Tracker,
    Mail,
    Calendar,
    /// Claude Desktop's sessions.
    Sessions,
    /// The overlaps between agents and worktrees (`[agents.overlaps]`).
    Overlaps,
    /// A block that reloads, alone.
    Block(&'static str),
}

const PARTS: [Part; 7] = [Part::Code, Part::Cluster, Part::Tracker, Part::Mail, Part::Calendar, Part::Sessions, Part::Overlaps];

impl Part {
    /// The tables of the config only it reads (`reload_keys`).
    fn tables(self) -> &'static [&'static str] {
        match self {
            Part::Code => &["forge", "gitlab", "worktrees", "vulnerabilities", "renovate"],
            Part::Cluster => &["clusters", "pods", "environments", "argocd", "argocd_layout"],
            Part::Tracker => &["tracker", "clickup", "obsidian", "jira"],
            Part::Mail => &["mail"],
            Part::Calendar => &["calendar"],
            Part::Sessions => &["agents"],
            Part::Overlaps => &[],
            Part::Block(_) => &[],
        }
    }

    /// Its collectors' sources, their errors gone with what they brought.
    fn sources(self) -> Vec<&'static str> {
        match self {
            Part::Code => vec!["gitlab", "github", "gitlab repos", "github repos", "ci_health", "renovate", "vulnerabilities", "repos", "worktrees"],
            Part::Cluster => vec!["pods", "metrics", "environments", "argocd"],
            Part::Tracker => vec!["clickup", "obsidian", "jira", "time"],
            Part::Mail => vec!["mail"],
            Part::Calendar => vec!["calendar"],
            Part::Sessions => vec!["sessions"],
            Part::Overlaps => vec!["overlaps"],
            Part::Block(id) => std::iter::once(id).chain(blocks::find(id).map(|b| b.parts().to_vec()).unwrap_or_default()).collect(),
        }
    }

    /// What it brought, gone before it starts again (the worktrees and Claude Desktop's sessions
    /// stay until read again, as in a full reload).
    fn forget(self, s: &mut state::Snapshot) {
        match self {
            Part::Code => {
                (s.me, s.me_elsewhere, s.merge_queue_label, s.merge_priority_prefix) = (None, None, None, None);
                (s.repos, s.merge_requests, s.pipelines, s.pending_jobs) = Default::default();
                (s.ci_health, s.vulnerabilities, s.renovate, s.features) = Default::default();
            }
            Part::Cluster => {
                (s.pods, s.pod_usage, s.pod_nodes, s.environments, s.argocd) = Default::default();
            }
            Part::Tracker => {
                (s.sprint, s.next_sprint, s.time_tasks, s.refinement, s.pickup, s.outside_sprint) = Default::default();
                (s.time, s.tracker_me, s.clickup_team, s.ticket_base) = Default::default();
            }
            Part::Mail => s.mail = None,
            Part::Calendar => s.agenda = None,
            Part::Sessions => {}
            Part::Overlaps => s.overlaps = Default::default(),
            Part::Block(id) => drop(s.blocks.remove(id)),
        }
        let sources = self.sources();
        s.errors.retain(|source, _| !sources.contains(source));
        s.answered.retain(|source| !sources.contains(source));
    }
}

impl Collectors {
    /// At launch: the collectors of the active profile's config.
    pub fn start(hub: &state::Hub, time: &collectors::time::TimeService, team_id: collectors::clickup::TeamId) -> (Self, Started) {
        // The active profile, named by its config once read; its id even without one.
        let id = profile::active().to_string();
        hub.update(|s| {
            s.profile = state::ActiveProfile { name: if id == profile::DEFAULT { "Default".into() } else { id.clone() }, id: id.clone(), color: String::new(), shared: profile::names().len() > 1, fresh: !config_path().exists() };
            true
        });
        let cfg = read(hub);
        let mut running = Running::default();
        if let Some(cfg) = &cfg {
            prepare(hub, cfg);
        }
        let keys = cfg.as_ref().map(reload_keys);
        let gens: BTreeMap<Part, Generation> = keys.iter().flat_map(|k| k.parts.keys()).map(|p| (*p, Generation::default())).collect();
        let table = cfg.as_ref().map(|c| c.table.clone());
        let started = start(hub, time, team_id.clone(), cfg, &mut running, &gens, false);
        if let Some(keys) = keys {
            running.common = Some(keys.common);
            running.parts = keys.parts.into_iter().map(|(p, key)| (p, (gens[&p].clone(), key))).collect();
        }
        running.blocks = table.filter(|_| started.blocks);
        (Self { hub: hub.clone(), time: time.clone(), team_id, running: std::sync::Mutex::new(running) }, started)
    }

    /// `cfg`, read again after a change: the parts whose reading of it changed (all of them when
    /// what they all read did, or `force`) stop, what they brought leaves the snapshot, and they
    /// start again from it; what the commands need of it is returned. Otherwise nothing: the rest
    /// applies live.
    pub fn reload(&self, cfg: &config::Config, force: bool) -> Option<Started> {
        let mut running = self.running.lock().expect("collectors");
        let waiting: Vec<String> = running.blocks.as_ref().map(|launch| blocks::waiting_restart(launch, &cfg.table)).unwrap_or_default().into_iter().map(String::from).collect();
        self.hub.update(|s| {
            let changed = s.waiting_restart != waiting;
            s.waiting_restart = waiting;
            changed
        });
        prepare(&self.hub, cfg);
        let keys = reload_keys(cfg);
        let all = force || running.common.as_ref() != Some(&keys.common);
        let changed: Vec<Part> = keys.parts.keys().chain(running.parts.keys()).copied().collect::<std::collections::BTreeSet<_>>().into_iter().filter(|p| all || running.parts.get(p).map(|(_, key)| key) != keys.parts.get(p)).collect();
        if changed.is_empty() {
            return None;
        }
        tracing::info!("config: the collectors start again: {}", if all { "all".to_string() } else { format!("{changed:?}") });
        for p in &changed {
            if let Some((generation, _)) = running.parts.remove(p) {
                generation.cancel();
            }
        }
        self.hub.update(|s| {
            if all {
                forget_collected(s);
            } else {
                changed.iter().for_each(|p| p.forget(s));
            }
            true
        });
        let gens: BTreeMap<Part, Generation> = changed.iter().filter(|p| keys.parts.contains_key(p)).map(|p| (*p, Generation::default())).collect();
        let first_blocks = running.blocks.is_none();
        let started = start(&self.hub, &self.time, self.team_id.clone(), Some(cfg.clone()), &mut running, &gens, !first_blocks);
        if first_blocks && started.blocks {
            running.blocks = Some(cfg.table.clone());
        }
        for (p, generation) in gens {
            running.parts.insert(p, (generation, keys.parts[&p].clone()));
        }
        running.common = Some(keys.common);
        Some(started)
    }
}

/// The config, its error in Alerts when it does not read.
fn read(hub: &state::Hub) -> Option<config::Config> {
    let path = config_path();
    match config::load(&path) {
        Ok(cfg) => Some(cfg),
        // First launch: no config yet. Say how to start, rather than a reading error.
        Err(_) if !path.exists() => {
            hub.set_error("config", Some(format!("no config yet: open Settings, import a pack, or copy config.example.toml to {}", path.display())));
            None
        }
        Err(e) => {
            tracing::error!("config: {e:#}");
            hub.set_error("config", Some(format!("{e:#}")));
            None
        }
    }
}

/// What the collectors read of the config, apart: what they all read, and what each part does.
struct Keys {
    common: serde_json::Value,
    parts: BTreeMap<Part, serde_json::Value>,
}

/// All of the config but what applies live (`commands::config`), split by part (`Part::tables`);
/// the rest, and the conventions they all follow, common. Run after `prepare` (the stages).
fn reload_keys(cfg: &config::Config) -> Keys {
    let mut common = serde_json::to_value(cfg).unwrap_or_default();
    if let Some(table) = common.as_object_mut() {
        for live in ["profile", "open_with", "console", "palette", "files", "notes", "dictation", "appearance", "views", "templates", "alerts", "remote", "updates", "feedback", "logs", "audio"] {
            table.remove(live);
        }
    }
    // `[agents]` applies live, but for Claude Desktop's sessions, read by their collector.
    let sessions = (&cfg.agents.claude.sessions_dir, &cfg.agents.claude.transcripts, cfg.agents.claude.sessions_poll);
    common["agents"] = serde_json::to_value(sessions).unwrap_or_default();
    if let Some(stale) = common.pointer_mut("/worktrees/stale_days") {
        *stale = serde_json::Value::Null;
    }
    let mut parts = BTreeMap::new();
    for part in PARTS {
        let read: serde_json::Map<String, serde_json::Value> = part.tables().iter().filter_map(|t| common.as_object_mut()?.remove(*t).map(|v| (t.to_string(), v))).collect();
        parts.insert(part, serde_json::Value::Object(read));
    }
    // The stages (a block's, or `[[environments]]`') name ArgoCD's applications.
    // `[agents]` applies live, but for its overlaps, read by their own collector.
    // With the repositories it compares, as `start` gives them.
    let compared: Vec<_> = if cfg.forge.enabled { cfg.worktree_repos().into_iter().map(|r| (r.name, r.path, r.main_branch, r.release_prefix, r.base_branch)).collect() } else { Vec::new() };
    parts.insert(Part::Overlaps, serde_json::json!([cfg.agents.overlaps, compared]));
    parts.get_mut(&Part::Cluster).expect("cluster")["stages"] = serde_json::to_value(&stages::get().0).unwrap_or_default();
    common["conventions"] = serde_json::to_value(conventions::Conventions::of(cfg)).unwrap_or_default();
    // A block reads its table, and what the core resolved of the repositories it reads.
    for b in blocks::all().iter().filter(|b| b.reloads()) {
        let env = blocks::env_of(cfg, *b);
        let repos: Vec<_> = env.repos.iter().map(|r| (&r.name, &r.project, &r.clone)).collect();
        parts.insert(Part::Block(b.id()), serde_json::json!([b.settings(&cfg.table), [serde_json::json!(env.forge), serde_json::json!(env.code), serde_json::json!(repos), serde_json::json!(env.gitlab)]]));
    }
    Keys { common, parts }
}

/// What the collectors brought, gone before they start again. Kept: the agents and what they
/// read (worktrees, sessions), the profile, the plan usage, the shared stores, the remote, the
/// settings shown (set again right after), and the blocks that do not reload.
fn forget_collected(s: &mut state::Snapshot) {
    let old = std::mem::take(s);
    let kept = |source: &str| matches!(source, "usage" | "config") || blocks::kept(source);
    *s = state::Snapshot {
        profile: old.profile,
        other_profiles: old.other_profiles,
        agents: old.agents,
        left: old.left,
        agents_settings: old.agents_settings,
        worktrees: old.worktrees,
        sessions: old.sessions,
        archived_sessions: old.archived_sessions,
        usage: old.usage,
        other_usage: old.other_usage,
        shared_revs: old.shared_revs,
        remote: old.remote,
        waiting_restart: old.waiting_restart,
        open_with: old.open_with,
        console: old.console,
        palette: old.palette,
        files: old.files,
        notes: old.notes,
        dictation: old.dictation,
        appearance: old.appearance,
        views: old.views,
        templates: old.templates,
        alerts_settings: old.alerts_settings,
        conventions: old.conventions,
        blocks: old.blocks.into_iter().filter(|(id, _)| kept(id)).collect(),
        errors: old.errors.into_iter().filter(|(source, _)| kept(source)).collect(),
        problems: old.problems,
        answered: old.answered.into_iter().filter(|source| kept(source)).collect(),
        ..Default::default()
    };
}

/// What comes before any collector, at each start and reload: the repositories git trusts, the
/// conventions, what the blocks declare (the stages of a block providing them), the stages.
fn prepare(hub: &state::Hub, cfg: &config::Config) {
    // Before any git: the repositories whose `.git` it trusts (an agent's folder may hold another).
    prism_base::git::trust(cfg.repositories().into_iter().filter_map(|r| r.path));
    // Before any collector: they read the ticket prefix, GitLab host and branches here.
    conventions::set(conventions_of(cfg));
    blocks::configure(&cfg.table, hub);
    set_stages(cfg, hub);
}

/// The conventions, the primary repository's main branch read from its clone when none is declared
/// (`origin/HEAD`; otherwise `main`).
pub fn conventions_of(cfg: &config::Config) -> conventions::Conventions {
    let mut c = conventions::Conventions::of(cfg);
    if cfg.primary_branches().0.is_empty()
        && let Some(dir) = cfg.primary().and_then(|r| r.path)
        && let Ok(out) = std::process::Command::new("git").arg("-C").arg(&dir).args(["rev-parse", "--abbrev-ref", "origin/HEAD"]).output()
        && let Some(main) = String::from_utf8_lossy(&out.stdout).trim().strip_prefix("origin/").filter(|b| !b.is_empty())
    {
        c.main_branch = main.to_string();
    }
    c
}

/// Runs `start` in the generation of `part`, when it starts (`gens`).
fn within(gens: &BTreeMap<Part, Generation>, part: Part, start: impl FnOnce()) {
    if let Some(generation) = gens.get(&part) {
        generation.enter(start);
    }
}

/// Starts the parts of the collectors of `cfg` in `gens` (none without a config), after `prepare`.
/// `reloading`: the blocks that do not reload run on from launch.
fn start(hub: &state::Hub, time: &collectors::time::TimeService, team_id: collectors::clickup::TeamId, cfg: Option<config::Config>, running: &mut Running, gens: &BTreeMap<Part, Generation>, reloading: bool) -> Started {
    let Some(cfg) = cfg else { return Started::without_config(team_id) };
    let conventions = conventions_of(&cfg);
    if gens.contains_key(&Part::Code) {
        running.forges = Forges::default();
        // Another account or instance, maybe: its members, labels and emoji read again.
        collectors::gitlab::review::forget_cached();
        collectors::github::review::forget_cached();
    }
    let forge = running.forges.of(&cfg);
    if gens.contains_key(&Part::Tracker) || running.tracker.is_none() {
        running.tracker = Some(tracker_of(&cfg, team_id.clone()));
        collectors::clickup::forget_members();
    }
    let tracker = running.tracker.clone().expect("tracker");
    // The demo profile: its sample data, and no collector at all.
    if cfg.demo.enabled {
        show_settings(hub, &cfg, conventions);
        demo::fill(hub, chrono::Utc::now());
        return Started { audio: cfg.audio, pod_logs: Default::default(), agents: AgentsSetup { cfg: cfg.agents, demo: true, ..Default::default() }, forge, tracker, remote: Some(cfg.remote), blocks: false };
    }
    // Without access to the clusters (`[clusters] enabled = false`): no Kubernetes collector.
    let cluster = cfg.clusters.enabled;
    // Without access to the forge (`[forge] enabled = false`), neither it nor the repositories.
    let code = cfg.forge.enabled;
    let primary = Primary::of(&cfg);
    // The repositories whose worktrees are listed, where agents create theirs.
    let worktree_repos = if code { cfg.worktree_repos() } else { Vec::new() };
    hub.update(|s| {
        s.repositories = cfg.repositories().iter().map(config::RepoConfig::info).collect();
        s.repo_blocks = blocks::repo_blocks(&cfg);
        // Kept through a reload until read again: none to read any more.
        if worktree_repos.is_empty() {
            s.worktrees.clear();
        }
        true
    });
    if code {
        within(gens, Part::Code, || start_code(&cfg, &primary, &worktree_repos, &running.forges, hub));
    }
    // Overlaps compare the worktrees listed: none without the repositories.
    within(gens, Part::Overlaps, || spawn(hub, "overlaps", collectors::overlaps::run(cfg.agents.overlaps.clone(), worktree_repos.clone(), hub.clone())));
    if cluster {
        within(gens, Part::Cluster, || start_cluster(&cfg, &primary, hub));
    }
    // The blocks with a source of their own (`blocks`): each reads its table and starts if on.
    // A block names a repository no `[[repos]]` entry has: said in Alerts, under its id.
    for e in blocks::missing_repos(&cfg) {
        tracing::warn!("blocks: {e}");
    }
    blocks::start(&cfg.table, &|b| blocks::env_of(&cfg, b), hub, reloading, &|id, start| within(gens, Part::Block(id), start));
    start_inbox(&cfg, gens, hub);
    show_settings(hub, &cfg, conventions);
    hub.update(|s| {
        s.cluster = cluster;
        s.pod_shell = cluster && cfg.clusters.shell;
        s.pod_history = cluster && cfg.clusters.usage_history;
        s.gitlab = code;
        s.calendar = cfg.calendar.enabled;
        true
    });
    within(gens, Part::Tracker, || start_tracker(&cfg, &tracker, time, team_id, hub));
    // Claude Desktop sessions: those of this profile's folders, and those of no other profile.
    within(gens, Part::Sessions, || spawn(hub, "sessions", collectors::sessions::run(cfg.agents.claude.sessions(), profile::SessionScope::of(&cfg), hub.clone())));
    within(gens, Part::Calendar, || collectors::calendar::run(cfg.calendar.clone(), hub.clone()));
    let repo_agents = cfg.repo_agents();
    let envs = if cluster { cfg.environments } else { Vec::new() };
    Started {
        audio: cfg.audio,
        pod_logs: collectors::environments::PodLogs::new(envs),
        agents: AgentsSetup { worktrees: worktree_repos, repos: repo_agents, cfg: cfg.agents, demo: false },
        forge,
        tracker,
        remote: Some(cfg.remote),
        blocks: true,
    }
}

/// Runs a collector, in the generation being started; one that panics is said in the log and in
/// Alerts, where it used to vanish: nothing starts it again before the config changes (or Reload
/// collectors). One that returns has nothing (more) to read; one stopped by a reload, nothing to say.
fn spawn<F>(hub: &state::Hub, name: &'static str, collector: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let task = prism_base::rt::spawn(collector);
    let hub = hub.clone();
    prism_base::rt::spawn(async move {
        match task.await {
            Ok(()) => tracing::debug!("{name}: the collector returned"),
            Err(e) if e.is_cancelled() => {}
            Err(e) => {
                tracing::error!("{name}: the collector panicked: {e}");
                hub.set_error("collectors", Some(tr!("{name}: the collector panicked; Reload collectors (console, palette) to read it again", name = name)));
            }
        }
    });
}

/// The stages Environments, ArgoCD and Settings read (`[clusters] stages`): their provider's, or
/// `[[environments]]`' names.
fn set_stages(cfg: &config::Config, hub: &state::Hub) {
    let provider = stages::provider(&cfg.clusters.stages).unwrap_or_else(|e| {
        hub.set_error("stages", Some(format!("{e:#}")));
        None
    });
    let staged = provider.map_or_else(|| stages::Stages::of_environments(&cfg.environments), |p| p.stages());
    stages::set(staged.clone(), provider);
    hub.update(|s| {
        s.stages = staged.0.clone();
        true
    });
}

/// Both forges' actions: the configured one serves the commands, and is configured at start.
#[derive(Default)]
struct Forges {
    gitlab: collectors::gitlab::Actions,
    github: collectors::github::Actions,
}

impl Forges {
    fn actions(&self, provider: forge::Provider) -> forge::Actions {
        match provider {
            forge::Provider::Gitlab => forge::Actions::Gitlab(self.gitlab.clone()),
            forge::Provider::Github => forge::Actions::Github(self.github.clone()),
        }
    }

    /// The profile's forge, and the other one for the MRs of the repositories there.
    fn of(&self, cfg: &config::Config) -> forge::Forge {
        let elsewhere = elsewhere_repos(cfg);
        forge::Forge::new(self.actions(cfg.forge.provider)).with_elsewhere(self.actions(other(cfg.forge.provider)), elsewhere)
    }
}

fn other(provider: forge::Provider) -> forge::Provider {
    match provider {
        forge::Provider::Gitlab => forge::Provider::Github,
        forge::Provider::Github => forge::Provider::Gitlab,
    }
}

/// The repositories on the other forge whose MRs are followed (`[[repos]] forge`): (name, project).
fn elsewhere_repos(cfg: &config::Config) -> Vec<(String, String)> {
    cfg.repositories().iter().filter_map(config::RepoConfig::mr_project_elsewhere).collect()
}

/// The tracker (`[tracker] provider`): ClickUp, Obsidian's boards, or Jira.
fn tracker_of(cfg: &config::Config, team_id: collectors::clickup::TeamId) -> collectors::tracker::Tracker {
    match cfg.tracker.provider {
        tracker::Provider::Clickup => collectors::tracker::Tracker::Clickup(collectors::clickup::Actions::new(team_id)),
        tracker::Provider::Obsidian => collectors::tracker::Tracker::Obsidian(collectors::obsidian::Actions::new(&cfg.obsidian)),
        tracker::Provider::Jira => collectors::tracker::Tracker::Jira(collectors::jira::Actions::new(&cfg.jira)),
    }
}

/// The primary repository (`[forge] bare_mr_repo`, `[[repos]] primary`): the one its MRs, read with
/// its pipelines, have no repository name; without it, Prism follows only the other repositories'
/// MRs.
struct Primary {
    gitlab: config::GitlabConfig,
    project: Option<String>,
    clone: Option<std::path::PathBuf>,
}

impl Primary {
    fn of(cfg: &config::Config) -> Self {
        let gitlab = cfg.gitlab_of_primary();
        let project = (!gitlab.project.is_empty()).then(|| gitlab.project.clone());
        Self { gitlab, project, clone: cfg.primary().and_then(|r| r.path) }
    }
}

/// The forge and the repositories: worktrees, MRs and pipelines, CI health, Renovate,
/// vulnerabilities, the Repos menu.
fn start_code(cfg: &config::Config, primary: &Primary, worktree_repos: &[config::WorktreeRepo], forges: &Forges, hub: &state::Hub) {
    let Primary { gitlab: gitlab_cfg, project, .. } = primary;
    let on_gitlab = cfg.forge.provider == forge::Provider::Gitlab;
    let repositories = cfg.repositories();
    if !worktree_repos.is_empty() {
        spawn(hub, "worktrees", collectors::worktrees::run(cfg.worktrees.clone(), worktree_repos.to_vec(), hub.clone()));
    }
    // The other repositories whose MRs join the primary one's (`[[repos]] merge_requests`).
    let mr_repos: Vec<(String, String)> = repositories.iter().filter_map(config::RepoConfig::mr_project).collect();
    // With the protected branches' pipelines of the repositories with `[[repos]] pipelines`.
    if on_gitlab {
        forges.gitlab.configure(gitlab_cfg, mr_repos.clone(), repositories.iter().map(config::RepoConfig::info).collect());
        let (label, priority) = (cfg.gitlab.merge_queue_label.clone(), cfg.gitlab.merge_priority_prefix.clone());
        hub.update(|s| {
            (s.merge_queue_label, s.merge_priority_prefix) = (Some(label), Some(priority));
            true
        });
        spawn(hub, "gitlab", collectors::gitlab::run(gitlab_cfg.clone(), cfg.forge.clone(), mr_repos, cfg.pipeline_repos(), hub.clone()));
    } else {
        let repos: collectors::github::Repos = project.clone().map(|p| (None, p)).into_iter().chain(mr_repos.into_iter().map(|(name, p)| (Some(name), p))).collect();
        forges.github.configure(repos.clone(), cfg.pipeline_repos().into_iter().map(|r| r.project).collect());
        spawn(hub, "github", collectors::github::run(cfg.forge.clone(), repos, cfg.pipeline_repos(), hub.clone()));
    }
    // The MRs of the repositories on the other forge, read by its own collector beside this one.
    let elsewhere = elsewhere_repos(cfg);
    if !elsewhere.is_empty() {
        if on_gitlab {
            forges.github.configure(elsewhere.iter().map(|(name, p)| (Some(name.clone()), p.clone())).collect(), Vec::new());
            spawn(hub, "github repos", collectors::github::run_elsewhere(cfg.forge.clone(), elsewhere, hub.clone()));
        } else {
            forges.gitlab.configure(&config::GitlabConfig { project: String::new(), merge_queue_label: String::new(), ..cfg.gitlab.clone() }, elsewhere.clone(), Vec::new());
            spawn(hub, "gitlab repos", collectors::gitlab::run_elsewhere(cfg.forge.clone(), elsewhere, hub.clone()));
        }
    }
    // One switch per feature reading a source of its own, each in its repository: Vulnerabilities,
    // Renovate, CI health (a block has its own: `blocks`).
    let ci_health = cfg.ci_health_repo().filter(|r| r.forge.is_none()).and_then(|r| cfg.pipeline_repo(&r));
    let renovate = cfg.renovate_repo().filter(|r| r.forge.is_none()).and_then(|r| r.project);
    let scanned: Vec<collectors::vulnerabilities::ScanRepo> = cfg
        .vulnerability_repos()
        .into_iter()
        .filter_map(|r| {
            let (main_branch, release_prefix, _) = cfg.branches(&r);
            Some(collectors::vulnerabilities::ScanRepo { clone: r.path.clone()?, project: r.project.clone(), name: r.name, main_branch, release_prefix })
        })
        .collect();
    let features = state::Features {
        // Trivy reads the local clones and the images' registry: GitLab's or GitHub's.
        vulnerabilities: cfg.vulnerabilities.enabled && !scanned.is_empty(),
        renovate: cfg.renovate.enabled && renovate.is_some(),
        ci_health: cfg.forge.ci_health && ci_health.is_some(),
    };
    hub.update(|s| {
        s.features = features.clone();
        true
    });
    collectors::gitlab::renovate::configure(&cfg.renovate, renovate.clone());
    if let (Some(repo), true) = (ci_health, features.ci_health) {
        if on_gitlab {
            spawn(hub, "ci health", collectors::gitlab::ci_health::run(repo, cfg.forge.ci_health_days, hub.clone()));
        } else {
            spawn(hub, "ci health", collectors::github::ci_health::run(repo, cfg.forge.ci_health_days, hub.clone()));
        }
    }
    if let (Some(project), true) = (renovate, features.renovate) {
        if on_gitlab {
            spawn(hub, "renovate", collectors::gitlab::renovate::run(config::GitlabConfig { project, ..gitlab_cfg.clone() }, hub.clone()));
        } else {
            spawn(hub, "renovate", collectors::github::renovate::run(project, hub.clone()));
        }
    }
    // Images and Repos menu: the repositories that have a local clone, the primary one first.
    let local: Vec<config::RepoConfig> = repositories.iter().filter(|r| r.path.is_some()).cloned().collect();
    if features.vulnerabilities {
        let vulns = cfg.vulnerabilities.clone();
        // The images to scan (`[vulnerabilities] images`): their provider's, or none; read in the
        // repository of the block providing them.
        let provider = collectors::vulnerabilities::image_provider(&vulns.images).unwrap_or_else(|e| {
            hub.set_error("vulnerabilities", Some(format!("{e:#}")));
            None
        });
        let images = provider.and_then(|provider| {
            let env = blocks::find(vulns.images.trim()).map(|b| blocks::env_of(cfg, b))?;
            let local = local.iter().filter_map(|r| Some(collectors::vulnerabilities::LocalRepo { name: r.name.clone(), clone: r.path.clone()? })).collect();
            Some(collectors::vulnerabilities::Images { provider, clone: env.clone?, project: env.project?, local })
        });
        spawn(hub, "vulnerabilities", collectors::vulnerabilities::run(scanned, vulns, images, hub.clone()));
    }
    spawn(hub, "repos", collectors::repos::run(local, !worktree_repos.is_empty(), hub.clone()));
}

/// The clusters: pods, environments, ArgoCD.
fn start_cluster(cfg: &config::Config, primary: &Primary, hub: &state::Hub) {
    if !cfg.pods.is_empty() {
        spawn(hub, "pods", collectors::pods::run(cfg.pods.clone(), cfg.clusters.clone(), hub.clone()));
    }
    // The stages' provider (a block) groups the workloads with its repository's applications.
    let clone = blocks::find(cfg.clusters.stages.trim()).map_or_else(|| primary.clone.clone(), |b| blocks::env_of(cfg, b).clone);
    spawn(hub, "environments", collectors::environments::run(cfg.environments.clone(), clone, hub.clone()));
    // Discovery reads the default context's cluster: without it nor declared instances, nothing to read.
    if !cfg.argocd.is_empty() || !cfg.clusters.default_context.is_empty() {
        spawn(hub, "argocd", collectors::argocd::run(cfg.argocd.clone(), collectors::argocd::Layout::of(&cfg.argocd_layout, stages::get()), cfg.clusters.default_context.clone(), hub.clone()));
    }
}

/// The Google account Gmail and Google Calendar sign in with, and the mail inbox.
fn start_inbox(cfg: &config::Config, gens: &BTreeMap<Part, Generation>, hub: &state::Hub) {
    // The permissions of those turned on.
    collectors::google::init(
        cfg.google.clone(),
        cfg.mail.enabled && cfg.mail.provider == config::MailProvider::Gmail,
        cfg.calendar.enabled && cfg.calendar.provider == config::CalendarProvider::Google,
    );
    // The Microsoft 365 or Gmail inbox: off by default (`[mail] enabled`).
    if cfg.mail.enabled {
        within(gens, Part::Mail, || {
            // The block knows it is on before the first read answers.
            hub.update(|s| {
                s.mail = Some(state::MailState { provider: cfg.mail.provider, ..Default::default() });
                true
            });
            spawn(hub, "mail", collectors::mail::run(cfg.mail.clone(), hub.clone()));
        });
    }
}

/// The tracker's collector and its time tracking, and what the window offers of them.
fn start_tracker(cfg: &config::Config, tracker: &collectors::tracker::Tracker, time: &collectors::time::TimeService, team_id: collectors::clickup::TeamId, hub: &state::Hub) {
    // Without ClickUp (`[clickup] enabled = false`): no tickets, sprint nor time entries, and no token needed.
    let tracked = match cfg.tracker.provider {
        tracker::Provider::Clickup => cfg.clickup.enabled,
        tracker::Provider::Obsidian | tracker::Provider::Jira => true,
    };
    hub.update(|s| {
        s.tracker = tracked.then_some(cfg.tracker.provider);
        s.tracker_features = s.tracker.map(tracker::Provider::features).unwrap_or_default();
        s.tracker_features.time &= match cfg.tracker.provider {
            tracker::Provider::Jira => cfg.jira.time_tracking,
            _ => cfg.clickup.time_tracking,
        };
        true
    });
    // Jira's worklogs, or ClickUp's entries.
    time.use_jira(match tracker {
        collectors::tracker::Tracker::Jira(j) if cfg.jira.time_tracking => Some(j.clone()),
        _ => None,
    });
    match tracker {
        collectors::tracker::Tracker::Clickup(_) if tracked => {
            spawn(hub, "clickup", collectors::clickup::run(cfg.clickup.clone(), hub.clone(), team_id));
            if cfg.clickup.time_tracking {
                spawn(hub, "time", time.clone().run());
            }
        }
        collectors::tracker::Tracker::Obsidian(o) => {
            spawn(hub, "obsidian", o.clone().run(hub.clone()));
        }
        collectors::tracker::Tracker::Jira(j) => {
            spawn(hub, "jira", j.clone().run(hub.clone()));
            if cfg.jira.time_tracking {
                spawn(hub, "time", time.clone().run());
            }
        }
        collectors::tracker::Tracker::Clickup(_) => {}
    }
}

/// What the window shows of the config itself: conventions, alert thresholds, pages, console,
/// notes, appearance, the profile's name and color, the apps to open a folder in.
pub fn show_settings(hub: &state::Hub, cfg: &config::Config, conventions: conventions::Conventions) {
    let open_with = state::OpenWith { editor: cfg.open_with.editor.clone(), terminal: Some(cfg.open_with.terminal.clone().unwrap_or_else(|| platform::DEFAULT_TERMINAL.into())) };
    // Prism's messages (the errors the window shows) in the window's language.
    prism_base::i18n::set(prism_base::i18n::resolve(&cfg.appearance.language));
    hub.update(|s| {
        s.conventions = conventions;
        s.alerts_settings = cfg.alerts.clone();
        s.open_with = open_with;
        s.console = cfg.console.clone();
        s.palette = cfg.palette.clone();
        s.files = cfg.files.clone();
        s.notes = cfg.notes.clone();
        s.dictation = cfg.dictation.clone();
        s.appearance = cfg.appearance.clone();
        s.views = cfg.views.clone();
        s.templates = cfg.templates.clone();
        if !cfg.profile.name.trim().is_empty() {
            s.profile.name = cfg.profile.name.trim().to_string();
        }
        s.profile.color = cfg.profile.color.trim().to_string();
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Keys {
        crate::blocks::register();
        let mut cfg = config::parsed(text);
        cfg.table = text.parse().unwrap();
        reload_keys(&cfg)
    }

    /// The parts a change from `base` to `text` starts again; `None`: all of them (common).
    fn restarts(base: &str, text: &str) -> Option<Vec<Part>> {
        let (old, new) = (key(base), key(text));
        if old.common != new.common {
            return None;
        }
        Some(new.parts.iter().filter(|(p, k)| old.parts.get(p) != Some(k)).map(|(p, _)| *p).collect())
    }

    const BASE: &str = "[forge]\nenabled = true\n";

    #[test]
    fn what_applies_live_does_not_start_the_collectors_again() {
        let none = Some(Vec::new());
        assert_eq!(restarts(BASE, "[forge]\nenabled = true\n[appearance]\ntheme = \"paper\"\n[alerts]\nyour_turn_minutes = 3\n[logs]\nlevel = \"debug\"\n"), none);
        assert_eq!(restarts(BASE, "[forge]\nenabled = true\n[agents]\ncost_alert = 12.0\n"), none);
    }

    #[test]
    fn what_a_part_reads_starts_it_again_alone() {
        assert_eq!(restarts(BASE, "[forge]\nenabled = true\n[mail]\nenabled = true\n"), Some(vec![Part::Mail]));
        // Claude Desktop's sessions, though in `[agents]`, are read by their collector.
        assert_eq!(restarts(BASE, "[forge]\nenabled = true\n[agents.claude]\nsessions_poll = 99\n"), Some(vec![Part::Sessions]));
        // A block that reloads: its settings count, for it alone.
        assert_eq!(restarts(BASE, "[forge]\nenabled = true\n[docker]\nenabled = true\n"), Some(vec![Part::Block("docker")]));
        assert_eq!(restarts(BASE, "[forge]\nenabled = true\n[clickup]\nenabled = false\n"), Some(vec![Part::Tracker]));
        // The forge: its collectors, and the blocks told of it.
        let forge = restarts(BASE, "[forge]\nenabled = false\n").unwrap();
        assert!(forge.contains(&Part::Code) && forge.contains(&Part::Block("docker")) && !forge.contains(&Part::Mail), "{forge:?}");
    }

    #[test]
    fn what_they_all_read_starts_them_all_again() {
        assert_eq!(restarts(BASE, "[forge]\nenabled = true\n[[repos]]\nname = \"shop\"\n"), None);
    }

    #[test]
    fn a_part_forgets_what_it_brought_only() {
        let mut s = state::Snapshot { merge_requests: vec![Default::default()], mail: Some(Default::default()), ..Default::default() };
        s.blocks.insert("docker".into(), serde_json::json!({}));
        s.errors.insert("gitlab", "boom".into());
        s.errors.insert("mail", "down".into());
        Part::Mail.forget(&mut s);
        assert!(s.mail.is_none() && s.merge_requests.len() == 1 && s.blocks.contains_key("docker"));
        assert_eq!(s.errors.keys().copied().collect::<Vec<_>>(), ["gitlab"]);
        Part::Block("docker").forget(&mut s);
        assert!(s.blocks.is_empty());
    }

    #[test]
    fn a_reload_forgets_what_was_collected_but_not_what_outlives_it() {
        let mut s = state::Snapshot { merge_requests: vec![Default::default()], ..Default::default() };
        s.shared_revs.insert("notes".into(), 1);
        s.errors.insert("gitlab", "boom".into());
        s.errors.insert("usage", "rate limited".into());
        forget_collected(&mut s);
        assert!(s.merge_requests.is_empty());
        assert_eq!(s.shared_revs.len(), 1);
        assert_eq!(s.errors.keys().copied().collect::<Vec<_>>(), ["usage"]);
    }
}
