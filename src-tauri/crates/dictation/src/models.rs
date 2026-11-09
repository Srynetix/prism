//! whisper.cpp's models: never shipped with Prism, downloaded on demand from Settings into a folder
//! of Prism's data (shared by the profiles), each checked against its size and SHA-256 before it
//! is kept.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, bail, Context, Result};
use prism_base::{tr, i18n::tr};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// One of whisper.cpp's multilingual models.
#[derive(Debug, Clone, Copy)]
pub struct Model {
    pub name: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

/// The models offered, smallest first (sizes and digests: the `ggerganov/whisper.cpp` repository
/// of Hugging Face).
pub const MODELS: [Model; 4] = [
    Model { name: "tiny", bytes: 77_691_713, sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21" },
    Model { name: "base", bytes: 147_951_465, sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe" },
    Model { name: "small", bytes: 487_601_967, sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b" },
    Model { name: "medium", bytes: 1_533_763_059, sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208" },
];

pub fn model(name: &str) -> Option<Model> {
    MODELS.iter().copied().find(|m| m.name == name)
}

/// Where a model is kept.
pub fn path(dir: &Path, m: &Model) -> PathBuf {
    dir.join(format!("ggml-{}.bin", m.name))
}

fn url(m: &Model) -> String {
    format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin", m.name)
}

/// A model as Settings lists it.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DictationModel"))]
pub struct ModelInfo {
    pub name: String,
    pub bytes: u64,
    pub installed: bool,
}

pub fn list(dir: &Path) -> Vec<ModelInfo> {
    MODELS.iter().map(|m| ModelInfo { name: m.name.into(), bytes: m.bytes, installed: installed(dir, m) }).collect()
}

/// Downloaded whole (its size; its digest was checked before it was kept).
pub fn installed(dir: &Path, m: &Model) -> bool {
    std::fs::metadata(path(dir, m)).is_ok_and(|md| md.len() == m.bytes)
}

/// Removes a downloaded model.
pub fn remove(dir: &Path, m: &Model) -> Result<()> {
    match std::fs::remove_file(path(dir, m)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Downloads `m` into `dir`: a `.part` file, checked (size and SHA-256), then renamed. `progress`
/// gets the bytes received; `cancel` stops it (the part removed).
pub async fn download(dir: &Path, m: &Model, cancel: &AtomicBool, progress: impl Fn(u64)) -> Result<PathBuf> {
    tokio::fs::create_dir_all(dir).await.with_context(|| tr!("Could not create {path}", path = dir.display().to_string()))?;
    let dest = path(dir, m);
    let part = dest.with_extension("bin.part");
    let result = fetch(m, &part, cancel, progress).await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e);
    }
    tokio::fs::rename(&part, &dest).await?;
    Ok(dest)
}

async fn fetch(m: &Model, part: &Path, cancel: &AtomicBool, progress: impl Fn(u64)) -> Result<()> {
    let mut resp = prism_base::http::client().get(url(m)).send().await.with_context(|| tr!("Could not download the {model} model", model = m.name))?;
    if !resp.status().is_success() {
        bail!(tr!("Could not download the {model} model: HTTP {status}", model = m.name, status = resp.status().as_u16()));
    }
    let mut file = tokio::fs::File::create(part).await?;
    let mut hash = Sha256::new();
    let mut done = 0u64;
    while let Some(chunk) = tokio::time::timeout(std::time::Duration::from_secs(60), resp.chunk()).await.map_err(|_| anyhow!(tr("The download stalled")))?? {
        if cancel.load(Ordering::Relaxed) {
            bail!(tr("Download cancelled"));
        }
        hash.update(&chunk);
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        if done > m.bytes {
            bail!(tr!("The {model} model is larger than expected: not kept", model = m.name));
        }
        progress(done);
    }
    file.flush().await?;
    drop(file);
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if done != m.bytes || digest != m.sha256 {
        bail!(tr!("The {model} model does not match its checksum: not kept", model = m.name));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_are_known_by_name() {
        assert_eq!(model("base").map(|m| m.bytes), Some(147_951_465));
        assert!(model("large").is_none());
        let dir = std::env::temp_dir().join(format!("prism-dictation-{}", std::process::id()));
        assert!(list(&dir).iter().all(|m| !m.installed));
        assert_eq!(path(&dir, &MODELS[0]).file_name().unwrap(), "ggml-tiny.bin");
    }
}
