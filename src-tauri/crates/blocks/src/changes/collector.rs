//! The Changes block's collector: every 30 s it compares what Prism reads with the previous read
//! (`observe`), and appends the changes to the journal (`changes.jsonl` in the profile's data
//! folder, one event a line, rewritten without those past `retention_days` at each launch). Beside
//! it, `changes-last.json` keeps the previous read and when Prism was reading, to tell the ranges
//! it was closed.
//!
//! - the merge request of a sync's revision: the commit read in a local clone (`!123` of a GitLab
//!   merge commit, `(#123)` of a GitHub squash), once per revision;
//! - the Kubernetes events (`[changes] kube_events`): `kubectl get events` of each environment's
//!   namespace, its warnings;
//! - the blocks of a pack: `Block::changes` of their state.

use std::{
    collections::{BTreeMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration as Span, Utc};
use prism_base::{tr, i18n::tr};
use serde::{Deserialize, Serialize};

use super::observe::{self, Last, View};
use super::{ChangeEvent, ChangeKind, ChangeLink, ChangeMr, ChangeSeverity, ChangesConfig, ChangesState, Closed, WindowAlert};
use crate::{git::run as git, state::Snapshot, BlockCtx};

static REFRESH: OnceLock<tokio::sync::Notify> = OnceLock::new();
/// The window's alerts, as it last sent them.
static ALERTS: Mutex<Option<Vec<WindowAlert>>> = Mutex::new(None);

/// Every read, and at most this long without one before Prism counts as closed.
const EVERY: Duration = Duration::from_secs(30);
const CLOSED_AFTER: i64 = 5;
/// What the window receives at most (the newest).
const SHOWN: usize = 4000;

/// Reads the sources again now (the window's ↻).
pub fn refresh() {
    if let Some(n) = REFRESH.get() {
        n.notify_one();
    }
}

/// The window's alerts changed.
pub fn alerts(alerts: Vec<WindowAlert>) {
    let changed = {
        let mut held = ALERTS.lock().unwrap_or_else(|e| e.into_inner());
        let changed = held.as_ref() != Some(&alerts);
        *held = Some(alerts);
        changed
    };
    if changed {
        refresh();
    }
}

/// What the collector keeps beside the journal.
#[derive(Default, Serialize, Deserialize)]
struct Kept {
    #[serde(default)]
    last: Last,
    /// When Prism was reading, oldest first: between two, it was closed.
    #[serde(default)]
    sessions: Vec<Closed>,
    /// The merge request of each revision found.
    #[serde(default)]
    revisions: BTreeMap<String, ChangeMr>,
}

pub async fn run(cfg: ChangesConfig, ctx: BlockCtx) {
    let notify = REFRESH.get_or_init(tokio::sync::Notify::new);
    let dir = ctx.data_dir();
    let journal_path = dir.join("changes.jsonl");
    let kept_path = dir.join("changes-last.json");
    let now = Utc::now();
    let since = || Utc::now() - Span::days(cfg.retention_days.max(1).into());
    let mut events = load_journal(&journal_path, since());
    // A crash whose deploy came in a later read: linked now, written with it below.
    observe::correlate(&mut events, cfg.correlate_minutes);
    // Rotation: the journal rewritten without what is past the retention.
    if let Err(e) = rewrite(&journal_path, &events) {
        tracing::warn!("changes: {e:#}");
    }
    let mut kept: Kept = std::fs::read(&kept_path).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
    kept.sessions.retain(|s| s.to >= since());
    kept.sessions.push(Closed { from: now, to: now });
    let mut known: HashSet<String> = events.iter().map(|e| e.id.clone()).collect();
    // Revisions whose MR was not found, when: asked again later (the clone may fetch it).
    let mut misses: BTreeMap<String, DateTime<Utc>> = BTreeMap::new();
    let mut envs = Vec::new();
    publish(&ctx, &cfg, &events, &kept, &envs);
    // The first read waits for the other sources (Argo CD, the clusters, the forge).
    tokio::time::sleep(Duration::from_secs(15)).await;
    loop {
        let now = Utc::now();
        let view = ctx.snapshot(View::of);
        let blocks = ctx.snapshot(|s: &Snapshot| s.blocks.clone());
        let clones = ctx.snapshot(|s: &Snapshot| s.repos.iter().filter(|r| r.error.is_none() && !r.path.is_empty()).map(|r| (r.name.clone(), r.path.clone())).collect::<Vec<_>>());
        envs = view.env_names();
        let alerts = ALERTS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut fresh = observe::observe(&cfg, &view, alerts.as_deref(), &mut kept.last, now, since());
        let mut errors = Vec::new();
        if cfg.kube_events {
            let (read, failed) = kube_events(&view, since()).await;
            fresh.extend(read);
            errors.extend(failed.into_iter().map(|e| format!("kube events: {e}")));
        }
        if cfg.reads("packs") || prism_core::blocks::all().iter().any(|b| cfg.reads(b.id())) {
            for b in prism_core::blocks::all().iter().filter(|b| b.id() != "changes" && (cfg.reads("packs") || cfg.reads(b.id()))) {
                if let Some(state) = blocks.get(b.id()) {
                    fresh.extend(b.changes(state, now).into_iter().filter(|e| e.at >= since()));
                }
            }
        }
        fresh.retain(|e| !known.contains(&e.id));
        let mut seen_now = HashSet::new();
        fresh.retain(|e| seen_now.insert(e.id.clone()));
        if !fresh.is_empty() {
            mrs_of_revisions(&mut fresh, &mut kept.revisions, &mut misses, &clones, &view, now).await;
            known.extend(fresh.iter().map(|e| e.id.clone()));
            let fresh_ids: HashSet<String> = fresh.iter().map(|e| e.id.clone()).collect();
            events.extend(fresh);
            events.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));
            // The crashes linked to what was deployed before them, those already written
            // included (a deploy read after its crash): their line written again, the last of
            // an id winning when the journal is read.
            let unlinked: HashSet<String> = events.iter().filter(|e| e.related.is_none()).map(|e| e.id.clone()).collect();
            observe::correlate(&mut events, cfg.correlate_minutes);
            let written: Vec<ChangeEvent> = events.iter().filter(|e| fresh_ids.contains(&e.id) || (unlinked.contains(&e.id) && e.related.is_some())).cloned().collect();
            if let Err(e) = append(&journal_path, &written) {
                tracing::warn!("changes: {e:#}");
            }
        }
        let floor = since();
        let count = events.len();
        events.retain(|e| e.at >= floor);
        if events.len() != count {
            known = events.iter().map(|e| e.id.clone()).collect();
        }
        if let Some(s) = kept.sessions.last_mut() {
            s.to = now;
        }
        if let Err(e) = save(&kept_path, &kept) {
            tracing::warn!("changes: {e:#}");
        }
        publish(&ctx, &cfg, &events, &kept, &envs);
        ctx.set_error((!errors.is_empty()).then(|| errors.join(" · ")));
        tokio::select! {
            _ = ctx.pause(EVERY) => {}
            _ = notify.notified() => {}
        }
    }
}

