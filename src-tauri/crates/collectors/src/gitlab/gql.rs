//! GitLab's GraphQL answers (merge requests, pipelines) and how they become the snapshot's.

use super::*;

#[derive(Deserialize)]
pub(super) struct GqlPipelinesData {
    /// By alias (`b<i>`), one per branch.
    pub(super) project: Option<HashMap<String, Nodes<GqlPipelineFull>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GqlPipelineFull {
    id: String,
    status: String,
    r#ref: Option<String>,
    updated_at: DateTime<Utc>,
    path: Option<String>,
    stages: Option<Nodes<GqlStage>>,
}

#[derive(Deserialize)]
struct GqlStage {
    name: String,
    status: String,
    jobs: Option<Nodes<GqlJob>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlJob {
    id: Option<String>,
    name: String,
    status: String,
    #[serde(default)]
    allow_failure: bool,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    web_path: Option<String>,
    #[serde(default)]
    playable: bool,
    #[serde(default)]
    can_play_job: bool,
    #[serde(default)]
    manual_job: bool,
    #[serde(default)]
    retryable: bool,
    #[serde(default)]
    retried: bool,
    user_permissions: Option<GqlJobPermissions>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlJobPermissions {
    #[serde(default)]
    update_build: bool,
}

/// `gid://gitlab/Ci::Build/123` → 123
fn gid(id: &str) -> Option<u64> {
    id.rsplit('/').next()?.parse().ok()
}


impl From<GqlPipelineFull> for Pipeline {
    fn from(p: GqlPipelineFull) -> Self {
        let stages: Vec<(Stage, Vec<String>)> = p
            .stages
            .map(|s| s.nodes)
            .unwrap_or_default()
            .into_iter()
            .map(|st| {
                let jobs = st.jobs.map(|j| j.nodes).unwrap_or_default();
                let status = |j: &GqlJob| PipelineStatus::of(&j.status);
                let failed_jobs: Vec<JobLink> = jobs
                    .iter()
                    .filter(|j| status(j) == PipelineStatus::Failed && !j.allow_failure)
                    .map(|j| JobLink {
                        name: j.name.clone(),
                        url: j.web_path.as_ref().map(|w| crate::conventions::web(w)).unwrap_or_default(),
                    })
                    .collect();
                let manual_jobs: Vec<ManualJob> = jobs
                    .iter()
                    .filter(|j| status(j) == PipelineStatus::Manual && j.playable && j.can_play_job)
                    .filter_map(|j| {
                        Some(ManualJob {
                            id: gid(j.id.as_deref()?)?,
                            name: j.name.clone(),
                            url: j.web_path.as_ref().map(|w| crate::conventions::web(w)).unwrap_or_default(),
                        })
                    })
                    .collect();
                // Manual jobs already run, their latest run, that I may run again (`retry`).
                let played_jobs: Vec<PlayedJob> = jobs
                    .iter()
                    .filter(|j| j.manual_job && j.retryable && !j.retried && status(j).is_finished() && j.user_permissions.as_ref().is_some_and(|p| p.update_build))
                    .filter_map(|j| {
                        Some(PlayedJob {
                            id: gid(j.id.as_deref()?)?,
                            name: j.name.clone(),
                            url: j.web_path.as_ref().map(|w| crate::conventions::web(w)).unwrap_or_default(),
                            status: status(j),
                            finished_at: j.finished_at,
                        })
                    })
                    .collect();
                let finished = jobs.iter().filter(|j| status(j).is_finished()).count() as u32;
                // Stage time: from the first job started to the last one finished; open while a job is running.
                let started_at = jobs.iter().filter_map(|j| j.started_at).min();
                let all_done = !jobs.is_empty() && finished as usize == jobs.len();
                let finished_at = all_done.then(|| jobs.iter().filter_map(|j| j.finished_at).max()).flatten();
                let names = failed_jobs.iter().map(|j| j.name.clone()).collect();
                let stage = Stage {
                    status: PipelineStatus::of(&st.status),
                    jobs: jobs.len() as u32,
                    finished,
                    failed: failed_jobs.len() as u32,
                    failed_jobs,
                    manual_jobs,
                    played_jobs,
                    started_at,
                    finished_at,
                    name: st.name,
                };
                (stage, names)
            })
            .collect();
        Pipeline {
            // `gid://gitlab/Ci::Pipeline/123` → 123
            id: gid(&p.id).unwrap_or_default(),
            repo: None,
            project: None,
            target: target(p.r#ref.as_deref().unwrap_or_default()),
            status: PipelineStatus::of(&p.status),
            url: p.path.map(|path| crate::conventions::web(&path)).unwrap_or_default(),
            updated_at: p.updated_at,
            failed_jobs: stages.iter().flat_map(|(_, f)| f.clone()).collect(),
            stages: stages.into_iter().map(|(s, _)| s).collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GqlData {
    pub(super) current_user: Option<GqlUser>,
    pub(super) project: Option<GqlProject>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GqlProject {
    pub(super) merge_requests: Nodes<GqlMr>,
}

#[derive(Deserialize)]
pub(super) struct Nodes<T> {
    pub(super) nodes: Vec<T>,
}

#[derive(Deserialize)]
struct GqlLabel {
    title: String,
}

#[derive(Deserialize)]
pub(super) struct GqlUser {
    pub(super) username: String,
    #[serde(default)]
    name: String,
    /// Absolute (Gravatar) or relative to the instance (an uploaded picture).
    #[serde(default, rename = "avatarUrl")]
    avatar_url: Option<String>,
}

/// An avatar's https address: GitLab gives an uploaded one relative to the instance, whose
/// origin is that of the MR's `web_url`.
fn avatar(url: Option<String>, web_url: &str) -> Option<String> {
    let url = url?;
    if url.starts_with("https://") {
        return Some(url);
    }
    let origin = reqwest::Url::parse(web_url).ok().filter(|u| u.scheme() == "https")?.join("/").ok()?;
    url.starts_with('/').then(|| origin.join(&url).ok().map(String::from)).flatten()
}

/// A reviewer, with where their review stands (`REQUESTED_CHANGES`, `APPROVED`…).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlReviewer {
    username: String,
    merge_request_interaction: Option<GqlInteraction>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlInteraction {
    review_state: Option<String>,
}

/// One of the checks GitLab's page lists under the merge button (`CONFLICT`, `CI_MUST_PASS`…).
#[derive(Deserialize)]
struct GqlCheck {
    identifier: String,
    status: String,
}

impl GqlReviewer {
    fn requested_changes(&self) -> bool {
        self.merge_request_interaction.as_ref().and_then(|i| i.review_state.as_deref()) == Some("REQUESTED_CHANGES")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GqlMr {
    iid: String,
    title: String,
    draft: bool,
    web_url: String,
    source_branch: String,
    target_branch: String,
    // Absent from older answers (tests, a cached page): the MR then counts as opened long ago.
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    merged_at: Option<DateTime<Utc>>,
    state: String,
    author: GqlUser,
    #[serde(default)]
    assignees: Option<Nodes<GqlUser>>,
    reviewers: Nodes<GqlReviewer>,
    approved_by: Nodes<GqlUser>,
    approvals_left: Option<i64>,
    head_pipeline: Option<GqlPipelineFull>,
    detailed_merge_status: Option<String>,
    conflicts: bool,
    #[serde(default)]
    mergeability_checks: Vec<GqlCheck>,
    #[serde(default)]
    should_be_rebased: bool,
    #[serde(default)]
    merge_error: Option<String>,
    #[serde(default)]
    rebase_in_progress: bool,
    #[serde(default)]
    auto_merge_enabled: bool,
    labels: Option<Nodes<GqlLabel>>,
    resolvable_discussions_count: Option<i64>,
    resolved_discussions_count: Option<i64>,
    #[serde(default)]
    diff_stats_summary: Option<GqlDiffStats>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlDiffStats {
    additions: u64,
    deletions: u64,
    file_count: u64,
}

impl From<GqlMr> for MergeRequest {
    fn from(m: GqlMr) -> Self {
        MergeRequest {
            iid: m.iid.parse().unwrap_or_default(),
            // Before `web_url` moves to `url`.
            author_avatar: avatar(m.author.avatar_url, &m.web_url),
            repo: None,
            project: None,
            forge: None,
            tickets: ticket::find_all([m.title.as_str(), m.source_branch.as_str()]),
            ticket: ticket::find([m.title.as_str(), m.source_branch.as_str()]),
            title: m.title,
            url: m.web_url,
            state: MrState::of(&m.state),
            draft: m.draft,
            source_branch: m.source_branch,
            target_branch: m.target_branch,
            author: m.author.username,
            author_name: m.author.name,
            assignees: m.assignees.map(|a| a.nodes.into_iter().map(|u| u.username).collect()).unwrap_or_default(),
            changes_requested_by: m.reviewers.nodes.iter().filter(|r| r.requested_changes()).map(|r| r.username.clone()).collect(),
            reviewers: m.reviewers.nodes.into_iter().map(|u| u.username).collect(),
            approved_by: m.approved_by.nodes.into_iter().map(|u| u.username).collect(),
            approvals_left: m.approvals_left.unwrap_or(0),
            pipeline: m.head_pipeline.as_ref().map(|p| PipelineStatus::of(&p.status)),
            head_pipeline: m.head_pipeline.map(Pipeline::from),
            conflicts: conflicts(m.conflicts, m.detailed_merge_status.as_deref(), m.mergeability_checks.iter().find(|c| c.identifier == "CONFLICT").map(|c| c.status.as_str())),
            merge_status: m.detailed_merge_status.unwrap_or_default().to_lowercase(),
            needs_rebase: m.should_be_rebased,
            rebase_in_progress: m.rebase_in_progress,
            merge_error: m.merge_error.map(|e| e.trim().to_string()).filter(|e| !e.is_empty()),
            auto_merge: m.auto_merge_enabled,
            labels: m.labels.map(|l| l.nodes.into_iter().map(|l| l.title).collect()).unwrap_or_default(),
            open_threads: m.resolvable_discussions_count.unwrap_or(0) - m.resolved_discussions_count.unwrap_or(0),
            size: m.diff_stats_summary.map(|d| MrSize { additions: d.additions, deletions: d.deletions, files: d.file_count }),
            created_at: m.created_at,
            updated_at: m.updated_at,
            merged_at: m.merged_at,
        }
    }
}

/// Whether the MR is in conflict, as GitLab's own page says: its `CONFLICT` merge check, which
/// the page lists under the merge button. `conflicts` alone is not: it is GitLab's last
/// mergeability result (`cannot_be_merged?`), kept until it is checked again, so it stays set
/// after the branch was pushed or its target moved, while the page says "checking" then nothing.
/// The detailed merge status names only the first check that fails (an approval missing hides a
/// conflict). Without the checks (an older GitLab): the detailed merge status, then `conflicts`,
/// except on an MR to rebase (GitLab sets it on a branch behind its target at times, which its
/// Rebase button rebases fine: the rebase then succeeds, or fails with the conflicting commit,
/// "Rebase failed", resolved by an agent).
fn conflicts(flag: bool, status: Option<&str>, check: Option<&str>) -> bool {
    if let Some(check) = check {
        let failed = check.eq_ignore_ascii_case("failed");
        if flag != failed {
            tracing::debug!("gitlab: conflicts {flag}, its merge check {check}: the check is followed");
        }
        return failed;
    }
    match status.map(str::to_ascii_lowercase).as_deref() {
        Some("conflict") => true,
        Some("need_rebase") => {
            if flag {
                tracing::debug!("gitlab: conflicts set on an MR to rebase, shown as to rebase");
            }
            false
        }
        _ => flag,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_avatar_is_made_absolute_on_the_instance() {
        let mr = "https://git.acme.io/shop/web/-/merge_requests/7";
        assert_eq!(avatar(Some("/uploads/-/system/user/avatar/12/avatar.png".into()), mr).as_deref(), Some("https://git.acme.io/uploads/-/system/user/avatar/12/avatar.png"));
        assert_eq!(avatar(Some("https://secure.gravatar.com/avatar/ab?s=80&d=identicon".into()), mr).as_deref(), Some("https://secure.gravatar.com/avatar/ab?s=80&d=identicon"));
        assert_eq!(avatar(Some("http://git.acme.io/a.png".into()), mr), None);
        assert_eq!(avatar(Some("/a.png".into()), "http://git.acme.io/x"), None);
        assert_eq!(avatar(None, mr), None);
    }

    /// GitLab's page first: its conflict check, whatever `conflicts` and the merge status say.
    #[test]
    fn conflicts_follow_the_merge_check() {
        assert!(!conflicts(true, Some("NOT_APPROVED"), Some("SUCCESS")), "a stale `conflicts`");
        assert!(!conflicts(true, Some("CHECKING"), Some("CHECKING")), "pushed, not checked yet");
        assert!(conflicts(false, Some("NOT_APPROVED"), Some("FAILED")), "an earlier check hides the conflict");
        assert!(!conflicts(true, Some("MERGEABLE"), Some("INACTIVE")));
    }

    /// Without the checks: a conflict the merge status names, a rebase it asks for whatever `conflicts` says.
    #[test]
    fn conflicts_follow_the_detailed_merge_status() {
        assert!(conflicts(false, Some("CONFLICT"), None));
        assert!(!conflicts(true, Some("NEED_REBASE"), None), "to rebase, not in conflict");
        assert!(conflicts(true, Some("NOT_APPROVED"), None), "an earlier check may hide the conflict");
        assert!(!conflicts(false, Some("MERGEABLE"), None) && conflicts(true, None, None));
    }

    /// A tag job already run stays, to run again; one still to run, one retried since (its new run
    /// shows instead), one I may not run again, or an automatic one, does not.
    #[test]
    fn manual_jobs_already_run_are_kept() {
        let job = |id: u32, name: &str, status: &str, manual: bool, retried: bool, update: bool| serde_json::json!({
            "id": format!("gid://gitlab/Ci::Build/{id}"), "name": name, "status": status, "allowFailure": false,
            "startedAt": null, "finishedAt": "2026-11-08T10:00:00Z", "webPath": format!("/g/app/-/jobs/{id}"),
            "playable": status == "MANUAL", "canPlayJob": true, "manualJob": manual, "retryable": status != "MANUAL", "retried": retried,
            "userPermissions": { "updateBuild": update }
        });
        let p: GqlPipelineFull = serde_json::from_value(serde_json::json!({
            "id": "gid://gitlab/Ci::Pipeline/9", "status": "SUCCESS", "ref": "main", "updatedAt": "2026-11-08T10:00:00Z", "path": "/g/app/-/pipelines/9",
            "stages": { "nodes": [{ "name": "tag", "status": "success", "jobs": { "nodes": [
                job(1, "tag(api)", "SUCCESS", true, false, true),
                job(2, "tag(web)", "MANUAL", true, false, true),
                job(3, "tag(old)", "SUCCESS", true, true, true),
                job(4, "tag(ro)", "SUCCESS", true, false, false),
                job(5, "build", "SUCCESS", false, false, true),
                job(6, "tag(ko)", "FAILED", true, false, true)
            ] } }] }
        }))
        .unwrap();
        let stage = &Pipeline::from(p).stages[0];
        assert_eq!(stage.manual_jobs.iter().map(|j| j.id).collect::<Vec<_>>(), [2], "still to run: as before");
        assert_eq!(stage.played_jobs.iter().map(|j| (j.id, j.status)).collect::<Vec<_>>(), [(1, PipelineStatus::Success), (6, PipelineStatus::Failed)]);
        assert!(stage.played_jobs[0].finished_at.is_some() && stage.played_jobs[0].url.ends_with("/g/app/-/jobs/1"));
    }

    #[test]
    fn a_failed_rebase_is_read() {
        let m: GqlMr = serde_json::from_value(serde_json::json!({
            "iid": "42", "title": "feat: x", "draft": false, "webUrl": "https://g/x/-/merge_requests/42",
            "sourceBranch": "feat/x", "targetBranch": "release/1", "updatedAt": "2026-10-12T08:00:00Z", "mergedAt": null,
            "state": "opened", "author": { "username": "me" }, "reviewers": { "nodes": [] }, "approvedBy": { "nodes": [] },
            "approvalsLeft": 0, "headPipeline": null, "detailedMergeStatus": "MERGEABLE", "conflicts": false, "shouldBeRebased": false,
            "mergeError": "Rebase failed: Rebase locally, resolve all conflicts, then push the branch. Try again.", "rebaseInProgress": false,
            "labels": { "nodes": [] }, "resolvableDiscussionsCount": 0, "resolvedDiscussionsCount": 0
        }))
        .unwrap();
        let mr = MergeRequest::from(m);
        assert_eq!(mr.merge_error.as_deref(), Some("Rebase failed: Rebase locally, resolve all conflicts, then push the branch. Try again."));
        assert!(!mr.rebase_in_progress && !mr.needs_rebase, "GitLab does not always say it is to rebase");
    }

    /// GitLab's merge status says "conflict" before its `conflicts` does: a conflict all the same.
    #[test]
    fn a_conflict_in_the_merge_status_is_one() {
        let m: GqlMr = serde_json::from_value(serde_json::json!({
            "iid": "43", "title": "feat: y", "draft": false, "webUrl": "https://g/x/-/merge_requests/43",
            "sourceBranch": "feat/y", "targetBranch": "release/1", "updatedAt": "2026-10-12T08:00:00Z", "mergedAt": null,
            "state": "opened", "author": { "username": "me" }, "reviewers": { "nodes": [] }, "approvedBy": { "nodes": [] },
            "approvalsLeft": 0, "headPipeline": null, "detailedMergeStatus": "CONFLICT", "conflicts": false, "shouldBeRebased": true,
            "mergeError": null, "rebaseInProgress": false,
            "labels": { "nodes": [] }, "resolvableDiscussionsCount": 0, "resolvedDiscussionsCount": 0
        }))
        .unwrap();
        let mr = MergeRequest::from(m);
        assert!(mr.conflicts && mr.needs_rebase);
        assert_eq!(mr.merge_status, "conflict");
    }
}
