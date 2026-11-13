//! Prism's assistant (`[agents.assistant]`), as the rest of Prism knows it: the folder it works in,
//! its memory (one Markdown file per fact and an index, `MEMORY.md`, per profile), the
//! instructions written there for its CLI, how a request finds its repository, the repositories it
//! may not see (`[[repos]] assistant = false`), and what the window shows of its proposals
//! (`Snapshot.assistant`). The tools themselves are in `prism-commands` (`mcp::assistant`).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{Config, RepoConfig};

/// The folder it works in: `[agents.assistant] dir`, else `assistant/` in the profile's folder.
pub fn dir(cfg: &Config) -> PathBuf {
    match &cfg.agents.assistant.dir {
        Some(d) if !d.as_os_str().is_empty() => crate::config::expand_home(d),
        _ => crate::profile::dir(crate::profile::active()).join("assistant"),
    }
}

/// Its memory's folder: `memory/` in its own.
pub fn memory_dir(cfg: &Config) -> PathBuf {
    dir(cfg).join("memory")
}

/// The index of its memory, one line per fact, read at the start of each conversation.
pub const INDEX: &str = "MEMORY.md";

/// What a fact is about.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "FactKind"))]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    /// A project: what an app is, its repository, the names it goes by.
    #[default]
    Project,
    /// How a project does things: its tickets, its branches.
    Convention,
    /// A decision taken.
    Decision,
    /// The user: how they like things done.
    You,
}

impl FactKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "convention" | "conventions" => Self::Convention,
            "decision" | "decisions" => Self::Decision,
            "you" | "user" | "preference" | "preferences" => Self::You,
            _ => Self::Project,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Convention => "convention",
            Self::Decision => "decision",
            Self::You => "you",
        }
    }
}

/// A fact of its memory: one file, `<id>.md`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Fact {
    /// Its file's name without `.md`.
    pub id: String,
    pub kind: FactKind,
    /// The fact, in a sentence or a few.
    pub text: String,
    /// Where it comes from: « after a question », « “remember that…” », a conversation's title.
    #[serde(default)]
    pub source: String,
    /// When it was remembered.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub at: Option<NaiveDate>,
}

/// The facts of its memory, oldest first; a file it cannot read is passed over.
pub fn facts(cfg: &Config) -> Vec<Fact> {
    facts_in(&memory_dir(cfg))
}

pub fn facts_in(dir: &Path) -> Vec<Fact> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Fact> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md") && p.file_name().is_some_and(|n| n != INDEX))
        .filter_map(|p| {
            let id = p.file_stem()?.to_str()?.to_string();
            parse_fact(&id, &std::fs::read_to_string(&p).ok()?)
        })
        .collect();
    out.sort_by(|a, b| a.at.cmp(&b.at).then(a.id.cmp(&b.id)));
    out
}

/// A fact's file: front matter (`kind`, `source`, `at`), then its text.
fn parse_fact(id: &str, text: &str) -> Option<Fact> {
    let mut fact = Fact { id: id.to_string(), ..Default::default() };
    let body = match text.strip_prefix("---\n").and_then(|rest| rest.split_once("\n---")) {
        Some((head, body)) => {
            for line in head.lines() {
                let Some((k, v)) = line.split_once(':') else { continue };
                let v = v.trim().trim_matches('"').to_string();
                match k.trim() {
                    "kind" => fact.kind = FactKind::parse(&v),
                    "source" => fact.source = v,
                    "at" => fact.at = NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok(),
                    _ => {}
                }
            }
            body
        }
        None => text,
    };
    fact.text = body.trim().to_string();
    (!fact.text.is_empty()).then_some(fact)
}

fn fact_file(f: &Fact) -> String {
    let at = f.at.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
    format!("---\nkind: {}\nsource: {}\nat: {at}\n---\n{}\n", f.kind.word(), serde_json::to_string(&f.source.replace('\n', " ")).unwrap_or_default(), f.text.trim())
}

