//! GitLab's side of CI health (`collectors::ci_health`): its pipelines over the window, for
//! `main`, the `release/*` branches and MR pipelines, with their jobs.
//!
//! One GraphQL query brings back 50 pipelines with their jobs. Only the pipelines updated
//! since the previous pass are re-read; the others stay cached as long as they are within
//! the window.

use prism_base::i18n::tr;
use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{api, encode, glab};
use crate::forge::cli::Graphql;
use crate::{
    ci_health::{label, scope, set_window, window_days, Job, Run, SCOPES, SOURCE},
    config::PipelineRepo,
    state::{CiHealth, Hub},
};

const QUERY: &str = include_str!("ci_health.graphql");
/// Trends over two weeks: no point re-reading more often.
const POLL: Duration = Duration::from_secs(600);
/// Safeguard: 20 pages of 50 pipelines per re-read.
const MAX_PAGES: usize = 20;

/// `repo`: the project CI health reads (`[forge] ci_health_repo`), and its branches.
pub async fn run(repo: PipelineRepo, window_days: u32, hub: Hub) {
    set_window(window_days);
    let mut cache = Cache::default();
    let mut first = true;
    loop {
        if !std::mem::take(&mut first) {
            hub.pause(POLL).await;
        }
        hub.publish(SOURCE, collect(&repo, &mut cache).await, |s, health| crate::state::replace(&mut s.ci_health, Some(health)));
    }
}

#[derive(Default)]
struct Cache {
    /// Per scope, then per pipeline.
    runs: HashMap<&'static str, HashMap<String, Run>>,
    last: Option<DateTime<Utc>>,
}

/// The pipelines of a scope: by branch, or by source (`merge_request_event`).
enum Filter {
    Ref(String),
    Source(&'static str),
}

/// A project's own fields, for its default branch.
#[derive(Deserialize)]
struct ProjectInfo {
    default_branch: Option<String>,
}

async fn collect(repo: &PipelineRepo, cache: &mut Cache) -> Result<CiHealth> {
    let now = Utc::now();
    let start = now - chrono::Duration::days(window_days());
    // A margin: a pipeline updated during the previous re-read is not lost.
    let since = cache.last.map_or(start, |l| (l - chrono::Duration::minutes(30)).max(start));
    // Without a release prefix (no Git Flow), no release branches.
    let releases: Vec<RawBranch> = if repo.release_prefix.is_empty() {
        Vec::new()
    } else {
        api(&format!("projects/{}/repository/branches?search={}&per_page=20", encode(&repo.project), encode(&format!("^{}", repo.release_prefix)))).await?
    };
    // No main branch declared: the project's default one.
    let main = if repo.main_branch.is_empty() {
        api::<ProjectInfo>(&format!("projects/{}", encode(&repo.project))).await?.default_branch.unwrap_or_else(|| "main".into())
    } else {
        repo.main_branch.clone()
    };
    for key in SCOPES {
        let filters = match key {
            "main" => vec![Filter::Ref(main.clone())],
            "release" => releases.iter().map(|b| Filter::Ref(b.name.clone())).collect(),
            _ => vec![Filter::Source("merge_request_event")],
        };
        let runs = cache.runs.entry(key).or_default();
        for f in &filters {
            runs.extend(fetch(&repo.project, f, since).await?);
        }
        runs.retain(|_, r| r.at >= start);
    }
    cache.last = Some(now);
    let scopes = SCOPES
        .iter()
        .map(|key| scope(key, &label(key, &main, &repo.release_prefix), cache.runs.get(key).map(|m| m.values().collect()).unwrap_or_default(), now))
        .collect();
    Ok(CiHealth { window_days: window_days() as u32, scopes })
}

async fn fetch(project: &str, filter: &Filter, since: DateTime<Utc>) -> Result<Vec<(String, Run)>> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut args: Vec<String> = vec![
            "api".into(), "graphql".into(),
            "-f".into(), format!("query={QUERY}"),
            "-f".into(), format!("path={project}"),
            "-f".into(), format!("after={}", since.to_rfc3339()),
        ];
        match filter {
            Filter::Ref(r) => args.extend(["-f".into(), format!("ref={r}")]),
            Filter::Source(s) => args.extend(["-f".into(), format!("source={s}")]),
        }
        if let Some(c) = &cursor {
            args.extend(["-f".into(), format!("cursor={c}")]);
        }
        let resp: Graphql<Data> = glab(&args).await?;
        let page = resp.into_data()?.project.context(tr("project not found or not accessible"))?.pipelines;
        out.extend(page.nodes.into_iter().map(|p| {
            let jobs = p.jobs.nodes.into_iter().map(|j| Job {
                name: j.name,
                status: j.status.to_lowercase(),
                duration: j.duration,
                queued: j.queued_duration,
                retried: j.retried.unwrap_or(false),
                url: j.web_path.map(|w| crate::conventions::web(&w)),
            });
            (p.id, Run { at: p.created_at, status: p.status.to_lowercase(), duration: p.duration, jobs: jobs.collect() })
        }));
        match page.page_info.end_cursor {
            Some(c) if page.page_info.has_next_page => cursor = Some(c),
            _ => break,
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct RawBranch {
    name: String,
}

#[derive(Deserialize)]
struct Data {
    project: Option<Project>,
}

#[derive(Deserialize)]
struct Project {
    pipelines: Page,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    page_info: PageInfo,
    nodes: Vec<RawPipeline>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPipeline {
    id: String,
    status: String,
    created_at: DateTime<Utc>,
    duration: Option<f64>,
    jobs: Jobs,
}

#[derive(Deserialize)]
struct Jobs {
    nodes: Vec<RawJob>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawJob {
    name: String,
    status: String,
    duration: Option<f64>,
    queued_duration: Option<f64>,
    retried: Option<bool>,
    web_path: Option<String>,
}
