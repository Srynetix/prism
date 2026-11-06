//! The clusters: pods, environments, Argo CD.

use serde::{Deserialize, Serialize};

use super::*;

/// `[argocd_layout]`: an Application's environment is the first segment of its values path (after
/// `root`) naming a stage's environment (`stages`); its component, the segment after
/// `apps_segment`; its group, the segment before the environment. A group of `customer_groups`
/// (`customers/<customer>/…`) is a customer's instance: production, outside Environments.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct ArgoLayoutConfig {
    /// Namespaces discovered on the default context's cluster when no `[[argocd]]` is declared.
    pub namespace_prefix: String,
    /// The path segment where the layout starts (`argocd`); empty: the whole path.
    pub root: String,
    pub apps_segment: String,
    pub customer_groups: Vec<String>,
}

impl Default for ArgoLayoutConfig {
    fn default() -> Self {
        Self { namespace_prefix: "argocd".into(), root: String::new(), apps_segment: "apps".into(), customer_groups: Vec::new() }
    }
}

/// An ArgoCD instance: its namespace, the kube context of the cluster hosting it (the
/// the default context by default) and the URL of its UI, for links.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ArgoConfig {
    pub namespace: String,
    pub context: Option<String>,
    pub url: Option<String>,
    /// Environments the instance is authoritative for (`prod`, `preprod`, `staging`); absent: all those
    /// of its Applications. A replaced instance (new infra): `envs = []`.
    #[serde(default)]
    pub envs: Option<Vec<String>>,
}

/// An environment in a cluster: its name (`prod`, `preprod`, `staging`, or any other such as `ops`),
/// the kube context and the namespaces where its applications run.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EnvironmentConfig {
    pub env: String,
    pub context: String,
    /// The namespaces, `*` matching any characters (`*`: every namespace, `app-*`). Empty: none.
    pub namespaces: Vec<String>,
}

impl EnvironmentConfig {
    /// Does it read `namespace`?
    pub fn reads(&self, namespace: &str) -> bool {
        self.namespaces.iter().any(|p| glob(p.trim(), namespace))
    }

    /// Does a pattern ask for the cluster's namespaces to be listed?
    pub fn has_patterns(&self) -> bool {
        self.namespaces.iter().any(|n| n.contains('*'))
    }

    /// The namespaces it reads among the cluster's `all`, in config order: a named one as is, a
    /// pattern's matches in `all`'s order, each once.
    pub fn namespaces_in(&self, all: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in self.namespaces.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
            let matches: Vec<&str> = if p.contains('*') { all.iter().map(String::as_str).filter(|n| glob(p, n)).collect() } else { vec![p] };
            for n in matches {
                if !out.iter().any(|o| o == n) {
                    out.push(n.to_string());
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClustersConfig {
    /// Access to the Kubernetes clusters. Off (profile without rights on the clusters), neither pods,
    /// nor environments, nor ArgoCD are read: no access error in Alerts.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// The context a `[[pods]]` target or ArgoCD's discovery uses when it names none.
    #[serde(default)]
    pub default_context: String,
    /// What declares the stages (their names and environments) Environments, ArgoCD and Settings
    /// read: a block declaring them (`Block::stages`, such as a team's `shipping`); `none` (default):
    /// `[[environments]]`' names.
    #[serde(default = "default_stages")]
    pub stages: String,
    /// A shell in a pod (`kubectl exec`) from its logs pane, with the rights of the kubeconfig.
    #[serde(default = "default_true")]
    pub shell: bool,
    /// A pod's CPU and memory over time, in its logs pane: Prism keeps the `[[pods]]` targets'
    /// metrics-server readings (6 h) for it.
    #[serde(default = "default_true")]
    pub usage_history: bool,
    /// For a GKE context (`gke_<project>_<location>_<cluster>`), that history is read from Google
    /// Managed Prometheus instead (cAdvisor's metrics, any pod, days back), with gcloud's token.
    #[serde(default = "default_true")]
    pub gcp_metrics: bool,
}

fn default_stages() -> String {
    "none".into()
}

impl Default for ClustersConfig {
    fn default() -> Self {
        Self { enabled: true, default_context: String::new(), stages: default_stages(), shell: true, usage_history: true, gcp_metrics: true }
    }
}

/// Pods followed (`[[pods]]`): a cluster's namespaces, all of them or those matching a pattern.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PodsTarget {
    /// Its name in the Pods block. Absent: its context's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The kube context. Absent: `[clusters] default_context`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub context: String,
    /// The namespaces, `*` matching any characters (`app-*`). Empty: every namespace.
    #[serde(default)]
    pub namespaces: Vec<String>,
}

impl PodsTarget {
    /// Its context, or the clusters' default one.
    pub fn context_or<'a>(&'a self, clusters: &'a ClustersConfig) -> &'a str {
        if self.context.trim().is_empty() { clusters.default_context.trim() } else { self.context.trim() }
    }

    /// Its name in Prism: its own, or its context's.
    pub fn label(&self, clusters: &ClustersConfig) -> String {
        if self.name.trim().is_empty() { self.context_or(clusters).to_string() } else { self.name.trim().to_string() }
    }

    /// Does it follow `namespace`?
    pub fn follows(&self, namespace: &str) -> bool {
        self.namespaces.is_empty() || self.namespaces.iter().any(|p| glob(p.trim(), namespace))
    }

    /// The namespaces named exactly (no `*`): each can be watched on its own. None when a
    /// pattern (or no namespace) asks for the whole cluster.
    pub fn exact_namespaces(&self) -> Option<Vec<String>> {
        let names: Vec<String> = self.namespaces.iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
        (!names.is_empty() && names.iter().all(|n| !n.contains('*'))).then_some(names)
    }
}

/// `pattern` matches the whole of `text`, `*` standing for any characters.
fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !text.starts_with(first) || !text[first.len()..].ends_with(last) || text.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}
