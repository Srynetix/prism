//! The forge (`[forge]`, `[gitlab]`) and what reads its repositories: vulnerabilities, Renovate.

use serde::{Deserialize, Serialize};

use super::*;

/// "Vulnerabilities" block.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct VulnerabilitiesConfig {
    /// The Vulnerabilities block: Trivy scans of its repositories' dependencies, and of the images
    /// a block names. Off by default (it needs `trivy`).
    #[serde(default)]
    pub enabled: bool,
    /// The repositories whose dependencies are scanned (`[[repos]]` names, their local clone).
    /// Empty: the primary repository (`Config::primary`).
    #[serde(default)]
    pub repos: Vec<String>,
    /// The branches whose dependencies are scanned, in each repository: `main` (its main branch,
    /// the default), `release` (its latest release branch, otherwise its main one), or a branch's name.
    /// With several, each project's line carries the branch (`cart-api · main`).
    #[serde(default = "default_branches")]
    pub branches: Vec<String>,
    /// Folder of the shared libraries scanned besides the components (an Nx monorepo's `libs`):
    /// its `project.json` projects. Empty (default): none.
    #[serde(default)]
    pub libs: String,
    /// Folders Trivy skips in an image (a tool's cache).
    #[serde(default)]
    pub skip_dirs: Vec<String>,
    /// Folder of the components whose `project.json` projects are scanned one by one (an Nx
    /// monorepo's `apps`). Empty (default): the repository as a whole.
    #[serde(default)]
    pub components: String,
    /// The block naming the images to scan (`Block::images`, a team's delivery block, its own
    /// settings saying which); `none` (default): no image, the dependencies only.
    #[serde(default = "default_images")]
    pub images: String,
}

fn default_images() -> String {
    "none".into()
}

impl Default for VulnerabilitiesConfig {
    fn default() -> Self {
        Self { enabled: false, repos: Vec::new(), branches: default_branches(), libs: String::new(), skip_dirs: Vec::new(), components: String::new(), images: default_images() }
    }
}

fn default_branches() -> Vec<String> {
    vec!["main".into()]
}

/// Renovate (`[renovate]`): its Dependency Dashboard issue and the pipeline schedule running it,
/// in its repository's project, found by their names.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct RenovateConfig {
    /// Off by default.
    pub enabled: bool,
    /// The repository whose project runs Renovate (a `[[repos]]` name). Empty: the primary one.
    pub repo: String,
    /// Title of the dashboard issue.
    pub dashboard_title: String,
    /// Description of the pipeline schedule running Renovate ("Run now"). Empty: the first
    /// schedule whose description names Renovate.
    pub schedule: String,
}

impl Default for RenovateConfig {
    fn default() -> Self {
        Self { enabled: false, repo: String::new(), dashboard_title: "Dependency Dashboard".into(), schedule: String::new() }
    }
}

/// `[forge]`: one forge per profile, and what holds whichever it is. On GitHub, `gh` (its login,
/// in the keychain) reads the merge requests and checks of the `[[repos]]` (`project =
/// "owner/repo"`); what only GitLab has (`[gitlab]`) stays off. The switch, branches, merged window,
/// polling and CI health were `[gitlab]`'s: still read there (`migrate`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ForgeConfig {
    #[serde(default)]
    pub provider: crate::forge::Provider,
    /// Access to the forge (and thus to the repository). Off (Product profile): no MRs, no
    /// pipelines, no worktrees, vulnerabilities, repositories or agents; no access error in Alerts.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Main branch, and release branch prefix (Git Flow: `release/`; empty, the default: none), of
    /// every repository that does not name its own (`[[repos]] main_branch`, `release_prefix`):
    /// together, the protected branches whose pipelines are followed; the most recent release is
    /// the base. An empty main branch (the default): the remote's default branch.
    #[serde(default = "default_main_branch")]
    pub main_branch: String,
    #[serde(default = "default_release_prefix")]
    pub release_prefix: String,
    /// The branches whose pipeline is followed, of every repository that names none of its own
    /// (`[[repos]] pipeline_branches`): names, `*` patterns, `@main`, `@release`. Empty (the
    /// default): the main branch and the release ones.
    #[serde(default)]
    pub pipeline_branches: Vec<String>,
    /// Days during which a merged MR stays followed.
    #[serde(default = "default_merged_days")]
    pub merged_days: u32,
    /// Polling of the merge requests and pipelines: no forge reaches a local machine by webhook.
    #[serde(default = "default_gitlab_poll")]
    pub poll_seconds: u64,
    /// The CI health block: failure rate and durations of its repository's pipelines.
    #[serde(default = "default_true")]
    pub ci_health: bool,
    /// The repository CI health reads (a `[[repos]]` name). Empty: the primary one.
    #[serde(default)]
    pub ci_health_repo: String,
    /// The repository a bare MR reference (`!1294`, no repository before it) names, its MRs listed
    /// without their repository's name (a `[[repos]]` name, on this profile's forge). Empty: the
    /// entry with `primary = true`, if any.
    #[serde(default)]
    pub bare_mr_repo: String,
    /// CI health window, in days.
    #[serde(default = "default_ci_health_days")]
    pub ci_health_days: u32,
    /// MR authors and members shown with their GitLab / GitHub profile picture; off, or without
    /// one, their initials.
    #[serde(default = "default_true")]
    pub avatars: bool,
}

impl Default for ForgeConfig {
    fn default() -> Self {
        Self {
            provider: Default::default(),
            enabled: true,
            main_branch: default_main_branch(),
            release_prefix: default_release_prefix(),
            pipeline_branches: Vec::new(),
            merged_days: default_merged_days(),
            poll_seconds: default_gitlab_poll(),
            ci_health: true,
            ci_health_repo: String::new(),
            bare_mr_repo: String::new(),
            ci_health_days: default_ci_health_days(),
            avatars: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct GitlabConfig {
    /// Former way of naming the primary repository's project (now `[[repos]] primary`), still read:
    /// see `Config::primary`. Authentication is that of `glab`.
    #[serde(default)]
    pub project: String,
    /// Host of the instance (`gitlab.com`, or a self-hosted instance): links, `glab`, image
    /// registry (`registry.<host>`).
    #[serde(default = "default_gitlab_host")]
    pub host: String,
    /// Label a CI job watches to rebase then put the MR in auto-merge; empty (default): no
    /// such job, no Merge queue button.
    #[serde(default = "default_merge_queue_label")]
    pub merge_queue_label: String,
    /// Prefix of the labels that order the merge queue (`merge-priority::` for
    /// `merge-priority::0`): the lowest number first, then the MRs without one, the oldest first
    /// within each. Empty (default): the oldest first only.
    #[serde(default)]
    pub merge_priority_prefix: String,
}

impl Default for GitlabConfig {
    fn default() -> Self {
        Self {
            project: String::new(),
            host: default_gitlab_host(),
            merge_queue_label: default_merge_queue_label(),
            merge_priority_prefix: String::new(),
        }
    }
}

fn default_gitlab_host() -> String {
    "gitlab.com".into()
}

fn default_main_branch() -> String {
    String::new()
}

fn default_release_prefix() -> String {
    String::new()
}

fn default_merged_days() -> u32 {
    7
}

fn default_ci_health_days() -> u32 {
    14
}

fn default_merge_queue_label() -> String {
    String::new()
}

fn default_gitlab_poll() -> u64 {
    15
}
