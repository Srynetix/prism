//! Edits of the vault's Markdown, as text in and text out: a board's lanes and cards located line
//! by line (the reading uses the same layout), a note's frontmatter and comments. Everything else
//! in the files stays byte for byte as it was.

use prism_base::{i18n::tr, tr};
use std::sync::LazyLock;

use anyhow::{bail, Context, Result};
use regex::Regex;

use super::status_of;

/// Where a board's lanes and cards are, by line index.
#[derive(Debug)]
pub(super) struct LaneAt {
    pub name: String,
    pub heading: usize,
    /// The `**Complete**` line, for a lane marked complete.
    pub complete: Option<usize>,
    pub cards: Vec<CardAt>,
}

#[derive(Debug)]
pub(super) struct CardAt {
    /// Its `- [ ]` line; its indented lines follow, up to `end` (excluded).
    pub line: usize,
    pub end: usize,
    pub checked: bool,
    pub text: String,
}

static CARD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^- \[(.)\] (.*)$").unwrap());

/// The lanes of a board's lines, up to the archive (`***`) or the plugin's settings.
pub(super) fn layout(lines: &[&str]) -> Vec<LaneAt> {
    let mut lanes: Vec<LaneAt> = Vec::new();
    let mut i = 0;
    // Frontmatter (`kanban-plugin: board`).
    if lines.first().is_some_and(|l| l.trim() == "---") {
        i = lines.iter().skip(1).position(|l| l.trim() == "---").map_or(lines.len(), |p| p + 2);
    }
    while i < lines.len() {
        let line = lines[i];
        if line.trim() == "***" || line.starts_with("%% kanban:settings") {
            break;
        }
        if let Some(name) = line.strip_prefix("## ") {
            lanes.push(LaneAt { name: name.trim().to_string(), heading: i, complete: None, cards: Vec::new() });
        } else if line.trim() == "**Complete**" {
            if let Some(lane) = lanes.last_mut() {
                lane.complete = Some(i);
            }
        } else if let Some(c) = CARD.captures(line)
            && let Some(lane) = lanes.last_mut()
        {
            lane.cards.push(CardAt { line: i, end: i + 1, checked: &c[1] != " ", text: c[2].trim().to_string() });
        } else if line.starts_with([' ', '\t'])
            && let Some(card) = lanes.last_mut().and_then(|l| l.cards.last_mut())
        {
            card.end = i + 1;
        }
        i += 1;
    }
    lanes
}

/// A card's indented lines, unindented.
pub(super) fn body(lines: &[&str], card: &CardAt) -> String {
    lines[card.line + 1..card.end]
        .iter()
        .map(|l| l.strip_prefix('\t').unwrap_or_else(|| l.trim_start()).trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// Where a card was read: its first line, its lane's status, and its rank among the cards of that
/// lane with the same line (two cards can read the same).
#[derive(Debug, Clone, Copy)]
pub(super) struct Spot<'a> {
    pub text: &'a str,
    pub status: &'a str,
    pub nth: usize,
}

/// The card at `spot`: in its lane, the `nth` with its line; failing that (it moved), the first
/// card of the board with that line.
fn locate<'a>(lanes: &'a [LaneAt], spot: Spot) -> Result<&'a CardAt> {
    let in_lane = lanes.iter().filter(|l| status_of(&l.name) == spot.status).flat_map(|l| &l.cards).filter(|c| c.text == spot.text).nth(spot.nth);
    in_lane
        .or_else(|| lanes.iter().flat_map(|l| &l.cards).find(|c| c.text == spot.text))
        .context(tr("the card is no longer on its board: it changed in Obsidian meanwhile"))
}