fn publish(ctx: &BlockCtx, cfg: &ChangesConfig, events: &[ChangeEvent], kept: &Kept, envs: &[String]) {
    let mut envs = envs.to_vec();
    for e in events.iter().filter_map(|e| e.env.as_ref()) {
        if !envs.contains(e) {
            envs.push(e.clone());
        }
    }
    ctx.publish(&ChangesState {
        events: events[events.len().saturating_sub(SHOWN)..].to_vec(),
        envs,
        sources: cfg.sources.clone(),
        closed: closed(&kept.sessions),
        correlate_minutes: cfg.correlate_minutes,
        default_period: cfg.default_period.clone(),
        retention_days: cfg.retention_days,
    });
}

/// The gaps between two reading sessions, longer than a few minutes.
fn closed(sessions: &[Closed]) -> Vec<Closed> {
    sessions.windows(2).filter(|w| w[1].from - w[0].to > Span::minutes(CLOSED_AFTER)).map(|w| Closed { from: w[0].to, to: w[1].from }).collect()
}

/* ---------- The journal ---------- */

fn load_journal(path: &Path, since: DateTime<Utc>) -> Vec<ChangeEvent> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let mut seen = HashSet::new();
    let mut events: Vec<ChangeEvent> = text.lines().filter_map(|l| serde_json::from_str::<ChangeEvent>(l).ok()).filter(|e| e.at >= since).collect();
    // The last line of an id wins (a crash linked after it was written).
    events.reverse();
    events.retain(|e| seen.insert(e.id.clone()));
    events.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));
    events
}

fn lines(events: &[ChangeEvent]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for e in events {
        serde_json::to_writer(&mut out, e)?;
        out.push(b'\n');
    }
    Ok(out)
}

/// Owner-only: what ran where is the team's.
fn create(path: &Path, append: bool) -> Result<std::fs::File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut o = std::fs::OpenOptions::new();
    o.create(true).write(true);
    if append { o.append(true) } else { o.truncate(true) };
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path).with_context(|| tr!("writing {path}", path = path.display()))
}

