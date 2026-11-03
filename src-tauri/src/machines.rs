//! Other computers' Prism (`[[machines]]`), each in a window of its own (`machine-<n>`): the same
//! page as the main window's, its calls sent to that machine (`prism_remote::machine`) rather than
//! to this Prism. A window reaches only its own machine: which one is told by its label, never by
//! what the page sends. The token stays here (the keychain), never in the page.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use prism_commands::commands::Cmd;
use prism_core::config::MachineConfig;
use prism_remote::machine::{Machine, Target, Tty};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

/// The machines open in a window, by the window's label, and their terminals.
#[derive(Default)]
pub struct Machines {
    open: Mutex<HashMap<String, Open>>,
    ttys: Mutex<HashMap<(String, String), Tty>>,
}

struct Open {
    name: String,
    /// One of its profiles reached through it; `None`, the one its remote answers at.
    profile: Option<String>,
    machine: Option<Arc<Machine>>,
}

const PREFIX: &str = "machine-";

/// A window's label for machine `name` (and one of its profiles): Tauri takes letters, digits,
/// `-` and `_` only.
fn label(name: &str, profile: Option<&str>) -> String {
    let safe = |s: &str| -> String { s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect() };
    match profile {
        Some(p) => format!("{PREFIX}{}--{}", safe(name), safe(p)),
        None => format!("{PREFIX}{}", safe(name)),
    }
}

fn config() -> anyhow::Result<prism_core::config::Config> {
    prism_core::config::load(&crate::config_path())
}

fn find(name: &str) -> anyhow::Result<MachineConfig> {
    config()?.machines.into_iter().find(|m| m.name.trim() == name).ok_or_else(|| anyhow::anyhow!("no machine {name} in [[machines]]"))
}

/// Its token: from the environment (`PRISM_MACHINE_TOKEN_BUILD_SRV` for `build-srv`) where no
/// keychain is unlocked, as the other tokens; otherwise the keychain's.
async fn token(m: &MachineConfig) -> anyhow::Result<Option<String>> {
    if let Some(t) = std::env::var(m.token_env()).ok().filter(|t| !t.trim().is_empty()) {
        return Ok(Some(t.trim().to_string()));
    }
    prism_base::platform::secret(&prism_core::profile::service(&m.token_service())).await
}

/// A machine as the profile menu and Settings show it.
#[derive(serde::Serialize)]
pub struct MachineView {
    name: String,
    url: String,
    ssh: String,
    /// Its token is in the keychain.
    token: bool,
    /// Its window is open.
    open: bool,
    /// Its profiles as it said last (`watch`), the hub first; none read yet, none.
    profiles: Vec<prism_core::state::MachineProfile>,
}

/// The machines of `[[machines]]`, with their profiles: this computer's view, whichever window asks
/// (a machine's has that machine's snapshot, not this one's).
#[tauri::command]
pub async fn machine_list(state: State<'_, Arc<Machines>>, app: State<'_, Arc<prism_commands::AppState>>) -> Cmd<Vec<MachineView>> {
    let presence = app.hub.subscribe().borrow().machines.clone();
    let mut out = Vec::new();
    for m in config()?.machines.into_iter().filter(|m| !m.name.trim().is_empty()) {
        let name = m.name.trim().to_string();
        let token = token(&m).await.ok().flatten().is_some();
        let open = state.open.lock().unwrap().contains_key(&label(&name, None));
        let profiles = presence.iter().find(|p| p.name == name).map(|p| p.profiles.clone()).unwrap_or_default();
        out.push(MachineView { name, url: m.url, ssh: m.ssh, token, open, profiles });
    }
    Ok(out)
}

/// Machine `name`'s token, given once by its Prism (`prism peer add`), kept in the keychain.
#[tauri::command]
pub async fn machine_token(name: String, token: String) -> Cmd {
    let m = find(&name)?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("no token".into());
    }
    Ok(keep(&m, token).await?)
}

async fn keep(m: &MachineConfig, token: String) -> anyhow::Result<()> {
    let service = prism_core::profile::service(&m.token_service());
    tokio::task::spawn_blocking(move || prism_base::platform::store_secret(&service, &token)).await?
}