fn split(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

/// Moves a card (and its indented lines) to the end of the lane in status `to`, checked as that
/// lane's cards are: in a lane marked complete, or one whose cards all are; unchecked otherwise.
pub(super) fn move_card(text: &str, spot: Spot, to: &str) -> Result<String> {
    let lines = split(text);
    let lanes = layout(&lines);
    let source = locate(&lanes, spot)?;
    let target = lanes.iter().find(|l| status_of(&l.name) == to).with_context(|| tr!("no lane \"{lane}\" on this board", lane = to))?;
    let done = target.complete.is_some() || (!target.cards.is_empty() && target.cards.iter().all(|c| c.checked || c.line == source.line));
    let mut block: Vec<String> = lines[source.line..source.end].iter().map(|l| l.to_string()).collect();
    block[0] = format!("- [{}] {}", if done { "x" } else { " " }, source.text);
    if target.cards.iter().any(|c| c.line == source.line) {
        // Same lane: only the checkbox may change.
        let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        out[source.line] = block[0].clone();
        return Ok(out.join("\n"));
    }
    // After its last card; in an empty lane, after its heading (and its blank line, its mark).
    let at = match target.cards.last() {
        Some(last) => last.end,
        None => {
            let after = target.complete.unwrap_or(target.heading) + 1;
            if lines.get(after).is_some_and(|l| l.trim().is_empty()) { after + 1 } else { after }
        }
    };
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    let len = source.end - source.line;
    out.drain(source.line..source.end);
    let at = if at > source.line { at - len } else { at };
    let next_is_heading = out.get(at).is_some_and(|l| l.starts_with("## "));
    if next_is_heading {
        block.push(String::new());
    }
    out.splice(at..at, block);
    Ok(out.join("\n"))
}

/// Adds a card at the end of the lane in status `to`, unchecked.
pub(super) fn add_card(text: &str, to: &str, card: &str) -> Result<String> {
    let lines = split(text);
    let lanes = layout(&lines);
    let target = lanes.iter().find(|l| status_of(&l.name) == to).with_context(|| tr!("no lane \"{lane}\" on this board", lane = to))?;
    let at = match target.cards.last() {
        Some(last) => last.end,
        None => {
            let after = target.complete.unwrap_or(target.heading) + 1;
            if lines.get(after).is_some_and(|l| l.trim().is_empty()) { after + 1 } else { after }
        }
    };
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    let mut block = vec![format!("- [ ] {card}")];
    if out.get(at).is_some_and(|l| l.starts_with("## ")) {
        block.push(String::new());
    }
    out.splice(at..at, block);
    Ok(out.join("\n"))
}

static NOTE_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Z][A-Z0-9]*)-(\d+)\b").unwrap());

/// The id after those leading the notes' names (`T-001 — …`): their most common prefix, the next
/// number, as many digits as the widest. None without such notes.
pub(super) fn next_id<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut by_prefix: std::collections::HashMap<String, (usize, u64, usize)> = Default::default();
    for name in names {
        if let Some(c) = NOTE_ID.captures(name)
            && let Ok(n) = c[2].parse::<u64>()
        {
            let e = by_prefix.entry(c[1].to_string()).or_default();
            *e = (e.0 + 1, e.1.max(n), e.2.max(c[2].len()));
        }
    }
    let (prefix, (_, max, width)) = by_prefix.into_iter().max_by_key(|(p, (count, _, _))| (*count, std::cmp::Reverse(p.clone())))?;
    Some(format!("{prefix}-{:0width$}", max + 1))
}

/// A new note shaped as `template` (another note of the board): its frontmatter keys in its order,
/// `fields` filled and the others empty (`[]` for a list), then its title and its `##` sections,
/// empty. Without a template, the fields and the title. `description`, if any, right under the title.
pub(super) fn new_note(template: Option<&str>, fields: &[(&str, String)], title: &str, description: &str) -> String {
    let (keys, sections): (Vec<String>, Vec<String>) = match template {
        Some(t) => {
            let (front, body) = super::frontmatter(t);
            let order: Vec<String> = t.lines().skip(1).take_while(|l| l.trim() != "---").filter_map(|l| l.split_once(':').map(|(k, _)| k)).filter(|k| !k.starts_with(char::is_whitespace) && front.contains_key(*k)).map(String::from).collect();
            let sections = body.lines().filter(|l| l.starts_with("## ") && !l.trim().eq_ignore_ascii_case(COMMENTS)).map(String::from).collect();
            (order, sections)
        }
        None => (Vec::new(), Vec::new()),
    };
    let template_front = template.map(|t| super::frontmatter(t).0).unwrap_or_default();
    let mut lines = vec!["---".to_string()];
    let mut written: Vec<&str> = Vec::new();
    for k in &keys {
        match fields.iter().find(|(f, _)| f == k) {
            Some((f, v)) => {
                lines.push(format!("{f}: {v}"));
                written.push(f);
            }
            None if template_front.get(k).is_some_and(|v| v.starts_with('[')) => lines.push(format!("{k}: []")),
            None => lines.push(format!("{k}:")),
        }
    }
    lines.extend(fields.iter().filter(|(f, _)| !written.contains(f)).map(|(f, v)| format!("{f}: {v}")));
    lines.push("---".into());
    lines.push(format!("# {title}"));
    let description = description.trim();
    if !description.is_empty() {
        lines.extend([String::new(), description.to_string()]);
    }
    for s in sections {
        lines.extend([String::new(), s]);
    }
    lines.join("\n") + "\n"
}

