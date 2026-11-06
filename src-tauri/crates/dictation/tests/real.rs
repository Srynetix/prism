//! Against the real thing, by hand (`--ignored`): a whisper model downloaded and checked, then a
//! sentence spoken by macOS's `say` transcribed. `PRISM_DICTATION_DIR`: where the model goes.

use std::sync::atomic::AtomicBool;

#[tokio::test]
#[ignore]
async fn downloads_tiny_and_transcribes_a_spoken_sentence() {
    let dir = std::path::PathBuf::from(std::env::var("PRISM_DICTATION_DIR").expect("PRISM_DICTATION_DIR"));
    let m = prism_dictation::models::model("tiny").unwrap();
    if !prism_dictation::models::installed(&dir, &m) {
        prism_dictation::models::download(&dir, &m, &AtomicBool::new(false), |_| {}).await.unwrap();
    }
    assert!(prism_dictation::models::installed(&dir, &m));
    let wav = dir.join("spoken.wav");
    let aiff = dir.join("spoken.aiff");
    assert!(std::process::Command::new("say").args(["-v", "Samantha", "-o"]).arg(&aiff).arg("Please run the tests on the main branch.").status().unwrap().success());
    assert!(std::process::Command::new("afconvert").args(["-f", "WAVE", "-d", "LEF32@44100", "-c", "1"]).arg(&aiff).arg(&wav).status().unwrap().success());
    let bytes = std::fs::read(&wav).unwrap();
    // The `data` chunk of the WAV: little-endian floats at 44.1 kHz, brought to 16 kHz.
    let at = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
    let samples: Vec<f32> = bytes[at..].chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    let samples = prism_dictation::resample::to_rate(&samples, 44_100, 16_000);
    let o = prism_dictation::Options { engine: "whisper".into(), model: "tiny".into(), language: "en".into(), fallback_language: "en".into(), hints: vec!["main".into()], models_dir: dir.clone() };
    let t = prism_dictation::transcribe(&o, &samples).unwrap();
    // Before the process exits (ggml's Metal backend aborts otherwise).
    prism_dictation::whisper::unload(std::time::Duration::ZERO);
    println!("{} ms: {}", t.millis, t.text);
    let low = t.text.to_lowercase();
    assert!(low.contains("tests") && low.contains("main"), "{}", t.text);
}
