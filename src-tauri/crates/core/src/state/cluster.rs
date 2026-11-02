//! Clusters: pods, environments, Argo CD.

use chrono::{DateTime, Utc};
use serde::Serialize;


/// A pod of a `[[pods]]` target. Only the fields useful for display are kept: no IP, no spec, no logs.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PodView {
    /// Its target's name (`[[pods]] name`, or its context).
    pub target: String,
    /// Its target's kube context: to read its logs.
    pub context: String,
    pub namespace: String,
    pub name: String,
    /// What runs it: its Deployment (its ReplicaSet's name without the hash), StatefulSet,
    /// DaemonSet or Job. None for a bare pod.
    pub workload: Option<String>,
    /// `runner`: a GitLab runner manager; `job`: a CI job pod its Kubernetes executor created;
    /// `pod`: any other.
    pub role: PodRole,
    /// CI job pod (created by the Kubernetes executor) rather than the runner manager.
    pub is_job: bool,
    /// CI job name, ref and link, from the annotations set by the runner.
    pub job_name: Option<String>,
    pub job_ref: Option<String>,
    pub job_url: Option<String>,
    /// Sum of the containers' limits (failing that, of their requests).
    pub cpu_limit_millis: Option<u64>,
    pub mem_limit_bytes: Option<u64>,
    /// k8s phase, or the container's waiting reason when it says more (CrashLoopBackOff…).
    pub status: String,
    pub healthy: bool,
    pub restarts: i32,
    pub started_at: Option<DateTime<Utc>>,
    /// Node the pod runs on (absent until it is scheduled: `Pending`).
    pub node: Option<String>,
    /// A container's last termination (`OOMKilled`, `exit 1`) and when: what the restarts were.
    pub last_exit: Option<String>,
    pub last_exit_at: Option<DateTime<Utc>>,
    /// Not scheduled: why (the scheduler's message, `0/3 nodes are available: 3 Insufficient cpu.`).
    pub unscheduled: Option<String>,
}

/// What a pod is to Prism: a GitLab runner, one of its CI jobs, or any pod.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum PodRole {
    Runner,
    Pod,
    Job,
}

/// Load of a node hosting pods of a `[[pods]]` target. `used`: actual consumption
/// (metrics-server); `requested`: reserved by the node's pods, across all namespaces,
/// which decides whether a new job fits there; `allocatable`: what the node offers.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NodeLoad {
    /// The target whose pods it hosts.
    pub target: String,
    pub name: String,
    pub cpu_used_millis: Option<u64>,
    pub mem_used_bytes: Option<u64>,
    pub cpu_requested_millis: u64,
    pub mem_requested_bytes: u64,
    pub cpu_allocatable_millis: Option<u64>,
    pub mem_allocatable_bytes: Option<u64>,
    pub pods: u32,
    pub max_pods: Option<u32>,
    /// Read denied or impossible (RBAC rights on nodes, metrics-server…).
    pub error: Option<String>,
}

/// Instantaneous consumption of a pod, from metrics-server.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Usage {
    pub cpu_millis: u64,
    pub mem_bytes: u64,
}

/// A pod's consumption over a period (`pod_usage_history`): from Google Managed Prometheus for a
/// GKE cluster (`[clusters] gcp_metrics`), otherwise what Prism kept of metrics-server's readings.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UsageHistory {
    pub source: HistorySource,
    /// Oldest first.
    pub points: Vec<UsagePoint>,
    /// Sum of the containers' limits (failing that, of their requests).
    pub cpu_limit_millis: Option<u64>,
    pub mem_limit_bytes: Option<u64>,
    /// Seconds between two points.
    pub step: u32,
    /// Google Managed Prometheus refused or unreachable: why, Prism's history shown instead.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum HistorySource {
    /// Google Managed Prometheus (Cloud Monitoring), read on demand.
    Gcp,
    /// Kept by Prism since it started (`[[pods]]` targets only).
    Prism,
}

/// A pod's usage at a moment (seconds since the epoch); none where nothing was measured.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UsagePoint {
    pub at: i64,
    pub cpu_millis: Option<u64>,
    pub mem_bytes: Option<u64>,
}

/// State of the application environments: one item per configured (cluster, namespace).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Environments {
    pub envs: Vec<EnvStatus>,
    /// Load of each configured cluster (one per kube context).
    #[serde(default)]
    pub clusters: Vec<ClusterLoad>,
}

/// The ArgoCD instances read (one namespace each, on the ops cluster).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ArgoCd {
    pub instances: Vec<ArgoInstance>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ArgoInstance {
    /// Cluster hosting it (end of the kube context: `shop-ops`, `tool`).
    pub cluster: String,
    /// Namespace of the instance (`argocd-prod`).
    pub namespace: String,
    /// Web interface, for links (`[[argocd]] url`).
    pub url: Option<String>,
    /// Environments the instance is authoritative for (`[[argocd]] envs`); absent: all.
    pub envs: Option<Vec<String>>,
    pub error: Option<String>,
    pub apps: Vec<ArgoApp>,
}

