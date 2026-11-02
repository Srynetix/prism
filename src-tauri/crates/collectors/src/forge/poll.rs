//! What GitHub's and GitLab's poll loops share: the protected branches, re-listed rarely; every
//! repository's pipelines read at once; a cycle merged into the snapshot, a repository unreadable
//! this time keeping what it had; the errors, the primary repository's apart from the others'.
//! Each provider brings only its reads.

use std::{collections::HashMap, future::Future, time::Duration};

use anyhow::Result;

use crate::{
    config::PipelineRepo,
    state::{MergeRequest, PendingJob, Pipeline, Snapshot},
};

/// A repository's protected branches' latest pipelines, and the jobs waiting for a runner on them.
pub type Pipelines = Result<(Vec<Pipeline>, Vec<PendingJob>)>;

/// Branches whose latest pipeline is followed: the main one and the release ones, re-listed this often.
const REFS_EVERY: Duration = Duration::from_secs(600);

/// The protected branches of each pipeline repository, by `[[repos]] name`, and when they were listed.
#[derive(Default)]
pub struct Refs(HashMap<String, (std::time::Instant, Vec<String>)>);

impl Refs {
    /// `r`'s branches: the cached ones while fresh, otherwise `list`'s.
    async fn of<'a, F: Future<Output = Result<Vec<String>>>>(&mut self, r: &'a PipelineRepo, list: impl FnOnce(&'a PipelineRepo) -> F) -> Result<Vec<String>> {
        if let Some((_, refs)) = self.0.get(&r.name).filter(|(at, _)| at.elapsed() < REFS_EVERY) {
            return Ok(refs.clone());
        }
        let refs = list(r).await?;
        self.0.insert(r.name.clone(), (std::time::Instant::now(), refs.clone()));
        Ok(refs)
    }
}

/// Every pipeline repository's pipelines, by repository (`None`: the primary one): the branches
/// first (one at a time, mostly cached), then every repository's pipelines at once.
pub async fn pipelines<'a, L, LF, R, RF>(repos: &'a [PipelineRepo], refs: &mut Refs, list: L, read: R) -> Vec<(Option<String>, Pipelines)>
where
    L: Fn(&'a PipelineRepo) -> LF,
    LF: Future<Output = Result<Vec<String>>>,
    R: Fn(&'a PipelineRepo, Vec<String>) -> RF,
    RF: Future<Output = Pipelines>,
{
    let mut branches = Vec::new();
    for r in repos {
        branches.push(refs.of(r, &list).await);
    }
    futures::future::join_all(repos.iter().zip(branches).map(|(r, b)| {
        let got = b.map(|b| read(r, b));
        async move {
            let got = match got {
                Ok(read) => read.await,
                Err(e) => Err(e),
            };
            (r.repo.clone(), got)
        }
    }))
    .await
}

/// A cycle into the snapshot: each repository's MRs and the jobs queued on them (`None`:
/// unreadable), its pipelines, the MRs of the repositories on the other forge as they were, and
/// the login. A repository unreadable this time keeps its MRs, pipelines and queued jobs. An MR's
/// queued job targets it by `#<number>`, a branch's by the branch's name. Whether it changed.
pub fn merge<'a>(s: &mut Snapshot, me: Option<String>, mrs: impl IntoIterator<Item = (&'a Option<String>, Option<(&'a [MergeRequest], &'a [PendingJob])>)>, pipelines: &[(Option<String>, Pipelines)]) -> bool {
    let mut merge_requests = Vec::new();
    let mut pending = Vec::new();
    for (repo, got) in mrs {
        match got {
            Some((m, j)) => {
                merge_requests.extend(m.iter().cloned());
                pending.extend(j.iter().cloned());
            }
            None => {
                merge_requests.extend(s.merge_requests.iter().filter(|m| &m.repo == repo && m.forge.is_none()).cloned());
                pending.extend(s.pending_jobs.iter().filter(|j| &j.repo == repo && j.target.starts_with('#')).cloned());
            }
        }
    }
    let mut list = Vec::new();
    for (repo, got) in pipelines {
        match got {
            Ok((p, j)) => {
                list.extend(p.iter().cloned());
                pending.extend(j.iter().cloned());
            }
            Err(_) => {
                list.extend(s.pipelines.iter().filter(|p| &p.repo == repo).cloned());
                pending.extend(s.pending_jobs.iter().filter(|j| &j.repo == repo && !j.target.starts_with('#')).cloned());
            }
        }
    }
    merge_requests.extend(super::elsewhere::kept(s));
    let me = me.or_else(|| s.me.clone());
    let changed = s.me != me || s.merge_requests != merge_requests || s.pipelines != list || s.pending_jobs != pending;
    s.me = me;
    s.merge_requests = merge_requests;
    s.pipelines = list;
    s.pending_jobs = pending;
    changed
}

/// The cycle's errors, the primary repository's (`primary`, as is) or the others' (each named).
pub fn errors<'a>(mrs: impl IntoIterator<Item = (&'a Option<String>, Option<&'a anyhow::Error>)>, pipelines: &'a [(Option<String>, Pipelines)], primary: bool) -> Vec<String> {
    mrs.into_iter()
        .chain(pipelines.iter().map(|(r, got)| (r, got.as_ref().err())))
        .filter(|(r, _)| r.is_none() == primary)
        .filter_map(|(r, e)| {
            let e = e?;
            Some(match r {
                Some(r) => format!("{r}: {e:#}"),
                None => format!("{e:#}"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mr(repo: Option<&str>, iid: u64) -> MergeRequest {
        MergeRequest { repo: repo.map(Into::into), iid, ..Default::default() }
    }

    /// A repository unreadable this time keeps its MRs and the jobs queued on them; the others
    /// take what they read, an unreadable repository's pipelines staying too.
    #[test]
    fn an_unreadable_repository_keeps_what_it_had() {
        let mut s = Snapshot::default();
        let api = Some("api".to_string());
        s.merge_requests = vec![mr(None, 1), mr(Some("api"), 2)];
        s.pending_jobs = vec![PendingJob { repo: api.clone(), name: "test".into(), target: "#2".into(), queued_since: Default::default() }];
        s.pipelines = vec![Pipeline { repo: api.clone(), ..Default::default() }];
        let fresh = [mr(None, 3)];
        let failed: Pipelines = Err(anyhow::anyhow!("down"));
        let pipelines = vec![(api.clone(), failed)];
        let changed = merge(&mut s, Some("me".into()), [(&None, Some((&fresh[..], &[][..]))), (&api, None)], &pipelines);
        assert!(changed);
        assert_eq!(s.merge_requests.iter().map(|m| m.iid).collect::<Vec<_>>(), [3, 2]);
        assert_eq!((s.pending_jobs.len(), s.pipelines.len(), s.me.as_deref()), (1, 1, Some("me")));
        assert_eq!(errors([(&api, None)], &pipelines, false), ["api: down"]);
        assert!(errors([(&api, None)], &pipelines, true).is_empty());
    }
}
