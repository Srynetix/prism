//! Renovate on GitHub: the items of its "Dependency Dashboard" issue, as on GitLab
//! (`gitlab::renovate::parse`), and how it runs. Self-hosted, Renovate is a GitHub Actions
//! workflow (`[renovate] schedule` names it; empty: the first whose name says Renovate): its last
//! run is shown, and "Run now" dispatches it. As the Renovate app, it has no workflow: "Run now"
//! ticks the dashboard's "manual job" box, which asks the app for a pass.

use prism_base::{i18n::tr, tr};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{api, gh_input, split};
use crate::{
    gitlab::renovate::{names, parse, tick},
    state::{Hub, Renovate, RenovateRun},
};

const SOURCE: &str = "renovate";
const POLL: Duration = Duration::from_secs(300);
/// While Renovate's workflow is running, its status is followed closely.
const POLL_RUNNING: Duration = Duration::from_secs(30);
/// The dashboard's box asking the Renovate app for a pass.
const MANUAL_JOB: &str = "manual job";

pub async fn run(project: String, hub: Hub) {
    let mut first = true;
    let mut every = POLL;
    loop {
        if !std::mem::take(&mut first) {
            hub.pause(every).await;
        }
        let read = collect(&project).await;
        if let Ok(r) = &read {
            every = if r.run.as_ref().is_some_and(|x| x.status.is_active()) { POLL_RUNNING } else { POLL };
        }
        hub.publish(SOURCE, read, |s, r| crate::state::replace(&mut s.renovate, Some(r)));
    }
}

#[derive(Deserialize)]
struct Found {
    items: Vec<RawIssue>,
}

#[derive(Deserialize)]
struct RawIssue {
    number: u64,
    title: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Deserialize)]
struct Workflows {
    workflows: Vec<RawWorkflow>,
}

#[derive(Deserialize)]
struct RawWorkflow {
    id: u64,
    name: String,
    state: String,
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
    head_branch: Option<String>,
    created_at: DateTime<Utc>,
    run_started_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    html_url: String,
}

/// A workflow run's state, in the window's (GitLab's) terms.
fn status(r: &RawRun) -> crate::state::PipelineStatus {
    super::run_status(&r.status.to_uppercase(), r.conclusion.as_deref().map(str::to_uppercase).as_deref())
}

/// The dashboard issue, found by its exact title (`[renovate] dashboard_title`).
async fn dashboard(project: &str) -> Result<Option<RawIssue>> {
    let title = names().dashboard_title;
    let q = format!("repo:{project} is:issue is:open in:title \"{}\"", title.replace('"', ""));
    let found: Found = super::gh_bytes(&["api".into(), "-X".into(), "GET".into(), "search/issues".into(), "-f".into(), format!("q={q}")])
        .await
        .and_then(|b| serde_json::from_slice(&b).context("unreadable gh response"))?;
    Ok(found.items.into_iter().find(|i| i.title == title))
}

/// Renovate's workflow, if it runs as one.
async fn workflow(project: &str) -> Result<Option<RawWorkflow>> {
    let all: Workflows = api(&format!("repos/{project}/actions/workflows?per_page=100")).await?;
    let name = names().schedule;
    Ok(all.workflows.into_iter().filter(|w| w.state == "active").find(|w| if name.is_empty() { w.name.to_lowercase().contains("renovate") } else { w.name == name }))
}

async fn collect(project: &str) -> Result<Renovate> {
    split(project)?;
    let issue = dashboard(project).await?;
    let workflow = workflow(project).await?;
    let last = match &workflow {
        Some(w) => {
            let runs: Runs = api(&format!("repos/{project}/actions/workflows/{}/runs?per_page=1", w.id)).await?;
            runs.workflow_runs.into_iter().next()
        }
        None => None,
    };
    Ok(Renovate {
        dashboard_url: issue.as_ref().map(|i| i.html_url.clone()),
        dashboard_iid: issue.as_ref().map(|i| i.number),
        sections: issue.and_then(|i| i.body).map(|b| parse(&b)).unwrap_or_default(),
        schedule_id: workflow.as_ref().map(|w| w.id),
        schedule_ref: last.as_ref().and_then(|r| r.head_branch.clone()),
        run: last.map(|r| RenovateRun { id: r.id, status: status(&r), started_at: r.run_started_at.unwrap_or(r.created_at), updated_at: r.updated_at, url: r.html_url }),
    })
}

