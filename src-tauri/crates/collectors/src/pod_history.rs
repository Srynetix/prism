//! A pod's CPU and memory over time (`[clusters] usage_history`), for its logs pane. For a GKE
//! context (`[clusters] gcp_metrics`), read on demand from Google Managed Prometheus: cAdvisor's
//! metrics of any pod, days back. Otherwise, what Prism kept of metrics-server's readings of the
//! `[[pods]]` targets (`keep`, every 15 s by `pods`), since it started and for `KEPT` at most.

use prism_base::{i18n::tr, tr};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Result};
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use serde::Deserialize;

use crate::state::{HistorySource, Usage, UsageHistory, UsagePoint};

/// How far back Prism keeps its own readings.
pub const KEPT: Duration = Duration::from_secs(6 * 3600);
/// Points of a period read from Google Managed Prometheus, at most: its step follows.
const POINTS: i64 = 240;
/// cAdvisor is scraped every 30 s or so: a finer step only repeats it.
const MIN_STEP: i64 = 30;
/// gcloud's tokens last an hour: asked again before.
const TOKEN_FOR: Duration = Duration::from_secs(45 * 60);
const SERVICE: &str = "Cloud Monitoring";

/// Readings kept, by `pods::usage_key`: kept across a reload of the config.
type Readings = HashMap<String, VecDeque<(i64, Usage)>>;
static KEPT_USAGE: LazyLock<Mutex<Readings>> = LazyLock::new(Default::default);
static TOKEN: LazyLock<tokio::sync::Mutex<Option<(String, Instant)>>> = LazyLock::new(Default::default);

/// A target's readings (`usage`, its keys starting with `prefix`): each added to its pod's, the
/// pods gone forgotten.
pub fn keep(prefix: &str, usage: &BTreeMap<String, Usage>, at: i64) {
    let mut all = KEPT_USAGE.lock().unwrap();
    all.retain(|k, _| !k.starts_with(prefix) || usage.contains_key(k));
    let oldest = at - KEPT.as_secs() as i64;
    for (key, u) in usage {
        let kept = all.entry(key.clone()).or_default();
        kept.push_back((at, *u));
        while kept.front().is_some_and(|(t, _)| *t < oldest) {
            kept.pop_front();
        }
    }
}

/// What Prism kept of the pod `key` since `since`.
fn kept(key: &str, since: i64) -> Vec<UsagePoint> {
    let all = KEPT_USAGE.lock().unwrap();
    let readings = all.get(key).map(|k| k.iter().filter(|(t, _)| *t >= since).copied().collect::<Vec<_>>()).unwrap_or_default();
    readings.into_iter().map(|(at, u)| UsagePoint { at, cpu_millis: Some(u.cpu_millis), mem_bytes: Some(u.mem_bytes) }).collect()
}

/// Where a GKE context's metrics are: its project, location and cluster
/// (`gke_<project>_<location>_<cluster>`, none of them holding a `_`).
pub fn gke(context: &str) -> Option<(&str, &str, &str)> {
    let mut parts = context.strip_prefix("gke_")?.split('_');
    let (project, location, cluster) = (parts.next()?, parts.next()?, parts.next()?);
    (parts.next().is_none() && ![project, location, cluster].contains(&"")).then_some((project, location, cluster))
}

/// The pod asked for: its usage over the last `minutes`. `kept_key`: its key in Prism's own
/// readings (a pod of a `[[pods]]` target); `limits`: its containers' limits when known.
pub struct Request<'a> {
    pub context: &'a str,
    pub namespace: &'a str,
    pub pod: &'a str,
    pub minutes: u32,
    pub gcp: bool,
    pub kept_key: Option<String>,
    pub limits: Option<(Option<u64>, Option<u64>)>,
}

