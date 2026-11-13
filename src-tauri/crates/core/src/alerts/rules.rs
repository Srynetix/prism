//! Prism's rules: the sources, the clusters, Argo CD, the protected branches' CI, the tickets
//! against their MRs, the merge queue, the agents, their proposals and overlaps, the reviews and my
//! MRs. Faithful to what the window computed before (`web/src/rules.ts`), its texts the same.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use prism_base::{i18n::tr, tn, tr};

use super::{round, to_fixed, js_number, Alert, AlertRule, Fix, FixAction, OpenTarget, Prefs, Severity, Side};
use crate::agents::provider::Provider as Cli;
use crate::config::{AlertsConfig, RepoInfo};
use crate::conventions::Conventions;
use crate::state::*;

/// A ticket Prism reads, with the statuses of its list (to offer a change).
#[derive(Clone, Copy)]
pub(super) struct Entry<'a> {
    pub t: &'a Ticket,
    pub statuses: &'a [StatusOption],
}

/// MRs by ticket, in the order they were first met.
#[derive(Default)]
struct ByTicket<'a> {
    order: Vec<(String, Vec<&'a MergeRequest>)>,
    at: HashMap<String, usize>,
}

impl<'a> ByTicket<'a> {
    fn push(&mut self, key: String, m: &'a MergeRequest) {
        match self.at.get(&key) {
            Some(&i) => self.order[i].1.push(m),
            None => {
                self.at.insert(key.clone(), self.order.len());
                self.order.push((key, vec![m]));
            }
        }
    }

    fn get(&self, key: &str) -> &[&'a MergeRequest] {
        self.at.get(key).map_or(&[], |&i| &self.order[i].1)
    }
}

/// What the rules read: the snapshot, the shared prefs, the time, the thresholds and conventions.
pub(super) struct Ctx<'a> {
    pub s: &'a Snapshot,
    pub prefs: &'a Prefs,
    pub now: DateTime<Utc>,
    pub cfg: &'a AlertsConfig,
    pub c: &'a Conventions,
    /// The tickets by `ticket_ref`, built once a computation.
    index: std::cell::OnceCell<HashMap<String, Entry<'a>>>,
}

/// Claude usage has its own display, under the sessions: no duplicate alert.
const QUIET_SOURCES: [&str; 1] = ["usage"];

/// Beyond 99.5 % a plan is used up: its CLI refuses to go on until it resets.
const USED_UP: f64 = 99.5;

/// The pipeline statuses still to finish (`created`… `running`).
fn running(p: Option<PipelineStatus>) -> bool {
    p.is_some_and(PipelineStatus::is_active)
}

fn some(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|s| !s.is_empty())
}

/// GitLab's last rebase of the MR failed, whoever asked for it: a commit in between conflicts
/// although the branches do not as a whole. Rebased locally, by an agent, rather than retried.
pub(super) fn rebase_failed(m: &MergeRequest) -> bool {
    m.state == MrState::Opened && !m.conflicts && !m.rebase_in_progress && some(&m.merge_error).is_some_and(|e| e.to_lowercase().contains("rebase"))
}

/// GitLab's reason, without its `Rebase failed:` lead.
fn rebase_failure(m: &MergeRequest) -> String {
    let e = m.merge_error.as_deref().unwrap_or_default();
    let lead = "rebase failed:";
    let rest = if e.len() >= lead.len() && e.is_char_boundary(lead.len()) && e[..lead.len()].eq_ignore_ascii_case(lead) { e[lead.len()..].trim_start() } else { e };
    rest.trim().to_string()
}

/// A reviewer asked for changes, and has not approved since.
fn changes_requested(m: &MergeRequest) -> bool {
    !m.changes_requested_by.is_empty()
}

/// At least one approval, and no request for changes: one that may go "to merge".
pub(super) fn is_approved(m: &MergeRequest) -> bool {
    !m.approved_by.is_empty() && !changes_requested(m)
}

/// May go into the merge queue: neither a draft nor in conflict, its last rebase did not fail, no
/// required approval missing.
pub(super) fn can_queue(m: &MergeRequest) -> bool {
    !m.draft && !m.conflicts && !rebase_failed(m) && m.approvals_left <= 0
}

/// An open MR with nothing left but its running CI: the forge could merge it by itself once green.
pub(super) fn only_waits_for_ci(m: &MergeRequest) -> bool {
    m.state == MrState::Opened && !m.draft && !m.auto_merge && !m.conflicts && !rebase_failed(m) && !m.rebase_in_progress && !m.needs_rebase && m.merge_status != "need_rebase" && running(m.pipeline) && !changes_requested(m) && m.approvals_left <= 0 && m.open_threads == 0
}

/// Waits for my review: asked of me, not approved by me, no changes requested by me (the author's turn then).
pub(super) fn awaits_my_review(m: &MergeRequest, me: Option<&str>) -> bool {
    let Some(me) = me.filter(|me| !me.is_empty()) else { return false };
    let has = |list: &[String]| list.iter().any(|x| x == me);
    m.state == MrState::Opened && !m.draft && has(&m.reviewers) && !has(&m.approved_by) && !has(&m.changes_requested_by)
}

/// Waiting for an answer or a permission.
pub(super) fn waits_for_you(s: SessionStatus) -> bool {
    matches!(s, SessionStatus::NeedsYou | SessionStatus::WaitingTool)
}

/// Working: on its turn, or its turn over while its subagents or background tasks go on.
fn is_working(s: SessionStatus) -> bool {
    matches!(s, SessionStatus::Running | SessionStatus::Background)
}

/// A Prism agent's turn over, asking nothing (Your turn): flagged a short while only.
pub(super) fn your_turn_only(c: &ClaudeSession) -> bool {
    c.origin == SessionOrigin::Agent && c.status == SessionStatus::NeedsYou && some(&c.needs_action).is_none()
}

/// A date as the snapshot's JSON writes it, to compare with what the window kept of it.
pub(super) fn iso(at: Option<DateTime<Utc>>) -> String {
    at.and_then(|a| serde_json::to_value(a).ok()).and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

/// The last name of a path.
pub(super) fn base_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or_default()
}