/// A file name for a new fact: its first words, made unique in `dir`.
fn new_id(dir: &Path, text: &str) -> String {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(6)
        .map(|w| w.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let stem = if words.is_empty() { "fact".to_string() } else { words.join("-") };
    let mut id = stem.clone();
    let mut n = 2;
    while dir.join(format!("{id}.md")).exists() {
        id = format!("{stem}-{n}");
        n += 1;
    }
    id
}

/// A file name of the memory: no folder, no dot first.
fn checked_id(id: &str) -> Result<&str> {
    let id = id.trim().trim_end_matches(".md");
    if id.is_empty() || id.starts_with('.') || id.contains(['/', '\\']) || id == INDEX.trim_end_matches(".md") {
        bail!("{id}: not a fact of the memory");
    }
    Ok(id)
}

/// Writes a fact (a new one when its id is empty) and the index; the fact as written.
pub fn save(cfg: &Config, mut fact: Fact) -> Result<Fact> {
    let dir = memory_dir(cfg);
    std::fs::create_dir_all(&dir).with_context(|| dir.display().to_string())?;
    fact.text = fact.text.trim().to_string();
    if fact.text.is_empty() {
        bail!("an empty fact");
    }
    if let Some(repo) = excluded_in(cfg, &fact.text) {
        bail!("the repository {repo} is out of the assistant's reach ([[repos]] assistant = false): nothing of it is remembered");
    }
    fact.id = if fact.id.trim().is_empty() { new_id(&dir, &fact.text) } else { checked_id(&fact.id)?.to_string() };
    fact.at = fact.at.or_else(|| Some(Utc::now().date_naive()));
    std::fs::write(dir.join(format!("{}.md", fact.id)), fact_file(&fact))?;
    write_index(cfg)?;
    Ok(fact)
}

/// Forgets a fact.
pub fn forget(cfg: &Config, id: &str) -> Result<()> {
    let path = memory_dir(cfg).join(format!("{}.md", checked_id(id)?));
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e).with_context(|| path.display().to_string()),
        _ => {}
    }
    write_index(cfg)
}

/// Forgets every fact.
pub fn forget_all(cfg: &Config) -> Result<usize> {
    let all = facts(cfg);
    for f in &all {
        let _ = std::fs::remove_file(memory_dir(cfg).join(format!("{}.md", f.id)));
    }
    write_index(cfg)?;
    Ok(all.len())
}

/// The index, as the assistant reads it: one line per fact it may see.
pub fn index_text(cfg: &Config) -> String {
    let lines: Vec<String> = visible_facts(cfg).iter().map(|f| format!("- [{}] {} ({}.md)", f.kind.word(), one_line(&f.text), f.id)).collect();
    if lines.is_empty() { "(nothing remembered yet)".into() } else { lines.join("\n") }
}

/// The facts the assistant may see: none naming a repository out of its reach.
pub fn visible_facts(cfg: &Config) -> Vec<Fact> {
    facts(cfg).into_iter().filter(|f| excluded_in(cfg, &f.text).is_none()).collect()
}

fn write_index(cfg: &Config) -> Result<()> {
    let dir = memory_dir(cfg);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(INDEX), format!("# Memory\n\nOne line per fact, its file beside this one.\n\n{}\n", index_text(cfg)))?;
    Ok(())
}

fn one_line(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > 200 { format!("{}…", line.chars().take(199).collect::<String>()) } else { line }
}

/// The repositories the assistant may see.
pub fn visible(cfg: &Config) -> Vec<&RepoConfig> {
    cfg.repos.iter().filter(|r| r.assistant).collect()
}

/// The repository out of its reach that `text` names (its name, an alias, its project), if any.
pub fn excluded_in(cfg: &Config, text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    cfg.repos.iter().filter(|r| !r.assistant).find(|r| names_of(r).iter().any(|n| contains_word(&lower, n))).map(|r| r.name.clone())
}

/// A repository by its name, refused when out of the assistant's reach (said as unknown).
pub fn repo<'a>(cfg: &'a Config, name: &str) -> Result<&'a RepoConfig> {
    let name = name.trim();
    match cfg.repos.iter().find(|r| r.name.eq_ignore_ascii_case(name)) {
        Some(r) if !r.assistant => bail!("the repository {} is out of the assistant's reach ([[repos]] assistant = false): it may not be read", r.name),
        Some(r) => Ok(r),
        None => bail!("no repository named {name}: call repos for the list"),
    }
}

/// What names a repository: its name, its aliases, its project's last part.
fn names_of(r: &RepoConfig) -> Vec<String> {
    let mut names = vec![r.name.to_lowercase()];
    names.extend(r.aliases.iter().map(|a| a.trim().to_lowercase()).filter(|a| !a.is_empty()));
    if let Some(last) = r.project.as_deref().and_then(|p| p.rsplit('/').next()).filter(|l| !l.is_empty()) {
        names.push(last.to_lowercase());
    }
    names.dedup();
    names
}

/// `needle` in `hay` as whole words (both lowercase).
fn contains_word(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '_');
    hay.match_indices(needle).any(|(i, _)| boundary(hay[..i].chars().next_back()) && boundary(hay[i + needle.len()..].chars().next()))
}

/// A repository a request may be about, and why.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Candidate {
    pub repo: String,
    /// 0 to 1.
    pub score: f32,
    pub why: Vec<String>,
}