/// An ArgoCD Application. No cluster address nor repository: only what Prism displays.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ArgoApp {
    pub name: String,
    /// `prod`, `demo`, `staging`, from the Helm values path (`argocd/<group>/<env>/…`).
    pub env: Option<String>,
    /// The instance group (`main`, `data`…), or `customers` (instance dedicated to a customer, outside the Environments block).
    pub group: Option<String>,
    /// Repository application (`intake-api`), from that same path.
    pub app: Option<String>,
    /// An instance dedicated to a customer (`[argocd_layout] customer_groups`): outside Environments.
    pub customer: bool,
    /// Destination namespace.
    pub namespace: Option<String>,
    /// `Synced`, `OutOfSync`, `Unknown`.
    pub sync: String,
    /// `Healthy`, `Progressing`, `Degraded`, `Suspended`, `Missing`, `Unknown`.
    pub health: String,
    /// Synced Git revision (short).
    pub revision: Option<String>,
    /// Last sync operation: `Succeeded`, `Running`, `Failed`, `Error`…
    pub operation: Option<String>,
    /// Message of the operation or of the first error condition.
    pub message: Option<String>,
    pub synced_at: Option<DateTime<Utc>>,
    /// Tags of the deployed images.
    pub images: Vec<String>,
    /// Managed Deployments, by name (in `namespace`).
    pub deployments: Vec<String>,
    /// When the last operation started.
    pub started_at: Option<DateTime<Utc>>,
    /// The last operation was started by ArgoCD itself (auto-sync), not by someone.
    pub automated: bool,
    /// The last sync's resources, those that did not sync first (at most 25).
    pub sync_resources: Vec<ArgoResource>,
    /// Resources out of sync or not healthy now (at most 25).
    pub drifting: Vec<ArgoResource>,
    /// The last deployments, most recent first (at most 5).
    pub history: Vec<ArgoDeploy>,
}

/// A resource of an Application: its last sync's result, or its current state.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ArgoResource {
    pub kind: String,
    pub name: String,
    /// `Synced`, `SyncFailed`, `Pruned`, `PruneSkipped`, `OutOfSync`…
    pub status: Option<String>,
    /// `Healthy`, `Progressing`, `Degraded`, `Missing`…
    pub health: Option<String>,
    /// A sync hook's phase (`Running`, `Succeeded`, `Failed`).
    pub hook: Option<String>,
    pub message: Option<String>,
}

/// A deployment in an Application's history.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ArgoDeploy {
    /// Git revision (short).
    pub revision: Option<String>,
    pub deployed_at: Option<DateTime<Utc>>,
}

/// Consumption of a cluster: the sum of its nodes, from metrics-server, relative to what
/// is allocatable to pods.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ClusterLoad {
    pub context: String,
    /// End of the kube context name (`shop-prod`).
    pub cluster: String,
    pub nodes: u32,
    pub cpu_used_millis: Option<u64>,
    pub mem_used_bytes: Option<u64>,
    pub cpu_allocatable_millis: u64,
    pub mem_allocatable_bytes: u64,
    /// Read denied or impossible (rights on nodes, metrics-server…).
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EnvStatus {
    /// `prod`, `demo` or `staging`.
    pub env: String,
    /// Full kube context, to diagnose with kubectl.
    pub context: String,
    /// End of the kube context name (`shop-prod`).
    pub cluster: String,
    pub namespace: String,
    /// The namespace could not be read.
    pub error: Option<String>,
    pub workloads: Vec<Workload>,
}

/// A Deployment and its pods.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Workload {
    pub name: String,
    /// Repository application (`billing-api`), when the name carries it.
    pub app: Option<String>,
    /// Role in the application: `api`, `internal`, `worker-default`…
    pub role: String,
    pub desired: u32,
    pub ready: u32,
    pub pods: u32,
    /// Since the current pods started.
    pub restarts: u32,
    /// Abnormal waiting reason of a container (CrashLoopBackOff, ImagePullBackOff…).
    pub waiting: Option<String>,
    /// Reason of a container's last termination (OOMKilled, Error…), and its date.
    pub last_exit: Option<String>,
    pub last_exit_at: Option<DateTime<Utc>>,
    pub image_tag: Option<String>,
    /// Latest git tag of the application for this environment (prod, demo).
    pub expected: Option<String>,
    /// The running image is not the one of the latest tag.
    pub drift: bool,
    /// Consumption, requests and limits of its active pods.
    pub load: WorkloadLoad,
    /// End of the last rollout (`Progressing` condition), failing that the Deployment's creation.
    pub updated_at: Option<DateTime<Utc>>,
    /// Deployment revision (`deployment.kubernetes.io/revision`): one per rollout.
    pub revision: Option<u32>,
}

/// Load of a Deployment: the sum of its active pods.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WorkloadLoad {
    /// Absent if metrics-server did not respond.
    pub cpu_used_millis: Option<u64>,
    pub mem_used_bytes: Option<u64>,
    pub cpu_requested_millis: u64,
    pub mem_requested_bytes: u64,
    /// Absent as soon as a container has no limit (the pod can then take more).
    pub cpu_limit_millis: Option<u64>,
    pub mem_limit_bytes: Option<u64>,
}
