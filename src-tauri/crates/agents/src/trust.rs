//! What a CLI asks on its screen before its session starts, while no hook can say it (none runs
//! until it is answered): Claude Code's trust of the folder, Codex's trust of the folder and its
//! approval of Prism's hooks. Read off the agent's screen (`capture-pane`, as `question.rs`), shown
//! as its gate (`Agent::gate`), answered with its menu's keys — only a screen known for sure, and
//! still on its screen when the keys go. What is not known for sure is shown, never answered: its
//! terminal does.
//!
//! Claude Code 2.1.319, a folder never trusted: « Accessing workspace: <folder> … Quick safety
//! check: Is this a project you created or one you trust? … ❯ No, exit / Yes, I trust this folder
//! … Enter to confirm · Esc to cancel » — no digits, « No, exit » picked first. Earlier versions:
//! « Do you trust the files in this folder? … 1. Yes, proceed / 2. No, exit ».

use anyhow::{bail, Result};
use prism_base::i18n::tr;

use crate::{provider::Provider, state::{AgentGate, GateKind}};

/// The screen's text, its colours out, a line per row.
fn lines(screen: &str) -> Vec<String> {
    screen.lines().map(crate::suggestion::visible).collect()
}

/// A line of a menu: its marker (`❯`, `›`) cut, `1.` cut, trimmed.
fn label(line: &str) -> &str {
    let l = line.trim().trim_start_matches(['❯', '›', '>']).trim_start();
    match l.split_once(". ") {
        Some((n, rest)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => rest.trim(),
        _ => l,
    }
}

/// The line the menu's cursor is on.
fn picked(line: &str) -> bool {
    let l = line.trim_start();
    l.starts_with('❯') || l.starts_with('›')
}

const CLAUDE_YES: [&str; 2] = ["Yes, I trust this folder", "Yes, proceed"];
const CLAUDE_NO: [&str; 2] = ["No, exit", "No, exit"];

/// The folder Claude Code names: the lines after « Accessing workspace: » up to a blank one (a
/// long path wraps), or the line after « Do you trust the files in this folder? ».
fn claude_folder(lines: &[String]) -> Option<String> {
    let start = lines.iter().position(|l| l.contains("Accessing workspace:") || l.contains("Do you trust the files in this folder?"))?;
    let path: String = lines[start + 1..].iter().map(|l| l.trim().trim_matches('│').trim()).skip_while(|l| l.is_empty()).take_while(|l| !l.is_empty()).collect();
    (!path.is_empty()).then_some(path)
}

/// What `provider` asks on `screen` before it starts; `None`: nothing of the kind.
pub fn read(provider: Provider, screen: &str) -> Option<AgentGate> {
    let lines = lines(screen);
    let text = lines.join("\n");
    match provider {
        Provider::Claude => {
            let asks = (text.contains("Accessing workspace:") || text.contains("Do you trust the files in this folder?")) && lines.iter().any(|l| CLAUDE_YES.contains(&label(l)));
            asks.then(|| AgentGate { kind: GateKind::Folder, folder: claude_folder(&lines), answerable: claude_keys(&lines, true).is_ok() && claude_keys(&lines, false).is_ok(), parent: None })
        }
        Provider::Codex => {
            // Its words not read for sure yet: shown, answered in its terminal.
            let low = text.to_lowercase();
            let menu = lines.iter().any(|l| picked(l) && l.trim_start().trim_start_matches(['❯', '›']).trim_start().starts_with(|c: char| c.is_ascii_digit()));
            if !menu {
                return None;
            }
            let kind = if low.contains("hook") && (low.contains("approve") || low.contains("trust") || low.contains("review")) {
                GateKind::Hooks
            } else if low.contains("trust") && (low.contains("folder") || low.contains("directory")) {
                GateKind::Folder
            } else if low.contains("update available") {
                GateKind::Other
            } else {
                return None;
            };
            Some(AgentGate { kind, folder: None, answerable: false, parent: None })
        }
        Provider::CommandCode | Provider::OpenCode => None,
    }
}

/// Claude Code's menu: the arrows from the line picked to the one wanted, then Enter.
fn claude_keys(lines: &[String], yes: bool) -> Result<Vec<&'static str>> {
    let options: Vec<&String> = lines.iter().filter(|l| CLAUDE_YES.contains(&label(l)) || CLAUDE_NO.contains(&label(l))).collect();
    let want = options.iter().position(|l| if yes { CLAUDE_YES.contains(&label(l)) } else { CLAUDE_NO.contains(&label(l)) });
    let at = options.iter().position(|l| picked(l));
    let (Some(want), Some(at)) = (want, at) else { bail!(tr("its question is not one Prism knows: answer it in its terminal")) };
    if options.len() != 2 {
        bail!(tr("its question is not one Prism knows: answer it in its terminal"));
    }
    let arrow = if want > at { "Down" } else { "Up" };
    let mut keys = vec![arrow; want.abs_diff(at)];
    keys.push("Enter");
    Ok(keys)
}

