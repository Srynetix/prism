//! A question Claude Code asks with its `AskUserQuestion` tool: read from the transcript (the
//! phone's Chat shows it as a card), and answered as the keys its menu takes, so a phone answers
//! it without the terminal. The menu, as Claude Code 2.1 draws it: one tab per question, then a
//! review tab ("Submit answers") when there are several questions or one picks several choices.
//! A choice's digit picks it and moves on (one choice) or ticks it (several); the line after the
//! choices takes your own words ("Type something"), the one after it moves on. One choice takes a
//! choice or your words: both go as words, the choice's label first. Command Code's menu (1.88) is
//! the same but for two things: no digit reaches the line for your words (the arrows do), and its
//! review tab is always there, one question picked at once included.

use anyhow::{bail, ensure, Result};
use prism_base::tr;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A question asked, its choices, and the answer it got once answered.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentQuestion"))]
pub struct Ask {
    /// The tool call's id: what an answer names, so it never goes to another question.
    pub call: String,
    pub questions: Vec<Question>,
    /// Once answered, each question's answer as Claude Code wrote it (`Apple, Kiwi`).
    pub answers: Vec<Option<String>>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "QuestionAsked"))]
pub struct Question {
    pub header: String,
    pub question: String,
    /// Several choices may be picked.
    pub multi: bool,
    pub options: Vec<Choice>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "QuestionChoice"))]
pub struct Choice {
    pub label: String,
    pub description: String,
}

/// An answer to one question: the choices picked (from 1), and your own words.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Answer {
    #[serde(default)]
    pub picks: Vec<u32>,
    #[serde(default)]
    pub text: Option<String>,
}

/// Whose menu the question is in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Menu {
    Claude,
    CommandCode,
}

/// A step of the answer: a key pressed, or a text pasted.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Key(&'static str),
    Paste(String),
}

/// The tool call's question, `None` for anything else.
pub fn parse(call: &str, input: &Value) -> Option<Ask> {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let questions: Vec<Question> = input
        .get("questions")?
        .as_array()?
        .iter()
        .map(|q| Question {
            header: text(q, "header"),
            question: text(q, "question"),
            multi: q.get("multiSelect").and_then(Value::as_bool) == Some(true),
            options: q.get("options").and_then(Value::as_array).into_iter().flatten().map(|o| Choice { label: text(o, "label"), description: text(o, "description") }).collect(),
        })
        .collect();
    (!questions.is_empty()).then(|| Ask { call: call.to_string(), answers: vec![None; questions.len()], questions })
}

impl Ask {
    /// The answers Claude Code recorded (`toolUseResult.answers`, by question).
    pub fn answered(&mut self, answers: &Value) {
        for (q, a) in self.questions.iter().zip(self.answers.iter_mut()) {
            *a = answers.get(&q.question).and_then(Value::as_str).map(String::from);
        }
    }

    /// The answers Command Code gave its model, a line per question: `Which color? → Blue`.
    pub fn answered_text(&mut self, text: &str) {
        for (q, a) in self.questions.iter().zip(self.answers.iter_mut()) {
            let lead = format!("{} → ", q.question);
            *a = text.lines().find_map(|l| l.strip_prefix(&lead)).map(|t| t.trim().to_string());
        }
    }

