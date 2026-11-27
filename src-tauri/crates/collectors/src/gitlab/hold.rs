//! MRs held out of the merge queue (`prism_core::queue_hold`): put back into it once the MR they
//! wait for is merged, forgotten when there is nothing more to wait for. Follows the snapshot.

use prism_core::queue_hold::{self, Outcome};

use super::Actions;
use crate::state::Hub;

pub async fn run(actions: Actions, hub: Hub) {
    queue_hold::load(&hub);
    let mut rx = hub.subscribe();
    loop {
        let due = queue_hold::due(&rx.borrow_and_update());
        for (h, outcome) in due {
            // Forgotten first: a failure is said once, not at each change of the snapshot.
            queue_hold::release(&hub, h.repo.as_deref(), h.iid);
            if outcome != Outcome::Requeue {
                tracing::info!("queue hold: {}!{} no longer held", h.repo.as_deref().unwrap_or_default(), h.iid);
                continue;
            }
            match actions.set_merge_queue(&hub, h.repo.as_deref(), h.iid, true).await {
                Ok(()) => {
                    tracing::info!("queue hold: {}!{} back in the merge queue", h.repo.as_deref().unwrap_or_default(), h.iid);
                    hub.forge_changed();
                }
                Err(e) => tracing::warn!("queue hold: {}!{} not put back in the merge queue: {e:#}", h.repo.as_deref().unwrap_or_default(), h.iid),
            }
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}
