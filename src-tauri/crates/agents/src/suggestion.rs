//! The next prompt Claude Code suggests after a turn: dimmed text in its empty input
//! (`❯ run it`), read off the agent's screen. Not in its transcript nor its hooks: the screen is
//! the only place it is. Sent as any prompt would be, it is the same as accepting it there.

/// The suggestion on `screen` (a capture with its colours, `capture-pane -e`): the text after
/// the last `❯` when it is all dimmed (typed text is not); `None` without one. Claude Code's
/// first-run placeholder (`Try "…"`) is not a suggestion.
pub fn read(screen: &str) -> Option<String> {
    let line = screen.lines().rev().find(|l| visible(l).trim_start().starts_with('❯'))?;
    let after = &line[line.find('❯')? + '❯'.len_utf8()..];
    let mut dim = false;
    let mut text = String::new();
    let mut chars = after.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // CSI: ESC [ params final; only SGR (`m`) changes the dimming.
            if chars.next_if_eq(&'[').is_none() {
                chars.next();
                continue;
            }
            let mut params = String::new();
            let mut fin = 'm';
            for p in chars.by_ref() {
                if ('@'..='~').contains(&p) {
                    fin = p;
                    break;
                }
                params.push(p);
            }
            if fin == 'm' {
                sgr(&params, &mut dim);
            }
        } else if !c.is_whitespace() && !dim {
            return None;
        } else {
            text.push(c);
        }
    }
    let text = text.trim();
    (!text.is_empty() && !text.starts_with("Try \"")).then(|| text.chars().take(500).collect())
}

/// Applies an SGR's parameters to the dimming: 2 on, 22 or a reset off; colours skipped whole.
fn sgr(params: &str, dim: &mut bool) {
    let mut it = params.split(';');
    while let Some(p) = it.next() {
        match p {
            "" | "0" | "22" => *dim = false,
            "2" => *dim = true,
            "38" | "48" | "58" => match it.next() {
                Some("5") => {
                    it.next();
                }
                Some("2") => {
                    it.nth(2);
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// A line without its escape sequences.
pub(crate) fn visible(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() == Some('[') {
                for p in chars.by_ref() {
                    if ('@'..='~').contains(&p) {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: &str = "\x1b[38;2;136;136;136m────────────\x1b[39m";

    #[test]
    fn reads_the_dimmed_suggestion_of_claude_code_2_1() {
        // As captured from Claude Code 2.1.302 after a turn.
        let screen = format!("\x1b[38;2;153;153;153m\x1b[48;2;55;55;55m❯ \x1b[38;2;255;255;255mCreate a file\x1b[39m\n⏺ Done.\n{RULE}\n\x1b[39m❯ \x1b[2mrun it\x1b[0m\n{RULE}\n  -- INSERT --");
        assert_eq!(read(&screen).as_deref(), Some("run it"));
    }

    #[test]
    fn nothing_typed_nor_placeholder_nor_menu() {
        assert_eq!(read(&format!("{RULE}\n\x1b[39m❯ \n{RULE}")), None, "empty input");
        assert_eq!(read("\x1b[39m❯ \x1b[2mrun\x1b[0m it"), None, "partly typed");
        assert_eq!(read("\x1b[39m❯ run it"), None, "typed");
        assert_eq!(read("❯ \x1b[2mTry \"how does main.rs work?\"\x1b[0m"), None, "first-run placeholder");
        assert_eq!(read(" ❯ 1. \x1b[38;5;153mYes\x1b[0m\n   2. No"), None, "a menu's choice");
        assert_eq!(read("no prompt here"), None);
    }

    #[test]
    fn colours_do_not_read_as_dim() {
        // 38;2;2;2;2: a colour whose components are 2, not the dim attribute.
        assert_eq!(read("❯ \x1b[38;2;2;2;2mrun it\x1b[0m"), None);
        assert_eq!(read("❯ \x1b[2;38;5;244mrun it\x1b[22m "), Some("run it".into()));
    }
}
