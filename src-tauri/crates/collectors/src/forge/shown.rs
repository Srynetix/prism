//! What both forges' actions share: the MRs and pipelines Prism displays (the window cannot target
//! others), the snapshot following a pipeline's new status right away, and an MR's diff capped.

use crate::state::{Hub, MergeRequest, PipelineStatus};

use super::{FileDiff, MrDiff};

/// Beyond this, the following files of a diff are no longer loaded: the panel says so.
pub const DIFF_MAX_BYTES: usize = 3 * 1024 * 1024;
/// Beyond this, a file is not read whole for its preview.
pub const FILE_MAX_BYTES: usize = 2 * 1024 * 1024;

/// The MR (repository, number): `repo` is `None` for the primary repository.
pub fn is(m: &MergeRequest, repo: Option<&str>, iid: u64) -> bool {
    m.iid == iid && m.repo.as_deref() == repo
}

/// The MR as Prism displays it, if it does.
pub fn mr(hub: &Hub, repo: Option<&str>, iid: u64) -> Option<MergeRequest> {
    hub.subscribe().borrow().merge_requests.iter().find(|m| is(m, repo, iid)).cloned()
}

/// The status of a pipeline Prism displays: a repository's branch's, or an MR's head one.
pub fn pipeline_status(hub: &Hub, id: u64) -> Option<PipelineStatus> {
    let snap = hub.subscribe();
    let snap = snap.borrow();
    snap.pipelines.iter().chain(snap.merge_requests.iter().filter_map(|m| m.head_pipeline.as_ref())).find(|p| p.id == id).map(|p| p.status)
}

/// The pipeline's status everywhere the window shows it, right away, without waiting for the next
/// collection; `clear_failed`: its failed jobs run again.
pub fn set_pipeline_status(hub: &Hub, id: u64, status: PipelineStatus, clear_failed: bool) {
    hub.update(|s| {
        let mut changed = false;
        for p in s.pipelines.iter_mut().chain(s.merge_requests.iter_mut().filter_map(|m| m.head_pipeline.as_mut())) {
            if p.id == id {
                changed |= p.status != status || (clear_failed && !p.failed_jobs.is_empty());
                p.status = status;
                if clear_failed {
                    p.failed_jobs.clear();
                }
            }
        }
        for m in s.merge_requests.iter_mut().filter(|m| m.head_pipeline.as_ref().is_some_and(|p| p.id == id)) {
            changed |= m.pipeline != Some(status);
            m.pipeline = Some(status);
        }
        changed
    });
}

/// An MR's diff, file by file, up to `DIFF_MAX_BYTES`: the files beyond come collapsed, empty.
#[derive(Default)]
pub struct Diff {
    files: Vec<FileDiff>,
    size: usize,
    truncated: bool,
}

impl Diff {
    pub fn push(&mut self, mut file: FileDiff) {
        self.size += file.diff.len();
        if self.size > DIFF_MAX_BYTES {
            self.truncated = true;
            file.collapsed = true;
            file.diff.clear();
        }
        self.files.push(file);
    }

    pub fn finish(self) -> MrDiff {
        MrDiff { files: self.files, truncated: self.truncated }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, bytes: usize) -> FileDiff {
        FileDiff { old_path: name.into(), new_path: name.into(), new_file: false, deleted_file: false, renamed_file: false, diff: "x".repeat(bytes), collapsed: false }
    }

    #[test]
    fn a_diff_stops_loading_past_its_cap() {
        let mut diff = Diff::default();
        diff.push(file("a", DIFF_MAX_BYTES - 10));
        diff.push(file("b", 10));
        diff.push(file("c", 1));
        let d = diff.finish();
        assert!(d.truncated);
        assert_eq!(d.files.iter().map(|f| (f.collapsed, f.diff.len())).collect::<Vec<_>>(), [(false, DIFF_MAX_BYTES - 10), (false, 10), (true, 0)]);
    }
}
