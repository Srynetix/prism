//! Dictation at the computer (`[dictation]`, `prism_dictation`): the window's commands. One
//! recording at a time, held in memory here; its level pushed to the window (`dictation-level`)
//! while it lasts; its text returned when it stops, never sent anywhere by Prism. The whisper
//! models are downloaded from Settings (`dictation-download`). macOS: the Fn key, watched while Prism
//! is the active app (`dictation-fn`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use prism_base::{tr, i18n::tr};
use prism_commands::commands::Cmd;
use prism_dictation::{capture::Recording, models, Options, Transcript};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app::AppState;

/// The recording under way, and the downloads.
pub struct Dictation {
    active: Mutex<Option<Active>>,
    next: AtomicU64,
    downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    models_dir: PathBuf,
}

struct Active {
    id: u64,
    /// `None` in the demo profile: no microphone opened.
    recording: Option<Recording>,
    started: std::time::Instant,
}

/// Installs the state; on macOS, the Fn key's watch (on the main thread, where setup runs).
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let models_dir = app.path().app_data_dir()?.join("dictation").join("models");
    app.manage(Dictation { active: Mutex::new(None), next: AtomicU64::new(1), downloads: Mutex::new(HashMap::new()), models_dir });
    #[cfg(target_os = "macos")]
    {
        let handle = app.handle().clone();
        if !prism_macos::appkit::watch_fn_key(move |down| {
            let _ = handle.emit("dictation-fn", down);
        }) {
            tracing::warn!("dictation: the Fn key is not watched");
        }
    }
    Ok(())
}

/// What a recording's meter shows, pushed every 80 ms.
#[derive(Clone, Serialize)]
struct Level {
    id: u64,
    seconds: f32,
    level: f32,
    /// `max_seconds` reached: the window stops and transcribes.
    full: bool,
    error: Option<String>,
}

/// A recording started.
#[derive(Serialize)]
pub struct Started {
    id: u64,
    device: String,
    max_seconds: u32,
}

fn settings(state: &AppState) -> (prism_core::config::DictationConfig, String, String) {
    let s = state.hub.subscribe();
    let s = s.borrow();
    (s.dictation.clone(), s.agents_settings.prompt_language.clone(), s.appearance.language.clone())
}

/// Starts recording from the microphone of `[dictation] input_device`. One at a time: a recording
/// still under way is dropped first, before the device is opened again. Off the main thread: opening
/// the device may wait on macOS's permission prompt or a Bluetooth headset.
#[tauri::command]
pub async fn dictation_start(app: AppHandle, state: State<'_, Arc<AppState>>, dictation: State<'_, Dictation>) -> Cmd<Started> {
    let (cfg, _, _) = settings(&state);
    if !cfg.enabled {
        return Err(tr("Dictation is off: Settings › Agents › Dictation").into());
    }
    let max = cfg.max_seconds.clamp(5, 3600);
    let old = dictation.active.lock().unwrap_or_else(|e| e.into_inner()).take();
    let device = cfg.input_device.clone();
    let recording = tokio::task::spawn_blocking(move || {
        // Its capture thread joined here, out of the lock, before the device opens again.
        drop(old);
        if prism_demo::active() { Ok(None) } else { Recording::start(&device, max).map(Some) }
    })
    .await??;
    let device = recording.as_ref().map(|r| r.device.clone()).unwrap_or_else(|| tr("Demo microphone").into());
    let id = dictation.next.fetch_add(1, Ordering::Relaxed);
    let replaced = dictation.active.lock().unwrap_or_else(|e| e.into_inner()).replace(Active { id, recording, started: std::time::Instant::now() });
    if replaced.is_some() {
        tokio::task::spawn_blocking(move || drop(replaced));
    }
    tauri::async_runtime::spawn(meter(app, id, max));
    Ok(Started { id, device, max_seconds: max })
}

