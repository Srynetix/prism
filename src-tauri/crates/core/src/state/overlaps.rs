//! Overlaps between the agents and worktrees of a repository (`prism_collectors::overlaps`): two
//! of them changing the same lines (a merge conflict all but certain) or the same file, told
//! before the merge rather than at the rebase.

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Overlaps {
    /// One per pair of worktrees, the highest level first.
    pub pairs: Vec<Overlap>,
    /// The worktrees with changes on another base than their repository's: not compared with
    /// those of that base (only with those of their own).
    pub other_base: Vec<OverlapElsewhere>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum OverlapLevel {
    /// Different lines of the same file: a clean merge is likely, its sense to check.
    File,
    /// Hunks that cover the same lines of the base: a merge conflict all but certain.
    Lines,
}

/// Two worktrees of a repository on the same base, their changes against it sharing files.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Overlap {
    pub repo: String,
    pub base: String,
    /// The worktrees' folders (`Worktree::path`), and their branches.
    pub a: String,
    pub b: String,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a_branch: Option<String>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b_branch: Option<String>,
    /// The highest of its files'.
    pub level: OverlapLevel,
    pub files: Vec<OverlapFile>,
    /// `a` is merged (its branch gone from the remote, or its worktree removed): `b` has to rebase
    /// on it, until its changes no longer meet what `a` brought to the base.
    pub merged: bool,
    /// What the shared hunks are, both sides: an overlap ignored stays so until it changes.
    pub key: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OverlapFile {
    /// Relative to the repository's top, as it is in the base.
    pub path: String,
    pub level: OverlapLevel,
    /// Each side's hunks in this file, in the lines of its base (the side's own merge-base): at the
    /// lines level, only those that meet the other side's.
    pub a: Vec<LineRange>,
    pub b: Vec<LineRange>,
}

/// Lines `start`..`start + len` of the base; `len` 0: lines inserted after line `start`.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LineRange {
    pub start: u32,
    pub len: u32,
}

/// A worktree not compared with those of its repository's base: "not compared: other base".
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OverlapElsewhere {
    pub repo: String,
    pub path: String,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub base: String,
}
