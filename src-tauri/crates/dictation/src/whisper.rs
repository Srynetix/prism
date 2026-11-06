//! whisper.cpp, built into Prism: the model loaded at the first dictation and kept for the next
//! ones (loading it takes longer than transcribing a sentence), then let go when unused for a while
//! and before Prism exits: ggml's Metal backend aborts the process if a model is still loaded when
//! its own statics are destroyed.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use prism_base::tr;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// The model loaded, by its path, and when it was last used.
static LOADED: Mutex<Option<(PathBuf, WhisperContext, Instant)>> = Mutex::new(None);
static QUIET: Once = Once::new();
/// Set while Prism exits: a transcription under way stops at once (`unload`).
static ABORT: AtomicBool = AtomicBool::new(false);

/// `samples` (mono, 16 kHz) as text, with the model at `model`. `language`: a code (`fr`, `en`),
/// or `auto` to let whisper detect it; `hints`: words it should expect (the repository's,
/// branches', files' names), given as its initial prompt.
pub fn transcribe(model: &Path, samples: &[f32], language: &str, hints: &[String]) -> Result<String> {
    // whisper.cpp writes its progress to stderr: kept out of Prism's log.
    QUIET.call_once(whisper_rs::install_logging_hooks);
    let mut loaded = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if loaded.as_ref().is_none_or(|(p, _, _)| p != model) {
        *loaded = None;
        let ctx = WhisperContext::new_with_params(model, WhisperContextParameters::default()).map_err(|e| anyhow!(tr!("Could not load the whisper model: {err}", err = e)))?;
        *loaded = Some((model.to_path_buf(), ctx, Instant::now()));
    }
    let (_, ctx, used) = loaded.as_mut().expect("loaded above");
    *used = Instant::now();
    let mut state = ctx.create_state().map_err(|e| anyhow!("whisper: {e}"))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8) as i32;
    params.set_n_threads(threads);
    let language = language.trim();
    params.set_language(Some(if language.is_empty() { "auto" } else { language }));
    params.set_translate(false);
    params.set_no_timestamps(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_abort_callback_safe(|| ABORT.load(Ordering::Relaxed));
    let prompt = prompt_of(hints);
    if !prompt.is_empty() {
        params.set_initial_prompt(&prompt);
    }
    // Under a second, whisper hears nothing: padded with silence to one.
    let mut audio = samples.to_vec();
    if audio.len() < crate::capture::RATE as usize + crate::capture::RATE as usize / 10 {
        audio.resize(crate::capture::RATE as usize + crate::capture::RATE as usize / 10, 0.0);
    }
    state.full(params, &audio).map_err(|e| anyhow!("whisper: {e}"))?;
    let text: Vec<String> = state.as_iter().filter_map(|s| s.to_str_lossy().ok().map(|t| t.trim().to_string())).filter(|t| !t.is_empty()).collect();
    Ok(clean(&text.join(" ")))
}

/// Lets the model go when it has not been used for `idle`. `Duration::ZERO`, as Prism exits: a
/// transcription under way is stopped first, rather than waited for.
pub fn unload(idle: Duration) {
    let now = idle.is_zero();
    if now {
        ABORT.store(true, Ordering::Relaxed);
    }
    let mut loaded = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if loaded.as_ref().is_some_and(|(_, _, used)| used.elapsed() >= idle) {
        *loaded = None;
    }
    if now {
        ABORT.store(false, Ordering::Relaxed);
    }
}

/// The hints as whisper's initial prompt: a short list, as a sentence it continues from (whisper
/// keeps the last 224 tokens of it).
pub fn prompt_of(hints: &[String]) -> String {
    let mut out = String::new();
    for h in hints.iter().map(|h| h.trim()).filter(|h| !h.is_empty()) {
        if out.len() + h.len() + 2 > 600 {
            break;
        }
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(h);
    }
    if out.is_empty() { out } else { format!("{out}.") }
}

/// What whisper says of silence or noise, rather than words: nothing.
pub fn clean(text: &str) -> String {
    let t = text.trim();
    let bare = t.trim_matches(|c: char| c == '[' || c == ']' || c == '(' || c == ')' || c == '*' || c.is_whitespace()).to_ascii_lowercase();
    const NOISE: [&str; 8] = ["blank_audio", "silence", "music", "musique", "inaudible", "no speech", "applause", "sous-titrage st' 501"];
    if t.is_empty() || NOISE.contains(&bare.as_str()) || (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')')) {
        return String::new();
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_make_a_short_prompt() {
        assert_eq!(prompt_of(&[]), "");
        assert_eq!(prompt_of(&["prism".into(), " ".into(), "feat/dictation".into()]), "prism, feat/dictation.");
        let many: Vec<String> = (0..400).map(|i| format!("file{i}.rs")).collect();
        assert!(prompt_of(&many).len() <= 601);
    }

    #[test]
    fn noise_is_nothing() {
        assert_eq!(clean(" [BLANK_AUDIO] "), "");
        assert_eq!(clean("(music)"), "");
        assert_eq!(clean("Relance la CI."), "Relance la CI.");
    }
}
