//! The forge and the repositories: worktrees, MRs, pipelines, Renovate, vulnerabilities, CI health.

use chrono::{DateTime, Utc};
use serde::Serialize;


#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Worktree {
    /// Its repository (`[[repos]] name`).
    pub repo: String,
    pub path: String,
    pub branch: Option<String>,
    pub ticket: Option<String>,
    pub base: String,
    pub behind: u32,
    pub ahead: u32,
    /// Modified tracked files (`changes`); untracked ones are counted separately, less often.
    pub dirty: bool,
    pub changes: u32,
    /// Untracked files, outside the `.claude/` folders Claude leaves behind; re-read
    /// along with the size, since walking the folder is costly. Absent before the first measurement.
    pub untracked: Option<u32>,
    pub size_bytes: Option<u64>,
    /// Commits that are on no remote branch.
    pub unpushed: u32,
    /// Upstream branch (`origin/…`), and whether it is gone from the remote (MR merged, branch deleted).
    pub upstream: Option<String>,
    pub upstream_gone: bool,
    /// Commits of the upstream branch not here yet (as of the last fetch): what a pull brings.
    pub unpulled: u32,
    /// Checked-out commit, abbreviated (useful with a detached HEAD).
    pub head: Option<String>,
    pub last_commit: Option<DateTime<Utc>>,
    pub is_main: bool,
    /// Folder deleted without `git worktree remove`: git keeps it until `git worktree prune`.
    pub prunable: bool,
    pub locked: bool,
}

/// A tracked repository with a local clone (the primary one first): its current branch and its
/// gap with the remote's default branch, for the Repos menu.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Repo {
    pub name: String,
    pub path: String,
    /// GitLab project (`group/project`), for links.
    pub project: Option<String>,
    pub branch: Option<String>,
    /// Remote default branch (`main`), which `behind` and `ahead` compare against.
    pub base: String,
    pub behind: u32,
    pub ahead: u32,
    pub dirty: bool,
    /// The primary repository (`[[repos]] primary`).
    pub is_main: bool,
    /// Folder not found or not a git repository.
    pub error: Option<String>,
    /// The forge its project is on, when not the profile's (`[[repos]] forge`): its link.
    pub forge: Option<crate::forge::Provider>,
}

/// A pipeline's status, a stage's or a job's, in GitLab's words (GitHub's checks are read as
/// them): what the window colors and what an action is allowed on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    Created,
    WaitingForResource,
    Preparing,
    #[default]
    Pending,
    Running,
    Success,
    Failed,
    Canceled,
    Skipped,
    Manual,
    Scheduled,
}

impl PipelineStatus {
    const ALL: [Self; 11] = [
        Self::Created,
        Self::WaitingForResource,
        Self::Preparing,
        Self::Pending,
        Self::Running,
        Self::Success,
        Self::Failed,
        Self::Canceled,
        Self::Skipped,
        Self::Manual,
        Self::Scheduled,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::WaitingForResource => "waiting_for_resource",
            Self::Preparing => "preparing",
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Skipped => "skipped",
            Self::Manual => "manual",
            Self::Scheduled => "scheduled",
        }
    }

    /// GitLab's status, whatever its case (`SUCCESS` in GraphQL); `canceling` is still running, one
    /// of a newer GitLab's own is pending.
    pub fn of(status: &str) -> Self {
        let status = status.to_lowercase();
        match status.as_str() {
            "canceling" => Self::Running,
            "cancelled" => Self::Canceled,
            s => Self::ALL.into_iter().find(|p| p.name() == s).unwrap_or(Self::Pending),
        }
    }

    /// Over, whatever its outcome.
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Success | Self::Failed | Self::Canceled | Self::Skipped)
    }

    /// Under way: still to start, or running (a manual or scheduled one waits for someone).
    pub fn is_active(self) -> bool {
        matches!(self, Self::Created | Self::WaitingForResource | Self::Preparing | Self::Pending | Self::Running)
    }

    /// Still to run or running: it can be stopped.
    pub fn can_cancel(self) -> bool {
        matches!(self, Self::Created | Self::WaitingForResource | Self::Preparing | Self::Pending | Self::Running | Self::Scheduled)
    }

    /// Failed or canceled: its jobs can run again.
    pub fn can_retry(self) -> bool {
        matches!(self, Self::Failed | Self::Canceled)
    }
}

impl std::fmt::Display for PipelineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Where an MR is: open, merged, or closed without a merge (GitHub lists those of its pull requests).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum MrState {
    #[default]
    Opened,
    Merged,
    Closed,
}

