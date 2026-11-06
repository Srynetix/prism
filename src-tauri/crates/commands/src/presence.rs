//! Profiles open side by side on this computer, each in its own Prism (the app's, or one without a
//! window shown in a window of the app): each one writes what its agents are doing
//! (`profile::Presence`) and reads the others', for the status bar, the menu bar's count and the
//! notifications (`other_profiles`).

use std::time::Duration;

use prism_core::{
    profile::{self, Presence},
    state::{AgentState, Hub, OtherProfile, SessionStatus},
};

/// Often enough for an agent waiting elsewhere to show within seconds; a file read per profile.
const EVERY: Duration = Duration::from_secs(3);
/// The profiles (their names, colors) are listed again this often: a config read each.
const LIST_EVERY: Duration = Duration::from_secs(30);

/// This Prism's agents, as the others see them.
fn mine(hub: &Hub) -> Presence {
    let snap = hub.subscribe();
    let snap = snap.borrow();
    let running: Vec<_> = snap.agents.iter().filter(|a| a.state == AgentState::Running).collect();
    let mut waiting: Vec<_> = running.iter().filter(|a| matches!(a.status, SessionStatus::WaitingTool | SessionStatus::NeedsYou | SessionStatus::ReviewReady)).collect();
    waiting.sort_by_key(|a| a.last_activity);
    let waiting_agents = waiting
        .iter()
        .map(|a| prism_core::state::WaitingAgent { id: a.id.clone(), title: a.title.clone(), status: serde_json::to_value(a.status).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(), profile: None })
        .collect();
    Presence { agents: running.len() as u32, waiting: waiting.len() as u32, first: waiting.first().map(|a| a.title.clone()), waiting_agents }
}

pub async fn run(hub: Hub) {
    let mut written: Option<Presence> = None;
    let mut known: Vec<(String, String, String)> = Vec::new();
    let mut listed: Option<std::time::Instant> = None;
    loop {
        let now = mine(&hub);
        if written.as_ref() != Some(&now) {
            match profile::write_presence(&now) {
                Ok(()) => written = Some(now),
                Err(e) => tracing::warn!("presence: {e:#}"),
            }
        }
        // Written always (the others read it), read only for a window or a phone that shows it,
        // and always by the app holding the windows: it notifies the others' agents waiting.
        if !hub.shown() && !profile::is_host() {
            tokio::time::sleep(EVERY).await;
            continue;
        }
        if listed.is_none_or(|at| at.elapsed() >= LIST_EVERY) {
            known = profile::list().await.into_iter().filter(|p| !p.active).map(|p| (p.id, p.name, p.color)).collect();
            listed = Some(std::time::Instant::now());
        }
        let others: Vec<OtherProfile> = known
            .iter()
            .filter_map(|(id, name, color)| {
                let p = profile::presence_of(id)?;
                Some(OtherProfile { id: id.clone(), name: name.clone(), color: color.clone(), agents: p.agents, waiting: p.waiting, first: p.first, waiting_agents: p.waiting_agents })
            })
            .collect();
        hub.update(|s| {
            let changed = s.other_profiles != others;
            s.other_profiles = others;
            changed
        });
        tokio::time::sleep(EVERY).await;
    }
}
