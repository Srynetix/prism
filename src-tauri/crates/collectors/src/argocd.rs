//! ArgoCD applications, read read-only as Kubernetes objects (`applications.argoproj.io`)
//! on the cluster hosting the instances, every 30 s. Without `[[argocd]]`, the instances
//! are the `argocd*` namespaces of the default context's cluster (`[clusters] default_context`).
//!
//! Only what Prism displays is kept: name, environment and application (from the path of the
//! Helm values, `argocd/<group>/<env>/apps/<app>/…`), sync and health state, last
//! operation, image tags and managed Deployments. Neither cluster address nor repository.

use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Namespace;
use kube::{
    api::{ApiResource, DynamicObject, GroupVersionKind, ListParams},
    config::{KubeConfigOptions, Kubeconfig},
    Api, Client,
};
use serde_json::Value;

use crate::{
    config::ArgoConfig,
    state::{ArgoApp, ArgoCd, ArgoDeploy, ArgoInstance, ArgoResource, Hub},
};

const SOURCE: &str = "argocd";

/// How Applications are read (`[argocd_layout]`), with the stages' environments (`stages`).
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub namespace_prefix: String,
    pub root: String,
    pub apps_segment: String,
    pub customer_groups: Vec<String>,
    /// Environment names a path segment can be: the stages' ones.
    pub envs: Vec<String>,
    /// That of customers' instances: production's.
    pub production: String,
}

impl Layout {
    pub fn of(cfg: &crate::config::ArgoLayoutConfig, stages: &crate::stages::Stages) -> Self {
        Self {
            namespace_prefix: cfg.namespace_prefix.trim().to_string(),
            root: cfg.root.trim().trim_matches('/').to_string(),
            apps_segment: cfg.apps_segment.trim().to_string(),
            customer_groups: cfg.customer_groups.clone(),
            envs: stages.envs().into_iter().map(String::from).collect(),
            production: stages.production().to_string(),
        }
    }
}

static LAYOUT: prism_base::rt::Swap<Layout> = prism_base::rt::Swap::new();

fn current() -> &'static Layout {
    LAYOUT.get().unwrap_or_else(|| LAYOUT.set(Layout::of(&Default::default(), crate::stages::get())))
}
const POLL: Duration = Duration::from_secs(30);
/// While an operation runs, followed closely: the sync view shows it resource by resource.
const POLL_SYNCING: Duration = Duration::from_secs(5);
/// Resources kept per list, history entries per Application.
const MAX_RESOURCES: usize = 25;
const MAX_HISTORY: usize = 5;

pub async fn run(configured: Vec<ArgoConfig>, layout: Layout, default_context: String, hub: Hub) {
    LAYOUT.set(layout);
    let mut first = true;
    let mut instances = configured;
    let mut syncing = false;
    loop {
        if !std::mem::take(&mut first) {
            hub.pause(if syncing { POLL_SYNCING } else { POLL }).await;
        }
        if instances.is_empty() {
            match discover(&default_context).await {
                Ok(found) if !found.is_empty() => instances = found,
                // No instance: the source answered anyway (startup screen).
                Ok(_) => return hub.set_error(SOURCE, None),
                Err(e) => {
                    hub.set_error(SOURCE, Some(format!("discovery: {e:#}")));
                    continue;
                }
            }
        }
        let mut read = Vec::new();
        for i in &instances {
            read.push(instance(i, &default_context).await);
        }
        let failed = read.iter().filter(|i| i.error.is_some()).count();
        hub.set_error(SOURCE, (failed == read.len()).then(|| read[0].error.clone().unwrap_or_default()));
        syncing = read.iter().flat_map(|i| &i.apps).any(|a| a.operation.as_deref() == Some("Running"));
        let state = ArgoCd { instances: read };
        hub.update(|s| {
            let changed = s.argocd.as_ref() != Some(&state);
            s.argocd = Some(state);
            changed
        });
    }
}

async fn client(context: &str) -> Result<Client> {
    let options = KubeConfigOptions { context: Some(context.to_string()), ..Default::default() };
    Ok(Client::try_from(kube::Config::from_custom_kubeconfig(Kubeconfig::read()?, &options).await?)?)
}

