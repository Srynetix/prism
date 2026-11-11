//! The rules, as the window's tests had them (`web/src/rules.test.ts`), and each producer.

use chrono::{DateTime, Duration, TimeZone, Utc};

use super::rules::{access_alert, awaits_my_review, can_queue, is_approved, only_waits_for_ci, Ctx};
use super::*;
use crate::agents::provider::Provider as Cli;
use crate::config::RepoInfo;
use crate::state::*;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 9, 12, 0, 0).unwrap()
}

fn ago(minutes: i64) -> DateTime<Utc> {
    now() - Duration::minutes(minutes)
}

fn mr(f: impl FnOnce(&mut MergeRequest)) -> MergeRequest {
    let mut m = MergeRequest { iid: 1, title: "feat: x".into(), state: MrState::Opened, source_branch: "feat/x".into(), target_branch: "main".into(), author: "me".into(), author_name: "Me".into(), pipeline: Some(PipelineStatus::Success), merge_status: "mergeable".into(), updated_at: ago(60), url: "https://forge/mr/1".into(), ..Default::default() };
    f(&mut m);
    m
}

fn ticket(id: &str, status: &str) -> Ticket {
    Ticket {
        id: id.into(),
        task_id: format!("t{id}"),
        parent: None,
        priority: None,
        task_type: None,
        task_type_icon: None,
        list: None,
        repo: None,
        description: None,
        time_estimate: None,
        time_spent: None,
        tags: Vec::new(),
        name: format!("Ticket {id}"),
        status: status.into(),
        closed: false,
        assignees: Vec::new(),
        url: format!("https://tracker/{id}"),
        created_at: None,
        updated_at: Some(ago(60)),
        fields: Vec::new(),
        waiting_on: Vec::new(),
        blocking: Vec::new(),
    }
}

fn status(s: &str) -> StatusOption {
    StatusOption { status: s.into(), color: None, closed: false }
}

fn sprint(tickets: Vec<Ticket>) -> Sprint {
    Sprint { name: "S1".into(), list_id: "l".into(), url: String::new(), start: None, end: None, tickets, statuses: ["to do", "in progress", "review", "complete"].map(status).to_vec(), boards: Vec::new() }
}

fn session(f: impl FnOnce(&mut ClaudeSession)) -> ClaudeSession {
    let mut c = ClaudeSession { id: "s".into(), title: "t".into(), folder: String::new(), worktree: None, branch: None, ticket: None, status: SessionStatus::NeedsYou, needs_action: None, last_activity: Some(ago(30)), group: None, group_order: None, order_in_group: None, origin: SessionOrigin::Agent, transcript: None, path: None };
    f(&mut c);
    c
}

fn agent(id: &str, f: impl FnOnce(&mut Agent)) -> Agent {
    let mut a = Agent {
        id: id.into(),
        provider: Cli::Claude,
        account: None,
        title: format!("Agent {id}"),
        category: None,
        tags: Vec::new(),
        cwd: format!("/w/{id}"),
        worktree: None,
        worktree_note: None,
        adopted: Vec::new(),
        also: Vec::new(),
        branch: None,
        ticket: None,
        ticket_linked: false,
        mr: None,
        prompt: None,
        launch: None,
        created_at: ago(120),
        state: AgentState::Running,
        exit_code: None,
        status: SessionStatus::Running,
        needs_action: None,
        suggestion: None,
        gate: None,
        last_activity: Some(ago(5)),
        context: None,
        looping: None,
        planned: false,
        usage: None,
        model: None,
        legs: Vec::new(),
    };
    f(&mut a);
    a
}

fn repo(name: &str, primary: bool) -> RepoInfo {
    RepoInfo { name: name.into(), project: None, primary, clone: true, clone_missing: false, worktrees: true, merge_queue: false, ticket_required: false, pipelines: false, agent: None, forge: None }
}

fn snap() -> Snapshot {
    Snapshot { me: Some("me".into()), repositories: vec![repo("shop", true)], ..Default::default() }
}

fn run(s: &Snapshot) -> Vec<Alert> {
    compute(s, &Prefs::default(), now())
}