const ACCESS: [(&str, &str, &str); 6] = [("clickup", "clickup", "ClickUp"), ("clickup queues", "clickup", "ClickUp"), ("jira", "jira", "Jira"), ("jira queues", "jira", "Jira"), ("gitlab", "forge", "GitLab"), ("github", "forge", "GitHub")];

/// `HTTP 401` or `HTTP 403`, a whole word.
fn refused(err: &str) -> bool {
    ["HTTP 401", "HTTP 403"].iter().any(|code| err.match_indices(code).any(|(i, _)| !err[i + code.len()..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_')))
}

/// A source's access missing or refused, in words, with the Settings section that fixes it: a
/// tracker's token not in the keychain or refused (401, 403), the forge CLI not signed in.
pub fn access_alert(source: &str, err: &str) -> Option<Alert> {
    let &(_, section, name) = ACCESS.iter().find(|(s, _, _)| *s == source)?;
    let fix = Fix::Settings { section: section.into(), label: tr!("Settings › {name}", name = if section == "forge" { tr("Forge") } else { name }) };
    if err.contains("missing token") {
        return Some(Alert::new("Source", Severity::Warn, tr!("{name}: no token yet — paste it in Settings, then restart", name = name)).fixes(vec![fix]));
    }
    if refused(err) {
        return Some(Alert::new("Source", Severity::Ko, tr!("{name} refused the token (revoked or regenerated?): paste a new one in Settings", name = name)).fixes(vec![fix]));
    }
    let lower = err.to_lowercase();
    if section == "forge" && (["auth login", "not logged", "401 unauthorized", "http 401", "authentication"].iter().any(|w| lower.contains(w))) {
        let cli = if name == "GitHub" { "gh" } else { "glab" };
        return Some(Alert::new("Source", Severity::Ko, tr!("{name}: {cli} is not signed in — {err}", name = name, cli = cli, err = err)).fixes(vec![fix]));
    }
    None
}

impl<'a> Ctx<'a> {
    pub fn new(s: &'a Snapshot, prefs: &'a Prefs, now: DateTime<Utc>) -> Self {
        Ctx { s, prefs, now, cfg: &s.alerts_settings, c: &s.conventions, index: std::cell::OnceCell::new() }
    }

    /// Minutes since `at`, 0 without it.
    pub fn minutes_since(&self, at: Option<DateTime<Utc>>) -> f64 {
        at.map_or(0.0, |at| (self.now - at).num_milliseconds() as f64 / 60_000.0)
    }

    /* ---------- Repositories, forge, tracker ---------- */

    fn primary(&self) -> Option<&'a RepoInfo> {
        self.s.repositories.iter().find(|r| r.primary)
    }

    /// The repository of an MR (`repo` absent: the primary one).
    fn repo_of(&self, repo: Option<&str>) -> Option<&'a RepoInfo> {
        match repo.filter(|r| !r.is_empty()) {
            Some(name) => self.s.repositories.iter().find(|r| r.name == name),
            None => self.primary(),
        }
    }

    /// The name of an MR's repository, the primary one's when absent.
    pub fn repo_name(&self, repo: Option<&str>) -> String {
        repo.map(String::from).or_else(|| self.primary().map(|r| r.name.clone())).unwrap_or_default()
    }

    fn has_worktrees(&self, repo: Option<&str>) -> bool {
        self.repo_of(repo).is_some_and(|r| r.worktrees)
    }

    /// Your login on an MR's forge: the profile's, or the other forge's for an MR of a repository there.
    pub fn me_on(&self, m: &MergeRequest) -> Option<&'a str> {
        if m.forge.is_some() { self.s.me_elsewhere.as_deref() } else { self.s.me.as_deref() }
    }

    fn mine(&self, m: &MergeRequest) -> bool {
        self.me_on(m).is_some_and(|me| m.author == me)
    }

    /// `!1621` for the primary repository, `helm!214` for another; `#` on GitHub.
    pub fn mr_key(&self, m: &MergeRequest) -> String {
        let forge = m.forge.or_else(|| some(&m.repo).and_then(|r| self.s.repositories.iter().find(|x| x.name == r)).and_then(|r| r.forge)).unwrap_or(self.s.forge);
        let sign = if forge == crate::forge::Provider::Github { '#' } else { '!' };
        format!("{}{sign}{}", m.repo.as_deref().unwrap_or_default(), m.iid)
    }

    /// The key the rules look a ticket up by: its id; on Obsidian, qualified by its repository.
    pub fn ticket_ref(&self, id: &str, repo: Option<&str>) -> String {
        if self.s.tracker == Some(crate::tracker::Provider::Obsidian) {
            let repo = repo.filter(|r| !r.is_empty()).map(String::from).or_else(|| self.primary().map(|r| r.name.clone())).unwrap_or_default();
            format!("{repo}|{id}")
        } else {
            id.to_string()
        }
    }

    /// The key a ticket opens by: its id, unless ids are only unique per repository.
    fn ticket_open(&self, t: &Ticket) -> OpenTarget {
        let key = if some(&t.repo).is_some() || t.id.is_empty() { t.task_id.clone() } else { t.id.clone() };
        OpenTarget::Ticket { id: key }
    }

    fn has(list: &[String], status: &str) -> bool {
        list.iter().any(|s| s == status)
    }

    fn blocked(&self, status: &str) -> bool {
        Self::has(&self.c.blocked_statuses, status)
    }

    fn in_progress(&self, status: &str) -> bool {
        Self::has(&self.c.in_progress_statuses, status)
    }

    pub fn is_done(&self, t: &Ticket) -> bool {
        t.closed || Self::has(&self.c.done_statuses, &t.status)
    }

    /// Tickets Prism knows, with the statuses of their list, by `ticket_ref`.
    pub fn ticket_index(&self) -> &HashMap<String, Entry<'a>> {
        self.index.get_or_init(|| self.index_tickets())
    }

    fn index_tickets(&self) -> HashMap<String, Entry<'a>> {
        let s = self.s;
        let mut index = HashMap::new();
        let mut add = |t: &'a Ticket, statuses: &'a [StatusOption]| {
            index.entry(self.ticket_ref(&t.id, t.repo.as_deref())).or_insert(Entry { t, statuses });
        };
        if let Some(sprint) = &s.sprint {
            for t in &sprint.tickets {
                // A sprint of several boards (Obsidian): each ticket its own board's lanes.
                let board = t.list.as_ref().and_then(|l| sprint.boards.iter().find(|b| b.id == l.id));
                add(t, board.map_or(&sprint.statuses, |b| &b.statuses));
            }
        }
        for b in &s.pickup.boards {
            for t in &b.tickets {
                add(t, &b.statuses);
            }
        }
        for t in &s.outside_sprint.tickets {
            add(t, t.list.as_ref().and_then(|l| s.outside_sprint.list_statuses.get(&l.id)).map_or(&[], Vec::as_slice));
        }
        index
    }

    /// Move the ticket to the first of these statuses its list knows; none, no fix.
    fn move_to(entry: Option<&Entry>, wanted: &[String]) -> Vec<Fix> {
        let Some(e) = entry else { return Vec::new() };
        wanted
            .iter()
            .find(|w| **w != e.t.status && e.statuses.iter().any(|o| o.status == **w))
            .map(|status| vec![Fix::Status { task_id: e.t.task_id.clone(), ticket: e.t.id.clone(), status: status.clone(), statuses: e.statuses.to_vec() }])
            .unwrap_or_default()
    }

    fn open_mrs(&self) -> impl Iterator<Item = &'a MergeRequest> {
        self.s.merge_requests.iter().filter(|m| m.state == MrState::Opened)
    }

    /// The open MRs by ticket they name.
    fn open_by_ticket(&self) -> ByTicket<'a> {
        let mut by = ByTicket::default();
        for m in self.open_mrs() {
            for id in &m.tickets {
                by.push(self.ticket_ref(id, Some(&self.repo_name(m.repo.as_deref()))), m);
            }
        }
        by
    }

    fn mr_open(m: &MergeRequest) -> OpenTarget {
        OpenTarget::Mr { iid: m.iid, repo: m.repo.clone() }
    }

    fn mr_fix(m: &MergeRequest) -> Fix {
        Fix::Mr { iid: m.iid, repo: m.repo.clone() }
    }

    fn claude_fix(&self, m: &MergeRequest, action: FixAction) -> Option<Fix> {
        self.has_worktrees(m.repo.as_deref()).then(|| Fix::Claude { iid: m.iid, repo: m.repo.clone(), action })
    }

    /* ---------- Sources ---------- */

    pub fn source_alerts(&self) -> Vec<Alert> {
        self.s
            .errors
            .iter()
            .filter(|(source, _)| !QUIET_SOURCES.contains(source))
            .map(|(&source, err)| {
                access_alert(source, err).unwrap_or_else(|| match source {
                    "config" => Alert::new("Source", if err.starts_with("no config yet") { Severity::Warn } else { Severity::Ko }, tr!("Config: {err}", err = err)),
                    // Setup warnings rather than an unreachable source: the source answers, the config points at nothing.
                    "clickup-setup" => Alert::new("Source", Severity::Warn, format!("{}: {err}", tr("ClickUp config"))),
                    _ => Alert::new("Source", Severity::Ko, tr!("{source} unreachable: {err}", source = source, err = err)),
                })
                // Its source: a phone kept from it (the mail) is not told its error either.
                .item(source)
            })
            .collect()
    }

    /* ---------- Clusters ---------- */

    /// Pods and runners: jobs stuck (with the runner down, one incident), pods broken, CI jobs with no runner left.
    pub fn infra_alerts(&self) -> Vec<Alert> {
        let s = self.s;
        let mut out = Vec::new();
        let broken: Vec<&PodView> = s.pods.iter().filter(|p| !p.healthy).collect();
        let stuck: Vec<f64> = s.pending_jobs.iter().map(|j| self.minutes_since(Some(j.queued_since))).filter(|m| *m > f64::from(self.cfg.pending_job_minutes)).collect();
        if !stuck.is_empty() {
            let cause = broken.iter().find(|p| p.role == PodRole::Runner);
            let oldest = round(stuck.iter().copied().fold(f64::MIN, f64::max));
            let mut text = tn!(stuck.len(), "{n} job queued for {min} min", "{n} jobs queued for {min} min", min = oldest);
            if let Some(p) = cause {
                text += &tr!(" — runner {name} is {status}", name = p.name, status = p.status);
            }
            // Pending jobs: to see in Alerts, nothing worth interrupting for (the runner down, for its part, notifies).
            out.push(Alert::new("Infra", Severity::Ko, text).notify(false));
        }
        for p in &broken {
            let what = match p.role {
                PodRole::Job => tr!("Job {name}", name = p.job_name.as_deref().unwrap_or(&p.name)),
                PodRole::Runner => tr!("Runner {name}", name = p.name),
                PodRole::Pod => format!("Pod {}/{} ({})", p.namespace, p.name, p.target),
            };
            let mut text = tr!("{what} is {status}", what = what, status = p.status);
            if p.restarts != 0 {
                text += &tn!(p.restarts, " ({n} restart)", " ({n} restarts)");
            }
            let fixes = if p.role == PodRole::Job {
                Vec::new()
            } else {
                let restarts = if p.restarts != 0 { format!(" ({} restarts)", p.restarts) } else { String::new() };
                vec![Fix::Investigate { workload: p.workload.clone().unwrap_or_else(|| p.name.clone()), namespace: p.namespace.clone(), context: p.context.clone(), env: p.target.clone(), issue: format!("pod {} is {}{restarts}", p.name, p.status) }]
            };
            // A pod waiting for room: visible in Alerts, without notification.
            out.push(Alert::new("Infra", Severity::Ko, text).notify(p.status != "Pending").fixes(fixes));
        }
        // CI jobs with no runner left to take them, target by target.
        let mut seen = HashSet::new();
        for target in s.pods.iter().filter(|p| p.role == PodRole::Job).map(|p| &p.target) {
            if seen.insert(target) && !s.pods.iter().any(|p| p.target == *target && p.role == PodRole::Runner) {
                out.push(Alert::new("Infra", Severity::Ko, tr!("No GitLab runner pod left in {target}, only its CI jobs", target = target)));
            }
        }
        out
    }

    /// Argo CD: last sync failed, application degraded or missing, or out of sync that lasts.
    /// Fixes: the pods' logs (when the Environments block knows the namespace), an agent to
    /// investigate, the Changes timeline; the link to the Argo CD UI (`url`).
    pub fn argo_alerts(&self) -> Vec<Alert> {
        let s = self.s;
        let mut out = Vec::new();
        for instance in s.argocd.iter().flat_map(|a| &a.instances) {
            for a in &instance.apps {
                // Instance replaced for this environment (`[[argocd]] envs`): its states no longer mean anything.
                if let Some(envs) = &instance.envs
                    && !some(&a.env).is_some_and(|e| envs.iter().any(|x| x == e))
                {
                    continue;
                }
                let url = some(&instance.url).map(|u| format!("{}/applications/{}", u.strip_suffix('/').unwrap_or(u), a.name));
                let open = OpenTarget::Argo { cluster: instance.cluster.clone(), app: format!("{}/{}/{}", instance.cluster, instance.namespace, a.name) };
                let env = s.environments.iter().flat_map(|e| &e.envs).find(|e| a.env.as_deref() == Some(e.env.as_str()) && a.namespace.as_deref() == Some(e.namespace.as_str()));
                let name = a.app.as_deref().unwrap_or(&a.name);
                let mut fixes = Vec::new();
                if let (Some(env), Some(d)) = (env, a.deployments.first()) {
                    fixes.push(Fix::Podlogs { context: env.context.clone(), namespace: env.namespace.clone(), deployment: d.clone(), label: format!("{name} · {}", env.env) });
                }
                let why = some(&a.message).map(|m| format!(": {m}")).unwrap_or_default();
                let degraded = a.health == "Degraded";
                let failed = matches!(a.operation.as_deref(), Some("Failed" | "Error"));
                if let Some(env) = env
                    && degraded
                {
                    let workload = a.deployments.first().or(a.app.as_ref()).unwrap_or(&a.name).clone();
                    fixes.push(Fix::Investigate { workload, namespace: env.namespace.clone(), context: env.context.clone(), env: env.env.clone(), issue: format!("ArgoCD application {} is Degraded{why}", a.name) });
                }
                // What changed just before: the Changes timeline around it (its block on).
                if s.blocks.get("changes").is_some_and(|v| !v.is_null()) && (degraded || a.health == "Missing" || failed) {
                    fixes.push(Fix::Changes { env: a.env.clone(), app: a.app.as_ref().or(a.deployments.first()).unwrap_or(&a.name).clone() });
                }
                let alert = |severity, text: String| {
                    let alert = Alert::new("ArgoCD", severity, text).open(open.clone()).fixes(fixes.clone());
                    match &url {
                        Some(u) => alert.url(u.clone()),
                        None => alert,
                    }
                };
                if failed {
                    out.push(alert(Severity::Ko, tr!("{name}: last sync failed", name = a.name) + &why));
                } else if degraded || a.health == "Missing" {
                    out.push(alert(Severity::Ko, tr!("{name} is {health}", name = a.name, health = a.health) + &why));
                } else if a.sync == "OutOfSync" && a.operation.as_deref() != Some("Running") && (a.synced_at.is_none() || self.minutes_since(a.synced_at) > f64::from(self.cfg.argo_out_of_sync_minutes)) {
                    let text = match a.synced_at {
                        Some(at) => tr!("{name} out of sync with Git since {min} min", name = a.name, min = round(self.minutes_since(Some(at)))),
                        None => tr!("{name} out of sync with Git", name = a.name),
                    };
                    out.push(alert(Severity::Warn, text).notify(false));
                }
            }
        }
        out
    }

    /* ---------- CI ---------- */

    /// A protected branch's pipeline failed (only those are collected).
    pub fn ci_alerts(&self) -> Vec<Alert> {
        self.s
            .pipelines
            .iter()
            .filter(|p| p.status == PipelineStatus::Failed)
            .map(|p| {
                let branch = match some(&p.repo) {
                    Some(repo) => format!("{repo} · {}", p.target),
                    None => p.target.clone(),
                };
                let stage = p.stages.iter().find(|st| st.failed > 0 || st.status == PipelineStatus::Failed).map(|st| &st.name);
                let mut text = match stage {
                    Some(stage) => tr!("{branch} failed at stage {stage}", branch = branch, stage = stage),
                    None => tr!("{branch} failed", branch = branch),
                };
                if !p.failed_jobs.is_empty() {
                    text += &tr!(": {jobs}", jobs = p.failed_jobs.iter().take(3).cloned().collect::<Vec<_>>().join(", "));
                }
                Alert::new("CI", Severity::Ko, text).url(p.url.clone()).open(OpenTarget::Pipeline { pipeline: p.id, label: branch.clone(), project: p.project.clone() }).fixes(vec![
                    Fix::Logs { pipeline: p.id, label: branch.clone(), project: p.project.clone() },
                    Fix::Retry { pipeline: p.id, label: branch, project: None },
                    Fix::FixBranch { pipeline: p.id },
                ])
            })
            .collect()
    }

    /* ---------- Tickets against their MRs ---------- */

    /// The heart of Prism: confront the state of the MRs with that of the tickets, both ways.
    pub fn sync_alerts(&self) -> Vec<Alert> {
        let (s, c) = (self.s, self.c);
        let mut out = Vec::new();
        // `me` is only known after the first forge read: before, the absence of an MR proves nothing.
        if s.me.is_none() {
            return out;
        }
        // The sprint, and the boards read whole to check their tickets (`sync`).
        let mut tickets: HashMap<String, &Ticket> = HashMap::new();
        for t in s.sprint.iter().flat_map(|sp| &sp.tickets).chain(s.pickup.boards.iter().filter(|b| b.sync).flat_map(|b| &b.tickets)) {
            tickets.insert(self.ticket_ref(&t.id, t.repo.as_deref()), t);
        }
        if tickets.is_empty() {
            return out;
        }
        let mut by = ByTicket::default();
        for m in &s.merge_requests {
            for id in &m.tickets {
                by.push(self.ticket_ref(id, Some(&self.repo_name(m.repo.as_deref()))), m);
            }
        }
        let branches: HashSet<String> = s.worktrees.iter().filter_map(|w| w.ticket.as_deref().filter(|t| !t.is_empty()).map(|t| self.ticket_ref(t, Some(&w.repo)))).collect();
        let index = self.ticket_index();

        for (key, mrs) in &by.order {
            let Some(t) = tickets.get(key) else { continue };
            if self.blocked(&t.status) {
                continue;
            }
            let opened: Vec<_> = mrs.iter().filter(|m| m.state == MrState::Opened).collect();
            let merged: Vec<_> = mrs.iter().filter(|m| m.state == MrState::Merged).collect();
            // MR merged, ticket not yet to validate: to flag, except in progress (other MRs may follow).
            if opened.is_empty() && !merged.is_empty() && !t.closed && !Self::has(&c.after_merge_statuses, &t.status) && !self.in_progress(&t.status) {
                let text = tr!("{mr} merged — {id} still “{status}”", mr = self.mr_key(merged[0]), id = t.id, status = t.status);
                out.push(Alert::new("Sync", Severity::Warn, text).url(t.url.clone()).open(self.ticket_open(t)).fixes(Self::move_to(index.get(key), &c.after_merge_statuses)));
            }
            if let Some(m) = opened.iter().find(|m| !m.draft)
                && Self::has(&c.before_dev_statuses, &t.status)
            {
                let wanted: Vec<String> = c.review_statuses.iter().chain(&c.in_progress_statuses).cloned().collect();
                let text = tr!("{mr} open — {id} still “{status}”", mr = self.mr_key(m), id = t.id, status = t.status);
                out.push(Alert::new("Sync", Severity::Warn, text).url(t.url.clone()).open(self.ticket_open(t)).fixes(Self::move_to(index.get(key), &wanted)));
            }
            if let Some(m) = opened.first()
                && t.closed
            {
                let text = tr!("{id} closed while {mr} is still open", id = t.id, mr = self.mr_key(m));
                out.push(Alert::new("Sync", Severity::Warn, text).url(m.url.clone()).open(Self::mr_open(m)).fixes(vec![Self::mr_fix(m)]));
            }
        }

        // A parent (EPIC) whose children are in the sprint: the work, and the branches, live with them.
        let parents: HashSet<&str> = s.sprint.iter().flat_map(|sp| &sp.tickets).filter_map(|t| some(&t.parent)).collect();
        let idle_days = self.cfg.doing_idle_days;
        for t in s.sprint.iter().flat_map(|sp| &sp.tickets) {
            if self.blocked(&t.status) {
                continue;
            }
            let reference = self.ticket_ref(&t.id, t.repo.as_deref());
            let mrs = by.get(&reference);
            if Self::has(&c.review_statuses, &t.status) && !mrs.iter().any(|m| m.state == MrState::Opened) && !mrs.iter().any(|m| m.state == MrState::Merged) {
                out.push(Alert::new("Sync", Severity::Warn, tr!("{id} is “{status}” with no open MR", id = t.id, status = t.status)).url(t.url.clone()).open(self.ticket_open(t)));
            }
            if self.in_progress(&t.status) && mrs.is_empty() && !branches.contains(&reference) && !parents.contains(t.task_id.as_str()) && self.minutes_since(t.updated_at) > f64::from(idle_days) * 1440.0 {
                let text = tr!("{id} “{status}” for over {days} days with no branch or MR", id = t.id, status = t.status, days = idle_days);
                out.push(Alert::new("Sync", Severity::Info, text).url(t.url.clone()).open(self.ticket_open(t)));
            }
        }
        out
    }

    /// One of my MRs is a draft: I am working on it, its ticket should be "in progress". Tickets
    /// are looked up wherever Prism reads them: a bug or hotfix MR counts too.
    pub fn draft_alerts(&self) -> Vec<Alert> {
        if self.s.me.is_none() {
            return Vec::new();
        }
        let index = self.ticket_index();
        let wanted = &self.c.in_progress_statuses;
        let mut out = Vec::new();
        for m in self.open_mrs().filter(|m| m.draft && self.mine(m)) {
            for id in &m.tickets {
                let entry = index.get(&self.ticket_ref(id, Some(&self.repo_name(m.repo.as_deref()))));
                let Some(t) = entry.map(|e| e.t) else { continue };
                if t.closed || self.in_progress(&t.status) {
                    continue;
                }
                let first = wanted.first().map_or_else(|| tr("in progress").to_string(), Clone::clone);
                let text = tr!("{mr} is a draft — {id} is “{status}”, not “{wanted}”", mr = self.mr_key(m), id = t.id, status = t.status, wanted = first);
                out.push(Alert::new("Sync", Severity::Warn, text).url(t.url.clone()).open(self.ticket_open(t)).fixes(Self::move_to(entry, wanted)));
            }
        }
        out
    }

    /// A ticket still in progress whose open MRs are all out of draft: the work is in review, the
    /// ticket should say so, or directly "to merge" when they are all approved.
    pub fn in_review_alerts(&self) -> Vec<Alert> {
        let c = self.c;
        if self.s.me.is_none() || c.review_statuses.is_empty() {
            return Vec::new();
        }
        let index = self.ticket_index();
        let by = self.open_by_ticket();
        let mut out = Vec::new();
        for (key, mrs) in &by.order {
            let entry = index.get(key);
            let Some(t) = entry.map(|e| e.t) else { continue };
            if t.closed || self.blocked(&t.status) || !self.in_progress(&t.status) || mrs.iter().any(|m| m.draft) {
                continue;
            }
            let wanted = if !c.approved_statuses.is_empty() && mrs.iter().all(|m| is_approved(m)) { &c.approved_statuses } else { &c.review_statuses };
            let text = tr!("{mr} in review — {id} still “{status}”, not “{wanted}”", mr = self.mr_key(mrs[0]), id = t.id, status = t.status, wanted = wanted[0]);
            out.push(Alert::new("Sync", Severity::Warn, text).url(t.url.clone()).open(self.ticket_open(t)).fixes(Self::move_to(entry, wanted)));
        }
        out
    }

    /// A ticket "to merge" whose open MR, in a repository with a merge queue, is not in it: nothing
    /// will merge it. Already merging or a draft, left alone; in conflict, after a failed rebase or
    /// still short of an approval, flagged without the fix.
    pub fn queue_alerts(&self) -> Vec<Alert> {
        let statuses = &self.c.merge_queue_statuses;
        let Some(label) = some(&self.s.merge_queue_label) else { return Vec::new() };
        if statuses.is_empty() {
            return Vec::new();
        }
        let index = self.ticket_index();
        let mut out = Vec::new();
        for m in self.open_mrs() {
            if m.draft || m.auto_merge || m.labels.iter().any(|l| l == label) || !self.repo_of(m.repo.as_deref()).is_some_and(|r| r.merge_queue) {
                continue;
            }
            // Of the tickets it names, the first one waiting to be merged.
            let repo = self.repo_name(m.repo.as_deref());
            let Some(t) = m.tickets.iter().filter_map(|id| index.get(&self.ticket_ref(id, Some(&repo))).map(|e| e.t)).find(|t| !t.closed && !self.blocked(&t.status) && Self::has(statuses, &t.status)) else { continue };
            let mut fixes = Vec::new();
            if can_queue(m) {
                fixes.push(Fix::Queue { iid: m.iid, repo: m.repo.clone() });
            }
            fixes.push(Self::mr_fix(m));
            let first = if m.conflicts {
                tr(" (conflicts first)")
            } else if rebase_failed(m) {
                tr(" (failed rebase first)")
            } else if m.approvals_left > 0 {
                tr(" (approval first)")
            } else {
                ""
            };
            let text = tr!("{id} “{status}” — {mr} not in the merge queue", id = t.id, status = t.status, mr = self.mr_key(m)) + first;
            out.push(Alert::new("Sync", Severity::Warn, text).url(m.url.clone()).open(Self::mr_open(m)).fixes(fixes));
        }
        out
    }

    /// A ticket's open MRs are all approved (none a draft), but it is still in review: it should
    /// be "to merge".
    pub fn approved_alerts(&self) -> Vec<Alert> {
        let c = self.c;
        if self.s.me.is_none() || c.approved_statuses.is_empty() {
            return Vec::new();
        }
        let index = self.ticket_index();
        let by = self.open_by_ticket();
        let mut out = Vec::new();
        for (key, mrs) in &by.order {
            let entry = index.get(key);
            let Some(t) = entry.map(|e| e.t) else { continue };
            let awaiting = Self::has(&c.review_statuses, &t.status) && !Self::has(&c.approved_statuses, &t.status);
            if t.closed || self.blocked(&t.status) || !awaiting || !mrs.iter().all(|m| !m.draft && is_approved(m)) {
                continue;
            }
            let text = tr!("{mr} approved — {id} still “{status}”, not “{wanted}”", mr = self.mr_key(mrs[0]), id = t.id, status = t.status, wanted = c.approved_statuses[0]);
            out.push(Alert::new("Sync", Severity::Warn, text).url(t.url.clone()).open(self.ticket_open(t)).fixes(Self::move_to(entry, &c.approved_statuses)));
        }
        out
    }

    /* ---------- Agents ---------- */

    /// The Claude sessions counted in Prism's figures: Prism's agents, and Claude Desktop's with
    /// `[agents.claude] count_desktop`, when they are shown.
    fn counted_sessions(&self) -> impl Iterator<Item = &'a ClaudeSession> {
        let a = &self.s.agents_settings;
        let desktop = a.claude.count_desktop && a.claude.show_desktop && a.provider == Cli::Claude;
        self.s.sessions.iter().filter(move |c| c.origin == SessionOrigin::Agent || desktop)
    }

    /// A session waiting for you, flagged a while: a short one for a turn just over, hours otherwise.
    pub fn waiting_alert(&self, c: &ClaudeSession) -> bool {
        let limit = if your_turn_only(c) { f64::from(self.cfg.your_turn_minutes) } else { f64::from(self.cfg.waiting_session_hours) * 60.0 };
        waits_for_you(c.status) && self.minutes_since(c.last_activity) < limit
    }

    /// How much of `p`'s plan is used now (its fullest window), when Prism reads it.
    fn plan_use(&self, p: Cli) -> Option<f64> {
        let s = self.s;
        let configured = s.agents_settings.provider;
        let agents = |p: Cli| s.agents.iter().filter(|a| a.state == AgentState::Running && a.provider == p).count();
        let mut sections: Vec<(Cli, Option<&ClaudeUsage>, bool, usize)> = vec![(configured, s.usage.as_ref(), s.errors.contains_key("usage"), agents(configured))];
        sections.extend(s.other_usage.iter().map(|o| (o.provider, o.usage.as_ref(), o.error.is_some(), agents(o.provider))));
        if configured != Cli::OpenCode && agents(Cli::OpenCode) > 0 {
            sections.push((Cli::OpenCode, None, false, agents(Cli::OpenCode)));
        }
        let (_, usage, _, _) = sections.into_iter().filter(|(_, u, error, agents)| u.is_some_and(|u| u.five_hour.is_some() || u.seven_day.is_some()) || *agents > 0 || *error).find(|(provider, ..)| *provider == p)?;
        let u = usage?;
        Some(u.five_hour.iter().chain(&u.seven_day).chain(u.others.iter().map(|(_, w)| w)).map(|w| w.utilization).fold(0.0, f64::max))
    }

    /// What waits on you from the agents: a permission, a question or a turn over, a cost past its
    /// limit, a plan used up, a command failing again and again.
    pub fn session_alerts(&self) -> Vec<Alert> {
        let s = self.s;
        let mut out = Vec::new();
        let recent: Vec<&ClaudeSession> = self.counted_sessions().filter(|c| self.waiting_alert(c)).collect();
        for c in recent.iter().filter(|c| c.status == SessionStatus::WaitingTool) {
            out.push(Alert::new("Claude", Severity::Warn, tr!("“{title}” is stuck on a tool or a permission prompt", title = c.title)).rule(AlertRule::Permission).since(c.last_activity));
        }
        let waiting: Vec<&&ClaudeSession> = recent.iter().filter(|c| c.status == SessionStatus::NeedsYou).collect();
        if !waiting.is_empty() {
            // One agent alone waits: its quick answers in the row.
            let agents: Vec<&Agent> = s.agents.iter().filter(|a| a.state == AgentState::Running && a.status == SessionStatus::NeedsYou).collect();
            let answer = (waiting.len() == 1 && agents.len() == 1).then(|| agents[0]);
            let text = match answer {
                Some(a) => tr!("“{title}” is waiting for your answer", title = a.title),
                None => tn!(waiting.len(), "{n} session is waiting for your answer", "{n} sessions are waiting for your answer"),
            };
            // A question, a plan to approve: someone waits on you. Only turns just over: nothing presses.
            let rule = if waiting.iter().all(|c| your_turn_only(c)) { AlertRule::YourTurn } else { AlertRule::Question };
            let mut alert = Alert::new("Claude", Severity::Info, text).rule(rule).since(waiting.iter().filter_map(|c| c.last_activity).min());
            alert.answer = answer.map(|a| a.id.clone());
            // Seen, and nothing to answer: out of the way until their next turn.
            let read: Vec<String> = waiting.iter().filter(|c| crate::read_marks::can_mark(c.status, c.needs_action.as_deref())).map(|c| c.id.clone()).collect();
            if !read.is_empty() {
                alert = alert.fixes(vec![Fix::Read { sessions: read }]);
            }
            out.push(alert);
        }
        // A Claude Code agent costing more than `[agents.claude] cost_alert` (its conversation, at the API prices).
        let limit = s.agents_settings.claude.cost_alert;
        for a in s.agents.iter().filter(|a| a.provider == Cli::Claude && limit > 0.0) {
            let cost = a.usage.as_ref().and_then(|u| u.cost).unwrap_or(0.0);
            if cost >= limit {
                out.push(Alert::new("Claude", Severity::Warn, tr!("“{title}” has cost about ${cost} (beyond ${limit})", title = a.title, cost = to_fixed(cost, 2), limit = js_number(limit))).rule(AlertRule::AgentCost));
            }
        }
        // An agent whose CLI's plan is used up, waiting: another CLI can take it on (`[agents] suggest_switch`).
        if s.agents_settings.suggest_switch {
            for a in s.agents.iter().filter(|a| a.state != AgentState::Prepared && !is_working(a.status) && self.plan_use(a.provider).unwrap_or(0.0) >= USED_UP) {
                out.push(Alert::new("Claude", Severity::Warn, tr!("“{title}” stopped at its plan’s limit: another CLI can take it on", title = a.title)).rule(AlertRule::AgentLimit).fixes(vec![Fix::Switch { agent: a.id.clone() }]));
            }
        }
        // An agent running the same failing command again and again (5 times: past a first warning in its row).
        for a in &s.agents {
            if let Some(l) = a.looping.as_ref().filter(|l| l.count >= 5) {
                out.push(Alert::new("Claude", Severity::Warn, tr!("“{title}” is going in circles: {command} failed {count} times in a row", title = a.title, command = l.command, count = l.count)).rule(AlertRule::AgentLoop));
            }
        }
        out
    }

    /// A change of Prism an agent proposed (`prism mcp`), waiting for your answer: one line each,
    /// from when it came, gone once answered. Its fix opens it.
    pub fn proposal_alerts(&self) -> Vec<Alert> {
        let s = self.s;
        s.mcp
            .proposals
            .iter()
            .map(|p| {
                let who = some(&p.agent).and_then(|id| s.agents.iter().find(|a| a.id == id)).map(|a| a.title.as_str()).filter(|t| !t.is_empty()).unwrap_or(&p.client);
                // What stays the same while it waits (its id, its reason), not its diff: the alert keeps its identity.
                let why = if p.why.trim().is_empty() { format!("#{}", p.id) } else { p.why.trim().to_string() };
                let text = if p.kind == crate::mcp::ProposalKind::CustomBlock { tr!("“{who}” proposes a new custom block: {what}", who = who, what = why) } else { tr!("“{who}” proposes a change of Prism: {what}", who = who, what = why) };
                Alert::new("Claude", Severity::Warn, text).since(Some(p.at)).notify(false).fixes(vec![Fix::Proposal { id: p.id }])
            })
            .collect()
    }

    /// The agent working in `path`: a live one first, the most recent.
    fn agent_at(&self, path: &str) -> Option<&'a Agent> {
        let here = |a: &&Agent| a.worktree.as_deref().unwrap_or(&a.cwd) == path || a.adopted.iter().any(|p| p == path) || a.also.iter().any(|p| p == path);
        let rank = |a: &Agent| u8::from(a.state != AgentState::Gone);
        self.s.agents.iter().filter(here).max_by(|x, y| rank(x).cmp(&rank(y)).then_with(|| x.created_at.cmp(&y.created_at)).then(std::cmp::Ordering::Greater))
    }

    /// A side's name: its agent's title, otherwise its branch, otherwise its folder's name.
    fn side_name(&self, path: &str, branch: Option<&str>) -> String {
        let title = self.agent_at(path).map(|a| a.title.as_str()).filter(|t| !t.is_empty());
        title.or(branch.filter(|b| !b.is_empty())).or(Some(base_name(path)).filter(|b| !b.is_empty())).unwrap_or(path).to_string()
    }

    /// Two agents (or worktrees) on the same lines (or the same file, `[agents.overlaps] alert`):
    /// one alert per pair, gone with the overlap. After a merge, the one left has to rebase.
    pub fn overlap_alerts(&self) -> Vec<Alert> {
        let cfg = &self.s.agents_settings.overlaps;
        if !cfg.enabled || cfg.alert == "off" {
            return Vec::new();
        }
        let lines_only = cfg.alert == "lines";
        self.s
            .overlaps
            .pairs
            .iter()
            .filter(|o| self.prefs.overlaps_ignored.get(&format!("{}|{}|{}", o.repo, o.a, o.b)) != Some(&o.key) && (!lines_only || o.level == OverlapLevel::Lines))
            .map(|o| {
                let (a, b) = (self.side_name(&o.a, o.a_branch.as_deref()), self.side_name(&o.b, o.b_branch.as_deref()));
                let files: Vec<_> = o.files.iter().filter(|f| f.level == o.level).collect();
                let file = if files.len() == 1 { files[0].path.clone() } else { tn!(files.len(), "{n} file", "{n} files") };
                let text = if o.merged {
                    tr!("“{b}” has to rebase on “{a}” (merged): they change the same lines of {file}", a = a, b = b, file = file)
                } else if o.level == OverlapLevel::Lines {
                    tr!("“{a}” and “{b}” change the same lines of {file}", a = a, b = b, file = file)
                } else {
                    tr!("“{a}” and “{b}” change the same file: {file}", a = a, b = b, file = file)
                };
                let severity = if o.level == OverlapLevel::Lines { Severity::Ko } else { Severity::Warn };
                Alert::new("Claude", severity, text).rule(AlertRule::Overlap).short(tr("overlap")).fixes(vec![Fix::Overlap { id: format!("{}|{}|{}", o.repo, o.a, o.b), side: Side::B }])
            })
            .collect()
    }

    /* ---------- Reviews and my MRs ---------- */

    /// A review asked of me, waiting beyond `[alerts] review_wait_hours`.
    pub fn review_alerts(&self) -> Vec<Alert> {
        self.open_mrs()
            .filter(|m| awaits_my_review(m, self.me_on(m)) && self.minutes_since(Some(m.updated_at)) > f64::from(self.cfg.review_wait_hours) * 60.0)
            .map(|m| {
                let author = if m.author_name.is_empty() { &m.author } else { &m.author_name };
                let mut fixes = vec![Self::mr_fix(m)];
                fixes.extend(self.claude_fix(m, FixAction::Review));
                Alert::new("Review", Severity::Info, tr!("{mr} by {author} awaits your review", mr = self.mr_key(m), author = author)).since(Some(m.updated_at)).short(tr("your review")).url(m.url.clone()).open(Self::mr_open(m)).fixes(fixes)
            })
            .collect()
    }

    /// Out of draft with nobody to review it, `[alerts] no_reviewer_minutes` after it opened (0:
    /// never). Reviewed already counts, even by someone not asked.
    pub fn lacks_reviewer(&self, m: &MergeRequest) -> bool {
        let grace = self.cfg.no_reviewer_minutes;
        grace > 0 && !m.draft && m.reviewers.is_empty() && m.approved_by.is_empty() && m.changes_requested_by.is_empty() && m.created_at.is_some() && self.minutes_since(m.created_at) > f64::from(grace)
    }

    /// Review threads left unresolved on my MR (`[alerts] open_threads`).
    pub fn has_open_threads(&self, m: &MergeRequest) -> bool {
        self.cfg.open_threads && m.state == MrState::Opened && m.open_threads > 0
    }

    /// My open MRs: CI failed, in conflict, a rebase failed or to do, only its CI left, no
    /// reviewer, threads unresolved, no ticket id.
    pub fn my_mr_alerts(&self) -> Vec<Alert> {
        let mut out = Vec::new();
        for m in self.open_mrs().filter(|m| self.mine(m)) {
            let key = self.mr_key(m);
            let mr = |rule: AlertRule, short: String, severity: Severity, text: String| Alert::new("MR", severity, text).rule(rule).short(short).url(m.url.clone()).open(Self::mr_open(m));
            if m.pipeline == Some(PipelineStatus::Failed) {
                let mut fixes = match &m.head_pipeline {
                    Some(p) => vec![Fix::Logs { pipeline: p.id, label: key.clone(), project: m.project.clone() }, Fix::Retry { pipeline: p.id, label: key.clone(), project: None }],
                    None => Vec::new(),
                };
                fixes.extend(self.claude_fix(m, FixAction::FixPipeline));
                out.push(mr(AlertRule::MrCi, tr("CI failed").into(), Severity::Warn, tr!("{mr}: CI failed", mr = key)).fixes(fixes));
            }
            let rebase = || {
                let mut fixes: Vec<Fix> = self.claude_fix(m, FixAction::Rebase).into_iter().collect();
                fixes.push(Self::mr_fix(m));
                fixes
            };
            // In conflict: an agent rebases it and resolves the conflicts, where the branch has a worktree to go to.
            if m.conflicts {
                out.push(mr(AlertRule::Conflicts, tr("conflicts").into(), Severity::Warn, tr!("{mr}: conflicts with {target}", mr = key, target = m.target_branch)).fixes(rebase()));
            } else if rebase_failed(m) {
                // The forge could not rebase it: locally, by an agent, where the conflicting commit shows.
                let text = tr!("{mr}: {forge} could not rebase it onto {target} ({reason})", mr = key, forge = self.s.forge.label(), target = m.target_branch, reason = rebase_failure(m));
                out.push(mr(AlertRule::RebaseFailed, tr("rebase failed").into(), Severity::Warn, text).fixes(rebase()));
            } else if m.merge_status == "need_rebase" && !m.rebase_in_progress {
                // Only when the rebase is the last blocker: in review, a branch behind is normal.
                out.push(mr(AlertRule::NeedRebase, tr("to rebase").into(), Severity::Info, tr!("{mr} is ready, rebase it onto {target}", mr = key, target = m.target_branch)).fixes(vec![Fix::Rebase { iid: m.iid, repo: m.repo.clone() }]));
            }
            // Only its CI left: auto-merge, rather than coming back once it is green. In the merge queue too.
            if only_waits_for_ci(m) {
                out.push(mr(AlertRule::OnlyCi, tr("only waits for its CI").into(), Severity::Info, tr!("{mr} only waits for its CI: set auto-merge", mr = key)).fixes(vec![Fix::Automerge { iid: m.iid, repo: m.repo.clone() }]));
            }
            if self.lacks_reviewer(m) {
                out.push(mr(AlertRule::NoReviewer, tr("no reviewer").into(), Severity::Warn, tr!("{mr} has no reviewer", mr = key)).since(m.created_at).fixes(vec![Self::mr_fix(m)]));
            }
            if self.has_open_threads(m) {
                let mut fixes = vec![Self::mr_fix(m)];
                fixes.extend(self.claude_fix(m, FixAction::ReviewFix));
                let text = tn!(m.open_threads, "{mr}: {n} unresolved thread", "{mr}: {n} unresolved threads", mr = key);
                out.push(mr(AlertRule::OpenThreads, tn!(m.open_threads, "{n} thread", "{n} threads"), Severity::Warn, text).fixes(fixes));
            }
            // Not every repository (infra, helm…) has tickets: only those with `[[repos]] ticket_required`.
            if some(&m.ticket).is_none() && self.repo_of(m.repo.as_deref()).is_some_and(|r| r.ticket_required) && !self.c.ticket_patterns.is_empty() {
                out.push(mr(AlertRule::NoTicket, tr("no ticket id").into(), Severity::Info, tr!("{mr} has no {prefix}- id in its title or branch", mr = key, prefix = self.c.ticket_prefix)));
            }
        }
        out
    }
}
