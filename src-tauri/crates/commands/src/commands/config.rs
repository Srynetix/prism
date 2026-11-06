//! The config: read, preview, save and apply live; packs, updates, setting checks, clones, tokens.


use prism_base::{i18n::tr, tr};
use super::State;


use crate::app::AppState;

use super::Cmd;

/// The settings as a phone reads them (`[remote] settings`): the values, the packs' and the
/// defaults, and which keys it may change; neither the file's text nor the packs' (the tree shows
/// them). No secret is in there: tokens live in the keychain.
#[derive(serde::Serialize)]
pub struct PhoneSettings {
    values: serde_json::Value,
    effective: serde_json::Value,
    defaults: serde_json::Value,
    inherited: serde_json::Value,
    error: Option<String>,
    /// `alerts.quiet_hours`: the keys of `PHONE_SETTINGS`.
    editable: Vec<String>,
}

impl From<crate::settings::FileView> for PhoneSettings {
    fn from(v: crate::settings::FileView) -> Self {
        let editable = prism_core::config::PHONE_SETTINGS.iter().map(|(t, k)| format!("{t}.{k}")).collect();
        Self { values: v.values, effective: v.effective, defaults: v.defaults, inherited: v.inherited, error: v.error, editable }
    }
}

/// More › Settings on a phone: read (the server lets it through with `[remote] settings` only).
#[prism_macros::command]
pub fn remote_settings_read() -> Cmd<PhoneSettings> {
    Ok(crate::settings::read(&crate::config_path())?.into())
}

/// A phone's changes, once its passkey confirmed them: refused whole if one is not a key of
/// `PHONE_SETTINGS` set to a plain value, whatever the phone's page showed.
#[prism_macros::command(async)]
pub fn remote_settings_save(state: State<'_, AppState>, edits: Vec<crate::settings::Edit>) -> Cmd<PhoneSettings> {
    if let Some(e) = edits.iter().find(|e| !prism_core::config::phone_may_set(&e.path, e.value.as_ref())) {
        return Err(tr!("{key} is not changed from a phone", key = e.path.join(".")).into());
    }
    let path = crate::config_path();
    crate::settings::save(&path, &edits)?;
    apply_live(&state, &path);
    Ok(crate::settings::read(&path)?.into())
}

/// Settings view: `config.toml` as it is on disk.
#[prism_macros::command]
pub fn read_config() -> Cmd<crate::settings::FileView> {
    crate::settings::read(&crate::config_path()).map_err(Into::into)
}

/// The text these changes would produce, without writing anything.
#[prism_macros::command]
pub fn preview_config(edits: Vec<crate::settings::Edit>) -> Cmd<String> {
    crate::settings::preview(&crate::config_path(), &edits).map_err(Into::into)
}

/// `text` (what saving would write) merged on top of its packs, for the TOML panel of Settings.
#[prism_macros::command]
pub fn merged_config(text: String) -> Cmd<String> {
    crate::settings::merged(&crate::config_path(), &text).map_err(Into::into)
}

/// Saves the changes, then applies right away what can be (console, appearance, agents);
/// the rest waits for a Prism restart.
#[prism_macros::command(async)]
pub fn save_config(state: State<'_, AppState>, edits: Vec<crate::settings::Edit>) -> Cmd<crate::settings::FileView> {
    let path = crate::config_path();
    let before = crate::config::load(&path).ok();
    crate::settings::save(&path, &edits)?;
    // The routines this save changed (or their quick actions) trusted as saved, before they
    // are read: changed anywhere else, they wait for trust.
    if let Ok(after) = crate::config::load(&path) {
        state.agents.routines_saved(before.as_ref().map(|c| &c.agents), &after.agents);
    }
    apply_live(&state, &path);
    crate::settings::read(&path).map_err(Into::into)
}

/// Settings › General › Packs: imports a pack from a file, `glab:group/project:path@branch`, `gh:owner/repo:path@branch` or `prism:starter`, and adds it
/// to `config.toml`. What applies live does; the rest waits for a restart, as for a save.
#[prism_macros::command]
pub async fn pack_import(state: State<'_, AppState>, source: String) -> Cmd<crate::settings::FileView> {
    let path = crate::config_path();
    crate::packs::import(&path, &source).await?;
    apply_live(&state, &path);
    crate::settings::read(&path).map_err(Into::into)
}