pub async fn history(r: Request<'_>) -> Result<UsageHistory> {
    let now = chrono::Utc::now().timestamp();
    let since = now - i64::from(r.minutes.max(1)) * 60;
    let limits = async {
        match r.limits {
            Some(l) => l,
            None => pod_limits(r.context, r.namespace, r.pod).await.unwrap_or_default(),
        }
    };
    let gcp = async {
        match gke(r.context) {
            Some(at) if r.gcp => Some(read_gmp(at, r.namespace, r.pod, since, now).await),
            _ => None,
        }
    };
    let ((cpu_limit_millis, mem_limit_bytes), gcp) = tokio::join!(limits, gcp);
    let mut error = None;
    match gcp {
        Some(Ok((points, step))) => return Ok(UsageHistory { source: HistorySource::Gcp, points, cpu_limit_millis, mem_limit_bytes, step, error }),
        Some(Err(e)) => error = Some(format!("{e:#}")),
        None => {}
    }
    let Some(key) = r.kept_key else {
        bail!(error.unwrap_or_else(|| tr("Prism keeps the usage of the [[pods]] targets' pods only").to_string()));
    };
    let step = super::pods::METRICS_EVERY.as_secs() as u32;
    Ok(UsageHistory { source: HistorySource::Prism, points: kept(&key, since), cpu_limit_millis, mem_limit_bytes, step, error })
}

/// A pod outside the `[[pods]]` targets (an environment's): its limits, read from its spec.
async fn pod_limits(context: &str, namespace: &str, pod: &str) -> Result<(Option<u64>, Option<u64>)> {
    // Kept a while: the chart polls every 15 s, and each read was a kube config parsed, a TLS
    // connection and a request, for limits a pod keeps for its life.
    type Limits = (Option<u64>, Option<u64>);
    static KEPT: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, (Instant, Limits)>>> = std::sync::LazyLock::new(Default::default);
    let key = format!("{context}\n{namespace}\n{pod}");
    if let Some((_, l)) = KEPT.lock().unwrap().get(&key).filter(|(at, _)| at.elapsed() < LIMITS_FOR) {
        return Ok(*l);
    }
    let api: Api<Pod> = Api::namespaced(super::pods::client(context).await?, namespace);
    let limits = super::pods::limits(&api.get(pod).await?);
    let mut kept = KEPT.lock().unwrap();
    kept.retain(|_, (at, _)| at.elapsed() < LIMITS_FOR);
    kept.insert(key, (Instant::now(), limits));
    Ok(limits)
}

/// How long a pod's limits are kept.
const LIMITS_FOR: Duration = Duration::from_secs(600);

async fn token(fresh: bool) -> Result<String> {
    let mut cached = TOKEN.lock().await;
    if let Some((token, _)) = cached.as_ref().filter(|(_, at)| !fresh && at.elapsed() < TOKEN_FOR) {
        return Ok(token.clone());
    }
    let out = crate::platform::command("gcloud").args(["auth", "print-access-token", "--quiet"]).stdin(std::process::Stdio::null()).kill_on_drop(true).output().await.map_err(|e| anyhow!("gcloud: {e}"))?;
    if !out.status.success() {
        bail!(tr("gcloud not logged in (gcloud auth login)"));
    }
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    *cached = Some((token.clone(), Instant::now()));
    Ok(token)
}

/// A PromQL label value, quoted.
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The pod's CPU (rate of its containers' CPU time) and memory (working set) between `since` and
/// `until`, a point each `step`.
async fn read_gmp((project, location, cluster): (&str, &str, &str), namespace: &str, pod: &str, since: i64, until: i64) -> Result<(Vec<UsagePoint>, u32)> {
    let step = ((until - since) / POINTS).max(MIN_STEP);
    let selector = format!(
        "cluster={},location={},namespace={},pod={},container!=\"\",container!=\"POD\"",
        quoted(cluster),
        quoted(location),
        quoted(namespace),
        quoted(pod)
    );
    // The rate over two steps at least: cAdvisor's counter needs two scrapes in its window.
    let window = (2 * step).max(120);
    let cpu = format!("sum(rate(container_cpu_usage_seconds_total{{{selector}}}[{window}s]))");
    let mem = format!("sum(container_memory_working_set_bytes{{{selector}}})");
    let (cpu, mem) = tokio::join!(query_range(project, &cpu, since, until, step), query_range(project, &mem, since, until, step));
    let (cpu, mem) = (cpu?, mem?);
    let mut points: BTreeMap<i64, UsagePoint> = BTreeMap::new();
    for (at, cores) in cpu {
        points.entry(at).or_insert(UsagePoint { at, cpu_millis: None, mem_bytes: None }).cpu_millis = Some((cores * 1000.0).round() as u64);
    }
    for (at, bytes) in mem {
        points.entry(at).or_insert(UsagePoint { at, cpu_millis: None, mem_bytes: None }).mem_bytes = Some(bytes.round() as u64);
    }
    Ok((points.into_values().collect(), step as u32))
}