fn texts(list: &[Alert]) -> Vec<&str> {
    list.iter().map(|a| a.text.as_str()).collect()
}

#[test]
fn keeps_out_of_the_merge_queue_an_mr_its_job_could_not_rebase() {
    assert!(can_queue(&mr(|_| {})));
    assert!(!can_queue(&mr(|m| m.conflicts = true)));
    assert!(!can_queue(&mr(|m| m.merge_error = Some("Rebase failed: conflicts".into()))));
    assert!(!can_queue(&mr(|m| m.draft = true)));
    assert!(!can_queue(&mr(|m| m.approvals_left = 1)));
}

#[test]
fn tells_an_mr_that_only_waits_for_its_ci() {
    let running = |f: fn(&mut MergeRequest)| {
        mr(|m| {
            m.pipeline = Some(PipelineStatus::Running);
            f(m);
        })
    };
    assert!(only_waits_for_ci(&running(|_| {})));
    assert!(only_waits_for_ci(&running(|m| m.labels = vec!["queue".into()])));
    assert!(!only_waits_for_ci(&running(|m| m.auto_merge = true)));
    assert!(!only_waits_for_ci(&running(|m| m.needs_rebase = true)));
    assert!(!only_waits_for_ci(&mr(|_| {})));
}

#[test]
fn changes_requested_keep_an_mr_from_approved_and_from_my_review() {
    let asked = |m: &mut MergeRequest| {
        m.reviewers = vec!["sam".into()];
        m.approved_by = vec!["jo".into()];
        m.changes_requested_by = vec!["sam".into()];
    };
    assert!(is_approved(&mr(|m| m.approved_by = vec!["jo".into()])));
    assert!(!is_approved(&mr(asked)));
    assert!(!awaits_my_review(
        &mr(|m| {
            m.author = "rin".into();
            asked(m);
        }),
        Some("sam")
    ));
    assert!(awaits_my_review(
        &mr(|m| {
            m.author = "rin".into();
            m.reviewers = vec!["sam".into()];
        }),
        Some("sam")
    ));
}

#[test]
fn lets_an_agents_plain_turn_end_go_after_a_short_while() {
    let s = snap();
    let prefs = Prefs::default();
    let x = Ctx::new(&s, &prefs, now());
    assert!(x.waiting_alert(&session(|c| c.last_activity = Some(ago(5)))));
    assert!(!x.waiting_alert(&session(|_| {})));
    // A question, a permission or a Desktop session: for hours.
    assert!(x.waiting_alert(&session(|c| c.needs_action = Some("Answer its question".into()))));
    assert!(x.waiting_alert(&session(|c| c.status = SessionStatus::WaitingTool)));
    assert!(x.waiting_alert(&session(|c| c.origin = SessionOrigin::Desktop)));
}

#[test]
fn flags_an_mr_out_of_draft_with_nobody_to_review_it() {
    let s = snap();
    let prefs = Prefs::default();
    let x = Ctx::new(&s, &prefs, now());
    let opened = |minutes: i64, f: fn(&mut MergeRequest)| {
        mr(|m| {
            m.created_at = Some(ago(minutes));
            f(m);
        })
    };
    assert!(x.lacks_reviewer(&opened(30, |_| {})));
    assert!(!x.lacks_reviewer(&opened(2, |_| {})));
    assert!(!x.lacks_reviewer(&opened(30, |m| m.draft = true)));
    assert!(!x.lacks_reviewer(&opened(30, |m| m.reviewers = vec!["bob".into()])));
    assert!(!x.lacks_reviewer(&opened(30, |m| m.approved_by = vec!["bob".into()])));
    assert!(!x.lacks_reviewer(&opened(30, |m| m.changes_requested_by = vec!["bob".into()])));
    assert!(!x.lacks_reviewer(&mr(|m| m.created_at = None)));
}

#[test]
fn flags_an_open_mr_with_threads_left_unresolved() {
    let s = snap();
    let prefs = Prefs::default();
    let x = Ctx::new(&s, &prefs, now());
    assert!(x.has_open_threads(&mr(|m| m.open_threads = 2)));
    assert!(!x.has_open_threads(&mr(|m| m.open_threads = 0)));
    assert!(!x.has_open_threads(&mr(|m| {
        m.open_threads = 2;
        m.state = MrState::Merged;
    })));
}