/// Reimports a pack from where it came from.
#[prism_macros::command]
pub async fn pack_update(state: State<'_, AppState>, name: String, reference: Option<String>) -> Cmd<crate::settings::FileView> {
    let path = crate::config_path();
    crate::packs::update(&path, &name, reference.as_deref()).await?;
    apply_live(&state, &path);
    crate::settings::read(&path).map_err(Into::into)
}

/// Prism's newest release in `[updates] source`, and whether it is newer (Settings › General ›
/// Updates, the status bar). The config is read anew: a pack updated since launch counts. `auto`:
/// the periodic check, which reads nothing with `[updates] enabled` off.
#[prism_macros::command]
pub async fn update_check(auto: bool) -> Cmd<crate::updates::UpdateInfo> {
    if crate::demo::active() {
        return Ok(crate::updates::UpdateInfo { current: crate::packs::PRISM.into(), ..Default::default() });
    }
    let cfg = crate::config::load(&crate::config_path())?;
    if auto && !cfg.updates.enabled {
        return Ok(crate::updates::UpdateInfo { current: crate::packs::PRISM.into(), source: cfg.updates.source.trim().into(), ..Default::default() });
    }
    crate::updates::check(&cfg.updates).await.map_err(Into::into)
}

/// The packs read again from their sources (the status bar, Settings › Packs). `auto`: the periodic
/// check, which reads nothing with `[updates] enabled` or `packs` off.
#[prism_macros::command]
pub async fn pack_check(auto: bool) -> Cmd<Vec<crate::packs::PackCheck>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    let path = crate::config_path();
    let cfg = crate::config::load(&path)?;
    if auto && !(cfg.updates.enabled && cfg.updates.packs) {
        return Ok(Vec::new());
    }
    crate::packs::check(&path).await.map_err(Into::into)
}

/// Installs the release `tag`: whether Prism must restart (`restart_app`), and what was done.
#[prism_macros::command]
pub async fn update_install(tag: String) -> Cmd<(bool, String)> {
    let cfg = crate::config::load(&crate::config_path())?;
    crate::updates::install(&cfg.updates, &tag).await.map_err(Into::into)
}

/// Takes a pack out of `config.toml`.
#[prism_macros::command(async)]
pub fn pack_remove(state: State<'_, AppState>, name: String) -> Cmd<crate::settings::FileView> {
    let path = crate::config_path();
    crate::packs::remove(&path, &name)?;
    apply_live(&state, &path);
    crate::settings::read(&path).map_err(Into::into)
}

/// Applies a change of the config: what the window shows of it and the agents' settings at once,
/// and the collectors, stopped and started again from it when what they read of it changed. A
/// config that does not read says so in Alerts, the collectors running on as they were.
pub(crate) fn apply_live(state: &AppState, path: &std::path::Path) {
    apply(state, path, false);
}

fn apply(state: &AppState, path: &std::path::Path, force: bool) {
    let cfg = match crate::config::load(path) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!("config: {e:#}");
            state.hub.set_error("config", Some(format!("{e:#}")));
            return;
        }
    };
    state.hub.set_error("config", None);
    crate::logs::apply(path);
    prism_base::git::trust(cfg.repositories().into_iter().filter_map(|r| r.path));
    // The remote starts, restarts or stops as `[remote]` now says.
    let (remote, remote_cfg) = (state.remote.clone(), cfg.remote.clone());
    crate::app::keep_running(&remote_cfg);
    prism_base::rt::spawn(async move { remote.apply(&remote_cfg).await });
    let conventions = crate::bootstrap::conventions_of(&cfg);
    crate::conventions::set(conventions.clone());
    // The tickets' custom fields: from ClickUp's next read.
    crate::collectors::clickup::fields::configure(&cfg.clickup.fields, cfg.clickup.filter_fields.as_deref(), &cfg.clickup.estimate_fields);
    crate::bootstrap::show_settings(&state.hub, &cfg, conventions);
    state.hub.update(|s| {
        let changed = s.worktree_stale_days != cfg.worktrees.stale_days;
        s.worktree_stale_days = cfg.worktrees.stale_days;
        changed
    });
    state.live.write().expect("live").audio = cfg.audio.clone();
    state.agents.configure(cfg.agents.clone());
    crate::app::routine_context(&state.agents, &cfg, path);
    if let Some(started) = state.collectors.reload(&cfg, force) {
        state.adopt(started);
    }
    // The agents' proposals made on the former file say so.
    crate::mcp::host::refresh();
}

