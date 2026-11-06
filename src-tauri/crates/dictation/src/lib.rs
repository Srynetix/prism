//! A prompt dictated at the computer (`[dictation]`): the microphone recorded in memory while a
//! button or a key is held, then transcribed on this computer, never online. Two engines:
//! `system`, macOS's speech recognition forced on the device, and `whisper`, whisper.cpp built in
//! (macOS and Linux), its model downloaded on demand. `auto`: the system's where it recognizes the
//! language on this Mac, whisper otherwise.

pub mod capture;
pub mod models;
pub mod resample;
pub mod whisper;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use prism_base::{tr, i18n::tr};
use serde::Serialize;

/// What a transcription needs, from `[dictation]` and the agent dictated to.
#[derive(Debug, Clone)]
pub struct Options {
    /// `auto`, `system` or `whisper`.
    pub engine: String,
    /// The whisper model (`base`…).
    pub model: String,
    /// `auto` or a language code (`fr`, `en`).
    pub language: String,
    /// The language when `language` is `auto` and the engine needs one (the system's): the
    /// prompts' or the app's.
    pub fallback_language: String,
    /// Words to expect: the repository's, branches', files' names.
    pub hints: Vec<String>,
    /// Where the whisper models are kept.
    pub models_dir: PathBuf,
}

/// The engine that transcribes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    System,
    Whisper,
}

/// A transcription: its text, and what made it.
#[derive(Debug, Clone, Serialize)]
pub struct Transcript {
    pub text: String,
    pub engine: Engine,
    /// The whisper model, for whisper.
    pub model: Option<String>,
    pub millis: u64,
}

/// The language as a locale the system's recognition takes (`fr` → `fr-FR`).
pub fn locale_of(lang: &str) -> String {
    let l = lang.trim();
    match l.to_ascii_lowercase().as_str() {
        "fr" => "fr-FR".into(),
        "en" => "en-US".into(),
        "de" => "de-DE".into(),
        "es" => "es-ES".into(),
        "it" => "it-IT".into(),
        "pt" => "pt-PT".into(),
        _ => l.replace('_', "-"),
    }
}

/// The language a transcription asks for: `language`, or the fallback where it is `auto` and the
/// engine cannot detect it; for whisper, its primary subtag (`fr-FR` → `fr`), the codes it knows.
pub fn language_for(o: &Options, engine: Engine) -> String {
    let primary = |l: &str| l.split(['-', '_']).next().unwrap_or(l).trim().to_ascii_lowercase();
    let l = o.language.trim();
    if l.is_empty() || l == "auto" {
        if engine == Engine::Whisper {
            return "auto".into();
        }
        let f = primary(&o.fallback_language);
        return if f.is_empty() { "en".into() } else { f };
    }
    if engine == Engine::System { l.to_string() } else { primary(l) }
}

/// macOS's on-device recognition can take the language (and Prism has not been refused it).
pub fn system_usable(lang: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        use prism_macos::speech::{authorization, on_device, Auth};
        !matches!(authorization(), Auth::Denied | Auth::Restricted) && on_device(&locale_of(lang))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = lang;
        false
    }
}

/// The engine `o` resolves to.
pub fn engine_of(o: &Options) -> Engine {
    match o.engine.as_str() {
        "system" if cfg!(target_os = "macos") => Engine::System,
        "whisper" | "system" => Engine::Whisper,
        _ => {
            let lang = language_for(o, Engine::System);
            if system_usable(&lang) { Engine::System } else { Engine::Whisper }
        }
    }
}

