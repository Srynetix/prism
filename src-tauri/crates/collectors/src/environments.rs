//! State of the application environments: the Deployments and their pods, in the configured
//! namespaces (`[[environments]]`, `*` patterns matched against the cluster's), re-read read-only every 30 s.
//!
//! Per workload: ready replicas, restarts, waiting reason (CrashLoopBackOff,
//! ImagePullBackOff…) or last termination reason (OOMKilled, Error), and the tag of the running image.
//! Where the stages' provider (`[clusters] stages`) expects a version, this tag is compared to it
//! (the stage's latest tag): a mismatch flags a lagging deployment or an unpublished image.
//! As for the pods, no IP, spec or logs are kept.
//!
//! A workload's logs are read on demand (`PodLogs`, the block's log panel): the Deployment's
//! pods, then a container's log, the current one or the one from before its last restart.
//! Only in the configured contexts and namespaces, and nothing is kept.
//!
//! Load is read at the same pace, from metrics-server: per cluster, the usage of its
//! nodes relative to their allocatable capacity; per Deployment, that of its pods, their requests
//! and their limits.

use prism_base::i18n::tr;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Namespace, Node, Pod},
};
use kube::{
    api::{ApiResource, DynamicObject, GroupVersionKind, ListParams, LogParams},
    config::{KubeConfigOptions, Kubeconfig},
    Api, Client,
};

use super::{
    pods::{bytes, cpu_millis, pod_usage, requests},
};
use crate::{
    config::EnvironmentConfig,
    state::{ClusterLoad, EnvStatus, Environments, Hub, Usage, Workload, WorkloadLoad},
};

const SOURCE: &str = "environments";
const POLL: Duration = Duration::from_secs(30);
/// Namespaces of one environment read at once.
const READ_AT_ONCE: usize = 8;
/// Normal pod startup stages: to display, not to flag.
const STARTING: [&str; 2] = ["ContainerCreating", "PodInitializing"];

/// `repo`: the primary repository's clone, whose applications group the workloads; without it, no grouping.
pub async fn run(targets: Vec<EnvironmentConfig>, repo: Option<PathBuf>, hub: Hub) {
    if targets.is_empty() {
        return;
    }
    let mut clients: HashMap<String, Client> = HashMap::new();
    let mut first = true;
    loop {
        if !std::mem::take(&mut first) {
            hub.pause(POLL).await;
        }
        // The stages' provider (`[clusters] stages`) groups the workloads and knows their expected versions.
        let hints = match (&repo, crate::stages::current_provider()) {
            (Some(repo), Some(p)) => p.hints(repo).await.unwrap_or_default(),
            _ => Default::default(),
        };
        let mut envs = Vec::new();
        for t in &targets {
            let read = match namespaces(&mut clients, t).await {
                Ok(namespaces) => client(&mut clients, &t.context).await.map(|c| (namespaces, c)),
                Err(e) => Err(e),
            };
            match read {
                // Its namespaces side by side (a `*` may match dozens), a few at a time.
                Ok((namespaces, c)) => {
                    for chunk in namespaces.chunks(READ_AT_ONCE) {
                        let reads = futures::future::join_all(chunk.iter().map(|ns| read_namespace(&c, t, ns, &hints))).await;
                        for (ns, result) in chunk.iter().zip(reads) {
                            envs.push(result.unwrap_or_else(|e| unreadable(&mut clients, t, ns, &e)));
                        }
                    }
                }
                Err(e) => envs.push(unreadable(&mut clients, t, &t.namespaces.join(", "), &e)),
            }
        }
        let failed = envs.iter().filter(|e| e.error.is_some()).count();
        hub.set_error(SOURCE, (failed == envs.len()).then(|| tr("no environment could be read").into()));
        // One cluster per context, in config order.
        let mut contexts: Vec<&str> = Vec::new();
        for t in &targets {
            if !contexts.contains(&t.context.as_str()) {
                contexts.push(&t.context);
            }
        }
        let mut clusters = Vec::new();
        for context in contexts {
            clusters.push(cluster_load(&mut clients, context).await);
        }
        let state = Environments { envs, clusters };
        hub.update(|s| {
            let changed = s.environments.as_ref() != Some(&state);
            s.environments = Some(state);
            changed
        });
    }
}

