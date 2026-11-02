//! The repositories on the other forge than the profile's (`[[repos]] forge`): their MRs, read by
//! that forge's own collector beside the profile's. Each replaces only its own in the snapshot:
//! the profile's collector keeps those of the other forge as they are, and the reverse.

use crate::state::{Hub, MergeRequest, Snapshot};

use super::Provider;

/// The other forge's MRs, as the profile's collector keeps them.
pub fn kept(s: &Snapshot) -> impl Iterator<Item = MergeRequest> + '_ {
    s.merge_requests.iter().filter(|m| m.forge.is_some()).cloned()
}

/// The MRs read on `provider` this cycle, per repository (`None`: unreadable, its MRs stay as they
/// were), and the login there.
pub fn set(hub: &Hub, provider: Provider, me: Option<String>, read: Vec<(String, Option<Vec<MergeRequest>>)>) {
    hub.update(|s| {
        let mut list: Vec<MergeRequest> = s.merge_requests.iter().filter(|m| m.forge.is_none()).cloned().collect();
        for (repo, got) in &read {
            match got {
                Some(mrs) => list.extend(mrs.iter().cloned().map(|m| MergeRequest { forge: Some(provider), ..m })),
                None => list.extend(kept(s).filter(|m| m.repo.as_deref() == Some(repo.as_str()))),
            }
        }
        let me = me.clone().or_else(|| s.me_elsewhere.clone());
        let changed = s.merge_requests != list || s.me_elsewhere != me;
        s.merge_requests = list;
        s.me_elsewhere = me;
        changed
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mr(repo: Option<&str>, iid: u64, forge: Option<Provider>) -> MergeRequest {
        MergeRequest { iid, repo: repo.map(Into::into), forge, ..Default::default() }
    }

    #[test]
    fn each_forge_replaces_only_its_own_mrs() {
        let hub = Hub::new();
        hub.update(|s| {
            s.merge_requests = vec![mr(None, 1, None), mr(Some("mine"), 7, Some(Provider::Github)), mr(Some("perso"), 3, Some(Provider::Github))];
            true
        });
        // `mine` read again, `perso` unreadable: its MR stays.
        set(&hub, Provider::Github, Some("me".into()), vec![("mine".into(), Some(vec![mr(Some("mine"), 8, None)])), ("perso".into(), None)]);
        let s = hub.subscribe().borrow().clone();
        let got: Vec<_> = s.merge_requests.iter().map(|m| (m.repo.clone(), m.iid, m.forge)).collect();
        assert_eq!(got, [(None, 1, None), (Some("mine".into()), 8, Some(Provider::Github)), (Some("perso".into()), 3, Some(Provider::Github))]);
        assert_eq!(s.me_elsewhere.as_deref(), Some("me"));
        assert_eq!(kept(&s).count(), 2);
    }
}
