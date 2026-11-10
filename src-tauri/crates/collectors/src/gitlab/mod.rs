use prism_base::{i18n::tr, tr};
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Utc};
use futures::FutureExt;
use serde::{de::DeserializeOwned, Deserialize};
use tokio::process::Command;

use crate::forge::cli::{Cli, Graphql};
use crate::forge::{poll, shown};
use crate::{
    config::GitlabConfig,
    state::{Hub, JobLink, ManualJob, MergeRequest, MrSize, MrState, PendingJob, Pipeline, PipelineStatus, Stage},
    ticket,
};

pub mod ci_health;
pub mod logs;
pub mod renovate;
pub mod review;
mod actions;
mod gql;
pub use actions::{allow_log_project, Actions};
use gql::*;
#[cfg(test)]
mod actions_tests;

const SOURCE: &str = "gitlab";
const MR_QUERY: &str = include_str!("merge_requests.graphql");
const PIPELINES_QUERY: &str = include_str!("pipelines.graphql");

/// GitLab cannot reach a local machine via webhook: we poll at a regular interval.
/// Errors from the other repositories (`[[repos]]`): kept separate, they do not hide the primary
/// repository's MRs and pipelines, nor do its errors hide theirs.
const REPOS_SOURCE: &str = "gitlab repos";

/// `mr_repos`: the other repositories whose MRs are followed; `pipeline_repos`: those whose protected
/// branches' pipelines are (the primary one first, see `Config::pipeline_repos`).
pub async fn run(cfg: GitlabConfig, forge: crate::config::ForgeConfig, mr_repos: Vec<(String, String)>, pipeline_repos: Vec<crate::config::PipelineRepo>, hub: Hub) {
    let every = Duration::from_secs(forge.poll_seconds.max(5));
    let mut first = true;
    let mut refs = poll::Refs::default();
    let merged = MergedCache::new();
    let mut changes = hub.forge_changes();
    loop {
        if !std::mem::take(&mut first) && hub.pause_forge(every, &mut changes).await {
            merged.forget();
        }
        let c = collect(&cfg, forge.merged_days, &mr_repos, &pipeline_repos, &mut refs, &merged).await;
        // The user GitLab answered for: the primary repository's read, failing that another's.
        let me = c.mrs.iter().find_map(|(_, got)| got.as_ref().ok()?.me.clone());
        hub.update(|s| poll::merge(s, me, c.mrs.iter().map(|(r, got)| (r, got.as_ref().ok().map(|g| (&g.mrs[..], &[][..])))), &c.pipelines));
        // After the data: whoever sees the source as answered also sees what it brought.
        let primary = poll::errors(c.mrs.iter().map(|(r, got)| (r, got.as_ref().err())), &c.pipelines, true);
        let others = poll::errors(c.mrs.iter().map(|(r, got)| (r, got.as_ref().err())), &c.pipelines, false);
        for e in &primary {
            tracing::warn!("gitlab: {e}");
        }
        for e in &others {
            tracing::warn!("gitlab repos: {e}");
        }
        hub.set_error(SOURCE, (!primary.is_empty()).then(|| primary.join("; ")));
        if !mr_repos.is_empty() || pipeline_repos.iter().any(|r| r.repo.is_some()) {
            hub.set_error(REPOS_SOURCE, (!others.is_empty()).then(|| others.join("; ")));
        }
    }
}

/// A repository's open MRs and those merged since a date, with the user GitLab answered for.
struct Mrs {
    me: Option<String>,
    mrs: Vec<MergeRequest>,
}

struct Collected {
    /// Per repository (`None`: the primary one; without one, empty, never an error).
    mrs: Vec<(Option<String>, Result<Mrs>)>,
    pipelines: Vec<(Option<String>, poll::Pipelines)>,
}

/// The branches whose pipeline is followed (`[[repos]] pipeline_branches`; none: the main branch,
/// the project's default one if none is declared, and the release branches).
async fn protected_refs(r: &crate::config::PipelineRepo) -> Result<Vec<String>> {
    Ok(pipeline_branches(r).await?.refs)
}