#[test]
fn says_a_token_is_missing_or_refused_and_where_to_paste_it() {
    let a = access_alert("clickup", "missing token: set CLICKUP_TOKEN or the \"prism-clickup\" keychain entry").unwrap();
    assert!(a.text.starts_with("ClickUp: no token yet"));
    assert_eq!(a.fixes, Some(vec![Fix::Settings { section: "clickup".into(), label: "Settings › ClickUp".into() }]));
    assert!(access_alert("jira queues", "Jira /rest/api/3/search: HTTP 401 Unauthorized").unwrap().text.starts_with("Jira refused the token"));
    assert_eq!(access_alert("clickup", "ClickUp /team: HTTP 403 Forbidden").unwrap().severity, Severity::Ko);
    let forge = access_alert("gitlab", "glab api: 401 Unauthorized").unwrap();
    assert!(matches!(&forge.fixes.unwrap()[0], Fix::Settings { section, .. } if section == "forge"));
    assert!(access_alert("clickup", "ClickUp /team: HTTP 500").is_none());
    assert!(access_alert("clickup", "ClickUp /team: HTTP 4010").is_none());
    assert!(access_alert("kube", "missing token").is_none());
}

#[test]
fn sources_unreachable_and_the_config() {
    let mut s = snap();
    s.errors.insert("argocd", "timeout".into());
    s.errors.insert("usage", "quiet".into());
    s.errors.insert("config", "no config yet".into());
    let list = run(&s);
    assert_eq!(texts(&list), ["argocd unreachable: timeout", "Config: no config yet"]);
    assert!(list.iter().all(|a| a.rule == Some(AlertRule::Source) && a.kind == "Source"));
    assert_eq!(list[1].severity, Severity::Warn);
}

#[test]
fn my_mrs_failing_in_conflict_and_waiting() {
    let mut s = snap();
    s.merge_requests = vec![
        mr(|m| {
            m.iid = 7;
            m.pipeline = Some(PipelineStatus::Failed);
            m.head_pipeline = Some(Pipeline { id: 99, repo: None, project: None, target: "feat/x".into(), status: PipelineStatus::Failed, url: String::new(), updated_at: ago(1), failed_jobs: Vec::new(), stages: Vec::new() });
            m.conflicts = true;
            m.open_threads = 1;
        }),
        mr(|m| {
            m.iid = 8;
            m.repo = Some("helm".into());
            m.pipeline = Some(PipelineStatus::Running);
            m.reviewers = vec!["bob".into()];
        }),
        mr(|m| {
            m.iid = 9;
            m.author = "you".into();
            m.merge_error = Some("Rebase failed: conflicts".into());
        }),
    ];
    let list = run(&s);
    assert_eq!(texts(&list), ["!7: CI failed", "!7: conflicts with main", "!7: 1 unresolved thread", "helm!8 only waits for its CI: set auto-merge"]);
    assert_eq!(list[0].rule, Some(AlertRule::MrCi));
    assert_eq!(list[0].fixes.as_ref().unwrap().len(), 3, "logs, retry, the agent's fix");
    assert_eq!(list[1].fixes, Some(vec![Fix::Claude { iid: 7, repo: None, action: FixAction::Rebase }, Fix::Mr { iid: 7, repo: None }]));
    assert_eq!(list[2].short.as_deref(), Some("1 thread"));
    // helm is no repository of the config: no worktree, no agent's fix.
    assert_eq!(list[3].fixes, Some(vec![Fix::Automerge { iid: 8, repo: Some("helm".into()) }]));
    assert_eq!(list[3].open, Some(OpenTarget::Mr { iid: 8, repo: Some("helm".into()) }));
}

