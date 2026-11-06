// No extra console besides the window on Windows; no effect elsewhere.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod audio;
mod menu;
mod feedback;
mod youtube_relay;
mod machines;
mod dictation;
mod setup;

mod blocks;

use prism_agents as agents;
use prism_base::{fonts, platform};
use prism_collectors as collectors;
use prism_core::{config, profile, state};
#[cfg(test)]
use prism_collectors::forge;
#[cfg(test)]
use prism_core::{detect, packs, settings};
use prism_demo as demo;
#[cfg(test)]
use prism_ops::{clones, updates};
pub use prism_commands::config_path;

use tauri::{Manager, WindowEvent};

fn main() {
    // Prism's own blocks, then a team's (`PRISM_EXTRA_BLOCKS`).
    blocks::register();

    let mut args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // `prism --profile <id> …`: the profile, before the subcommands.
    profile::take_arg(&mut args);
    if let Some(code) = prism_commands::cli::run(&args) {
        std::process::exit(code);
    }

    // Before any thread (the runtime's workers start with `process::start`, below).
    prism_commands::process::before_threads();
    // One Prism per profile: if this one is already open, it comes to the front instead.
    if let Some(pid) = prism_commands::process::start(tauri::async_runtime::handle().inner().clone()) {
        tracing::info!("profile {} already open (pid {pid}): brought to the front", profile::active());
        profile::wake(pid);
        std::process::exit(0);
    }
    // What the library crates reach of Tauri: its opener.
    platform::set_opener(platform::Opener {
        path: |path| tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| anyhow::anyhow!("{e}")),
        reveal: |path| tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| anyhow::anyhow!("{e}")),
        url: |url| tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| anyhow::anyhow!("{e}")),
    });
    #[cfg(target_os = "linux")]
    warn_webkit_env();

    let hub = state::Hub::new();
    let team_id = collectors::clickup::TeamId::default();
    let time = collectors::time::TimeService::new(hub.clone(), team_id.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // The file and folder pickers of Settings.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        // An agent's pages in the Web view: `prismfile://localhost/<agent>/<path>`, only
        // inside its folder; its CSP sandboxes it (an opaque origin: no IPC).
        .register_asynchronous_uri_scheme_protocol("prismfile", app::serve_agent_file)
        // Other computers' Prism, each in a window of its own.
        .manage(std::sync::Arc::new(machines::Machines::default()))
        .setup({
            let (hub, time, team_id) = (hub.clone(), time.clone(), team_id.clone());
            move |app| setup::run(app, hub, time, team_id)
        })
        .on_window_event(|window, event| {
            // Closing the window hides it: the menu bar and notifications keep running. On Linux it
            // is minimised instead: without AppIndicator (GNOME by default) there is no tray icon to
            // bring a hidden window back, while the task bar always shows a minimised one.
            // A machine's window closes for good, its socket and terminals with it.
            if window.label().starts_with("machine-") {
                if let WindowEvent::Destroyed = event {
                    machines::closed(window.app_handle(), window.label());
                }
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    #[cfg(target_os = "linux")]
                    let _ = window.minimize();
                    #[cfg(not(target_os = "linux"))]
                    let _ = window.hide();
                    window.state::<std::sync::Arc<app::AppState>>().hub.set_visible(false);
                }
                // Back from the task bar (Linux) or the Dock: the window is seen again.
                WindowEvent::Focused(true) => window.state::<std::sync::Arc<app::AppState>>().hub.set_visible(true),
                _ => {}
            }
        })
        .invoke_handler(commands::handler())
        .build(tauri::generate_context!())
        .expect("failed to build Prism")
        .run(|app, event| match event {
            // Click on the Dock icon while the window is hidden.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => app::show_main(app),
            // ⌘Q, Quit from the menu, system shutdown: nothing must outlive Prism.
            tauri::RunEvent::Exit => {
                profile::release();
                // whisper's model let go before ggml's statics are (it aborts otherwise).
                prism_dictation::whisper::unload(std::time::Duration::ZERO);
                // `tailscale serve` must not keep sending phones to a Prism gone.
                let remote = app.state::<std::sync::Arc<app::AppState>>().remote.clone();
                // Bounded: quitting never waits on Tailscale for long.
                tauri::async_runtime::block_on(async { tokio::time::timeout(std::time::Duration::from_secs(5), remote.apply(&Default::default())).await.ok() });
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                audio::now_playing::stop();
            }
            _ => {
                let _ = app;
            }
        });
}