async fn client(clients: &mut HashMap<String, Client>, context: &str) -> Result<Client> {
    if let Some(c) = clients.get(context) {
        return Ok(c.clone());
    }
    let options = KubeConfigOptions { context: Some(context.to_string()), ..Default::default() };
    let config = kube::Config::from_custom_kubeconfig(Kubeconfig::read()?, &options).await?;
    let client = Client::try_from(config)?;
    clients.insert(context.to_string(), client.clone());
    Ok(client)
}

async fn read_namespace(client: &Client, t: &EnvironmentConfig, ns: &str, hints: &crate::stages::Hints) -> Result<EnvStatus> {
    let mut status = section(t, ns);
    let deployments = Api::<Deployment>::namespaced(client.clone(), ns).list(&ListParams::default()).await?;
    let pods = Api::<Pod>::namespaced(client.clone(), ns).list(&ListParams::default()).await?;
    let usage = pod_metrics(client, ns).await;
    status.workloads = deployments.items.iter().map(|d| workload(d, &pods.items, usage.as_ref(), &t.env, hints)).collect();
    status.workloads.sort_by(|a, b| a.app.cmp(&b.app).then(a.name.cmp(&b.name)));
    Ok(status)
}

/// Its namespaces: those named, and the cluster's that its patterns (`*`, `app-*`) match.
async fn namespaces(clients: &mut HashMap<String, Client>, t: &EnvironmentConfig) -> Result<Vec<String>> {
    if !t.has_patterns() {
        return Ok(t.namespaces_in(&[]));
    }
    let client = client(clients, &t.context).await?;
    let all = Api::<Namespace>::all(client).list(&ListParams::default()).await?;
    let mut names: Vec<String> = all.items.into_iter().filter_map(|n| n.metadata.name).collect();
    names.sort();
    Ok(t.namespaces_in(&names))
}

fn section(t: &EnvironmentConfig, ns: &str) -> EnvStatus {
    let cluster = t.context.rsplit('_').next().unwrap_or(&t.context).to_string();
    EnvStatus { env: t.env.clone(), context: t.context.clone(), cluster, namespace: ns.to_string(), error: None, workloads: Vec::new() }
}

fn unreadable(clients: &mut HashMap<String, Client>, t: &EnvironmentConfig, ns: &str, e: &anyhow::Error) -> EnvStatus {
    // The client may hold a stale connection: it will be recreated on the next round.
    clients.remove(&t.context);
    tracing::warn!("environments: {} {ns}: {e:#}", t.context);
    EnvStatus { error: Some(format!("{e:#}").lines().next().unwrap_or_default().chars().take(200).collect()), ..section(t, ns) }
}

fn workload(d: &Deployment, pods: &[Pod], usage: Option<&HashMap<String, Usage>>, env: &str, hints: &crate::stages::Hints) -> Workload {
    let apps = &hints.apps;
    let name = d.metadata.name.clone().unwrap_or_default();
    let desired = d.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1).max(0) as u32;
    let ready = d.status.as_ref().and_then(|s| s.ready_replicas).unwrap_or(0).max(0) as u32;
    let image = d.spec.as_ref().and_then(|s| s.template.spec.as_ref()?.containers.first()?.image.clone());
    let image_tag = image.as_deref().map(tag);
    // The image tells the application (`media-prod` runs `media-api`); otherwise, the name.
    let app = image
        .as_deref()
        .map(repository)
        .filter(|r| apps.iter().any(|a| a == r))
        .or_else(|| apps.iter().find(|a| name == **a || name.starts_with(&format!("{a}-"))).cloned());
    // The version expected there, from the stages' provider (the latest tag of the stage).
    let expected = app.as_ref().and_then(|a| hints.expected.get(&(a.clone(), env.to_string())).cloned());
    // The Deployment's pods: those of its ReplicaSets (`<deployment>-<hash>`).
    let mine: Vec<&Pod> = pods.iter().filter(|p| owned_by(p, &name)).collect();
    let containers: Vec<_> = mine.iter().flat_map(|p| p.status.as_ref().and_then(|s| s.container_statuses.clone()).unwrap_or_default()).collect();
    let waiting = containers
        .iter()
        .filter_map(|c| c.state.as_ref()?.waiting.as_ref()?.reason.clone())
        .find(|r| !STARTING.contains(&r.as_str()));
    let last_exit = containers
        .iter()
        .filter_map(|c| {
            let t = c.last_state.as_ref()?.terminated.as_ref()?;
            let at = t.finished_at.as_ref().and_then(|f| DateTime::<Utc>::from_timestamp(f.0.as_second(), f.0.subsec_nanosecond() as u32));
            Some((at, t.reason.clone().unwrap_or_else(|| format!("exit {}", t.exit_code))))
        })
        .max_by_key(|(at, _)| *at);
    Workload {
        role: role(&name, app.as_deref(), env),
        drift: expected.as_ref().is_some_and(|e| image_tag.as_ref() != Some(e)),
        name,
        app,
        desired,
        ready,
        pods: mine.len() as u32,
        restarts: containers.iter().map(|c| c.restart_count.max(0) as u32).sum(),
        waiting,
        last_exit_at: last_exit.as_ref().and_then(|(at, _)| *at),
        last_exit: last_exit.map(|(_, r)| r),
        image_tag,
        expected,
        load: workload_load(mine.iter().copied().filter(|p| is_active(p)), usage),
        updated_at: updated_at(d),
        revision: d.metadata.annotations.as_ref().and_then(|a| a.get("deployment.kubernetes.io/revision")?.parse().ok()),
    }
}

