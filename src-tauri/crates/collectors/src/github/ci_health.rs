//! GitHub's side of CI health (`collectors::ci_health`): the GitHub Actions workflow runs of the
//! window, each one a pipeline: pushes to the main branch and to the release branches, and pull
//! requests. Scheduled runs and those of other events are left out, as GitLab has no such
//! pipelines on a branch.
//!
//! The REST API lists the runs (its own quota, apart from GraphQL's); a run's jobs, all attempts
//! included, are read once it is completed, then kept while it is in the window, unless it is
//! run again.

use std::{collections::HashMap, time::Duration};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{api, split};
use crate::{
    ci_health::{label, scope, set_window, window_days, Job, Run, SCOPES, SOURCE},
    config::PipelineRepo,
    state::{CiHealth, Hub},
};

/// Trends over two weeks: no point re-reading more often.
const POLL: Duration = Duration::from_secs(600);
/// Safeguard: runs read per scope and branch (pages of 100).
const MAX_PAGES: usize = 5;

/// `repo`: the project CI health reads (`[forge] ci_health_repo`), and its branches.
pub async fn run(repo: PipelineRepo, window: u32, hub: Hub) {
    set_window(window);
    let mut cache = Cache::default();
    let mut first = true;
    loop {
        if !std::mem::take(&mut first) {
            hub.pause(POLL).await;
        }
        hub.publish(SOURCE, collect(&repo, &mut cache).await, |s, health| crate::state::replace(&mut s.ci_health, Some(health)));
    }
}

/// Per scope, the completed runs by id, with the attempt their jobs were read at.
#[derive(Default)]
struct Cache {
    runs: HashMap<&'static str, HashMap<u64, (u32, Run)>>,
}

#[derive(Deserialize)]
struct Runs {
    workflow_runs: Vec<RawRun>,
}

#[derive(Deserialize)]
struct RawRun {
    id: u64,
    status: String,
    conclusion: Option<String>,
    created_at: DateTime<Utc>,
    run_started_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    run_attempt: u32,
}

#[derive(Deserialize)]
struct Jobs {
    jobs: Vec<RawJob>,
}

#[derive(Deserialize)]
struct RawJob {
    name: String,
    conclusion: Option<String>,
    created_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    run_attempt: Option<u32>,
    html_url: Option<String>,
}

#[derive(Deserialize)]
struct MatchingRef {
    r#ref: String,
}

/// A finished run or job, in the window's (GitLab's) terms: `success`, `failed`, `canceled`, or
/// `skipped` (neither passed nor failed).
fn outcome(conclusion: Option<&str>) -> &'static str {
    match conclusion {
        Some("success") => "success",
        Some("failure" | "timed_out" | "startup_failure") => "failed",
        Some("cancelled") => "canceled",
        _ => "skipped",
    }
}

