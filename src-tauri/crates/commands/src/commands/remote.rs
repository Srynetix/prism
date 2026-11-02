//! The class of each command of the window called from a phone (`prism_core::remote`), as
//! `table.rs` gives it with the command itself: a new command has no place without one.
//!
//! Also the desk's commands of Settings › Remote › Devices (pairing, revoking): never from a phone.

use prism_core::remote::Class::{self, Never};
use super::State;

use super::Cmd;
use crate::app::AppState;

/// Stops the remote's server and starts it again as `[remote]` says: Tailscale was off when it
/// started, or went down since. The phones' sessions end; nothing to do if it is off.
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn remote_restart(state: State<'_, AppState>) -> Cmd {
    let cfg = crate::config::load(&crate::config_path())?;
    if cfg.remote.enabled {
        state.remote.restart(&cfg.remote).await;
    }
    Ok(())
}

/// Settings › Remote › Away: what keeps the Mac within reach of the phone for days, or not.
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn remote_away_checks() -> Cmd<Vec<prism_core::state::AwayCheck>> {
    let cfg = crate::config::load(&crate::config_path())?;
    Ok(prism_remote::away::checks(&cfg.remote).await)
}

/// Settings › Remote › Devices: opens a pairing, its QR code in the snapshot.
#[cfg_attr(feature = "tauri", tauri::command)]
pub fn remote_pair_start(state: State<'_, AppState>) -> Cmd {
    state.remote.pair_start().map_err(Into::into)
}

/// The desk confirms the code the phone shows: the phone is paired.
#[cfg_attr(feature = "tauri", tauri::command)]
pub fn remote_pair_confirm(state: State<'_, AppState>) -> Cmd {
    state.remote.pair_confirm(None).map_err(Into::into)
}

#[cfg_attr(feature = "tauri", tauri::command)]
pub fn remote_pair_cancel(state: State<'_, AppState>) {
    state.remote.pair_cancel();
}

/// The desk takes an agent's terminal back from the phone typing in it.
#[cfg_attr(feature = "tauri", tauri::command)]
pub fn remote_release(state: State<'_, AppState>, agent: String) {
    state.remote.release(&agent);
}

/// Forgets a phone: its passkey opens nothing anymore, its sessions end.
#[cfg_attr(feature = "tauri", tauri::command)]
pub fn remote_revoke(state: State<'_, AppState>, id: String) -> Cmd {
    state.remote.revoke(&id).map_err(Into::into)
}

/// Renames a phone: its passkey and sessions stay.
#[cfg_attr(feature = "tauri", tauri::command)]
pub fn remote_rename(state: State<'_, AppState>, id: String, name: String) -> Cmd {
    state.remote.rename(&id, &name).map_err(Into::into)
}

/// A command's class; `None` for a command this Prism does not have. `block_call` is the
/// block's business: `block_class`.
pub fn class(command: &str) -> Option<Class> {
    super::COMMANDS.iter().find(|(name, _, _)| *name == command).map(|&(_, class, _)| class)
}

/// The class of a block's command; `Never` for a block this Prism does not have.
pub fn block_class(block: &str, command: &str, args: &serde_json::Value) -> Class {
    crate::blocks::find(block).map_or(Never, |b| b.remote(command, args))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_command_is_listed_once() {
        let names: BTreeSet<&str> = super::super::COMMANDS.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(names.len(), super::super::COMMANDS.len(), "a command is listed twice (table.rs)");
        assert!(names.len() > 100, "{} commands in table.rs", names.len());
    }

    #[test]
    fn the_demo_refuses_what_acts() {
        use super::super::{Demo, COMMANDS};
        let demo = |name: &str| COMMANDS.iter().find(|(n, _, _)| *n == name).map(|&(_, _, d)| d);
        for name in ["merge_mr", "agent_spawn", "set_ticket_status", "open_repo", "check_renovate", "google_sign_in"] {
            assert_eq!(demo(name), Some(Demo::Refused), "{name}");
        }
        for name in ["get_snapshot", "mr_detail", "save_config", "agent_write", "restart_app", "block_call", "shared_save"] {
            assert_eq!(demo(name), Some(Demo::Allowed), "{name}");
        }
    }

    #[test]
    fn what_touches_the_config_or_the_mac_is_never_routed() {
        for name in ["save_config", "store_token", "update_install", "switch_profile", "open_repo", "paste_text"] {
            assert_eq!(class(name), Some(Never), "{name}");
        }
        assert_eq!(class("merge_mr"), Some(Class::Critical));
        assert_eq!(class("agent_write"), Some(Class::Critical));
        assert_eq!(class("no_such_command"), None);
    }

    /// A command with a phone route is one a phone may call: never `Never` (`block_call` has its
    /// block's command's class, `block_class`).
    #[test]
    fn what_a_phone_reaches_is_classed_for_it() {
        for name in crate::commands::PHONE_ROUTED.iter().filter(|n| **n != "block_call") {
            assert!(class(name).is_some_and(|c| c != Never), "{name}: routed for a phone, classed {:?}", class(name));
        }
    }
}