    /// The keys that give these answers, checked against the question: one answer per question,
    /// one choice or your words for a single choice, choices that exist.
    pub fn keys(&self, answers: &[Answer], menu: Menu) -> Result<Vec<Step>> {
        const DIGITS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
        ensure!(answers.len() == self.questions.len(), "{} answers for {} questions", answers.len(), self.questions.len());
        let mut steps = Vec::new();
        for (q, a) in self.questions.iter().zip(answers) {
            let n = q.options.len();
            // Down to the line for your words, from the first.
            let words = |steps: &mut Vec<Step>| match menu {
                Menu::Claude => steps.push(Step::Key(DIGITS[n])),
                Menu::CommandCode => steps.extend(std::iter::repeat_n(Step::Key("Down"), n)),
            };
            // The choices and the line for your words, each a digit.
            ensure!(n < DIGITS.len(), "too many choices");
            let text = a.text.as_deref().map(str::trim).filter(|t| !t.is_empty());
            let mut picks = a.picks.clone();
            picks.sort_unstable();
            picks.dedup();
            ensure!(picks.iter().all(|p| (1..=n as u32).contains(p)), "no such choice");
            if q.multi {
                ensure!(!picks.is_empty() || text.is_some(), "“{}”: nothing picked", q.header);
                steps.extend(picks.iter().map(|p| Step::Key(DIGITS[*p as usize - 1])));
                // The cursor is still on the first line: down to the line for your words, then to
                // the one that moves on.
                steps.extend(std::iter::repeat_n(Step::Key("Down"), n));
                if let Some(t) = text {
                    steps.push(Step::Paste(t.to_string()));
                }
                steps.extend([Step::Key("Down"), Step::Key("Enter")]);
            } else {
                // Its menu takes a choice or your words, never both: a choice with your words goes
                // as words, the choice's label first.
                match (picks.as_slice(), text) {
                    ([p], None) => steps.push(Step::Key(DIGITS[*p as usize - 1])),
                    ([], Some(t)) => {
                        words(&mut steps);
                        steps.extend([Step::Paste(t.to_string()), Step::Key("Enter")]);
                    }
                    ([p], Some(t)) => {
                        let label = &q.options[*p as usize - 1].label;
                        words(&mut steps);
                        steps.extend([Step::Paste(format!("{label} — {t}")), Step::Key("Enter")]);
                    }
                    _ => bail!(tr!("“{header}”: one choice", header = q.header)),
                }
            }
        }
        if menu == Menu::CommandCode || self.questions.len() > 1 || self.questions.iter().any(|q| q.multi) {
            steps.push(Step::Key("1"));
        }
        Ok(steps)
    }

