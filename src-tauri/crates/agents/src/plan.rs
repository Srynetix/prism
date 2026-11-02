//! The plan Claude Code proposes in plan mode (its `ExitPlanMode`), read from the transcript (the
//! Chat shows it as a card, commented on), and answered as the keys its menu takes. As Claude Code
//! 2.1.303 does it: the call carries no plan (`{}`), the plan is the file it wrote just before
//! (`~/.claude/plans/<slug>.md`, a `Write` then `Edit`s for each version); approved, the result's
//! `toolUseResult.plan` holds it. Its menu: "Yes, and use auto mode" (or "auto-accept edits"),
//! "Yes, manually approve edits", "Tell Claude what to change": that last one's digit opens a line
//! for your words, Enter sends them (the plan refused, "the user said:\n…"), shift+tab approves the
//! plan with them as its first choice (auto mode: Prism does not use it, approves, then sends them). The line takes one line: a line break
//! pasted is dropped, the words before and after glued.

use anyhow::{bail, Context, Result};
use prism_base::i18n::tr;
use serde::{Deserialize, Serialize};

use crate::question::Step;

/// A plan proposed, and what became of it.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ProposedPlan"))]
pub struct Proposal {
    /// The tool call's id: what an answer names, so it never goes to another plan.
    pub call: String,
    /// The plan, Markdown; empty when its file was written before what Prism read.
    pub text: String,
    /// Its number among the plans of the conversation read, from 1.
    pub version: u32,
    /// `waiting` (its menu open), `approved`, `changes` (refused with your words), `refused`.
    #[cfg_attr(feature = "ts", ts(type = "'waiting' | 'approved' | 'changes' | 'refused'"))]
    pub verdict: &'static str,
    /// Your words with the answer: what to change, or what came with the approval.
    pub feedback: Option<String>,
    /// Its file, read when its writing came before what Prism read.
    #[serde(skip)]
    pub file: Option<String>,
}

/// How a plan is answered.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum How {
    /// Approved: the agent goes on with its edits allowed ("auto-accept edits"); where its menu
    /// offers only auto mode to allow them, each edit asking you: auto mode is never this.
    Approve,
    /// Approved in auto mode (its commands run without asking): asked for by name, at the desk.
    ApproveAuto,
    /// Approved, each edit asking you.
    ApproveEach,
    /// Refused with your words: it plans again.
    Changes,
    /// Approved as `Approve`, your words sent to it next.
    ApproveWith,
}

/// A plan file of Claude Code's: what its `Write` and `Edit` calls make a plan of.
pub fn is_plan_file(path: &str) -> bool {
    path.contains("/.claude/plans/") && path.ends_with(".md")
}

/// Your words in a refused plan's result (`… the user said:\n<words>\n\nNote: …`).
pub fn feedback_of(result: &str) -> Option<String> {
    let (_, words) = result.split_once("the user said:\n")?;
    let words = words.split("\n\nNote: ").next().unwrap_or(words).trim();
    (!words.is_empty()).then(|| words.to_string())
}

/// Words for its menu's line: on one line, a line break a space.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The digits of its menu, read off its screen.
struct Menu {
    /// Approved, its edits allowed ("auto-accept edits"); failing that, each edit asking you.
    approve: char,
    /// Approved, each edit asking you ("manually approve edits").
    each: char,
    /// Approved in auto mode ("use auto mode"), or bypassing its permissions: when offered.
    auto: Option<char>,
    /// Your words: what to change.
    tell: char,
}

fn menu(screen: &str) -> Option<Menu> {
    let lines: Vec<&str> = screen.lines().collect();
    let from = lines.iter().rposition(|l| l.contains("Would you like to proceed"))?;
    let (mut edits, mut each, mut auto, mut tell) = (None, None, None, None);
    for l in &lines[from + 1..] {
        let l = l.trim_start().trim_start_matches('❯').trim_start();
        let Some((digit, label)) = l.split_once(". ") else { continue };
        let Some(d) = digit.chars().next().filter(|c| digit.len() == 1 && c.is_ascii_digit()) else { continue };
        let label = label.to_lowercase();
        if !label.starts_with("yes") {
            // "Tell Claude what to change", or your words typed in it already.
            tell = Some(d);
        } else if label.contains("auto mode") || label.contains("bypass") {
            auto.get_or_insert(d);
        } else if label.contains("manually") {
            each.get_or_insert(d);
        } else {
            edits.get_or_insert(d);
        }
    }
    let each = each?;
    Some(Menu { approve: edits.unwrap_or(each), each, auto, tell: tell? })
}