/// The followed branches as GitLab has them, and the entries it has no branch for (Settings' preview).
pub async fn pipeline_branches(r: &crate::config::PipelineRepo) -> Result<crate::config::PipelineBranches> {
    crate::forge::branches::resolve(r, lookup_branches).await
}

/// The project's default branch (when `default`), and each search's branches, all at once: those
/// starting with it (`^`, sorted by name, so a name comes before the longer ones it begins).
async fn lookup_branches(r: &crate::config::PipelineRepo, default: bool, searches: Vec<crate::forge::branches::Search>) -> Result<(Option<String>, Vec<Vec<String>>)> {
    #[derive(Deserialize)]
    struct Branch {
        name: String,
    }
    #[derive(Deserialize)]
    struct Project {
        default_branch: Option<String>,
    }
    let project = encode(&r.project);
    let main = async {
        if !default {
            return Ok(None);
        }
        let p: Project = api(&format!("projects/{project}")).await?;
        p.default_branch.context(tr("the project has no default branch")).map(Some)
    };
    let lists = futures::future::try_join_all(searches.iter().map(|s| {
        let path = match (s.exact, s.prefix.is_empty()) {
            (true, _) => format!("projects/{project}/repository/branches?search={}&per_page=20", encode_query(&format!("^{}", s.prefix))),
            (false, true) => format!("projects/{project}/repository/branches?per_page=100"),
            (false, false) => format!("projects/{project}/repository/branches?search={}&per_page=100", encode_query(&format!("^{}", s.prefix))),
        };
        async move { api::<Vec<Branch>>(&path).await.map(|b| b.into_iter().map(|b| b.name).collect::<Vec<_>>()) }
    }));
    tokio::try_join!(main, lists)
}