#[test]
fn a_rebase_the_forge_could_not_do_and_a_review_waiting() {
    let mut s = snap();
    s.merge_requests = vec![
        mr(|m| m.merge_error = Some("Rebase failed: Rebase locally, resolve all conflicts.".into())),
        mr(|m| {
            m.iid = 2;
            m.author = "rin".into();
            m.author_name = "Rin".into();
            m.reviewers = vec!["me".into()];
            m.approvals_left = 1;
            m.updated_at = ago(5 * 60);
        }),
    ];
    let list = run(&s);
    assert_eq!(texts(&list), ["!2 by Rin awaits your review", "!1: GitLab could not rebase it onto main (Rebase locally, resolve all conflicts.)"]);
    assert_eq!(list[0].rule, Some(AlertRule::Review));
    assert_eq!(list[0].since, Some(ago(5 * 60)));
    assert_eq!(list[1].rule, Some(AlertRule::RebaseFailed));
}

#[test]
fn an_mr_with_no_ticket_where_one_is_required() {
    let mut s = snap();
    s.repositories[0].ticket_required = true;
    s.conventions.ticket_prefix = "ABC".into();
    s.conventions.ticket_patterns = vec!["ABC".into()];
    s.merge_requests = vec![mr(|_| {}), mr(|m| {
        m.iid = 2;
        m.ticket = Some("ABC-1".into());
    })];
    assert_eq!(texts(&run(&s)), ["!1 has no ABC- id in its title or branch"]);
    s.conventions.ticket_patterns.clear();
    assert!(run(&s).is_empty());
}

#[test]
fn tickets_against_their_mrs() {
    let mut s = snap();
    s.conventions.review_statuses = vec!["review".into()];
    s.conventions.in_progress_statuses = vec!["in progress".into()];
    s.conventions.before_dev_statuses = vec!["to do".into()];
    s.conventions.after_merge_statuses = vec!["complete".into()];
    s.sprint = Some(sprint(vec![ticket("ABC-1", "review"), ticket("ABC-2", "to do"), ticket("ABC-3", "review"), ticket("ABC-4", "in progress")]));
    s.merge_requests = vec![
        mr(|m| {
            m.iid = 1;
            m.state = MrState::Merged;
            m.tickets = vec!["ABC-1".into()];
        }),
        mr(|m| {
            m.iid = 2;
            m.tickets = vec!["ABC-2".into()];
            m.author = "you".into();
        }),
    ];
    s.sprint.as_mut().unwrap().tickets[3].updated_at = Some(ago(5 * 1440));
    let list = run(&s);
    assert_eq!(texts(&list), ["!1 merged — ABC-1 still “review”", "!2 open — ABC-2 still “to do”", "ABC-3 is “review” with no open MR", "ABC-4 “in progress” for over 3 days with no branch or MR"]);
    assert_eq!(list[0].fixes, Some(vec![Fix::Status { task_id: "tABC-1".into(), ticket: "ABC-1".into(), status: "complete".into(), statuses: s.sprint.as_ref().unwrap().statuses.clone() }]));
    assert_eq!(list[1].fixes.as_ref().unwrap().len(), 1);
    assert!(matches!(&list[1].fixes.as_ref().unwrap()[0], Fix::Status { status, .. } if status == "review"));
    assert_eq!(list[0].open, Some(OpenTarget::Ticket { id: "ABC-1".into() }));
    assert!(list.iter().all(|a| a.rule == Some(AlertRule::Sync)));
    // Before the forge's first read, nothing.
    s.me = None;
    assert!(run(&s).is_empty());
}

#[test]
fn a_draft_in_review_or_approved_moves_its_ticket() {
    let mut s = snap();
    s.conventions.review_statuses = vec!["review".into()];
    s.conventions.in_progress_statuses = vec!["in progress".into()];
    s.conventions.approved_statuses = vec!["to merge".into()];
    let mut sp = sprint(vec![ticket("ABC-1", "to do"), ticket("ABC-2", "in progress"), ticket("ABC-3", "review")]);
    sp.statuses.push(status("to merge"));
    s.sprint = Some(sp);
    s.merge_requests = vec![
        mr(|m| {
            m.iid = 1;
            m.draft = true;
            m.tickets = vec!["ABC-1".into()];
        }),
        mr(|m| {
            m.iid = 2;
            m.tickets = vec!["ABC-2".into()];
        }),
        mr(|m| {
            m.iid = 3;
            m.tickets = vec!["ABC-3".into()];
            m.approved_by = vec!["bob".into()];
        }),
    ];
    let list = run(&s);
    assert_eq!(texts(&list), ["!1 is a draft — ABC-1 is “to do”, not “in progress”", "!2 in review — ABC-2 still “in progress”, not “review”", "!3 approved — ABC-3 still “review”, not “to merge”"]);
}