/// The keys that answer the plan whose menu `screen` shows; `text`: your words, needed to refuse
/// it or to approve with them.
pub fn keys(screen: &str, how: How, text: Option<&str>) -> Result<Vec<Step>> {
    const DIGITS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
    let menu = menu(screen).context(tr("its plan's menu is not on its screen"))?;
    let digit = |d: char| DIGITS[(d as u8 - b'1') as usize];
    let words = text.map(one_line).filter(|t| !t.is_empty());
    Ok(match (how, words) {
        (How::Approve, _) => vec![Step::Key(digit(menu.approve))],
        (How::ApproveEach, _) => vec![Step::Key(digit(menu.each))],
        (How::ApproveAuto, _) => vec![Step::Key(digit(menu.auto.context(tr("its menu offers no auto mode"))?))],
        (How::Changes, Some(w)) => vec![Step::Key(digit(menu.tell)), Step::Paste(w), Step::Key("Enter")],
        // Not shift+tab on the words' line: it approves as the menu's first choice, auto mode.
        (How::ApproveWith, Some(w)) => vec![Step::Key(digit(menu.approve)), Step::Paste(w), Step::Key("Enter")],
        (How::Changes | How::ApproveWith, None) => bail!(tr("no words to send with it")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use Step::{Key, Paste};

    /// Claude Code 2.1.303's screen, its plan above.
    const SCREEN: &str = " Here is Claude's plan:\n Fix bug in calc.py\n 1. Fix the operator in calc.py:2.\n 2. Add a quick test.\n────\n Claude has written up a plan and is ready to execute. Would you like to proceed?\n ❯ 1. Yes, and use auto mode\n   2. Yes, manually approve edits\n   3. Tell Claude what to change\n      shift+tab to approve with this feedback\n ctrl+g to edit in Vim · ~/.claude/plans/plan-a-fix.md\n";

    #[test]
    fn answers_as_its_menu_takes_it() {
        assert_eq!(keys(SCREEN, How::Approve, None).unwrap(), vec![Key("2")], "never auto mode unasked: each edit asking, the plan's own numbered steps not the menu");
        assert_eq!(keys(SCREEN, How::ApproveAuto, None).unwrap(), vec![Key("1")]);
        assert_eq!(keys(SCREEN, How::ApproveEach, Some("ignored")).unwrap(), vec![Key("2")]);
        assert_eq!(keys(SCREEN, How::Changes, Some("Put the test\nin tests/.")).unwrap(), vec![Key("3"), Paste("Put the test in tests/.".into()), Key("Enter")]);
        assert_eq!(keys(SCREEN, How::ApproveWith, Some("Keep it minimal.")).unwrap(), vec![Key("2"), Paste("Keep it minimal.".into()), Key("Enter")]);
        assert!(keys(SCREEN, How::Changes, Some("  ")).is_err(), "nothing to say");
        assert!(keys(" ❯ 1. Yes\n", How::Approve, None).is_err(), "no menu of a plan");
        let older = " Would you like to proceed?\n ❯ 1. Yes, and auto-accept edits\n   2. Yes, manually approve edits\n   3. Tell Claude what to change\n";
        assert_eq!(keys(older, How::Approve, None).unwrap(), vec![Key("1")], "its edits allowed where offered");
        assert!(keys(older, How::ApproveAuto, None).is_err(), "no auto mode offered");
    }

    /// An older menu: four choices, the context cleared first.
    #[test]
    fn reads_another_menu() {
        let screen = "Would you like to proceed?\n ❯ 1. Yes, clear context and auto-accept edits\n   2. Yes, and auto-accept edits\n   3. Yes, manually approve edits\n   4. No, keep planning\n";
        assert_eq!(keys(screen, How::Approve, None).unwrap(), vec![Key("1")]);
        assert_eq!(keys(screen, How::ApproveEach, None).unwrap(), vec![Key("3")]);
        assert_eq!(keys(screen, How::Changes, Some("x")).unwrap()[0], Key("4"));
    }

    #[test]
    fn reads_your_words_in_its_result() {
        let r = "The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). To tell you how to proceed, the user said:\nStep 2: put the test in tests/.\n\nNote: The user's next message may contain a correction or preference.";
        assert_eq!(feedback_of(r).as_deref(), Some("Step 2: put the test in tests/."));
        assert_eq!(feedback_of("The user doesn't want to proceed with this tool use."), None);
        assert!(is_plan_file("/Users/x/.claude/plans/plan-a-fix.md") && !is_plan_file("/Users/x/plans/a.md"));
    }
}
