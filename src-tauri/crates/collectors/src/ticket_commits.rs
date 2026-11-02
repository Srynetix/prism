//! A ticket's commits (the ticket panel), read from the local clones as of their last fetch: those
//! whose message names it (`ABC-123`, not `ABC-1234`), on any branch, local or remote, and those of
//! the branches named after it (`feat/abc-123-…`, or with an alias of the prefix) that no other
//! branch has. Each says whether it reached the main or a release branch. No forge call.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{conventions, git::run as git, state::Repo};

/// Commits read at most per repository.
const MAX: usize = 100;
/// A field separator git never prints in a subject.
const SEP: char = '\u{1f}';

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TicketCommit {
    pub repo: String,
    pub sha: String,
    pub short: String,
    pub subject: String,
    pub author: String,
    pub date: DateTime<Utc>,
    /// In the main branch or a release branch.
    pub merged: bool,
    /// Not merged: the branch named after the ticket it is on, when it is on one.
    pub branch: Option<String>,
    /// Its page on the forge, when the repository names its project.
    pub url: Option<String>,
}

/// Every clone's commits of `ticket` (`ABC-123`), newest first.
pub async fn read(repos: &[Repo], ticket: &str) -> Vec<TicketCommit> {
    let Some((prefix, number)) = ticket.trim().rsplit_once('-').filter(|(p, n)| !p.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) else { return Vec::new() };
    let c = conventions::get();
    let mut names: Vec<String> = std::iter::once(prefix.to_string()).chain(c.branch_aliases.iter().cloned()).map(|p| format!("{}-{number}", p.to_lowercase())).collect();
    names.dedup();
    let reads = repos.iter().filter(|r| r.error.is_none() && !r.path.is_empty()).map(|r| one(r, ticket, &names, &c.release_prefix));
    let mut all: Vec<TicketCommit> = futures::future::join_all(reads).await.into_iter().flatten().collect();
    all.sort_by_key(|a| std::cmp::Reverse(a.date));
    all
}

async fn one(repo: &Repo, ticket: &str, names: &[String], release_prefix: &str) -> Vec<TicketCommit> {
    let dir = repo.path.as_str();
    // The protected branches: what is in them is merged.
    let mut protected = vec![format!("origin/{}", repo.base)];
    if !release_prefix.is_empty() {
        let refs = git(dir, &["for-each-ref", "--format=%(refname:short)", &format!("refs/remotes/origin/{release_prefix}")]).await.unwrap_or_default();
        protected.extend(refs.lines().map(String::from));
    }
    // Its id, not one that goes on with more digits.
    let grep = format!("--grep={}([^0-9]|$)", regex_escape(ticket));
    let format = format!("--format=%H{SEP}%h{SEP}%an{SEP}%aI{SEP}%s");
    let max = format!("-n{MAX}");
    let mut merged_args = vec!["log", "-i", "-E", &grep, "--format=%H", &max];
    merged_args.extend(protected.iter().map(String::as_str));
    let merged: HashSet<String> = git(dir, &merged_args).await.unwrap_or_default().lines().map(String::from).collect();

    let mut found: HashMap<String, TicketCommit> = HashMap::new();
    let named = git(dir, &["log", "--branches", "--remotes", "-i", "-E", &grep, &format, &max]).await.unwrap_or_default();
    for c in parse(&named, repo) {
        found.insert(c.sha.clone(), TicketCommit { merged: merged.contains(&c.sha), ..c });
    }
    // The branches named after it: their commits on no other branch, local or remote, which is what
    // is not merged anywhere yet (a branch started from another's work leaves that work out).
    let refs = git(dir, &["for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes"]).await.unwrap_or_default();
    let ticket_refs: Vec<&str> = refs.lines().filter(|r| names_ticket(r, names)).collect();
    let mut excludes = Vec::new();
    for r in &ticket_refs {
        if let Some(b) = r.strip_prefix("refs/heads/") {
            excludes.push(("--branches", format!("--exclude={b}")));
        } else if let Some(b) = r.strip_prefix("refs/remotes/") {
            excludes.push(("--remotes", format!("--exclude={b}")));
        }
    }
    let mut seen = HashSet::new();
    for r in &ticket_refs {
        let name = r.trim_start_matches("refs/heads/").trim_start_matches("refs/remotes/").trim_start_matches("origin/");
        if !seen.insert(name.to_string()) {
            continue;
        }
        let mut args = vec!["log", &format, &max, r, "--not"];
        for side in ["--branches", "--remotes"] {
            args.extend(excludes.iter().filter(|(s, _)| *s == side).map(|(_, e)| e.as_str()));
            args.push(side);
        }
        for c in parse(&git(dir, &args).await.unwrap_or_default(), repo) {
            found.entry(c.sha.clone()).or_insert(c).branch.get_or_insert_with(|| name.to_string());
        }
    }
    let mut out: Vec<TicketCommit> = found.into_values().map(|c| TicketCommit { branch: c.branch.filter(|_| !c.merged), ..c }).collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.date));
    out.truncate(MAX);
    out
}