/// This computer's name, as the machine is asked to accept it: the Mac's, or the host's.
async fn computer_name() -> String {
    #[cfg(target_os = "macos")]
    if let Ok(out) = prism_base::platform::command("scutil").args(["--get", "ComputerName"]).output().await
        && out.status.success()
    {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    let host = prism_base::platform::command("hostname").output().await.ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    if host.is_empty() { "Prism".into() } else { host }
}

/// Pairs with machine `name` by the code its Prism shows (`prism peer pair` there): returns once
/// accepted there, its token kept in the keychain.
#[tauri::command]
pub async fn machine_pair(name: String, code: String) -> Cmd {
    let m = find(&name)?;
    let me = computer_name().await;
    let token = prism_remote::machine::pair(&m.url, &code, &me).await?;
    // Accepted there, its token given once: lost here, the pairing is to be done again.
    keep(&m, token).await.map_err(|e| format!("{name} accepted this computer, but the keychain did not keep its token ({e:#}): run prism peer remove \"{me}\" there, then pair again").into())
}

/// Machine `name` in its window, or one of its profiles (`profile`, reached through it) in a
/// window of its own: brought to the front if open, opened otherwise.
#[tauri::command]
pub fn machine_open(app: AppHandle, state: State<'_, Arc<Machines>>, prism: State<'_, Arc<prism_commands::AppState>>, name: String, profile: Option<String>) -> Cmd {
    let m = find(&name)?;
    let name = m.name.trim().to_string();
    let profile = profile.filter(|p| !p.trim().is_empty());
    let label = label(&name, profile.as_deref());
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    state.open.lock().unwrap().insert(label.clone(), Open { name: name.clone(), profile: profile.clone(), machine: None });
    // The page learns which machine (and profile) it shows before any of its scripts.
    // Strings and an optional one: JSON that cannot fail to write.
    let script = format!("window.__PRISM_MACHINE__ = {}; window.__PRISM_MACHINE_PROFILE__ = {};", serde_json::json!(name), serde_json::json!(profile));
    // The profile by its name, as the machine said it last.
    let shown = profile.as_ref().map(|id| {
        let snap = prism.hub.subscribe();
        let s = snap.borrow();
        s.machines.iter().find(|m| m.name == name).and_then(|m| m.profiles.iter().find(|p| &p.id == id)).map_or(id.clone(), |p| p.name.clone())
    });
    let title = match &shown {
        Some(p) => format!("Prism — {name} · {p}"),
        None => format!("Prism — {name}"),
    };
    let window = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(1440.0, 900.0)
        .min_inner_size(900.0, 560.0)
        .disable_drag_drop_handler()
        .initialization_script(&script)
        .build();
    if let Err(e) = window {
        state.open.lock().unwrap().remove(&label);
        return Err(e.into());
    }
    Ok(())
}

/// This computer's own window, from a machine's (its profile menu).
#[tauri::command]
pub fn machine_main(app: AppHandle) {
    crate::app::show_main(&app);
}

/// Each machine's summary into the snapshot (`Snapshot.machines`), every few seconds while the
/// window shows, less often hidden: the status bar's chips, and what the window notifies (an
/// agent there waiting for you), whether its window is open or not. Its config read each round:
/// a machine added or paired shows without a restart.
pub fn watch(hub: prism_core::state::Hub) {
    tauri::async_runtime::spawn(async move {
        // The tokens as read last: the keychain (`security` on macOS) is not run every few seconds;
        // read again when missing or refused.
        let mut tokens: std::collections::HashMap<String, String> = Default::default();
        loop {
            let machines = config().map(|c| c.machines).unwrap_or_default();
            let mut out = Vec::new();
            for m in machines.iter().filter(|m| !m.name.trim().is_empty()) {
                let name = m.name.trim().to_string();
                if !tokens.contains_key(&name)
                    && let Ok(Some(t)) = token(m).await
                {
                    tokens.insert(name.clone(), t);
                }
                let presence = match tokens.get(&name).cloned() {
                    Some(token) => match prism_remote::machine::summary(&Target { url: m.url.clone(), token, profile: None }).await {
                        Ok(s) => prism_core::state::MachinePresence { name, reachable: true, error: None, agents: s.agents, waiting: s.waiting, profiles: s.profiles },
                        Err(e) => {
                            // Refused: its token may have been replaced (paired again) since.
                            if format!("{e:#}").contains("token") {
                                tokens.remove(&name);
                            }
                            prism_core::state::MachinePresence { name, error: Some(format!("{e:#}")), ..Default::default() }
                        }
                    },
                    None => prism_core::state::MachinePresence { name, error: Some("not paired yet".into()), ..Default::default() },
                };
                out.push(presence);
            }
            hub.update(|s| prism_core::state::replace(&mut s.machines, out));
            let pace = if hub.shown() { 5 } else { 15 };
            tokio::time::sleep(std::time::Duration::from_secs(pace)).await;
        }
    });
}

/// A machine window closed: its socket and terminals with it.
pub fn closed(app: &AppHandle, label: &str) {
    if !label.starts_with(PREFIX) {
        return;
    }
    let state = app.state::<Arc<Machines>>();
    state.open.lock().unwrap().remove(label);
    state.ttys.lock().unwrap().retain(|(l, _), _| l != label);
}

/// The calling window's machine.
fn machine(state: &Machines, window: &WebviewWindow) -> Cmd<Arc<Machine>> {
    let open = state.open.lock().unwrap();
    let o = open.get(window.label()).ok_or("not a machine's window")?;
    o.machine.clone().ok_or_else(|| "not connected yet".into())
}

/// The calling window's machine, its socket opened again (the page loaded or reloaded): what it
/// sends comes as the window's `machine` event, its snapshot whole first. Answers its name and
/// SSH host.
#[tauri::command]
pub async fn machine_connect(state: State<'_, Arc<Machines>>, window: WebviewWindow) -> Cmd<Value> {
    let (name, profile) = state.open.lock().unwrap().get(window.label()).map(|o| (o.name.clone(), o.profile.clone())).ok_or("not a machine's window")?;
    let m = find(&name)?;
    let token = token(&m).await?.ok_or_else(|| format!("no token for {name}: pair this computer with it (prism peer pair there)"))?;
    let target = Target { url: m.url.clone(), token, profile };
    let to = window.clone();
    let machine = Machine::open(
        target,
        Arc::new(move |msg: Value| {
            let _ = to.emit_to(tauri::EventTarget::webview_window(to.label()), "machine", msg);
        }),
    );
    if let Some(o) = state.open.lock().unwrap().get_mut(window.label()) {
        o.machine = Some(Arc::new(machine));
    }
    state.ttys.lock().unwrap().retain(|(l, _), _| l != window.label());
    Ok(serde_json::json!({ "name": name, "ssh": m.ssh }))
}

/// A command of the window, answered by its machine. What is critical there is first confirmed by
/// the person at this computer (Touch ID, or its password), unless `[[machines]] skip_confirm`:
/// asked here, by the app, so that nothing the page does skips it.
#[tauri::command]
pub async fn machine_call(state: State<'_, Arc<Machines>>, window: WebviewWindow, command: String, args: Option<Value>) -> Cmd<Value> {
    let machine = machine(&state, &window)?;
    let args = args.unwrap_or(Value::Null);
    if critical(&command, &args) {
        let name = state.open.lock().unwrap().get(window.label()).map(|o| o.name.clone()).unwrap_or_default();
        if !find(&name).is_ok_and(|m| m.skip_confirm) {
            let reason = format!("{} on {name}", describe(&command, &args));
            if !tokio::task::spawn_blocking(move || confirm_here(&reason)).await? {
                return Err(format!("{command}: not confirmed on this computer (Touch ID or its password on a Mac, polkit's agent on Linux; Settings › Remote › Machines › No confirmation goes without)").into());
            }
        }
    }
    Ok(machine.call(&command, args).await?)
}

/// Whether `command` is critical, as the machine classes it for a phone (a block's command, its
/// block's class here).
fn critical(command: &str, args: &Value) -> bool {
    use prism_commands::commands::remote::{block_class, class};
    let class = if command == "block_call" {
        let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or_default();
        Some(block_class(s("block"), s("command"), args.get("args").unwrap_or(&Value::Null)))
    } else {
        class(command)
    };
    class == Some(prism_core::remote::Class::Critical)
}

/// What macOS's sheet says Prism wants to do: the command and what it acts on.
fn describe(command: &str, args: &Value) -> String {
    let what = command.replace('_', " ");
    let field = |k: &str| args.get(k).and_then(|v| v.as_str().map(String::from).or_else(|| v.as_u64().map(|n| n.to_string())));
    let target = match (field("iid"), field("repo"), field("id"), field("path"), field("command")) {
        (Some(iid), Some(repo), ..) => format!(" !{iid} ({repo})"),
        (Some(iid), None, ..) => format!(" !{iid}"),
        (_, _, Some(id), ..) => format!(" {id}"),
        (_, _, _, Some(path), _) => format!(" {path}"),
        (_, _, _, _, Some(c)) => format!(" {c}"),
        _ => String::new(),
    };
    format!("{what}{target}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    /// What a machine classes critical asks first; what it reads or acts on lightly does not.
    #[test]
    fn what_is_critical_there_is_confirmed_here() {
        assert!(super::critical("merge_mr", &json!({ "iid": 42 })));
        assert!(super::critical("agent_stop", &json!({ "id": "a1" })));
        assert!(!super::critical("rebase_mr", &json!({ "iid": 42 })));
        assert!(!super::critical("mr_detail", &json!({ "iid": 42 })));
        assert_eq!(super::describe("merge_mr", &json!({ "repo": "api", "iid": 42 })), "merge mr !42 (api)");
        assert_eq!(super::describe("agent_stop", &json!({ "id": "a1" })), "agent stop a1");
    }
}

/// The person at this computer confirms. macOS: Touch ID, or the account's password
/// (LocalAuthentication). Linux: polkit's agent of the desktop (GNOME, KDE, Xfce…) asks for the
/// password, or a fingerprint where PAM takes one, as for any administrative action
/// (`org.freedesktop.policykit.exec`, nothing run); without `pkcheck` or an agent, refused:
/// `skip_confirm` goes without. Elsewhere, nothing to ask with: refused too.
fn confirm_here(reason: &str) -> bool {
    #[cfg(target_os = "macos")]
    return prism_macos::localauth::confirm(reason, std::time::Duration::from_secs(90));
    #[cfg(target_os = "linux")]
    {
        tracing::info!("machines: polkit asked to confirm {reason}");
        let pid = std::process::id().to_string();
        let asked = std::process::Command::new("pkcheck").args(["--action-id", "org.freedesktop.policykit.exec", "--process", &pid, "--allow-user-interaction"]).status();
        match asked {
            Ok(s) => s.success(),
            Err(e) => {
                tracing::warn!("machines: pkcheck: {e} (polkit missing: [[machines]] skip_confirm goes without)");
                false
            }
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = reason;
        false
    }
}

/// An agent's terminal on the window's machine, as `agent_attach` here: its output as raw bytes,
/// then `{"end":true}`. One per agent: attached again, the former is detached.
#[tauri::command]
pub async fn machine_attach(state: State<'_, Arc<Machines>>, window: WebviewWindow, id: String, cols: u16, rows: u16, on_data: tauri::ipc::Channel<tauri::ipc::InvokeResponseBody>) -> Cmd {
    let machine = machine(&state, &window)?;
    let key = (window.label().to_string(), id.clone());
    state.ttys.lock().unwrap().remove(&key);
    let end = on_data.clone();
    let tty = machine
        .attach(
            &id,
            cols,
            rows,
            Box::new(move |bytes| {
                let _ = on_data.send(tauri::ipc::InvokeResponseBody::Raw(bytes));
            }),
            Box::new(move || {
                let _ = end.send(tauri::ipc::InvokeResponseBody::Json(r#"{"end":true}"#.into()));
            }),
        )
        .await?;
    state.ttys.lock().unwrap().insert(key, tty);
    Ok(())
}

#[tauri::command]
pub fn machine_write(state: State<'_, Arc<Machines>>, window: WebviewWindow, id: String, data: String) -> Cmd {
    let ttys = state.ttys.lock().unwrap();
    let tty = ttys.get(&(window.label().to_string(), id)).ok_or("not attached")?;
    Ok(tty.write(data.as_bytes())?)
}

#[tauri::command]
pub fn machine_resize(state: State<'_, Arc<Machines>>, window: WebviewWindow, id: String, cols: u16, rows: u16) -> Cmd {
    let ttys = state.ttys.lock().unwrap();
    let tty = ttys.get(&(window.label().to_string(), id)).ok_or("not attached")?;
    Ok(tty.resize(cols, rows)?)
}

#[tauri::command]
pub fn machine_detach(state: State<'_, Arc<Machines>>, window: WebviewWindow, id: String) {
    state.ttys.lock().unwrap().remove(&(window.label().to_string(), id));
}