    /// Its menu is on `screen` (`capture-pane -e`): a question's tab ("Enter to select") or the
    /// review one ("Ready to submit", Command Code's "← to go back and edit"), one of its questions
    /// written on it. Before the keys, so they go to its menu only; after them, the answer was not
    /// taken while it still shows.
    pub fn on_screen(&self, screen: &str) -> bool {
        // Words alone: a question wrapped on the screen's width is the same.
        let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let screen = words(&screen.lines().map(crate::suggestion::visible).collect::<Vec<_>>().join("\n"));
        let menu = screen.contains("Enter to select") || screen.contains("Ready to submit") || screen.contains("to go back and edit");
        menu && self.questions.iter().any(|q| {
            let q = words(&q.question);
            !q.is_empty() && screen.contains(&q.chars().take(40).collect::<String>())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ask() -> Ask {
        parse(
            "toolu_1",
            &json!({ "questions": [
                { "question": "Which color?", "header": "Color", "multiSelect": false, "options": [{ "label": "Red", "description": "Warm" }, { "label": "Blue", "description": "Cool" }] },
                { "question": "Which fruits?", "header": "Fruits", "multiSelect": true, "options": [{ "label": "Apple" }, { "label": "Pear" }, { "label": "Kiwi" }] },
            ] }),
        )
        .unwrap()
    }

    fn pick(picks: &[u32], text: Option<&str>) -> Answer {
        Answer { picks: picks.to_vec(), text: text.map(String::from) }
    }

    #[test]
    fn reads_the_question_and_its_answers() {
        let mut a = ask();
        assert_eq!((a.questions[0].options[1].label.as_str(), a.questions[1].multi, a.answers.len()), ("Blue", true, 2));
        a.answered(&json!({ "Which color?": "Red", "Which fruits?": "Apple, Kiwi" }));
        assert_eq!(a.answers, vec![Some("Red".into()), Some("Apple, Kiwi".into())]);
        assert!(parse("x", &json!({ "command": "ls" })).is_none());
    }

    /// The keys tried on Claude Code 2.1.302's menu.
    #[test]
    fn answers_as_the_menu_takes_them() {
        use Step::{Key, Paste};
        let steps = ask().keys(&[pick(&[2], None), pick(&[3, 1], Some("Mango"))], Menu::Claude).unwrap();
        assert_eq!(steps, vec![Key("2"), Key("1"), Key("3"), Key("Down"), Key("Down"), Key("Down"), Paste("Mango".into()), Key("Down"), Key("Enter"), Key("1")]);
        let one = Ask { questions: vec![ask().questions[0].clone()], ..ask() };
        assert_eq!(one.keys(&[pick(&[1], None)], Menu::Claude).unwrap(), vec![Key("1")], "one question, one choice: its digit sends it");
        assert_eq!(one.keys(&[pick(&[], Some("Green"))], Menu::Claude).unwrap(), vec![Key("3"), Paste("Green".into()), Key("Enter")]);
        assert_eq!(one.keys(&[pick(&[1], Some("but darker"))], Menu::Claude).unwrap(), vec![Key("3"), Paste("Red — but darker".into()), Key("Enter")], "a choice and your words: both, as words");
    }

    /// The keys tried on Command Code 1.88's menu: the arrows to your words, its review tab always.
    #[test]
    fn answers_as_command_codes_menu_takes_them() {
        use Step::{Key, Paste};
        let steps = ask().keys(&[pick(&[], Some("Green")), pick(&[1, 3], Some("Mango"))], Menu::CommandCode).unwrap();
        assert_eq!(steps, vec![Key("Down"), Key("Down"), Paste("Green".into()), Key("Enter"), Key("1"), Key("3"), Key("Down"), Key("Down"), Key("Down"), Paste("Mango".into()), Key("Down"), Key("Enter"), Key("1")]);
        let one = Ask { questions: vec![ask().questions[0].clone()], ..ask() };
        assert_eq!(one.keys(&[pick(&[2], None)], Menu::CommandCode).unwrap(), vec![Key("2"), Key("1")], "one question: its review tab too");
        let mut a = ask();
        a.answered_text("Which color? → Blue\nWhich fruits? → Apple, Kiwi, Mango\n\nContinue with the user's answers in mind.");
        assert_eq!(a.answers, vec![Some("Blue".into()), Some("Apple, Kiwi, Mango".into())]);
    }

    /// Screens of Claude Code 2.1.306's menu, its colors as `capture-pane -e` gives them.
    #[test]
    fn sees_its_menu_on_the_screen() {
        let a = ask();
        let tab = "←  ☐ Color  ☐ Fruits  ✔ Submit  →\n\x1b[1mWhich \x1b[0mcolor?\n\x1b[38;5;153m❯ 1. Red\x1b[0m\n  2. Blue\n  3. Type something.\nEnter to select · Tab/Arrow keys to navigate · Esc to cancel";
        assert!(a.on_screen(tab));
        assert!(a.on_screen("✔ Color | ✔ Fruits | ● Review\n\n1. Which color?\n   Blue\n\n❯ 1. Submit\n  2. Cancel\n\n← to go back and edit"), "Command Code 1.88's review tab");
        assert!(a.on_screen("Review your answers\n ● Which\n   color?\n   → Red\nReady to submit your answers?\n❯ 1. Submit answers"), "the review tab, a question wrapped");
        assert!(!a.on_screen("⏺ User answered Claude's questions:\n  ⎿  · Which color? → Red\n❯ \n  -- INSERT --"), "answered: its menu gone");
        assert!(!a.on_screen("Which pet?\n❯ 1. Cat\nEnter to select"), "another question's menu");
    }

    #[test]
    fn refuses_what_the_question_does_not_offer() {
        let a = ask();
        assert!(a.keys(&[pick(&[1], None)], Menu::Claude).is_err(), "an answer per question");
        assert!(a.keys(&[pick(&[3], None), pick(&[1], None)], Menu::Claude).is_err(), "no third color");
        assert!(a.keys(&[pick(&[1, 2], None), pick(&[1], None)], Menu::Claude).is_err(), "one color");
        assert!(a.keys(&[pick(&[1], None), pick(&[], Some("  "))], Menu::Claude).is_err(), "some fruit");
    }
}