#[test]
fn a_ticket_to_merge_whose_mr_is_not_in_the_queue() {
    let mut s = snap();
    s.repositories[0].merge_queue = true;
    s.merge_queue_label = Some("queue".into());
    s.conventions.merge_queue_statuses = vec!["to merge".into()];
    s.sprint = Some(sprint(vec![ticket("ABC-1", "to merge")]));
    s.merge_requests = vec![mr(|m| {
        m.tickets = vec!["ABC-1".into()];
        m.approvals_left = 1;
        m.author = "you".into();
    })];
    let list = run(&s);
    assert_eq!(texts(&list), ["ABC-1 “to merge” — !1 not in the merge queue (approval first)"]);
    assert_eq!(list[0].rule, Some(AlertRule::Queue));
    assert_eq!(list[0].fixes, Some(vec![Fix::Mr { iid: 1, repo: None }]));
    s.merge_requests[0].labels = vec!["queue".into()];
    assert!(run(&s).is_empty());
}

#[test]
fn a_protected_branch_failing() {
    let mut s = snap();
    s.pipelines = vec![Pipeline {
        id: 5,
        repo: Some("helm".into()),
        project: Some("g/helm".into()),
        target: "main".into(),
        status: PipelineStatus::Failed,
        url: "https://p/5".into(),
        updated_at: ago(1),
        failed_jobs: ["a", "b", "c", "d"].map(String::from).to_vec(),
        stages: vec![Stage { name: "test".into(), status: PipelineStatus::Failed, jobs: 4, finished: 4, failed: 4, failed_jobs: Vec::new(), manual_jobs: Vec::new(), played_jobs: Vec::new(), started_at: None, finished_at: None }],
    }];
    let list = run(&s);
    assert_eq!(texts(&list), ["helm · main failed at stage test: a, b, c"]);
    assert_eq!(list[0].rule, Some(AlertRule::BranchCi));
    assert_eq!(serde_json::to_value(&list[0].fixes).unwrap(), serde_json::json!([{"kind": "logs", "pipeline": 5, "label": "helm · main", "project": "g/helm"}, {"kind": "retry", "pipeline": 5, "label": "helm · main"}, {"kind": "fix-branch", "pipeline": 5}]));
}

fn pod(name: &str, role: PodRole, healthy: bool, status: &str) -> PodView {
    PodView { target: "ci".into(), context: "k".into(), namespace: "runners".into(), name: name.into(), workload: None, role, is_job: role == PodRole::Job, job_name: None, job_ref: None, job_url: None, cpu_limit_millis: None, mem_limit_bytes: None, status: status.into(), healthy, restarts: 0, started_at: None, node: None, last_exit: None, last_exit_at: None, unscheduled: None }
}

#[test]
fn pods_and_jobs_stuck_with_their_runner_down() {
    let mut s = snap();
    s.pods = vec![pod("runner-1", PodRole::Runner, false, "CrashLoopBackOff"), pod("job-1", PodRole::Job, true, "Running")];
    s.pods[0].restarts = 3;
    s.pending_jobs = vec![PendingJob { repo: None, name: "build".into(), target: "ci".into(), queued_since: ago(12) }, PendingJob { repo: None, name: "lint".into(), target: "ci".into(), queued_since: ago(2) }];
    let list = run(&s);
    assert_eq!(texts(&list), ["1 job queued for 12 min — runner runner-1 is CrashLoopBackOff", "Runner runner-1 is CrashLoopBackOff (3 restarts)"]);
    assert_eq!(list[0].notify, Some(false));
    assert_eq!(list[1].notify, Some(true));
    assert!(matches!(&list[1].fixes.as_ref().unwrap()[0], Fix::Investigate { issue, .. } if issue == "pod runner-1 is CrashLoopBackOff (3 restarts)"));
    // No runner left in its target: its jobs wait for nothing.
    s.pods.remove(0);
    s.pending_jobs.clear();
    assert_eq!(texts(&run(&s)), ["No GitLab runner pod left in ci, only its CI jobs"]);
}