/// The repositories `text` may be about, the likeliest first: by its memory first, then their
/// name, aliases, project and description. Those out of its reach are never listed.
pub fn find_repo(cfg: &Config, text: &str) -> Vec<Candidate> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-').filter(|w| w.chars().count() >= 3 && !STOP.contains(w)).collect();
    let facts = visible_facts(cfg);
    let mut out = Vec::new();
    for r in visible(cfg) {
        let mut score: f32 = 0.0;
        let mut why = Vec::new();
        let name = r.name.to_lowercase();
        if contains_word(&lower, &name) {
            score = score.max(0.9);
            why.push(format!("its name, {}", r.name));
        }
        for a in r.aliases.iter().filter(|a| !a.trim().is_empty()) {
            if contains_word(&lower, &a.trim().to_lowercase()) {
                score = score.max(0.9);
                why.push(format!("its alias « {} »", a.trim()));
            }
        }
        if let Some(p) = &r.project
            && p.split('/').any(|part| part.len() >= 3 && contains_word(&lower, &part.to_lowercase()))
        {
            score = score.max(0.7);
            why.push(format!("its project, {p}"));
        }
        // A fact naming this repository alone and words of the request (one naming several is
        // read by the assistant, `remembered`).
        for f in &facts {
            let ft = f.text.to_lowercase();
            if contains_word(&ft, &name) && repos_named(cfg, &ft) == 1 && words.iter().any(|w| *w != name && contains_word(&ft, w)) {
                score = score.max(0.95);
                why.push(format!("remembered: {}", one_line(&f.text)));
            }
        }
        if let Some(d) = &r.description {
            let d = d.to_lowercase();
            let hits = words.iter().filter(|w| contains_word(&d, w)).count();
            if hits > 0 {
                score = score.max((0.2 * hits as f32).min(0.5));
                why.push(format!("its description ({hits} word{})", if hits > 1 { "s" } else { "" }));
            }
        }
        if score > 0.0 {
            out.push(Candidate { repo: r.name.clone(), score, why });
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.repo.cmp(&b.repo)));
    out
}

/// How many repositories the assistant may see a (lowercase) text names.
fn repos_named(cfg: &Config, lower: &str) -> usize {
    visible(cfg).iter().filter(|r| names_of(r).iter().any(|n| contains_word(lower, n))).count()
}

/// The facts that share words with `text`, the most shared first: what the assistant reads before
/// it decides, or asks.
pub fn remembered(cfg: &Config, text: &str) -> Vec<Fact> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-').filter(|w| w.chars().count() >= 3 && !STOP.contains(w)).collect();
    let mut hits: Vec<(usize, Fact)> = visible_facts(cfg)
        .into_iter()
        .map(|f| {
            let ft = f.text.to_lowercase();
            (words.iter().filter(|w| contains_word(&ft, w)).count(), f)
        })
        .filter(|(n, _)| *n > 0)
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    hits.into_iter().map(|(_, f)| f).take(8).collect()
}

/// Words too common to tell a repository.
const STOP: &[&str] = &["the", "and", "for", "app", "ticket", "create", "make", "add", "with", "that", "this", "les", "des", "une", "pour", "sur", "dans", "crée", "créer", "implémente", "ticket", "projet", "project", "repo", "faire", "qui", "est", "pas"];

/// What its CLI reads first in its folder (`CLAUDE.md`, `AGENTS.md`): its role, its memory's
/// index, the repositories it may see. Written again at each launch and resume.
pub fn instructions(cfg: &Config) -> String {
    let repos: Vec<String> = visible(cfg)
        .iter()
        .map(|r| {
            let mut line = format!("- {}", r.name);
            if !r.aliases.is_empty() {
                line.push_str(&format!(" (also: {})", r.aliases.join(", ")));
            }
            if let Some(d) = r.description.as_deref().filter(|d| !d.trim().is_empty()) {
                line.push_str(&format!(": {}", one_line(d)));
            }
            line
        })
        .collect();
    let language = if cfg.agents.prompt_language == "fr" { "Answer in French." } else { "Answer in the user's language." };
    format!(
        "{ROLE}\n\n{language}\n\n## Your memory (memory/MEMORY.md)\n\n{}\n\n## The repositories you may see\n\n{}\n",
        index_text(cfg),
        if repos.is_empty() { "(none configured: propose_repo adds one)".to_string() } else { repos.join("\n") }
    )
}

/// Its role, as its CLI reads it.
pub const ROLE: &str = include_str!("assistant_role.md");

/// Writes its folder: the instructions for each CLI and its memory's index. Its path.
pub fn write_home(cfg: &Config) -> Result<PathBuf> {
    let home = dir(cfg);
    std::fs::create_dir_all(home.join("memory")).with_context(|| home.display().to_string())?;
    let text = instructions(cfg);
    for file in ["CLAUDE.md", "AGENTS.md"] {
        std::fs::write(home.join(file), &text)?;
    }
    write_index(cfg)?;
    Ok(home)
}