/// Reload collectors (console, palette): they stop and start again from the config, even
/// unchanged (one that panicked, a source that went wrong).
#[prism_macros::command(async)]
pub fn reload_collectors(state: State<'_, AppState>) {
    apply(&state, &crate::config_path(), true);
}

/// Watches `config.toml` and its packs: a change made by hand, by an agent or by another Prism
/// applies as a save from Settings does, half a second after the last write.
pub fn watch(state: std::sync::Arc<AppState>) {
    use notify::{RecursiveMode, Watcher};
    let path = crate::config_path();
    let packs = crate::packs::dir(&path);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let (file, folder) = (path.clone(), packs.clone());
    let handler = move |event: notify::Result<notify::Event>| {
        if let Ok(e) = event
            && !e.kind.is_access()
            && e.paths.iter().any(|p| *p == file || (p.starts_with(&folder) && p.extension().is_some_and(|x| x == "toml")))
        {
            let _ = tx.send(());
        }
    };
    let mut watcher = match notify::recommended_watcher(handler) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("config: no file watch, changes made outside Settings apply on Reload collectors: {e}");
            return;
        }
    };
    // The folder, not the file: editors replace it rather than write it.
    for (dir, mode) in [(path.parent(), RecursiveMode::NonRecursive), (Some(packs.as_path()), RecursiveMode::Recursive)] {
        if let Some(dir) = dir.filter(|d| d.is_dir())
            && let Err(e) = watcher.watch(dir, mode)
        {
            tracing::warn!("config: cannot watch {}: {e}", dir.display());
        }
    }
    prism_base::rt::spawn(async move {
        let _watcher = watcher;
        while rx.recv().await.is_some() {
            // Several events per save: the last one counts. A save from Settings applied already:
            // applied again, it changes nothing.
            while tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await.is_ok_and(|e| e.is_some()) {}
            tracing::debug!("config: changed on disk");
            apply_live(&state, &path);
        }
    });
}
/// Settings: the installed applications, for `[open_with]`.
#[prism_macros::command]
pub fn installed_apps() -> Vec<String> {
    crate::platform::installed_apps()
}

/// Settings: is a config.toml value usable (project, context, folder…)?
#[prism_macros::command]
pub async fn check_setting(check: crate::checks::Check) -> crate::checks::Outcome {
    // The demo's values point nowhere: nothing is asked of a forge, a cluster or a tracker.
    if crate::demo::active() {
        return crate::checks::Outcome { ok: true, message: tr("the demo checks nothing").into() };
    }
    crate::checks::run(check).await
}

/// The pages wizard: what this machine already has (forges' logins, agent CLIs, repositories of the
/// usual folders, kubectl contexts, trackers), each step pre-filled from it. Asked by the wizard
/// only: nothing is searched unasked.
#[prism_macros::command]
pub async fn detect_setup() -> crate::detect::Detection {
    if crate::demo::active() {
        return crate::demo::details::detection();
    }
    let cfg = crate::config::load(&crate::config_path()).ok();
    let workspaces: Vec<_> = cfg.as_ref().map(|c| c.workspaces.iter().map(|w| w.dir.clone()).collect()).unwrap_or_default();
    let host = cfg.as_ref().map_or_else(|| "gitlab.com".to_string(), |c| c.gitlab.host.clone());
    crate::detect::detect(&workspaces, &host).await
}

/// Settings › Repositories: the clones of `projects` in a folder the user picked (and only there).
#[prism_macros::command]
pub async fn find_clones(root: String, projects: Vec<String>) -> Cmd<Vec<crate::clones::Found>> {
    let root = crate::config::expand_home(std::path::Path::new(root.trim()));
    tokio::task::spawn_blocking(move || crate::clones::find(&root, &projects)).await?.map_err(Into::into)
}

