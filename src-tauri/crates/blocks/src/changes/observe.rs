//! The transitions of one read: what changed since the previous one (`Last`), with the date the
//! source gives when it gives one. Pure: the collector reads, this compares.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration as Span, Utc};
use serde::{Deserialize, Serialize};

use super::{ChangeEvent, ChangeKind, ChangeLink, ChangeMr, ChangeSeverity, ChangesConfig, WindowAlert};
use crate::state::{ArgoApp, EnvStatus, Snapshot, Vulnerabilities};

/// What a read needs of the snapshot.
pub struct View {
    /// Argo CD's applications with their instance's cluster (a customer's left out).
    pub argo: Vec<(String, ArgoApp)>,
    pub envs: Vec<EnvStatus>,
    pub mrs: Vec<Mr>,
    pub vulns: Option<Vulnerabilities>,
}

#[derive(Clone)]
pub struct Mr {
    pub repo: Option<String>,
    pub iid: u64,
    pub title: String,
    pub url: String,
    pub target_branch: String,
    pub merged_at: Option<DateTime<Utc>>,
}

impl View {
    pub fn of(s: &Snapshot) -> Self {
        View {
            argo: s.argocd.iter().flat_map(|a| a.instances.iter()).flat_map(|i| i.apps.iter().filter(|a| !a.customer).map(|a| (i.cluster.clone(), a.clone()))).collect(),
            envs: s.environments.as_ref().map(|e| e.envs.clone()).unwrap_or_default(),
            mrs: s
                .merge_requests
                .iter()
                .filter(|m| m.merged_at.is_some())
                .map(|m| Mr { repo: m.repo.clone(), iid: m.iid, title: m.title.clone(), url: m.url.clone(), target_branch: m.target_branch.clone(), merged_at: m.merged_at })
                .collect(),
            vulns: s.vulnerabilities.clone(),
        }
    }

    /// The environments, in the order of `[[environments]]`, then those only Argo CD names.
    pub fn env_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for e in self.envs.iter().map(|e| e.env.clone()).chain(self.argo.iter().filter_map(|(_, a)| a.env.clone())) {
            if !out.contains(&e) {
                out.push(e);
            }
        }
        out
    }
}

