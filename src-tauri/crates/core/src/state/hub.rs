//! The hub the collectors publish into and the window reads: the snapshot, and the pace of the collections.

use chrono::Utc;
use tokio::sync::watch;

use super::*;

#[derive(Clone)]
pub struct Hub {
    tx: watch::Sender<Snapshot>,
    /// Window shown: when it is hidden (menu bar only), collections
    /// from the APIs slow down, and resume right away when it reappears.
    visible: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Phones watching through the remote: they keep the pace as a shown window does.
    watching: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    shown: std::sync::Arc<tokio::sync::Notify>,
    /// An action from Prism changed the forge (merge, approval, comment, retry…): its reads come
    /// again at once rather than at their pace.
    forge: std::sync::Arc<watch::Sender<()>>,
    /// What the alerts read of the stores the window and the phones share (`prefs`).
    prefs: std::sync::Arc<std::sync::RwLock<std::sync::Arc<crate::alerts::Prefs>>>,
    /// The snapshot changed: the alerts are computed again (`crate::alerts::run`).
    alerts_wake: std::sync::Arc<tokio::sync::Notify>,
    /// One computation of the alerts at a time: the one that read the later snapshot publishes last.
    refreshing: std::sync::Arc<std::sync::Mutex<()>>,
}

/// Minimum collection pace when the window is hidden.
const HIDDEN_MIN: std::time::Duration = std::time::Duration::from_secs(60);

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(Snapshot::default());
        Self { tx, visible: Default::default(), watching: Default::default(), shown: Default::default(), forge: std::sync::Arc::new(watch::channel(()).0), prefs: Default::default(), alerts_wake: Default::default(), refreshing: Default::default() }.with_visible(true)
    }

    fn with_visible(self, v: bool) -> Self {
        self.set_visible(v);
        self
    }

    pub fn set_visible(&self, v: bool) {
        let was = self.visible.swap(v, std::sync::atomic::Ordering::Relaxed);
        if v && !was {
            self.shown.notify_waiters();
        }
    }

    /// A phone starts (`true`) or stops watching through the remote.
    pub fn set_watching(&self, on: bool) {
        use std::sync::atomic::Ordering::Relaxed;
        if on {
            if self.watching.fetch_add(1, Relaxed) == 0 {
                self.shown.notify_waiters();
            }
        } else {
            let _ = self.watching.try_update(Relaxed, Relaxed, |n| n.checked_sub(1));
        }
    }

    /// The window is shown, or a phone watches.
    pub fn shown(&self) -> bool {
        self.visible.load(std::sync::atomic::Ordering::Relaxed) || self.watching.load(std::sync::atomic::Ordering::Relaxed) > 0
    }

    /// Wait between two collections: `every` with the window shown or a phone watching; four
    /// times longer (at least one minute) otherwise, cut short as soon as either comes back.
    pub async fn pause(&self, every: std::time::Duration) {
        if self.shown() {
            tokio::time::sleep(every).await;
            return;
        }
        let shown = self.shown.notified();
        tokio::select! {
            _ = tokio::time::sleep((every * 4).max(HIDDEN_MIN)) => {}
            _ = shown => {}
        }
    }

    /// The forge was changed from Prism: its reads waiting in `pause_forge` come again now.
    pub fn forge_changed(&self) {
        self.forge.send_replace(());
    }

    /// What a forge read waits on in `pause_forge`, taken before its first read.
    pub fn forge_changes(&self) -> watch::Receiver<()> {
        self.forge.subscribe()
    }

    /// `pause`, cut short when the forge was changed from Prism since the last read began
    /// (`forge_changed`): `true` then, the next read takes nothing from its caches.
    pub async fn pause_forge(&self, every: std::time::Duration, changes: &mut watch::Receiver<()>) -> bool {
        let changed = tokio::select! {
            _ = self.pause(every) => changes.has_changed().unwrap_or(false),
            Ok(()) = changes.changed() => true,
        };
        changes.mark_unchanged();
        changed
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.tx.subscribe()
    }

    /// Applies `f` and only notifies clients if the snapshot actually changed; then its alerts are
    /// computed again, shortly and away from the lock (`crate::alerts::run`; `refresh_alerts` now).
    pub fn update(&self, f: impl FnOnce(&mut Snapshot) -> bool) {
        let changed = self.tx.send_if_modified(|s| {
            let changed = f(s);
            if changed {
                s.updated_at = Some(Utc::now());
            }
            changed
        });
        if changed {
            self.alerts_wake.notify_one();
        }
    }

    /// What wakes `crate::alerts::run`: a change of the snapshot.
    pub fn alerts_wake(&self) -> std::sync::Arc<tokio::sync::Notify> {
        std::sync::Arc::clone(&self.alerts_wake)
    }

    /// The alerts computed again now, on a copy of the snapshot (its lock held only to copy it and
    /// to publish them), the snapshot unchanged otherwise: by `crate::alerts::run`, once the shared
    /// prefs changed, and by the tests. Clients notified only when they changed; nothing woken.
    pub fn refresh_alerts(&self) {
        let _one = self.refreshing.lock().unwrap_or_else(|e| e.into_inner());
        let prefs = std::sync::Arc::clone(&self.prefs.read().unwrap_or_else(|e| e.into_inner()));
        let snap = self.tx.borrow().clone();
        let alerts = crate::alerts::compute(&snap, &prefs, Utc::now());
        drop(snap);
        self.tx.send_if_modified(|s| {
            let changed = alerts != s.alerts;
            s.alerts = alerts;
            changed
        });
    }

    /// The alerts of the modules' blocks, as the window computes them (they say
    /// theirs in TypeScript only): kept for the routines and the Changes block (`Snapshot::all_alerts`).
    pub fn set_external_alerts(&self, alerts: Vec<crate::alerts::Alert>) {
        self.update(|s| {
            if s.external_alerts == alerts {
                return false;
            }
            s.external_alerts = alerts;
            true
        });
    }

    /// The shared store `prefs` as written last (at launch, then by the window or a phone): the
    /// agents and worktrees archived, the overlaps ignored. The alerts follow.
    pub fn set_alert_prefs(&self, value: &serde_json::Value) {
        let prefs = crate::alerts::Prefs::of(value);
        {
            let mut held = self.prefs.write().unwrap_or_else(|e| e.into_inner());
            if **held == prefs {
                return;
            }
            *held = std::sync::Arc::new(prefs);
        }
        self.refresh_alerts();
    }

    /// The outcome of a read of `source`: its data first (`apply` says whether the snapshot
    /// changed), then the source as answered, so that whoever sees it answered also sees what it
    /// brought; or its error, logged. Returns the data's success.
    pub fn publish<T>(&self, source: &'static str, result: anyhow::Result<T>, apply: impl FnOnce(&mut Snapshot, T) -> bool) -> bool {
        match result {
            Ok(data) => {
                self.update(|s| apply(s, data));
                self.set_error(source, None);
                true
            }
            Err(e) => {
                tracing::warn!("{source}: {e:#}");
                self.set_error(source, Some(format!("{e:#}")));
                false
            }
        }
    }

    /// Error of a source, or `None` after a successful read; either way, it has responded.
    pub fn set_error(&self, source: &'static str, err: Option<String>) {
        self.update(|s| {
            let first = s.answered.insert(source);
            let changed = match err {
                Some(e) => s.errors.insert(source, e.clone()).as_ref() != Some(&e),
                None => s.errors.remove(source).is_some(),
            };
            first || changed
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn update_notifies_only_on_change() {
        let hub = Hub::new();
        let mut rx = hub.subscribe();
        rx.mark_unchanged();
        hub.update(|_| false);
        assert!(!rx.has_changed().unwrap());
        assert!(hub.subscribe().borrow().updated_at.is_none());
        hub.update(|s| {
            s.errors.insert("x", "boom".into());
            true
        });
        assert!(rx.has_changed().unwrap());
        assert!(rx.borrow_and_update().updated_at.is_some());
    }

    #[test]
    fn the_alerts_follow_the_snapshot() {
        let hub = Hub::new();
        let mut rx = hub.subscribe();
        hub.set_error("argocd", Some("timeout".into()));
        hub.refresh_alerts();
        assert_eq!(rx.borrow_and_update().alerts.iter().map(|a| a.text.as_str()).collect::<Vec<_>>(), ["argocd unreachable: timeout"]);
        // Nothing they read changed: nothing new.
        hub.refresh_alerts();
        hub.set_alert_prefs(&serde_json::json!({ "notes.tag": "x" }));
        assert!(!rx.has_changed().unwrap());
        hub.set_error("argocd", None);
        hub.refresh_alerts();
        assert!(rx.borrow_and_update().alerts.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn the_alerts_are_computed_once_the_changes_settle() {
        let hub = Hub::new();
        let mut rx = hub.subscribe();
        tokio::spawn(crate::alerts::run(hub.clone()));
        tokio::task::yield_now().await;
        // A burst of changes: not computed under the snapshot's lock, nor once each.
        hub.set_error("argocd", Some("timeout".into()));
        hub.set_error("gitlab", Some("down".into()));
        assert!(rx.borrow_and_update().alerts.is_empty());
        let texts = |rx: &mut watch::Receiver<Snapshot>| rx.borrow_and_update().alerts.iter().map(|a| a.text.clone()).collect::<Vec<_>>();
        let mut seen = Vec::new();
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if rx.has_changed().unwrap() {
                seen.push(texts(&mut rx));
            }
        }
        assert_eq!(seen, [vec!["argocd unreachable: timeout".to_string(), "gitlab unreachable: down".to_string()]]);
        // Their publication wakes nothing: no computation, no change after.
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(!rx.has_changed().unwrap());
    }

    #[test]
    fn the_external_alerts_are_kept_beside() {
        let hub = Hub::new();
        let mut rx = hub.subscribe();
        let alert = crate::alerts::Alert::new("Docker", crate::alerts::Severity::Warn, "low disk");
        hub.set_external_alerts(vec![alert.clone()]);
        assert!(rx.has_changed().unwrap());
        hub.refresh_alerts();
        let snap = rx.borrow_and_update().clone();
        assert!(snap.alerts.is_empty());
        assert_eq!(snap.all_alerts().map(|a| a.text.as_str()).collect::<Vec<_>>(), ["low disk"]);
        // The same again: nothing new; and never sent.
        hub.set_external_alerts(vec![alert]);
        assert!(!rx.has_changed().unwrap());
        assert!(serde_json::to_value(&snap).unwrap().get("external_alerts").is_none());
    }

    #[test]
    fn set_error_marks_the_source_answered() {
        let hub = Hub::new();
        let mut rx = hub.subscribe();
        hub.set_error("gitlab", None);
        assert!(rx.borrow_and_update().answered.contains("gitlab"));
        // Answered and no error: nothing new.
        hub.set_error("gitlab", None);
        assert!(!rx.has_changed().unwrap());
        hub.set_error("gitlab", Some("down".into()));
        assert_eq!(rx.borrow_and_update().errors.get("gitlab").map(String::as_str), Some("down"));
        // The same error again: nothing new.
        hub.set_error("gitlab", Some("down".into()));
        assert!(!rx.has_changed().unwrap());
        hub.set_error("gitlab", None);
        assert!(rx.borrow_and_update().errors.is_empty());
    }

    #[test]
    fn publish_puts_the_data_before_the_answer() {
        let hub = Hub::new();
        let mut rx = hub.subscribe();
        let ok = hub.publish("ci_health", Ok(3), |s, n: u32| {
            // Not answered yet while its data goes in.
            assert!(!s.answered.contains("ci_health"));
            replace(&mut s.worktree_stale_days, n)
        });
        assert!(ok);
        let snap = rx.borrow_and_update().clone();
        assert_eq!(snap.worktree_stale_days, 3);
        assert!(snap.answered.contains("ci_health") && snap.errors.is_empty());

        assert!(!hub.publish("ci_health", Err::<u32, _>(anyhow::anyhow!("down")), |_, _| unreachable!()));
        let snap = rx.borrow_and_update().clone();
        assert_eq!(snap.errors.get("ci_health").map(String::as_str), Some("down"));
        // The last data stays.
        assert_eq!(snap.worktree_stale_days, 3);
    }

    #[tokio::test(start_paused = true)]
    async fn pause_follows_the_window() {
        let hub = Hub::new();
        let start = tokio::time::Instant::now();
        hub.pause(Duration::from_secs(5)).await;
        assert_eq!(start.elapsed(), Duration::from_secs(5));

        // Hidden: four times longer, at least a minute.
        hub.set_visible(false);
        let start = tokio::time::Instant::now();
        hub.pause(Duration::from_secs(5)).await;
        assert_eq!(start.elapsed(), HIDDEN_MIN);
        let start = tokio::time::Instant::now();
        hub.pause(Duration::from_secs(30)).await;
        assert_eq!(start.elapsed(), Duration::from_secs(120));

        // Shown again: the pause ends right away.
        let h = hub.clone();
        let start = tokio::time::Instant::now();
        let waiting = tokio::spawn(async move { h.pause(Duration::from_secs(30)).await });
        tokio::time::sleep(Duration::from_secs(10)).await;
        hub.set_visible(true);
        waiting.await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_secs(10));
    }

    #[tokio::test(start_paused = true)]
    async fn pause_forge_ends_on_a_change() {
        let hub = Hub::new();
        let mut changes = hub.forge_changes();
        // Nothing changed: the whole pause.
        let start = tokio::time::Instant::now();
        assert!(!hub.pause_forge(Duration::from_secs(15), &mut changes).await);
        assert_eq!(start.elapsed(), Duration::from_secs(15));

        // Changed during the pause: it ends there.
        let h = hub.clone();
        let start = tokio::time::Instant::now();
        let mut c = changes.clone();
        let waiting = tokio::spawn(async move { h.pause_forge(Duration::from_secs(15), &mut c).await });
        tokio::time::sleep(Duration::from_secs(3)).await;
        hub.forge_changed();
        assert!(waiting.await.unwrap());
        assert_eq!(start.elapsed(), Duration::from_secs(3));

        // Changed during the read before: no pause at all, and once only.
        hub.forge_changed();
        let start = tokio::time::Instant::now();
        assert!(hub.pause_forge(Duration::from_secs(15), &mut changes).await);
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert!(!hub.pause_forge(Duration::from_secs(15), &mut changes).await);
    }
}