/// Settings › Repositories: clones a project into a folder the user picked; its path, `~/…`.
#[prism_macros::command]
pub async fn clone_repo(project: String, parent: String, forge: Option<crate::forge::Provider>) -> Cmd<String> {
    crate::clones::clone(&project, &crate::config::expand_home(std::path::Path::new(parent.trim())), forge).await.map_err(Into::into)
}

/// Settings › Repositories › Add: what a picked folder is (a clone, its project).
#[prism_macros::command]
pub fn inspect_folder(path: String) -> Cmd<crate::clones::Folder> {
    crate::clones::inspect(&crate::config::expand_home(std::path::Path::new(path.trim()))).map_err(Into::into)
}

/// Settings › Repositories › Add: `git init` in a picked folder that is not a repository.
#[prism_macros::command]
pub async fn init_repo(path: String) -> Cmd {
    crate::clones::init(&crate::config::expand_home(std::path::Path::new(path.trim()))).await.map_err(Into::into)
}

/// Settings › Repositories › New project: a folder with a git repository, and the project on the
/// forge with `remote`.
#[prism_macros::command]
pub async fn create_project(parent: String, name: String, remote: Option<crate::clones::NewRemote>) -> Cmd<crate::clones::Created> {
    crate::clones::create(&crate::config::expand_home(std::path::Path::new(parent.trim())), &name, remote).await.map_err(Into::into)
}

/// A working folder as a phone sees it: its name, its folder (`~/…` and absolute, to tell the
/// repositories in it), the forge group offered for its projects.
#[derive(serde::Serialize)]
pub struct WorkspaceView {
    name: String,
    dir: String,
    abs: String,
    namespace: String,
    /// Its forge, the profile's when it names none.
    forge: crate::forge::Provider,
}

/// More › New project on a phone (`[remote] projects`): the working folders it may create in.
#[prism_macros::command]
pub fn workspaces() -> Cmd<Vec<WorkspaceView>> {
    let cfg = crate::config::load(&crate::config_path())?;
    let profile = cfg.forge.provider;
    Ok(cfg.workspaces.into_iter().map(|w| WorkspaceView { dir: crate::clones::tilde(&w.dir), abs: w.dir.display().to_string(), name: w.name, namespace: w.namespace, forge: w.forge.unwrap_or(profile) }).collect())
}

/// More › New project on a phone, once its passkey confirmed it: `name` in the working folder
/// `workspace`, the only places a phone creates in, added to the repositories (read at the next
/// launch).
#[prism_macros::command]
pub async fn remote_project_create(workspace: String, name: String, remote: Option<crate::clones::NewRemote>) -> Cmd<crate::clones::Created> {
    not_public(remote.as_ref())?;
    crate::clones::create_in_workspace(&crate::config_path(), &workspace, &name, remote).await.map_err(Into::into)
}

/// A phone makes no project public: what it holds is out for anyone, at the desk only.
fn not_public(remote: Option<&crate::clones::NewRemote>) -> Cmd {
    match remote {
        Some(r) if r.visibility.trim() == "public" => Err(tr("from a phone, a project is private or internal: public at the desk").into()),
        _ => Ok(()),
    }
}

/// More › Clone a project on a phone (`[remote] projects`): the forge's projects of a working
/// folder, matching `query`.
#[prism_macros::command]
pub async fn remote_project_search(workspace: String, query: String) -> Cmd<Vec<crate::clones::ForgeProject>> {
    crate::clones::search_projects(&crate::config_path(), &workspace, &query).await.map_err(Into::into)
}

/// More › Clone a project on a phone, once its passkey confirmed it: `project` cloned into the
/// working folder `workspace`, added to the repositories (read at the next launch).
#[prism_macros::command]
pub async fn remote_project_clone(workspace: String, project: String) -> Cmd<crate::clones::Created> {
    crate::clones::clone_in_workspace(&crate::config_path(), &workspace, &project).await.map_err(Into::into)
}

