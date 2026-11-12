//! The window and the machine: fonts, clipboard, logs, folders, the store, audio, the tray count.


use prism_base::{i18n::tr, tr};
use super::State;

use crate::state::Snapshot;

use crate::app::AppState;

use super::Cmd;

/// The faces of an installed font family the webview cannot see (macOS's WebKit hides the
/// user's fonts from web content): the page loads them itself, with `font_face`.
#[prism_macros::command]
pub async fn font_faces(family: String) -> Vec<crate::fonts::Face> {
    tokio::task::spawn_blocking(move || crate::fonts::faces(&family)).await.unwrap_or_default()
}

/// Writes a table exported by the window (the Actuals of My sprint) where the save dialog said:
/// a `.csv` file only, never anything else.
#[prism_macros::command]
pub async fn save_csv(dest: String, text: String) -> Cmd {
    let path = std::path::PathBuf::from(dest);
    if !path.is_absolute() || path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("csv")) {
        return Err(anyhow::anyhow!("{}", tr("only a .csv file is written")).into());
    }
    // Never through a link to another file.
    if tokio::fs::symlink_metadata(&path).await.is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(anyhow::anyhow!("{}", tr("only a .csv file is written")).into());
    }
    tokio::fs::write(&path, text).await.map_err(|e| anyhow::anyhow!("{}: {e}", path.display()).into())
}

#[prism_macros::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    tracing::debug!("front: initial snapshot requested");
    state.hub.subscribe().borrow().clone()
}

/// The whole snapshot as the window last got it, with its revision: what its changes
/// (`snapshot-changes`) apply to.
#[prism_macros::command]
pub fn snapshot_sync(state: State<'_, AppState>) -> serde_json::Value {
    crate::app::snapshot_sync(&state.hub)
}

/// Relaunches Prism: settings that do not apply live (collectors) are reread.
#[prism_macros::command]
pub fn restart_app(state: State<'_, AppState>) {
    state.shell.restart()
}

/// Restarts or shuts down the computer (`restart`, `shutdown`): from a phone only (More › The
/// computer), with `[remote] power` and its passkey (the remote's server). With FileVault, a restart
/// takes the user's `password` (typed on the phone, never kept): the Mac comes back by itself,
/// without waiting for it at startup.
#[prism_macros::command]
pub async fn mac_power(action: String, password: Option<String>) -> Cmd {
    tracing::warn!("power: {action}, asked from a phone");
    if action != "restart" || crate::platform::restart_unlock().await != crate::platform::Unlock::Password {
        return crate::platform::power(&action).await.map_err(Into::into);
    }
    let password = password.filter(|p| !p.is_empty()).ok_or(tr("FileVault is on: the Mac's password is needed for it to come back by itself after the restart"))?;
    crate::platform::arm_unlock(&password, false).await?;
    tracing::warn!("power: the next restart skips FileVault's unlock");
    // The unlock armed must not outlive a restart that does not come: refused (Automation), the
    // authenticated restart is done at once instead.
    if let Err(e) = crate::platform::power(&action).await {
        tracing::warn!("power: {e:#}; restarting at once");
        crate::platform::arm_unlock(&password, true).await?;
        return Ok(());
    }
    // Cancelled by an app with unsaved work: said where it shows (Problems), on the computer and the phone.
    prism_base::rt::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        tracing::error!("power: the Mac did not restart (an application refused?) and FileVault's unlock is armed until it does: restart it");
    });
    Ok(())
}

