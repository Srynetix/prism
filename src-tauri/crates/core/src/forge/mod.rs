//! The forge: where the repositories, their merge requests and their CI live. A profile speaks to
//! one (`[forge] provider`): GitLab, or GitHub. The window only sees `Forge` and the types below,
//! never a provider's own: a provider maps its API onto them. What only GitLab has (merge queue
//! label, redeploying a tag) goes through `Forge::gitlab`, which says so otherwise. `Forge`
//! itself, which reaches the provider, is in `prism-collectors` (`collectors::forge`).

use prism_base::i18n::tr;
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod cli;

/// The provider of the active profile's forge, in the snapshot: the window's vocabulary and links follow it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "ForgeProvider"))]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Gitlab,
    Github,
}

impl Provider {
    /// Its name, for messages: `GitLab`, `GitHub`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Gitlab => "GitLab",
            Self::Github => "GitHub",
        }
    }
}

/// Beyond this, an attached image is not displayed.
const IMAGE_MAX_BYTES: usize = 15 * 1024 * 1024;

/// An attached image as a `data:` URL: only PNG, JPEG, GIF and WebP (never an SVG, which can
/// carry script), recognized by their content rather than by what the server says.
pub fn image_data_url(bytes: &[u8]) -> Result<String> {
    use base64::Engine;
    if bytes.len() > IMAGE_MAX_BYTES {
        bail!(tr("image too large to preview"));
    }
    let mime = match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xff, 0xd8, 0xff, ..] => "image/jpeg",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => bail!(tr("not a displayable image")),
    };
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/* ---------- Review of a merge request ---------- */

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MrDetail {
    pub description: String,
    pub discussions: Vec<Discussion>,
    /// Users who approved, and the current user's state.
    pub approved_by: Vec<String>,
    pub approvals_left: i64,
    pub user_has_approved: bool,
    pub user_can_approve: bool,
    /// Reviewers whose latest word is a request for changes, and whether the current user's is.
    pub changes_requested_by: Vec<String>,
    pub user_requested_changes: bool,
    /// Who approved, took it back or asked for changes, and when; the pushes between, which may
    /// reset approvals. Oldest first.
    pub approval_history: Vec<ApprovalEvent>,
}

/// A step of an MR's approval history.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ApprovalEvent {
    pub at: DateTime<Utc>,
    pub kind: ApprovalKind,
    /// Who did it (`pushed`: who pushed, when the forge says so).
    pub author: String,
    pub author_name: String,
    /// `dismissed`: whose review; `reset`: whose approvals, as the forge words it.
    pub whom: Option<String>,
    /// `pushed`: the number of commits.
    pub commits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    Approved,
    /// The approver took it back (GitLab).
    Unapproved,
    ChangesRequested,
    /// A review dismissed by someone (GitHub).
    Dismissed,
    /// Approvals reset by a push (GitLab's project setting).
    Reset,
    Pushed,
}

/// The history in time order, consecutive pushes merged into one (GitHub lists commits one by one).
pub fn approval_history(mut events: Vec<ApprovalEvent>) -> Vec<ApprovalEvent> {
    events.sort_by_key(|e| e.at);
    let mut out: Vec<ApprovalEvent> = Vec::with_capacity(events.len());
    for e in events {
        match out.last_mut() {
            Some(last) if last.kind == ApprovalKind::Pushed && e.kind == ApprovalKind::Pushed => {
                last.commits += e.commits;
                last.at = e.at;
                if last.author != e.author {
                    (last.author, last.author_name) = (String::new(), String::new());
                }
            }
            _ => out.push(e),
        }
    }
    out
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Discussion {
    pub id: String,
    /// Standalone comment (not a reply thread).
    pub individual: bool,
    pub resolvable: bool,
    pub resolved: bool,
    /// File and line, for a thread placed on the diff.
    pub path: Option<String>,
    pub line: Option<u64>,
    /// The line is the old file's (deleted line), not the new one's.
    pub old_side: bool,
    /// A thread on several lines: the first one (`line` is the last, where it hangs).
    pub start_line: Option<u64>,
    pub notes: Vec<Note>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "MrNote"))]
