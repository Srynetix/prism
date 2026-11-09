//! The state the commands share, started once: the collectors, the agents and their chains, the
//! remote. What only the app around it does (relaunch itself, serve the phone's page) comes
//! through `Shell`: the window's, or a server's without one.

use std::sync::Arc;

use crate::{
    collectors::time::TimeService,
    state::Hub,
    store::Store,
};

/// What only the app around the commands does.
pub trait Shell: Send + Sync + 'static {
    /// Relaunches Prism (Restart Prism, a phone's, a profile just set up): agents and stores
    /// outlive it.
    fn restart(&self) -> !;
    /// A file of the phone's page (`remote.html`, its scripts and styles, its icons): its bytes
    /// and type.
    fn asset(&self, path: &str) -> Option<(Vec<u8>, String)>;
    /// What the agents' chains and inbox tell (`Agents::notices`): the window's toast and the
    /// system's notification; without a window, the log.
    fn notice(&self, n: &crate::agents::Notice) {
        tracing::info!("{}", n.text);
    }
}

pub struct AppState {
    /// Prism from a phone (`[remote]`).
    pub remote: prism_remote::Remote,
    pub hub: Hub,
    pub time: TimeService,
    pub store: Store,
    pub agents: crate::agents::Agents,
    /// The collectors, started again when what they read of the config changes.
    pub collectors: crate::bootstrap::Collectors,
    /// What the commands reach of them, replaced along with them (`adopt`).
    pub(crate) live: std::sync::RwLock<Live>,
    pub shell: Arc<dyn Shell>,
    /// This build: its commit and its extra blocks' (feedback's context).
    pub build: String,
}

/// What the commands reach of the collectors running.
pub(crate) struct Live {
    pub forge: crate::forge::Forge,
    pub tracker: crate::collectors::tracker::Tracker,
    pub audio: crate::config::AudioConfig,
    pub pod_logs: crate::collectors::environments::PodLogs,
}

impl AppState {
    /// Starts Prism on the active profile's config, its data in `data_dir`: the collectors, the
    /// agents and their chains (not in the demo, which shows sample ones), the remote as
    /// `[remote]` says, and the watch of `config.toml`.
    pub fn start(data_dir: std::path::PathBuf, hub: Hub, time: TimeService, team_id: crate::collectors::clickup::TeamId, shell: Arc<dyn Shell>, build: String) -> anyhow::Result<Arc<Self>> {
        use prism_base::rt::spawn;
        let (collecting, started) = crate::bootstrap::Collectors::start(&hub, &time, team_id);
        // Read whatever the config: the plan usage of the CLIs agents run on. Not in the demo, which
        // shows a sample one and reads neither the keychain nor the CLIs' sessions.
        if !started.agents.demo {
            spawn(crate::collectors::usage::run(hub.clone()));
        }
        // Prism's warnings and errors, for the status bar.
        spawn(prism_core::problems::run(hub.clone()));
        // The other profiles open beside this one, and what this one tells them.
        spawn(crate::presence::run(hub.clone()));
        let agents = crate::agents::Agents::new(data_dir.clone(), hub.clone(), Vec::new(), Vec::new())?;
        agents.configure(started.agents.cfg.clone());
        // `builtin:retro-draft` reads the retro here, the window open or not.
        let (retro_hub, retro_store) = (hub.clone(), Store::new(data_dir.clone()));
        agents.set_retro_source(Arc::new(move || crate::retro::draft(&retro_hub, &retro_store)));
        // The demo profile shows sample agents: the real ones (tmux) would replace them.
        if !started.agents.demo {
            spawn(agents.clone().run());
            // Their chains and the prompts held for them: run here, whatever the window shows.
            spawn(agents.clone().run_chains());
            // The routines, at their times: here too, the window open or not.
            spawn(agents.clone().run_routines());
            // What they flag in passing: the Desktop sessions' calls, the files changed since.
            spawn(agents.clone().run_asides());
        }
        if let Ok(cfg) = crate::config::load(&crate::config_path()) {
            routine_context(&agents, &cfg, &crate::config_path());
        }
        // Prism from a phone: off until `[remote]` turns it on, here and on each save.
        let host = crate::remote::Host::default();
        let remote = prism_remote::Remote::new(&data_dir, hub.clone(), Arc::new(host.clone()))?;
        if let Some(cfg) = started.remote.clone() {
            let remote = remote.clone();
            keep_running(&cfg);
            spawn(async move { remote.apply(&cfg).await });
        }
        let live = Live { forge: started.forge.clone(), tracker: started.tracker.clone(), audio: started.audio.clone(), pod_logs: started.pod_logs.clone() };
        let state = Arc::new(Self { remote, hub, time, store: Store::new(data_dir), agents, collectors: collecting, live: std::sync::RwLock::new(live), shell, build });
        state.adopt(started);
        host.serve(&state);
        notices(&state);
        // `config.toml` and its packs changed by hand (or by an agent) apply as they are saved.
        crate::commands::config::watch(state.clone());
        // Prism's MCP server for agents (`prism mcp`) reaches this Prism on the profile's socket.
        crate::mcp::host::serve(state.clone());
        Ok(state)
    }

