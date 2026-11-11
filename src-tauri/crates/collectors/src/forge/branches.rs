//! The branches whose pipeline a repository follows (`[[repos]] pipeline_branches`): its list
//! read against the forge's branches. Each provider only answers the searches; the order, the
//! patterns, the tokens and the duplicates are settled here, the same for both.

use std::future::Future;

use anyhow::Result;

use crate::config::{PipelineBranches, PipelineRepo};

/// At most this many branches per pattern (and for the release prefix), as before the list.
pub const PER_PATTERN: usize = 20;

/// One entry of the list.
#[derive(Debug, Clone, PartialEq)]
enum Entry {
    /// `@main`: the main branch, whichever it is.
    Main,
    /// `@release`: the branches of the release prefix (none without one).
    Release,
    Name(String),
    /// A name with `*`.
    Pattern(String),
}

/// What a forge is asked to list: the branches whose name starts with `prefix`, or the one named
/// `prefix` when `exact`, sorted by name.
#[derive(Debug, Clone, PartialEq)]
pub struct Search {
    pub prefix: String,
    pub exact: bool,
}

/// The list, empty: the main branch and the release ones (as before the list).
fn entries(r: &PipelineRepo) -> Vec<Entry> {
    if r.branches.iter().all(|b| b.trim().is_empty()) {
        return vec![Entry::Main, Entry::Release];
    }
    r.branches
        .iter()
        .map(|b| b.trim())
        .filter(|b| !b.is_empty())
        .map(|b| match b {
            "@main" => Entry::Main,
            "@release" => Entry::Release,
            b if b.contains('*') => Entry::Pattern(b.to_string()),
            b => Entry::Name(b.to_string()),
        })
        .collect()
}

/// The search an entry needs (`None`: nothing to ask, the main branch or no release prefix).
fn search(r: &PipelineRepo, e: &Entry) -> Option<Search> {
    match e {
        Entry::Main => None,
        Entry::Release => (!r.release_prefix.is_empty()).then(|| Search { prefix: r.release_prefix.clone(), exact: false }),
        Entry::Name(n) => Some(Search { prefix: n.clone(), exact: true }),
        Entry::Pattern(p) => Some(Search { prefix: p.split('*').next().unwrap_or_default().to_string(), exact: false }),
    }
}