/// Settings › Repositories › Publish: a local repository without a remote, its project created on
/// the forge and pushed. Its project; Settings writes it in the row.
#[prism_macros::command]
pub async fn publish_repo(path: String, remote: crate::clones::NewRemote) -> Cmd<String> {
    crate::clones::publish_local(&crate::config::expand_home(std::path::Path::new(path.trim())), &remote).await.map_err(Into::into)
}

/// More › New project › Publish on a phone, once its passkey confirmed it: the repository `name`
/// (local, without a project) on the forge, its project written in config.toml.
#[prism_macros::command]
pub async fn remote_project_publish(name: String, remote: crate::clones::NewRemote) -> Cmd<crate::clones::Created> {
    not_public(Some(&remote))?;
    crate::clones::publish_in_config(&crate::config_path(), &name, remote).await.map_err(Into::into)
}

/// `config.toml` in the file manager (`finder`) or the `[open_with]` editor (`editor`).
#[prism_macros::command]
pub async fn open_config(state: State<'_, AppState>, with: String) -> Cmd {
    let path = crate::config_path();
    let opened = match with.as_str() {
        "finder" => crate::platform::reveal(&path),
        "editor" => {
            let editor = state.hub.subscribe().borrow().open_with.editor.clone().ok_or(tr("no editor in [open_with]"))?;
            crate::platform::open_with(&path, Some(&editor)).await
        }
        other => return Err(format!("unknown target {other}").into()),
    };
    opened.map_err(Into::into)
}

/// The keychain entry of a tracker's token (`clickup`, `jira`), or of the Google OAuth client's
/// secret (`google`), for the active profile.
fn token_service(tracker: &str) -> Cmd<String> {
    let base = match tracker {
        "clickup" => crate::collectors::clickup::KEYCHAIN_SERVICE,
        "jira" => crate::collectors::jira::KEYCHAIN_SERVICE,
        "google" => crate::collectors::google::SECRET_SERVICE,
        other => return Err(format!("no token for {other}").into()),
    };
    Ok(crate::profile::service(base))
}

/// Whether a tracker's token is in the keychain: its entry, and its length (never the token).
#[prism_macros::command]
pub async fn token_state(tracker: String) -> Cmd<(String, Option<usize>)> {
    let service = token_service(&tracker)?;
    let found = crate::platform::secret(&service).await?;
    Ok((service, found.map(|t| t.chars().count())))
}

/// Stores a tracker’s token in the keychain; the collectors start again to read with it.
#[prism_macros::command]
pub async fn store_token(state: State<'_, AppState>, tracker: String, token: String) -> Cmd<usize> {
    let service = token_service(&tracker)?;
    let token = token.trim().to_string();
    let length = token.chars().count();
    tokio::task::spawn_blocking(move || crate::platform::store_secret(&service, &token)).await??;
    tracing::info!("{tracker}: token stored ({length} characters)");
    if tracker == "clickup" {
        crate::collectors::clickup::forget_token();
    }
    // Read with the new token at once: the collectors start again.
    apply(&state, &crate::config_path(), true);
    Ok(length)
}

/// A proposal of an agent (`prism mcp`) answered in the window: `apply` (two clicks), `refuse`
/// (`reason`, given back to the agent), `modified` (`text`, saved from the TOML panel). Applied, it
/// applies live as a save from Settings does.
#[prism_macros::command]
pub async fn mcp_proposal_answer(id: u64, action: String, reason: Option<String>, text: Option<String>) -> Cmd {
    crate::mcp::host::answer(id, &action, reason, text).await.map_err(Into::into)
}

/// Settings › Agents › Prism MCP: where each agent CLI outside Prism stands (its server, its skill).
#[prism_macros::command]
pub fn mcp_installs() -> Vec<crate::mcp::install::Installed> {
    crate::mcp::install::status()
}

/// Settings › Agents › Prism MCP: Prism's MCP server and skill added to an agent CLI's config
/// (`on`), or taken out (`prism mcp install|uninstall <cli>`).
#[prism_macros::command]
pub async fn mcp_install(cli: String, on: bool) -> Cmd<String> {
    let done = tokio::task::spawn_blocking(move || if on { crate::mcp::install::install(&cli) } else { crate::mcp::install::uninstall(&cli) }).await?;
    done.map_err(Into::into)
}
