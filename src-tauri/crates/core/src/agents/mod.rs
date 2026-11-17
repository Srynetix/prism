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
/// Accents are folded (`é`, `ñ`, `å` → `e`, `n`, `a`) and ligatures spelt out (`œ` → `oe`): the rule
/// `slug()` of `web/src/agents.svelte.ts` follows for the launcher's preview.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        // A combining accent (a text written decomposed, `İ` lowered): dropped, its letter stays.
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        let one;
        let chars: &[char] = match c {
            'œ' => &['o', 'e'],
            'æ' => &['a', 'e'],
            'ß' => &['s', 's'],
            c => {
                one = [unaccented(c)];
                &one
            }
        };
        for &c in chars {
            if c.is_ascii_alphanumeric() {
                out.push(c);
            } else if !out.ends_with('-') && !out.is_empty() {
                out.push('-');
            }
        }
        if out.len() >= 40 {
            out.truncate(40);
            break;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// A lower-case Latin letter without its accents (those Unicode decomposes into a letter and
/// accents); anything else as is.
fn unaccented(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' => 'h',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        c => c,
    }
}

/// Worktrees folder of a repository: `<parent>/<repository name>.worktrees`.
pub fn worktrees_dir(repo: &Path) -> PathBuf {
    let name = repo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "repo".into());
    repo.with_file_name(format!("{name}.worktrees"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cases of `slug` in `web/src/agents.test.ts`: both sides name a worktree alike.
    #[test]
    fn slug_folds_every_accent() {
        assert_eq!(slug("ABC-4661 · Export CSV des mesures"), "abc-4661-export-csv-des-mesures");
        assert_eq!(slug("  Révision: état ! "), "revision-etat");
        assert_eq!(slug("Año nuevo"), "ano-nuevo");
        assert_eq!(slug("Århus"), "arhus");
        assert_eq!(slug("Œuvre complète"), "oeuvre-complete");
        assert_eq!(slug("Straße"), "strasse");
        assert_eq!(slug("e\u{301}te\u{301}"), "ete");
        assert_eq!(slug("!!!"), "");
        assert_eq!(slug(&"é".repeat(100)), "e".repeat(40));
    }
}