#[derive(Deserialize)]
struct Answer {
    data: Option<Data>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct Data {
    result: Vec<Series>,
}

#[derive(Deserialize)]
struct Series {
    values: Vec<(f64, String)>,
}

/// One PromQL query over the project's metrics: its (only) series' values.
async fn query_range(project: &str, query: &str, since: i64, until: i64, step: i64) -> Result<Vec<(i64, f64)>> {
    let path = format!("/v1/projects/{}/location/global/prometheus/api/v1/query_range", prism_base::http::encode(project));
    let url = format!("https://monitoring.googleapis.com{path}");
    let params = [("query", query.to_string()), ("start", since.to_string()), ("end", until.to_string()), ("step", step.to_string())];
    let mut fresh = false;
    loop {
        let resp = prism_base::http::client().get(&url).query(&params).bearer_auth(token(fresh).await?).timeout(Duration::from_secs(20)).send().await?;
        // A token revoked before its hour: asked again, once.
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && !fresh {
            fresh = true;
            continue;
        }
        if resp.status() == reqwest::StatusCode::FORBIDDEN {
            bail!(tr!("{service} refused to read {project}'s metrics (monitoring.viewer role?)", service = SERVICE, project = project));
        }
        let body = prism_base::http::body(SERVICE, &path, resp).await?;
        let answer: Answer = prism_base::http::json(SERVICE, &path, &body)?;
        if let Some(e) = answer.error {
            bail!("{SERVICE}: {e}");
        }
        let series = answer.data.map(|d| d.result).unwrap_or_default();
        return Ok(series.into_iter().next().map(|s| s.values.into_iter().filter_map(|(at, v)| Some((at as i64, v.parse::<f64>().ok().filter(|v| v.is_finite())?))).collect()).unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_gke_context() {
        assert_eq!(gke("gke_shop-staging_europe-west9-a_staging"), Some(("shop-staging", "europe-west9-a", "staging")));
        assert_eq!(gke("gke_shop_europe-west9_apps_more"), None);
        assert_eq!(gke("minikube"), None);
        assert_eq!(gke("gke__europe-west9_apps"), None);
    }

    #[test]
    fn keeps_a_targets_readings_and_forgets_its_pods_gone() {
        let u = |cpu| Usage { cpu_millis: cpu, mem_bytes: 1 };
        let one = BTreeMap::from([("kept-t/ns/a".to_string(), u(10)), ("kept-t/ns/b".to_string(), u(20))]);
        keep("kept-t/", &one, 1000);
        keep("kept-t/", &BTreeMap::from([("kept-t/ns/a".to_string(), u(30))]), 1015);
        assert_eq!(kept("kept-t/ns/a", 0).iter().map(|p| (p.at, p.cpu_millis)).collect::<Vec<_>>(), [(1000, Some(10)), (1015, Some(30))]);
        assert!(kept("kept-t/ns/b", 0).is_empty());
        assert_eq!(kept("kept-t/ns/a", 1010).len(), 1);
        // Older than `KEPT`: dropped.
        keep("kept-t/", &BTreeMap::from([("kept-t/ns/a".to_string(), u(40))]), 1015 + KEPT.as_secs() as i64);
        assert_eq!(kept("kept-t/ns/a", 0).len(), 2);
    }

    #[test]
    fn quotes_a_label_value() {
        assert_eq!(quoted(r#"a"b\c"#), r#""a\"b\\c""#);
    }
}