/// macOS asks to reduce transparency (Accessibility › Display), which WebKit does not tell the
/// window: a theme's glass and overlays then stay off. On Linux, WebKitGTK without compositing
/// (`WEBKIT_DISABLE_COMPOSITING_MODE`, the usual way around Nvidia and Wayland glitches) draws no
/// `backdrop-filter`: the glass would be a see-through panel, so opaque panels instead.
#[prism_macros::command]
pub fn reduce_transparency() -> bool {
    #[cfg(target_os = "macos")]
    {
        prism_macos::appkit::reduce_transparency()
    }
    #[cfg(target_os = "linux")]
    {
        set_for_webkit(std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").as_deref())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    false
}

/// WebKitGTK reads its `WEBKIT_DISABLE_*` variables as set when there and not `0` (an empty one included).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub fn set_for_webkit(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| v != "0")
}

/// The log file's path, and its size in bytes (absent: none yet).
#[prism_macros::command]
pub fn logs_info() -> (String, Option<u64>) {
    let path = crate::logs::path();
    let size = std::fs::metadata(&path).ok().map(|m| m.len());
    (path.display().to_string(), size)
}

/// The tools Prism runs and their versions (Settings › General › Dependencies); `fresh`: asked
/// again rather than the answer of the last minutes.
#[prism_macros::command]
pub async fn dependencies(fresh: bool) -> Cmd<Vec<crate::deps::Dependency>> {
    if crate::demo::active() {
        return Ok(crate::deps::sample());
    }
    Ok(crate::deps::list(&crate::config::load(&crate::config_path())?, fresh).await)
}

/// `prism` on the PATH (Settings › General › Command line): where it stands.
#[prism_macros::command]
pub fn cli_command() -> Cmd<crate::cli::command::Status> {
    crate::cli::command::status().map_err(Into::into)
}

/// Installs the `prism` command (a script running this Prism's binary).
#[prism_macros::command]
pub fn cli_command_install() -> Cmd<crate::cli::command::Status> {
    if crate::demo::active() {
        return Err(anyhow::anyhow!("not in the demo").into());
    }
    crate::cli::command::install().map_err(Into::into)
}

/// Removes the `prism` command Install wrote.
#[prism_macros::command]
pub fn cli_command_remove() -> Cmd<crate::cli::command::Status> {
    if crate::demo::active() {
        return Err(anyhow::anyhow!("not in the demo").into());
    }
    crate::cli::command::remove().map_err(Into::into)
}

/// The last `lines` lines of the log (300 at most when absent), to copy into a report.
#[prism_macros::command]
pub fn logs_tail(lines: Option<usize>) -> Cmd<String> {
    crate::logs::tail(lines.unwrap_or(300).clamp(1, 5000)).map_err(Into::into)
}

/// Forgets Prism's recent problems (the status bar's list): they come back as they happen again.
#[prism_macros::command]
pub fn problems_clear() {
    prism_core::problems::clear();
}

/// The log file in the file manager (`finder`), or opened with its default app (`app`).
#[prism_macros::command]
pub async fn open_logs(with: String) -> Cmd {
    let path = crate::logs::path();
    let opened = match with.as_str() {
        "finder" => crate::platform::reveal(&path),
        "app" => crate::platform::open_with(&path, None).await,
        other => return Err(format!("unknown target {other}").into()),
    };
    opened.map_err(Into::into)
}

/// Opens a repository from the Repos menu in the editor, the terminal (applications from the config) or the
/// Finder. Only the folders of followed repositories are accepted.
#[prism_macros::command]
pub async fn open_repo(state: State<'_, AppState>, path: String, with: String) -> Cmd {
    let (known, apps) = {
        let snap = state.hub.subscribe();
        let snap = snap.borrow();
        let tracked = snap.repos.iter().any(|r| r.path == path) || snap.worktrees.iter().any(|w| w.path == path && !w.prunable);
        (tracked, snap.open_with.clone())
    };
    if !known {
        return Err(tr!("{path} is neither a tracked repository nor a worktree", path = path).into());
    }
    open_folder(&path, &with, apps).await
}

/// Opens a folder in the `[open_with]` editor or terminal, or the file manager.
pub(super) async fn open_folder(path: &str, with: &str, apps: crate::state::OpenWith) -> Cmd {
    let path = std::path::Path::new(path);
    let opened = match with {
        "editor" => crate::platform::open_with(path, Some(&apps.editor.ok_or(tr("no editor in [open_with]"))?)).await,
        "terminal" => {
            let terminal = apps.terminal.unwrap_or_else(|| crate::platform::DEFAULT_TERMINAL.into());
            crate::platform::open_terminal_in(path, &terminal).await
        }
        "finder" => crate::platform::open_with(path, None).await,
        other => return Err(format!("unknown target {other}").into()),
    };
    opened.map_err(Into::into)
}

/// A call to a block with a source of its own (`blocks`): its command, its arguments, its answer.
#[prism_macros::command]
pub async fn block_call(block: String, command: String, args: Option<serde_json::Value>) -> Cmd<serde_json::Value> {
    let b = crate::blocks::find(&block).ok_or_else(|| format!("no block {block} in this Prism"))?;
    // The demo profile reads nothing: a block answers from its sample data, or refuses.
    if crate::demo::active() {
        return b.demo_call(&command, &args.unwrap_or_default()).unwrap_or_else(|| Err(anyhow::anyhow!(crate::demo::READ_ONLY))).map_err(Into::into);
    }
    b.call(command, args.unwrap_or_default()).await.map_err(Into::into)
}

#[prism_macros::command]
pub fn store_load(state: State<'_, AppState>, name: String) -> Cmd<Option<serde_json::Value>> {
    tracing::debug!("front: load store {name}");
    state.store.load(&name).map_err(Into::into)
}

#[prism_macros::command]
pub fn store_save(state: State<'_, AppState>, name: String, value: serde_json::Value) -> Cmd {
    state.store.save(&name, &value).map_err(Into::into)
}

/// The stores the window and the phones share: the notes, the retro points, the pet, the blocks'
/// filters and sorts, the agents' chains (written by the window alone, which runs them). Whoever
/// writes, the others read again (`shared_revs`).
const SHARED: [&str; 4] = ["notes", "retro", "pet", "prefs"];

fn shared_name(name: &str) -> Result<(), super::CmdError> {
    if SHARED.contains(&name) { Ok(()) } else { Err(format!("{name} is not a shared store").into()) }
}

/// A shared store, as written last by the window or a phone.
#[prism_macros::command]
pub fn shared_load(state: State<'_, AppState>, name: String) -> Cmd<Option<serde_json::Value>> {
    shared_name(&name)?;
    state.store.load(&name).map_err(Into::into)
}

/// Writes a shared store and bumps its revision, so that the other pages read it again; returns
/// the new revision, which the writer then knows as its own.
#[prism_macros::command]
pub fn shared_save(state: State<'_, AppState>, name: String, value: serde_json::Value) -> Cmd<u64> {
    shared_name(&name)?;
    if (name == "notes" && !value.is_array()) || (name != "notes" && !value.is_object()) {
        return Err(format!("{name}: unexpected shape").into());
    }
    // The alerts follow the agents and worktrees archived, the overlaps ignored (in the demo too).
    if name == "prefs" {
        state.hub.set_alert_prefs(&value);
    }
    // The demo writes nothing: the page keeps what it changed until it is closed.
    if crate::demo::active() {
        return Ok(state.hub.subscribe().borrow().shared_revs.get(&name).copied().unwrap_or(0));
    }
    // The points written without the sprints kept (`crate::retro::keep`): those stay, under the
    // store's lock that `keep` takes too.
    state.store.update(&name, |stored| Ok(Some(if name == "retro" { crate::blocks::trail::retro::with_history(stored.as_ref(), value) } else { value })))?;
    Ok(shared_written(&state.hub, &name))
}

/// The alerts of the blocks of another repository (`BlockDef.alerts`, TypeScript only), as the
/// window computes them: the routines (`{alerts}`) and the Changes block read them with the core's.
/// Sent by the window only when it has such blocks; without the window they are absent.
#[prism_macros::command]
pub fn alerts_external(state: State<'_, AppState>, alerts: Vec<prism_core::alerts::Alert>) {
    state.hub.set_external_alerts(alerts);
}

/// A shared store just written: its revision bumped, so that the other pages read it again; the new one.
pub(crate) fn shared_written(hub: &crate::state::Hub, name: &str) -> u64 {
    // In milliseconds, so that it keeps growing across restarts: a phone's page outlives them.
    let now = u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0);
    let mut rev = 0;
    hub.update(|s| {
        let r = s.shared_revs.entry(name.to_string()).or_default();
        *r = now.max(*r + 1);
        rev = *r;
        true
    });
    rev
}

#[cfg(test)]
mod tests {
    use super::set_for_webkit;
    use std::ffi::OsStr;

    #[test]
    fn a_webkit_variable_is_set_when_there_and_not_zero() {
        assert!(!set_for_webkit(None));
        assert!(!set_for_webkit(Some(OsStr::new("0"))));
        assert!(set_for_webkit(Some(OsStr::new("1"))));
        assert!(set_for_webkit(Some(OsStr::new(""))));
    }
}