/// `*` matches any run of characters, `/` included (`env/*`, `release/*-lts`, `*-stable`).
pub fn glob(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if parts.len() == 1 {
        return pattern == name;
    }
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

/// The list against the forge's answers: `default` its default branch (asked only when the main
/// branch is needed and not declared), `found` each search's branches, in the order of `searches`.
fn settle(r: &PipelineRepo, entries: &[Entry], default: Option<String>, mut found: impl Iterator<Item = Vec<String>>) -> PipelineBranches {
    let mut out = PipelineBranches::default();
    let push = |refs: &mut Vec<String>, b: String| {
        if !refs.contains(&b) {
            refs.push(b);
        }
    };
    for e in entries {
        let names = search(r, e).map(|_| found.next().unwrap_or_default());
        match e {
            Entry::Main => {
                let main = if r.main_branch.is_empty() { default.clone() } else { Some(r.main_branch.clone()) };
                if let Some(main) = main {
                    push(&mut out.refs, main);
                }
            }
            Entry::Release => {
                // 20 new ones: those an earlier entry already follows do not take a place.
                let fresh: Vec<String> = names.unwrap_or_default().into_iter().filter(|b| b.starts_with(&r.release_prefix) && !out.refs.contains(b)).take(PER_PATTERN).collect();
                out.refs.extend(fresh);
            }
            Entry::Name(n) => match names.unwrap_or_default().into_iter().find(|b| b == n) {
                Some(b) => push(&mut out.refs, b),
                None => out.missing.push(n.clone()),
            },
            Entry::Pattern(p) => {
                let matching: Vec<String> = names.unwrap_or_default().into_iter().filter(|b| glob(p, b)).collect();
                if matching.is_empty() {
                    out.missing.push(p.clone());
                }
                let fresh: Vec<String> = matching.into_iter().filter(|b| !out.refs.contains(b)).take(PER_PATTERN).collect();
                out.refs.extend(fresh);
            }
        }
    }
    out
}

/// A repository's followed branches: `lookup` (the forge's) answers with the default branch when
/// asked (`true`), and the branches of each search, in order; all in one go.
pub async fn resolve<'a, F, Fut>(r: &'a PipelineRepo, lookup: F) -> Result<PipelineBranches>
where
    F: FnOnce(&'a PipelineRepo, bool, Vec<Search>) -> Fut,
    Fut: Future<Output = Result<(Option<String>, Vec<Vec<String>>)>>,
{
    let entries = entries(r);
    let needs_default = r.main_branch.is_empty() && entries.contains(&Entry::Main);
    let searches: Vec<Search> = entries.iter().filter_map(|e| search(r, e)).collect();
    let (default, found) = if !needs_default && searches.is_empty() { (None, Vec::new()) } else { lookup(r, needs_default, searches).await? };
    Ok(settle(r, &entries, default, found.into_iter()))
}

/// The same against a known list of branches (sorted by name) and default branch: the demo's.
pub fn resolve_against(r: &PipelineRepo, branches: &[&str], default: &str) -> PipelineBranches {
    let entries = entries(r);
    let found = entries.iter().filter_map(|e| search(r, e)).map(|s| branches.iter().filter(|b| if s.exact { **b == s.prefix } else { b.starts_with(&s.prefix) }).map(|b| b.to_string()).collect()).collect::<Vec<Vec<String>>>();
    settle(r, &entries, Some(default.to_string()), found.into_iter())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(main: &str, release: &str, branches: &[&str]) -> PipelineRepo {
        PipelineRepo { name: "app".into(), repo: None, project: "g/app".into(), main_branch: main.into(), release_prefix: release.into(), branches: branches.iter().map(|b| b.to_string()).collect() }
    }

    const BRANCHES: &[&str] = &["develop", "env/prod", "env/staging", "main", "release/2.4", "release/2.5", "trunk"];

    async fn resolved(r: PipelineRepo) -> (PipelineBranches, Vec<(bool, Vec<Search>)>) {
        // A forge with these branches (`trunk` its default one), the searches it was asked kept.
        let asked = std::cell::RefCell::new(Vec::new());
        let lookup = |_: &PipelineRepo, default: bool, searches: Vec<Search>| {
            let found = searches.iter().map(|s| BRANCHES.iter().filter(|b| if s.exact { **b == s.prefix } else { b.starts_with(&s.prefix) }).map(|b| b.to_string()).collect()).collect();
            asked.borrow_mut().push((default, searches));
            std::future::ready(Ok((default.then(|| "trunk".to_string()), found)))
        };
        let got = resolve(&r, lookup).await.unwrap();
        (got, asked.into_inner())
    }

    /// Without a list, as before: the main branch (the default one if none is declared) and the
    /// release ones; nothing asked when neither is needed.
    #[tokio::test]
    async fn no_list_is_main_and_releases() {
        assert_eq!(resolved(repo("main", "release/", &[])).await.0.refs, ["main", "release/2.4", "release/2.5"]);
        assert_eq!(resolved(repo("", "release/", &[])).await.0.refs, ["trunk", "release/2.4", "release/2.5"]);
        let (got, asked) = resolved(repo("main", "", &[])).await;
        assert_eq!((got.refs, asked.len()), (vec!["main".to_string()], 0), "no forge call");
        let (got, asked) = resolved(repo("", "", &[])).await;
        assert_eq!((got.refs, asked), (vec!["trunk".to_string()], vec![(true, vec![])]));
    }

    /// Names only: those branches and nothing else, the default branch not asked.
    #[tokio::test]
    async fn names_only() {
        let (got, asked) = resolved(repo("", "release/", &["main", "develop"])).await;
        assert_eq!(got.refs, ["main", "develop"]);
        assert!(got.missing.is_empty() && !asked[0].0);
    }

    /// Patterns, tokens, duplicates removed with the first place kept, a missing name reported.
    #[tokio::test]
    async fn patterns_tokens_and_duplicates() {
        let (got, _) = resolved(repo("main", "release/", &["develop", "@main", "env/*", "main", "@release", "release/2.5", "nope", "hotfix/*"])).await;
        assert_eq!(got.refs, ["develop", "main", "env/prod", "env/staging", "release/2.4", "release/2.5"]);
        assert_eq!(got.missing, ["nope", "hotfix/*"]);
        // `@release` without a prefix: nothing, and not reported.
        let (got, _) = resolved(repo("main", "", &["@main", "@release"])).await;
        assert_eq!((got.refs, got.missing), (vec!["main".to_string()], vec![]));
    }

    /// A pattern's 20 places go to branches not followed yet.
    #[tokio::test]
    async fn the_cap_counts_new_branches() {
        let r = repo("main", "", &["a/1*", "a/*"]);
        let names: Vec<String> = (10..40).map(|i| format!("a/{i}")).collect();
        let got = resolve(&r, |_, _, searches: Vec<Search>| std::future::ready(Ok((None, searches.iter().map(|s| names.iter().filter(|b| b.starts_with(&s.prefix)).cloned().collect()).collect())))).await.unwrap();
        // 10 from `a/1*`, then 20 more from `a/*`: 30, every one of them.
        assert_eq!(got.refs.len(), 30);
    }

    #[test]
    fn globs() {
        assert!(glob("env/*", "env/prod") && glob("env/*", "env/eu/prod") && glob("*-stable", "2.4-stable") && glob("release/*-lts", "release/2-lts"));
        assert!(!glob("env/*", "envs/prod") && !glob("release/*-lts", "release/2") && !glob("a*b*c", "acb") && glob("a*b*c", "axbyc"));
    }
}