/// Last update: the `Progressing` condition is rewritten on every rollout (creation of the
/// new ReplicaSet, then its availability); without it, the Deployment's creation.
fn updated_at(d: &Deployment) -> Option<DateTime<Utc>> {
    let ts = |t: &k8s_openapi::apimachinery::pkg::apis::meta::v1::Time| DateTime::<Utc>::from_timestamp(t.0.as_second(), t.0.subsec_nanosecond() as u32);
    let progressing = d.status.as_ref().and_then(|s| s.conditions.as_ref()?.iter().find(|c| c.type_ == "Progressing"));
    progressing.and_then(|c| c.last_update_time.as_ref().or(c.last_transition_time.as_ref())).and_then(ts).or_else(|| d.metadata.creation_timestamp.as_ref().and_then(ts))
}

fn metrics_api(client: &Client, kind: &str, plural: &str, namespace: Option<&str>) -> Api<DynamicObject> {
    let resource = ApiResource::from_gvk_with_plural(&GroupVersionKind::gvk("metrics.k8s.io", "v1beta1", kind), plural);
    match namespace {
        Some(ns) => Api::namespaced_with(client.clone(), ns, &resource),
        None => Api::all_with(client.clone(), &resource),
    }
}

/// Usage of each pod in the namespace; none if metrics-server does not respond.
async fn pod_metrics(client: &Client, ns: &str) -> Option<HashMap<String, Usage>> {
    let list = metrics_api(client, "PodMetrics", "pods", Some(ns)).list(&ListParams::default()).await.ok()?;
    Some(list.items.iter().filter_map(|m| Some((m.metadata.name.clone()?, pod_usage(&m.data)))).collect())
}

fn is_active(p: &Pod) -> bool {
    !matches!(p.status.as_ref().and_then(|s| s.phase.as_deref()), Some("Succeeded" | "Failed"))
}

/// Sum over a Deployment's pods: usage, requests and limits.
fn workload_load<'a>(pods: impl Iterator<Item = &'a Pod>, usage: Option<&HashMap<String, Usage>>) -> WorkloadLoad {
    let mut load = WorkloadLoad { cpu_used_millis: usage.map(|_| 0), mem_used_bytes: usage.map(|_| 0), cpu_limit_millis: Some(0), mem_limit_bytes: Some(0), ..Default::default() };
    for p in pods {
        let (cpu, mem) = requests(p);
        load.cpu_requested_millis += cpu;
        load.mem_requested_bytes += mem;
        let (cpu, mem) = limits(p);
        load.cpu_limit_millis = load.cpu_limit_millis.zip(cpu).map(|(a, b)| a + b);
        load.mem_limit_bytes = load.mem_limit_bytes.zip(mem).map(|(a, b)| a + b);
        if let Some(u) = usage.and_then(|u| u.get(p.metadata.name.as_deref()?)) {
            load.cpu_used_millis = load.cpu_used_millis.map(|c| c + u.cpu_millis);
            load.mem_used_bytes = load.mem_used_bytes.map(|m| m + u.mem_bytes);
        }
    }
    load
}