fn argo_app(name: &str, f: impl FnOnce(&mut ArgoApp)) -> ArgoApp {
    let mut a = ArgoApp { name: name.into(), env: Some("staging".into()), group: None, app: Some("orders".into()), customer: false, namespace: Some("shop".into()), sync: "Synced".into(), health: "Healthy".into(), revision: None, operation: None, message: None, synced_at: None, images: Vec::new(), deployments: vec!["orders-api".into()], started_at: None, automated: true, sync_resources: Vec::new(), drifting: Vec::new(), history: Vec::new() };
    f(&mut a);
    a
}

#[test]
fn argo_applications_degraded_failed_or_out_of_sync() {
    let mut s = snap();
    s.environments = Some(Environments { envs: vec![EnvStatus { env: "staging".into(), context: "gke".into(), cluster: "c".into(), namespace: "shop".into(), error: None, workloads: Vec::new() }], clusters: Vec::new() });
    s.blocks.insert("changes".into(), serde_json::json!({}));
    s.argocd = Some(ArgoCd {
        instances: vec![ArgoInstance {
            cluster: "c".into(),
            namespace: "argocd".into(),
            url: Some("https://argo/".into()),
            envs: None,
            error: None,
            apps: vec![
                argo_app("orders-staging", |a| {
                    a.health = "Degraded".into();
                    a.message = Some("probe failed".into());
                }),
                argo_app("pay-staging", |a| {
                    a.sync = "OutOfSync".into();
                    a.synced_at = Some(ago(20));
                }),
                argo_app("cart-staging", |a| {
                    a.sync = "OutOfSync".into();
                    a.synced_at = Some(ago(2));
                }),
            ],
        }],
    });
    let list = run(&s);
    assert_eq!(texts(&list), ["orders-staging is Degraded: probe failed", "pay-staging out of sync with Git since 20 min"]);
    assert_eq!(list[0].url.as_deref(), Some("https://argo/applications/orders-staging"));
    assert_eq!(list[0].open, Some(OpenTarget::Argo { cluster: "c".into(), app: "c/argocd/orders-staging".into() }));
    let kinds: Vec<_> = list[0].fixes.as_ref().unwrap().iter().map(|f| serde_json::to_value(f).unwrap()["kind"].as_str().unwrap().to_string()).collect();
    assert_eq!(kinds, ["podlogs", "investigate", "changes"]);
    assert_eq!(list[1].notify, Some(false));
    assert!(list.iter().all(|a| a.rule == Some(AlertRule::Argo)));
}

#[test]
fn agents_waiting_costing_looping_and_at_their_limit() {
    let mut s = snap();
    s.agents_settings.claude.cost_alert = 5.0;
    s.sessions = vec![session(|c| {
        c.id = "x".into();
        c.title = "Fix login".into();
        c.needs_action = Some("Answer its question".into());
    })];
    s.agents = vec![
        agent("x", |a| {
            a.title = "Fix login".into();
            a.status = SessionStatus::NeedsYou;
            a.usage = Some(crate::agents::usage::AgentUsage { cost: Some(6.5), ..Default::default() });
        }),
        agent("y", |a| a.looping = Some(Looping { command: "npm test".into(), count: 5 })),
    ];
    s.usage = Some(ClaudeUsage { five_hour: Some(UsageWindow { utilization: 100.0, resets_at: None }), seven_day: None, others: Vec::new(), fetched_at: ago(1) });
    let list = run(&s);
    assert_eq!(
        texts(&list),
        ["“Fix login” is waiting for your answer", "“Fix login” has cost about $6.50 (beyond $5)", "“Fix login” stopped at its plan’s limit: another CLI can take it on", "“Agent y” is going in circles: npm test failed 5 times in a row"]
    );
    assert_eq!(list[0].rule, Some(AlertRule::Question));
    assert_eq!(list[0].answer.as_deref(), Some("x"));
    assert_eq!(list[1].rule, Some(AlertRule::AgentCost));
    assert_eq!(list[2].fixes, Some(vec![Fix::Switch { agent: "x".into() }]));
    // A turn just over, asking nothing: Your turn; a permission: its own line.
    s.agents.clear();
    s.usage = None;
    s.sessions = vec![session(|c| c.last_activity = Some(ago(5))), session(|c| c.status = SessionStatus::WaitingTool)];
    let list = run(&s);
    assert_eq!(texts(&list), ["“t” is stuck on a tool or a permission prompt", "1 session is waiting for your answer"]);
    assert_eq!(list[1].rule, Some(AlertRule::YourTurn));
}

