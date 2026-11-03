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
    machine: Option<Arc<Machine>>,
}

const PREFIX: &str = "machine-";

/// A window's label for machine `name`: Tauri takes letters, digits, `-` and `_` only.
fn label(name: &str) -> String {
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("{PREFIX}{safe}")
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
}

/// The machines of `[[machines]]`.
#[tauri::command]
pub async fn machine_list(state: State<'_, Arc<Machines>>) -> Cmd<Vec<MachineView>> {
    let mut out = Vec::new();
    for m in config()?.machines.into_iter().filter(|m| !m.name.trim().is_empty()) {
        let token = token(&m).await.ok().flatten().is_some();
        let open = state.open.lock().unwrap().contains_key(&label(m.name.trim()));
        out.push(MachineView { name: m.name.trim().to_string(), url: m.url, ssh: m.ssh, token, open });
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

/// Machine `name` in its window: brought to the front if open, opened otherwise.
#[tauri::command]
pub fn machine_open(app: AppHandle, state: State<'_, Arc<Machines>>, name: String) -> Cmd {
    let m = find(&name)?;
    let name = m.name.trim().to_string();
    let label = label(&name);
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    state.open.lock().unwrap().insert(label.clone(), Open { name: name.clone(), machine: None });
    // The page learns which machine it shows before any of its scripts.
    let script = format!("window.__PRISM_MACHINE__ = {};", serde_json::to_string(&name).map_err(|e| e.to_string())?);
    let window = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("index.html".into()))
        .title(format!("Prism — {name}"))
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
    let name = state.open.lock().unwrap().get(window.label()).map(|o| o.name.clone()).ok_or("not a machine's window")?;
    let m = find(&name)?;
    let token = token(&m).await?.ok_or_else(|| format!("no token for {name}: pair this computer with it (prism peer pair there)"))?;
    let target = Target { url: m.url.clone(), token };
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

/// A command of the window, answered by its machine.
#[tauri::command]
pub async fn machine_call(state: State<'_, Arc<Machines>>, window: WebviewWindow, command: String, args: Option<Value>) -> Cmd<Value> {
    let machine = machine(&state, &window)?;
    Ok(machine.call(&command, args.unwrap_or(Value::Null)).await?)
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
