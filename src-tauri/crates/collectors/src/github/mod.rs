//! GitHub, through `gh` (its login, in the keychain: the token never passes through Prism): the
//! pull requests of the repositories and their checks, the latest checks of their protected
//! branches, and a pull request's review read on demand. A pull request is the window's merge
//! request; its checks (GitHub Actions jobs, commit statuses) a pipeline whose stages are the
//! workflows. The actions on a pull request are in `actions`; on checks (jobs, logs, retries) in `ci`.

use prism_base::{i18n::tr, tr};
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use futures::FutureExt;
use serde::{de::DeserializeOwned, Deserialize};
use tokio::process::Command;

use crate::state::MrState;
use crate::forge::cli::{Cli, Graphql};
use crate::forge::poll;
use crate::{
    config::{ForgeConfig, PipelineRepo},
    state::{Hub, JobLink, ManualJob, MergeRequest, MrSize, Pipeline, PipelineStatus, PlayedJob, Stage},
    ticket,
};

#[cfg(test)]
mod actions_tests;
mod actions;
mod ci;
pub mod ci_health;
pub mod renovate;
pub mod review;

const SOURCE: &str = "github";
/// Errors of the other repositories: kept apart, as on GitLab.
const REPOS_SOURCE: &str = "github repos";
const PRS_QUERY: &str = include_str!("pull_requests.graphql");
const BRANCH_QUERY: &str = include_str!("branch.graphql");
/// Pull requests per page: their checks make a page heavy, GitHub gives up beyond about thirty.
const PAGE: u32 = 25;
/// Pages read per repository and state: 100 pull requests at most.
const PAGES: usize = 4;
/// (repository, `owner/repo`): `None` for the primary repository.
pub type Repos = Vec<(Option<String>, String)>;

/// The repositories whose pull requests Prism follows, known once the config is read.
#[derive(Clone, Default)]
pub struct Actions {
    repos: Arc<prism_base::rt::Swap<Repos>>,
    /// Projects whose protected branches' checks Prism follows (`[[repos]] pipelines`).
    pipelines: Arc<prism_base::rt::Swap<Vec<String>>>,
}

impl Actions {
    pub fn configure(&self, repos: Repos, pipelines: Vec<String>) {
        self.repos.set(repos);
        self.pipelines.set(pipelines);
    }

    /// Project targeted by a jobs or logs command: the primary repository's by default, or one
    /// Prism follows. No other: the front end does not pick an arbitrary project.
    fn project_for(&self, requested: Option<&str>) -> Result<&str> {
        let Some(p) = requested else { return self.project_of(None) };
        self.repos
            .get()
            .into_iter()
            .flatten()
            .map(|(_, project)| project.as_str())
            .chain(self.pipelines.get().into_iter().flatten().map(String::as_str))
            .find(|project| *project == p)
            .with_context(|| tr!("{p} is not a project Prism follows", p = p))
    }

    /// Project of a pipeline Prism displays: that of its protected branch, or of the pull request carrying it.
    fn pipeline_project(&self, hub: &Hub, id: u64) -> Result<String> {
        let (branch, pr_repo) = {
            let snap = hub.subscribe();
            let snap = snap.borrow();
            let branch = snap.pipelines.iter().find(|p| p.id == id).and_then(|p| p.project.clone());
            let pr_repo = snap.merge_requests.iter().find(|m| m.head_pipeline.as_ref().is_some_and(|p| p.id == id)).map(|m| m.repo.clone());
            (branch, pr_repo)
        };
        match (branch, pr_repo) {
            (Some(project), _) => Ok(project),
            (None, Some(repo)) => self.project_of(repo.as_deref()).map(String::from),
            (None, None) => bail!("{}", tr!("pipeline {id} is not one Prism displays", id = id)),
        }
    }

    pub async fn pipeline_jobs(&self, project: Option<&str>, pipeline: u64) -> Result<Vec<crate::forge::JobInfo>> {
        ci::pipeline_jobs(self.project_for(project)?, pipeline).await
    }

    pub async fn job_log(&self, project: Option<&str>, job: u64) -> Result<crate::forge::JobLog> {
        ci::job_log(self.project_for(project)?, job).await
    }

    pub async fn retry_job(&self, project: Option<&str>, job: u64) -> Result<crate::forge::JobInfo> {
        ci::retry_job(self.project_for(project)?, job).await
    }

    /// The pipeline carrying a job Prism displays in `list`.
    fn job_pipeline(hub: &Hub, id: u64, list: crate::forge::JobList) -> Option<u64> {
        let snap = hub.subscribe();
        let snap = snap.borrow();
        snap.pipelines.iter().chain(snap.merge_requests.iter().filter_map(|m| m.head_pipeline.as_ref())).find(|p| list.in_pipeline(p, id)).map(|p| p.id)
    }

    /// Runs again a job that deployed (re-run): only those Prism displays as such.
    pub async fn replay_job(&self, hub: &Hub, id: u64) -> Result<()> {
        let Some(pipeline) = Self::job_pipeline(hub, id, crate::forge::JobList::Played) else { bail!("{}", tr!("job {id} is not a job run already Prism displays", id = id)) };
        ci::retry_job(&self.pipeline_project(hub, pipeline)?, id).await?;
        crate::forge::forget_job(hub, id, crate::forge::JobList::Played);
        Ok(())
    }

    /// Approves the deployment a job Prism displays is waiting for.
    pub async fn play_job(&self, hub: &Hub, id: u64) -> Result<()> {
        let Some(pipeline) = Self::job_pipeline(hub, id, crate::forge::JobList::Manual) else { bail!("{}", tr!("job {id} is not a job awaiting approval Prism displays", id = id)) };
        ci::approve(&self.pipeline_project(hub, pipeline)?, hub, id).await
    }

    pub async fn cancel_pipeline(&self, hub: &Hub, id: u64) -> Result<()> {
        ci::cancel_pipeline(&self.pipeline_project(hub, id)?, hub, id).await
    }

    pub async fn retry_pipeline(&self, hub: &Hub, id: u64) -> Result<()> {
        ci::retry_pipeline(&self.pipeline_project(hub, id)?, hub, id).await
    }

    /// Project of a pull request: that of its repository, which Prism must follow.
    fn project_of(&self, repo: Option<&str>) -> Result<&str> {
        self.repos
            .get()
            .into_iter()
            .flatten()
            .find(|(name, _)| name.as_deref() == repo)
            .map(|(_, project)| project.as_str())
            .with_context(|| tr!("{repo} is not a repository whose pull requests Prism follows", repo = repo.unwrap_or(tr("the primary repository"))))
    }