#[test]
fn a_proposal_waits_for_your_answer() {
    let mut s = snap();
    s.agents = vec![agent("x", |a| a.title = "Tidy".into())];
    s.mcp.proposals = vec![crate::mcp::Proposal { id: 3, kind: crate::mcp::ProposalKind::Config, client: "claude".into(), agent: Some("x".into()), why: "  ".into(), at: ago(3), text: String::new(), diff: Vec::new(), touches: Vec::new(), overrides: Vec::new(), stale: false, test: None, block: None, warnings: Vec::new(), client_id: None }];
    let list = run(&s);
    assert_eq!(texts(&list), ["“Tidy” proposes a change of Prism: #3"]);
    assert_eq!((list[0].rule, list[0].notify, list[0].since), (Some(AlertRule::Proposal), Some(false), Some(ago(3))));
}

fn overlap(level: OverlapLevel, b: &str) -> Overlap {
    Overlap { repo: "shop".into(), base: "main".into(), a: "/w/a".into(), b: b.into(), a_branch: Some("agent/a".into()), b_branch: Some("agent/b".into()), level, files: vec![OverlapFile { path: "bar.rs".into(), level, a: vec![LineRange { start: 92, len: 6 }], b: vec![LineRange { start: 95, len: 1 }] }], merged: false, key: "k".into() }
}

#[test]
fn overlaps_from_the_alert_setting_and_not_ignored() {
    let mut s = snap();
    s.agents = vec![agent("x", |a| {
        a.title = "Agent A".into();
        a.cwd = "/w/a".into();
    })];
    s.overlaps.pairs = vec![overlap(OverlapLevel::Lines, "/w/b"), overlap(OverlapLevel::File, "/w/c")];
    let list = run(&s);
    assert_eq!(texts(&list), ["“Agent A” and “agent/b” change the same lines of bar.rs"]);
    assert_eq!(list[0].severity, Severity::Ko);
    assert_eq!(list[0].fixes, Some(vec![Fix::Overlap { id: "shop|/w/a|/w/b".into(), side: Side::B }]));
    s.agents_settings.overlaps.alert = "file".into();
    assert_eq!(run(&s).len(), 2);
    s.agents_settings.overlaps.alert = "off".into();
    assert!(run(&s).is_empty());
    s.agents_settings.overlaps.alert = "lines".into();
    let prefs = Prefs::of(&serde_json::json!({ "overlaps.ignored": { "shop|/w/a|/w/b": "k" } }));
    assert!(compute(&s, &prefs, now()).is_empty());
}

fn worktree(path: &str, f: impl FnOnce(&mut Worktree)) -> Worktree {
    let mut w = Worktree { repo: "shop".into(), path: path.into(), branch: Some(format!("b{path}")), ticket: None, base: "main".into(), behind: 0, ahead: 0, dirty: false, changes: 0, untracked: None, size_bytes: None, unpushed: 0, upstream: None, upstream_gone: false, unpulled: 0, head: Some("abc".into()), last_commit: Some(ago(30 * 1440)), is_main: false, prunable: false, locked: false };
    f(&mut w);
    w
}