/// The level of recording `id`, pushed until it ends.
async fn meter(app: AppHandle, id: u64, max: u32) {
    let mut tick = tokio::time::interval(Duration::from_millis(80));
    let mut phase = 0f32;
    loop {
        tick.tick().await;
        let d = app.state::<Dictation>();
        let level = {
            let active = d.active.lock().unwrap_or_else(|e| e.into_inner());
            let Some(a) = active.as_ref().filter(|a| a.id == id) else { return };
            match &a.recording {
                Some(r) => Level { id, seconds: r.seconds(), level: r.level(), full: r.full(), error: r.failed() },
                None => {
                    phase += 0.7;
                    let seconds = a.started.elapsed().as_secs_f32();
                    Level { id, seconds, level: 0.25 + 0.2 * phase.sin().abs(), full: seconds >= max as f32, error: None }
                }
            }
        };
        let _ = app.emit("dictation-level", level);
    }
}

/// Stops recording `id` and transcribes it, `hints` (the repository's, branches', files' names)
/// as words to expect. Its audio is dropped once read.
#[tauri::command]
pub async fn dictation_stop(state: State<'_, Arc<AppState>>, dictation: State<'_, Dictation>, id: u64, hints: Vec<String>) -> Cmd<Transcript> {
    let active = {
        let mut a = dictation.active.lock().unwrap_or_else(|e| e.into_inner());
        if a.as_ref().is_none_or(|a| a.id != id) {
            return Err(tr("No recording under way").into());
        }
        a.take().expect("checked above")
    };
    let Some(recording) = active.recording else {
        let seconds = active.started.elapsed().as_secs_f32();
        return Ok(tokio::task::spawn_blocking(move || prism_dictation::demo(seconds)).await?);
    };
    let (cfg, prompt_language, app_language) = settings(&state);
    let options = Options {
        engine: cfg.engine.clone(),
        model: cfg.whisper_model.clone(),
        language: cfg.language.clone(),
        fallback_language: if prompt_language.trim().is_empty() { app_language } else { prompt_language },
        hints: if cfg.hints { hints } else { Vec::new() },
        models_dir: dictation.models_dir.clone(),
    };
    let transcript = tokio::task::spawn_blocking(move || {
        let samples = recording.finish();
        if samples.len() < prism_dictation::capture::RATE as usize / 4 {
            return Ok(Transcript { text: String::new(), engine: prism_dictation::engine_of(&options), model: None, millis: 0 });
        }
        prism_dictation::transcribe(&options, &samples)
    })
    .await??;
    // whisper's model (hundreds of MB) kept for the next dictation, let go after a few idle minutes.
    if transcript.model.is_some() {
        tauri::async_runtime::spawn(async {
            tokio::time::sleep(MODEL_IDLE).await;
            let _ = tokio::task::spawn_blocking(|| prism_dictation::whisper::unload(MODEL_IDLE)).await;
        });
    }
    Ok(transcript)
}

/// How long whisper's model stays loaded after a dictation.
const MODEL_IDLE: Duration = Duration::from_secs(300);

/// Drops recording `id`, without transcribing it (another, newer one is left alone).
#[tauri::command]
pub async fn dictation_cancel(dictation: State<'_, Dictation>, id: u64) -> Cmd {
    let taken = {
        let mut a = dictation.active.lock().unwrap_or_else(|e| e.into_inner());
        if a.as_ref().is_some_and(|a| a.id == id) { a.take() } else { None }
    };
    // Its capture thread joined off the main thread and out of the lock.
    tokio::task::spawn_blocking(move || drop(taken)).await?;
    Ok(())
}

/// What Settings › Agents › Dictation shows: the engine `auto` picks here, the system's
/// recognition, the models, the microphones, what macOS does with Fn.
#[derive(Serialize)]
pub struct Status {
    engine: prism_dictation::Engine,
    /// macOS's on-device recognition takes the language.
    system: bool,
    /// `authorized`, `denied`, `restricted`, `not_determined`; `none` off macOS.
    system_auth: &'static str,
    /// The language the system's recognition would use.
    system_language: String,
    models: Vec<models::ModelInfo>,
    downloading: Vec<String>,
    devices: Vec<String>,
    /// macOS's own use of Fn (`AppleFnUsageType`): 0 none, 1 input source, 2 emoji, 3 dictation.
    fn_usage: Option<u32>,
    demo: bool,
}