fn append(path: &Path, events: &[ChangeEvent]) -> Result<()> {
    create(path, true)?.write_all(&lines(events)?)?;
    Ok(())
}

/// Atomic: a temporary file then a rename.
fn rewrite(path: &Path, events: &[ChangeEvent]) -> Result<()> {
    let tmp = path.with_extension("jsonl.tmp");
    create(&tmp, false)?.write_all(&lines(events)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn save(path: &PathBuf, kept: &Kept) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    create(&tmp, false)?.write_all(&serde_json::to_vec(kept)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/* ---------- The merge request of a revision ---------- */

/// The MR each sync's revision came with, read in the local clones: at most a few revisions a
/// read; one not found is asked again half an hour later (the clone may have fetched it since).
async fn mrs_of_revisions(events: &mut [ChangeEvent], cache: &mut BTreeMap<String, ChangeMr>, misses: &mut BTreeMap<String, DateTime<Utc>>, clones: &[(String, String)], view: &View, now: DateTime<Utc>) {
    let mut asked = 0;
    for e in events.iter_mut().filter(|e| e.kind.deploys() && e.mr.is_none()) {
        let Some(rev) = e.revision.clone().filter(|r| r.len() >= 7 && r.chars().all(|c| c.is_ascii_hexdigit())) else { continue };
        if !cache.contains_key(&rev) {
            let recent = misses.get(&rev).is_some_and(|t| now - *t < Span::minutes(30));
            if asked >= 10 || recent || clones.is_empty() {
                continue;
            }
            asked += 1;
            match mr_of_revision(&rev, clones, view).await {
                Some(mr) => {
                    misses.remove(&rev);
                    cache.insert(rev.clone(), mr);
                }
                None => {
                    misses.insert(rev.clone(), now);
                }
            }
        }
        e.mr = cache.get(&rev).cloned();
    }
    // Bounded: a cache, whatever it drops is asked again.
    while cache.len() > 500 {
        let first = cache.keys().next().cloned();
        first.map(|k| cache.remove(&k));
    }
    misses.retain(|_, t| now - *t < Span::hours(6));
}

async fn mr_of_revision(rev: &str, clones: &[(String, String)], view: &View) -> Option<ChangeMr> {
    for (repo, dir) in clones {
        let Ok(message) = git(dir, &["log", "-1", "--format=%s%n%b", rev, "--"]).await else { continue };
        let Some(iid) = mr_number(&message) else { continue };
        let known = view.mrs.iter().find(|m| m.iid == iid && m.repo.as_deref().is_none_or(|r| r == repo));
        return Some(ChangeMr {
            repo: Some(repo.clone()),
            iid,
            title: known.map(|m| m.title.clone()).unwrap_or_else(|| message.lines().next().unwrap_or_default().to_string()),
            url: known.map(|m| m.url.clone()),
        });
    }
    None
}

/// The MR a commit message names: `See merge request group/app!123`, `… (#123)`.
fn mr_number(message: &str) -> Option<u64> {
    static GITLAB: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"See merge request \S*!(\d+)").expect("regex"));
    static GITHUB: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\(#(\d+)\)\s*$").expect("regex"));
    let first = message.lines().next().unwrap_or_default();
    GITLAB.captures(message).or_else(|| GITHUB.captures(first)).and_then(|c| c[1].parse().ok())
}

/* ---------- Kubernetes events ---------- */

#[derive(Deserialize)]
struct KubeList {
    #[serde(default)]
    items: Vec<KubeEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KubeEvent {
    metadata: KubeMeta,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    count: Option<u32>,
    last_timestamp: Option<DateTime<Utc>>,
    event_time: Option<DateTime<Utc>>,
    involved_object: KubeObject,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KubeMeta {
    uid: String,
    creation_timestamp: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct KubeObject {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    name: String,
}

/// The warnings of each environment's namespace (`kubectl get events`), every one at once: those
/// read, and why the others could not be.
async fn kube_events(view: &View, since: DateTime<Utc>) -> (Vec<ChangeEvent>, Vec<String>) {
    let reads = view.envs.iter().filter(|s| s.error.is_none()).map(|s| async move { (s, kube_list(s).await) });
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for (s, read) in futures::future::join_all(reads).await {
        match read {
            Ok(list) => out.extend(list.items.into_iter().filter_map(|k| kube_event(s, k)).filter(|e| e.at >= since)),
            Err(e) => errors.push(format!("{} ({}): {e:#}", s.env, s.namespace)),
        }
    }
    (out, errors)
}

async fn kube_list(s: &crate::state::EnvStatus) -> Result<KubeList> {
    let run = crate::platform::command("kubectl")
        .args(["--context", &s.context, "-n", &s.namespace, "get", "events", "--field-selector", "type=Warning", "-o", "json"])
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(Duration::from_secs(20), run).await.context("kubectl: no answer within 20 s")?.context(tr("kubectl not found in PATH"))?;
    if !output.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn kube_event(s: &crate::state::EnvStatus, k: KubeEvent) -> Option<ChangeEvent> {
    let at = k.last_timestamp.or(k.event_time).or(k.metadata.creation_timestamp)?;
    // The workload a pod belongs to: the longest name it starts with.
    let workload = s.workloads.iter().filter(|w| k.involved_object.name.starts_with(&w.name)).max_by_key(|w| w.name.len());
    let severity = if matches!(k.reason.as_str(), "OOMKilling" | "BackOff" | "Evicted" | "FailedScheduling") { ChangeSeverity::Ko } else { ChangeSeverity::Warn };
    let mut e = ChangeEvent::new(format!("kube:{}:{}", k.metadata.uid, k.count.unwrap_or(1)), at, "pods", ChangeKind::KubeEvent, severity, format!("{} {}", k.reason, k.involved_object.name));
    e.env = Some(s.env.clone());
    e.namespace = Some(s.namespace.clone());
    e.detail = Some(format!("{} · {}{}", k.involved_object.kind, k.message, k.count.filter(|c| *c > 1).map(|c| format!(" (×{c})")).unwrap_or_default()));
    if let Some(w) = workload {
        e.app = Some(w.app.clone().unwrap_or_else(|| w.name.clone()));
        e.workloads = vec![w.name.clone()];
        e.link = Some(ChangeLink::Pod { context: s.context.clone(), namespace: s.namespace.clone(), deployment: w.name.clone() });
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_request_of_a_commit() {
        assert_eq!(mr_number("Merge branch 'fix' into 'main'\n\nFix it\n\nSee merge request shop/api!1288"), Some(1288));
        assert_eq!(mr_number("In passing: launched in a click (#20)\n\nbody"), Some(20));
        assert_eq!(mr_number("fix: no number\n\nsee #12 in the body"), None);
    }

    #[test]
    fn closed_ranges_between_sessions() {
        let t = |m: i64| DateTime::parse_from_rfc3339("2026-11-05T10:00:00Z").unwrap().with_timezone(&Utc) + Span::minutes(m);
        let sessions = [Closed { from: t(0), to: t(60) }, Closed { from: t(62), to: t(120) }, Closed { from: t(600), to: t(700) }];
        assert_eq!(closed(&sessions), [Closed { from: t(120), to: t(600) }]);
    }

    #[test]
    fn the_journal_keeps_the_last_line_of_an_id() {
        let dir = std::env::temp_dir().join(format!("prism-changes-{}", std::process::id()));
        let path = dir.join("changes.jsonl");
        let now = Utc::now();
        let mut crash = ChangeEvent::new("crash", now, "pods", ChangeKind::CrashLoop, ChangeSeverity::Ko, "api");
        let old = ChangeEvent::new("old", now - Span::days(30), "pods", ChangeKind::Restart, ChangeSeverity::Warn, "api");
        append(&path, &[old, crash.clone()]).unwrap();
        crash.related = Some("sync".into());
        append(&path, std::slice::from_ref(&crash)).unwrap();
        let read = load_journal(&path, now - Span::days(14));
        assert_eq!(read, [crash]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_kube_event() {
        let s = crate::state::EnvStatus { env: "staging".into(), context: "c".into(), cluster: "c".into(), namespace: "shop".into(), error: None, workloads: vec![] };
        let k: KubeEvent = serde_json::from_value(serde_json::json!({
            "metadata": {"uid": "u1", "creationTimestamp": "2026-11-05T09:00:00Z"},
            "reason": "BackOff", "message": "Back-off restarting failed container", "count": 4,
            "lastTimestamp": "2026-11-05T09:44:00Z", "involvedObject": {"kind": "Pod", "name": "shop-api-7f9c-x2"}
        }))
        .unwrap();
        let e = kube_event(&s, k).unwrap();
        assert_eq!((e.id.as_str(), e.at.to_rfc3339().as_str(), e.severity), ("kube:u1:4", "2026-11-05T09:44:00+00:00", ChangeSeverity::Ko));
        assert_eq!(e.detail.as_deref(), Some("Pod · Back-off restarting failed container (×4)"));
    }
}
