//! The repositories (`[[repos]]`) and their worktrees.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A working folder (`[[workspaces]]`): where projects of one kind live on this machine (work,
/// personal…). Settings creates and clones projects in one of them unless told otherwise; they are
/// the only folders a phone creates projects in (`[remote] projects`). Personal: a pack does not
/// set them.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Workspace {
    /// What the pickers show (`Work`, `Personal`); empty: the folder's name.
    #[serde(default)]
    pub name: String,
    /// The folder (`~/Development`).
    pub dir: PathBuf,
    /// The forge group (GitLab) or owner (GitHub) offered for its projects; empty: your own account.
    #[serde(default)]
    pub namespace: String,
    /// The forge its projects go on (`gitlab`, `github`): personal projects on GitHub in a GitLab
    /// profile, say. Absent: the profile's (`[forge] provider`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge: Option<crate::forge::Provider>,
}

fn seen() -> bool {
    true
}

fn is_seen(b: &bool) -> bool {
    *b
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RepoConfig {
    pub name: String,
    /// Local clone: Repos menu and base images. Absent: the repository is only used for its MRs, read
    /// from GitLab (`project` and `merge_requests`).
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// GitLab project (`group/project`), for links.
    #[serde(default)]
    pub project: Option<String>,
    /// Its MRs in the Merge requests block (the project is required). Always on for the primary one.
    #[serde(default)]
    pub merge_requests: bool,
    /// The primary repository, at most one: its MRs are named `!iid`, and the pipelines of its
    /// protected branches, delivery, vulnerabilities, Renovate and CI health are read. Without
    /// one, Prism follows only the MRs of the other repositories.
    #[serde(default)]
    pub primary: bool,
    /// Its worktrees in the Worktrees view, and agents' new worktrees there; the Claude actions on
    /// its MRs (a local clone is required).
    #[serde(default)]
    pub worktrees: bool,
    /// The merge queue (`[gitlab] merge_queue_label`) on its MRs: a CI job of the repository watches
    /// the label.
    #[serde(default)]
    pub merge_queue: bool,
    /// A ticket id is expected in its MRs' title or branch (Alerts flags those without).
    #[serde(default)]
    pub ticket_required: bool,
    /// The pipelines of its protected branches (main and release ones, or `pipeline_branches`) and
    /// its jobs waiting for a runner: the branch banner, CI alerts, the header's count (the project
    /// is required).
    #[serde(default)]
    pub pipelines: bool,
    /// The branches whose pipeline is followed (`pipelines`), in this order: names, `*` patterns
    /// (`env/*`, 20 branches at most each), `@main` (the main branch, whichever it is) and
    /// `@release` (the release prefix's branches). Absent: `[forge] pipeline_branches`, empty
    /// there: the main branch and the release ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_branches: Option<Vec<String>>,
    /// Main branch. Absent: `[forge] main_branch` for the primary repository, the remote's
    /// default branch for the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_branch: Option<String>,
    /// Release branch prefix (Git Flow), empty for none. Absent: `[forge] release_prefix` for the
    /// primary repository, none for the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_prefix: Option<String>,
    /// Base of new worktrees: `auto` (the latest release, otherwise the main branch) or a branch.
    /// Absent: `[worktrees] base_branch` for the primary repository, `auto` for the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    /// The agent CLI of the agents launched there (its clone, its worktrees). Absent: `[agents] provider`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<crate::agents::provider::Provider>,
    /// The forge its project is on, when it is not the profile's (`[forge] provider`): a personal
    /// project on GitHub in a GitLab profile. Only its MRs are read there (`merge_requests`, by that
    /// forge's own login): no pipelines, merge queue, nor primary. Absent (or the profile's): the profile's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge: Option<crate::forge::Provider>,
    /// Other names it goes by, for Prism's assistant (« the shop app », « the front »): it finds the
    /// repository a request is about by its name, these, its description and its memory.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// What it is, in a sentence, for Prism's assistant (« the online shop: Svelte front, Rust server »).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Seen by Prism's assistant (`[agents.assistant]`). Off: it neither lists, reads, explores nor
    /// remembers anything of this repository, its tickets nor its MRs; the agents you launch there
    /// yourself work as before.
    #[serde(default = "seen", skip_serializing_if = "is_seen")]
    pub assistant: bool,
}

/// A repository whose worktrees Prism lists and creates, with its branches resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct WorktreeRepo {
    pub name: String,
    pub path: PathBuf,
    /// Empty: the remote's default branch, read from the clone.
    pub main_branch: String,
    /// Empty: no release branches.
    pub release_prefix: String,
    pub base_branch: String,
}