#[tauri::command]
pub async fn dictation_status(state: State<'_, Arc<AppState>>, dictation: State<'_, Dictation>) -> Cmd<Status> {
    let (cfg, prompt_language, app_language) = settings(&state);
    let options = Options { engine: cfg.engine.clone(), model: cfg.whisper_model.clone(), language: cfg.language.clone(), fallback_language: if prompt_language.trim().is_empty() { app_language } else { prompt_language }, hints: Vec::new(), models_dir: dictation.models_dir.clone() };
    let downloading: Vec<String> = dictation.downloads.lock().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect();
    let models_dir = dictation.models_dir.clone();
    let status = tokio::task::spawn_blocking(move || {
        let lang = prism_dictation::language_for(&options, prism_dictation::Engine::System);
        #[cfg(target_os = "macos")]
        let (auth, fn_usage) = (
            match prism_macos::speech::authorization() {
                prism_macos::speech::Auth::Authorized => "authorized",
                prism_macos::speech::Auth::Denied => "denied",
                prism_macos::speech::Auth::Restricted => "restricted",
                prism_macos::speech::Auth::NotDetermined => "not_determined",
            },
            prism_macos::appkit::fn_key_usage(),
        );
        #[cfg(not(target_os = "macos"))]
        let (auth, fn_usage) = ("none", None);
        Status {
            engine: prism_dictation::engine_of(&options),
            system: prism_dictation::system_usable(&lang),
            system_auth: auth,
            system_language: prism_dictation::locale_of(&lang),
            models: models::list(&models_dir),
            downloading,
            devices: prism_dictation::capture::devices(),
            fn_usage,
            demo: prism_demo::active(),
        }
    })
    .await?;
    Ok(status)
}

/// A download's progress, pushed as it goes and once at its end.
#[derive(Clone, Serialize)]
struct Download {
    model: String,
    done: u64,
    total: u64,
    finished: bool,
    error: Option<String>,
}

/// Downloads whisper's `model`, checked; its progress pushed as `dictation-download`.
#[tauri::command]
pub fn dictation_download(app: AppHandle, dictation: State<'_, Dictation>, model: String) -> Cmd {
    if prism_demo::active() {
        return Err(prism_demo::READ_ONLY.into());
    }
    let m = models::model(&model).ok_or_else(|| tr!("Unknown whisper model {model}", model = model))?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut d = dictation.downloads.lock().unwrap_or_else(|e| e.into_inner());
        if d.contains_key(&model) {
            return Ok(());
        }
        d.insert(model.clone(), cancel.clone());
    }
    let dir = dictation.models_dir.clone();
    tauri::async_runtime::spawn(async move {
        let last = AtomicU64::new(0);
        let progress = |done: u64| {
            // Every 1 % or so.
            if done - last.load(Ordering::Relaxed) >= m.bytes / 100 {
                last.store(done, Ordering::Relaxed);
                let _ = app.emit("dictation-download", Download { model: m.name.into(), done, total: m.bytes, finished: false, error: None });
            }
        };
        let result = models::download(&dir, &m, &cancel, progress).await;
        app.state::<Dictation>().downloads.lock().unwrap_or_else(|e| e.into_inner()).remove(m.name);
        let error = result.err().map(|e| format!("{e:#}"));
        if let Some(e) = &error {
            tracing::warn!("dictation: download of {}: {e}", m.name);
        }
        let _ = app.emit("dictation-download", Download { model: m.name.into(), done: if error.is_none() { m.bytes } else { 0 }, total: m.bytes, finished: true, error });
    });
    Ok(())
}

/// Stops a download under way.
#[tauri::command]
pub fn dictation_cancel_download(dictation: State<'_, Dictation>, model: String) {
    if let Some(c) = dictation.downloads.lock().unwrap_or_else(|e| e.into_inner()).get(&model) {
        c.store(true, Ordering::Relaxed);
    }
}

/// Removes a downloaded model.
#[tauri::command]
pub fn dictation_remove(dictation: State<'_, Dictation>, model: String) -> Cmd {
    // The models are the profiles' together: the demo profile leaves them alone.
    if prism_demo::active() {
        return Err(prism_demo::READ_ONLY.into());
    }
    let m = models::model(&model).ok_or_else(|| tr!("Unknown whisper model {model}", model = model))?;
    models::remove(&dictation.models_dir, &m).map_err(Into::into)
}
