//! Prism's logs, kept on disk for troubleshooting: an app opened from the Finder or a desktop
//! launcher has no terminal, and its standard error goes nowhere. Each event goes to standard error
//! and to the active profile's `prism.log`, rotated at 5 MB (the previous one kept as
//! `prism.log.1`). The level is `[logs] level` (applied as it is saved), `RUST_LOG` taking precedence.
//! Nothing secret is logged: the collectors never log a token, a body nor a message. The warnings
//! and errors are also kept to be shown in the window (`problems`).

use prism_base::tr;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::{Context, Result};
use tracing_subscriber::{fmt::MakeWriter, layer::SubscriberExt, reload, util::SubscriberInitExt, EnvFilter, Registry};

/// A log beyond this size is rotated.
const MAX_BYTES: u64 = 5 * 1024 * 1024;
const LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

/// The active profile's log folder: `~/Library/Logs/app.prism.desktop` on macOS (Console.app shows
/// it), the state folder elsewhere (`~/.local/state/app.prism.desktop` on Linux, the local data
/// folder on Windows); a profile other than the default one in `profiles/<id>` inside.
pub fn dir() -> PathBuf {
    let base = if cfg!(target_os = "macos") {
        dirs::home_dir().unwrap_or_default().join("Library/Logs")
    } else {
        dirs::state_dir().or_else(dirs::data_local_dir).unwrap_or_else(std::env::temp_dir)
    };
    crate::profile::data_dir(&base.join(crate::profile::IDENTIFIER))
}

pub fn path() -> PathBuf {
    dir().join("prism.log")
}

/// Logs to standard error and to the profile's file, at `[logs] level` of `config` (read here, before
/// anything else: the config proper is loaded later, and logs).
pub fn init(config: &Path) {
    let (filter, handle) = reload::Layer::new(filter_of(config));
    let _ = FILTER.set(handle);
    let stderr = tracing_subscriber::fmt::layer();
    match LogFile::open(path()) {
        Ok(file) => {
            let file = tracing_subscriber::fmt::layer().with_ansi(false).with_writer(file);
            tracing_subscriber::registry().with(filter).with(stderr).with(file).with(crate::problems::Keep).init();
        }
        Err(e) => {
            tracing_subscriber::registry().with(filter).with(stderr).with(crate::problems::Keep).init();
            tracing::warn!("logs: no log file: {e:#}");
        }
    }
}

/// The filter of both outputs, set again when the config changes (`apply`).
static FILTER: std::sync::OnceLock<reload::Handle<EnvFilter, Registry>> = std::sync::OnceLock::new();

fn filter_of(config: &Path) -> EnvFilter {
    EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(|_| format!("prism={}", level_of(config))))
}

/// `[logs] level` of `config` as it now reads (a save, a change by hand).
pub fn apply(config: &Path) {
    if let Some(handle) = FILTER.get()
        && let Err(e) = handle.reload(filter_of(config))
    {
        tracing::warn!("logs: level not changed: {e}");
    }
}

/// `[logs] level` of the config file, `info` when absent or unknown. A pack does not set it.
fn level_of(config: &Path) -> &'static str {
    let wanted = std::fs::read_to_string(config)
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .and_then(|t| t.get("logs")?.get("level")?.as_str().map(|l| l.trim().to_ascii_lowercase()));
    wanted.and_then(|w| LEVELS.into_iter().find(|l| *l == w)).unwrap_or("info")
}

/// The log file, appended to, rotated past `MAX_BYTES`.
struct LogFile {
    path: PathBuf,
    file: Mutex<(File, u64)>,
}

impl LogFile {
    fn open(path: PathBuf) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path).with_context(|| format!("opening {}", path.display()))?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self { path, file: Mutex::new((file, size)) })
    }

    fn write(&self, buf: &[u8]) -> std::io::Result<usize> {
        let mut guard = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if guard.1 + buf.len() as u64 > MAX_BYTES {
            // The previous log replaces the one before; a failed rotation keeps writing to this one.
            let old = self.path.with_extension("log.1");
            if std::fs::rename(&self.path, &old).is_ok()
                && let Ok(fresh) = OpenOptions::new().create(true).append(true).open(&self.path)
            {
                *guard = (fresh, 0);
            }
        }
        guard.0.write_all(buf)?;
        guard.1 += buf.len() as u64;
        Ok(buf.len())
    }
}

/// One event's writer: the formatter writes each event in one piece.
pub struct LogWriter<'a>(&'a LogFile);

impl Write for LogWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for LogFile {
    type Writer = LogWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        LogWriter(self)
    }
}

/// The last `lines` lines of the log, the rotated one before it when the current one is short.
pub fn tail(lines: usize) -> Result<String> {
    let current = std::fs::read_to_string(path()).unwrap_or_default();
    let mut all: Vec<&str> = current.lines().collect();
    let previous;
    if all.len() < lines {
        previous = std::fs::read_to_string(path().with_extension("log.1")).unwrap_or_default();
        let mut older: Vec<&str> = previous.lines().collect();
        older.append(&mut all);
        all = older;
    }
    anyhow::ensure!(!all.is_empty(), tr!("the log is empty: {path}", path = path().display()));
    Ok(last(&all, lines).join("\n"))
}

fn last<'a>(all: &'a [&'a str], n: usize) -> &'a [&'a str] {
    &all[all.len().saturating_sub(n)..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_past_its_size() {
        let dir = std::env::temp_dir().join(format!("prism-logs-{}", uuid::Uuid::new_v4().simple()));
        let log = LogFile::open(dir.join("prism.log")).unwrap();
        let line = vec![b'x'; 1024 * 1024];
        for _ in 0..6 {
            log.write(&line).unwrap();
        }
        assert!(dir.join("prism.log.1").is_file(), "rotated");
        assert_eq!(std::fs::metadata(dir.join("prism.log")).unwrap().len(), 1024 * 1024);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn reads_the_level_of_the_config() {
        let dir = std::env::temp_dir().join(format!("prism-level-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("config.toml");
        std::fs::write(&cfg, "[logs]\nlevel = \"Debug\"\n").unwrap();
        assert_eq!(level_of(&cfg), "debug");
        std::fs::write(&cfg, "[logs]\nlevel = \"verbose\"\n").unwrap();
        assert_eq!(level_of(&cfg), "info");
        assert_eq!(level_of(&dir.join("absent.toml")), "info");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn keeps_the_last_lines() {
        assert_eq!(last(&["a", "b", "c"], 2), ["b", "c"]);
        assert_eq!(last(&["a"], 5), ["a"]);
    }
}
