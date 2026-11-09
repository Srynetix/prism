//! Prism without a window (`prism-server serve`): on a server, run by systemd, reached from a phone
//! only. The same Prism as the desk's (`AppState::start`: collectors, agents and their chains, the
//! remote), the phone's page served from the files the binary carries. Also a profile's Prism shown
//! in a window of the app (`prism engine`), which reaches it through its local socket.

use std::sync::Arc;

use crate::{app::AppState, config_path, profile, state, Shell};

/// The phone's page as the binary carries it: a file's bytes, by its path in `web/dist`.
pub type Assets = Box<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/// What a Prism without a window does for the commands.
struct Headless {
    assets: Assets,
}

impl Shell for Headless {
    /// The same binary again, in place (systemd sees the same process): agents and stores
    /// outlive it.
    fn restart(&self) -> ! {
        profile::release();
        let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
        let error = match std::env::current_exe() {
            #[cfg(unix)]
            Ok(exe) => std::os::unix::process::CommandExt::exec(std::process::Command::new(exe).args(args)),
            #[cfg(not(unix))]
            Ok(exe) => match std::process::Command::new(exe).args(args).spawn() {
                Ok(_) => std::process::exit(0),
                Err(e) => e,
            },
            Err(e) => e,
        };
        tracing::error!("restart: {error}");
        std::process::exit(1)
    }

    fn asset(&self, path: &str) -> Option<(Vec<u8>, String)> {
        (self.assets)(path).map(|bytes| (bytes, mime(path).into()))
    }
}

/// A file's type, by its extension: the page's files only.
pub fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        "json" | "webmanifest" => "application/manifest+json",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// Runs Prism on the active profile until it is stopped (SIGTERM, SIGINT): its exit code. `build`
/// says which build this is (feedback's context); `engine`, for a window of the app.
pub fn run(assets: Assets, build: String, engine: bool) -> i32 {
    crate::process::before_threads();
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("prism: {e}");
            return 1;
        }
    };
    if let Some(pid) = crate::process::start(runtime.handle().clone()) {
        tracing::error!("profile {} already open (pid {pid})", profile::active());
        return 1;
    }
    if !engine && assets("remote.html").is_none() {
        tracing::warn!("this build carries no phone page (web/dist was not built before it): phones get nothing to open");
    }
    let code = runtime.block_on(serve(assets, build, engine));
    profile::release();
    // Prism's tasks (collectors, agents' watch) are not waited for.
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    code
}

async fn serve(assets: Assets, build: String, engine: bool) -> i32 {
    let hub = state::Hub::new();
    // No window: collections slow down until a phone watches.
    hub.set_visible(false);
    let team_id = crate::collectors::clickup::TeamId::default();
    let time = crate::collectors::time::TimeService::new(hub.clone(), team_id.clone());
    let data_dir = profile::data_dir(&profile::data_base());
    let state = match AppState::start(data_dir, hub, time, team_id, Arc::new(Headless { assets }), build) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("{e:#}");
            return 1;
        }
    };
    tracing::info!("Prism without a window, profile {}, config {}", profile::active(), config_path().display());
    // The app's window of this profile reaches it there.
    let socket = profile::engine_socket(profile::active());
    // Removed when stopping only if this Prism made it (another may listen there). Windows: a named
    // pipe, gone with the process.
    let bound = match state.remote.serve_local(&socket).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("local socket: {e:#}");
            false
        }
    };
    if !engine && !crate::config::load(&config_path()).is_ok_and(|c| c.remote.enabled) {
        tracing::warn!("[remote] is off in {}: no phone reaches this Prism (enabled = true, then `prism-server pair`)", config_path().display());
    }
    stopped(&state).await;
    tracing::info!("stopping");
    if bound && cfg!(unix) {
        let _ = std::fs::remove_file(&socket);
    }
    // `tailscale serve` must not keep sending phones to a Prism gone; bounded.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), state.remote.apply(&Default::default())).await;
    0
}

/// Until Prism is asked to stop; `prism restart` (SIGUSR2) restarts it meanwhile.
#[cfg(unix)]
async fn stopped(state: &AppState) {
    use tokio::signal::unix::{signal, SignalKind};
    let (Ok(mut term), Ok(mut int), Ok(mut restart), Ok(mut wake)) = (signal(SignalKind::terminate()), signal(SignalKind::interrupt()), signal(SignalKind::user_defined2()), signal(SignalKind::user_defined1())) else {
        let _ = tokio::signal::ctrl_c().await;
        return;
    };
    loop {
        tokio::select! {
            _ = term.recv() => return,
            _ = int.recv() => return,
            _ = restart.recv() => {
                tracing::info!("prism restart: restarting");
                let _ = tokio::time::timeout(std::time::Duration::from_secs(5), state.remote.apply(&Default::default())).await;
                state.shell.restart()
            }
            // Another launch on this profile wants its window shown: there is none (and the
            // signal must not end this Prism).
            _ = wake.recv() => tracing::info!("profile {} is open here, without a window", profile::active()),
        }
    }
}

/// Windows: until Ctrl-C or `STOP` (`profile::stop`); `prism restart` (its byte on this process's
/// pipe) restarts it meanwhile.
#[cfg(windows)]
async fn stopped(state: &AppState) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let listen = tokio::spawn(prism_sys::windows::listen_signals(move |byte| {
        let _ = tx.send(byte);
    }));
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            Some(byte) = rx.recv() => match byte {
                profile::RESTART => {
                    tracing::info!("prism restart: restarting");
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), state.remote.apply(&Default::default())).await;
                    state.shell.restart()
                }
                profile::WAKE => tracing::info!("profile {} is open here, without a window", profile::active()),
                // The app takes this profile over, or quits with the Prisms it started.
                profile::STOP => break,
                _ => {}
            },
        }
    }
    listen.abort();
}

#[cfg(not(any(unix, windows)))]
async fn stopped(_state: &AppState) {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(path: &str) -> Option<Vec<u8>> {
        (path == "remote.html").then(|| b"<html>".to_vec())
    }

    /// The page's files from what the binary carries, with their type; nothing else.
    #[test]
    fn serves_the_page_it_carries() {
        let shell = Headless { assets: Box::new(page) };
        assert_eq!(shell.asset("remote.html"), Some((b"<html>".to_vec(), "text/html; charset=utf-8".into())));
        assert_eq!(shell.asset("assets/x.js"), None);
        assert_eq!((mime("assets/a.js"), mime("remote/icon.webp"), mime("x.woff2"), mime("dev-snapshot")), ("text/javascript", "image/webp", "font/woff2", "application/octet-stream"));
    }
}
