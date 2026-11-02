//! Renovate: the items of the "Dependency Dashboard" (an issue maintained by the bot) and the
//! "Renovate (Daily)" schedule, run by hand from Prism.

use prism_base::{i18n::tr, tr};
use std::{sync::LazyLock, time::Duration};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Deserialize;

use super::{api, encode, glab};
use crate::{
    config::GitlabConfig,
    state::{Hub, Renovate, RenovateItem, RenovateRun, RenovateSection},
};

const SOURCE: &str = "renovate";
const POLL: Duration = Duration::from_secs(300);
/// While the Renovate pipeline is running, its status is followed closely.
const POLL_RUNNING: Duration = Duration::from_secs(30);
/// The dashboard issue's title and the schedule's description (`[renovate]`), set at launch
/// and on each change of the config.
static NAMES: prism_base::rt::Swap<crate::config::RenovateConfig> = prism_base::rt::Swap::new();

/// The project Renovate runs in (`[renovate] repo`, otherwise the primary repository's).
static PROJECT: prism_base::rt::Swap<Option<String>> = prism_base::rt::Swap::new();

pub fn configure(cfg: &crate::config::RenovateConfig, project: Option<String>) {
    NAMES.set(cfg.clone());
    PROJECT.set(project);
}

/// The project Renovate runs in, for its actions (Run now, a dashboard's box).
pub fn project() -> Result<String> {
    PROJECT.get().cloned().flatten().with_context(|| tr("no repository for Renovate ([renovate] repo, or a primary repository)"))
}

pub fn names() -> crate::config::RenovateConfig {
    NAMES.get().cloned().unwrap_or_default()
}

/// A query string value: everything but letters, digits and `-_.~` percent-encoded.
fn query(v: &str) -> String {
    crate::http::encode(v)
}

pub async fn run(cfg: GitlabConfig, hub: Hub) {
    let mut first = true;
    let mut every = POLL;
    loop {
        if !std::mem::take(&mut first) {
            hub.pause(every).await;
        }
        let read = collect(&cfg.project).await;
        if let Ok(r) = &read {
            every = if r.run.as_ref().is_some_and(|x| x.status.is_active()) { POLL_RUNNING } else { POLL };
        }
        hub.publish(SOURCE, read, |s, r| crate::state::replace(&mut s.renovate, Some(r)));
    }
}