/// A pod's limits: the sum of its containers, or nothing as soon as one of them has none.
fn limits(pod: &Pod) -> (Option<u64>, Option<u64>) {
    let Some(spec) = pod.spec.as_ref() else { return (None, None) };
    let of = |key: &str, parse: fn(&str) -> Option<u64>| {
        spec.containers.iter().try_fold(0, |acc, c| Some(acc + c.resources.as_ref()?.limits.as_ref()?.get(key).and_then(|q| parse(&q.0))?))
    };
    (of("cpu", cpu_millis), of("memory", bytes))
}

/// All the cluster's nodes: allocatable capacity, and usage according to metrics-server.
async fn cluster_load(clients: &mut HashMap<String, Client>, context: &str) -> ClusterLoad {
    let cluster = context.rsplit('_').next().unwrap_or(context).to_string();
    let mut load = ClusterLoad { context: context.to_string(), cluster, ..Default::default() };
    let client = match client(clients, context).await {
        Ok(c) => c,
        Err(e) => {
            load.error = Some(short(&e));
            return load;
        }
    };
    let mut errors = Vec::new();
    match Api::<Node>::all(client.clone()).list(&ListParams::default()).await {
        Ok(nodes) => {
            load.nodes = nodes.items.len() as u32;
            for n in &nodes.items {
                let alloc = n.status.as_ref().and_then(|s| s.allocatable.as_ref());
                load.cpu_allocatable_millis += alloc.and_then(|a| a.get("cpu")).and_then(|q| cpu_millis(&q.0)).unwrap_or(0);
                load.mem_allocatable_bytes += alloc.and_then(|a| a.get("memory")).and_then(|q| bytes(&q.0)).unwrap_or(0);
            }
        }
        Err(e) => errors.push(format!("nodes: {}", short(&e.into()))),
    }
    match metrics_api(&client, "NodeMetrics", "nodes", None).list(&ListParams::default()).await {
        Ok(list) => {
            load.cpu_used_millis = Some(list.items.iter().filter_map(|m| cpu_millis(m.data["usage"]["cpu"].as_str()?)).sum());
            load.mem_used_bytes = Some(list.items.iter().filter_map(|m| bytes(m.data["usage"]["memory"].as_str()?)).sum());
        }
        Err(e) => errors.push(format!("metrics: {}", short(&e.into()))),
    }
    if !errors.is_empty() {
        load.error = Some(errors.join(" · "));
    }
    load
}

/// First line of an error, bounded: enough to fit in a tooltip.
fn short(e: &anyhow::Error) -> String {
    format!("{e:#}").lines().next().unwrap_or_default().chars().take(160).collect()
}

/// A Deployment's pod: created by one of its ReplicaSets (`<deployment>-<hash>`).
fn owned_by(p: &Pod, deployment: &str) -> bool {
    p.metadata.owner_references.iter().flatten().any(|o| o.kind == "ReplicaSet" && o.name.rsplit_once('-').is_some_and(|(d, _)| d == deployment))
}

/// Lines read at the end of a log, and maximum size (a verbose log can weigh MBs).
const LOG_TAIL: i64 = 2000;
const LOG_BYTES: i64 = 2 * 1024 * 1024;
/// Further back on demand (scrolled to the top): up to the container's start, within these bounds.
const LOG_DEEP_LINES: i64 = 200_000;
const LOG_DEEP_BYTES: usize = 48 * 1024 * 1024;

/// A workload's pod, to choose which log to read.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PodInfo {
    pub name: String,
    /// Phase, or a container's waiting reason when it is more telling (CrashLoopBackOff…).
    pub status: String,
    pub ready: bool,
    pub restarts: u32,
    pub started_at: Option<DateTime<Utc>>,
    /// The application's containers, in spec order: the first is the main one.
    pub containers: Vec<String>,
    /// A container's last termination reason: its earlier log is then readable ("previous").
    pub last_exit: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PodLog {
    pub log: String,
    /// Only the end of the log was read (the lines asked for, or the bytes bound): older lines remain.
    pub truncated: bool,
}

