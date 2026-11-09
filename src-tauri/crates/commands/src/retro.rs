//! My sprint's retro (`prism_blocks::trail::retro`) on what the core holds: the snapshot, the Day
//! trail's journal, the notes and the store `retro` (the points, the sprints kept). The window shows
//! it (`retro_get`); the routine `builtin:retro-draft` drafts from it, the window open or not.

use chrono::Utc;
use prism_blocks::trail::retro::{self as r, Retro, RetroPoint, RetroSummary};
use prism_blocks::trail::TrailState;
use serde_json::Value;

use crate::{state::Hub, store::Store};

/// The current sprint's retro now, its Markdown with `points` (the window's, as it edits them; else
/// the store's); none without a sprint that has a start date.
pub fn current(hub: &Hub, store: &Store, points: Option<Vec<RetroPoint>>) -> Option<Retro> {
    let saved = store.load("retro").inspect_err(|e| tracing::warn!("retro: {e:#}")).ok().flatten();
    let notes = match store.load("notes") {
        Ok(Some(Value::Array(notes))) => notes,
        _ => Vec::new(),
    };
    let snap = hub.subscribe();
    let s = snap.borrow();
    let trail: Option<TrailState> = s.blocks.get("trail").and_then(|v| serde_json::from_value(v.clone()).ok());
    let mut retro = r::retro(&s, trail.as_ref(), &notes, &r::history_of(saved.as_ref()), Utc::now())?;
    let points = points.unwrap_or_else(|| r::points_of(saved.as_ref(), &retro.name));
    retro.markdown = r::markdown(&retro, &points);
    Some(retro)
}

/// The current sprint's figures kept in the store's history, to compare the next sprints with;
/// `migrated`, the window's former pref `retro.history`, moved in when the store has none. The other
/// pages read it again.
pub fn keep(hub: &Hub, store: &Store, migrated: Option<Vec<RetroSummary>>) -> anyhow::Result<()> {
    let saved = store.load("retro")?;
    let summary = current(hub, store, Some(Vec::new())).map(|r| r.summary);
    if let Some(value) = r::kept_in(saved, summary.as_ref(), migrated) {
        store.save("retro", &value)?;
        crate::commands::system::shared_written(hub, "retro");
    }
    Ok(())
}

/// What the routine `builtin:retro-draft` drafts from: the retro's Markdown, its points with it,
/// the sprint's figures kept on the way.
pub fn draft(hub: &Hub, store: &Store) -> Option<String> {
    if let Err(e) = keep(hub, store, None) {
        tracing::warn!("retro: keeping the sprint's figures: {e:#}");
    }
    current(hub, store, None).map(|r| r.markdown)
}