#[test]
fn worktrees_stale_safe_to_remove_gone_or_archived() {
    let mut s = snap();
    s.worktree_stale_days = 14;
    s.worktrees = vec![
        worktree("/w/main", |w| w.is_main = true),
        worktree("/w/old", |_| {}),
        worktree("/w/dirty", |w| w.changes = 2),
        worktree("/w/gone", |w| w.prunable = true),
        worktree("/w/fresh", |w| w.last_commit = Some(ago(60))),
    ];
    let list = run(&s);
    assert_eq!(texts(&list), ["1 stale worktree can be removed safely", "1 worktree whose folder is gone (prune)"]);
    assert!(list.iter().all(|a| a.rule == Some(AlertRule::Worktree) && a.notify == Some(false)));
    // Archived as it is: left alone, back once something happens in it.
    let prefs = Prefs::of(&serde_json::json!({ "worktrees.archived": { "/w/old": "abc|0" } }));
    assert_eq!(texts(&compute(&s, &prefs, now())), ["1 worktree whose folder is gone (prune)"]);
    s.worktrees[1].head = Some("def".into());
    assert_eq!(compute(&s, &prefs, now()).len(), 2);
}

#[test]
fn a_worktree_follows_its_agents_archived() {
    let mut s = snap();
    s.worktree_stale_days = 14;
    s.worktrees = vec![worktree("/w/old", |_| {})];
    s.agents = vec![agent("a1", |a| {
        a.cwd = "/w/old/src".into();
        a.state = AgentState::Exited;
        a.status = SessionStatus::Idle;
        a.created_at = ago(40 * 1440);
        a.last_activity = Some(ago(40 * 1440));
    })];
    assert_eq!(texts(&run(&s)), ["1 stale worktree can be removed safely"]);
    let at = super::rules::iso(Some(ago(40 * 1440)));
    let prefs = Prefs::of(&serde_json::json!({ "agents.archived": { "a1": at } }));
    assert!(compute(&s, &prefs, now()).is_empty());
    // Waiting for you since it was archived: back, and its worktree with it.
    s.agents[0].status = SessionStatus::NeedsYou;
    s.agents[0].last_activity = Some(ago(30 * 1440));
    assert_eq!(compute(&s, &prefs, now()).len(), 1);
}

#[test]
fn obsidian_tickets_are_their_repositorys() {
    let mut s = snap();
    s.tracker = Some(crate::tracker::Provider::Obsidian);
    let prefs = Prefs::default();
    let x = Ctx::new(&s, &prefs, now());
    assert_eq!(x.ticket_ref("T-1", None), "shop|T-1");
    assert_eq!(x.ticket_ref("T-1", Some("helm")), "helm|T-1");
}

#[test]
fn an_mr_is_named_as_its_forge_writes_it() {
    let mut s = snap();
    s.repositories.push(RepoInfo { forge: Some(crate::forge::Provider::Github), ..repo("site", false) });
    let prefs = Prefs::default();
    let x = Ctx::new(&s, &prefs, now());
    assert_eq!(x.mr_key(&mr(|_| {})), "!1");
    assert_eq!(x.mr_key(&mr(|m| m.repo = Some("site".into()))), "site#1");
    assert_eq!(x.mr_key(&mr(|m| m.forge = Some(crate::forge::Provider::Github))), "#1");
}

#[test]
fn numbers_as_the_window_writes_them() {
    assert_eq!(to_fixed(0.25, 1), "0.3");
    assert_eq!(to_fixed(6.5, 2), "6.50");
    assert_eq!(to_fixed(29.44, 1), "29.4");
    assert_eq!(round(2.5), 3);
    assert_eq!(js_number(5.0), "5");
    assert_eq!(js_number(2.5), "2.5");
}

#[test]
fn alerts_serialize_as_the_window_reads_them() {
    let a = Alert::new("MR", Severity::Warn, "x").rule(AlertRule::MrCi).open(OpenTarget::Mr { iid: 3, repo: None }).fixes(vec![Fix::DevServer { agent: "a".into(), command: "npm run dev".into(), place: "shell 1".into(), tail: String::new() }]);
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::json!({ "kind": "MR", "severity": "warn", "text": "x", "rule": "mr-ci", "open": { "kind": "mr", "iid": 3, "repo": null }, "fixes": [{ "kind": "dev-server", "agent": "a", "command": "npm run dev", "where": "shell 1", "tail": "" }] })
    );
}