/// Logs on demand, only in the `[[environments]]` namespaces.
#[derive(Clone, Default)]
pub struct PodLogs {
    targets: Arc<Vec<EnvironmentConfig>>,
    clients: Arc<tokio::sync::Mutex<HashMap<String, Client>>>,
}

impl PodLogs {
    pub fn new(targets: Vec<EnvironmentConfig>) -> Self {
        Self { targets: Arc::new(targets), clients: Default::default() }
    }

    /// A namespace of a configured environment.
    pub fn covers(&self, context: &str, namespace: &str) -> bool {
        self.targets.iter().any(|t| t.context == context && t.reads(namespace))
    }

    async fn pods_api(&self, context: &str, namespace: &str) -> Result<Api<Pod>> {
        if !self.covers(context, namespace) {
            bail!("{context} / {namespace} is not a configured environment");
        }
        let client = client(&mut *self.clients.lock().await, context).await?;
        Ok(Api::namespaced(client, namespace))
    }

    /// A failure may come from a stale connection: the client will be recreated on the next call.
    async fn forget(&self, context: &str) {
        self.clients.lock().await.remove(context);
    }

    /// The Deployment's pods, newest first.
    pub async fn pods(&self, context: &str, namespace: &str, deployment: &str) -> Result<Vec<PodInfo>> {
        let api = self.pods_api(context, namespace).await?;
        let list = match api.list(&ListParams::default()).await {
            Ok(l) => l,
            Err(e) => {
                self.forget(context).await;
                return Err(e.into());
            }
        };
        let mut pods: Vec<PodInfo> = list.items.iter().filter(|p| owned_by(p, deployment)).map(pod_info).collect();
        pods.sort_by_key(|a| std::cmp::Reverse(a.started_at));
        Ok(pods)
    }

    /// Its last `lines` lines (`LOG_TAIL` when none; 0: up to the container's start, `LOG_DEEP_LINES`
    /// at most). Further back than `LOG_TAIL`, the API's `limitBytes` would keep the oldest bytes of
    /// the tail: the log is streamed instead, and only its newest `LOG_DEEP_BYTES` kept.
    pub async fn log(&self, context: &str, namespace: &str, pod: &str, container: Option<String>, previous: bool, lines: Option<u32>) -> Result<PodLog> {
        let api = self.pods_api(context, namespace).await?;
        let lines = match lines.map(i64::from) {
            None => LOG_TAIL,
            Some(0) => LOG_DEEP_LINES,
            Some(n) => n.clamp(1, LOG_DEEP_LINES),
        };
        let shallow = lines <= LOG_TAIL;
        let params = LogParams { container, previous, tail_lines: Some(lines), limit_bytes: shallow.then_some(LOG_BYTES), ..Default::default() };
        let read = if shallow { api.logs(pod, &params).await.map(|l| (l, false)) } else { deep_log(&api, pod, &params).await };
        let (log, cut) = match read {
            Ok(l) => l,
            Err(e) => {
                if !matches!(&e, kube::Error::Api(_)) {
                    self.forget(context).await;
                }
                return Err(e.into());
            }
        };
        let truncated = cut || log.lines().count() as i64 >= lines || (shallow && log.len() as i64 >= LOG_BYTES);
        Ok(PodLog { log, truncated })
    }
}

/// A long tail, streamed: its newest `LOG_DEEP_BYTES` kept (whole lines), and whether older ones were dropped.
/// One buffer, cut back now and then: a line each was 128k allocations and the log three times over.
async fn deep_log(api: &Api<Pod>, pod: &str, params: &LogParams) -> kube::Result<(String, bool)> {
    use futures::AsyncReadExt;
    let mut stream = std::pin::pin!(api.log_stream(pod, params).await?);
    let mut kept = Vec::with_capacity(1 << 20);
    let mut chunk = vec![0; 64 << 10];
    let mut cut = false;
    loop {
        let n = stream.read(&mut chunk).await.map_err(kube::Error::ReadEvents)?;
        if n == 0 {
            break;
        }
        kept.extend_from_slice(&chunk[..n]);
        if kept.len() > 2 * LOG_DEEP_BYTES {
            cut |= newest_lines(&mut kept, LOG_DEEP_BYTES);
        }
    }
    cut |= newest_lines(&mut kept, LOG_DEEP_BYTES);
    let log = String::from_utf8(kept).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    Ok((log, cut))
}

