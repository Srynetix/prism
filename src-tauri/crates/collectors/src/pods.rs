//! The pods of the `[[pods]]` targets: each a cluster (its kube context) and its namespaces, all
//! of them or those matching a pattern. Their pods are watched (every change is pushed at once),
//! their usage and their nodes' load polled. A GitLab runner and the CI jobs its Kubernetes
//! executor creates are recognized as such: the alerts on runners and stuck jobs read them.

use prism_base::i18n::tr;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::{
    api::{ApiResource, DynamicObject, GroupVersionKind, ListParams},
    config::{KubeConfigOptions, Kubeconfig},
    runtime::{reflector, watcher, WatchStreamExt},
    Api, Client,
};

use crate::{
    config::{ClustersConfig, PodsTarget},
    state::{Hub, NodeLoad, PodRole, PodView, Usage},
};

const SOURCE: &str = "pods";
const METRICS_SOURCE: &str = "metrics";
const STARTING: [&str; 2] = ["ContainerCreating", "PodInitializing"];
/// metrics-server aggregates every ~15 s: polling more often is useless.
pub(crate) const METRICS_EVERY: Duration = Duration::from_secs(15);
/// A target over a whole cluster may span many nodes: the load of the busiest ones only.
const MAX_NODES: usize = 20;

/// A target, its context resolved.
#[derive(Clone)]
struct Target {
    label: String,
    context: String,
    cfg: PodsTarget,
    /// Its pods' readings kept, for their usage over time (`[clusters] usage_history`).
    history: bool,
}

/// The last error of each target, for the source's single message.
#[derive(Clone, Default)]
struct Errors(Arc<Mutex<BTreeMap<String, String>>>);

impl Errors {
    fn set(&self, hub: &Hub, source: &'static str, target: &str, error: Option<String>) {
        let mut all = self.0.lock().unwrap();
        let key = format!("{source}\0{target}");
        match error {
            Some(e) => all.insert(key, e),
            None => all.remove(&key),
        };
        let prefix = format!("{source}\0");
        let text: Vec<String> = all.iter().filter_map(|(k, e)| Some(format!("{}: {e}", k.strip_prefix(&prefix)?))).collect();
        hub.set_error(source, (!text.is_empty()).then(|| text.join(" · ")));
    }
}

