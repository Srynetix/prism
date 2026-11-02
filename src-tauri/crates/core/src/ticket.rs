use std::sync::LazyLock;

use regex::Regex;

/// Ticket identifiers, from the config's prefix (`[tracker] ticket_prefix`) and those of the
/// Obsidian boards (`[[obsidian.boards]] prefix`, patterns). Without any, the team has no ticket
/// ids: nothing matches.
struct Patterns {
    prefix: String,
    ticket: Option<Regex>,
    /// Branches created by Claude sessions: `claude/ticket-101-…`, `claude/<prefix>-101-…`,
    /// or one of `[tracker] branch_aliases`: `claude/<alias>-512-…`.
    session_branch: Option<Regex>,
}

impl Patterns {
    /// `patterns`: every prefix as a pattern, `prefix`'s included (escaped).
    fn new(prefix: &str, patterns: &[String], aliases: &[String]) -> Self {
        let prefix = prefix.to_uppercase();
        // Patterns are checked when the config is read; one that still fails matches nothing.
        let ticket = (!patterns.is_empty()).then(|| Regex::new(&format!(r"(?i)\b(?:{})-\d+", patterns.join("|"))).ok()).flatten();
        // A branch carrying only the number (`claude/ticket-101-…`) is `prefix`'s.
        let session_branch = (!prefix.is_empty()).then(|| {
            let p = regex::escape(&prefix);
            let names = std::iter::once("ticket".to_string()).chain(std::iter::once(p)).chain(aliases.iter().map(|a| regex::escape(a))).collect::<Vec<_>>().join("|");
            Regex::new(&format!(r"(?i)\b(?:{names})-(\d{{3,5}})\b")).unwrap()
        });
        Self { prefix, ticket, session_branch }
    }

    fn find<'a>(&self, texts: impl IntoIterator<Item = &'a str>) -> Option<String> {
        let re = self.ticket.as_ref()?;
        texts.into_iter().find_map(|t| re.find(t)).map(|m| m.as_str().to_uppercase())
    }

    fn find_all<'a>(&self, texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let Some(re) = self.ticket.as_ref() else { return Vec::new() };
        let mut out: Vec<String> = Vec::new();
        for id in texts.into_iter().flat_map(|t| re.find_iter(t)).map(|m| m.as_str().to_uppercase()) {
            if !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }

    fn find_in_session(&self, title: &str, branch: Option<&str>) -> Option<String> {
        self.find([title, branch.unwrap_or_default()])
            .or_else(|| self.session_branch.as_ref()?.captures(branch?).map(|c| format!("{}-{}", self.prefix, &c[1])))
    }
}

static PATTERNS: LazyLock<Patterns> = LazyLock::new(|| {
    let c = crate::conventions::get();
    Patterns::new(&c.ticket_prefix, &c.ticket_patterns, &c.branch_aliases)
});

/// The ticket prefix (`ABC`), empty without ticket ids.
pub fn prefix() -> &'static str {
    &PATTERNS.prefix
}

/// First ClickUp identifier (`ABC-1234`) found in one of the texts, normalized to uppercase.
pub fn find<'a>(texts: impl IntoIterator<Item = &'a str>) -> Option<String> {
    PATTERNS.find(texts)
}

/// Every identifier found in the texts, in order, without repeats: an MR that names two tickets
/// (`ABC-1 ABC-2 Shared fix`) is linked to both.
pub fn find_all<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    PATTERNS.find_all(texts)
}

/// Like [`find`], also accepting the bare number in session branch names.
pub fn find_in_session(title: &str, branch: Option<&str>) -> Option<String> {
    PATTERNS.find_in_session(title, branch)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The patterns of a lone prefix, as the conventions give them.
    fn pat(prefix: &str) -> Vec<String> {
        if prefix.is_empty() { Vec::new() } else { vec![regex::escape(prefix)] }
    }

    /// Boards' patterns: several prefixes, each a ticket id; the bare number stays the prefix's.
    #[test]
    fn board_patterns_add_prefixes() {
        let p = Patterns::new("T", &["T".into(), "[ABCLM]".into()], &[]);
        assert_eq!(p.find(["agent/c-68-standalone-build"]).as_deref(), Some("C-68"));
        assert_eq!(p.find(["fix T-12"]).as_deref(), Some("T-12"));
        assert_eq!(p.find(["X-12 or AB-3"]), None);
        assert_eq!(p.find_in_session("x", Some("claude/ticket-101-x")).as_deref(), Some("T-101"));
        let none = Patterns::new("", &["[ABCLM]".into()], &[]);
        assert_eq!((none.find(["B-07"]).as_deref(), none.session_branch.is_none()), (Some("B-07"), true));
    }

    /// Another prefix: only its identifiers are recognized.
    #[test]
    fn follows_the_prefix() {
        let p = Patterns::new("acme", &pat("acme"), &[]);
        assert_eq!(p.find(["fix ACME-42 and ABC-1"]).as_deref(), Some("ACME-42"));
        assert!(p.find(["ABC-1234"]).is_none());
        assert_eq!(p.find_in_session("x", Some("claude/acme-101-x")).as_deref(), Some("ACME-101"));
    }

    /// Every ticket of the texts, in order, each once.
    #[test]
    fn finds_every_ticket() {
        let p = Patterns::new("ABC", &pat("ABC"), &[]);
        assert_eq!(p.find_all(["feat: shared fix | ABC-12 abc-34", "feat/ABC-12-abc-56"]), ["ABC-12", "ABC-34", "ABC-56"]);
        assert!(p.find_all(["no ticket"]).is_empty());
    }

    #[test]
    fn explicit_id_wins() {
        let p = Patterns::new("ABC", &pat("ABC"), &[]);
        assert_eq!(p.find_in_session("ABC-5154 refinement", Some("claude/x")).as_deref(), Some("ABC-5154"));
    }

    #[test]
    fn number_from_session_branch() {
        let p = Patterns::new("ABC", &pat("ABC"), &[]);
        assert_eq!(p.find_in_session("Utilité", Some("claude/ticket-101-utility-a1bcd1")).as_deref(), Some("ABC-101"));
        assert_eq!(p.find_in_session("KPI n-1", Some("claude/acme-flow-512-e13fdd")), None);
    }

    /// A branch named after the configured skill plugin carries the ticket number too.
    #[test]
    fn number_from_plugin_branch() {
        let p = Patterns::new("ABC", &pat("ABC"), &["acme-flow".to_string()]);
        assert_eq!(p.find_in_session("KPI n-1", Some("claude/acme-flow-512-e13fdd")).as_deref(), Some("ABC-512"));
    }

    #[test]
    fn unrelated_branch_has_no_ticket() {
        let p = Patterns::new("ABC", &pat("ABC"), &["acme-flow".to_string()]);
        assert_eq!(p.find_in_session("CI failure", Some("claude/ci-failure-mr-1970-8a7e5a")), None);
    }

    /// No prefix: the team has no ticket ids, nothing matches.
    #[test]
    fn empty_prefix_matches_nothing() {
        let p = Patterns::new("", &pat(""), &["acme-flow".to_string()]);
        assert!(p.ticket.is_none() && p.session_branch.is_none());
        assert_eq!(p.find(["fix -42", "ABC-1234"]), None);
        assert_eq!(p.find_in_session("ABC-1 x", Some("claude/ticket-101-x")), None);
        assert_eq!(p.find_in_session("x", Some("claude/acme-flow-512-e13fdd")), None);
    }
}