/// What the previous read saw, kept between two launches.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Last {
    /// By `<cluster>/<namespace>/<name>`.
    #[serde(default)]
    pub argo: BTreeMap<String, ArgoLast>,
    /// By `<env>/<namespace>/<name>`.
    #[serde(default)]
    pub workloads: BTreeMap<String, WorkloadLast>,
    /// The window's alerts by key; absent until it sent them once.
    #[serde(default)]
    pub alerts: Option<BTreeMap<String, WindowAlert>>,
    /// Of those, the ones the window no longer sends, since when: over only after `ALERT_GRACE`
    /// (a source unreadable for a read, the window's first lists before the sources answer).
    #[serde(default)]
    pub missing: BTreeMap<String, DateTime<Utc>>,
    /// The findings to fix (`<id>|<project>`) of the last scan seen; absent before the first.
    #[serde(default)]
    pub vulns: Option<BTreeSet<String>>,
    #[serde(default)]
    pub vuln_scan: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ArgoLast {
    pub health: String,
    pub sync: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkloadLast {
    pub restarts: u32,
    pub waiting: Option<String>,
    pub last_exit_at: Option<DateTime<Utc>>,
}

/// The changes of this read, `since` the retention's start; `last` becomes this read.
pub fn observe(cfg: &ChangesConfig, view: &View, alerts: Option<&[WindowAlert]>, last: &mut Last, now: DateTime<Utc>, since: DateTime<Utc>) -> Vec<ChangeEvent> {
    let mut out = Vec::new();
    if cfg.reads("argocd") {
        argo(view, last, now, since, &mut out);
    }
    if cfg.reads("pods") {
        pods(view, last, now, since, &mut out);
    }
    if cfg.reads("mrs") {
        merged(cfg, view, since, &mut out);
    }
    if cfg.reads("alerts") && let Some(alerts) = alerts {
        window_alerts(alerts, last, now, &mut out);
    }
    if cfg.reads("vulnerabilities") {
        vulnerabilities(cfg, view, last, now, &mut out);
    }
    out
}

fn ts(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// An image as `app:tag`: Argo CD gives the tags alone.
fn image(app: &str, tag: &str) -> String {
    if tag.contains(':') { tag.rsplit('/').next().unwrap_or(tag).to_string() } else { format!("{app}:{tag}") }
}

/* ---------- Argo CD ---------- */

fn argo(view: &View, last: &mut Last, now: DateTime<Utc>, since: DateTime<Utc>, out: &mut Vec<ChangeEvent>) {
    let mut seen = BTreeMap::new();
    for (cluster, a) in &view.argo {
        let key = format!("{cluster}/{}/{}", a.namespace.as_deref().unwrap_or_default(), a.name);
        let app = a.app.clone().unwrap_or_else(|| a.name.clone());
        let base = |id: String, at: DateTime<Utc>, kind: ChangeKind, severity: ChangeSeverity| {
            let mut e = ChangeEvent::new(id, at, "argocd", kind, severity, a.name.clone());
            e.env = a.env.clone();
            e.app = Some(app.clone());
            e.namespace = a.namespace.clone();
            e.workloads = a.deployments.clone();
            e.link = Some(ChangeLink::Argo { cluster: cluster.clone(), app: key.clone() });
            e
        };
        // The deployments Argo CD keeps (its history), each at its own date.
        for (i, d) in a.history.iter().enumerate() {
            let Some(at) = d.deployed_at.filter(|t| *t >= since) else { continue };
            let mut e = base(format!("sync:{key}:{}", ts(at)), at, ChangeKind::Sync, ChangeSeverity::Info);
            e.revision = d.revision.clone();
            // The newest is what runs: its images, and who started it.
            if i == 0 && (d.revision.is_none() || d.revision == a.revision) {
                e.images = a.images.iter().map(|t| image(&app, t)).collect();
                e.detail = Some(if a.automated { "auto" } else { "manual" }.into());
            }
            out.push(e);
        }
        if let (Some(op), Some(at)) = (a.operation.as_deref(), a.started_at) && matches!(op, "Failed" | "Error") && at >= since {
            let mut e = base(format!("sync_failed:{key}:{}", ts(at)), at, ChangeKind::SyncFailed, ChangeSeverity::Ko);
            e.revision = a.revision.clone();
            e.detail = a.message.clone();
            out.push(e);
        }
        let now_state = ArgoLast { health: a.health.clone(), sync: a.sync.clone() };
        if let Some(before) = last.argo.get(&key) {
            if before.health != a.health && !a.health.is_empty() {
                let severity = match a.health.as_str() {
                    "Degraded" | "Missing" => ChangeSeverity::Ko,
                    "Healthy" => ChangeSeverity::Ok,
                    _ => ChangeSeverity::Info,
                };
                let mut e = base(format!("health:{key}:{}:{}", a.health, ts(now)), now, ChangeKind::Health, severity);
                e.seen = true;
                e.detail = Some(format!("{} → {}", before.health, a.health));
                out.push(e);
            }
            if before.sync == "Synced" && a.sync == "OutOfSync" {
                let mut e = base(format!("drift:{key}:{}", ts(now)), now, ChangeKind::Drift, ChangeSeverity::Warn);
                e.seen = true;
                e.detail = Some(a.drifting.iter().take(3).map(|r| format!("{} {}", r.kind, r.name)).collect::<Vec<_>>().join(", ")).filter(|d| !d.is_empty());
                out.push(e);
            }
        }
        seen.insert(key, now_state);
    }
    // An application gone from a read (its instance unreadable) keeps what was seen of it.
    last.argo.extend(seen);
}

/* ---------- Pods ---------- */

fn pods(view: &View, last: &mut Last, now: DateTime<Utc>, since: DateTime<Utc>, out: &mut Vec<ChangeEvent>) {
    for s in view.envs.iter().filter(|s| s.error.is_none()) {
        for w in &s.workloads {
            let key = format!("{}/{}/{}", s.env, s.namespace, w.name);
            let base = |id: String, at: DateTime<Utc>, kind: ChangeKind, severity: ChangeSeverity| {
                let mut e = ChangeEvent::new(id, at, "pods", kind, severity, w.name.clone());
                e.env = Some(s.env.clone());
                e.app = Some(w.app.clone().unwrap_or_else(|| w.name.clone()));
                e.namespace = Some(s.namespace.clone());
                e.workloads = vec![w.name.clone()];
                e.link = Some(ChangeLink::Pod { context: s.context.clone(), namespace: s.namespace.clone(), deployment: w.name.clone() });
                e
            };
            let before = last.workloads.get(&key).cloned();
            // A rollout (a new ReplicaSet) at its own date: also seen after Prism was closed.
            if let (Some(rev), Some(at)) = (w.revision, w.updated_at.filter(|t| *t >= since)) {
                // Revision 1 is the Deployment's creation, not a rollout.
                if rev > 1 {
                    let mut e = base(format!("rollout:{key}:{rev}"), at, ChangeKind::Rollout, ChangeSeverity::Info);
                    e.images = w.image_tag.iter().map(|t| image(&w.name, t)).collect();
                    e.detail = Some(format!("rev {rev}"));
                    out.push(e);
                }
            }
            // OOMKilled: the termination's own date.
            let mut crashed = false;
            if let (Some("OOMKilled"), Some(at)) = (w.last_exit.as_deref(), w.last_exit_at.filter(|t| *t >= since)) {
                let mut e = base(format!("oom:{key}:{}", ts(at)), at, ChangeKind::OomKilled, ChangeSeverity::Ko);
                e.detail = Some(format!("{} restarts", w.restarts));
                out.push(e);
                crashed = before.as_ref().is_none_or(|b| b.last_exit_at != Some(at));
            }
            let waiting = w.waiting.clone();
            if waiting.as_deref() != before.as_ref().and_then(|b| b.waiting.as_deref()) && let Some(reason) = &waiting {
                let crash = reason == "CrashLoopBackOff";
                // The last termination, if it is recent: when it began to crash.
                let at = w.last_exit_at.filter(|t| crash && *t >= now - Span::minutes(15) && *t <= now).unwrap_or(now);
                let kind = if crash { ChangeKind::CrashLoop } else { ChangeKind::Waiting };
                let mut e = base(format!("{}:{key}:{}", if crash { "crash" } else { "waiting" }, ts(at)), at, kind, if crash { ChangeSeverity::Ko } else { ChangeSeverity::Warn });
                e.seen = at == now;
                e.detail = Some(match (&w.last_exit, crash) {
                    (Some(x), true) => format!("{reason} · {x}"),
                    _ => reason.clone(),
                });
                crashed |= crash;
                out.push(e);
            }
            // Restarts during a CrashLoop are the CrashLoop's: its line says it.
            let looping = w.waiting.as_deref() == Some("CrashLoopBackOff");
            if let Some(b) = &before && w.restarts > b.restarts && !crashed && !looping {
                let at = w.last_exit_at.filter(|t| b.last_exit_at.is_none_or(|old| *t > old) && *t <= now).unwrap_or(now);
                let mut e = base(format!("restart:{key}:{}:{}", w.restarts, ts(at)), at, ChangeKind::Restart, ChangeSeverity::Warn);
                e.seen = at == now;
                e.detail = Some(format!("+{}{}", w.restarts - b.restarts, w.last_exit.as_deref().map(|x| format!(" · {x}")).unwrap_or_default()));
                out.push(e);
            }
            last.workloads.insert(key, WorkloadLast { restarts: w.restarts, waiting, last_exit_at: w.last_exit_at });
        }
    }
}

/* ---------- Merge requests ---------- */

/// The environments a branch feeds (`[changes] branches`); `[None]`: none named, every one.
pub fn envs_of_branch(cfg: &ChangesConfig, branch: &str) -> Vec<Option<String>> {
    if cfg.branches.is_empty() {
        return vec![None];
    }
    let mut out: Vec<Option<String>> = Vec::new();
    for b in cfg.branches.iter().filter(|b| prism_core::config::glob(b.branch.trim(), branch)) {
        if !out.contains(&Some(b.env.clone())) {
            out.push(Some(b.env.clone()));
        }
    }
    out
}

fn merged(cfg: &ChangesConfig, view: &View, since: DateTime<Utc>, out: &mut Vec<ChangeEvent>) {
    for m in &view.mrs {
        let Some(at) = m.merged_at.filter(|t| *t >= since) else { continue };
        for env in envs_of_branch(cfg, &m.target_branch) {
            let id = format!("mr:{}:{}:{}", m.repo.as_deref().unwrap_or_default(), m.iid, env.as_deref().unwrap_or("*"));
            let mut e = ChangeEvent::new(id, at, "mrs", ChangeKind::Merged, ChangeSeverity::Info, m.title.clone());
            e.env = env;
            e.app = m.repo.clone();
            e.detail = Some(m.target_branch.clone());
            e.link = Some(ChangeLink::Mr { repo: m.repo.clone(), iid: m.iid });
            e.mr = Some(ChangeMr { repo: m.repo.clone(), iid: m.iid, title: m.title.clone(), url: Some(m.url.clone()) });
            out.push(e);
        }
    }
}

/* ---------- Alerts ---------- */

/// An alert the window stops sending is over once it has been gone this long.
const ALERT_GRACE: i64 = 3;

fn window_alerts(alerts: &[WindowAlert], last: &mut Last, now: DateTime<Utc>, out: &mut Vec<ChangeEvent>) {
    let current: BTreeMap<String, WindowAlert> = alerts.iter().map(|a| (a.key.clone(), a.clone())).collect();
    let Some(on) = last.alerts.as_mut() else {
        // The first sight is only noted.
        last.alerts = Some(current);
        return;
    };
    let event = |a: &WindowAlert, start: bool, at: DateTime<Utc>| {
        let severity = match (start, a.severity.as_str()) {
            (false, _) => ChangeSeverity::Ok,
            (true, "ko") => ChangeSeverity::Ko,
            (true, "warn") => ChangeSeverity::Warn,
            _ => ChangeSeverity::Info,
        };
        let mut e = ChangeEvent::new(format!("alert:{}:{}:{}", if start { "on" } else { "off" }, a.key, ts(at)), at, "alerts", if start { ChangeKind::AlertOn } else { ChangeKind::AlertOff }, severity, a.text.clone());
        e.seen = true;
        e.env = a.env.clone();
        e.app = a.app.clone();
        e.link = a.url.clone().map(|url| ChangeLink::Url { url });
        e
    };
    for a in current.values() {
        last.missing.remove(&a.key);
        if !on.contains_key(&a.key) {
            out.push(event(a, true, now));
        }
        on.insert(a.key.clone(), a.clone());
    }
    let gone: Vec<String> = on.keys().filter(|k| !current.contains_key(*k)).cloned().collect();
    for key in gone {
        let since = *last.missing.entry(key.clone()).or_insert(now);
        if now - since >= Span::minutes(ALERT_GRACE) {
            if let Some(a) = on.remove(&key) {
                out.push(event(&a, false, since));
            }
            last.missing.remove(&key);
        }
    }
}

/* ---------- Vulnerabilities ---------- */

fn vulnerabilities(cfg: &ChangesConfig, view: &View, last: &mut Last, now: DateTime<Utc>, out: &mut Vec<ChangeEvent>) {
    let Some(v) = view.vulns.as_ref().filter(|v| !v.scanning) else { return };
    if v.scanned_at.is_some() && v.scanned_at == last.vuln_scan {
        return;
    }
    let at = v.scanned_at.unwrap_or(now);
    let env_names = view.env_names();
    // A project's environment: its image's (`cart-api · prod`, `prod/cart-api:1.2`), otherwise
    // the scanned branch's.
    let env_of = |project: &str| -> Vec<Option<String>> {
        let p = v.projects.iter().find(|p| p.project == project);
        let named = project.rsplit_once(" · ").map(|(_, e)| e.to_string()).or_else(|| p.and_then(|p| p.image.as_deref()).and_then(|i| i.split_once('/')).map(|(e, _)| e.to_string()));
        match named.filter(|e| env_names.contains(e)) {
            Some(e) => vec![Some(e)],
            // A lockfile's: the environments its branch feeds, otherwise every one.
            None => Some(envs_of_branch(cfg, &v.branch)).filter(|e| !e.is_empty()).unwrap_or_else(|| vec![None]),
        }
    };
    let mut current = BTreeSet::new();
    let mut by_key = BTreeMap::new();
    for f in v.findings.iter().filter(|f| !f.accepted) {
        for p in &f.projects {
            let key = format!("{}|{p}", f.id);
            by_key.insert(key.clone(), (f, p.clone()));
            current.insert(key);
        }
    }
    if let Some(before) = &last.vulns {
        let event = |key: &str, new: bool, title: String, severity: ChangeSeverity, project: &str, detail: Option<String>| {
            env_of(project).into_iter().map(|env| {
                let mut e = ChangeEvent::new(format!("vuln:{}:{key}:{}:{}", if new { "new" } else { "fixed" }, env.as_deref().unwrap_or("*"), ts(at)), at, "vulnerabilities", if new { ChangeKind::VulnNew } else { ChangeKind::VulnFixed }, severity, title.clone());
                e.seen = true;
                e.env = env;
                e.app = Some(project.split(" · ").next().unwrap_or(project).to_string());
                e.detail = detail.clone();
                e
            }).collect::<Vec<_>>()
        };
        for key in current.difference(before) {
            let (f, p) = &by_key[key];
            let severity = if f.severity == "critical" { ChangeSeverity::Ko } else { ChangeSeverity::Warn };
            let mut evs = event(key, true, format!("{} · {} {}", f.id, f.package, f.installed), severity, p, Some(f.title.clone()));
            for e in &mut evs {
                e.link = f.url.clone().map(|url| ChangeLink::Url { url });
            }
            out.extend(evs);
        }
        for key in before.difference(&current) {
            let (id, project) = key.split_once('|').unwrap_or((key, ""));
            out.extend(event(key, false, id.to_string(), ChangeSeverity::Ok, project, None));
        }
    }
    last.vulns = Some(current);
    last.vuln_scan = v.scanned_at;
}

/* ---------- Correlation ---------- */

/// Links each crash (a CrashLoop, an OOMKilled, a restart, an application turned Degraded) to the
/// newest sync or rollout of the same application in the `minutes` before it.
pub fn correlate(events: &mut [ChangeEvent], minutes: u32) {
    let window = Span::minutes(minutes.into());
    let deploys: Vec<(usize, DateTime<Utc>)> = events.iter().enumerate().filter(|(_, e)| e.kind.deploys()).map(|(i, e)| (i, e.at)).collect();
    for i in 0..events.len() {
        let e = &events[i];
        let crash = e.kind.is_crash() || (e.kind == ChangeKind::Health && e.severity == ChangeSeverity::Ko);
        if !crash || e.related.is_some() {
            continue;
        }
        let found = deploys
            .iter()
            .filter(|(j, at)| *at <= e.at && e.at - *at <= window && events[*j].same_app(e))
            .max_by_key(|(_, at)| *at)
            .map(|(j, _)| events[*j].id.clone());
        events[i].related = found;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ArgoDeploy, Workload, WorkloadLoad};

    fn cfg() -> ChangesConfig {
        crate::section(&toml::Table::new(), "changes").unwrap()
    }

    fn at(minutes_ago: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-05T10:00:00Z").unwrap().with_timezone(&Utc) - Span::minutes(minutes_ago)
    }

    fn workload(restarts: u32, waiting: Option<&str>, exit: Option<(&str, i64)>, revision: u32, updated: i64) -> Workload {
        Workload {
            name: "shop-api".into(),
            app: Some("shop".into()),
            role: "api".into(),
            desired: 2,
            ready: 2,
            pods: 2,
            restarts,
            waiting: waiting.map(String::from),
            last_exit: exit.map(|(x, _)| x.into()),
            last_exit_at: exit.map(|(_, m)| at(m)),
            image_tag: Some("2.15.0".into()),
            expected: None,
            drift: false,
            load: WorkloadLoad::default(),
            updated_at: Some(at(updated)),
            revision: Some(revision),
        }
    }

    fn env(w: Workload) -> EnvStatus {
        EnvStatus { env: "staging".into(), context: "ctx".into(), cluster: "c".into(), namespace: "shop".into(), error: None, workloads: vec![w] }
    }

    fn argo_app(health: &str, deployed: i64) -> ArgoApp {
        ArgoApp {
            name: "shop-staging".into(),
            env: Some("staging".into()),
            group: None,
            app: Some("shop".into()),
            customer: false,
            namespace: Some("shop".into()),
            sync: "Synced".into(),
            health: health.into(),
            revision: Some("8c1d2e3".into()),
            operation: Some("Succeeded".into()),
            message: None,
            synced_at: Some(at(deployed)),
            images: vec!["2.15.0".into()],
            deployments: vec!["shop-api".into()],
            started_at: Some(at(deployed)),
            automated: true,
            sync_resources: vec![],
            drifting: vec![],
            history: vec![ArgoDeploy { revision: Some("8c1d2e3".into()), deployed_at: Some(at(deployed)) }, ArgoDeploy { revision: Some("51b0a7c".into()), deployed_at: Some(at(60 * 24 * 30)) }],
        }
    }

    fn view(argo: Vec<ArgoApp>, envs: Vec<EnvStatus>) -> View {
        View { argo: argo.into_iter().map(|a| ("ops".to_string(), a)).collect(), envs, mrs: vec![], vulns: None }
    }

    #[test]
    fn a_sync_then_a_crash_are_linked() {
        let since = at(60 * 24 * 14);
        let mut last = Last::default();
        // First read: the sync from Argo's history (the older one out of retention), the rollout.
        let first = observe(&cfg(), &view(vec![argo_app("Healthy", 20)], vec![env(workload(0, None, None, 7, 19))]), None, &mut last, at(5), since);
        let kinds: Vec<_> = first.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [ChangeKind::Sync, ChangeKind::Rollout]);
        assert_eq!(first[0].images, ["shop:2.15.0"]);
        assert_eq!(first[0].revision.as_deref(), Some("8c1d2e3"));
        assert!(!first[0].seen);
        // Then it crashes: CrashLoop at its last exit's time, Degraded seen now.
        let mut second = observe(&cfg(), &view(vec![argo_app("Degraded", 20)], vec![env(workload(3, Some("CrashLoopBackOff"), Some(("Error", 2)), 7, 19))]), None, &mut last, at(0), since);
        let kinds: Vec<_> = second.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&ChangeKind::Health) && kinds.contains(&ChangeKind::CrashLoop));
        // A restart counted within the CrashLoop is not one more line.
        assert!(!kinds.contains(&ChangeKind::Restart));
        let mut all = first.clone();
        all.append(&mut second);
        all.sort_by_key(|e| e.at);
        correlate(&mut all, 30);
        let crash = all.iter().find(|e| e.kind == ChangeKind::CrashLoop).unwrap();
        assert_eq!(crash.at, at(2));
        // The newest deploy before it: the rollout (19 min before), not the sync (20).
        assert_eq!(crash.related.as_deref(), Some("rollout:staging/shop/shop-api:7"));
        // Too far apart: nothing.
        let mut far = all.clone();
        far.iter_mut().for_each(|e| e.related = None);
        correlate(&mut far, 10);
        assert!(far.iter().all(|e| e.related.is_none()));
    }

    #[test]
    fn restarts_and_oom() {
        let since = at(60 * 24);
        let mut last = Last::default();
        observe(&cfg(), &view(vec![], vec![env(workload(1, None, None, 1, 300))]), None, &mut last, at(10), since);
        let restart = observe(&cfg(), &view(vec![], vec![env(workload(3, None, Some(("Error", 4)), 1, 300))]), None, &mut last, at(0), since);
        assert_eq!(restart.len(), 1);
        assert_eq!((restart[0].kind, restart[0].detail.as_deref(), restart[0].at), (ChangeKind::Restart, Some("+2 · Error"), at(4)));
        let oom = observe(&cfg(), &view(vec![], vec![env(workload(4, None, Some(("OOMKilled", 1)), 1, 300))]), None, &mut last, at(0), since);
        assert_eq!(oom.iter().map(|e| e.kind).collect::<Vec<_>>(), [ChangeKind::OomKilled]);
        // Seen again: the same id, which the journal keeps once.
        let again = observe(&cfg(), &view(vec![], vec![env(workload(4, None, Some(("OOMKilled", 1)), 1, 300))]), None, &mut last, at(0), since);
        assert_eq!(again[0].id, oom[0].id);
    }

    #[test]
    fn alerts_appear_and_go() {
        let alert = |key: &str| WindowAlert { key: key.into(), severity: "ko".into(), text: key.into(), env: Some("staging".into()), app: None, url: None };
        let mut last = Last::default();
        let now = at(10);
        // The first sight is only noted.
        assert!(observe(&cfg(), &view(vec![], vec![]), Some(&[alert("a")]), &mut last, now, now).is_empty());
        let out = observe(&cfg(), &view(vec![], vec![]), Some(&[alert("b")]), &mut last, now, now);
        assert_eq!(out.iter().map(|e| (e.kind, e.title.as_str())).collect::<Vec<_>>(), [(ChangeKind::AlertOn, "b")]);
        // Gone for a moment (a source unreadable), back: nothing.
        assert!(observe(&cfg(), &view(vec![], vec![]), Some(&[]), &mut last, at(9), now).is_empty());
        assert!(observe(&cfg(), &view(vec![], vec![]), Some(&[alert("a"), alert("b")]), &mut last, at(8), now).is_empty());
        // Gone for good: over, at when it went.
        assert!(observe(&cfg(), &view(vec![], vec![]), Some(&[alert("b")]), &mut last, at(7), now).is_empty());
        let off = observe(&cfg(), &view(vec![], vec![]), Some(&[alert("b")]), &mut last, at(2), now);
        assert_eq!(off.iter().map(|e| (e.kind, e.title.as_str(), e.at)).collect::<Vec<_>>(), [(ChangeKind::AlertOff, "a", at(7))]);
        assert!(off.iter().all(|e| e.seen));
    }

    #[test]
    fn a_long_crash_loop_is_one_line() {
        let since = at(60 * 24);
        let mut last = Last::default();
        observe(&cfg(), &view(vec![], vec![env(workload(1, None, None, 1, 300))]), None, &mut last, at(10), since);
        let first = observe(&cfg(), &view(vec![], vec![env(workload(3, Some("CrashLoopBackOff"), Some(("Error", 6)), 1, 300))]), None, &mut last, at(5), since);
        assert_eq!(first.iter().map(|e| e.kind).collect::<Vec<_>>(), [ChangeKind::CrashLoop]);
        let later = observe(&cfg(), &view(vec![], vec![env(workload(6, Some("CrashLoopBackOff"), Some(("Error", 1)), 1, 300))]), None, &mut last, at(0), since);
        assert!(later.is_empty());
    }

    #[test]
    fn branches_name_the_environments() {
        let mut c = cfg();
        assert_eq!(envs_of_branch(&c, "main"), [None]);
        c.branches = vec![super::super::EnvBranch { env: "staging".into(), branch: "main".into() }, super::super::EnvBranch { env: "prod".into(), branch: "release/*".into() }];
        assert_eq!(envs_of_branch(&c, "release/2.15"), [Some("prod".to_string())]);
        assert!(envs_of_branch(&c, "feature/x").is_empty());
    }
}