    pub fn forge(&self) -> crate::forge::Forge {
        self.live.read().expect("live").forge.clone()
    }

    pub fn tracker(&self) -> crate::collectors::tracker::Tracker {
        self.live.read().expect("live").tracker.clone()
    }

    pub fn audio(&self) -> crate::config::AudioConfig {
        self.live.read().expect("live").audio.clone()
    }

    pub fn pod_logs(&self) -> crate::collectors::environments::PodLogs {
        self.live.read().expect("live").pod_logs.clone()
    }

    /// The collectors as they started (at launch, or again after a change of the config): the
    /// commands reach them, the next agents launch in their repositories.
    pub fn adopt(&self, started: crate::bootstrap::Started) {
        let crate::bootstrap::Started { audio, pod_logs, agents, forge, tracker, .. } = started;
        let provider = forge.provider();
        self.hub.update(|s| {
            let changed = s.forge != provider;
            s.forge = provider;
            changed
        });
        self.agents.set_repos(agents.worktrees, agents.repos);
        *self.live.write().expect("live") = Live { forge, tracker, audio, pod_logs };
    }
}

/// What the chains and the inbox tell, as the app shows it (`Shell::notice`); a "Notify me" step
/// reaches the phones too, those without the page open (`[remote] push`).
fn notices(state: &Arc<AppState>) {
    let mut rx = state.agents.notices();
    let me = Arc::downgrade(state);
    prism_base::rt::spawn(async move {
        loop {
            let n = match rx.recv().await {
                Ok(n) => n,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            };
            let Some(state) = me.upgrade() else { return };
            state.shell.notice(&n);
            if n.system {
                state.remote.notify(&n.text).await;
            }
        }
    });
}

/// What the routines read of the config beyond `[agents]`: where it lives, the first working
/// folder (a builtin runs there), whether Prism is started again when closed.
pub fn routine_context(agents: &crate::agents::Agents, cfg: &crate::config::Config, path: &std::path::Path) {
    let workspace = cfg.workspaces.first().map(|w| w.dir.clone());
    agents.set_routine_context(path.parent().map(std::path::Path::to_path_buf), workspace, cfg.remote.enabled && cfg.remote.keep_running);
}

/// `[remote] keep_running` as the config now says (a LaunchAgent: `launchctl`, off the async
/// runtime); a failure goes to the log, the Problems indicator.
pub fn keep_running(cfg: &prism_core::config::RemoteConfig) {
    let on = cfg.enabled && cfg.keep_running;
    std::thread::spawn(move || {
        if let Err(e) = crate::profile::keep_running(on) {
            tracing::warn!("keep_running: {e:#}");
        }
    });
}

/// The snapshot last sent to the window, and its revision.
static SENT: std::sync::Mutex<Option<(u64, serde_json::Value)>> = std::sync::Mutex::new(None);

/// The snapshot `now` as its changes to the one sent last (`base`, the revision they apply to, and
/// `rev`, the one they make), given to `send`; nothing when it has not changed. A window that has
/// not got `base` asks the whole one again (`snapshot_sync`).
pub fn send_changes(now: serde_json::Value, send: impl FnOnce(serde_json::Value)) {
    // Held while sent: a `snapshot_sync` gets the revision before these changes or after.
    let mut sent = SENT.lock().unwrap();
    let (base, changes) = match sent.as_ref() {
        Some((rev, old)) => (*rev, prism_remote::snapshot::diff(old, &now)),
        None => (0, vec![prism_remote::snapshot::Change { path: Vec::new(), value: Some(now.clone()), delete: false }]),
    };
    if !changes.is_empty() {
        let rev = base + 1;
        send(serde_json::json!({ "base": base, "rev": rev, "changes": changes }));
        *sent = Some((rev, now));
    }
}

/// The whole snapshot as the window has it after revision `rev` (on loading, or a revision missed);
/// before any was sent, the hub's, which the next changes then start from.
pub fn snapshot_sync(hub: &Hub) -> serde_json::Value {
    let mut sent = SENT.lock().unwrap();
    let (rev, snapshot) = sent.get_or_insert_with(|| (0, serde_json::to_value(&*hub.subscribe().borrow()).unwrap_or_default()));
    serde_json::json!({ "rev": *rev, "snapshot": snapshot })
}