/// `log` cut to its newest whole lines within `max` bytes (its last line alone, if longer);
/// whether any went.
fn newest_lines(log: &mut Vec<u8>, max: usize) -> bool {
    if log.len() <= max {
        return false;
    }
    // The first line starting `max` bytes from the end or after, else the last line.
    let from = log.len() - max;
    let start = match log[from - 1..].iter().position(|&b| b == b'\n').map(|i| from + i) {
        Some(s) if s < log.len() => s,
        _ => log[..log.len() - 1].iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1),
    };
    log.drain(..start);
    start > 0
}

fn pod_info(p: &Pod) -> PodInfo {
    let status = p.status.as_ref();
    let containers = status.and_then(|s| s.container_statuses.clone()).unwrap_or_default();
    let waiting = containers.iter().filter_map(|c| c.state.as_ref()?.waiting.as_ref()?.reason.clone()).next();
    let phase = status.and_then(|s| s.phase.clone()).unwrap_or_else(|| "Unknown".into());
    PodInfo {
        name: p.metadata.name.clone().unwrap_or_default(),
        status: waiting.unwrap_or(phase),
        ready: !containers.is_empty() && containers.iter().all(|c| c.ready),
        restarts: containers.iter().map(|c| c.restart_count.max(0) as u32).sum(),
        started_at: status.and_then(|s| s.start_time.as_ref()).and_then(|t| DateTime::<Utc>::from_timestamp(t.0.as_second(), t.0.subsec_nanosecond() as u32)),
        containers: p.spec.as_ref().map(|s| s.containers.iter().map(|c| c.name.clone()).collect()).unwrap_or_default(),
        last_exit: containers
            .iter()
            .filter_map(|c| {
                let t = c.last_state.as_ref()?.terminated.as_ref()?;
                Some(t.reason.clone().unwrap_or_else(|| format!("exit {}", t.exit_code)))
            })
            .next(),
    }
}

/// Name of an image, without registry or tag (`…/prod/cart-api:0.2.147` → `cart-api`).
fn repository(image: &str) -> String {
    let image = image.split('@').next().unwrap_or(image);
    let last = image.rsplit('/').next().unwrap_or(image);
    last.split(':').next().unwrap_or(last).to_string()
}

/// Tag of an image (`…/cart-api:0.2.147` → `0.2.147`), without its digest.
fn tag(image: &str) -> String {
    let image = image.split('@').next().unwrap_or(image);
    match image.rsplit_once(':') {
        Some((_, tag)) if !tag.contains('/') => tag.to_string(),
        _ => "latest".into(),
    }
}