/// What the window shows of the assistant: its proposals waiting for a click.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AssistantState {
    pub proposals: Vec<AssistantProposal>,
}

/// What a proposal of the assistant does once applied.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ProposalAction {
    /// Creates a ticket on the tracker.
    Ticket,
    /// Launches an agent.
    Launch,
    /// Writes a note.
    Note,
    /// Adds a repository to `[[repos]]` (a folder, a clone or a new project).
    Repo,
    /// Remembers a fact.
    Memory,
}

/// A proposal of the assistant, waiting for the user's click in its Chat.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AssistantProposal {
    pub id: u64,
    pub action: ProposalAction,
    /// The assistant's agent.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub at: DateTime<Utc>,
    /// The repository it is about.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// A ticket's or a note's title, an agent's, a repository's name.
    pub title: String,
    /// A ticket's description, an agent's prompt, a note's text, a fact (Markdown).
    pub text: String,
    /// A ticket's type (`feature`, `bug`…).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket_type: Option<String>,
    /// An agent to launch, as the launcher takes it.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch: Option<crate::state::LaunchSpec>,
    /// The ticket an agent is launched on (`SHOP-214`).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    /// A repository to add: its source (a folder, a clone URL, `group/project`).
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// A fact's kind.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_kind: Option<FactKind>,
    /// Why, in the assistant's words.
    pub why: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(dir: &Path) -> Config {
        let mut cfg: Config = toml::from_str(
            r#"
            [[repos]]
            name = "shop"
            aliases = ["the front"]
            description = "The online shop: orders, cart, invoices"
            project = "acme/shop"

            [[repos]]
            name = "api"
            project = "acme/api"

            [[repos]]
            name = "payroll"
            assistant = false
            "#,
        )
        .unwrap();
        cfg.agents.assistant.dir = Some(dir.to_path_buf());
        cfg
    }

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("prism-assistant-{}-{}", std::process::id(), rand_suffix()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    #[test]
    fn a_request_finds_its_repository_by_name_alias_and_memory() {
        let d = temp();
        let c = cfg(&d);
        assert_eq!(find_repo(&c, "Crée un ticket pour l'app shop")[0].repo, "shop");
        assert_eq!(find_repo(&c, "the front is slow")[0].repo, "shop");
        assert!(find_repo(&c, "billing logo").is_empty());
        save(&c, Fact { text: "The back means the server/ folder of the repository shop, not api.".into(), ..Default::default() }).unwrap();
        assert!(find_repo(&c, "implement 214 on the back").is_empty(), "a fact naming two repositories is read, not scored");
        assert_eq!(remembered(&c, "implement 214 on the back").len(), 1);
        save(&c, Fact { text: "The checkout is the cart/ folder of shop.".into(), ..Default::default() }).unwrap();
        let found = find_repo(&c, "the checkout is slow");
        assert_eq!(found[0].repo, "shop", "{found:?}");
        assert!(found[0].why[0].starts_with("remembered"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_excluded_repository_is_never_listed_read_nor_remembered() {
        let d = temp();
        let c = cfg(&d);
        assert!(find_repo(&c, "payroll leave computation").is_empty());
        assert!(repo(&c, "payroll").is_err());
        assert!(repo(&c, "shop").is_ok());
        assert!(save(&c, Fact { text: "payroll computes the leaves in hr/leave.rs".into(), ..Default::default() }).is_err());
        assert!(!instructions(&c).contains("payroll"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn facts_are_written_read_listed_and_forgotten() {
        let d = temp();
        let c = cfg(&d);
        let f = save(&c, Fact { kind: FactKind::Convention, text: "Tickets of shop: criteria as a list.".into(), source: "remember that…".into(), ..Default::default() }).unwrap();
        assert_eq!(f.id, "tickets-of-shop-criteria-as-a");
        let again = save(&c, Fact { text: "Tickets of shop: criteria as a list.".into(), ..Default::default() }).unwrap();
        assert_eq!(again.id, "tickets-of-shop-criteria-as-a-2");
        let all = facts(&c);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].kind, FactKind::Convention);
        assert_eq!(all[0].source, "remember that…");
        assert!(std::fs::read_to_string(memory_dir(&c).join(INDEX)).unwrap().contains("[convention] Tickets of shop"));
        forget(&c, &again.id).unwrap();
        assert_eq!(facts(&c).len(), 1);
        assert!(forget(&c, "../config").is_err());
        assert_eq!(forget_all(&c).unwrap(), 1);
        assert!(facts(&c).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
