//! What the core knows of the agents (the `prism-agents` crate runs them): their providers, what
//! they use (context, plan), and how their sessions and worktrees are named.

use std::path::{Path, PathBuf};

pub mod context;
pub mod models;
pub mod provider;
pub mod transcript;
pub mod usage;

/// Prism's own tmux server (`-L prism`) runs the agents: prefix of their sessions, followed by the
/// Claude session identifier.
pub const PREFIX: &str = "prism-";

/// What an agent Prism launches to draft a text (a quick action's prompt, a feedback issue) is
/// launched `from` (`Launch.from`; `quick-action-source.ts` keeps a copy): always out of the lists,
/// never kept as its repository's last session.
pub const DRAFTER: &str = "quick-action-draft";

/// Prism holds the agents' sessions itself (`prism_agents::tmux::local`, in `prism sessions`)
/// instead of a tmux server: on Windows, which has no tmux, or with `PRISM_SESSIONS=local` (to try
/// it elsewhere; `inprocess`: in Prism's process, stopping with it). No tmux is needed then.
pub fn local_sessions() -> bool {
    cfg!(windows) || std::env::var("PRISM_SESSIONS").is_ok_and(|v| v == "local" || v == "inprocess")
}

/// `ABC-4661 · Export CSV des mesures` → `abc-4661-export-csv-des-mesures` (40 characters at most).
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 40 {
            break;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Worktrees folder of a repository: `<parent>/<repository name>.worktrees`.
pub fn worktrees_dir(repo: &Path) -> PathBuf {
    let name = repo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "repo".into());
    repo.with_file_name(format!("{name}.worktrees"))
}
