//! Clusters: pods, their logs, kube contexts, the vulnerability scan.


use prism_base::{i18n::tr, tr};
use super::State;


use crate::app::AppState;

use super::Cmd;

/// Environments block: the pods of a Deployment, to read their logs.
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn workload_pods(state: State<'_, AppState>, context: String, namespace: String, deployment: String) -> Cmd<Vec<crate::collectors::environments::PodInfo>> {
    if crate::demo::active() {
        return crate::demo::details::pods(&state.hub.subscribe().borrow(), &context, &namespace, &deployment).map_err(Into::into);
    }
    tracing::debug!("front: pods of {deployment} in {context} / {namespace}");
    state.pod_logs().pods(&context, &namespace, &deployment).await.map_err(Into::into)
}

/// Log of a pod's container; `previous`: the one from before its last restart. `lines`: how far back
/// (its last lines; 0, up to the container's start), the default tail when none.
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn pod_log(state: State<'_, AppState>, context: String, namespace: String, pod: String, container: Option<String>, previous: bool, lines: Option<u32>) -> Cmd<crate::collectors::environments::PodLog> {
    if crate::demo::active() {
        return crate::demo::details::pod_log(&state.hub.subscribe().borrow(), &context, &namespace, &pod, previous).map_err(Into::into);
    }
    state.pod_logs().log(&context, &namespace, &pod, container, previous, lines).await.map_err(Into::into)
}

/// A pod Prism shows, in the Pods block or in an environment's namespace (the logs pane of either).
fn pod_shown(state: &AppState, context: &str, namespace: &str, pod: &str) -> Result<(), String> {
    let shown = state.hub.subscribe().borrow().pods.iter().any(|p| p.context == context && p.namespace == namespace && p.name == pod);
    if !shown && !state.pod_logs().covers(context, namespace) {
        return Err(tr!("{context} / {namespace} is neither a followed pod nor an environment", context = context, namespace = namespace));
    }
    Ok(())
}

/// A pod may get a shell: `[clusters] shell` on, and the pod one Prism shows.
fn shell_allowed(state: &AppState, pod: &crate::agents::PodRef) -> Result<(), String> {
    if !state.hub.subscribe().borrow().pod_shell {
        return Err(tr("a shell in a pod is off ([clusters] shell)").into());
    }
    pod_shown(state, &pod.context, &pod.namespace, &pod.pod)
}

/// A pod's CPU and memory over its last `minutes`: from Google Managed Prometheus for a GKE context
/// (`[clusters] gcp_metrics`), otherwise what Prism kept of a `[[pods]]` target's.
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn pod_usage_history(state: State<'_, AppState>, context: String, namespace: String, pod: String, minutes: u32) -> Cmd<crate::state::UsageHistory> {
    if crate::demo::active() {
        return crate::demo::details::pod_usage_history(&state.hub.subscribe().borrow(), &context, &namespace, &pod, minutes).map_err(Into::into);
    }
    if !state.hub.subscribe().borrow().pod_history {
        return Err(tr("a pod's usage over time is off ([clusters] usage_history)").into());
    }
    pod_shown(&state, &context, &namespace, &pod)?;
    let gcp = crate::config::load(&crate::config_path()).map(|c| c.clusters.gcp_metrics).unwrap_or(true);
    // A followed pod: its key in Prism's readings, its limits already read.
    let followed = state.hub.subscribe().borrow().pods.iter().find(|p| p.context == context && p.namespace == namespace && p.name == pod).map(|p| (crate::collectors::pods::usage_key(&p.target, &namespace, &pod), (p.cpu_limit_millis, p.mem_limit_bytes)));
    let (kept_key, limits) = followed.map_or((None, None), |(k, l)| (Some(k), Some(l)));
    let request = crate::collectors::pod_history::Request { context: &context, namespace: &namespace, pod: &pod, minutes, gcp, kept_key, limits };
    crate::collectors::pod_history::history(request).await.map_err(|e| format!("{e:#}").into())
}

/// The pod's shells still open (`kubectl exec`, kept by tmux).
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn pod_shells(state: State<'_, AppState>, pod: crate::agents::PodRef) -> Cmd<Vec<crate::agents::Shell>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    shell_allowed(&state, &pod)?;
    state.agents.pod_shells(&pod).await.map_err(Into::into)
}

/// Opens the pod's shell `n`, or a new one (`null`): its terminal key.
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn pod_shell(state: State<'_, AppState>, pod: crate::agents::PodRef, n: Option<u32>) -> Cmd<String> {
    if crate::demo::active() {
        return Ok(format!("{}kp0000000000000000:{}", crate::agents::SHELL, n.unwrap_or(1)));
    }
    shell_allowed(&state, &pod)?;
    tracing::info!("front: shell in {} / {} / {}", pod.context, pod.namespace, pod.pod);
    state.agents.pod_shell(&pod, n).await.map_err(Into::into)
}

/// Closes the pod's shell `n`, whatever it runs.
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn pod_close_shell(state: State<'_, AppState>, pod: crate::agents::PodRef, n: u32) -> Cmd {
    state.agents.close_pod_shell(&pod, n).await.map_err(Into::into)
}

/// Settings: the `kubectl` contexts, to pick them rather than type them.
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn kube_contexts() -> Cmd<Vec<String>> {
    let out = crate::platform::command("kubectl").args(["config", "get-contexts", "-o", "name"]).output().await?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

/// Reruns the Trivy scan without waiting for the next day; the result arrives through the snapshot.
#[cfg_attr(feature = "tauri", tauri::command)]
pub fn rescan_vulnerabilities() {
    if crate::demo::active() {
        return;
    }
    crate::collectors::vulnerabilities::rescan();
}