fn seconds(from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> Option<f64> {
    Some((to? - from?).num_milliseconds().max(0) as f64 / 1000.0)
}

/// A repository's own fields, for its default branch.
#[derive(Deserialize)]
struct Repository {
    default_branch: Option<String>,
}

async fn collect(repo: &PipelineRepo, cache: &mut Cache) -> Result<CiHealth> {
    let project = repo.project.as_str();
    split(project)?;
    let now = Utc::now();
    let start = now - chrono::Duration::days(window_days());
    // Without a release prefix (no Git Flow), no release branches.
    let releases: Vec<String> = if repo.release_prefix.is_empty() {
        Vec::new()
    } else {
        let refs: Vec<MatchingRef> = api(&format!("repos/{project}/git/matching-refs/heads/{}", repo.release_prefix)).await?;
        refs.into_iter().filter_map(|r| r.r#ref.strip_prefix("refs/heads/").map(String::from)).collect()
    };
    // No main branch declared: the repository's default one.
    let main = if repo.main_branch.is_empty() {
        api::<Repository>(&format!("repos/{project}")).await?.default_branch.unwrap_or_else(|| "main".into())
    } else {
        repo.main_branch.clone()
    };
    for key in SCOPES {
        let filters: Vec<String> = match key {
            "main" => vec![format!("branch={main}&event=push")],
            "release" => releases.iter().map(|b| format!("branch={b}&event=push")).collect(),
            _ => vec!["event=pull_request".into()],
        };
        let runs = cache.runs.entry(key).or_default();
        for f in &filters {
            for raw in list(project, f, start).await? {
                if raw.status != "completed" || runs.get(&raw.id).is_some_and(|(attempt, _)| *attempt == raw.run_attempt) {
                    continue;
                }
                let jobs: Jobs = api(&format!("repos/{project}/actions/runs/{}/jobs?filter=all&per_page=100", raw.id)).await?;
                runs.insert(raw.id, (raw.run_attempt, to_run(&raw, jobs.jobs)));
            }
        }
        runs.retain(|_, (_, r)| r.at >= start);
    }
    let scopes = SCOPES
        .iter()
        .map(|key| scope(key, &label(key, &main, &repo.release_prefix), cache.runs.get(key).map(|m| m.values().map(|(_, r)| r).collect()).unwrap_or_default(), now))
        .collect();
    Ok(CiHealth { window_days: window_days() as u32, scopes })
}

/// The runs of a filter created in the window, most recent first.
async fn list(project: &str, filter: &str, start: DateTime<Utc>) -> Result<Vec<RawRun>> {
    let mut out = Vec::new();
    for page in 1..=MAX_PAGES {
        let batch: Runs = api(&format!("repos/{project}/actions/runs?{filter}&created=>={}&per_page=100&page={page}", start.format("%Y-%m-%d"))).await?;
        let last = batch.workflow_runs.len() < 100;
        out.extend(batch.workflow_runs);
        if last {
            break;
        }
    }
    Ok(out)
}

/// A run and its jobs: an attempt below the run's is a retried one.
fn to_run(raw: &RawRun, jobs: Vec<RawJob>) -> Run {
    Run {
        at: raw.created_at,
        status: outcome(raw.conclusion.as_deref()).into(),
        duration: seconds(raw.run_started_at.or(Some(raw.created_at)), Some(raw.updated_at)),
        jobs: jobs
            .into_iter()
            .map(|j| Job {
                status: outcome(j.conclusion.as_deref()).into(),
                duration: seconds(j.started_at, j.completed_at),
                queued: seconds(j.created_at, j.started_at),
                retried: j.run_attempt.is_some_and(|a| a < raw.run_attempt),
                url: j.html_url,
                name: j.name,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A job failed at the first attempt and passed at the second: a retried failure, as on GitLab.
    #[test]
    fn a_rerun_job_is_a_retried_attempt() {
        let raw: RawRun = serde_json::from_str(r#"{ "id": 1, "status": "completed", "conclusion": "success", "created_at": "2026-10-10T10:00:00Z", "run_started_at": "2026-10-10T10:00:10Z", "updated_at": "2026-10-10T10:10:10Z", "run_attempt": 2 }"#).unwrap();
        let jobs: Jobs = serde_json::from_str(r#"{ "jobs": [
            { "name": "test", "conclusion": "failure", "created_at": "2026-10-10T10:00:10Z", "started_at": "2026-10-10T10:00:40Z", "completed_at": "2026-10-10T10:03:40Z", "run_attempt": 1, "html_url": "u1" },
            { "name": "test", "conclusion": "success", "created_at": "2026-10-10T10:05:00Z", "started_at": "2026-10-10T10:05:20Z", "completed_at": "2026-10-10T10:08:20Z", "run_attempt": 2, "html_url": "u2" }
        ] }"#).unwrap();
        let run = to_run(&raw, jobs.jobs);
        assert_eq!((run.status.as_str(), run.duration), ("success", Some(600.0)));
        assert_eq!(run.jobs.iter().map(|j| (j.status.as_str(), j.retried, j.queued, j.duration)).collect::<Vec<_>>(), [("failed", true, Some(30.0), Some(180.0)), ("success", false, Some(20.0), Some(180.0))]);
        assert_eq!(outcome(Some("cancelled")), "canceled");
        assert_eq!(outcome(Some("neutral")), "skipped");
    }

    /// Against GitHub itself: a public repository's runs make a CI health (3 days: some 50 calls).
    #[tokio::test]
    #[ignore = "needs gh logged in, and the network"]
    async fn reads_a_public_repositorys_ci() {
        let cfg = crate::config::parsed("[forge]\nprovider = \"github\"\n");
        crate::conventions::set(crate::conventions::Conventions::of(&cfg));
        set_window(3);
        let mut cache = Cache::default();
        let repo = PipelineRepo { name: "cli".into(), repo: None, project: "cli/cli".into(), main_branch: "trunk".into(), release_prefix: String::new(), branches: Vec::new() };
        let health = collect(&repo, &mut cache).await.unwrap();
        assert_eq!(health.scopes.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(), ["trunk", "*", "PRs"]);
        assert!(health.scopes[0].pipelines > 0 && health.scopes[2].pipelines > 0);
        println!("{}", health.scopes.iter().map(|s| format!("{}: {} runs, success {:?}", s.label, s.pipelines + s.pipelines_prev, s.success)).collect::<Vec<_>>().join(" · "));
    }
}