/// Rewrites a card's first line.
pub(super) fn edit_card(text: &str, spot: Spot, edit: impl FnOnce(&str) -> String) -> Result<String> {
    let lines = split(text);
    let lanes = layout(&lines);
    let at = locate(&lanes, spot)?;
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    let mark = if at.checked { "x" } else { " " };
    out[at.line] = format!("- [{mark}] {}", edit(&at.text));
    Ok(out.join("\n"))
}

/// Adds lines to a card's description (its indented lines), at its end.
pub(super) fn append_to_card(text: &str, spot: Spot, added: &[String]) -> Result<String> {
    let lines = split(text);
    let lanes = layout(&lines);
    let at = locate(&lanes, spot)?;
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    out.splice(at.end..at.end, added.iter().map(|l| format!("\t{l}")));
    Ok(out.join("\n"))
}

static PRIORITY_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|\s)#p[0-3]\b").unwrap());

/// A card's text with its priority tag (`#p0` to `#p3`) set, or removed.
pub(super) fn with_priority_tag(text: &str, tag: Option<&str>) -> String {
    let base = PRIORITY_TAG.replace_all(text, "$1");
    let base = base.split_whitespace().collect::<Vec<_>>().join(" ");
    match tag {
        Some(t) => format!("{base} #{t}"),
        None => base,
    }
}

/// The card has a priority tag.
pub(super) fn has_priority_tag(text: &str) -> bool {
    PRIORITY_TAG.is_match(text)
}

/// A note with frontmatter field `key` set to `value`, or removed (`None`). A note without
/// frontmatter gets one.
pub(super) fn set_field(note: &str, key: &str, value: Option<&str>) -> String {
    let mut lines: Vec<String> = note.split('\n').map(String::from).collect();
    let end = (lines.first().is_some_and(|l| l.trim() == "---")).then(|| lines.iter().skip(1).position(|l| l.trim() == "---").map(|p| p + 1)).flatten();
    let Some(end) = end else {
        return match value {
            Some(v) => format!("---\n{key}: {v}\n---\n{note}"),
            None => note.to_string(),
        };
    };
    let prefix = format!("{key}:");
    let found = lines[1..end].iter().position(|l| l.starts_with(&prefix)).map(|p| p + 1);
    match (found, value) {
        (Some(i), Some(v)) => lines[i] = format!("{key}: {v}"),
        (Some(i), None) => {
            lines.remove(i);
        }
        (None, Some(v)) => lines.insert(end, format!("{key}: {v}")),
        (None, None) => {}
    }
    lines.join("\n")
}

/// The heading of a note's comments.
const COMMENTS: &str = "## Comments";

static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^- \*\*([^*]+)\*\* ?(.*)$").unwrap());

/// A comment as a list item: `- **2026-10-10 14:32** text`, its other lines indented.
pub(super) fn comment_lines(stamp: &str, text: &str) -> Result<Vec<String>> {
    let text = text.trim();
    if text.is_empty() {
        bail!("{}", tr("empty comment"));
    }
    let mut lines = text.lines();
    let mut out = vec![format!("- **{stamp}** {}", lines.next().unwrap_or_default().trim_end())];
    out.extend(lines.map(|l| format!("  {}", l.trim_end())));
    Ok(out)
}