impl MrState {
    /// GitLab's state (`opened`, `merged`, `closed`, `locked`: being merged).
    pub fn of(state: &str) -> Self {
        match state.to_lowercase().as_str() {
            "merged" => Self::Merged,
            "opened" | "locked" => Self::Opened,
            _ => Self::Closed,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MergeRequest {
    pub iid: u64,
    /// Repository of the MR (`[[repos]] name`) and its GitLab project; `None`: the primary repository.
    /// An MR is identified by (repository, number): two projects each have their own `!1`.
    pub repo: Option<String>,
    pub project: Option<String>,
    /// The forge it is on, when not the profile's (`[[repos]] forge`): read by that forge's collector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge: Option<crate::forge::Provider>,
    pub title: String,
    pub url: String,
    /// Open, or recently merged (they feed the sync rules).
    pub state: MrState,
    pub draft: bool,
    pub source_branch: String,
    pub target_branch: String,
    /// The first ticket of its title or branch; `tickets` has them all (an MR may close several).
    pub ticket: Option<String>,
    pub tickets: Vec<String>,
    pub author: String,
    pub author_name: String,
    /// The author's profile picture (an https address), read by `forge_avatar`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_avatar: Option<String>,
    /// Assignees (usernames).
    pub assignees: Vec<String>,
    pub reviewers: Vec<String>,
    pub approved_by: Vec<String>,
    /// Reviewers whose latest word is a request for changes: the MR is not ready to merge.
    #[serde(default)]
    pub changes_requested_by: Vec<String>,
    pub approvals_left: i64,
    /// Its head pipeline's status.
    pub pipeline: Option<PipelineStatus>,
    pub merge_status: String,
    pub conflicts: bool,
    /// Branch behind the target while the project requires a linear history:
    /// to rebase before merging. Unlike `merge_status`, which only gives the first
    /// blocker, true even while the MR is waiting for its review or its CI.
    pub needs_rebase: bool,
    /// The forge is rebasing it (GitLab rebases in the background).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rebase_in_progress: bool,
    /// Why the forge's last rebase or merge of it failed (GitLab's `merge_error`): a rebase can
    /// fail on a commit in between even when the branches do not conflict as a whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_error: Option<String>,
    /// "Merge when checks pass" enabled.
    pub auto_merge: bool,
    /// GitLab labels of the MR (`security`…).
    pub labels: Vec<String>,
    /// Head pipeline with its stages (open MRs only).
    pub head_pipeline: Option<Pipeline>,
    pub open_threads: i64,
    /// Diff size (open MRs only).
    pub size: Option<MrSize>,
    /// When it was opened (none from an older reading).
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub merged_at: Option<DateTime<Utc>>,
}

/// Lines added and removed, files touched.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MrSize {
    pub additions: u64,
    pub deletions: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Pipeline {
    pub id: u64,
    /// Repository of a protected branch's pipeline (`[[repos]] name`) and its project; `None`: the
    /// primary repository (and an MR's pipeline, which its MR carries).
    pub repo: Option<String>,
    pub project: Option<String>,
    /// Branch, or `!<iid>` for a merge request pipeline.
    pub target: String,
    pub status: PipelineStatus,
    pub url: String,
    pub updated_at: DateTime<Utc>,
    pub failed_jobs: Vec<String>,
    pub stages: Vec<Stage>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Stage {
    pub name: String,
    pub status: PipelineStatus,
    pub jobs: u32,
    /// Finished jobs, whatever their outcome.
    pub finished: u32,
    pub failed: u32,
    pub failed_jobs: Vec<JobLink>,
    /// Manual jobs I can run.
    pub manual_jobs: Vec<ManualJob>,
    /// Manual jobs already run (a tag set), which I can run again: after a new base image, the
    /// same code rebuilt and delivered. Their latest run only.
    pub played_jobs: Vec<PlayedJob>,
    /// First job started, and last finished once all jobs are done: enough to display
    /// the time spent in the stage (while running, the front computes it from `started_at`).
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Renovate: Dependency Dashboard sections and last run of "Renovate (Daily)".
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Renovate {
    pub dashboard_url: Option<String>,
    pub dashboard_iid: Option<u64>,
    pub sections: Vec<RenovateSection>,
    pub schedule_id: Option<u64>,
    pub schedule_ref: Option<String>,
    pub run: Option<RenovateRun>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RenovateSection {
    pub title: String,
    pub items: Vec<RenovateItem>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RenovateItem {
    pub title: String,
    /// Checkbox marker: `approve-branch`, `rebase-branch`, `unlimit-branch`…
    pub action: String,
    pub branch: Option<String>,
    pub mr: Option<u64>,
    pub packages: Vec<String>,
    pub checked: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RenovateRun {
    pub id: u64,
    pub status: PipelineStatus,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub url: String,
}

/// Fixed HIGH and CRITICAL vulnerabilities in the release branch's dependencies and in the
/// backend images in production and demo, from a local Trivy scan. Kept in the app's cache between two launches.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Vulnerabilities {
    pub branch: String,
    /// Scanned commit, short.
    pub sha: String,
    /// `Version: 0.74.0`
    pub trivy: String,
    pub scanned_at: Option<DateTime<Utc>>,
    /// A scan is running (the results shown are from the previous one).
    pub scanning: bool,
    /// All scanned projects, clean ones included.
    pub projects: Vec<VulnProject>,
    /// The other branches scanned (`[vulnerabilities] branches`), each with its short commit.
    #[serde(default)]
    pub other_branches: Vec<(String, String)>,
    /// To fix first (critical, then new), then the accepted ones.
    pub findings: Vec<VulnFinding>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct VulnProject {
    /// `cart-api`, `libs/design-system`, `(root)`, or `cart-api · prod` for an image.
    pub project: String,
    /// Repository the target comes from (`dockerfiles` for a base image); absent: the primary repository.
    #[serde(default)]
    pub repo: Option<String>,
    /// A base image (an image provider's, built from `repo`): its CVEs are fixed there.
    #[serde(default)]
    pub base: bool,
    /// Scanned image (`prod/cart-api:0.2.147`); absent for a project's dependencies.
    #[serde(default)]
    pub image: Option<String>,
    /// Image read from the GCP registry, or newer tags without a published image.
    #[serde(default)]
    pub note: Option<String>,
    /// The image could not be read (registry, connection).
    #[serde(default)]
    pub error: Option<String>,
    pub critical: u32,
    pub high: u32,
    /// Absent from the previous scan.
    pub new: u32,
    /// Dismissed by the project's `.trivyignore`.
    #[serde(default)]
    pub accepted: u32,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct VulnFinding {
    pub id: String,
    /// `critical` or `high`.
    #[cfg_attr(feature = "ts", ts(type = "'critical' | 'high'"))]
    pub severity: String,
    pub package: String,
    pub installed: String,
    pub fixed: String,
    pub title: String,
    pub url: Option<String>,
    pub projects: Vec<String>,
    /// To fix, and absent from the previous scan in at least one of its projects.
    pub new: bool,
    /// Dismissed by a `.trivyignore`: accepted risk.
    #[serde(default)]
    pub accepted: bool,
}

/// CI health over the window (`window_days`): one scope per pipeline family.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CiHealth {
    pub window_days: u32,
    pub scopes: Vec<CiScope>,
}

/// `main`, `release/*` or MR pipelines. Figures cover the last 7 days,
/// `_prev` the 7 before; durations are in seconds, success rate between 0 and 1.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CiScope {
    pub key: String,
    pub label: String,
    pub pipelines: u32,
    pub pipelines_prev: u32,
    /// Median of the pipelines that ran to completion.
    pub duration: Option<f64>,
    pub duration_prev: Option<f64>,
    pub success: Option<f64>,
    pub success_prev: Option<f64>,
    /// Wait for a runner per job: median and 90th percentile.
    pub queue: Option<f64>,
    pub queue_p90: Option<f64>,
    /// One point per day of the window, from oldest to most recent.
    pub days: Vec<CiDay>,
    pub slow_jobs: Vec<SlowJob>,
    pub flaky: Vec<FlakyJob>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CiDay {
    pub date: chrono::NaiveDate,
    pub pipelines: u32,
    pub duration: Option<f64>,
    pub success: Option<f64>,
    pub queue: Option<f64>,
}

/// Successful job among the longest: median of the week, of the previous one, and of each day.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SlowJob {
    pub name: String,
    pub runs: u32,
    pub duration: f64,
    pub duration_prev: Option<f64>,
    pub days: Vec<Option<f64>>,
}

/// Job that failed then passed on retry in the same pipeline.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FlakyJob {
    pub name: String,
    /// Pipelines where it failed before passing on retry.
    pub flakes: u32,
    /// Pipelines where it ran.
    pub runs: u32,
    pub last_at: DateTime<Utc>,
    /// The most recent failures (at most 3).
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ManualJob {
    pub id: u64,
    pub name: String,
    pub url: String,
}

/// A manual job already run: how it ended, and when.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PlayedJob {
    pub id: u64,
    pub name: String,
    pub url: String,
    pub status: PipelineStatus,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct JobLink {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PendingJob {
    /// Its repository; `None`: the primary one.
    pub repo: Option<String>,
    pub name: String,
    pub target: String,
    pub queued_since: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_pipeline_status_reads_gitlab_s_words() {
        use super::PipelineStatus as P;
        assert_eq!((P::of("SUCCESS"), P::of("waiting_for_resource"), P::of("canceling"), P::of("new_one")), (P::Success, P::WaitingForResource, P::Running, P::Pending));
        assert_eq!(serde_json::to_value(P::WaitingForResource).unwrap(), "waiting_for_resource");
        assert!(P::Scheduled.can_cancel() && !P::Success.can_cancel() && P::Canceled.can_retry() && P::Skipped.is_finished());
    }
}