/// The namespaces of the default context's cluster named like ArgoCD's (`[argocd_layout] namespace_prefix`).
async fn discover(context: &str) -> Result<Vec<ArgoConfig>> {
    let namespaces = Api::<Namespace>::all(client(context).await?).list(&ListParams::default()).await?;
    Ok(namespaces
        .items
        .iter()
        .filter_map(|n| n.metadata.name.clone())
        .filter(|n| !current().namespace_prefix.is_empty() && n.starts_with(&current().namespace_prefix))
        .map(|namespace| ArgoConfig { namespace, context: None, url: None, envs: None })
        .collect())
}

async fn instance(cfg: &ArgoConfig, default_context: &str) -> ArgoInstance {
    let context = cfg.context.as_deref().unwrap_or(default_context);
    let cluster = context.rsplit('_').next().unwrap_or(context).to_string();
    let mut out = ArgoInstance { cluster, namespace: cfg.namespace.clone(), url: cfg.url.clone(), envs: cfg.envs.clone(), error: None, apps: Vec::new() };
    let result = async {
        let client = client(context).await?;
        let resource = ApiResource::from_gvk_with_plural(&GroupVersionKind::gvk("argoproj.io", "v1alpha1", "Application"), "applications");
        let list = Api::<DynamicObject>::namespaced_with(client, &cfg.namespace, &resource).list(&ListParams::default()).await?;
        Ok::<_, anyhow::Error>(list.items)
    }
    .await;
    match result {
        Ok(items) => {
            out.apps = items.iter().map(app).collect();
            out.apps.sort_by(|a, b| a.name.cmp(&b.name));
        }
        Err(e) => {
            tracing::warn!("argocd: {} {}: {e:#}", out.cluster, cfg.namespace);
            out.error = Some(format!("{e:#}").lines().next().unwrap_or_default().chars().take(200).collect());
        }
    }
    out
}

fn app(o: &DynamicObject) -> ArgoApp {
    let d = &o.data;
    let status = &d["status"];
    let str_at = |v: &Value| v.as_str().map(String::from);
    let time = |v: &Value| v.as_str().and_then(|t| DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&Utc));
    // Helm values: `spec.source` or, with multiple sources, `spec.sources[]`.
    let value_files: Vec<&str> = std::iter::once(&d["spec"]["source"])
        .chain(d["spec"]["sources"].as_array().into_iter().flatten())
        .flat_map(|s| s["helm"]["valueFiles"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let (group, env, application, customer) = value_files.iter().find_map(|p| layout(p, current())).unwrap_or_default();
    let operation = &status["operationState"];
    // First error condition (ComparisonError, SyncError…), otherwise the operation's message.
    let condition = status["conditions"].as_array().into_iter().flatten().find(|c| c["type"].as_str().is_some_and(|t| t.ends_with("Error"))).and_then(|c| str_at(&c["message"]));
    ArgoApp {
        name: o.metadata.name.clone().unwrap_or_default(),
        env,
        group,
        app: application,
        customer,
        namespace: str_at(&d["spec"]["destination"]["namespace"]),
        sync: str_at(&status["sync"]["status"]).unwrap_or_else(|| "Unknown".into()),
        health: str_at(&status["health"]["status"]).unwrap_or_else(|| "Unknown".into()),
        revision: str_at(&status["sync"]["revision"]).map(|r| r.chars().take(8).collect()),
        operation: str_at(&operation["phase"]),
        message: condition.or_else(|| str_at(&operation["message"])).map(|m| m.chars().take(300).collect()),
        synced_at: time(&operation["finishedAt"]).or_else(|| time(&status["reconciledAt"])),
        images: status["summary"]["images"].as_array().into_iter().flatten().filter_map(Value::as_str).map(|i| i.rsplit(':').next().unwrap_or(i).to_string()).collect(),
        deployments: status["resources"].as_array().into_iter().flatten().filter(|r| r["kind"] == "Deployment").filter_map(|r| str_at(&r["name"])).collect(),
        started_at: time(&operation["startedAt"]),
        automated: operation["operation"]["initiatedBy"]["automated"].as_bool().unwrap_or(false),
        sync_resources: sync_resources(&operation["syncResult"]["resources"]),
        drifting: drifting(&status["resources"]),
        history: status["history"]
            .as_array()
            .into_iter()
            .flatten()
            .rev()
            .take(MAX_HISTORY)
            .map(|h| ArgoDeploy { revision: str_at(&h["revision"]).map(|r| r.chars().take(8).collect()), deployed_at: time(&h["deployedAt"]) })
            .collect(),
    }
}

/// A short text, without its trailing spaces; none when empty.
fn text(v: &Value) -> Option<String> {
    v.as_str().map(|t| t.trim().chars().take(300).collect::<String>()).filter(|t| !t.is_empty())
}

/// The last sync's resources: those that did not sync (or whose hook did not succeed) first.
fn sync_resources(list: &Value) -> Vec<ArgoResource> {
    let mut out: Vec<ArgoResource> = list
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| ArgoResource { kind: text(&r["kind"]).unwrap_or_default(), name: text(&r["name"]).unwrap_or_default(), status: text(&r["status"]), health: None, hook: text(&r["hookPhase"]), message: text(&r["message"]) })
        .collect();
    let fine = |r: &ArgoResource| r.status.as_deref().is_none_or(|s| s == "Synced") && r.hook.as_deref().is_none_or(|h| h == "Succeeded");
    out.sort_by_key(|r| fine(r));
    out.truncate(MAX_RESOURCES);
    out
}