/// Role of a workload within its application: `billing-api-demo-worker-default` → `worker-default`,
/// `search-api-internal-prod` → `internal`, `cart-api-prod` → `api`, and
/// `media-prod-worker-default` (application `media-api`) → `worker-default`.
fn role(name: &str, app: Option<&str>, env: &str) -> String {
    let rest = app
        .and_then(|a| name.strip_prefix(a).or_else(|| name.strip_prefix(a.strip_suffix("-api")?)))
        .unwrap_or(name)
        .trim_start_matches('-');
    // Environment names are not roles: the workload's own one, and the stages'.
    let envs = crate::stages::get().envs();
    let parts: Vec<&str> = rest.split('-').filter(|p| !p.is_empty() && *p != env && !envs.contains(p)).collect();
    if parts.is_empty() { "api".into() } else { parts.join("-") }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The newest whole lines within the limit, as when lines were kept one by one.
    #[test]
    fn keeps_the_newest_whole_lines() {
        let one_by_one = |log: &[u8], max: usize| {
            let mut kept = std::collections::VecDeque::<Vec<u8>>::new();
            let mut size = 0;
            for line in log.split_inclusive(|&b| b == b'\n') {
                size += line.len();
                kept.push_back(line.to_vec());
                while size > max && kept.len() > 1 {
                    size -= kept.pop_front().map_or(0, |l| l.len());
                }
            }
            kept.into_iter().flatten().collect::<Vec<u8>>()
        };
        let mut seed = 11u64;
        let mut rnd = |n: u64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        for _ in 0..2000 {
            let log: Vec<u8> = (0..rnd(40)).flat_map(|_| {
                let mut l = vec![b'x'; rnd(9) as usize];
                if rnd(5) > 0 { l.push(b'\n') }
                l
            }).collect();
            let max = 1 + rnd(30) as usize;
            let mut cut = log.clone();
            super::newest_lines(&mut cut, max);
            assert_eq!(cut, one_by_one(&log, max), "{:?} within {max}", String::from_utf8_lossy(&log));
        }
    }

    #[test]
    fn image_tags() {
        assert_eq!(tag("registry.example/x/prod/cart-api:0.2.147"), "0.2.147");
        assert_eq!(tag("registry.example:5000/x/cart-api"), "latest");
        assert_eq!(tag("registry.example/x/cart-api:rc-1.0@sha256:abc"), "rc-1.0");
        assert_eq!(repository("registry.example:5000/x/prod/media-api:0.0.50"), "media-api");
    }

    #[test]
    fn pods_of_a_deployment() {
        let pod = |owner: &str| -> Pod {
            serde_json::from_value(serde_json::json!({ "metadata": { "name": "p", "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": owner, "uid": "u" }] } })).unwrap()
        };
        assert!(owned_by(&pod("cart-api-7d9f8b6c4"), "cart-api"));
        assert!(!owned_by(&pod("cart-api-worker-7d9f8b6c4"), "cart-api"));
        assert!(!owned_by(&pod("core-7d9f8b6c4"), "cart-api"));
    }

    #[test]
    fn workload_sums() {
        let pod = |name: &str, limits: serde_json::Value| -> Pod {
            serde_json::from_value(serde_json::json!({
                "metadata": { "name": name },
                "spec": { "containers": [{ "name": "app", "resources": { "requests": { "cpu": "250m", "memory": "256Mi" }, "limits": limits } }] }
            }))
            .unwrap()
        };
        let a = pod("a", serde_json::json!({ "cpu": "1", "memory": "512Mi" }));
        let b = pod("b", serde_json::json!({ "memory": "512Mi" }));
        let usage = HashMap::from([("a".to_string(), Usage { cpu_millis: 100, mem_bytes: 200 }), ("b".to_string(), Usage { cpu_millis: 50, mem_bytes: 100 })]);
        let load = workload_load([&a, &b].into_iter(), Some(&usage));
        assert_eq!((load.cpu_used_millis, load.mem_used_bytes), (Some(150), Some(300)));
        assert_eq!((load.cpu_requested_millis, load.mem_requested_bytes), (500, 512 * 1024 * 1024));
        // `b` has no CPU limit: the Deployment has none.
        assert_eq!((load.cpu_limit_millis, load.mem_limit_bytes), (None, Some(1024 * 1024 * 1024)));
        assert_eq!(workload_load([&a].into_iter(), None).cpu_used_millis, None);
    }

    #[test]
    fn last_update() {
        let d = |v: serde_json::Value| -> Deployment { serde_json::from_value(v).unwrap() };
        let rolled = d(serde_json::json!({
            "metadata": { "creationTimestamp": "2026-01-01T00:00:00Z" },
            "status": { "conditions": [
                { "type": "Available", "status": "True", "lastUpdateTime": "2026-10-06T09:00:00Z" },
                { "type": "Progressing", "status": "True", "lastUpdateTime": "2026-10-05T14:30:00Z", "lastTransitionTime": "2026-02-01T00:00:00Z" }
            ] }
        }));
        assert_eq!(updated_at(&rolled).unwrap().to_rfc3339(), "2026-10-05T14:30:00+00:00");
        let bare = d(serde_json::json!({ "metadata": { "creationTimestamp": "2026-01-01T00:00:00Z" } }));
        assert_eq!(updated_at(&bare).unwrap().to_rfc3339(), "2026-01-01T00:00:00+00:00");
    }

    #[test]
    fn roles() {
        assert_eq!(role("billing-api-preprod-worker-default", Some("billing-api"), "preprod"), "worker-default");
        assert_eq!(role("search-api-internal-prod", Some("search-api"), "prod"), "internal");
        assert_eq!(role("cart-api-prod", Some("cart-api"), "prod"), "api");
        assert_eq!(role("redis", None, "prod"), "redis");
        assert_eq!(role("media-prod-worker-default", Some("media-api"), "prod"), "worker-default");
        assert_eq!(role("media-staging", Some("media-api"), "staging"), "api");
    }
}
