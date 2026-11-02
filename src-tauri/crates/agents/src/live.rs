//! What an agent's CLI shows it doing right now, read off its screen: Claude Code's spinner line
//! (`✻ Frolicking… (3m 40s · ↓ 7.3k tokens)`), Codex's (`• Working (12s • esc to interrupt)`), and
//! the lines a running call shows under it (`⎿ …`). Neither writes these to its transcript.

use serde::Serialize;

use crate::suggestion::visible;

/// The characters Claude Code's and Codex's spinners turn through.
const SPINNER: &[char] = &['·', '✢', '✳', '✶', '✻', '✽', '*', '•', '◦'];
/// A running call's lines kept, its last ones.
const OUTPUT_LINES: usize = 8;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "LiveStatus"))]
pub struct Live {
    /// Its word for it (`Frolicking…`, `Working`).
    pub verb: String,
    /// What the CLI adds: how long, its tokens, `thinking`… (`esc to interrupt` left out).
    pub detail: Vec<String>,
    /// The last lines a running call shows (`⎿`), when its block is the last on screen.
    pub output: Vec<String>,
}

/// The spinner's line on `screen` (a capture, with or without its colours), and the call above it.
pub fn read(screen: &str) -> Option<Live> {
    let lines: Vec<String> = screen.lines().map(visible).collect();
    let (at, mut live) = lines.iter().enumerate().rev().find_map(|(i, l)| status(l).map(|s| (i, s)))?;
    // The call's block above: `⏺ Name(…)` then its lines.
    if let Some(head) = lines[..at].iter().rposition(|l| l.trim_start().starts_with('⏺')) {
        let call = lines[head].trim_start().trim_start_matches('⏺').trim_start();
        if call.split_once('(').is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')) {
            let body: Vec<String> = lines[head + 1..at]
                .iter()
                .map(|l| l.trim().trim_start_matches('⎿').trim().to_string())
                .filter(|l| !l.is_empty() && l != "Running…")
                .collect();
            live.output = body[body.len().saturating_sub(OUTPUT_LINES)..].to_vec();
        }
    }
    Some(live)
}

/// A spinner's line: its glyph, a verb, then what it adds in parentheses, a duration among it.
fn status(line: &str) -> Option<Live> {
    let t = line.trim();
    let rest = t.strip_prefix(|c| SPINNER.contains(&c))?.trim_start();
    let (verb, inside) = rest.split_once(" (")?;
    let inside = inside.strip_suffix(')').or_else(|| inside.rsplit_once(')').map(|(i, _)| i))?;
    let parts: Vec<&str> = inside.split(['·', '•']).map(str::trim).filter(|p| !p.is_empty()).collect();
    let interrupt = parts.iter().any(|p| p.starts_with("esc to interrupt"));
    let timed = parts.iter().any(|p| p.starts_with(|c: char| c.is_ascii_digit()) && p.ends_with(['s', 'm', 'h']));
    let verb = verb.trim();
    if verb.is_empty() || verb.chars().count() > 40 || !timed || !(verb.ends_with('…') || interrupt) {
        return None;
    }
    let detail = parts.into_iter().filter(|p| !p.starts_with("esc to interrupt")).map(String::from).collect();
    Some(Live { verb: verb.to_string(), detail, output: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_claude_codes_spinner_and_the_running_call() {
        let screen = "⏺ Reading the tests.\n\n⏺ Bash(pytest tests/test_retry.py --count=50 -q)\n  ⎿  ..........\n     ....................\n     Running…\n\n\x1b[38;5;174m✻\x1b[0m Frolicking… (3m 40s · ↓ 7.3k tokens · esc to interrupt)\n\n────\n❯ \n────\n";
        let live = read(screen).unwrap();
        assert_eq!(live.verb, "Frolicking…");
        assert_eq!(live.detail, vec!["3m 40s", "↓ 7.3k tokens"]);
        assert_eq!(live.output, vec!["..........", "...................."]);
    }

    #[test]
    fn reads_codexs_and_no_output_after_words() {
        let live = read("⏺ Done reading.\n• Working (12s • esc to interrupt)\n› ").unwrap();
        assert_eq!((live.verb.as_str(), live.detail.clone(), live.output.len()), ("Working", vec!["12s".to_string()], 0));
    }

    #[test]
    fn none_at_the_prompt() {
        assert_eq!(read("⏺ Fixed (see above).\n• Ran the tests (3 files)\n❯ "), None);
        assert_eq!(read("· a list item (2 notes)"), None);
    }
}