/// A note with a comment added at the end of its `## Comments` section, made if missing.
pub(super) fn add_comment(note: &str, comment: &[String]) -> String {
    let mut lines: Vec<String> = note.trim_end_matches('\n').split('\n').map(String::from).collect();
    match lines.iter().position(|l| l.trim().eq_ignore_ascii_case(COMMENTS)) {
        Some(h) => {
            let end = lines.iter().skip(h + 1).position(|l| l.starts_with("## ")).map_or(lines.len(), |p| p + h + 1);
            // Before the section's trailing blank lines.
            let mut at = end;
            while at > h + 1 && lines[at - 1].trim().is_empty() {
                at -= 1;
            }
            lines.splice(at..at, comment.iter().cloned());
        }
        None => {
            lines.extend([String::new(), COMMENTS.to_string(), String::new()]);
            lines.extend(comment.iter().cloned());
        }
    }
    lines.join("\n") + "\n"
}

/// A note's body without its `## Comments` section, and that section's comments (stamp, text).
pub(super) fn comments(body: &str) -> (String, Vec<(String, String)>) {
    let lines: Vec<&str> = body.lines().collect();
    let Some(h) = lines.iter().position(|l| l.trim().eq_ignore_ascii_case(COMMENTS)) else { return (body.to_string(), Vec::new()) };
    let end = lines.iter().skip(h + 1).position(|l| l.starts_with("## ")).map_or(lines.len(), |p| p + h + 1);
    let mut found: Vec<(String, String)> = Vec::new();
    for line in &lines[h + 1..end] {
        if let Some(c) = ITEM.captures(line) {
            found.push((c[1].trim().to_string(), c[2].to_string()));
        } else if line.starts_with([' ', '\t'])
            && let Some((_, text)) = found.last_mut()
        {
            text.push('\n');
            text.push_str(line.trim());
        }
    }
    let rest = lines[..h].iter().chain(&lines[end..]).copied().collect::<Vec<_>>().join("\n");
    (rest.trim().to_string(), found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot<'a>(text: &'a str, status: &'a str) -> Spot<'a> {
        Spot { text, status, nth: 0 }
    }

    /// Two cards reading the same: the one read second is the one changed.
    #[test]
    fn tells_identical_cards_apart() {
        let board = "## Todo\n\n- [ ] Same\n\tfirst\n- [ ] Same\n\tsecond\n\n## Done\n\n";
        let out = move_card(board, Spot { text: "Same", status: "todo", nth: 1 }, "done").unwrap();
        assert_eq!(out, "## Todo\n\n- [ ] Same\n\tfirst\n\n## Done\n\n- [ ] Same\n\tsecond\n");
    }

    const BOARD: &str = "---\nkanban-plugin: board\n---\n\n## 🎯 Next\n\n- [ ] [[T-2]] #p1\n\n## 📥 Backlog\n\n- [ ] Plain\n\tits line\n- [ ] Other\n\n## 🚧 En cours\n\n\n\n## ✅ Done\n\n**Complete**\n- [x] [[T-1]]\n\n%% kanban:settings\n```\n{}\n```\n%%\n";

    #[test]
    fn moves_a_card_with_its_lines() {
        let out = move_card(BOARD, spot("Plain", "backlog"), "next").unwrap();
        assert!(out.contains("- [ ] [[T-2]] #p1\n- [ ] Plain\n\tits line\n\n## 📥 Backlog\n\n- [ ] Other\n"), "{out}");
        assert!(out.ends_with("%% kanban:settings\n```\n{}\n```\n%%\n"), "the rest stays");
    }

    #[test]
    fn moves_into_an_empty_lane_and_a_done_one() {
        let out = move_card(BOARD, spot("Other", "backlog"), "en cours").unwrap();
        assert!(out.contains("## 🚧 En cours\n\n- [ ] Other\n\n"), "{out}");
        let done = move_card(BOARD, spot("[[T-2]] #p1", "next"), "done").unwrap();
        assert!(done.contains("**Complete**\n- [x] [[T-1]]\n- [x] [[T-2]] #p1\n"), "{done}");
        assert!(done.contains("## 🎯 Next\n\n\n## 📥 Backlog"), "{done}");
        assert!(move_card(BOARD, spot("Gone", "backlog"), "next").is_err());
        assert!(move_card(BOARD, spot("Plain", "backlog"), "nowhere").is_err());
        // A lane not marked complete whose cards are all checked: checked too.
        let checked = "## Todo\n\n- [ ] A\n\n## Done\n\n- [x] B\n";
        assert!(move_card(checked, spot("A", "todo"), "done").unwrap().contains("- [x] B\n- [x] A\n"));
    }

    #[test]
    fn edits_and_extends_a_card() {
        let out = edit_card(BOARD, spot("[[T-2]] #p1", "next"), |t| with_priority_tag(t, Some("p3"))).unwrap();
        assert!(out.contains("- [ ] [[T-2]] #p3\n"), "{out}");
        assert_eq!(with_priority_tag("Fix #p0 #ui", None), "Fix #ui");
        assert!(has_priority_tag("x #p2") && !has_priority_tag("x #p22 #ui"));
        let out = append_to_card(BOARD, spot("Plain", "backlog"), &["- **d** hi".into()]).unwrap();
        assert!(out.contains("- [ ] Plain\n\tits line\n\t- **d** hi\n- [ ] Other"), "{out}");
    }

    #[test]
    fn adds_cards_and_numbers_notes() {
        let out = add_card(BOARD, "en cours", "[[T-3 — New]]").unwrap();
        assert!(out.contains("## 🚧 En cours\n\n- [ ] [[T-3 — New]]\n"), "{out}");
        let out = add_card(BOARD, "backlog", "Loose").unwrap();
        assert!(out.contains("- [ ] Other\n- [ ] Loose\n\n## 🚧"), "{out}");
        assert!(add_card(BOARD, "nowhere", "x").is_err());
        assert_eq!(next_id(["T-001 — a.md", "T-012 — b.md", "X-99.md", "notes.md"]).as_deref(), Some("T-013"));
        assert_eq!(next_id(["T-9 — a"]).as_deref(), Some("T-10"));
        assert_eq!(next_id(["readme"]), None);
    }

    #[test]
    fn new_notes_follow_a_template() {
        let template = "---\nid: T-1\ntitle: Old\ntype: bug\ncreated: 2026-01-01\nstatus: done\ntags: [a, b]\n---\n# Old\n\n## Context\n\ntext\n\n## Comments\n\n- **x** y\n";
        let fields = [("id", "T-2".to_string()), ("title", "New".to_string()), ("created", "2026-10-11".to_string()), ("status", "todo".to_string())];
        assert_eq!(new_note(Some(template), &fields, "New", ""), "---\nid: T-2\ntitle: New\ntype:\ncreated: 2026-10-11\nstatus: todo\ntags: []\n---\n# New\n\n## Context\n");
        assert_eq!(new_note(None, &fields[..2], "New", ""), "---\nid: T-2\ntitle: New\n---\n# New\n");
        assert_eq!(new_note(Some(template), &fields[..2], "New", "Why.\n\n- a\n"), "---\nid: T-2\ntitle: New\ntype:\ncreated:\nstatus:\ntags: []\n---\n# New\n\nWhy.\n\n- a\n\n## Context\n");
    }

    #[test]
    fn sets_frontmatter_fields() {
        let note = "---\nid: T-1\nstatus: todo\n---\nBody\n";
        assert_eq!(set_field(note, "status", Some("done")), "---\nid: T-1\nstatus: done\n---\nBody\n");
        assert_eq!(set_field(note, "priority", Some("p1")), "---\nid: T-1\nstatus: todo\npriority: p1\n---\nBody\n");
        assert_eq!(set_field(note, "status", None), "---\nid: T-1\n---\nBody\n");
        assert_eq!(set_field("Body\n", "priority", Some("p0")), "---\npriority: p0\n---\nBody\n");
    }

    #[test]
    fn adds_and_reads_comments() {
        let c = comment_lines("2026-10-10 14:32", "First line\nsecond").unwrap();
        let note = add_comment("---\nid: T-1\n---\n# Title\n\nBody\n", &c);
        assert_eq!(note, "---\nid: T-1\n---\n# Title\n\nBody\n\n## Comments\n\n- **2026-10-10 14:32** First line\n  second\n");
        let again = add_comment(&note, &comment_lines("2026-10-11 09:00", "Next").unwrap());
        assert!(again.ends_with("  second\n- **2026-10-11 09:00** Next\n"), "{again}");
        let (body, found) = comments("# Title\n\nBody\n\n## Comments\n\n- **2026-10-10 14:32** First line\n  second\n\n## After\nmore");
        assert_eq!(body, "# Title\n\nBody\n\n## After\nmore");
        assert_eq!(found, [("2026-10-10 14:32".to_string(), "First line\nsecond".to_string())]);
        assert!(comment_lines("x", "  ").is_err());
    }
}