/// `samples` (mono, 16 kHz) as text, on this computer. Blocks (seconds for a long dictation): call
/// it on a blocking thread.
pub fn transcribe(o: &Options, samples: &[f32]) -> Result<Transcript> {
    let started = Instant::now();
    let engine = engine_of(o);
    let lang = language_for(o, engine);
    let text = match engine {
        Engine::System => system(samples, &lang, &o.hints)?,
        Engine::Whisper => {
            let m = models::model(&o.model).ok_or_else(|| anyhow!(tr!("Unknown whisper model {model}", model = o.model)))?;
            if !models::installed(&o.models_dir, &m) {
                bail!(tr!("The whisper model {model} is not downloaded: Settings › Agents › Dictation", model = m.name));
            }
            whisper::transcribe(&models::path(&o.models_dir, &m), samples, &lang, &o.hints)?
        }
    };
    Ok(Transcript { text: text.trim().to_string(), engine, model: (engine == Engine::Whisper).then(|| o.model.clone()), millis: started.elapsed().as_millis() as u64 })
}

#[cfg(target_os = "macos")]
fn system(samples: &[f32], lang: &str, hints: &[String]) -> Result<String> {
    use prism_macos::speech::{request_authorization, transcribe, Auth};
    match request_authorization(Duration::from_secs(120)) {
        Auth::Authorized => {}
        Auth::NotDetermined => bail!(tr("Speech recognition was not allowed yet: answer macOS’s request, then dictate again")),
        _ => bail!(tr("Speech recognition is not allowed for Prism: System Settings › Privacy & Security › Speech Recognition")),
    }
    // Up to a minute and a half for five minutes of speech.
    let wait = Duration::from_secs(30 + samples.len() as u64 / capture::RATE as u64 / 4);
    transcribe(samples, capture::RATE as f64, &locale_of(lang), hints, wait).map_err(|e| anyhow!(tr!("macOS’s speech recognition: {err}", err = e)))
}

#[cfg(not(target_os = "macos"))]
fn system(_: &[f32], _: &str, _: &[String]) -> Result<String> {
    bail!(tr("The system’s dictation is macOS’s only: choose whisper"))
}

/// The demo profile's transcription: a sentence, after a moment, whatever was said.
pub fn demo(seconds: f32) -> Transcript {
    std::thread::sleep(Duration::from_millis(600));
    let text = if prism_base::i18n::lang() == prism_base::i18n::Lang::Fr {
        "Pour la pagination, remplace l'offset par un curseur dans InvoiceList.svelte et ajoute un test sur la dernière page."
    } else {
        "For the pagination, replace the offset with a cursor in InvoiceList.svelte and add a test on the last page."
    };
    let _ = seconds;
    Transcript { text: text.into(), engine: Engine::Whisper, model: Some("base".into()), millis: 600 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(engine: &str, language: &str) -> Options {
        Options { engine: engine.into(), model: "base".into(), language: language.into(), fallback_language: "fr".into(), hints: vec![], models_dir: std::env::temp_dir().join("prism-dictation-none") }
    }

    #[test]
    fn languages_and_locales() {
        assert_eq!(locale_of("fr"), "fr-FR");
        assert_eq!(locale_of("en_GB"), "en-GB");
        assert_eq!(language_for(&opts("auto", "auto"), Engine::System), "fr");
        assert_eq!(language_for(&opts("auto", "auto"), Engine::Whisper), "auto");
        assert_eq!(language_for(&opts("auto", "en"), Engine::System), "en");
        assert_eq!(language_for(&opts("auto", ""), Engine::Whisper), "auto");
        assert_eq!(language_for(&opts("auto", "fr-FR"), Engine::Whisper), "fr");
        assert_eq!(language_for(&opts("auto", "fr-FR"), Engine::System), "fr-FR");
        let mut o = opts("auto", "auto");
        o.fallback_language = String::new();
        assert_eq!(language_for(&o, Engine::System), "en");
    }

    #[test]
    fn whisper_without_its_model_says_where_to_get_it() {
        let o = opts("whisper", "fr");
        assert_eq!(engine_of(&o), Engine::Whisper);
        let e = transcribe(&o, &[0.0; 16_000]).unwrap_err().to_string();
        assert!(e.contains("not downloaded"), "{e}");
    }
}