/// The keys answering the gate on `screen` (`yes`: trust, approve), only if it is one Prism knows
/// for sure and still the one shown as `kind`.
pub fn keys(provider: Provider, screen: &str, kind: GateKind, yes: bool) -> Result<Vec<&'static str>> {
    let Some(gate) = read(provider, screen) else { bail!(tr("its question is no longer on its screen")) };
    if gate.kind != kind || !gate.answerable {
        bail!(tr("its question is not one Prism knows: answer it in its terminal"));
    }
    match provider {
        Provider::Claude => claude_keys(&lines(screen), yes),
        _ => bail!(tr("its question is not one Prism knows: answer it in its terminal")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude Code 2.1.319's, as captured (a long path wrapped).
    const CLAUDE: &str = "────────────────────────────────────────\n Accessing workspace:\n\n /private/tmp/claude-502/-Users-denis/\n scratchpad/trust-probe/claude\n\n Quick safety check: Is this a project you created or one you trust? (Like your own code, a\n well-known open source project, or work from your team).\n\n Claude Code'll be able to read, edit, and execute files here.\n\n Security guide\n\n ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · Esc to cancel\n";

    #[test]
    fn reads_claude_codes_trust_of_its_folder() {
        let g = read(Provider::Claude, CLAUDE).unwrap();
        assert_eq!((g.kind, g.answerable), (GateKind::Folder, true));
        assert_eq!(g.folder.as_deref(), Some("/private/tmp/claude-502/-Users-denis/scratchpad/trust-probe/claude"));
        assert_eq!(read(Provider::Claude, "❯ Try \"fix the tests\"\n  ? for shortcuts"), None);
    }

    #[test]
    fn answers_it_with_the_arrows_from_the_line_picked() {
        assert_eq!(keys(Provider::Claude, CLAUDE, GateKind::Folder, true).unwrap(), ["Down", "Enter"]);
        assert_eq!(keys(Provider::Claude, CLAUDE, GateKind::Folder, false).unwrap(), ["Enter"]);
        let moved = CLAUDE.replace(" ❯ No, exit\n   Yes, I trust", "   No, exit\n ❯ Yes, I trust");
        assert_eq!(keys(Provider::Claude, &moved, GateKind::Folder, true).unwrap(), ["Enter"]);
        assert_eq!(keys(Provider::Claude, &moved, GateKind::Folder, false).unwrap(), ["Up", "Enter"]);
        let older = "Do you trust the files in this folder?\n\n/Users/me/app\n\n❯ 1. Yes, proceed\n  2. No, exit\n";
        assert_eq!(keys(Provider::Claude, older, GateKind::Folder, true).unwrap(), ["Enter"]);
        assert_eq!(read(Provider::Claude, older).unwrap().folder.as_deref(), Some("/Users/me/app"));
    }

    #[test]
    fn sends_nothing_once_it_is_gone_or_unknown() {
        assert!(keys(Provider::Claude, "❯ \n  ? for shortcuts", GateKind::Folder, true).is_err());
        // Its cursor not found: not answered blind.
        assert!(keys(Provider::Claude, &CLAUDE.replace('❯', " "), GateKind::Folder, true).is_err());
        assert!(!read(Provider::Claude, &CLAUDE.replace('❯', " ")).unwrap().answerable);
    }

    #[test]
    fn codex_is_shown_never_answered() {
        let update = "  Update available · 0.166.0 → 0.167.0\n\n› 1. Update now (runs `brew upgrade --cask codex`)\n  2. Skip\n  3. Skip until next version\n\n  enter continue · esc skip\n";
        let g = read(Provider::Codex, update).unwrap();
        assert_eq!((g.kind, g.answerable), (GateKind::Other, false));
        let folder = "Do you trust the contents of this directory?\n› 1. Yes, continue\n  2. No, quit\n";
        assert_eq!(read(Provider::Codex, folder).unwrap().kind, GateKind::Folder);
        let hooks = "Review and approve the hooks this session will run\n› 1. Approve\n  2. Not now\n";
        assert_eq!(read(Provider::Codex, hooks).unwrap().kind, GateKind::Hooks);
        assert!(keys(Provider::Codex, folder, GateKind::Folder, true).is_err());
        assert_eq!(read(Provider::Codex, "› Ask Codex to do anything\n  ? for shortcuts"), None);
    }
}