/// A repository whose protected branches' pipelines are followed, its branches resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineRepo {
    pub name: String,
    /// `None` for the primary repository (as on its MRs).
    pub repo: Option<String>,
    pub project: String,
    /// Empty: the project's default branch on GitLab.
    pub main_branch: String,
    /// Empty: no release branches.
    pub release_prefix: String,
    /// The branches followed (`[[repos]] pipeline_branches`); empty: the main and release ones.
    pub branches: Vec<String>,
}

/// A repository's followed branches as the forge has them (Settings › Repositories' preview):
/// those whose pipeline is read, in order, and the entries the forge has no branch for.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PipelineBranches {
    pub refs: Vec<String>,
    /// A name with no such branch, a pattern matching none (`@release` excepted: no release yet is normal).
    pub missing: Vec<String>,
}

/// What the frontend knows of a repository: its role and switches.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RepoInfo {
    pub name: String,
    pub project: Option<String>,
    pub primary: bool,
    /// A local clone.
    pub clone: bool,
    /// Its path names a folder that does not exist (moved, or never cloned there).
    pub clone_missing: bool,
    pub worktrees: bool,
    pub merge_queue: bool,
    pub ticket_required: bool,
    pub pipelines: bool,
    /// Its agent CLI; absent: `[agents] provider`.
    pub agent: Option<crate::agents::provider::Provider>,
    /// The forge its project is on, when not the profile's: its MRs' vocabulary (`#12`, `!12`).
    pub forge: Option<crate::forge::Provider>,
    /// Where its MRs are on the forge (`https://gitlab.com/g/p/-/merge_requests/`), when Prism
    /// reads them: an agent working there names them `!12`, a link to one Prism does not know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub mr_web: Option<String>,
}

impl RepoConfig {
    /// The local clone; only called on a repository that has one (Repos menu, base images).
    pub fn dir(&self) -> &Path {
        self.path.as_deref().unwrap_or(Path::new(""))
    }

    /// Its worktrees are listed, and agents can create theirs there.
    pub fn has_worktrees(&self) -> bool {
        self.path.is_some() && self.worktrees
    }

    pub fn info(&self) -> RepoInfo {
        RepoInfo {
            name: self.name.clone(),
            // On another forge: nothing this profile's forge acts on (an agent's MR, a pipeline).
            project: self.project.clone().filter(|_| self.forge.is_none()),
            primary: self.primary,
            clone: self.path.is_some(),
            clone_missing: self.path.as_deref().is_some_and(|p| !p.is_dir()),
            worktrees: self.has_worktrees(),
            merge_queue: self.merge_queue,
            ticket_required: self.ticket_required,
            pipelines: self.pipelines && self.project.is_some(),
            agent: self.agent,
            forge: self.forge,
            mr_web: None,
        }
    }

    /// `info`, with where its MRs are when Prism reads them (`forge`: the forge is read at all;
    /// `profile`: the profile's forge, `gitlab_host` its instance on GitLab).
    pub fn info_on(&self, forge: bool, profile: crate::forge::Provider, gitlab_host: &str) -> RepoInfo {
        let read = forge && (self.primary || self.merge_requests);
        let mr_web = self.project.as_deref().filter(|p| read && !p.is_empty()).map(|p| match self.forge.unwrap_or(profile) {
            crate::forge::Provider::Github => format!("https://github.com/{p}/pull/"),
            crate::forge::Provider::Gitlab => format!("https://{gitlab_host}/{p}/-/merge_requests/"),
        });
        RepoInfo { mr_web, ..self.info() }
    }

    /// Name and project of a non-primary repository whose MRs are followed on the profile's forge
    /// (the primary repository's are read with its pipelines).
    pub fn mr_project(&self) -> Option<(String, String)> {
        self.project.clone().filter(|_| self.merge_requests && !self.primary && self.forge.is_none()).map(|p| (self.name.clone(), p))
    }

    /// The same on the other forge (`forge`): its MRs are read by that forge's collector.
    pub fn mr_project_elsewhere(&self) -> Option<(String, String)> {
        self.project.clone().filter(|_| self.merge_requests && self.forge.is_some()).map(|p| (self.name.clone(), p))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorktreesConfig {
    /// Former way of naming the primary repository's clone (now `[[repos]] primary`), still read:
    /// see `Config::primary`.
    #[serde(default)]
    pub repo: PathBuf,
    #[serde(default = "default_base")]
    pub base_branch: String,
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
    /// With no commit nor session for this many days, a worktree is "stale".
    #[serde(default = "default_stale_days")]
    pub stale_days: u32,
}

fn default_stale_days() -> u32 {
    14
}

impl Default for WorktreesConfig {
    fn default() -> Self {
        Self { repo: PathBuf::new(), base_branch: default_base(), poll_seconds: default_poll(), stale_days: default_stale_days() }
    }
}

fn default_base() -> String {
    "auto".into()
}

fn default_poll() -> u64 {
    5
}