/// A query parameter's value, percent-encoded (`^release/` → `%5Erelease%2F`).
fn encode_query(v: &str) -> String {
    v.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A repository's protected branches' latest pipelines, and its pending jobs.
async fn collect_pipelines(r: &crate::config::PipelineRepo, refs: Vec<String>) -> poll::Pipelines {
    let (mut pipelines, mut jobs) = tokio::try_join!(latest_pipelines(&r.project, &refs), pending_jobs(&r.project))?;
    for p in &mut pipelines {
        p.repo = r.repo.clone();
        p.project = Some(r.project.clone());
    }
    for j in &mut jobs {
        j.repo = r.repo.clone();
    }
    Ok((pipelines, jobs))
}

async fn collect(cfg: &GitlabConfig, merged_days: u32, repos: &[(String, String)], pipeline_repos: &[crate::config::PipelineRepo], refs: &mut poll::Refs, merged: &MergedCache) -> Collected {
    let since = (Utc::now() - chrono::Duration::days(merged_days.into())).to_rfc3339();
    // Every repository's MRs at the same time: a repository in error does not block the others.
    let primary = (!cfg.project.is_empty()).then_some((None, cfg.project.as_str()));
    let mrs = futures::future::join_all(primary.into_iter().chain(repos.iter().map(|(name, project)| (Some(name.as_str()), project.as_str()))).map(|(name, project)| repo_mrs(name, project, &since, merged).map(move |r| (name.map(str::to_string), r))));
    let pipelines = poll::pipelines(pipeline_repos, refs, protected_refs, collect_pipelines);
    let (mrs, pipelines) = tokio::join!(mrs, pipelines);
    Collected { mrs, pipelines }
}

/// Merged MRs change little: re-read every `MERGED_CYCLES` reads, as soon as an open one is gone,
/// or after an action from Prism (as GitHub's).
const MERGED_CYCLES: u32 = 4;

/// Per project: its merged MRs, how many reads more they are kept, and the numbers of its open
/// ones last time. Counted in reads, not time: hidden, the window reads more slowly, and a time
/// of four reads at the shown pace had always gone by.
struct MergedCache(std::sync::Mutex<HashMap<String, Merged>>);
type Merged = (u32, Vec<MergeRequest>, Vec<u64>);

impl MergedCache {
    fn new() -> Self {
        Self(Default::default())
    }

    /// The next read takes every project's merged MRs again.
    fn forget(&self) {
        self.0.lock().unwrap().clear();
    }
}

/// A repository's open MRs and those merged since `since` (see `MERGED_CYCLES`), with the user
/// GitLab answered for; one of another repository (`name`) carries its name and project.
async fn repo_mrs(name: Option<&str>, project: &str, since: &str, cache: &MergedCache) -> Result<Mrs> {
    let opened = merge_requests(project, "opened", None).await?;
    let numbers: Vec<u64> = opened.1.iter().map(|m| m.iid).collect();
    let kept = cache.0.lock().unwrap().get(project).filter(|(left, _, open)| *left > 0 && open.iter().all(|n| numbers.contains(n))).map(|(left, merged, _)| (left - 1, merged.clone()));
    let (left, merged) = match kept {
        Some(k) => k,
        None => (MERGED_CYCLES - 1, merge_requests(project, "merged", Some(since)).await?.1),
    };
    cache.0.lock().unwrap().insert(project.to_string(), (left, merged.clone(), numbers));
    let mut mrs = opened.1;
    mrs.extend(merged);
    if let Some(name) = name {
        for m in &mut mrs {
            m.repo = Some(name.to_string());
            m.project = Some(project.to_string());
        }
    }
    Ok(Mrs { me: opened.0, mrs })
}

/// The MRs of the repositories on GitLab in a GitHub profile (`[[repos]] forge = "gitlab"`), read
/// with `glab` beside the profile's forge, at its pace; their errors are those of the other repositories.
pub async fn run_elsewhere(forge: crate::config::ForgeConfig, repos: Vec<(String, String)>, hub: Hub) {
    let every = Duration::from_secs(forge.poll_seconds.max(5));
    let mut first = true;
    let merged = MergedCache::new();
    let mut changes = hub.forge_changes();
    loop {
        if !std::mem::take(&mut first) && hub.pause_forge(every, &mut changes).await {
            merged.forget();
        }
        let since = (Utc::now() - chrono::Duration::days(forge.merged_days.into())).to_rfc3339();
        let read = futures::future::join_all(repos.iter().map(|(name, project)| repo_mrs(Some(name), project, &since, &merged).map(move |r| (name.clone(), r)))).await;
        let errors: Vec<String> = read.iter().filter_map(|(name, r)| Some(format!("{name}: {:#}", r.as_ref().err()?))).collect();
        let me = read.iter().find_map(|(_, r)| r.as_ref().ok()?.me.clone());
        crate::forge::elsewhere::set(&hub, crate::forge::Provider::Gitlab, me, read.into_iter().map(|(name, r)| (name, r.ok().map(|r| r.mrs))).collect());
        for e in &errors {
            tracing::warn!("gitlab repos: {e}");
        }
        hub.set_error(REPOS_SOURCE, (!errors.is_empty()).then(|| errors.join("; ")));
    }
}

async fn merge_requests(project: &str, state: &str, updated_after: Option<&str>) -> Result<(Option<String>, Vec<MergeRequest>)> {
    let mut args = vec![
        "api".to_string(), "graphql".into(),
        "-f".into(), format!("query={MR_QUERY}"),
        "-f".into(), format!("path={project}"),
        "-f".into(), format!("state={state}"),
        // Stage details, diff size and merge checks are only useful for open MRs: no point
        // paying for them for the history.
        "-F".into(), format!("stages={}", state == "opened"),
    ];
    if let Some(after) = updated_after {
        args.extend(["-f".into(), format!("after={after}")]);
    }
    let resp: Graphql<GqlData> = glab(&args).await?;
    let data = resp.into_data()?;
    let project = data.project.context(tr("project not found or not accessible"))?;
    let mrs = project.merge_requests.nodes.into_iter().map(MergeRequest::from).collect();
    Ok((data.current_user.map(|u| u.username), mrs))
}

/// Latest pipeline of each protected branch, with each stage's progress, in one query for the
/// project (one alias per branch): a `glab` costs ~0.4 s and 45 MB before any network, once per
/// branch it was a score of them per cycle. MR pipelines come with the MRs (`headPipeline`).
async fn latest_pipelines(project: &str, refs: &[String]) -> Result<Vec<Pipeline>> {
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec!["api".to_string(), "graphql".into(), "-f".into(), format!("query={}", pipelines_query(refs.len())), "-f".into(), format!("path={project}")];
    for (i, r) in refs.iter().enumerate() {
        args.extend(["-f".into(), format!("r{i}={r}")]);
    }
    let resp: Graphql<GqlPipelinesData> = glab(&args).await?;
    let mut found = resp.into_data()?.project.context(tr("project not found or not accessible"))?;
    Ok((0..refs.len()).filter_map(|i| found.remove(&format!("b{i}"))?.nodes.into_iter().next().map(Pipeline::from)).collect())
}

/// The query of `latest_pipelines` for `n` branches: `b<i>` the last pipeline of `$r<i>`.
fn pipelines_query(n: usize) -> String {
    let params: String = (0..n).map(|i| format!(", $r{i}: String!")).collect();
    let aliases: String = (0..n).map(|i| format!(" b{i}: pipelines(ref: $r{i}, first: 1) {{ nodes {{ ...Pipe }} }}")).collect();
    format!("query($path: ID!{params}) {{ project(fullPath: $path) {{{aliases} }} }}\n{PIPELINES_QUERY}")
}

async fn pending_jobs(project: &str) -> Result<Vec<PendingJob>> {
    let raw: Vec<RawJob> = api(&format!("projects/{}/jobs?scope=pending&per_page=50", encode(project))).await?;
    Ok(raw
        .into_iter()
        .map(|j| PendingJob { repo: None, name: j.name, target: target(&j.r#ref), queued_since: j.created_at })
        .collect())
}

/// `refs/merge-requests/1638/head` → `!1638`; a branch stays as is.
fn target(r: &str) -> String {
    r.strip_prefix("refs/merge-requests/")
        .and_then(|rest| rest.split('/').next())
        .map(|iid| format!("!{iid}"))
        .unwrap_or_else(|| r.to_string())
}

pub fn encode(project: &str) -> String {
    project.replace('/', "%2F")
}

/// Raw response of a `glab api` call (artifact file, text).
pub async fn glab_text(path: &str) -> Result<String> {
    Ok(String::from_utf8_lossy(&glab_bytes(path).await?).into_owned())
}


/// `glab`, pointed at the config's instance (`[gitlab] host`).
pub fn glab_cmd() -> Command {
    Cli::Glab.command()
}

pub async fn api<T: DeserializeOwned>(path: &str) -> Result<T> {
    glab(&["api".to_string(), path.to_string()]).await
}

/// `glab api` call with a full JSON body on standard input (`--input -`): the only
/// way to send nested objects (position of a comment on the diff).
pub async fn glab_input<T: DeserializeOwned>(method: &str, path: &str, body: &serde_json::Value) -> Result<T> {
    Cli::Glab.input(method, path, body).await
}

/// Binary response of a `glab api` call (uploaded file).
async fn glab_bytes(path: &str) -> Result<Vec<u8>> {
    Cli::Glab.run(&["api", path]).await
}

/// Goes through `glab`: it handles authentication (keychain), the token never passes through Prism.
pub async fn glab<T: DeserializeOwned>(args: &[String]) -> Result<T> {
    Cli::Glab.json(args).await
}

#[derive(Deserialize)]
struct RawJob {
    name: String,
    r#ref: String,
    created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without a primary repository, nothing is asked of GitLab for it, and it is not an error.
    #[tokio::test]
    async fn no_primary_repository_is_not_an_error() {
        let c = collect(&GitlabConfig::default(), 7, &[], &[], &mut poll::Refs::default(), &MergedCache::new()).await;
        assert!(c.mrs.is_empty() && c.pipelines.is_empty());
    }
}
