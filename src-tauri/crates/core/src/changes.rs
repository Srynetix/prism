//! What touched an environment, on one timeline (`[changes]`, the Changes block of `prism-blocks`):
//! its events, and how a block of a pack adds its own (`Block::changes`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One change on an environment: an Argo CD sync, an MR merged, a pod restarting, an alert, a
/// vulnerability, or a block's own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields))]
pub struct ChangeEvent {
    /// Unique: what it is and when (`sync:ops/argocd/api:2026-11-05T09:42:00Z`), so that two reads
    /// of the same fact make one.
    pub id: String,
    pub at: DateTime<Utc>,
    /// `at` is when Prism saw it, the source giving no date of its own (alerts, vulnerabilities,
    /// a restart counter going up).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub seen: bool,
    /// The environment (`[[environments]] env`, an Argo CD application's); absent: every one
    /// (an MR without `[changes] branches`, a lockfile's vulnerability).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    /// `argocd`, `mrs`, `pods`, `alerts`, `vulnerabilities`, or the id of the block that gave it.
    pub source: String,
    pub kind: ChangeKind,
    /// The application (`api`, an Argo CD application's repository app, a Deployment's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// The Deployments it is about: an Argo CD application's, a workload's own name.
    #[serde(default)]
    pub workloads: Vec<String>,
    /// The Git revision deployed (short).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// The images deployed with it (`api:2.15.0`): to tell two environments apart by what runs.
    #[serde(default)]
    pub images: Vec<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub severity: ChangeSeverity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ChangeLink>,
    /// The merge request that brought its revision, when Prism found it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mr: Option<ChangeMr>,
    /// A crash: the change just before it on the same application (`[changes] correlate_minutes`),
    /// by its id. A guess on time and name, never a certain cause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// An Argo CD sync, its revision deployed.
    Sync,
    SyncFailed,
    /// An application's health changed (Degraded, Healthy…).
    Health,
    /// An application went out of sync.
    Drift,
    /// A merge request merged into the branch that feeds the environment.
    Merged,
    /// A new ReplicaSet: a Deployment rolled out.
    Rollout,
    /// Containers restarted.
    Restart,
    CrashLoop,
    OomKilled,
    /// A container waiting for another reason (ImagePullBackOff, Pending…).
    Waiting,
    /// A Kubernetes event (`[changes] kube_events`).
    KubeEvent,
    AlertOn,
    AlertOff,
    VulnNew,
    VulnFixed,
    /// A block's own (a deployment a pack follows).
    Other,
}

impl ChangeKind {
    /// A crash: what the timeline looks for a change just before.
    pub fn is_crash(self) -> bool {
        matches!(self, Self::CrashLoop | Self::OomKilled | Self::Restart)
    }

    /// Something deployed: what a crash may come from.
    pub fn deploys(self) -> bool {
        matches!(self, Self::Sync | Self::Rollout)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum ChangeSeverity {
    Ok,
    Info,
    Warn,
    Ko,
}

/// What a line opens in Prism; `Url` elsewhere.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ChangeLink {
    /// An Argo CD application's panel: its instance's cluster, `<cluster>/<namespace>/<name>`.
    Argo { cluster: String, app: String },
    /// A Deployment's pods and logs.
    Pod { context: String, namespace: String, deployment: String },
    Mr { repo: Option<String>, iid: u64 },
    Url { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ChangeMr {
    /// `[[repos]] name`; absent: the primary repository.
    pub repo: Option<String>,
    pub iid: u64,
    pub title: String,
    pub url: Option<String>,
}

impl ChangeEvent {
    /// An event with what every one has; the rest left empty.
    pub fn new(id: impl Into<String>, at: DateTime<Utc>, source: impl Into<String>, kind: ChangeKind, severity: ChangeSeverity, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            at,
            seen: false,
            env: None,
            source: source.into(),
            kind,
            app: None,
            namespace: None,
            workloads: Vec::new(),
            revision: None,
            images: Vec::new(),
            title: title.into(),
            detail: None,
            severity,
            link: None,
            mr: None,
            related: None,
        }
    }

    /// The same application: the same environment, and the same app or a Deployment in common.
    pub fn same_app(&self, other: &ChangeEvent) -> bool {
        if self.env.is_some() && other.env.is_some() && self.env != other.env {
            return false;
        }
        let app = |e: &ChangeEvent| e.app.as_deref().map(str::to_ascii_lowercase);
        (app(self).is_some() && app(self) == app(other)) || self.workloads.iter().any(|w| other.workloads.contains(w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_app_by_name_or_deployment() {
        let now = Utc::now();
        let mut sync = ChangeEvent::new("s", now, "argocd", ChangeKind::Sync, ChangeSeverity::Info, "sync");
        sync.env = Some("staging".into());
        sync.app = Some("shop".into());
        sync.workloads = vec!["shop-api".into(), "shop-worker".into()];
        let mut crash = ChangeEvent::new("c", now, "pods", ChangeKind::CrashLoop, ChangeSeverity::Ko, "crash");
        crash.env = Some("staging".into());
        crash.app = Some("api".into());
        crash.workloads = vec!["shop-api".into()];
        assert!(crash.same_app(&sync));
        crash.env = Some("prod".into());
        assert!(!crash.same_app(&sync));
        crash.env = Some("staging".into());
        crash.workloads = vec!["other".into()];
        assert!(!crash.same_app(&sync));
        crash.app = Some("Shop".into());
        assert!(crash.same_app(&sync));
    }

    #[test]
    fn serializes_compact() {
        let e = ChangeEvent::new("x", DateTime::parse_from_rfc3339("2026-11-05T09:42:00Z").unwrap().with_timezone(&Utc), "argocd", ChangeKind::Sync, ChangeSeverity::Info, "Synced");
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json, serde_json::json!({"id": "x", "at": "2026-11-05T09:42:00Z", "source": "argocd", "kind": "sync", "workloads": [], "images": [], "title": "Synced", "severity": "info"}));
        assert_eq!(serde_json::from_value::<ChangeEvent>(json).unwrap(), e);
    }
}