/// The resources out of sync or not healthy now.
fn drifting(list: &Value) -> Vec<ArgoResource> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["status"].as_str().is_some_and(|s| s != "Synced") || r["health"]["status"].as_str().is_some_and(|h| h != "Healthy"))
        .take(MAX_RESOURCES)
        .map(|r| ArgoResource { kind: text(&r["kind"]).unwrap_or_default(), name: text(&r["name"]).unwrap_or_default(), status: text(&r["status"]), health: text(&r["health"]["status"]), hook: None, message: text(&r["health"]["message"]) })
        .collect()
}

/// A Helm values path → (group, environment, component, customer). After `root`, the first segment
/// naming a stage's environment is the environment, the one before it the group, the one after
/// `apps_segment` the component: `argocd/<group>/<env>/apps/<app>/values.yaml`. A customer group
/// first (`argocd/customers/<customer>/apps/<app>/…`): production, a customer's instance.
type Placement = (Option<String>, Option<String>, Option<String>, bool);

fn layout(path: &str, l: &Layout) -> Option<Placement> {
    let segments = path.split('/').filter(|p| !p.is_empty() && *p != "." && *p != "..");
    let parts: Vec<&str> = if l.root.is_empty() { segments.collect() } else { segments.skip_while(|p| *p != l.root).skip(1).collect() };
    let app = parts.iter().position(|p| !l.apps_segment.is_empty() && *p == l.apps_segment).and_then(|i| parts.get(i + 1)).map(|a| a.to_string());
    if let Some(first) = parts.first().filter(|f| l.customer_groups.iter().any(|g| g == *f)) {
        return Some((Some(first.to_string()), Some(l.production.clone()), app, true));
    }
    let at = parts.iter().position(|p| l.envs.iter().any(|e| e == p))?;
    let group = at.checked_sub(1).map(|i| parts[i].to_string());
    Some((group, Some(parts[at].to_string()), app, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helm_layout() {
        let cfg = crate::config::ArgoLayoutConfig { root: "argocd".into(), customer_groups: vec!["customers".into()], ..Default::default() };
        let stage = |role: &str, env: &str| crate::stages::Stage { role: role.into(), name: role.into(), env: env.into() };
        let l = Layout::of(&cfg, &crate::stages::Stages(vec![stage("staging", "staging"), stage("preprod", "preprod"), stage("production", "prod")]));
        let s = |v: &str| Some(v.to_string());
        assert_eq!(layout("../../../argocd/acme/staging/apps/cart-api/values.yaml", &l), Some((s("acme"), s("staging"), s("cart-api"), false)));
        assert_eq!(layout("../../../argocd/data/prod/apps/media-api/envs.yaml", &l), Some((s("data"), s("prod"), s("media-api"), false)));
        assert_eq!(layout("../../../argocd/customers/acme/apps/intake-api/values.yaml", &l), Some((s("customers"), s("prod"), s("intake-api"), true)));
        assert_eq!(layout("../../../argocd/data/staging/superset/values.yaml", &l), Some((s("data"), s("staging"), None, false)));
        assert_eq!(layout("../../../argocd/staging/apps/cart-api/values.yaml", &l), Some((None, s("staging"), s("cart-api"), false)));
        assert_eq!(layout("values.yaml", &l), None);
    }

    #[test]
    fn reads_an_application() {
        let o: DynamicObject = serde_json::from_value(serde_json::json!({
            "apiVersion": "argoproj.io/v1alpha1", "kind": "Application",
            "metadata": { "name": "cart-api-staging" },
            "spec": { "destination": { "server": "https://10.0.0.1", "namespace": "apps" }, "source": { "repoURL": "git@x:y.git", "helm": { "valueFiles": ["../../../argocd/acme/staging/apps/cart-api/values.yaml"] } } },
            "status": {
                "sync": { "status": "OutOfSync", "revision": "0123456789abcdef" },
                "health": { "status": "Degraded" },
                "operationState": {
                    "phase": "Failed", "message": "one or more objects failed to apply", "startedAt": "2026-10-07T09:59:30Z", "finishedAt": "2026-10-07T10:00:00Z",
                    "operation": { "initiatedBy": { "automated": true } },
                    "syncResult": { "resources": [
                        { "kind": "Service", "name": "cart-api", "status": "Synced", "message": "service/cart-api unchanged" },
                        { "kind": "Deployment", "name": "cart-api", "status": "SyncFailed", "message": "forbidden" },
                        { "kind": "Job", "name": "migrate", "status": "Synced", "hookPhase": "Failed", "message": "Job has reached the specified backoff limit" }
                    ] }
                },
                "history": [{ "revision": "aaaaaaaaaaaa", "deployedAt": "2026-10-06T08:00:00Z" }, { "revision": "bbbbbbbbbbbb", "deployedAt": "2026-10-07T08:00:00Z" }],
                "conditions": [{ "type": "SyncError", "message": "Deployment cart-api: forbidden" }],
                "summary": { "images": ["europe-docker.pkg.dev/x/cart-api:0.2.147"] },
                "resources": [{ "kind": "Deployment", "name": "cart-api", "status": "OutOfSync", "health": { "status": "Degraded", "message": "1 replica unavailable" } }, { "kind": "Service", "name": "cart-api", "status": "Synced", "health": { "status": "Healthy" } }]
            }
        }))
        .unwrap();
        let a = app(&o);
        assert_eq!((a.env.as_deref(), a.app.as_deref(), a.namespace.as_deref()), (Some("staging"), Some("cart-api"), Some("apps")));
        assert_eq!((a.sync.as_str(), a.health.as_str(), a.operation.as_deref()), ("OutOfSync", "Degraded", Some("Failed")));
        assert_eq!(a.revision.as_deref(), Some("01234567"));
        assert_eq!(a.message.as_deref(), Some("Deployment cart-api: forbidden"));
        assert_eq!((a.images.clone(), a.deployments.clone()), (vec!["0.2.147".to_string()], vec!["cart-api".to_string()]));
        assert!(a.automated && a.started_at.is_some());
        let failed: Vec<(&str, Option<&str>, Option<&str>)> = a.sync_resources.iter().map(|r| (r.name.as_str(), r.status.as_deref(), r.hook.as_deref())).collect();
        assert_eq!(failed, [("cart-api", Some("SyncFailed"), None), ("migrate", Some("Synced"), Some("Failed")), ("cart-api", Some("Synced"), None)], "what did not sync first");
        assert_eq!((a.drifting.len(), a.drifting[0].health.as_deref(), a.drifting[0].message.as_deref()), (1, Some("Degraded"), Some("1 replica unavailable")));
        assert_eq!(a.history.iter().map(|h| h.revision.as_deref().unwrap_or_default()).collect::<Vec<_>>(), ["bbbbbbbb", "aaaaaaaa"], "most recent first");
        // Neither cluster address nor repository in what is kept.
        let kept = serde_json::to_string(&app(&o)).unwrap();
        assert!(!kept.contains("10.0.0.1") && !kept.contains("git@"));
    }
}