/// Runs Renovate now: dispatches its workflow (it must accept `workflow_dispatch`), or, without
/// one, ticks the dashboard's "manual job" box for the Renovate app.
pub async fn play(project: &str, hub: &Hub) -> Result<()> {
    split(project)?;
    let (workflow, branch, issue, running) = {
        let snap = hub.subscribe();
        let snap = snap.borrow();
        let r = snap.renovate.as_ref().context(tr("Renovate not loaded yet"))?;
        (r.schedule_id, r.schedule_ref.clone(), r.dashboard_iid, r.run.as_ref().is_some_and(|x| x.status.is_active()))
    };
    if running {
        bail!("{}", tr("Renovate is already running"));
    }
    match (workflow, issue) {
        (Some(id), _) => {
            let branch = match branch {
                Some(b) => b,
                None => api::<serde_json::Value>(&format!("repos/{project}")).await?["default_branch"].as_str().context("no default branch")?.to_string(),
            };
            tracing::info!("renovate: dispatch workflow {id} on {branch}");
            let _: serde_json::Value = gh_input("POST", &format!("repos/{project}/actions/workflows/{id}/dispatches"), &serde_json::json!({ "ref": branch })).await?;
        }
        (None, Some(number)) => {
            let current: RawIssue = api(&format!("repos/{project}/issues/{number}")).await?;
            let Some(body) = tick(&current.body.unwrap_or_default(), MANUAL_JOB) else {
                bail!("{}", tr("no Renovate workflow, and the dashboard has no unticked \"run again\" box"));
            };
            tracing::info!("renovate: tick the dashboard's manual job box");
            let _: serde_json::Value = gh_input("PATCH", &format!("repos/{project}/issues/{number}"), &serde_json::json!({ "body": body })).await?;
        }
        (None, None) => bail!("{}", tr!("no Renovate workflow nor \"{issue}\" issue found", issue = names().dashboard_title)),
    }
    // The run is created with a delay: we re-read a bit later.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let r = collect(project).await?;
    hub.update(|s| {
        let changed = s.renovate.as_ref() != Some(&r);
        s.renovate = Some(r);
        changed
    });
    Ok(())
}

/// Ticks the dashboard's `<!-- action=branch -->` checkbox: Renovate runs it on its next pass. The
/// body is re-read just before being rewritten, and only that line changes.
pub async fn check(project: &str, hub: &Hub, marker: &str) -> Result<()> {
    split(project)?;
    let (number, known, running) = {
        let snap = hub.subscribe();
        let snap = snap.borrow();
        let r = snap.renovate.as_ref().context(tr("Renovate not loaded yet"))?;
        let known = r.sections.iter().flat_map(|s| &s.items).any(|i| !i.checked && i.branch.as_ref().is_some_and(|b| format!("{}={b}", i.action) == marker));
        (r.dashboard_iid.with_context(|| tr!("no \"{issue}\" issue", issue = names().dashboard_title))?, known, r.run.as_ref().is_some_and(|x| x.status.is_active()))
    };
    if !known {
        bail!("{}", tr!("{marker} is not an unchecked item of the dashboard", marker = marker));
    }
    if running {
        // Renovate rewrites the issue at the end of a pass: a tick made during it would be lost.
        bail!("{}", tr("Renovate is running: try again once it is done"));
    }
    let current: RawIssue = api(&format!("repos/{project}/issues/{number}")).await?;
    let Some(body) = tick(&current.body.unwrap_or_default(), marker) else {
        bail!("{}", tr!("{marker} is no longer unchecked in the dashboard", marker = marker));
    };
    tracing::info!("renovate: check {marker}");
    let _: serde_json::Value = gh_input("PATCH", &format!("repos/{project}/issues/{number}"), &serde_json::json!({ "body": body })).await?;
    hub.update(|s| {
        let Some(r) = s.renovate.as_mut() else { return false };
        for i in r.sections.iter_mut().flat_map(|s| s.items.iter_mut()) {
            if i.branch.as_ref().is_some_and(|b| format!("{}={b}", i.action) == marker) {
                i.checked = true;
            }
        }
        true
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manual_job_box_is_ticked_alone() {
        let body = " - [ ] <!-- manual job -->Check this box to trigger a request for Renovate to run again\n - [ ] <!-- rebase-branch=renovate/x -->[x](../pull/3)";
        assert_eq!(tick(body, MANUAL_JOB).unwrap(), " - [x] <!-- manual job -->Check this box to trigger a request for Renovate to run again\n - [ ] <!-- rebase-branch=renovate/x -->[x](../pull/3)");
        assert!(tick(" - [x] <!-- manual job -->again", MANUAL_JOB).is_none());
    }

    /// Against GitHub itself: Renovate's own dashboard.
    #[tokio::test]
    #[ignore = "needs gh logged in, and the network"]
    async fn reads_a_public_dashboard() {
        let r = collect("renovatebot/renovate").await.unwrap();
        assert_eq!(r.dashboard_iid, Some(2958));
        assert!(r.sections.iter().flat_map(|s| &s.items).any(|i| i.mr.is_some()), "{:?}", r.sections.iter().map(|s| &s.title).collect::<Vec<_>>());
    }
}