/// Follows every target: one watch each, restarted after an error.
pub async fn run(targets: Vec<PodsTarget>, clusters: ClustersConfig, hub: Hub) {
    let errors = Errors::default();
    let resolved: Vec<Target> = targets.iter().map(|t| Target { label: t.label(&clusters), context: t.context_or(&clusters).to_string(), cfg: t.clone(), history: clusters.usage_history }).collect();
    let tasks = resolved.into_iter().map(|t| {
        let (hub, errors) = (hub.clone(), errors.clone());
        async move {
            loop {
                if let Err(e) = watch(&t, &hub, &errors).await {
                    tracing::warn!("pods: {}: {e:#}", t.label);
                    errors.set(&hub, SOURCE, &t.label, Some(format!("{e:#}")));
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    });
    futures::future::join_all(tasks).await;
}

pub(crate) async fn client(context: &str) -> Result<Client> {
    anyhow::ensure!(!context.is_empty(), "{}", tr("no kube context: set [[pods]] context or [clusters] default_context"));
    let options = KubeConfigOptions { context: Some(context.to_string()), ..Default::default() };
    let kubeconfig = kube::Config::from_custom_kubeconfig(Kubeconfig::read()?, &options).await?;
    Ok(Client::try_from(kubeconfig)?)
}

/// The target's pods: one watch per namespace named exactly, or one on the whole cluster,
/// filtered by its patterns.
async fn watch(t: &Target, hub: &Hub, errors: &Errors) -> Result<()> {
    let client = client(&t.context).await?;
    let apis: Vec<Api<Pod>> = match t.cfg.exact_namespaces() {
        Some(names) => names.iter().map(|ns| Api::namespaced(client.clone(), ns)).collect(),
        None => vec![Api::all(client.clone())],
    };
    let metrics = prism_base::rt::spawn(poll_metrics(client.clone(), t.clone(), hub.clone(), errors.clone()));
    let mut readers = Vec::new();
    let mut streams = Vec::new();
    for api in apis {
        let (reader, writer) = reflector::store();
        readers.push(reader);
        // We consume the raw events: the store is only filled at `InitDone`,
        // which `touched_objects()` would filter out.
        streams.push(reflector(writer, watcher(api, watcher::Config::default())).default_backoff().boxed());
    }
    let mut events = futures::stream::select_all(streams);
    let result = async {
        while events.try_next().await?.is_some() {
            let mut pods: Vec<PodView> = readers
                .iter()
                .flat_map(|r| r.state())
                .filter(|p| t.cfg.follows(p.metadata.namespace.as_deref().unwrap_or_default()))
                .map(|p| summarize(&t.label, &t.context, &p))
                .collect();
            pods.sort_by(|a, b| (&a.namespace, a.role, &a.name).cmp(&(&b.namespace, b.role, &b.name)));
            errors.set(hub, SOURCE, &t.label, None);
            hub.update(|s| {
                let mine = s.pods.iter().filter(|p| p.target == t.label);
                if mine.clone().count() == pods.len() && mine.zip(&pods).all(|(a, b)| a == b) {
                    return false;
                }
                s.pods.retain(|p| p.target != t.label);
                s.pods.extend(pods.iter().cloned());
                s.pods.sort_by(|a, b| (&a.target, &a.namespace, a.role, &a.name).cmp(&(&b.target, &b.namespace, b.role, &b.name)));
                true
            });
        }
        Ok(())
    }
    .await;
    metrics.abort();
    result
}

/// The key of a pod's usage: `<target>/<namespace>/<name>`.
pub fn usage_key(target: &str, namespace: &str, name: &str) -> String {
    format!("{target}/{namespace}/{name}")
}

/// Pod metrics cannot be "watched": we poll the metrics.k8s.io API. The load of the target's
/// nodes is re-read at the same pace.
async fn poll_metrics(client: Client, t: Target, hub: Hub, errors: Errors) {
    let gvk = GroupVersionKind::gvk("metrics.k8s.io", "v1beta1", "PodMetrics");
    let resource = ApiResource::from_gvk_with_plural(&gvk, "pods");
    let apis: Vec<Api<DynamicObject>> = match t.cfg.exact_namespaces() {
        Some(names) => names.iter().map(|ns| Api::namespaced_with(client.clone(), ns, &resource)).collect(),
        None => vec![Api::all_with(client.clone(), &resource)],
    };
    let prefix = format!("{}/", t.label);
    let mut first = true;
    loop {
        if !std::mem::take(&mut first) {
            hub.pause(METRICS_EVERY).await;
        }
        // The nodes hosting the most of the target's pods first.
        let mut counts: HashMap<String, usize> = HashMap::new();
        for p in hub.subscribe().borrow().pods.iter().filter(|p| p.target == t.label) {
            if let Some(n) = &p.node {
                *counts.entry(n.clone()).or_default() += 1;
            }
        }
        let mut names: Vec<(String, usize)> = counts.into_iter().collect();
        names.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let names: Vec<String> = names.into_iter().take(MAX_NODES).map(|(name, _)| name).collect();
        let mut nodes = if names.is_empty() { Vec::new() } else { node_loads(&client, &t.label, &names).await };
        nodes.sort_by(|a, b| a.name.cmp(&b.name));
        hub.update(|s| {
            let before: Vec<&NodeLoad> = s.pod_nodes.iter().filter(|n| n.target == t.label).collect();
            if before.len() == nodes.len() && before.iter().zip(&nodes).all(|(a, b)| *a == b) {
                return false;
            }
            s.pod_nodes.retain(|n| n.target != t.label);
            s.pod_nodes.extend(nodes.iter().cloned());
            true
        });
        let mut usage = BTreeMap::new();
        let mut failed = None;
        for api in &apis {
            match api.list(&ListParams::default()).await {
                Ok(list) => usage.extend(list.items.iter().filter_map(|m| {
                    let (ns, name) = (m.metadata.namespace.as_deref()?, m.metadata.name.as_deref()?);
                    t.cfg.follows(ns).then(|| (usage_key(&t.label, ns, name), pod_usage(&m.data)))
                })),
                Err(e) => failed = Some(format!("metrics-server: {e}")),
            }
        }
        // A failed read keeps nothing: its pods would look gone.
        if t.history && failed.is_none() {
            crate::pod_history::keep(&prefix, &usage, chrono::Utc::now().timestamp());
        }
        errors.set(&hub, METRICS_SOURCE, &t.label, failed);
        hub.update(|s| {
            let before: BTreeMap<&String, &Usage> = s.pod_usage.iter().filter(|(k, _)| k.starts_with(&prefix)).collect();
            if before.len() == usage.len() && before.iter().all(|(k, v)| usage.get(*k) == Some(*v)) {
                return false;
            }
            s.pod_usage.retain(|k, _| !k.starts_with(&prefix));
            s.pod_usage.extend(usage.clone());
            true
        });
    }
}

/// Load of the nodes `names`: allocatable, requests of their active pods and usage, in three
/// reads of the cluster at once (the nodes, the active pods, the nodes' metrics) rather than three
/// a node one after the other (60 every 15 s for 20 nodes). A read that fails (permissions on
/// nodes, metrics-server) leaves the others alone and is shown in the block.
async fn node_loads(client: &Client, target: &str, names: &[String]) -> Vec<NodeLoad> {
    let gvk = GroupVersionKind::gvk("metrics.k8s.io", "v1beta1", "NodeMetrics");
    let metrics: Api<DynamicObject> = Api::all_with(client.clone(), &ApiResource::from_gvk_with_plural(&gvk, "nodes"));
    let (all, active) = (ListParams::default(), ListParams::default().fields("status.phase!=Succeeded,status.phase!=Failed"));
    let (node_api, pod_api) = (Api::<Node>::all(client.clone()), Api::<Pod>::all(client.clone()));
    let (nodes, pods, metrics) = tokio::join!(node_api.list(&all), pod_api.list(&active), metrics.list(&all));
    let text = |e: kube::Error| e.to_string();
    fold_loads(target, names, nodes.map(|l| l.items).map_err(text), pods.map(|l| l.items).map_err(text), metrics.map(|l| l.items).map_err(text))
}

/// The nodes' loads from the three reads (`node_loads`).
fn fold_loads(target: &str, names: &[String], nodes: Result<Vec<Node>, String>, pods: Result<Vec<Pod>, String>, metrics: Result<Vec<DynamicObject>, String>) -> Vec<NodeLoad> {
    names
        .iter()
        .map(|name| {
            let mut load = NodeLoad {
                target: target.to_string(),
                name: name.clone(),
                cpu_used_millis: None,
                mem_used_bytes: None,
                cpu_requested_millis: 0,
                mem_requested_bytes: 0,
                cpu_allocatable_millis: None,
                mem_allocatable_bytes: None,
                pods: 0,
                max_pods: None,
                error: None,
            };
            let mut errors = Vec::new();
            match &nodes {
                Ok(nodes) => match nodes.iter().find(|n| n.metadata.name.as_ref() == Some(name)) {
                    Some(node) => {
                        let alloc = node.status.as_ref().and_then(|s| s.allocatable.as_ref());
                        load.cpu_allocatable_millis = alloc.and_then(|a| a.get("cpu")).and_then(|q| cpu_millis(&q.0));
                        load.mem_allocatable_bytes = alloc.and_then(|a| a.get("memory")).and_then(|q| bytes(&q.0));
                        load.max_pods = alloc.and_then(|a| a.get("pods")).and_then(|q| q.0.parse().ok());
                    }
                    None => errors.push("node: not found".to_string()),
                },
                Err(e) => errors.push(format!("node: {e}")),
            }
            match &pods {
                Ok(pods) => {
                    for p in pods.iter().filter(|p| p.spec.as_ref().and_then(|s| s.node_name.as_ref()) == Some(name)) {
                        let (cpu, mem) = requests(p);
                        load.pods += 1;
                        load.cpu_requested_millis += cpu;
                        load.mem_requested_bytes += mem;
                    }
                }
                Err(e) => errors.push(format!("pods: {e}")),
            }
            match &metrics {
                Ok(metrics) => match metrics.iter().find(|m| m.metadata.name.as_ref() == Some(name)) {
                    Some(m) => {
                        load.cpu_used_millis = m.data["usage"]["cpu"].as_str().and_then(cpu_millis);
                        load.mem_used_bytes = m.data["usage"]["memory"].as_str().and_then(bytes);
                    }
                    None => errors.push("metrics: none for this node yet".to_string()),
                },
                Err(e) => errors.push(format!("metrics: {e}")),
            }
            if !errors.is_empty() {
                load.error = Some(errors.join(" · "));
            }
            load
        })
        .collect()
}

/// A pod's requests, as the scheduler counts them: the sum of the containers, or the largest
/// init container if it asks for more.
pub fn requests(pod: &Pod) -> (u64, u64) {
    let Some(spec) = pod.spec.as_ref() else { return (0, 0) };
    let of = |c: &k8s_openapi::api::core::v1::Container| {
        let r = c.resources.as_ref().and_then(|r| r.requests.as_ref());
        let get = |key: &str, parse: fn(&str) -> Option<u64>| r.and_then(|r| r.get(key)).and_then(|q| parse(&q.0)).unwrap_or(0);
        (get("cpu", cpu_millis), get("memory", bytes))
    };
    let (cpu, mem) = spec.containers.iter().map(of).fold((0, 0), |(a, b), (c, m)| (a + c, b + m));
    let (icpu, imem) = spec.init_containers.iter().flatten().map(of).fold((0, 0), |(a, b), (c, m)| (a.max(c), b.max(m)));
    (cpu.max(icpu), mem.max(imem))
}

pub fn pod_usage(data: &serde_json::Value) -> Usage {
    let containers = data["containers"].as_array().map(Vec::as_slice).unwrap_or_default();
    Usage {
        cpu_millis: containers.iter().filter_map(|c| cpu_millis(c["usage"]["cpu"].as_str()?)).sum(),
        mem_bytes: containers.iter().filter_map(|c| bytes(c["usage"]["memory"].as_str()?)).sum(),
    }
}

/// What a pod is: a CI job (its runner's annotations, or its name), a GitLab runner (its image or
/// its `app` label), or any pod.
fn role(pod: &Pod) -> PodRole {
    let name = pod.metadata.name.as_deref().unwrap_or_default();
    let annotated = pod.metadata.annotations.as_ref().is_some_and(|a| a.contains_key("job.runner.gitlab.com/name"));
    if annotated || (name.starts_with("runner-") && name.contains("-project-")) {
        return PodRole::Job;
    }
    let labels = pod.metadata.labels.as_ref();
    let labelled = ["app", "app.kubernetes.io/name"].iter().any(|k| labels.and_then(|l| l.get(*k)).is_some_and(|v| v.contains("gitlab-runner")));
    let image = pod.spec.as_ref().is_some_and(|s| s.containers.iter().any(|c| c.image.as_deref().is_some_and(|i| i.contains("gitlab-runner"))));
    if labelled || image { PodRole::Runner } else { PodRole::Pod }
}

fn summarize(target: &str, context: &str, pod: &Pod) -> PodView {
    let status = pod.status.as_ref();
    let containers = status.and_then(|s| s.container_statuses.as_deref()).unwrap_or_default();
    let restarts = containers.iter().map(|c| c.restart_count).sum();
    let waiting = containers
        .iter()
        .find_map(|c| c.state.as_ref()?.waiting.as_ref()?.reason.clone());
    let phase = status.and_then(|s| s.phase.clone()).unwrap_or_else(|| "Unknown".into());
    let all_ready = !containers.is_empty() && containers.iter().all(|c| c.ready);
    let terminating = pod.metadata.deletion_timestamp.is_some();

    let (label, healthy) = match (waiting, terminating) {
        (_, true) => ("Terminating".to_string(), true),
        // Normal pod startup stages: to display, not to flag.
        (Some(reason), _) if STARTING.contains(&reason.as_str()) => (reason, true),
        (Some(reason), _) => (reason, false),
        // A finished pod (a CI job, a Job's) is normal.
        (None, _) if phase == "Succeeded" => (phase, true),
        (None, _) => {
            let ok = phase == "Running" && all_ready;
            (phase, ok)
        }
    };

    let annotation = |key: &str| pod.metadata.annotations.as_ref()?.get(key).cloned();
    let (cpu_limit_millis, mem_limit_bytes) = limits(pod);
    let name = pod.metadata.name.clone().unwrap_or_default();
    let role = role(pod);
    let time = |t: &k8s_openapi::apimachinery::pkg::apis::meta::v1::Time| chrono::DateTime::from_timestamp(t.0.as_second(), t.0.subsec_nanosecond() as u32);
    let last_exit = containers
        .iter()
        .filter_map(|c| {
            let t = c.last_state.as_ref()?.terminated.as_ref()?;
            // `Error` says less than its code.
            let reason = t.reason.clone().filter(|r| r != "Error").unwrap_or_else(|| format!("exit {}", t.exit_code));
            Some((t.finished_at.as_ref().and_then(time), reason))
        })
        .max_by_key(|(at, _)| *at);
    let unscheduled = status
        .and_then(|s| s.conditions.as_ref())
        .and_then(|cs| cs.iter().find(|c| c.type_ == "PodScheduled" && c.status == "False"))
        .map(|c| c.message.clone().or_else(|| c.reason.clone()).unwrap_or_else(|| tr("not scheduled").into()));
    PodView {
        target: target.to_string(),
        context: context.to_string(),
        namespace: pod.metadata.namespace.clone().unwrap_or_default(),
        workload: workload(pod),
        role,
        is_job: role == PodRole::Job,
        job_name: annotation("job.runner.gitlab.com/name"),
        job_ref: annotation("job.runner.gitlab.com/ref"),
        job_url: annotation("job.runner.gitlab.com/url"),
        cpu_limit_millis,
        mem_limit_bytes,
        name,
        status: label,
        healthy,
        restarts,
        started_at: status
            .and_then(|s| s.start_time.as_ref())
            .and_then(|t| chrono::DateTime::from_timestamp(t.0.as_second(), t.0.subsec_nanosecond() as u32)),
        node: pod.spec.as_ref().and_then(|s| s.node_name.clone()),
        last_exit_at: last_exit.as_ref().and_then(|(at, _)| *at),
        last_exit: last_exit.map(|(_, r)| r),
        unscheduled,
    }
}

/// What runs a pod: a ReplicaSet's Deployment (`<deployment>-<hash>`), or its owner.
fn workload(pod: &Pod) -> Option<String> {
    let owner = pod.metadata.owner_references.iter().flatten().find(|o| o.controller == Some(true)).or_else(|| pod.metadata.owner_references.as_ref()?.first())?;
    match owner.kind.as_str() {
        "ReplicaSet" => Some(owner.name.rsplit_once('-').map_or(owner.name.as_str(), |(d, _)| d).to_string()),
        _ => Some(owner.name.clone()),
    }
}

/// Per-container sum of the limit, or of the request when no limit is set.
pub(crate) fn limits(pod: &Pod) -> (Option<u64>, Option<u64>) {
    let containers = pod.spec.as_ref().map(|s| s.containers.as_slice()).unwrap_or_default();
    let sum = |key: &str, parse: fn(&str) -> Option<u64>| {
        let values: Vec<u64> = containers
            .iter()
            .filter_map(|c| {
                let r = c.resources.as_ref()?;
                let q = r.limits.as_ref().and_then(|l| l.get(key)).or_else(|| r.requests.as_ref()?.get(key))?;
                parse(&q.0)
            })
            .collect();
        (!values.is_empty()).then(|| values.iter().sum())
    };
    (sum("cpu", cpu_millis), sum("memory", bytes))
}

/// Kubernetes CPU quantity (`2`, `500m`, `1500000n`) in millicores.
pub fn cpu_millis(q: &str) -> Option<u64> {
    let (num, factor) = match q.as_bytes().last()? {
        b'n' => (&q[..q.len() - 1], 1e-6),
        b'u' => (&q[..q.len() - 1], 1e-3),
        b'm' => (&q[..q.len() - 1], 1.0),
        _ => (q, 1000.0),
    };
    Some((num.parse::<f64>().ok()? * factor).round() as u64)
}

/// Kubernetes memory quantity (`512Mi`, `1G`, `1048576`) in bytes.
pub fn bytes(q: &str) -> Option<u64> {
    const UNITS: [(&str, f64); 12] = [
        ("Ki", 1024.0), ("Mi", 1048576.0), ("Gi", 1073741824.0), ("Ti", 1099511627776.0),
        ("k", 1e3), ("K", 1e3), ("M", 1e6), ("G", 1e9), ("T", 1e12), ("m", 1e-3), ("E", 1e18), ("P", 1e15),
    ];
    let (num, factor) = UNITS
        .iter()
        .find_map(|(u, f)| q.strip_suffix(u).map(|n| (n, *f)))
        .unwrap_or((q, 1.0));
    Some((num.parse::<f64>().ok()? * factor).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_loads_from_three_reads_of_the_cluster() {
        use serde_json::json;
        let node: Node = serde_json::from_value(json!({ "metadata": { "name": "n1" }, "status": { "allocatable": { "cpu": "4", "memory": "8Gi", "pods": "110" } } })).unwrap();
        let pod = |name: &str, on: &str, cpu: &str| -> Pod { serde_json::from_value(json!({ "metadata": { "name": name }, "spec": { "nodeName": on, "containers": [{ "name": "c", "resources": { "requests": { "cpu": cpu } } }] } })).unwrap() };
        let usage: DynamicObject = serde_json::from_value(json!({ "apiVersion": "metrics.k8s.io/v1beta1", "kind": "NodeMetrics", "metadata": { "name": "n1" }, "usage": { "cpu": "500m", "memory": "1Gi" } })).unwrap();
        let names = ["n1".to_string(), "n2".to_string()];
        let loads = fold_loads("t", &names, Ok(vec![node]), Ok(vec![pod("a", "n1", "250m"), pod("b", "n1", "100m"), pod("c", "n2", "1")]), Ok(vec![usage]));
        let n1 = &loads[0];
        assert_eq!((n1.pods, n1.cpu_requested_millis, n1.cpu_allocatable_millis, n1.max_pods, n1.cpu_used_millis, n1.error.as_deref()), (2, 350, Some(4000), Some(110), Some(500), None));
        assert_eq!((loads[1].pods, loads[1].cpu_requested_millis), (1, 1000));
        assert_eq!(loads[1].error.as_deref(), Some("node: not found · metrics: none for this node yet"));
        // A read refused: said on every node, the others read.
        let refused = fold_loads("t", &names[..1], Err("forbidden".into()), Ok(Vec::new()), Ok(Vec::new()));
        assert_eq!(refused[0].error.as_deref(), Some("node: forbidden · metrics: none for this node yet"));
    }

    #[test]
    fn parses_cpu_quantities() {
        assert_eq!(cpu_millis("2"), Some(2000));
        assert_eq!(cpu_millis("500m"), Some(500));
        assert_eq!(cpu_millis("14520000n"), Some(15));
    }

    #[test]
    fn sums_requests_like_the_scheduler() {
        let pod: Pod = serde_json::from_value(serde_json::json!({
            "spec": {
                "containers": [
                    { "name": "build", "resources": { "requests": { "cpu": "500m", "memory": "1Gi" } } },
                    { "name": "helper", "resources": { "requests": { "cpu": "100m", "memory": "128Mi" } } },
                ],
                "initContainers": [{ "name": "init", "resources": { "requests": { "cpu": "1", "memory": "64Mi" } } }],
            }
        }))
        .unwrap();
        assert_eq!(requests(&pod), (1000, (1024 + 128) * 1024 * 1024));
    }

    #[test]
    fn a_pod_is_a_runner_a_job_or_a_pod() {
        let pod = |v: serde_json::Value| -> Pod { serde_json::from_value(v).unwrap() };
        let job = pod(serde_json::json!({ "metadata": { "name": "runner-x7k2-project-12-concurrent-0", "annotations": { "job.runner.gitlab.com/name": "unit-tests" } } }));
        let runner = pod(serde_json::json!({ "metadata": { "name": "gitlab-runner-6c9f7d" }, "spec": { "containers": [{ "name": "runner", "image": "registry.gitlab.com/gitlab-org/gitlab-runner:v17.5.2" }] } }));
        let labelled = pod(serde_json::json!({ "metadata": { "name": "ci-1", "labels": { "app": "gitlab-runner" } } }));
        let api = pod(serde_json::json!({ "metadata": { "name": "shop-api-7d9c5b-2xkq" }, "spec": { "containers": [{ "name": "api", "image": "shop/api:1.4" }] } }));
        assert_eq!([role(&job), role(&runner), role(&labelled), role(&api)], [PodRole::Job, PodRole::Runner, PodRole::Runner, PodRole::Pod]);
        let view = summarize("Prod", "prod-ctx", &pod(serde_json::json!({ "metadata": { "name": "a", "namespace": "shop" }, "status": { "phase": "Succeeded" } })));
        assert_eq!((view.target.as_str(), view.namespace.as_str(), view.healthy), ("Prod", "shop", true));
    }

    #[test]
    fn reads_the_workload_the_last_exit_and_why_not_scheduled() {
        let pod: Pod = serde_json::from_value(serde_json::json!({
            "metadata": { "name": "shop-worker-5f6b8c-r4wd", "namespace": "shop", "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "shop-worker-5f6b8c", "uid": "1", "controller": true }] },
            "status": {
                "phase": "Running",
                "containerStatuses": [{ "name": "worker", "image": "w", "imageID": "", "ready": false, "restartCount": 7,
                    "state": { "waiting": { "reason": "CrashLoopBackOff" } },
                    "lastState": { "terminated": { "exitCode": 1, "reason": "Error", "finishedAt": "2026-10-14T08:00:00Z" } } }],
            }
        }))
        .unwrap();
        let v = summarize("Prod", "prod-ctx", &pod);
        assert_eq!((v.workload.as_deref(), v.last_exit.as_deref(), v.context.as_str()), (Some("shop-worker"), Some("exit 1"), "prod-ctx"));
        assert!(v.last_exit_at.is_some());
        let pending: Pod = serde_json::from_value(serde_json::json!({
            "metadata": { "name": "db-0", "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "StatefulSet", "name": "db", "uid": "2" }] },
            "status": { "phase": "Pending", "conditions": [{ "type": "PodScheduled", "status": "False", "reason": "Unschedulable", "message": "0/3 nodes are available: 3 Insufficient cpu." }] }
        }))
        .unwrap();
        let v = summarize("Prod", "prod-ctx", &pending);
        assert_eq!((v.workload.as_deref(), v.unscheduled.as_deref(), v.healthy), (Some("db"), Some("0/3 nodes are available: 3 Insufficient cpu."), false));
    }

    #[test]
    fn parses_memory_quantities() {
        assert_eq!(bytes("512Mi"), Some(512 * 1024 * 1024));
        assert_eq!(bytes("1G"), Some(1_000_000_000));
        assert_eq!(bytes("61440Ki"), Some(61440 * 1024));
    }
}