#[derive(Deserialize)]
struct RawIssue {
    iid: u64,
    title: String,
    web_url: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
struct RawSchedule {
    id: u64,
    description: String,
    r#ref: String,
    last_pipeline: Option<RawPipeline>,
}

#[derive(Deserialize)]
struct RawPipeline {
    id: u64,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    web_url: String,
}

async fn collect(project: &str) -> Result<Renovate> {
    let p = encode(project);
    let issues: Vec<RawIssue> =
        api(&format!("projects/{p}/issues?search={}&in=title&state=opened&per_page=5", query(&names().dashboard_title))).await?;
    let issue = issues.into_iter().find(|i| i.title == names().dashboard_title);
    let schedule = daily_schedule(&p).await?;
    Ok(Renovate {
        dashboard_url: issue.as_ref().map(|i| i.web_url.clone()),
        dashboard_iid: issue.as_ref().map(|i| i.iid),
        sections: issue.and_then(|i| i.description).map(|d| parse(&d)).unwrap_or_default(),
        schedule_id: schedule.as_ref().map(|s| s.id),
        schedule_ref: schedule.as_ref().map(|s| s.r#ref.trim_start_matches("refs/heads/").to_string()),
        run: schedule.and_then(|s| s.last_pipeline).map(|l| RenovateRun {
            id: l.id,
            status: crate::state::PipelineStatus::of(&l.status),
            started_at: l.created_at,
            updated_at: l.updated_at,
            url: l.web_url,
        }),
    })
}

async fn daily_schedule(p: &str) -> Result<Option<RawSchedule>> {
    let all: Vec<RawSchedule> = api(&format!("projects/{p}/pipeline_schedules?per_page=50")).await?;
    let name = names().schedule;
    let found = all.iter().find(|s| if name.is_empty() { s.description.to_lowercase().contains("renovate") } else { s.description == name });
    let Some(id) = found.map(|s| s.id) else { return Ok(None) };
    // The details carry the last pipeline run by the schedule.
    Ok(Some(api(&format!("projects/{p}/pipeline_schedules/{id}")).await?))
}

/// Runs Renovate's schedule (`[renovate] schedule`) now (`POST …/pipeline_schedules/:id/play`).
pub async fn play(project: &str, hub: &Hub) -> Result<()> {
    let p = encode(project);
    let id = hub.subscribe().borrow().renovate.as_ref().and_then(|r| r.schedule_id).with_context(|| match names().schedule.as_str() {
        "" => tr("no Renovate pipeline schedule found").to_string(),
        name => tr!("no \"{name}\" pipeline schedule found", name = name),
    })?;
    if hub.subscribe().borrow().renovate.as_ref().and_then(|r| r.run.as_ref()).is_some_and(|r| r.status.is_active()) {
        bail!("{}", tr("Renovate is already running"));
    }
    tracing::info!("renovate: play schedule {id}");
    let _: serde_json::Value = glab(&["api".into(), "--method".into(), "POST".into(), format!("projects/{p}/pipeline_schedules/{id}/play")]).await?;
    // The pipeline is created with a delay: we re-read the schedule a bit later.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let r = collect(project).await?;
    hub.update(|s| {
        let changed = s.renovate.as_ref() != Some(&r);
        s.renovate = Some(r);
        changed
    });
    Ok(())
}

/// Ticks the dashboard's `<!-- action=branch -->` checkbox: Renovate will run it on its next
/// pass. The description is re-read just before being rewritten, and only that line changes.
pub async fn check(project: &str, hub: &Hub, marker: &str) -> Result<()> {
    let p = encode(project);
    let (iid, known, running) = {
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
    let issue: RawIssue = api(&format!("projects/{p}/issues/{iid}")).await?;
    let Some(updated) = tick(&issue.description.unwrap_or_default(), marker) else {
        bail!("{}", tr!("{marker} is no longer unchecked in the dashboard", marker = marker));
    };
    tracing::info!("renovate: check {marker}");
    let _: serde_json::Value = glab(&[
        "api".into(), "--method".into(), "PUT".into(),
        format!("projects/{p}/issues/{iid}"),
        "-f".into(), format!("description={updated}"),
    ])
    .await?;
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

/// The dashboard with the `<!-- marker -->` checkbox ticked, and only that line changed; `None`
/// when it is not there unticked.
pub fn tick(description: &str, marker: &str) -> Option<String> {
    let needle = format!("<!-- {marker} -->");
    let mut found = false;
    let lines: Vec<String> = description
        .split('\n')
        .map(|line| {
            if !found && line.contains(&needle) && line.contains("- [ ]") {
                found = true;
                line.replacen("- [ ]", "- [x]", 1)
            } else {
                line.to_string()
            }
        })
        .collect();
    found.then(|| lines.join("\n"))
}

static ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*- \[(?P<checked>[ xX])\] <!-- (?P<action>[\w-]+)(?:=(?P<branch>[^ ]+))? -->(?P<rest>.*)$").unwrap()
});
/// The update's merge request: `(!1970)` on GitLab, `(../pull/1970)` or a full address on GitHub.
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[(?P<title>.+)\]\((?:!|#|[^)\s]*/pull/)(?P<mr>\d+)\)(?P<rest>.*)$").unwrap());
static PACKAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").unwrap());

/// Dashboard sections and their checkboxes; "Detected Dependencies" (the inventory) is ignored.
pub fn parse(markdown: &str) -> Vec<RenovateSection> {
    let mut out: Vec<RenovateSection> = Vec::new();
    for line in markdown.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            out.push(RenovateSection { title: title.trim().to_string(), items: Vec::new() });
            continue;
        }
        let Some(section) = out.last_mut() else { continue };
        if section.title.eq_ignore_ascii_case("Detected Dependencies") {
            continue;
        }
        let Some(c) = ITEM.captures(line) else { continue };
        // Global checkboxes ("rebase all open MRs"…): not an update, we do not show them.
        if c.name("branch").is_none() {
            continue;
        }
        let rest = c["rest"].trim();
        let (title, mr, tail) = match LINK.captures(rest) {
            Some(l) => (l["title"].to_string(), l["mr"].parse().ok(), l["rest"].to_string()),
            None => {
                // Title up to the package list: "fix(deps): … (`a`, `b`)".
                let cut = rest.find(" (`").unwrap_or(rest.len());
                (rest[..cut].to_string(), None, rest[cut..].to_string())
            }
        };
        section.items.push(RenovateItem {
            title: title.replace("**", ""),
            action: c["action"].to_string(),
            branch: c.name("branch").map(|b| b.as_str().to_string()),
            mr,
            packages: PACKAGE.captures_iter(&tail).map(|p| p[1].to_string()).collect(),
            checked: &c["checked"] != " ",
        });
    }
    out.retain(|s| !s.items.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(query("Dependency Dashboard"), "Dependency%20Dashboard");
        assert_eq!(query("Deps & #1"), "Deps%20%26%20%231");
    }

    #[test]
    fn dashboard_items_are_parsed() {
        let md = "## Pending Approval\n\n - [ ] <!-- approve-branch=renovate/x -->fix(deps): update all (`a`, `b`)\n\n## Open\n\n - [ ] <!-- rebase-branch=renovate/y -->[chore(deps): lock file maintenance](!1970)\n - [ ] <!-- rebase-all-open-prs -->**Click on this checkbox to rebase all open MRs at once**\n\n## Detected Dependencies\n\n - [ ] <!-- nope=1 -->ignored\n";
        let s = parse(md);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].items[0].packages, vec!["a", "b"]);
        assert_eq!(s[0].items[0].branch.as_deref(), Some("renovate/x"));
        assert_eq!(s[1].items[0].mr, Some(1970));
        assert_eq!(s[1].items.len(), 1, "global checkboxes are left out");
        let gh = parse("## Open\n\n - [ ] <!-- rebase-branch=renovate/z -->[chore(deps): update x](../pull/42)\n");
        assert_eq!((gh[0].items[0].mr, gh[0].items[0].title.as_str()), (Some(42), "chore(deps): update x"));
    }
}
