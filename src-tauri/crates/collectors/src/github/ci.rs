//! A commit's checks on demand: the GitHub Actions jobs of a pipeline (a commit, see
//! `pipeline_id`), a job's log, retrying a job, retrying or canceling a pipeline's workflow runs.
//! Logs are read when the window displays them, never kept (they may hold badly masked secrets).

use prism_base::{i18n::tr, tr};
use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{api, gh_bytes, gh_input, graphql, pipeline_id, run_status, sha_of, split, Nodes};
use crate::{
    forge::{
        shown::{pipeline_status, set_pipeline_status},
        JobInfo, JobLog,
    },
    state::{Hub, PipelineStatus},
};

const CHECKS_QUERY: &str = include_str!("commit_checks.graphql");
/// Beyond this, only the end of the log is kept: that is where the error is.
const LOG_MAX_BYTES: usize = 512 * 1024;

#[derive(Deserialize)]
struct Data {
    repository: Option<Repository>,
}

#[derive(Deserialize)]
struct Repository {
    object: Option<Commit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    oid: String,
    check_suites: Nodes<Suite>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Suite {
    workflow_run: Option<Run>,
    check_runs: Nodes<CheckRun>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Run {
    database_id: u64,
    workflow: Workflow,
}

#[derive(Deserialize)]
struct Workflow {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckRun {
    database_id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    details_url: Option<String>,
}

/// The workflow runs of a pipeline's commit, with their jobs: checks from other apps have no log.
async fn runs(project: &str, pipeline: u64) -> Result<(String, Vec<(Run, Vec<CheckRun>)>)> {
    let (owner, name) = split(project)?;
    let sha = sha_of(pipeline);
    let data: Data = graphql(CHECKS_QUERY, &[("owner", owner.into()), ("name", name.into()), ("sha", sha.as_str().into())]).await?;
    let commit = data.repository.context(tr("repository not found or not accessible"))?.object.with_context(|| format!("no commit {sha}"))?;
    let runs = commit.check_suites.nodes.into_iter().filter_map(|s| Some((s.workflow_run?, s.check_runs.nodes))).collect();
    Ok((commit.oid, runs))
}

fn duration(started: Option<DateTime<Utc>>, finished: Option<DateTime<Utc>>) -> Option<f64> {
    Some((finished? - started?).num_milliseconds().max(0) as f64 / 1000.0)
}

/// A pipeline's jobs, workflow by workflow.
pub async fn pipeline_jobs(project: &str, pipeline: u64) -> Result<Vec<JobInfo>> {
    let (_, runs) = runs(project, pipeline).await?;
    Ok(runs
        .into_iter()
        .flat_map(|(run, jobs)| {
            jobs.into_iter().map(move |j| {
                let finished = j.completed_at.filter(|_| j.status == "COMPLETED");
                JobInfo {
                    id: j.database_id,
                    status: run_status(&j.status, j.conclusion.as_deref()),
                    stage: run.workflow.name.clone(),
                    name: j.name,
                    allow_failure: false,
                    duration: duration(j.started_at, finished),
                    started_at: j.started_at,
                    finished_at: finished,
                    url: j.details_url.unwrap_or_default(),
                    pipeline_id: pipeline,
                    r#ref: String::new(),
                }
            })
        })
        .collect())
}

/// A job as the REST API gives it.
#[derive(Deserialize)]
struct RawJob {
    id: u64,
    run_id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    html_url: String,
    #[serde(default)]
    head_branch: Option<String>,
    head_sha: String,
    #[serde(default)]
    workflow_name: Option<String>,
}

impl From<RawJob> for JobInfo {
    fn from(j: RawJob) -> Self {
        let done = j.status == "completed";
        let finished = j.completed_at.filter(|_| done);
        JobInfo {
            id: j.id,
            status: run_status(&j.status.to_uppercase(), j.conclusion.map(|c| c.to_uppercase()).as_deref()),
            name: j.name,
            stage: j.workflow_name.unwrap_or_default(),
            allow_failure: false,
            duration: duration(j.started_at, finished),
            started_at: j.started_at,
            finished_at: finished,
            url: j.html_url,
            pipeline_id: pipeline_id(&j.head_sha),
            r#ref: j.head_branch.unwrap_or_default(),
        }
    }
}

/// GitHub's log: a timestamp opens each line; the window has its own.
fn clean(log: &str) -> String {
    static STAMP: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(?m)^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d+)?Z ").unwrap());
    STAMP.replace_all(log.trim_start_matches('\u{feff}'), "").into_owned()
}

pub async fn job_log(project: &str, job: u64) -> Result<JobLog> {
    split(project)?;
    let info: RawJob = api(&format!("repos/{project}/actions/jobs/{job}")).await?;
    // A job not started yet has no log.
    let log = if info.status == "queued" || info.status == "waiting" || info.status == "pending" {
        String::new()
    } else {
        let bytes = gh_bytes(&["api".into(), "--allow-escape-sequences".into(), format!("repos/{project}/actions/jobs/{job}/logs")]).await?;
        clean(&String::from_utf8_lossy(&bytes))
    };
    let (log, truncated) = crate::gitlab::logs::tail(log, LOG_MAX_BYTES);
    Ok(JobLog { job: info.into(), log, truncated })
}

/// Retries a finished job: GitHub runs it again in a new attempt of its workflow run, under a
/// new id, which is returned once it appears.
pub async fn retry_job(project: &str, job: u64) -> Result<JobInfo> {
    split(project)?;
    let current: RawJob = api(&format!("repos/{project}/actions/jobs/{job}")).await?;
    ensure!(current.status == "completed", "{}", tr!("job {job} is still {status}", job = job, status = current.status));
    tracing::info!("github: retry job {job}");
    let _: serde_json::Value = gh_input("POST", &format!("repos/{project}/actions/jobs/{job}/rerun"), &serde_json::json!({})).await?;
    #[derive(Deserialize)]
    struct Jobs {
        jobs: Vec<RawJob>,
    }
    let (run, name) = (current.run_id, current.name.clone());
    for _ in 0..5 {
        let latest: Jobs = api(&format!("repos/{project}/actions/runs/{run}/jobs?filter=latest&per_page=100")).await?;
        if let Some(j) = latest.jobs.into_iter().find(|j| j.name == name && j.id != job) {
            return Ok(j.into());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    // Not visible yet: the job as it was, waiting.
    let mut info = JobInfo::from(current);
    info.status = PipelineStatus::Pending;
    Ok(info)
}

/// Approves the deployments a job is waiting for (a protected environment), those I may
/// approve; the job then runs. GitLab's "play" of a manual job.
pub async fn approve(project: &str, hub: &Hub, job: u64) -> Result<()> {
    #[derive(Deserialize)]
    struct Pending {
        environment: Environment,
        current_user_can_approve: bool,
    }
    #[derive(Deserialize)]
    struct Environment {
        id: u64,
        name: String,
    }
    split(project)?;
    let info: RawJob = api(&format!("repos/{project}/actions/jobs/{job}")).await?;
    ensure!(info.status == "waiting", "{}", tr!("job {job} is not waiting for an approval ({status})", job = job, status = info.status));
    let pending: Vec<Pending> = api(&format!("repos/{project}/actions/runs/{}/pending_deployments", info.run_id)).await?;
    let (mine, others): (Vec<&Pending>, Vec<&Pending>) = pending.iter().partition(|p| p.current_user_can_approve);
    if mine.is_empty() {
        let names: Vec<&str> = others.iter().map(|p| p.environment.name.as_str()).collect();
        bail!("{}", tr!("you are not a reviewer of {what}: approve it on GitHub, or ask one", what = if names.is_empty() { tr("this deployment").to_string() } else { names.join(", ") }));
    }
    let ids: Vec<u64> = mine.iter().map(|p| p.environment.id).collect();
    tracing::info!("github: approve deployments {ids:?} of run {}", info.run_id);
    let _: serde_json::Value = gh_input(
        "POST",
        &format!("repos/{project}/actions/runs/{}/pending_deployments", info.run_id),
        &serde_json::json!({ "environment_ids": ids, "state": "approved", "comment": "Approved from Prism" }),
    )
    .await?;
    // Without waiting for the next collection: the job leaves the jobs to play.
    hub.update(|s| {
        let mut changed = false;
        for p in s.pipelines.iter_mut().chain(s.merge_requests.iter_mut().filter_map(|m| m.head_pipeline.as_mut())) {
            for st in p.stages.iter_mut() {
                let before = st.manual_jobs.len();
                st.manual_jobs.retain(|j| j.id != job);
                if st.manual_jobs.len() != before {
                    changed = true;
                    if st.status == PipelineStatus::Manual {
                        st.status = PipelineStatus::Pending;
                    }
                }
            }
        }
        changed
    });
    Ok(())
}

/// Cancels the commit's workflow runs still going.
pub async fn cancel_pipeline(project: &str, hub: &Hub, id: u64) -> Result<()> {
    match pipeline_status(hub, id) {
        None => bail!("{}", tr!("pipeline {id} is not one Prism displays", id = id)),
        Some(s) if !s.can_cancel() => bail!("{}", tr!("pipeline {id} is already {status}", id = id, status = s)),
        Some(_) => {}
    }
    let (_, runs) = runs(project, id).await?;
    let going: Vec<u64> = runs.iter().filter(|(_, jobs)| jobs.iter().any(|j| j.status != "COMPLETED")).map(|(r, _)| r.database_id).collect();
    ensure!(!going.is_empty(), "{}", tr("no workflow run is going"));
    for run in going {
        tracing::info!("github: cancel run {run}");
        let _: serde_json::Value = gh_input("POST", &format!("repos/{project}/actions/runs/{run}/cancel"), &serde_json::json!({})).await?;
    }
    set_pipeline_status(hub, id, PipelineStatus::Canceled, false);
    Ok(())
}

/// Runs again the failed and canceled jobs of the commit's workflow runs.
pub async fn retry_pipeline(project: &str, hub: &Hub, id: u64) -> Result<()> {
    match pipeline_status(hub, id) {
        None => bail!("{}", tr!("pipeline {id} is not one Prism displays", id = id)),
        Some(s) if !s.can_retry() => bail!("{}", tr!("pipeline {id} is {status}: nothing to retry", id = id, status = s)),
        Some(_) => {}
    }
    let (_, runs) = runs(project, id).await?;
    let failed: Vec<u64> = runs
        .iter()
        .filter(|(_, jobs)| jobs.iter().any(|j| run_status(&j.status, j.conclusion.as_deref()).can_retry()))
        .map(|(r, _)| r.database_id)
        .collect();
    ensure!(!failed.is_empty(), "{}", tr("no failed workflow run to retry (a check from another app is retried there)"));
    for run in failed {
        tracing::info!("github: retry run {run}");
        let _: serde_json::Value = gh_input("POST", &format!("repos/{project}/actions/runs/{run}/rerun-failed-jobs"), &serde_json::json!({})).await?;
    }
    set_pipeline_status(hub, id, PipelineStatus::Pending, true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_without_github_timestamps() {
        let raw = "\u{feff}2026-10-09T19:39:51.1037226Z Current runner\n2026-10-09T19:39:51.1062479Z ##[group]Image\nno stamp 2026-10-09T19:39:51Z kept\n";
        assert_eq!(clean(raw), "Current runner\n##[group]Image\nno stamp 2026-10-09T19:39:51Z kept\n");
    }

    /// Against GitHub itself: a public commit's jobs and a log.
    #[tokio::test]
    #[ignore = "needs gh logged in, and the network"]
    async fn reads_jobs_and_a_log() {
        let pipeline = pipeline_id("6ea10e3fa9998797e9ccea325272b9d9c02e0993");
        let jobs = pipeline_jobs("cli/cli", pipeline).await.unwrap();
        let lint = jobs.iter().find(|j| j.name == "lint").unwrap();
        assert_eq!((lint.stage.as_str(), lint.status), ("Lint", PipelineStatus::Success));
        let log = job_log("cli/cli", lint.id).await.unwrap();
        assert!(log.log.starts_with("Current runner") && log.job.pipeline_id == pipeline, "{}", &log.log[..80.min(log.log.len())]);
    }
}