pub struct Note {
    pub id: u64,
    pub author: String,
    pub author_name: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub new_file: bool,
    pub deleted_file: bool,
    pub renamed_file: bool,
    /// Unified diff of the file (the hunks, without the `---` / `+++` header); empty if it is too big.
    pub diff: String,
    /// The forge did not provide the content (binary file, too big, or beyond the limit).
    pub collapsed: bool,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MrDiff {
    pub files: Vec<FileDiff>,
    /// Files are missing: the size limit was reached.
    pub truncated: bool,
}

/// A commit of an MR, as its forge lists it.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MrCommit {
    pub sha: String,
    /// Its message's first line.
    pub title: String,
    pub author: String,
    /// When it was written (authored: a rebase keeps it).
    pub at: Option<DateTime<Utc>>,
    /// Its page on the forge.
    pub url: Option<String>,
}

/// Diff line targeted by a comment: old and new path, and the number in the old
/// file, the new one, or both for an unchanged line. On several lines, `start` is the first
/// one and this one the last, where the thread hangs.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LineRef {
    pub old_path: String,
    pub new_path: String,
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
    /// Where it sits in both files, a line added or removed included (GitLab's line code).
    #[serde(default)]
    pub pos: Option<(u64, u64)>,
    #[serde(default)]
    pub start: Option<LineAt>,
}

/// The first line of a range, in the same hunk as the last.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LineAt {
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
    pub pos: (u64, u64),
}

/// A project member, choice of assignee or reviewer.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Member {
    pub id: u64,
    pub username: String,
    pub name: String,
    pub avatar_url: Option<String>,
}

/// A label of a project, choice of an MR's labels.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ForgeLabel {
    pub name: String,
    /// `#rrggbb`, when the forge gives one.
    pub color: Option<String>,
    pub description: Option<String>,
}

/// An emoji a GitLab group made its own (`:party-parrot:`), drawn and offered with GitLab's.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CustomEmoji {
    pub name: String,
    /// Its picture: an https address, on the instance unless the group linked one from elsewhere.
    pub url: String,
}

/// What an MR's labels may be: neither empty nor with a comma (GitLab separates them with it), nor `.` or `..`, once each.
pub fn check_labels(labels: &[String]) -> Result<()> {
    for (i, l) in labels.iter().enumerate() {
        // `.` and `..` would be a path of the forge's API, not a label's name.
        anyhow::ensure!(!l.trim().is_empty() && l.trim() == l && !l.contains(',') && l != "." && l != "..", "{}", prism_base::tr!("invalid label: “{label}”", label = l));
        anyhow::ensure!(!labels[..i].contains(l), "{}", prism_base::tr!("label given twice: “{label}”", label = l));
    }
    Ok(())
}

/// The labels to add and remove to go from `shown` to `wanted`: only those, so a label set
/// elsewhere meanwhile (a CI job, the forge's page) stays.
pub fn label_changes<'a>(shown: &'a [String], wanted: &'a [String]) -> (Vec<&'a str>, Vec<&'a str>) {
    let add = wanted.iter().filter(|l| !shown.contains(l)).map(String::as_str).collect();
    let remove = shown.iter().filter(|l| !wanted.contains(l)).map(String::as_str).collect();
    (add, remove)
}

/* ---------- Jobs ---------- */

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct JobInfo {
    pub id: u64,
    pub name: String,
    pub stage: String,
    pub status: crate::state::PipelineStatus,
    pub allow_failure: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    /// Seconds.
    pub duration: Option<f64>,
    pub url: String,
    pub pipeline_id: u64,
    pub r#ref: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct JobLog {
    pub job: JobInfo,
    /// Raw log text, ANSI codes included (the front end interprets them).
    pub log: String,
    /// Start of the log cut off: only the end is there.
    pub truncated: bool,
}

#[cfg(test)]
mod label_tests {
    use super::*;

    #[test]
    fn changes_only_the_labels_that_move() {
        let shown = vec!["bug".to_string(), "merge-queue".into(), "backend".into()];
        let wanted = vec!["bug".to_string(), "backend".into(), "security".into()];
        assert_eq!(label_changes(&shown, &wanted), (vec!["security"], vec!["merge-queue"]));
        assert_eq!(label_changes(&shown, &shown), (vec![], vec![]));
    }

    #[test]
    fn refuses_a_label_gitlab_would_split_or_trim() {
        assert!(check_labels(&["bug".into(), "scope::api".into(), "needs review".into()]).is_ok());
        assert!(check_labels(&["a,b".into()]).is_err());
        assert!(check_labels(&[" bug".into()]).is_err());
        assert!(check_labels(&["".into()]).is_err());
        assert!(check_labels(&["bug".into(), "bug".into()]).is_err());
        assert!(check_labels(&["..".into()]).is_err());
    }
}