fn parse(text: &str, repo: &Repo) -> Vec<TicketCommit> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.splitn(5, SEP);
            let (sha, short, author, date, subject) = (f.next()?, f.next()?, f.next()?, f.next()?, f.next()?);
            Some(TicketCommit {
                repo: repo.name.clone(),
                sha: sha.into(),
                short: short.into(),
                subject: subject.into(),
                author: author.into(),
                date: DateTime::parse_from_rfc3339(date).ok()?.with_timezone(&Utc),
                merged: false,
                branch: None,
                url: repo.project.as_deref().map(|p| commit_url(p, sha)),
            })
        })
        .collect()
}

/// `feat/abc-123-x` names `abc-123`, `abc-1234` does not.
fn names_ticket(branch: &str, names: &[String]) -> bool {
    let branch = branch.to_lowercase();
    names.iter().any(|n| {
        branch.match_indices(n.as_str()).any(|(i, _)| {
            let before = branch[..i].chars().next_back();
            let after = branch[i + n.len()..].chars().next();
            !before.is_some_and(|c| c.is_ascii_alphanumeric()) && !after.is_some_and(|c| c.is_ascii_digit())
        })
    })
}

fn commit_url(project: &str, sha: &str) -> String {
    let c = conventions::get();
    match c.forge {
        crate::forge::Provider::Gitlab => conventions::web(&format!("{project}/-/commit/{sha}")),
        crate::forge::Provider::Github => format!("https://github.com/{project}/commit/{sha}"),
    }
}

/// For git's extended expressions.
fn regex_escape(s: &str) -> String {
    s.chars().flat_map(|c| if "\\.^$|?*+()[]{}".contains(c) { vec!['\\', c] } else { vec![c] }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &std::path::Path, args: &[&str]) {
        let ok = std::process::Command::new("git").arg("-C").arg(dir).args(args).env("GIT_AUTHOR_NAME", "Ada").env("GIT_AUTHOR_EMAIL", "a@x").env("GIT_COMMITTER_NAME", "Ada").env("GIT_COMMITTER_EMAIL", "a@x").output().unwrap();
        assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
    }

    #[test]
    fn names_the_ticket_not_a_longer_one() {
        let names = vec!["abc-12".to_string(), "x-12".to_string()];
        assert!(names_ticket("origin/feat/ABC-12-fix", &names));
        assert!(names_ticket("x-12", &names));
        assert!(!names_ticket("feat/abc-123", &names));
        assert!(!names_ticket("feat/xabc-12", &names));
    }

    #[tokio::test]
    async fn reads_named_and_branch_commits() {
        let dir = std::env::temp_dir().join(format!("prism-ticket-commits-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        sh(&dir, &["init", "-q", "-b", "main"]);
        sh(&dir, &["commit", "-q", "--allow-empty", "--date=2026-01-01T10:00:00Z", "-m", "feat: ABC-12 the start"]);
        sh(&dir, &["commit", "-q", "--allow-empty", "--date=2026-01-02T10:00:00Z", "-m", "fix: ABC-123 another ticket"]);
        // origin/main, as a fetch leaves it.
        sh(&dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        // Another unmerged branch, the ticket's started from it: its commit is not the ticket's.
        sh(&dir, &["checkout", "-q", "-b", "feat/other"]);
        sh(&dir, &["commit", "-q", "--allow-empty", "--date=2026-01-03T10:00:00Z", "-m", "other work"]);
        sh(&dir, &["checkout", "-q", "-b", "feat/abc-12-more"]);
        sh(&dir, &["commit", "-q", "--allow-empty", "--date=2026-01-04T10:00:00Z", "-m", "wip"]);
        sh(&dir, &["update-ref", "refs/remotes/origin/feat/abc-12-more", "HEAD"]);
        let repo = Repo { name: "app".into(), path: dir.display().to_string(), project: Some("acme/app".into()), branch: None, base: "main".into(), behind: 0, ahead: 0, dirty: false, is_main: true, error: None, forge: None };
        let commits = read(&[repo], "ABC-12").await;
        let subjects: Vec<(&str, bool, Option<&str>)> = commits.iter().map(|c| (c.subject.as_str(), c.merged, c.branch.as_deref())).collect();
        assert_eq!(subjects, [("wip", false, Some("feat/abc-12-more")), ("feat: ABC-12 the start", true, None)]);
        assert!(commits[0].url.as_deref().unwrap().ends_with(&format!("acme/app/-/commit/{}", commits[0].sha)));
        let _ = std::fs::remove_dir_all(dir);
    }
}