/// WebKitGTK 2.50 crashes (`AcceleratedBackingStore::update`) on the first page change that
/// composites when `WEBKIT_DISABLE_COMPOSITING_MODE` or `WEBKIT_DISABLE_DMABUF_RENDERER` is set,
/// the usual workarounds for Nvidia and Wayland glitches with older versions: said in the log,
/// where a crash report leads.
#[cfg(target_os = "linux")]
fn warn_webkit_env() {
    let (major, minor) = prism_sys::webkit::version();
    if (major, minor) < (2, 50) {
        return;
    }
    for name in ["WEBKIT_DISABLE_COMPOSITING_MODE", "WEBKIT_DISABLE_DMABUF_RENDERER"] {
        if prism_commands::commands::system::set_for_webkit(std::env::var_os(name).as_deref()) {
            tracing::warn!("{name} is set: WebKitGTK {major}.{minor} crashes with it on a page change; unset it if Prism quits on its own");
        }
    }
}

/// `web/src/bindings/`: the window's types, generated from the crates' own (`cargo test`, `task
/// check`), which derive them with their `ts` feature.
#[cfg(test)]
#[test]
fn export_bindings() {
    use ts_rs::TS;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/src/bindings");
    // Rewritten whole: a type renamed or removed leaves no file behind.
    let _ = std::fs::remove_dir_all(&dir);
    // JSON numbers: `number`, whatever their Rust width.
    let cfg = ts_rs::Config::new().with_out_dir(dir).with_large_int("number");
    state::Snapshot::export_all(&cfg).unwrap();
    // What commands return, beyond the snapshot.
    collectors::task_view::TaskView::export_all(&cfg).unwrap();
    collectors::ticket_commits::TicketCommit::export_all(&cfg).unwrap();
    state::UsageHistory::export_all(&cfg).unwrap();
    state::AwayCheck::export_all(&cfg).unwrap();
    // The pages wizard: what the machine has.
    detect::Detection::export_all(&cfg).unwrap();
    prism_core::deps::Dependency::export_all(&cfg).unwrap();
    // Settings › Agents › Prism MCP: the agent CLIs outside Prism.
    prism_commands::mcp::install::Installed::export_all(&cfg).unwrap();
    forge::MrDetail::export_all(&cfg).unwrap();
    forge::MrDiff::export_all(&cfg).unwrap();
    forge::MrCommit::export_all(&cfg).unwrap();
    // An agent's panels: its conversation, context, activity, files, diff, tasks, shells, slash commands.
    agents::conversation::Conversation::export_all(&cfg).unwrap();
    agents::switch::Material::export_all(&cfg).unwrap();
    agents::breakdown::Breakdown::export_all(&cfg).unwrap();
    agents::activity::Activity::export_all(&cfg).unwrap();
    state::AgentCost::export_all(&cfg).unwrap();
    agents::files::Listing::export_all(&cfg).unwrap();
    agents::files::Diff::export_all(&cfg).unwrap();
    agents::files::OverlapDiff::export_all(&cfg).unwrap();
    agents::files::Preview::export_all(&cfg).unwrap();
    agents::files::Found::export_all(&cfg).unwrap();
    agents::files::Query::export_all(&cfg).unwrap();
    agents::files::Sent::export_all(&cfg).unwrap();
    agents::files::Artifact::export_all(&cfg).unwrap();
    agents::files::Page::export_all(&cfg).unwrap();
    agents::tasks::Source::export_all(&cfg).unwrap();
    agents::Shell::export_all(&cfg).unwrap();
    agents::OpenShell::export_all(&cfg).unwrap();
    agents::slash::SlashCommand::export_all(&cfg).unwrap();
    // Settings, packs, profiles, the forge's people, labels and logs, a pod, an event.
    packs::PackInfo::export_all(&cfg).unwrap();
    packs::PackCheck::export_all(&cfg).unwrap();
    settings::PackFile::export_all(&cfg).unwrap();
    settings::FileView::export_all(&cfg).unwrap();
    profile::Profile::export_all(&cfg).unwrap();
    forge::Member::export_all(&cfg).unwrap();
    forge::JobLog::export_all(&cfg).unwrap();
    forge::JobInfo::export_all(&cfg).unwrap();
    forge::ForgeLabel::export_all(&cfg).unwrap();
    collectors::environments::PodInfo::export_all(&cfg).unwrap();
    collectors::calendar::EventDetails::export_all(&cfg).unwrap();
    agents::worktree::Plan::export_all(&cfg).unwrap();
    agents::vcs::GitState::export_all(&cfg).unwrap();
    agents::vcs::BranchChoice::export_all(&cfg).unwrap();
    // Projects, updates, feedback (`prism-ops`), and the app's own.
    updates::UpdateInfo::export_all(&cfg).unwrap();
    clones::ForgeProject::export_all(&cfg).unwrap();
    clones::Folder::export_all(&cfg).unwrap();
    clones::Created::export_all(&cfg).unwrap();
    prism_commands::commands::feedback::FeedbackInfo::export_all(&cfg).unwrap();
    feedback::Issue::export_all(&cfg).unwrap();
    feedback::Request::export_all(&cfg).unwrap();
    audio::Episode::export_all(&cfg).unwrap();
    audio::now_playing::NowPlaying::export_all(&cfg).unwrap();
    // The blocks' states (`Snapshot.blocks.<id>`).
    prism_blocks::docker::DockerState::export_all(&cfg).unwrap();
    prism_blocks::status::StatusState::export_all(&cfg).unwrap();
    prism_blocks::dev_servers::DevServersState::export_all(&cfg).unwrap();
    prism_blocks::system::SystemState::export_all(&cfg).unwrap();
    prism_blocks::sound::SoundState::export_all(&cfg).unwrap();
    prism_blocks::disk_usage::DiskUsageState::export_all(&cfg).unwrap();
    prism_blocks::custom::State::export_all(&cfg).unwrap();
    prism_blocks::custom::Read::export_all(&cfg).unwrap();
    prism_blocks::disk_usage::NodeView::export_all(&cfg).unwrap();
    prism_blocks::tokens::TokensState::export_all(&cfg).unwrap();
    prism_blocks::data::GcpData::export_all(&cfg).unwrap();
    prism_blocks::trail::TrailState::export_all(&cfg).unwrap();
    prism_blocks::changes::ChangesState::export_all(&cfg).unwrap();
    // What each agent CLI can do: values, not only a type, for the window to read at once.
    use prism_core::agents::provider::{Provider, ProviderSpec};
    ProviderSpec::export_all(&cfg).unwrap();
    let specs: serde_json::Map<String, serde_json::Value> = Provider::ALL.iter().map(|p| (serde_json::to_value(p).unwrap().as_str().unwrap().to_string(), serde_json::to_value(p.spec()).unwrap())).collect();
    let ts = format!(
        "// Generated by export_bindings from `Provider::spec` (crates/core/src/agents/provider.rs). Do not edit this file manually.\nimport type {{ AgentProvider }} from './AgentProvider'\nimport type {{ ProviderSpec }} from './ProviderSpec'\n\nexport const PROVIDER_SPECS: Record<AgentProvider, ProviderSpec> = {};\n",
        serde_json::to_string_pretty(&specs).unwrap()
    );
    std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/src/bindings/provider-specs.ts"), ts).unwrap();
}

/// The demo snapshot for the window's browser preview, with every block of this build:
/// `PRISM_DEMO_SNAPSHOT=<file> cargo test dump_for_preview -- --ignored`.
#[cfg(test)]
#[test]
#[ignore]
fn dump_for_preview() {
    blocks::register();
    let path = std::env::var("PRISM_DEMO_SNAPSHOT").expect("PRISM_DEMO_SNAPSHOT=<file>");
    std::fs::write(path, serde_json::to_vec(&demo::preview(chrono::Utc::now())).unwrap()).unwrap();
}