    pub async fn mr_detail(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<crate::forge::MrDetail> {
        review::detail(self.project_of(repo)?, hub, repo, iid).await
    }

    pub async fn mr_diff(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<crate::forge::MrDiff> {
        review::diff(self.project_of(repo)?, hub, repo, iid).await
    }

    /// An MR's commits, newest first, read when its Commits tab is shown.
    pub async fn mr_commits(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<Vec<crate::forge::MrCommit>> {
        review::commits(self.project_of(repo)?, hub, repo, iid).await
    }

    pub async fn mr_file(&self, hub: &Hub, repo: Option<&str>, iid: u64, path: &str) -> Result<String> {
        review::file(self.project_of(repo)?, hub, repo, iid, path).await
    }

    /// An image of an MR's diff, as a `data:` URL: its new version, or (`old`) the target branch's.
    pub async fn mr_image(&self, hub: &Hub, repo: Option<&str>, iid: u64, path: &str, old: bool) -> Result<String> {
        review::file_image(self.project_of(repo)?, hub, repo, iid, path, old).await
    }

    pub async fn members(&self, repo: Option<&str>) -> Result<Vec<crate::forge::Member>> {
        review::members(self.project_of(repo)?).await
    }

    /// An image of a text of the pull request, as a `data:` URL; `project`: that of the text.
    pub async fn upload(&self, project: Option<&str>, url: &str) -> Result<String> {
        review::image(self.project_for(project)?, url).await
    }

    /// Runs Renovate now, in its repository (`[renovate] repo`).
    pub async fn play_renovate(&self, hub: &Hub) -> Result<()> {
        renovate::play(&crate::gitlab::renovate::project()?, hub).await
    }

    /// Ticks an item of Renovate's Dependency Dashboard.
    pub async fn check_renovate(&self, hub: &Hub, marker: &str) -> Result<()> {
        renovate::check(&crate::gitlab::renovate::project()?, hub, marker).await
    }

    pub async fn rebase_mr(&self, hub: &Hub, repo: Option<&str>, number: u64) -> Result<()> {
        actions::rebase(self.project_of(repo)?, hub, repo, number).await
    }

    pub async fn merge_mr(&self, hub: &Hub, repo: Option<&str>, number: u64) -> Result<()> {
        actions::merge(self.project_of(repo)?, hub, repo, number).await
    }

    pub async fn set_auto_merge(&self, hub: &Hub, repo: Option<&str>, number: u64, enable: bool) -> Result<()> {
        actions::set_auto_merge(self.project_of(repo)?, hub, repo, number, enable).await
    }

    pub async fn approve_mr(&self, hub: &Hub, repo: Option<&str>, number: u64, on: bool) -> Result<()> {
        actions::approve(self.project_of(repo)?, hub, repo, number, on).await
    }

    pub async fn request_changes(&self, hub: &Hub, repo: Option<&str>, number: u64, on: bool, body: &str) -> Result<()> {
        actions::request_changes(self.project_of(repo)?, hub, repo, number, on, body).await
    }

    pub async fn comment_mr(&self, hub: &Hub, repo: Option<&str>, number: u64, discussion: Option<&str>, body: &str) -> Result<()> {
        actions::comment(self.project_of(repo)?, hub, repo, number, discussion, body).await
    }

    pub async fn comment_line(&self, hub: &Hub, repo: Option<&str>, number: u64, line: crate::forge::LineRef, body: &str) -> Result<()> {
        actions::comment_line(self.project_of(repo)?, hub, repo, number, line, body).await
    }

    pub async fn resolve_thread(&self, hub: &Hub, repo: Option<&str>, number: u64, discussion: &str, resolved: bool) -> Result<()> {
        self.project_of(repo)?;
        actions::resolve(hub, repo, number, discussion, resolved).await
    }

    pub async fn set_draft(&self, hub: &Hub, repo: Option<&str>, number: u64, draft: bool) -> Result<()> {
        actions::set_draft(self.project_of(repo)?, hub, repo, number, draft).await
    }

    pub async fn set_people(&self, hub: &Hub, repo: Option<&str>, number: u64, reviewers: bool, logins: Vec<String>) -> Result<()> {
        actions::set_people(self.project_of(repo)?, hub, repo, number, reviewers, logins).await
    }

    pub async fn labels(&self, repo: Option<&str>) -> Result<Vec<crate::forge::ForgeLabel>> {
        review::labels(self.project_of(repo)?).await
    }

    pub async fn set_labels(&self, hub: &Hub, repo: Option<&str>, number: u64, labels: Vec<String>) -> Result<()> {
        actions::set_labels(self.project_of(repo)?, hub, repo, number, labels).await
    }
}

/// `repos`: every repository whose pull requests are followed, the primary one (`None`) first;
/// `pipeline_repos`: those whose protected branches' checks are. The API's hourly quota (5000
/// points, shared with every other use of `gh`) sets the pace: open pull requests and branches
/// every cycle, one query each per repository; merged ones and teams rarely; slower when the
/// quota runs low.
pub async fn run(cfg: ForgeConfig, repos: Repos, pipeline_repos: Vec<PipelineRepo>, hub: Hub) {
    let every = Duration::from_secs(cfg.poll_seconds.max(5));
    let mut wait = Duration::ZERO;
    let mut refs = poll::Refs::default();
    let mut followed: Vec<Followed> = repos.iter().map(|(repo, project)| Followed { repo: repo.clone(), project: project.clone(), merged: None, open: Vec::new() }).collect();
    let mut me = Identity::default();
    let mut changes = hub.forge_changes();
    loop {
        if !wait.is_zero() && hub.pause_forge(wait, &mut changes).await {
            followed.iter_mut().for_each(Followed::forget);
        }
        me.refresh().await;
        let since = Utc::now() - chrono::Duration::days(cfg.merged_days.into());
        let prs = futures::future::join_all(followed.iter_mut().map(|f| {
            let repo = f.repo.clone();
            f.read(since, &me).map(move |got| (repo, got))
        }));
        let pipelines = poll::pipelines(&pipeline_repos, &mut refs, protected_refs, |r, b| async move { branch_pipelines(r, &b).await });
        let (prs, pipelines) = tokio::join!(prs, pipelines);
        hub.update(|s| poll::merge(s, me.login.clone(), prs.iter().map(|(r, got)| (r, got.as_ref().ok().map(|g| (&g.mrs[..], &g.pending[..])))), &pipelines));
        // After the data: whoever sees the source as answered also sees what it brought.
        let quota = prs.iter().filter_map(|(_, got)| got.as_ref().ok()?.quota).min_by_key(|q| q.0);
        wait = pace(every, quota, Utc::now());
        let errors = |primary: bool| -> Vec<String> {
            let mut list = poll::errors(prs.iter().map(|(r, got)| (r, got.as_ref().err())), &pipelines, primary);
            if primary && wait > every {
                list.push(tr!("GitHub API quota nearly used ({points} points left): next read in {n} min", points = quota.map(|q| q.0).unwrap_or(0), n = wait.as_secs().div_ceil(60)));
            }
            list
        };
        let primary = errors(true);
        for e in &primary {
            tracing::warn!("github: {e}");
        }
        hub.set_error(SOURCE, (!primary.is_empty()).then(|| primary.join("; ")));
        if repos.iter().any(|(r, _)| r.is_some()) || pipeline_repos.iter().any(|r| r.repo.is_some()) {
            let others = errors(false);
            hub.set_error(REPOS_SOURCE, (!others.is_empty()).then(|| others.join("; ")));
        }
    }
}

/// The pull requests of the repositories on GitHub in a GitLab profile (`[[repos]] forge =
/// "github"`), read with `gh` beside the profile's forge, at its pace (slower when the quota runs
/// low); their errors are those of the other repositories.
pub async fn run_elsewhere(cfg: ForgeConfig, repos: Vec<(String, String)>, hub: Hub) {
    let every = Duration::from_secs(cfg.poll_seconds.max(5));
    let mut wait = Duration::ZERO;
    let mut followed: Vec<Followed> = repos.into_iter().map(|(repo, project)| Followed { repo: Some(repo), project, merged: None, open: Vec::new() }).collect();
    let mut me = Identity::default();
    let mut changes = hub.forge_changes();
    loop {
        if !wait.is_zero() && hub.pause_forge(wait, &mut changes).await {
            followed.iter_mut().for_each(Followed::forget);
        }
        me.refresh().await;
        let since = Utc::now() - chrono::Duration::days(cfg.merged_days.into());
        let read = futures::future::join_all(followed.iter_mut().map(|f| {
            let repo = f.repo.clone().unwrap_or_default();
            f.read(since, &me).map(move |got| (repo, got))
        }))
        .await;
        let quota = read.iter().filter_map(|(_, got)| got.as_ref().ok()?.quota).min_by_key(|q| q.0);
        wait = pace(every, quota, Utc::now());
        let mut errors: Vec<String> = read.iter().filter_map(|(repo, got)| Some(format!("{repo}: {:#}", got.as_ref().err()?))).collect();
        if wait > every {
            errors.push(format!("GitHub API quota nearly used ({} points left): next read in {} min", quota.map(|q| q.0).unwrap_or(0), wait.as_secs().div_ceil(60)));
        }
        crate::forge::elsewhere::set(&hub, crate::forge::Provider::Github, me.login.clone(), read.into_iter().map(|(repo, got)| (repo, got.ok().map(|r| r.mrs))).collect());
        for e in &errors {
            tracing::warn!("github repos: {e}");
        }
        hub.set_error(REPOS_SOURCE, (!errors.is_empty()).then(|| errors.join("; ")));
    }
}

/// Points left in the hour, and when the quota is renewed.
type Quota = (u32, DateTime<Utc>);

/// Below this many points left, a cycle waits longer; below the second, until the quota is renewed.
const QUOTA_LOW: u32 = 1000;
const QUOTA_SPENT: u32 = 200;

/// The wait before the next cycle: the configured one while the quota allows it.
fn pace(every: Duration, quota: Option<Quota>, now: DateTime<Utc>) -> Duration {
    match quota {
        Some((left, reset)) if left < QUOTA_SPENT => (reset - now).to_std().unwrap_or_default().clamp(every, Duration::from_secs(15 * 60)),
        Some((left, _)) if left < QUOTA_LOW => every.max(Duration::from_secs(60)),
        _ => every,
    }
}

/// Merged pull requests change little (and cost quota): re-read this often, as soon as an open
/// one is gone, or after an action from Prism.
const MERGED_EVERY: Duration = Duration::from_secs(300);
/// My teams, for the reviews asked of them: re-read hourly.
const TEAMS_EVERY: Duration = Duration::from_secs(3600);

/// A repository whose pull requests Prism follows, and what is kept from one cycle to the next.
struct Followed {
    repo: Option<String>,
    project: String,
    /// Merged pull requests, and when they were read.
    merged: Option<(std::time::Instant, Vec<MergeRequest>)>,
    /// Numbers of the open ones last time: one gone was merged or closed.
    open: Vec<u64>,
}

/// What a cycle reads of a repository.
struct Read {
    mrs: Vec<MergeRequest>,
    pending: Vec<crate::state::PendingJob>,
    quota: Option<Quota>,
}

impl Followed {
    /// The next read takes the merged pull requests again.
    fn forget(&mut self) {
        self.merged = None;
    }

    async fn read(&mut self, since: DateTime<Utc>, me: &Identity) -> Result<Read> {
        let (owner, name) = split(&self.project)?;
        let (open, quota) = pages(owner, name, "OPEN", since).await?;
        let numbers: Vec<u64> = open.iter().map(|p| p.number).collect();
        let gone = self.open.iter().any(|n| !numbers.contains(n));
        let stale = self.merged.as_ref().is_none_or(|(at, _)| at.elapsed() >= MERGED_EVERY);
        if gone || stale {
            let (merged, _) = pages(owner, name, "MERGED", since).await?;
            let merged = merged.into_iter().filter(|p| p.merged_at.is_some_and(|at| at >= since)).map(|p| self.own(p)).collect();
            self.merged = Some((std::time::Instant::now(), merged));
        }
        self.open = numbers;
        let mut pending = Vec::new();
        let mut mrs: Vec<MergeRequest> = open
            .into_iter()
            .map(|mut p| {
                if let Some(c) = p.commits.nodes.first() {
                    pending.extend(queued(&c.commit, &self.repo, &format!("#{}", p.number)));
                }
                me.answer_for_teams(&mut p);
                self.own(p)
            })
            .collect();
        let merged = self.merged.as_ref().map(|(_, m)| m.as_slice()).unwrap_or_default();
        mrs.extend(merged.iter().filter(|m| m.merged_at.is_some_and(|at| at >= since)).cloned());
        Ok(Read { mrs, pending, quota })
    }

    /// As on GitLab: the primary repository's carry neither repository nor project.
    fn own(&self, p: GqlPr) -> MergeRequest {
        let mut m = MergeRequest::from(p);
        if self.repo.is_some() {
            m.repo = self.repo.clone();
            m.project = Some(self.project.clone());
        }
        m
    }
}

/// A state's pull requests, most recently updated first: the merged ones stop at the first page
/// reaching beyond `since`. With the quota the first page left.
async fn pages(owner: &str, name: &str, state: &'static str, since: DateTime<Utc>) -> Result<(Vec<GqlPr>, Option<Quota>)> {
    let (mut prs, mut after, mut quota) = (Vec::new(), None::<String>, None);
    for _ in 0..PAGES {
        let mut vars = vec![("owner", owner.into()), ("name", name.into()), ("states[]", state.into()), ("first", Var::Raw(PAGE.to_string())), ("open", Var::Raw((state == "OPEN").to_string()))];
        if let Some(cursor) = &after {
            vars.push(("after", cursor.as_str().into()));
        }
        let data: GqlData = graphql(PRS_QUERY, &vars).await?;
        quota = data.rate_limit.map(|r| (r.remaining, r.reset_at)).or(quota);
        let page = data.repository.context(tr("repository not found or not accessible"))?.pull_requests;
        let older = page.nodes.last().is_some_and(|p| p.updated_at < since);
        prs.extend(page.nodes);
        match page.page_info {
            PageInfo { has_next_page: true, end_cursor: Some(cursor) } if !(state == "MERGED" && older) => after = Some(cursor),
            _ => break,
        }
    }
    Ok((prs, quota))
}

/// Who `gh` answers for, and the teams they belong to (`org/team`): a review asked of one of
/// them is asked of them. Without the right to read the organizations, no team.
#[derive(Default)]
struct Identity {
    login: Option<String>,
    teams: Vec<String>,
    read: Option<std::time::Instant>,
}

impl Identity {
    async fn refresh(&mut self) {
        if self.read.is_some_and(|at| at.elapsed() < TEAMS_EVERY) {
            return;
        }
        #[derive(Deserialize)]
        struct Viewer {
            viewer: Login,
        }
        #[derive(Deserialize)]
        struct Orgs {
            viewer: OrgsOf,
        }
        #[derive(Deserialize)]
        struct OrgsOf {
            organizations: Nodes<Org>,
        }
        #[derive(Deserialize)]
        struct Org {
            teams: Nodes<Team>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Team {
            combined_slug: String,
        }
        let v = match graphql::<Viewer>("query { viewer { login } }", &[]).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("github: who I am unreadable (my MRs and reviews are not told apart): {e:#}");
                self.read = Some(std::time::Instant::now());
                return;
            }
        };
        const TEAMS: &str = "query($login: String!) { viewer { organizations(first: 100) { nodes { teams(first: 100, userLogins: [$login]) { nodes { combinedSlug } } } } } }";
        self.teams = match graphql::<Orgs>(TEAMS, &[("login", v.viewer.login.as_str().into())]).await {
            Ok(o) => o.viewer.organizations.nodes.into_iter().flat_map(|o| o.teams.nodes).map(|t| t.combined_slug).collect(),
            Err(e) => {
                tracing::warn!("github: my teams unreadable (reviews asked of a team are not mine): {e:#}");
                Vec::new()
            }
        };
        self.login = Some(v.viewer.login);
        self.read = Some(std::time::Instant::now());
    }

    /// A review asked of one of my teams is asked of me.
    fn answer_for_teams(&self, p: &mut GqlPr) {
        let Some(me) = &self.login else { return };
        for r in p.review_requests.nodes.iter_mut().filter_map(|r| r.requested_reviewer.as_mut()) {
            if r.team.as_ref().is_some_and(|t| self.teams.contains(t)) {
                r.login = Some(me.clone());
            }
        }
    }
}

/// The jobs of a commit waiting for a runner: GitHub Actions check runs still queued.
fn queued(c: &GqlCommit, repo: &Option<String>, target: &str) -> Vec<crate::state::PendingJob> {
    let contexts = c.status_check_rollup.as_ref().and_then(|r| r.contexts.as_ref()).map(|n| n.nodes.as_slice()).unwrap_or_default();
    contexts
        .iter()
        .filter_map(|ctx| match ctx {
            CheckContext::CheckRun { name, status, check_suite, .. } if status == "QUEUED" => {
                let since = check_suite.as_ref()?.workflow_run.as_ref()?.created_at?;
                Some(crate::state::PendingJob { repo: repo.clone(), name: name.clone(), target: target.to_string(), queued_since: since })
            }
            _ => None,
        })
        .collect()
}

/// The branches whose checks are followed (`[[repos]] pipeline_branches`; none: the main branch,
/// the default one if none is declared, and the release branches).
async fn protected_refs(r: &PipelineRepo) -> Result<Vec<String>> {
    Ok(pipeline_branches(r).await?.refs)
}

/// The followed branches as GitHub has them, and the entries it has no branch for (Settings' preview).
pub async fn pipeline_branches(r: &PipelineRepo) -> Result<crate::config::PipelineBranches> {
    crate::forge::branches::resolve(r, lookup_branches).await
}

/// The default branch (when `default`), and each search's branches, in one query: a name by its
/// ref, a prefix by `refs`, under its folder (`release/` of `release/2.`, names given without it)
/// with the rest as `query` (which matches anywhere in the name: those starting with the prefix
/// kept). A prefix with more than a page of branches reads its next pages on its own.
async fn lookup_branches(r: &PipelineRepo, default: bool, searches: Vec<crate::forge::branches::Search>) -> Result<(Option<String>, Vec<Vec<String>>)> {
    #[derive(Deserialize)]
    struct Data {
        repository: Option<HashMap<String, serde_json::Value>>,
    }
    #[derive(Deserialize)]
    struct Name {
        name: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Page {
        nodes: Vec<Name>,
        page_info: PageInfo,
    }
    /// At most this many pages of 100 per prefix.
    const PAGES: usize = 10;
    // (folder, rest) of a prefix: `refs(refPrefix:)` needs a folder ending with `/`.
    let split_prefix = |p: &str| match p.rfind('/') {
        Some(i) => (p[..=i].to_string(), p[i + 1..].to_string()),
        None => (String::new(), p.to_string()),
    };
    let (owner, name) = split(&r.project)?;
    let mut params = String::new();
    let mut fields = String::new();
    let mut values: Vec<(String, String)> = Vec::new();
    if default {
        fields.push_str(" defaultBranchRef { name }");
    }
    for (i, s) in searches.iter().enumerate() {
        if s.exact {
            params.push_str(&format!(", $q{i}: String!"));
            fields.push_str(&format!(" s{i}: ref(qualifiedName: $q{i}) {{ name }}"));
            values.push((format!("q{i}"), format!("refs/heads/{}", s.prefix)));
        } else {
            let (dir, rest) = split_prefix(&s.prefix);
            params.push_str(&format!(", $p{i}: String!, $q{i}: String!"));
            fields.push_str(&format!(" s{i}: refs(refPrefix: $p{i}, query: $q{i}, first: 100) {{ nodes {{ name }} pageInfo {{ hasNextPage endCursor }} }}"));
            values.push((format!("p{i}"), format!("refs/heads/{dir}")));
            values.push((format!("q{i}"), rest));
        }
    }
    let query = format!("query($owner: String!, $name: String!{params}) {{ repository(owner: $owner, name: $name) {{{fields} }} }}");
    let mut vars: Vec<(&str, Var)> = vec![("owner", owner.into()), ("name", name.into())];
    vars.extend(values.iter().map(|(k, v)| (k.as_str(), Var::Str(v))));
    let data: Data = graphql(&query, &vars).await?;
    let mut repo = data.repository.context(tr("repository not found or not accessible"))?;
    let main = if default {
        let main: Option<Name> = serde_json::from_value(repo.remove("defaultBranchRef").unwrap_or_default())?;
        Some(main.context(tr("the repository has no default branch"))?.name)
    } else {
        None
    };
    let mut lists = Vec::new();
    for (i, s) in searches.iter().enumerate() {
        let got = repo.remove(&format!("s{i}")).unwrap_or_default();
        if s.exact {
            lists.push(serde_json::from_value::<Option<Name>>(got)?.map(|n| n.name).into_iter().collect());
            continue;
        }
        let (dir, rest) = split_prefix(&s.prefix);
        let mut page: Option<Page> = serde_json::from_value(got)?;
        let mut names = Vec::new();
        for n in 1.. {
            let Some(p) = page.take() else { break };
            names.extend(p.nodes.into_iter().map(|b| format!("{dir}{}", b.name)).filter(|b| b.starts_with(&s.prefix)));
            let after = match p.page_info.end_cursor {
                Some(c) if p.page_info.has_next_page && n < PAGES => c,
                _ => break,
            };
            #[derive(Deserialize)]
            struct More {
                repository: Option<RefsOnly>,
            }
            #[derive(Deserialize)]
            struct RefsOnly {
                refs: Option<Page>,
            }
            const MORE: &str = "query($owner: String!, $name: String!, $p: String!, $q: String!, $after: String!) { repository(owner: $owner, name: $name) { refs(refPrefix: $p, query: $q, first: 100, after: $after) { nodes { name } pageInfo { hasNextPage endCursor } } } }";
            let prefix = format!("refs/heads/{dir}");
            let more: More = graphql(MORE, &[("owner", owner.into()), ("name", name.into()), ("p", prefix.as_str().into()), ("q", rest.as_str().into()), ("after", after.as_str().into())]).await?;
            page = more.repository.and_then(|r| r.refs);
        }
        lists.push(names);
    }
    Ok((main, lists))
}

/// The checks of each protected branch's last commit, in one query for the repository (one alias
/// per branch), and the jobs waiting for a runner on them.
async fn branch_pipelines(r: &PipelineRepo, refs: &[String]) -> Result<(Vec<Pipeline>, Vec<crate::state::PendingJob>)> {
    #[derive(Deserialize)]
    struct Data {
        repository: Option<HashMap<String, Option<Ref>>>,
    }
    #[derive(Deserialize)]
    struct Ref {
        target: Option<GqlCommit>,
    }
    let (owner, name) = split(&r.project)?;
    let params: String = (0..refs.len()).map(|i| format!(", $r{i}: String!")).collect();
    let aliases: String = (0..refs.len()).map(|i| format!(" b{i}: ref(qualifiedName: $r{i}) {{ target {{ ...Checks }} }}")).collect();
    let query = format!("query($owner: String!, $name: String!{params}) {{ repository(owner: $owner, name: $name) {{{aliases} }} }}\n{BRANCH_QUERY}");
    let names: Vec<(String, String)> = refs.iter().enumerate().map(|(i, b)| (format!("r{i}"), format!("refs/heads/{b}"))).collect();
    let mut vars: Vec<(&str, Var)> = vec![("owner", owner.into()), ("name", name.into())];
    vars.extend(names.iter().map(|(k, v)| (k.as_str(), v.as_str().into())));
    let data: Data = graphql(&query, &vars).await?;
    let mut found = data.repository.context(tr("repository not found or not accessible"))?;
    let (mut pipelines, mut pending) = (Vec::new(), Vec::new());
    for (i, branch) in refs.iter().enumerate() {
        let Some(c) = found.remove(&format!("b{i}")).flatten().and_then(|r| r.target) else { continue };
        pending.extend(queued(&c, &r.repo, branch));
        let url = c.url.clone();
        if let Some(mut p) = pipeline(c, branch.clone(), url) {
            p.repo = r.repo.clone();
            p.project = Some(r.project.clone());
            pipelines.push(p);
        }
    }
    Ok((pipelines, pending))
}

/// `owner/repo` → (`owner`, `repo`).
fn split(project: &str) -> Result<(&str, &str)> {
    project.split_once('/').filter(|(o, n)| !o.is_empty() && !n.is_empty() && !n.contains('/')).with_context(|| tr!("{project}: expected owner/repo", project = project))
}

/* ---------- gh ---------- */

/// A GraphQL variable: a string (`-f`), or a number, boolean (`-F`).
pub enum Var<'a> {
    Str(&'a str),
    Raw(String),
}

impl<'a> From<&'a str> for Var<'a> {
    fn from(s: &'a str) -> Self {
        Var::Str(s)
    }
}

pub fn gh_cmd() -> Command {
    Cli::Gh.command()
}

/// Goes through `gh`: it handles authentication, the token never passes through Prism.
pub async fn gh_bytes(args: &[String]) -> Result<Vec<u8>> {
    Cli::Gh.run(args).await
}

pub async fn api<T: DeserializeOwned>(path: &str) -> Result<T> {
    serde_json::from_slice(&gh_bytes(&["api".into(), path.into()]).await?).context("unreadable gh response")
}

/// A REST call with a JSON body on standard input (`--input -`): lists (assignees, reviewers) go this way.
pub async fn gh_input<T: DeserializeOwned>(method: &str, path: &str, body: &serde_json::Value) -> Result<T> {
    Cli::Gh.input(method, path, body).await
}

pub async fn graphql<T: DeserializeOwned>(query: &str, vars: &[(&str, Var<'_>)]) -> Result<T> {
    let mut args = vec!["api".to_string(), "graphql".into(), "-f".into(), format!("query={query}")];
    for (k, v) in vars {
        match v {
            Var::Str(s) => args.extend(["-f".into(), format!("{k}={s}")]),
            Var::Raw(s) => args.extend(["-F".into(), format!("{k}={s}")]),
        }
    }
    Cli::Gh.json::<Graphql<T>, _>(&args).await?.into_data()
}

#[derive(Deserialize)]
pub struct Nodes<T> {
    pub nodes: Vec<T>,
}

/* ---------- Pull requests ---------- */

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlData {
    rate_limit: Option<RateLimit>,
    repository: Option<GqlRepository>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimit {
    remaining: u32,
    reset_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlRepository {
    pull_requests: PrPage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrPage {
    nodes: Vec<GqlPr>,
    page_info: PageInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct Login {
    pub login: String,
}

/// A user, a bot or a team: only a user has a name, a team has no login here.
#[derive(Deserialize)]
pub struct Actor {
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// A team asked for a review: `org/team`.
    #[serde(default, rename = "combinedSlug")]
    pub team: Option<String>,
    #[serde(default, rename = "avatarUrl")]
    pub avatar: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRequest {
    requested_reviewer: Option<Actor>,
}

#[derive(Deserialize)]
pub struct Opinion {
    pub state: String,
    pub author: Option<Actor>,
}

#[derive(Deserialize)]
struct Label {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Thread {
    is_resolved: bool,
}

#[derive(Deserialize)]
struct CommitNode {
    commit: GqlCommit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlPr {
    number: u64,
    title: String,
    is_draft: bool,
    url: String,
    head_ref_name: String,
    base_ref_name: String,
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    merged_at: Option<DateTime<Utc>>,
    state: String,
    author: Option<Actor>,
    assignees: Nodes<Login>,
    review_requests: Nodes<ReviewRequest>,
    latest_opinionated_reviews: Nodes<Opinion>,
    review_decision: Option<String>,
    #[serde(default)]
    base_ref: Option<BaseRef>,
    mergeable: String,
    merge_state_status: String,
    auto_merge_request: Option<serde_json::Value>,
    labels: Nodes<Label>,
    review_threads: Option<Nodes<Thread>>,
    additions: u64,
    deletions: u64,
    changed_files: u64,
    commits: Nodes<CommitNode>,
}

/// Those who approved, from the latest review of each reviewer.
pub fn approvers(reviews: &[Opinion]) -> Vec<String> {
    reviews.iter().filter(|r| r.state == "APPROVED").filter_map(|r| r.author.as_ref()?.login.clone()).collect()
}

/// Those who requested changes, from the latest review of each reviewer.
pub fn requesters(reviews: &[Opinion]) -> Vec<String> {
    reviews.iter().filter(|r| r.state == "CHANGES_REQUESTED").filter_map(|r| r.author.as_ref()?.login.clone()).collect()
}

/// Reviewers: those asked, and those who already gave their opinion (GitHub no longer asks them).
pub fn reviewers(requests: Vec<ReviewRequest>, opinions: &[Opinion], author: &str) -> Vec<String> {
    let mut list: Vec<String> = requests.into_iter().filter_map(|r| r.requested_reviewer?.login).collect();
    for r in opinions.iter().filter_map(|r| r.author.as_ref()?.login.clone()) {
        if r != author && !list.contains(&r) {
            list.push(r);
        }
    }
    list
}

/// The rules of the branch a pull request targets, as far as I may read them.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BaseRef {
    branch_protection_rule: Option<Protection>,
    rules: Option<Nodes<Rule>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Protection {
    required_approving_review_count: Option<u32>,
}

#[derive(Deserialize)]
struct Rule {
    r#type: String,
    parameters: Option<RuleParameters>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuleParameters {
    #[serde(default)]
    required_approving_review_count: Option<u32>,
}

impl BaseRef {
    /// Approvals the branch requires: the most demanding of its protection and its rulesets;
    /// `None` when neither is visible to me.
    pub fn required(&self) -> Option<u32> {
        let protection = self.branch_protection_rule.as_ref().and_then(|p| p.required_approving_review_count);
        let rules = self.rules.iter().flat_map(|r| &r.nodes).filter(|r| r.r#type == "PULL_REQUEST").filter_map(|r| r.parameters.as_ref()?.required_approving_review_count);
        protection.into_iter().chain(rules).max()
    }
}

/// Approvals still missing: those the branch requires, less those given. Its rules unreadable
/// (no rights on the repository), or met while GitHub still asks for a review (code owners): one
/// while it does, none otherwise.
pub fn approvals_left(decision: Option<&str>, required: Option<u32>, approvals: usize) -> i64 {
    let asked = i64::from(decision == Some("REVIEW_REQUIRED"));
    match required {
        Some(n) => (i64::from(n) - approvals as i64).max(asked),
        None => asked,
    }
}

/// GitHub's merge state, in the window's (GitLab's detailed merge status) terms.
fn merge_status(state: &str, decision: Option<&str>) -> &'static str {
    match state {
        "CLEAN" | "HAS_HOOKS" | "UNSTABLE" => "mergeable",
        "BEHIND" => "need_rebase",
        "DIRTY" => "conflict",
        "DRAFT" => "draft_status",
        "BLOCKED" if matches!(decision, Some("REVIEW_REQUIRED" | "CHANGES_REQUESTED")) => "not_approved",
        "BLOCKED" => "blocked_status",
        _ => "checking",
    }
}

impl From<GqlPr> for MergeRequest {
    fn from(p: GqlPr) -> Self {
        let open = p.state == "OPEN";
        let author = p.author.unwrap_or(Actor { login: None, name: None, team: None, avatar: None });
        let login = author.login.unwrap_or_else(|| "ghost".into());
        let approved_by = approvers(&p.latest_opinionated_reviews.nodes);
        let changes_requested_by = requesters(&p.latest_opinionated_reviews.nodes);
        let left = if open { approvals_left(p.review_decision.as_deref(), p.base_ref.as_ref().and_then(BaseRef::required), approved_by.len()) } else { 0 };
        let reviewers = reviewers(p.review_requests.nodes, &p.latest_opinionated_reviews.nodes, &login);
        let head = p.commits.nodes.into_iter().next().map(|c| c.commit);
        let checks = format!("{}/checks", p.url);
        let head_pipeline = head.and_then(|c| pipeline(c, p.head_ref_name.clone(), checks));
        MergeRequest {
            iid: p.number,
            repo: None,
            project: None,
            forge: None,
            ticket: ticket::find([p.title.as_str(), p.head_ref_name.as_str()]),
            tickets: ticket::find_all([p.title.as_str(), p.head_ref_name.as_str()]),
            title: p.title,
            url: p.url,
            state: if p.merged_at.is_some() { MrState::Merged } else if open { MrState::Opened } else { MrState::Closed },
            draft: p.is_draft,
            source_branch: p.head_ref_name,
            target_branch: p.base_ref_name,
            author_name: author.name.filter(|n| !n.is_empty()).unwrap_or_else(|| login.clone()),
            author_avatar: author.avatar.filter(|a| a.starts_with("https://")),
            author: login,
            assignees: p.assignees.nodes.into_iter().map(|a| a.login).collect(),
            reviewers,
            approved_by,
            changes_requested_by,
            approvals_left: left,
            pipeline: head_pipeline.as_ref().map(|h| h.status),
            head_pipeline,
            merge_status: merge_status(&p.merge_state_status, p.review_decision.as_deref()).into(),
            conflicts: p.mergeable == "CONFLICTING",
            needs_rebase: p.merge_state_status == "BEHIND",
            rebase_in_progress: false,
            merge_error: None,
            auto_merge: p.auto_merge_request.is_some(),
            labels: p.labels.nodes.into_iter().map(|l| l.name).collect(),
            open_threads: p.review_threads.map(|t| t.nodes.iter().filter(|t| !t.is_resolved).count() as i64).unwrap_or(0),
            size: open.then_some(MrSize { additions: p.additions, deletions: p.deletions, files: p.changed_files }),
            created_at: p.created_at,
            updated_at: p.updated_at,
            merged_at: p.merged_at,
        }
    }
}

/* ---------- Checks ---------- */

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlCommit {
    oid: String,
    url: String,
    committed_date: DateTime<Utc>,
    status_check_rollup: Option<Rollup>,
}

#[derive(Deserialize)]
struct Rollup {
    state: String,
    contexts: Option<Nodes<CheckContext>>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum CheckContext {
    #[serde(rename_all = "camelCase")]
    CheckRun {
        #[serde(default)]
        database_id: Option<u64>,
        name: String,
        status: String,
        conclusion: Option<String>,
        started_at: Option<DateTime<Utc>>,
        completed_at: Option<DateTime<Utc>>,
        details_url: Option<String>,
        check_suite: Option<CheckSuite>,
        /// The deployment it ran, if any: a job of a protected environment (GitLab's manual one).
        #[serde(default)]
        deployment: Option<Deployment>,
    },
    #[serde(rename_all = "camelCase")]
    StatusContext {
        context: String,
        state: String,
        target_url: Option<String>,
        created_at: Option<DateTime<Utc>>,
    },
}

/// A check run's deployment: only that there is one matters.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Deployment {
    #[serde(default)]
    #[allow(dead_code, reason = "read for its presence")]
    database_id: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckSuite {
    workflow_run: Option<WorkflowRun>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowRun {
    workflow: Workflow,
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct Workflow {
    name: String,
}

/// A check, in the window's (GitLab's) statuses.
struct Check {
    /// The job's id, for a GitHub Actions one.
    id: Option<u64>,
    stage: String,
    name: String,
    status: PipelineStatus,
    url: String,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    /// It deployed to an environment: once finished, a job that can run again (re-run).
    deployed: bool,
}

fn rollup_status(state: &str) -> PipelineStatus {
    match state {
        "SUCCESS" => PipelineStatus::Success,
        "FAILURE" | "ERROR" => PipelineStatus::Failed,
        "PENDING" => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
}

fn run_status(status: &str, conclusion: Option<&str>) -> PipelineStatus {
    match (status, conclusion) {
        ("COMPLETED", Some("SUCCESS")) => PipelineStatus::Success,
        ("COMPLETED", Some("CANCELLED")) => PipelineStatus::Canceled,
        ("COMPLETED", Some("SKIPPED" | "NEUTRAL" | "STALE")) => PipelineStatus::Skipped,
        ("COMPLETED", _) => PipelineStatus::Failed,
        ("IN_PROGRESS", _) => PipelineStatus::Running,
        // Waiting for a deployment's approval (a protected environment): GitLab's manual job.
        ("WAITING", _) => PipelineStatus::Manual,
        _ => PipelineStatus::Pending,
    }
}

impl From<CheckContext> for Check {
    fn from(c: CheckContext) -> Self {
        match c {
            CheckContext::CheckRun { database_id, name, status, conclusion, started_at, completed_at, details_url, check_suite, deployment } => Check {
                deployed: deployment.is_some(),
                id: database_id,
                stage: check_suite.and_then(|s| s.workflow_run).map(|r| r.workflow.name).unwrap_or_else(|| "checks".into()),
                status: run_status(&status, conclusion.as_deref()),
                name,
                url: details_url.unwrap_or_default(),
                started_at,
                finished_at: completed_at.filter(|_| status == "COMPLETED"),
            },
            CheckContext::StatusContext { context, state, target_url, created_at } => {
                let status = rollup_status(&state);
                Check { id: None, stage: "statuses".into(), name: context, status, url: target_url.unwrap_or_default(), started_at: created_at, finished_at: created_at.filter(|_| status.is_finished()), deployed: false }
            }
        }
    }
}

/// A commit's pipeline id: its first 12 hex digits (48 bits), from which `sha_of` gets them back.
pub fn pipeline_id(oid: &str) -> u64 {
    oid.get(..12).and_then(|h| u64::from_str_radix(h, 16).ok()).unwrap_or_default()
}

/// The abbreviated SHA of a pipeline's commit: GitHub resolves it.
pub fn sha_of(pipeline: u64) -> String {
    format!("{pipeline:012x}")
}

/// The checks of a commit as a pipeline, one stage per workflow; none without checks. Its id
/// comes from the commit (unique, stable): GitHub has no pipeline gathering a commit's checks.
fn pipeline(c: GqlCommit, target: String, url: String) -> Option<Pipeline> {
    let rollup = c.status_check_rollup?;
    let checks: Vec<Check> = rollup.contexts.map(|n| n.nodes).unwrap_or_default().into_iter().map(Check::from).collect();
    let mut names: Vec<&str> = Vec::new();
    for ch in &checks {
        if !names.contains(&ch.stage.as_str()) {
            names.push(&ch.stage);
        }
    }
    let stages: Vec<Stage> = names
        .iter()
        .map(|name| {
            let jobs: Vec<&Check> = checks.iter().filter(|ch| ch.stage == *name).collect();
            let failed_jobs: Vec<JobLink> = jobs.iter().filter(|j| j.status == PipelineStatus::Failed).map(|j| JobLink { name: j.name.clone(), url: j.url.clone() }).collect();
            let finished = jobs.iter().filter(|j| j.status.is_finished()).count() as u32;
            let all_done = finished as usize == jobs.len();
            let any = |s: PipelineStatus| jobs.iter().any(|j| j.status == s);
            // The first of these any job is in, else skipped when all were, else a success.
            let status = [PipelineStatus::Failed, PipelineStatus::Running, PipelineStatus::Pending, PipelineStatus::Manual, PipelineStatus::Canceled]
                .into_iter()
                .find(|s| any(*s))
                .unwrap_or(if jobs.iter().all(|j| j.status == PipelineStatus::Skipped) { PipelineStatus::Skipped } else { PipelineStatus::Success });
            Stage {
                name: name.to_string(),
                status,
                jobs: jobs.len() as u32,
                finished,
                failed: failed_jobs.len() as u32,
                failed_jobs,
                // Those awaiting a deployment's approval: approving one plays it.
                manual_jobs: jobs.iter().filter(|j| j.status == PipelineStatus::Manual).filter_map(|j| Some(ManualJob { id: j.id?, name: j.name.clone(), url: j.url.clone() })).collect(),
                // Those that deployed and are finished: a re-run deploys the same commit again.
                played_jobs: jobs
                    .iter()
                    .filter(|j| j.deployed && j.status.is_finished() && j.status != PipelineStatus::Skipped)
                    .filter_map(|j| Some(PlayedJob { id: j.id?, name: j.name.clone(), url: j.url.clone(), status: j.status, finished_at: j.finished_at }))
                    .collect(),
                started_at: jobs.iter().filter_map(|j| j.started_at).min(),
                finished_at: all_done.then(|| jobs.iter().filter_map(|j| j.finished_at).max()).flatten(),
            }
        })
        .collect();
    let updated_at = checks.iter().filter_map(|ch| ch.finished_at.or(ch.started_at)).max().unwrap_or(c.committed_date);
    Some(Pipeline {
        id: pipeline_id(&c.oid),
        repo: None,
        project: None,
        target,
        status: rollup_status(&rollup.state),
        url,
        updated_at,
        failed_jobs: stages.iter().flat_map(|s| s.failed_jobs.iter().map(|j| j.name.clone())).collect(),
        stages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quota_sets_the_pace() {
        let now = Utc::now();
        let every = Duration::from_secs(15);
        assert_eq!(pace(every, None, now), every);
        assert_eq!(pace(every, Some((4000, now)), now), every);
        assert_eq!(pace(every, Some((800, now)), now), Duration::from_secs(60));
        let reset = now + chrono::Duration::minutes(5);
        assert_eq!(pace(every, Some((50, reset)), now).as_secs(), 300);
        assert_eq!(pace(every, Some((50, now + chrono::Duration::hours(1))), now), Duration::from_secs(900), "15 min at most");
    }

    /// A review asked of one of my teams is mine; one asked of another team is nobody's here.
    #[test]
    fn reviews_asked_of_my_teams() {
        let mut p: GqlPr = serde_json::from_str(&PR.replace(r#"{ "requestedReviewer": {} }"#, r#"{ "requestedReviewer": { "combinedSlug": "acme/core" } }, { "requestedReviewer": { "combinedSlug": "acme/other" } }"#)).unwrap();
        let me = Identity { login: Some("dee".into()), teams: vec!["acme/core".into()], read: None };
        me.answer_for_teams(&mut p);
        assert_eq!(MergeRequest::from(p).reviewers, ["bob", "dee", "carl"]);
    }

    /// A check run still queued waits for a runner, since its workflow run started.
    #[test]
    fn queued_jobs_wait_for_a_runner() {
        let p: GqlPr = serde_json::from_str(
            &PR.replace(r#""name": "test", "status": "IN_PROGRESS""#, r#""name": "test", "status": "QUEUED""#)
                .replace(r#""https://x/test", "checkSuite": { "workflowRun": {"#, r#""https://x/test", "checkSuite": { "workflowRun": { "createdAt": "2026-10-09T09:00:30Z","#),
        )
        .unwrap();
        let jobs = queued(&p.commits.nodes[0].commit, &Some("helm".into()), "#7");
        assert_eq!(jobs.iter().map(|j| (j.name.as_str(), j.target.as_str(), j.repo.as_deref(), j.queued_since.to_rfc3339())).collect::<Vec<_>>(), [("test", "#7", Some("helm"), "2026-10-09T09:00:30+00:00".to_string())]);
    }

    /// A job awaiting a deployment's approval is a manual job of its stage, as on GitLab.
    #[test]
    fn a_job_awaiting_approval_is_manual() {
        let p: GqlPr = serde_json::from_str(&PR.replace(r#""name": "test", "status": "IN_PROGRESS""#, r#""databaseId": 77, "name": "test", "status": "WAITING""#).replace(r#""conclusion": "FAILURE""#, r#""conclusion": "SUCCESS""#)).unwrap();
        let ci = MergeRequest::from(p).head_pipeline.unwrap();
        let stage = &ci.stages[0];
        assert_eq!((stage.status.name(), stage.manual_jobs.iter().map(|j| (j.id, j.name.as_str())).collect::<Vec<_>>()), ("manual", vec![(77, "test")]));
    }

    /// A finished job that deployed can run again; a check that deployed nothing cannot.
    #[test]
    fn a_finished_deployment_job_can_run_again() {
        let p: GqlPr = serde_json::from_str(
            &PR.replace(r#""name": "test", "status": "IN_PROGRESS", "conclusion": null, "startedAt": "2026-10-09T09:01:00Z", "completedAt": null"#, r#""databaseId": 78, "name": "test", "status": "COMPLETED", "conclusion": "SUCCESS", "startedAt": "2026-10-09T09:01:00Z", "completedAt": "2026-10-09T09:03:00Z", "deployment": { "databaseId": 5 }"#),
        )
        .unwrap();
        let ci = MergeRequest::from(p).head_pipeline.unwrap();
        let played: Vec<_> = ci.stages.iter().flat_map(|s| &s.played_jobs).map(|j| (j.id, j.name.as_str(), j.status)).collect();
        assert_eq!(played, [(78, "test", PipelineStatus::Success)]);
    }

    #[test]
    fn a_pipeline_id_gives_back_its_commit() {
        assert_eq!(sha_of(pipeline_id("06ea10e3fa9998797e9ccea325272b9d9c02e099")), "06ea10e3fa99");
    }

    #[test]
    fn statuses_in_the_windows_terms() {
        assert_eq!(run_status("COMPLETED", Some("SUCCESS")).name(), "success");
        assert_eq!(run_status("COMPLETED", Some("TIMED_OUT")).name(), "failed");
        assert_eq!(run_status("COMPLETED", Some("NEUTRAL")).name(), "skipped");
        assert_eq!(run_status("QUEUED", None).name(), "pending");
        assert_eq!(merge_status("BEHIND", None), "need_rebase");
        assert_eq!(merge_status("BLOCKED", Some("REVIEW_REQUIRED")), "not_approved");
        assert_eq!(approvals_left(Some("APPROVED"), None, 1), 0);
        assert_eq!(approvals_left(Some("REVIEW_REQUIRED"), None, 0), 1, "rules unreadable: one while GitHub asks");
        assert_eq!(approvals_left(Some("REVIEW_REQUIRED"), Some(2), 0), 2);
        assert_eq!(approvals_left(Some("REVIEW_REQUIRED"), Some(2), 2), 1, "met, yet a code owner is still asked");
        assert_eq!(approvals_left(None, Some(0), 0), 0);
        let rules: BaseRef = serde_json::from_str(r#"{ "branchProtectionRule": { "requiredApprovingReviewCount": 1 }, "rules": { "nodes": [{ "type": "PULL_REQUEST", "parameters": { "requiredApprovingReviewCount": 2 } }, { "type": "DELETION", "parameters": null }] } }"#).unwrap();
        assert_eq!(rules.required(), Some(2));
        assert_eq!(BaseRef::default().required(), None);
    }

    /// A pull request as GitHub gives it.
    const PR: &str = r#"{
                "number": 7, "title": "ABC-12 Fix the thing", "isDraft": false, "url": "https://github.com/o/r/pull/7",
                "headRefName": "fix/thing", "baseRefName": "main", "updatedAt": "2026-10-09T10:00:00Z", "mergedAt": null, "state": "OPEN",
                "author": { "login": "ann", "name": "Ann" }, "assignees": { "nodes": [{ "login": "ann" }] },
                "reviewRequests": { "nodes": [{ "requestedReviewer": { "login": "bob" } }, { "requestedReviewer": {} }] },
                "latestOpinionatedReviews": { "nodes": [{ "state": "APPROVED", "author": { "login": "carl" } }] },
                "reviewDecision": "REVIEW_REQUIRED", "mergeable": "MERGEABLE", "mergeStateStatus": "BLOCKED", "autoMergeRequest": null,
                "labels": { "nodes": [{ "name": "bug" }] }, "reviewThreads": { "nodes": [{ "isResolved": false }, { "isResolved": true }] },
                "additions": 3, "deletions": 1, "changedFiles": 2,
                "commits": { "nodes": [{ "commit": { "oid": "6ea10e3fa9998797e9ccea325272b9d9c02e0993", "url": "https://github.com/o/r/commit/6ea1", "committedDate": "2026-10-09T09:00:00Z",
                    "statusCheckRollup": { "state": "FAILURE", "contexts": { "nodes": [
                        { "__typename": "CheckRun", "name": "lint", "status": "COMPLETED", "conclusion": "FAILURE", "startedAt": "2026-10-09T09:01:00Z", "completedAt": "2026-10-09T09:02:00Z", "detailsUrl": "https://x/lint", "checkSuite": { "workflowRun": { "workflow": { "name": "CI" } } } },
                        { "__typename": "CheckRun", "name": "test", "status": "IN_PROGRESS", "conclusion": null, "startedAt": "2026-10-09T09:01:00Z", "completedAt": null, "detailsUrl": "https://x/test", "checkSuite": { "workflowRun": { "workflow": { "name": "CI" } } } },
                        { "__typename": "StatusContext", "context": "ci/legacy", "state": "SUCCESS", "targetUrl": null, "createdAt": "2026-10-09T09:03:00Z" }
                    ] } } } }] }
            }"#;

    /// A pull request as GitHub gives it: one stage per workflow, commit statuses apart.
    #[test]
    fn a_pull_request_and_its_checks() {
        let json = PR;
        let m = MergeRequest::from(serde_json::from_str::<GqlPr>(json).unwrap());
        assert_eq!((m.iid, m.state, m.author_name.as_str()), (7, MrState::Opened, "Ann"));
        assert_eq!(m.reviewers, ["bob", "carl"]);
        assert_eq!((m.approved_by.as_slice(), m.approvals_left, m.open_threads), (&["carl".to_string()][..], 1, 1));
        assert_eq!(m.merge_status, "not_approved");
        let p = m.head_pipeline.unwrap();
        assert_eq!((p.status.name(), p.url.as_str(), p.failed_jobs.as_slice()), ("failed", "https://github.com/o/r/pull/7/checks", &["lint".to_string()][..]));
        assert_eq!(p.stages.iter().map(|s| (s.name.as_str(), s.status.name(), s.jobs, s.finished)).collect::<Vec<_>>(), [("CI", "failed", 2, 1), ("statuses", "success", 1, 1)]);
    }

    /// Against GitHub itself: a list of followed branches, a name, a pattern and the default
    /// branch in one query; a missing name reported.
    #[tokio::test]
    #[ignore = "needs gh logged in, and the network"]
    async fn resolves_followed_branches() {
        let repo = PipelineRepo { name: "cli".into(), repo: None, project: "cli/cli".into(), main_branch: String::new(), release_prefix: String::new(), branches: vec!["@main".into(), "8761/*".into(), "no-such-branch-here".into(), "trunk".into()] };
        let got = pipeline_branches(&repo).await.unwrap();
        assert_eq!(got.refs, ["trunk", "8761/allow-multiple-items-in-nested-array"]);
        assert_eq!(got.missing, ["no-such-branch-here"]);
        // No prefix: every page read (cli/cli has some 260 branches), the last ones found.
        let repo = PipelineRepo { branches: vec!["*-spike".into()], ..repo };
        assert!(pipeline_branches(&repo).await.unwrap().refs.contains(&"workflow-test-spike".to_string()));
    }

    /// Against GitHub itself: a public repository's pull requests, branch checks and a review.
    #[tokio::test]
    #[ignore = "needs gh logged in, and the network"]
    async fn reads_a_public_repository() {
        let mut me = Identity::default();
        me.refresh().await;
        let mut followed = Followed { repo: None, project: "cli/cli".into(), merged: None, open: Vec::new() };
        let read = followed.read(Utc::now() - chrono::Duration::days(7), &me).await.unwrap();
        let mrs = read.mrs;
        assert!(me.login.is_some() && mrs.iter().any(|m| m.state == MrState::Opened) && mrs.iter().any(|m| m.state == MrState::Merged));
        assert!(read.quota.is_some_and(|(left, _)| left > 0));
        // The merged ones come from the cache the next time.
        let again = followed.read(Utc::now() - chrono::Duration::days(7), &me).await.unwrap();
        assert_eq!(again.mrs.iter().filter(|m| m.state == MrState::Merged).count(), mrs.iter().filter(|m| m.state == MrState::Merged).count());
        let repo = PipelineRepo { name: "cli".into(), repo: None, project: "cli/cli".into(), main_branch: String::new(), release_prefix: String::new(), branches: Vec::new() };
        let refs = protected_refs(&repo).await.unwrap();
        assert_eq!(refs, ["trunk"]);
        let (pipelines, _) = branch_pipelines(&repo, &refs).await.unwrap();
        assert!(pipelines[0].stages.iter().any(|s| s.jobs > 0), "{:?}", pipelines[0]);
        let open = mrs.iter().find(|m| m.state == MrState::Opened).unwrap().clone();
        let hub = Hub::new();
        hub.update(|s| {
            s.merge_requests = mrs.clone();
            true
        });
        let detail = review::detail("cli/cli", &hub, None, open.iid).await.unwrap();
        let diff = review::diff("cli/cli", &hub, None, open.iid).await.unwrap();
        assert!(!detail.description.is_empty() || !diff.files.is_empty());
        let path = diff.files.iter().find(|f| !f.deleted_file).unwrap().new_path.clone();
        review::file("cli/cli", &hub, None, open.iid, &path).await.unwrap();
        assert!(!review::members("cli/cli").await.unwrap().is_empty());
        assert!(review::detail("cli/cli", &hub, None, 1).await.is_err(), "a pull request Prism does not display");
    }
}
