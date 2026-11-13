//! The demo profile (`[demo] enabled`): a fictitious team's work in every block, nothing read from
//! anywhere. No collector runs: no GitLab, ClickUp, cluster nor Claude call, and the machine's own
//! Claude sessions are not read. Times are relative to the launch, so that "2 h ago" stays true.
//!
//! The team ships an online shop (`example-org/shop`, tickets `SHOP-…`): components through
//! staging, a preprod tag and production, Helm and ArgoCD, a sprint, a refinement board.
//! What the window reads on demand (an MR's diff, a job's log, a ticket…) comes from
//! [`details`]; every action that would write answers [`READ_ONLY`].

use std::{collections::BTreeMap, sync::OnceLock};

use chrono::{DateTime, Duration, Utc};

use crate::{config::RepoInfo, state::*};

use prism_agents as agents;
use prism_collectors as collectors;
use prism_core::{blocks, config, forge, profile, state, ticket, tracker};

pub mod details;

/// The launch of the demo: set when its data fills the snapshot, the clock of what it reads.
static LAUNCH: OnceLock<DateTime<Utc>> = OnceLock::new();

/// This Prism shows the demo: what the window reads on demand comes from [`details`], and
/// nothing is written anywhere.
pub fn active() -> bool {
    LAUNCH.get().is_some()
}

/// What a demo profile answers to an action that would write.
pub const READ_ONLY: &str = "The demo profile changes nothing: its data is a sample";

/// The jobs of each stage, in the order a pipeline runs them (the build, test and deploy stages).
const STAGE_JOBS: [&[&str]; 3] = [&["compile", "docker-image", "webapp-bundle"], &["lint", "typecheck", "integration-tests", "sast", "unit-tests", "e2e-checkout"], &["deploy-review"]];

/// A job's id: its pipeline's, its stage's index and its own.
fn job_id(pipeline: u64, stage: usize, job: usize) -> u64 {
    pipeline * 100 + stage as u64 * 10 + job as u64
}

/// The demo profile's config: the conventions the data follows, and its pages.
pub use crate::profile::DEMO_CONFIG as CONFIG;

const HOST: &str = "https://gitlab.example.org";
const PROJECT: &str = "example-org/shop";
const HELM: &str = "example-org/helm";
/// A personal repository on GitHub (`[[repos]] forge`).
const TOOLKIT: &str = "alex-dev/toolkit";
const CLICKUP: &str = "https://app.clickup.example.org/t";

/// Every source the boot screen and Alerts wait for: answered at once.
const SOURCES: [&str; 15] = [
    "gitlab", "gitlab repos", "worktrees", "repos", "pods", "mail", "environments", "argocd", "vulnerabilities", "renovate", "ci_health", "clickup", "clickup queues", "sessions", "usage",
];

/// Fills the snapshot with the demo data, as of `now`.
pub fn fill(hub: &Hub, now: DateTime<Utc>) {
    let d = Demo { now };
    let _ = LAUNCH.set(now);
    hub.update(|s| {
        d.write(s);
        true
    });
    crate::blocks::demo(hub, now);
    for source in SOURCES {
        hub.set_error(source, None);
    }
}

/// The branches of the demo projects, sorted by name (Settings › Repositories' followed branches).
pub const BRANCHES: &[&str] = &["develop", "env/preprod", "env/prod", "main", "release/2026.09", "release/2026.10"];

/// The demo snapshot for the window's browser preview (`web/public/dev-snapshot.json`, out of
/// git), the registered blocks' sample state included: written by the app's `dump_for_preview`.
pub fn preview(now: DateTime<Utc>) -> serde_json::Value {
    let cfg: crate::config::Config = toml::from_str(CONFIG).expect("demo config");
    let mut s = Snapshot { views: cfg.views.clone(), conventions: prism_core::conventions::Conventions::of(&cfg), ..Default::default() };
    Demo { now }.write(&mut s);
    // Its alerts, as the core computes them.
    s.alerts = prism_core::alerts::compute(&s, &Default::default(), now);
    let mut v = serde_json::to_value(&s).expect("demo snapshot");
    v["answered"] = serde_json::json!(SOURCES);
    v
}

struct Demo {
    now: DateTime<Utc>,
}

/// What the demo's agents cost (`agent_costs`): `agents`, those of its snapshot, and closed ones.
pub fn agent_costs(agents: &[Agent]) -> Vec<AgentCost> {
    Demo { now: LAUNCH.get().copied().unwrap_or_else(Utc::now) }.agent_costs(agents)
}

fn person(id: u64, name: &str) -> Person {
    let initials = name.split_whitespace().filter_map(|w| w.chars().next()).collect();
    // ClickUp's palette, one per person.
    const COLORS: [&str; 6] = ["#7b68ee", "#e5484d", "#0f9d9f", "#d6a100", "#2ea52c", "#c2185b"];
    Person { id, name: name.into(), initials, color: Some(COLORS[(id as usize) % COLORS.len()].into()), avatar: None }
}

fn status(name: &str, color: &str, closed: bool) -> StatusOption {
    StatusOption { status: name.into(), color: Some(color.into()), closed }
}

impl Demo {
    fn ago(&self, minutes: i64) -> DateTime<Utc> {
        self.now - Duration::minutes(minutes)
    }

    fn in_(&self, minutes: i64) -> DateTime<Utc> {
        self.now + Duration::minutes(minutes)
    }

    fn statuses() -> Vec<StatusOption> {
        [
            ("ready to dev", "#87909e", false),
            ("todo", "#87909e", false),
            ("doing", "#5f55ee", false),
            ("review", "#e16b16", false),
            ("to merge", "#ee5e99", false),
            ("to validate", "#0f9d9f", false),
            ("validation", "#0f9d9f", false),
            ("done", "#008844", true),
        ]
        .into_iter()
        .map(|(n, c, closed)| status(n, c, closed))
        .collect()
    }

    fn alex() -> Person {
        person(1, "Alex Martin")
    }

    fn team() -> [Person; 5] {
        [Self::alex(), person(2, "Sam Rivera"), person(3, "Rin Tanaka"), person(4, "Maya Okafor"), person(5, "Jo Weber")]
    }

    #[allow(clippy::too_many_arguments)]
    fn ticket(&self, n: u32, name: &str, status: &str, who: Option<usize>, priority: Option<&str>, kind: Option<&str>, days: i64) -> Ticket {
        let id = format!("SHOP-{n}");
        Ticket {
            waiting_on: Vec::new(),
            blocking: Vec::new(),
            task_id: format!("demo{n}"),
            parent: None,
            priority: priority.and_then(prism_core::state::Priority::of),
            task_type: kind.map(String::from),
            // As a ClickUp workspace draws them.
            task_type_icon: kind.and_then(|k| match k {
                "Bug" => Some("bug".into()),
                "EPIC" => Some("book".into()),
                _ => None,
            }),
            list: None,
            repo: None,
            description: Some(format!("{name}.\n\nAcceptance: covered by tests, documented in the changelog.")),
            time_estimate: None,
            time_spent: None,
            tags: Vec::new(),
            name: name.into(),
            status: status.into(),
            closed: status == "done",
            assignees: who.map(|i| vec![Self::team()[i].clone()]).unwrap_or_default(),
            url: format!("{CLICKUP}/{id}"),
            created_at: Some(self.ago(days * 1440 + 300)),
            updated_at: Some(self.ago(days * 600 + 20)),
            id,
            // A `Category` dropdown, as `[clickup] fields = ["Category"]` would show it.
            fields: vec![TicketField {
                estimate: false,
                name: "Category".into(),
                filter: true,
                values: vec![match n % 3 {
                    0 => FieldValue { label: "Billing".into(), color: Some("#e5484d".into()), order: Some(2) },
                    1 => FieldValue { label: "Checkout".into(), color: Some("#3e63dd".into()), order: Some(1) },
                    _ => FieldValue { label: "Catalog".into(), color: Some("#30a46c".into()), order: Some(0) },
                }],
            }],
        }
    }

    fn stage(&self, name: &str, status: &str, jobs: u32, finished: u32, minutes: i64) -> Stage {
        Stage {
            name: name.into(),
            status: PipelineStatus::of(status),
            jobs,
            finished,
            failed: 0,
            failed_jobs: Vec::new(),
            manual_jobs: Vec::new(),
            played_jobs: Vec::new(),
            started_at: Some(self.ago(minutes)),
            finished_at: (finished == jobs && status != "manual").then(|| self.ago(minutes.saturating_sub(4))),
        }
    }

    /// A manual job run successfully `minutes` ago.
    fn played(&self, id: u64, name: &str, minutes: i64) -> PlayedJob {
        PlayedJob { id, name: name.into(), url: format!("{HOST}/{PROJECT}/-/jobs/{id}"), status: PipelineStatus::Success, finished_at: Some(self.ago(minutes)) }
    }

    /// A pipeline with build, test and deploy stages, as far as `status` goes.
    fn pipeline(&self, id: u64, target: &str, status: &str, minutes: i64) -> Pipeline {
        let mut stages = vec![self.stage("build", "success", 3, 3, minutes + 12), self.stage("test", "success", 6, 6, minutes + 6)];
        match status {
            "running" => stages[1] = self.stage("test", "running", 6, 4, minutes + 6),
            "failed" => {
                stages[1] = Stage { failed: 1, failed_jobs: vec![JobLink { name: "unit-tests".into(), url: format!("{HOST}/{PROJECT}/-/jobs/{}", job_id(id, 1, 4)) }], ..self.stage("test", "failed", 6, 6, minutes + 6) };
            }
            _ => stages.push(self.stage("deploy", "success", 1, 1, minutes + 2)),
        }
        let failed_jobs = if status == "failed" { vec!["unit-tests".to_string()] } else { Vec::new() };
        Pipeline { id, repo: None, project: None, target: target.into(), status: PipelineStatus::of(status), url: format!("{HOST}/{PROJECT}/-/pipelines/{id}"), updated_at: self.ago(minutes), failed_jobs, stages }
    }

    #[allow(clippy::too_many_arguments)]
    fn mr(&self, iid: u64, title: &str, author: usize, branch: &str, ticket: Option<u32>, minutes: i64, pipeline: Option<Pipeline>, merge_status: &str) -> MergeRequest {
        let a = &Self::team()[author];
        let login = a.name.split_whitespace().next().unwrap_or_default().to_lowercase();
        MergeRequest {
            iid,
            repo: None,
            project: Some(PROJECT.into()),
            forge: None,
            title: title.into(),
            url: format!("{HOST}/{PROJECT}/-/merge_requests/{iid}"),
            state: MrState::Opened,
            draft: false,
            source_branch: branch.into(),
            target_branch: "release/2026.10".into(),
            ticket: ticket.map(|n| format!("SHOP-{n}")),
            tickets: ticket.map(|n| format!("SHOP-{n}")).into_iter().collect(),
            author: login,
            author_name: a.name.clone(),
            author_avatar: None,
            assignees: Vec::new(),
            reviewers: Vec::new(),
            approved_by: Vec::new(),
            changes_requested_by: Vec::new(),
            approvals_left: 1,
            pipeline: pipeline.as_ref().map(|p| p.status),
            merge_status: merge_status.into(),
            conflicts: false,
            needs_rebase: false,
            rebase_in_progress: false,
            merge_error: None,
            auto_merge: false,
            labels: Vec::new(),
            head_pipeline: pipeline,
            open_threads: 0,
            size: Some(MrSize { additions: 120 + iid % 7 * 40, deletions: 20 + iid % 5 * 9, files: 3 + iid % 6 }),
            // Opened a few days before its last move.
            created_at: Some(self.ago(minutes + 3 * 24 * 60)),
            updated_at: self.ago(minutes),
            merged_at: None,
        }
    }

    fn merge_requests(&self) -> Vec<MergeRequest> {
        let mut out = vec![
            MergeRequest {
                reviewers: vec!["sam".into()],
                // Sam asked for changes in its threads: my MR is back to me.
                changes_requested_by: vec!["sam".into()],
                open_threads: 2,
                ..self.mr(1288, "SHOP-412 Save the cart between sessions", 0, "feat/SHOP-412-saved-cart", Some(412), 35, Some(self.pipeline(9101, "!1288", "success", 40)), "not_approved")
            },
            MergeRequest {
                reviewers: vec!["alex".into()],
                ..self.mr(1290, "SHOP-418 Paginate the order history", 1, "feat/SHOP-418-order-pagination", Some(418), 300, Some(self.pipeline(9102, "!1290", "success", 290)), "not_approved")
            },
            MergeRequest { open_threads: 1, ..self.mr(1291, "SHOP-433 Fix VAT rounding on invoices", 0, "fix/SHOP-433-vat-rounding", Some(433), 18, Some(self.pipeline(9103, "!1291", "failed", 15)), "ci_must_pass") },
            MergeRequest {
                labels: vec!["dependencies".into()],
                approvals_left: 0,
                approved_by: vec!["jo".into()],
                author: "renovate-bot".into(),
                author_name: "Renovate Bot".into(),
                ..self.mr(1293, "Update dependency axios to v1.7.9", 4, "renovate/axios-1.x", None, 95, Some(self.pipeline(9104, "!1293", "success", 90)), "mergeable")
            },
            MergeRequest { draft: true, ..self.mr(1294, "Draft: SHOP-425 Lazy-load product images", 3, "feat/SHOP-425-lazy-images", Some(425), 50, Some(self.pipeline(9105, "!1294", "running", 3)), "draft_status") },
            MergeRequest {
                approvals_left: 0,
                approved_by: vec!["alex".into(), "sam".into()],
                ..self.mr(1292, "SHOP-427 Export orders as CSV", 2, "feat/SHOP-427-orders-csv", Some(427), 70, Some(self.pipeline(9106, "!1292", "success", 65)), "mergeable")
            },
            // The merge queue, oldest first: the job rebased the first and set its auto-merge; the
            // next waits its turn, though its priority label puts it ahead; the last one, on main
            // (its own queue), is stuck on conflicts.
            MergeRequest {
                approvals_left: 0,
                approved_by: vec!["sam".into()],
                labels: vec!["merge-queue".into()],
                auto_merge: true,
                ..self.mr(1284, "SHOP-409 Stack coupons at checkout", 1, "feat/SHOP-409-coupons", Some(409), 900, Some(self.pipeline(9107, "!1284", "running", 4)), "ci_still_running")
            },
            MergeRequest {
                approvals_left: 0,
                approved_by: vec!["alex".into()],
                labels: vec!["merge-queue".into(), "merge-priority::0".into()],
                needs_rebase: true,
                ..self.mr(1286, "SHOP-415 Show delivery slots in the cart", 3, "feat/SHOP-415-delivery-slots", Some(415), 600, Some(self.pipeline(9108, "!1286", "success", 120)), "need_rebase")
            },
            MergeRequest {
                approvals_left: 0,
                approved_by: vec!["jo".into()],
                labels: vec!["merge-queue".into()],
                conflicts: true,
                target_branch: "main".into(),
                ..self.mr(1289, "SHOP-422 Split the order service", 2, "refactor/SHOP-422-orders", Some(422), 200, Some(self.pipeline(9109, "!1289", "success", 180)), "conflict")
            },
            MergeRequest {
                repo: Some("helm".into()),
                project: Some(HELM.into()),
                url: format!("{HOST}/{HELM}/-/merge_requests/88"),
                target_branch: "main".into(),
                approvals_left: 0,
                approved_by: vec!["rin".into()],
                ..self.mr(88, "Raise orders-api memory limit to 768 Mi", 4, "orders-api-memory", None, 130, None, "mergeable")
            },
            // A personal repository on GitHub, in this GitLab profile: my pull request there, by my GitHub login.
            MergeRequest {
                repo: Some("toolkit".into()),
                project: Some(TOOLKIT.into()),
                forge: Some(crate::forge::Provider::Github),
                url: format!("https://github.com/{TOOLKIT}/pull/14"),
                target_branch: "main".into(),
                author: "alex-dev".into(),
                reviewers: Vec::new(),
                approvals_left: 0,
                auto_merge: true,
                ..self.mr(14, "Add a --dry-run flag to the sync script", 0, "feat/dry-run", None, 45, None, "mergeable")
            },
        ];
        for (iid, title, author, ticket, hours) in [(1285, "SHOP-421 Retry payment webhooks with backoff", 2, 421, 3), (1280, "SHOP-430 Search the catalog by SKU", 4, 430, 26)] {
            out.push(MergeRequest {
                state: MrState::Merged,
                approvals_left: 0,
                merged_at: Some(self.ago(hours * 60)),
                ..self.mr(iid, title, author, &format!("feat/SHOP-{ticket}"), Some(ticket), hours * 60, Some(self.pipeline(9000 + iid, &format!("!{iid}"), "success", hours * 60)), "mergeable")
            });
        }
        out
    }

    fn pipelines(&self) -> Vec<Pipeline> {
        let mut release = self.pipeline(9200, "release/2026.10", "running", 4);
        // The manual preprod and production tag jobs, which a delivery block offers as buttons.
        release.stages.push(Stage {
            manual_jobs: ["catalog-api", "orders-api", "storefront-webapp"]
                .iter()
                .enumerate()
                .map(|(i, c)| ManualJob { id: 9300 + i as u64, name: format!("tag-release({c}:preprod)"), url: format!("{HOST}/{PROJECT}/-/jobs/{}", 9300 + i) })
                .chain(std::iter::once(ManualJob { id: 9310, name: "tag-release(preprod)".into(), url: format!("{HOST}/{PROJECT}/-/jobs/9310") }))
                .collect(),
            // Tagged already (no change since): set again after a new base image.
            played_jobs: vec![self.played(9311, "tag-release(payments-api:preprod)", 1440)],
            ..self.stage("tag", "manual", 5, 1, 4)
        });
        let mut main = self.pipeline(9201, "main", "success", 25);
        main.stages.push(Stage {
            manual_jobs: vec![ManualJob { id: 9320, name: "tag-release(production)".into(), url: format!("{HOST}/{PROJECT}/-/jobs/9320") }],
            played_jobs: vec![self.played(9321, "tag-release(payments-api:production)", 1400), self.played(9322, "tag-release(storefront-webapp:production)", 6 * 1440)],
            ..self.stage("tag", "manual", 3, 2, 25)
        });
        // Followed too (`pipeline_branches = ["@main", "develop", "@release"]`): the integration branch.
        let develop = self.pipeline(9202, "develop", "success", 12);
        vec![main, develop, release]
    }

    fn environments(&self) -> (Environments, ArgoCd) {
        let workload = |name: &str, app: &str, role: &str, ready: u32, tag: &str, expected: Option<&str>, restarts: u32| Workload {
            name: name.into(),
            app: Some(app.into()),
            role: role.into(),
            desired: 2,
            ready,
            pods: 2,
            restarts,
            waiting: (ready < 2).then(|| "CrashLoopBackOff".into()),
            last_exit: (restarts > 0).then(|| "OOMKilled".into()),
            last_exit_at: (restarts > 0).then(|| self.ago(35)),
            image_tag: Some(tag.into()),
            expected: expected.map(String::from),
            drift: expected.is_some_and(|e| e != tag),
            load: WorkloadLoad { cpu_used_millis: Some(140), mem_used_bytes: Some(380 << 20), cpu_requested_millis: 250, mem_requested_bytes: 512 << 20, cpu_limit_millis: Some(1000), mem_limit_bytes: Some(768 << 20) },
            updated_at: Some(self.ago(600)),
            revision: Some(12),
        };
        let env = |env: &str, context: &str, workloads: Vec<Workload>| EnvStatus { env: env.into(), context: context.into(), cluster: context.into(), namespace: "shop".into(), error: None, workloads };
        let envs = vec![
            env("prod", "demo-prod", vec![
                workload("catalog-api", "catalog-api", "api", 2, "2.13.2", Some("2.13.2"), 0),
                workload("orders-api", "orders-api", "api", 2, "1.7.3", Some("1.7.3"), 0),
                workload("orders-api-worker", "orders-api", "worker", 2, "1.7.3", Some("1.7.3"), 0),
                workload("payments-api", "payments-api", "api", 2, "3.2.0", Some("3.2.1"), 0),
            ]),
            env("preprod", "demo-preprod", vec![
                workload("catalog-api", "catalog-api", "api", 2, "rc-2.14.0", Some("rc-2.14.0"), 0),
                workload("orders-api", "orders-api", "api", 1, "rc-1.8.0", Some("rc-1.8.0"), 3),
                workload("payments-api", "payments-api", "api", 2, "rc-3.2.1", Some("rc-3.2.1"), 0),
            ]),
            env("staging", "demo-staging", vec![
                workload("catalog-api", "catalog-api", "api", 2, "release-2026-10-4f2a9c1e", None, 0),
                workload("orders-api", "orders-api", "api", 2, "release-2026-10-4f2a9c1e", None, 0),
                workload("payments-api", "payments-api", "api", 2, "release-2026-10-4f2a9c1e", None, 0),
            ]),
        ];
        let cluster = |c: &str, used: u64| ClusterLoad { context: c.into(), cluster: c.into(), nodes: 3, cpu_used_millis: Some(used), mem_used_bytes: Some(used << 22), cpu_allocatable_millis: 12000, mem_allocatable_bytes: 48 << 30, error: None };
        let app = |name: &str, env: &str, sync: &str, health: &str, image: &str| ArgoApp {
            name: format!("{name}-{env}"),
            env: Some(env.into()),
            group: None,
            app: Some(name.into()),
            customer: false,
            namespace: Some("shop".into()),
            sync: sync.into(),
            health: health.into(),
            revision: Some("8c1d2e3f".into()),
            operation: Some(if health == "Progressing" { "Running" } else { "Succeeded" }.into()),
            message: None,
            synced_at: Some(self.ago(if sync == "OutOfSync" { 40 } else { 90 })),
            images: vec![image.into()],
            deployments: vec![name.into()],
            started_at: Some(self.ago(if health == "Progressing" { 1 } else if sync == "OutOfSync" { 41 } else { 91 })),
            automated: sync != "OutOfSync",
            sync_resources: if health == "Progressing" {
                vec![
                    ArgoResource { kind: "Job".into(), name: format!("{name}-migrate"), status: Some("Synced".into()), health: None, hook: Some("Succeeded".into()), message: Some("Reached expected number of succeeded pods".into()) },
                    ArgoResource { kind: "Deployment".into(), name: name.into(), status: Some("Synced".into()), health: None, hook: None, message: Some(format!("deployment.apps/{name} configured")) },
                    ArgoResource { kind: "ConfigMap".into(), name: format!("{name}-config"), status: Some("Synced".into()), health: None, hook: None, message: Some(format!("configmap/{name}-config unchanged")) },
                ]
            } else {
                Vec::new()
            },
            drifting: match (sync, health) {
                ("OutOfSync", _) => vec![ArgoResource { kind: "Deployment".into(), name: name.into(), status: Some("OutOfSync".into()), health: Some("Healthy".into()), hook: None, message: None }],
                (_, "Progressing") => vec![ArgoResource { kind: "Deployment".into(), name: name.into(), status: Some("Synced".into()), health: Some("Progressing".into()), hook: None, message: Some("Waiting for rollout to finish: 1 of 2 updated replicas are available...".into()) }],
                _ => Vec::new(),
            },
            history: vec![ArgoDeploy { revision: Some("8c1d2e3f".into()), deployed_at: Some(self.ago(90)) }, ArgoDeploy { revision: Some("51b0a7c2".into()), deployed_at: Some(self.ago(60 * 26)) }],
        };
        let argocd = ArgoCd {
            instances: vec![ArgoInstance {
                cluster: "demo-ops".into(),
                namespace: "argocd".into(),
                url: Some("https://argocd.example.org".into()),
                envs: None,
                error: None,
                apps: vec![
                    app("catalog-api", "prod", "Synced", "Healthy", "2.13.2"),
                    app("orders-api", "prod", "Synced", "Healthy", "1.7.3"),
                    app("payments-api", "prod", "OutOfSync", "Healthy", "3.2.0"),
                    app("catalog-api", "preprod", "Synced", "Healthy", "rc-2.14.0"),
                    app("orders-api", "preprod", "Synced", "Progressing", "rc-1.8.0"),
                    app("payments-api", "preprod", "Synced", "Healthy", "rc-3.2.1"),
                    // The platform's, which no delivery pipeline ships: one whose auto-sync failed on a hook.
                    ArgoApp {
                        namespace: Some("cert-manager".into()),
                        operation: Some("Failed".into()),
                        message: Some("one or more synchronization tasks completed unsuccessfully".into()),
                        started_at: Some(self.ago(14)),
                        synced_at: Some(self.ago(10)),
                        automated: true,
                        sync_resources: vec![ArgoResource { kind: "Job".into(), name: "cert-manager-startupapicheck".into(), status: Some("Synced".into()), health: None, hook: Some("Failed".into()), message: Some("Job has reached the specified backoff limit".into()) }],
                        drifting: vec![ArgoResource { kind: "ClusterIssuer".into(), name: "letsencrypt-prod".into(), status: Some("OutOfSync".into()), health: None, hook: None, message: None }],
                        ..app("cert-manager", "prod", "OutOfSync", "Healthy", "v1.16.1")
                    },
                    ArgoApp { namespace: Some("ingress-nginx".into()), started_at: Some(self.ago(301)), synced_at: Some(self.ago(300)), ..app("ingress-nginx", "prod", "Synced", "Healthy", "1.11.3") },
                    ArgoApp { namespace: Some("monitoring".into()), automated: false, started_at: Some(self.ago(181)), synced_at: Some(self.ago(180)), ..app("monitoring", "prod", "Synced", "Healthy", "65.1.0") },
                    // A customer's instances, outside Environments.
                    ArgoApp { name: "acme-intake-api".into(), group: Some("customers".into()), customer: true, namespace: Some("acme".into()), ..app("intake-api", "prod", "Synced", "Healthy", "2.4.0") },
                    ArgoApp { name: "globex-intake-api".into(), group: Some("customers".into()), customer: true, namespace: Some("globex".into()), ..app("intake-api", "prod", "Synced", "Healthy", "2.3.1") },
                ],
            }],
        };
        (Environments { envs, clusters: vec![cluster("demo-prod", 5200), cluster("demo-preprod", 2100), cluster("demo-staging", 3300)] }, argocd)
    }

    fn ci_health(&self) -> CiHealth {
        let day = |i: i64, pipelines: u32, success: f64| CiDay {
            date: (self.now - Duration::days(13 - i)).date_naive(),
            pipelines,
            duration: Some(11.0 + (i % 4) as f64),
            success: Some(success),
            queue: Some(0.6 + (i % 3) as f64 * 0.3),
        };
        let scope = |key: &str, label: &str, base: u32, success: f64| CiScope {
            key: key.into(),
            label: label.into(),
            pipelines: base * 7,
            pipelines_prev: base * 6,
            duration: Some(12.4),
            duration_prev: Some(13.8),
            success: Some(success),
            success_prev: Some(success - 0.03),
            queue: Some(0.9),
            queue_p90: Some(2.4),
            days: (0..14).map(|i| day(i, base + (i as u32 % 3), success - if i == 9 { 0.2 } else { 0.0 })).collect(),
            slow_jobs: vec![SlowJob { name: "e2e-checkout".into(), runs: base * 7, duration: 7.2, duration_prev: Some(6.1), days: (0..14).map(|i| Some(6.0 + (i % 5) as f64 * 0.4)).collect() }],
            flaky: vec![FlakyJob { name: "e2e-checkout".into(), flakes: 3, runs: base * 7, last_at: self.ago(180), urls: vec![format!("{HOST}/{PROJECT}/-/jobs/8800")] }],
        };
        CiHealth { window_days: 14, scopes: vec![scope("main", "main", 4, 0.96), scope("release", "release/*", 6, 0.91), scope("mr", "MRs", 22, 0.84)] }
    }

    fn vulnerabilities(&self) -> Vulnerabilities {
        let project = |p: &str, image: Option<&str>, critical: u32, high: u32, new: u32| VulnProject { project: p.into(), repo: None, base: false, image: image.map(String::from), note: None, error: None, critical, high, new, accepted: 0, scanned: None, remote: None };
        let finding = |id: &str, severity: &str, package: &str, installed: &str, fixed: &str, title: &str, projects: &[&str], new: bool| VulnFinding {
            id: id.into(),
            severity: severity.into(),
            package: package.into(),
            installed: installed.into(),
            fixed: fixed.into(),
            title: title.into(),
            url: Some(format!("https://avd.aquasec.com/nvd/{}", id.to_lowercase())),
            projects: projects.iter().map(|p| p.to_string()).collect(),
            new,
            new_in: if new { projects.iter().map(|p| p.to_string()).collect() } else { Vec::new() },
            accepted: false,
        };
        Vulnerabilities {
            branch: "release/2026.10".into(),
            sha: "4f2a9c1e".into(),
            other_branches: Vec::new(),
            trivy: "Version: 0.58.1".into(),
            scanned_at: Some(self.ago(240)),
            scanning: false,
            checking: false,
            projects: vec![
                project("catalog-api", None, 0, 1, 1),
                project("orders-api", None, 0, 0, 0),
                project("storefront-webapp", None, 1, 0, 0),
                VulnProject {
                    scanned: Some(ScannedImage {
                        reference: "registry.example.org/example-org/orders-api:1.7.3".into(),
                        digest: Some("sha256:3f2a9c1e7b4d5a6c8e0f1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f".into()),
                        pulled: None,
                        created: Some(self.ago(60 * 26)),
                        scanned_at: self.ago(240),
                        platform: "linux/amd64".into(),
                        gcp: false,
                    }),
                    // Asked the registry since: the tag points elsewhere, and a newer one is out.
                    remote: Some(RemoteImage { at: self.ago(5), digest: Some("sha256:9b1c0d2e3f405162738495a6b7c8d9e0f1a2b3c4d5e6f708192a3b4c5d6e7f80".into()), newer: Some("prod/orders-api:1.7.4".into()), error: None }),
                    ..project("orders-api · prod", Some("prod/orders-api:1.7.3"), 0, 2, 0)
                },
            ],
            findings: vec![
                finding("CVE-2025-27152", "high", "axios", "1.7.4", "1.8.2", "Possible SSRF and credential leakage via absolute URL", &["catalog-api"], true),
                finding("CVE-2025-29927", "critical", "next", "14.2.10", "14.2.25", "Authorization bypass in middleware", &["storefront-webapp"], false),
                finding("CVE-2024-45337", "high", "golang.org/x/crypto", "0.27.0", "0.31.0", "Misuse of ServerConfig.PublicKeyCallback", &["orders-api · prod"], false),
                finding("CVE-2024-24790", "high", "stdlib", "1.22.2", "1.22.4", "Unexpected behavior from Is methods for IPv4-mapped IPv6", &["orders-api · prod"], false),
            ],
        }
    }

    fn renovate(&self) -> Renovate {
        let item = |title: &str, action: &str, branch: Option<&str>, mr: Option<u64>, packages: &[&str]| RenovateItem {
            title: title.into(),
            action: action.into(),
            branch: branch.map(String::from),
            mr,
            packages: packages.iter().map(|p| p.to_string()).collect(),
            checked: false,
        };
        Renovate {
            dashboard_url: Some(format!("{HOST}/{PROJECT}/-/issues/12")),
            dashboard_iid: Some(12),
            sections: vec![
                RenovateSection { title: "Open".into(), items: vec![item("Update dependency axios to v1.7.9", "rebase-branch", Some("renovate/axios-1.x"), Some(1293), &["axios"])] },
                RenovateSection {
                    title: "Pending Approval".into(),
                    items: vec![
                        item("Update dependency vite to v6", "approve-branch", Some("renovate/vite-6.x"), None, &["vite"]),
                        item("Update postgres Docker tag to v17", "approve-branch", Some("renovate/postgres-17.x"), None, &["postgres"]),
                    ],
                },
            ],
            schedule_id: Some(4),
            schedule_ref: Some("main".into()),
            run: Some(RenovateRun { id: 9600, status: PipelineStatus::Success, started_at: self.ago(600), updated_at: self.ago(585), url: format!("{HOST}/{PROJECT}/-/pipelines/9600") }),
        }
    }

    fn sprint(&self) -> Sprint {
        let tickets = vec![
            self.ticket(412, "Checkout: save the cart between sessions", "review", Some(0), Some("high"), None, 6),
            self.ticket(433, "Fix VAT rounding on invoices", "doing", Some(0), Some("urgent"), Some("Bug"), 1),
            self.ticket(418, "Orders API: paginate the order history", "review", Some(1), Some("normal"), None, 5),
            self.ticket(421, "Payments: retry webhooks with backoff", "to validate", Some(2), Some("high"), None, 4),
            self.ticket(425, "Storefront: lazy-load product images", "doing", Some(3), Some("normal"), None, 3),
            self.ticket(427, "Back office: export orders as CSV", "to merge", Some(2), Some("low"), None, 4),
            self.ticket(430, "Catalog: search by SKU", "done", Some(4), Some("normal"), None, 8),
            self.ticket(436, "Orders: email the invoice once paid", "todo", None, Some("normal"), None, 2),
        ];
        // Estimates, and the time the team logged (the Actual of a ticket, the Actuals of My sprint).
        let hours = |h: f64| (h * 3_600_000.0) as u64;
        let times: [(f64, f64, &str); 8] = [(7.0, 8.5, "back"), (3.5, 2.0, "back"), (3.5, 4.0, "back"), (7.0, 5.5, "back"), (3.5, 3.0, "front"), (3.5, 1.5, "back"), (7.0, 10.0, "back"), (0.0, 0.0, "back")];
        let tickets = tickets
            .into_iter()
            .zip(times)
            .map(|(t, (estimate, spent, tag))| Ticket { time_estimate: (estimate > 0.0).then(|| hours(estimate)), time_spent: (spent > 0.0).then(|| hours(spent)), tags: vec![tag.into()], ..t })
            .collect();
        let start = self.ago(3 * 1440);
        let end = self.in_(9 * 1440);
        Sprint {
            name: format!("Sprint 42 ({} - {})", start.format("%d/%m"), end.format("%d/%m")),
            list_id: "demo-sprint".into(),
            url: format!("{CLICKUP}/sprint-42"),
            start: Some(start),
            end: Some(end),
            tickets,
            statuses: Self::statuses(),
            boards: Vec::new(),
        }
    }

    fn refinement(&self) -> Refinement {
        let statuses = ["to specify", "to refine", "to estimate"].map(String::from).to_vec();
        let board_statuses = statuses.iter().map(|s| status(s, "#87909e", false)).chain(Self::statuses()).collect::<Vec<_>>();
        Refinement {
            statuses,
            boards: vec![QuarterBoard {
                folder: "Shop".into(),
                quarter: "Q4 2026".into(),
                url: format!("{CLICKUP}/q4-2026"),
                tickets: {
                    let epic = |n: u32, name: &str, status: &str, days: i64| Ticket { task_type: Some("EPIC".into()), task_type_icon: Some("book".into()), ..self.ticket(n, name, status, None, Some("high"), None, days) };
                    let under = |t: Ticket, epic: u32| Ticket { parent: Some(format!("demo{epic}")), ..t };
                    vec![
                        epic(444, "International shop", "to refine", 30),
                        under(self.ticket(441, "Multi-currency prices", "to refine", Some(1), Some("high"), None, 12), 444),
                        under(self.ticket(445, "Translate the storefront (de, es)", "to estimate", Some(3), Some("normal"), None, 11), 444),
                        under(self.ticket(446, "Local payment methods (iDEAL, Bancontact)", "to specify", None, Some("normal"), None, 10), 444),
                        epic(447, "Self-service after-sales", "to estimate", 25),
                        under(self.ticket(442, "Returns: self-service portal", "to estimate", Some(2), Some("normal"), None, 7), 447),
                        under(self.ticket(448, "Track a parcel from the order page", "ready to dev", Some(4), Some("normal"), None, 6), 447),
                        self.ticket(440, "Gift cards at checkout", "to specify", None, Some("normal"), None, 9),
                        self.ticket(443, "Search: typo tolerance", "to refine", None, Some("low"), None, 15),
                    ]
                },
                statuses: board_statuses.clone(),
            }, QuarterBoard {
                folder: "Architecture".into(),
                quarter: String::new(),
                url: format!("{CLICKUP}/architecture"),
                tickets: self.architecture(),
                statuses: board_statuses,
            }],
        }
    }

    /// The sprint after this one, being filled.
    fn next_sprint(&self) -> Sprint {
        let start = self.in_(9 * 1440);
        let end = self.in_(23 * 1440);
        Sprint {
            name: format!("Sprint 43 ({} - {})", start.format("%d/%m"), end.format("%d/%m")),
            list_id: "demo-sprint-43".into(),
            url: format!("{CLICKUP}/sprint-43"),
            start: Some(start),
            end: Some(end),
            tickets: vec![
                self.ticket(448, "Track a parcel from the order page", "ready to dev", Some(4), Some("normal"), None, 6),
                self.ticket(449, "Orders API: idempotency keys on POST /orders", "ready to dev", None, Some("high"), None, 4),
                self.ticket(437, "Coupon code ignored when the cart is empty first", "ready to dev", None, Some("high"), Some("Bug"), 2),
            ],
            statuses: Self::statuses(),
            boards: Vec::new(),
        }
    }

    /// The Architecture board: topics to pick, and one to refine.
    fn architecture(&self) -> Vec<Ticket> {
        vec![
            self.ticket(450, "ADR: an event bus for order events", "todo", None, Some("normal"), None, 14),
            self.ticket(451, "Split catalog-api's read model", "ready to dev", None, Some("low"), None, 20),
            self.ticket(453, "Cache the catalog at the edge", "to refine", Some(1), Some("normal"), None, 9),
        ]
    }

    /// The work done outside the sprint.
    fn outside(&self) -> OutsideSprint {
        let list = ListRef { id: "demo-ops".into(), name: "Ops".into() };
        OutsideSprint {
            statuses: ["doing", "review"].map(String::from).to_vec(),
            tickets: vec![Ticket { list: Some(list.clone()), ..self.ticket(452, "Rotate the payment provider's webhook secret", "doing", Some(4), Some("urgent"), None, 1) }],
            list_statuses: BTreeMap::from([(list.id, Self::statuses())]),
        }
    }

    /// The team's recurring tasks to log time on, with subtasks.
    fn time_tasks(&self) -> Board {
        let task = |n: u32, name: &str, parent: Option<u32>, about: &str| Ticket {
            parent: parent.map(|p| format!("demo{p}")),
            description: Some(about.into()),
            ..self.ticket(n, name, "todo", None, None, None, 60)
        };
        Board {
            name: "Team".into(),
            url: format!("{CLICKUP}/team"),
            tickets: vec![
                task(460, "Meetings", None, "Rituals and team meetings."),
                task(461, "Daily stand-up", Some(460), "Every working day, 15 min."),
                task(462, "Sprint ceremonies", Some(460), "Planning, review and retrospective."),
                task(463, "Support", None, "Questions from support and customers."),
                task(464, "On-call", Some(463), "Weeks on call: incidents and alerts."),
                task(465, "Hiring", None, "Interviews and take-home reviews."),
            ],
            statuses: Self::statuses(),
        }
    }

    /// Time logged over the last days: tickets, reviews, meetings.
    fn time(&self) -> TimeTracking {
        let ticket = |n: u32| {
            let t = self.sprint().tickets.into_iter().find(|t| t.id == format!("SHOP-{n}"));
            (t.as_ref().map(|t| t.id.clone()), t.as_ref().map(|t| t.name.clone()), t.map(|t| t.url))
        };
        let entry = |i: u32, n: Option<u32>, day: i64, hour: u32, minutes: i64, what: &str| {
            let (ticket, task_name, task_url) = n.map(ticket).unwrap_or_default();
            let date = (self.now - Duration::days(day)).with_timezone(&chrono::Local).date_naive();
            let start = date.and_hms_opt(hour, 0, 0).and_then(|t| t.and_local_timezone(chrono::Local).single()).map(|t| t.with_timezone(&Utc)).unwrap_or(self.now);
            TimeEntry { id: format!("demo-t{i}"), ticket, task_name, task_url, start, duration_ms: minutes * 60_000, description: what.into() }
        };
        let entries = vec![
            entry(1, Some(433), 0, 8, 75, "VAT per line, failing test"),
            entry(2, Some(412), 1, 14, 180, "Saved cart: repository and migration"),
            entry(3, Some(418), 1, 11, 45, "Review of the order pagination"),
            entry(4, Some(412), 2, 9, 240, "Saved cart: merge on login"),
            entry(5, Some(427), 3, 15, 30, "Review of the CSV export"),
            entry(6, Some(412), 3, 10, 150, "Saved cart: storefront hook"),
        ];
        let mut recent = Vec::new();
        for e in &entries {
            if let Some(t) = &e.ticket && !recent.contains(t) {
                recent.push(t.clone());
            }
        }
        TimeTracking { entries, recent_tickets: recent }
    }

    fn pickup(&self) -> Pickup {
        let statuses = ["ready to dev", "todo"].map(String::from).to_vec();
        let board = |name: &str, kind: &str, tone: &str, sync: bool, tickets: Vec<Ticket>| PickBoard {
            name: name.into(),
            kind: kind.into(),
            tone: tone.into(),
            pick: true,
            sync,
            url: format!("{CLICKUP}/{}", name.to_lowercase().replace(' ', "-")),
            tickets,
            statuses: Self::statuses(),
        };
        let next = self.next_sprint();
        let ready = |t: &Ticket| statuses.contains(&t.status);
        Pickup {
            boards: vec![
                board("Hotfix", "board", "ko", false, vec![self.ticket(439, "Payment page blank on Safari 16", "ready to dev", None, Some("urgent"), Some("Bug"), 0)]),
                board(
                    "Bugs",
                    "board",
                    "warn",
                    false,
                    vec![
                        self.ticket(437, "Coupon code ignored when the cart is empty first", "ready to dev", None, Some("high"), Some("Bug"), 2),
                        self.ticket(438, "Back office: dates shown in UTC", "todo", None, Some("normal"), Some("Bug"), 5),
                    ],
                ),
                PickBoard { url: next.url.clone(), ..board(&next.name, "next_sprint", "info", false, next.tickets.into_iter().filter(ready).collect()) },
                board("Architecture", "board", "violet", true, self.architecture()),
            ],
            statuses,
        }
    }

    fn agenda(&self) -> Agenda {
        // Local hours: the day as the window shows it.
        Agenda { access: "granted".into(), provider: Default::default(), events: details::day(self.now.with_timezone(&chrono::Local).date_naive()) }
    }

    fn worktrees(&self) -> Vec<Worktree> {
        let wt = |path: &str, branch: Option<&str>, ticket: Option<u32>, ahead: u32, behind: u32, changes: u32, days: i64| Worktree {
            repo: "shop".into(),
            path: path.into(),
            branch: branch.map(String::from),
            ticket: ticket.map(|n| format!("SHOP-{n}")),
            base: "release/2026.10".into(),
            behind,
            ahead,
            dirty: changes > 0,
            changes,
            untracked: Some(0),
            size_bytes: Some(812 << 20),
            unpushed: if ahead > 0 && changes > 0 { 1 } else { 0 },
            upstream: branch.map(|b| format!("origin/{b}")),
            upstream_gone: days > 14,
            unpulled: if path.ends_with("saved-cart") { 2 } else { 0 },
            head: Some("4f2a9c1e5d".into()),
            last_commit: Some(self.ago(days * 1440 + 30)),
            is_main: path == "/demo/shop",
            prunable: false,
            locked: false,
        };
        vec![
            wt("/demo/shop", Some("release/2026.10"), None, 0, 0, 0, 0),
            wt("/demo/shop.worktrees/shop-412-saved-cart", Some("feat/SHOP-412-saved-cart"), Some(412), 4, 2, 0, 0),
            wt("/demo/shop.worktrees/shop-433-vat-rounding", Some("fix/SHOP-433-vat-rounding"), Some(433), 2, 0, 3, 0),
            wt("/demo/shop.worktrees/shop-401-wishlist", Some("feat/SHOP-401-wishlist"), Some(401), 0, 37, 0, 21),
        ]
    }

    /// Two agents on the same lines of the VAT rounding, and the wishlist (no agent) in one of
    /// their files, elsewhere in it.
    fn overlaps(&self) -> Overlaps {
        let wt = |name: &str| format!("/demo/shop.worktrees/{name}");
        let r = |start: u32, len: u32| LineRange { start, len };
        let pair = |a: &str, ab: &str, b: &str, bb: &str, files: Vec<OverlapFile>, key: &str| Overlap {
            repo: "shop".into(),
            base: "release/2026.10".into(),
            a: wt(a),
            b: wt(b),
            a_branch: Some(ab.into()),
            b_branch: Some(bb.into()),
            level: files.iter().map(|f| f.level).max().unwrap_or(OverlapLevel::File),
            files,
            merged: false,
            key: key.into(),
        };
        let file = |path: &str, level, a, b| OverlapFile { path: path.into(), level, a, b };
        Overlaps {
            pairs: vec![
                pair("shop-412-saved-cart", "feat/SHOP-412-saved-cart", "shop-433-vat-rounding", "fix/SHOP-433-vat-rounding", vec![
                    file("src/pricing/vat.rs", OverlapLevel::Lines, vec![r(92, 6)], vec![r(95, 3)]),
                    file("src/cart/total.rs", OverlapLevel::File, vec![r(14, 2)], vec![r(60, 1)]),
                ], "d3m0a"),
                pair("shop-433-vat-rounding", "fix/SHOP-433-vat-rounding", "shop-401-wishlist", "feat/SHOP-401-wishlist", vec![
                    file("src/cart/total.rs", OverlapLevel::File, vec![r(60, 1)], vec![r(120, 8)]),
                ], "d3m0b"),
            ],
            other_base: Vec::new(),
        }
    }

    /// Three tasks flagged in passing: two open (one seen by two agents), one launched.
    fn asides(&self) -> Vec<Aside> {
        let seen = |agent: &str, name: &str, branch: &str, tldr: &str, minutes: i64| AsideSeen {
            agent: Some(agent.into()),
            session: None,
            call: None,
            name: name.into(),
            cli: "claude".into(),
            branch: Some(branch.into()),
            cwd: None,
            effort: None,
            tldr: tldr.into(),
            at: self.ago(minutes),
        };
        let aside = |id: u64, title: &str, tldr: &str, files: &[&str], kind: Option<&str>, seen: Vec<AsideSeen>, state: AsideState| Aside {
            id,
            repo: Some("shop".into()),
            title: title.into(),
            tldr: tldr.into(),
            prompt: format!("{tldr}\n\nDo it in its own change, with a test; nothing else."),
            files: files.iter().map(|f| f.to_string()).collect(),
            kind: kind.map(String::from),
            needs_current_branch: false,
            at: seen[0].at,
            seen,
            state,
            changed_at: None,
            agent: (state == AsideState::Launched).then(|| "demo-a2".into()),
            agent_title: (state == AsideState::Launched).then(|| "SHOP-412 · Review-fix".into()),
            from_branch: None,
            ticket: None,
            dismissed_by: None,
            why: None,
            files_changed: false,
        };
        let vat = "SHOP-433 · Fix VAT rounding on invoices";
        vec![
            aside(3, "Remove the unused round_half_even helper", "src/pricing/round.rs keeps a helper nothing calls since the VAT fix; it misleads whoever reads the rounding.", &["src/pricing/round.rs"], Some("Development"), vec![seen("demo-a1", vat, "fix/SHOP-433-vat-rounding", "Dead helper next to the one I fixed.", 3)], AsideState::Open),
            aside(2, "Add a test for credit notes' VAT", "Credit notes go through the same rounding and have no test: the fix of SHOP-433 is not checked for them.", &["src/invoices/credit_note.rs", "tests/invoices.rs"], Some("Development"), vec![seen("demo-a1", vat, "fix/SHOP-433-vat-rounding", "Credit notes round VAT too, untested.", 9), seen("demo-a2", "SHOP-412 · Review-fix", "feat/SHOP-412-saved-cart", "No test covers credit notes' totals.", 5)], AsideState::Open),
            aside(1, "Update the README's checkout setup", "The README still says to run the checkout with STRIPE_KEY, renamed PAYMENT_KEY last month.", &["README.md"], None, vec![seen("demo-a3", "SHOP-441 · Refine multi-currency prices", "feat/SHOP-441-multi-currency", "Stale env var in the README.", 95)], AsideState::Launched),
            // Sent back to its agent's session: done there after its turn, then said done.
            Aside { agent: Some("demo-a1".into()), agent_title: Some(vat.into()), ..aside(4, "Log the rounding mode at startup", "Nothing says which rounding mode the invoices use: one log line at startup would.", &["src/main.rs"], None, vec![seen("demo-a1", vat, "fix/SHOP-433-vat-rounding", "One log line, next to my change.", 2)], AsideState::Session) },
            Aside { agent: Some("demo-a3".into()), agent_title: Some("SHOP-441 · Refine multi-currency prices".into()), why: Some("currency codes upper-cased in the parser, with its test".into()), ..aside(5, "Upper-case currency codes on input", "eur and EUR are two currencies today: the parser keeps them as typed.", &["src/pricing/currency.rs"], None, vec![seen("demo-a3", "SHOP-441 · Refine multi-currency prices", "feat/SHOP-441-multi-currency", "Lower-case codes slip through.", 40)], AsideState::Done) },
        ]
    }

    /// Two targets: the GitLab runners and their jobs, and the shop's pods in production.
    fn pods(&self) -> (Vec<PodView>, BTreeMap<String, Usage>, Vec<NodeLoad>) {
        let pod = |target: &str, namespace: &str, name: &str, role: PodRole, job: Option<(&str, &str)>, status: &str, minutes: i64| PodView {
            target: target.into(),
            context: if target == "Runners" { "demo-runners" } else { "demo-prod" }.into(),
            namespace: namespace.into(),
            name: name.into(),
            // `shop-api-7d9c5b-2xkq` → `shop-api`; a CI job's pod has none.
            workload: (role != PodRole::Job).then(|| name.rsplitn(3, '-').nth(2).unwrap_or(name).into()),
            role,
            is_job: role == PodRole::Job,
            job_name: job.map(|(n, _)| n.into()),
            job_ref: job.map(|(_, r)| r.into()),
            job_url: job.map(|_| format!("{HOST}/{PROJECT}/-/jobs/{}", job_id(9105, 1, 4))),
            cpu_limit_millis: Some(2000),
            mem_limit_bytes: Some(4 << 30),
            status: status.into(),
            healthy: status == "Running",
            restarts: if status == "CrashLoopBackOff" { 7 } else { 0 },
            started_at: Some(self.ago(minutes)),
            node: (status != "Pending").then(|| if target == "Runners" { "runners-pool-a1" } else { "prod-pool-b3" }.into()),
            last_exit: (status == "CrashLoopBackOff").then(|| "exit 1".into()),
            last_exit_at: (status == "CrashLoopBackOff").then(|| self.ago(4)),
            unscheduled: (status == "Pending").then(|| "0/3 nodes are available: 3 Insufficient cpu.".into()),
        };
        let pods = vec![
            pod("Runners", "gitlab-runner", "gitlab-runner-6c9f7d-x4kq", PodRole::Runner, None, "Running", 4000),
            pod("Runners", "gitlab-runner", "runner-x7k2-project-12-concurrent-0", PodRole::Job, Some(("unit-tests", "refs/merge-requests/1294/head")), "Running", 3),
            pod("Runners", "gitlab-runner", "runner-x7k2-project-12-concurrent-1", PodRole::Job, Some(("e2e-checkout", "release/2026.10")), "Pending", 1),
            pod("Production", "shop-prod", "shop-api-7d9c5b-2xkq", PodRole::Pod, None, "Running", 2900),
            pod("Production", "shop-prod", "shop-api-7d9c5b-9mzt", PodRole::Pod, None, "Running", 2900),
            pod("Production", "shop-prod", "shop-worker-5f6b8c-r4wd", PodRole::Pod, None, "CrashLoopBackOff", 40),
        ];
        let usage = pods
            .iter()
            .map(|p| (format!("{}/{}/{}", p.target, p.namespace, p.name), Usage { cpu_millis: if p.is_job { 1400 } else { 120 }, mem_bytes: if p.is_job { 2 << 30 } else { 300 << 20 } }))
            .collect();
        let node = |target: &str, name: &str| NodeLoad { target: target.into(), name: name.into(), cpu_used_millis: Some(2900), mem_used_bytes: Some(5 << 30), cpu_requested_millis: 4300, mem_requested_bytes: 9 << 30, cpu_allocatable_millis: Some(7800), mem_allocatable_bytes: Some(28 << 30), pods: 7, max_pods: Some(110), error: None };
        (pods, usage, vec![node("Runners", "runners-pool-a1"), node("Production", "prod-pool-b3")])
    }

    /// The inbox: the shop's suppliers and team, a flagged message already read.
    fn mail(&self) -> MailState {
        let message = |id: &str, from: &str, address: &str, subject: &str, minutes: i64, unread: bool, flagged: bool| MailMessage {
            id: id.into(),
            subject: subject.into(),
            from: from.into(),
            address: address.into(),
            received: Some(self.ago(minutes)),
            unread,
            flagged,
            link: format!("https://outlook.office.com/mail/inbox/id/{id}"),
        };
        MailState {
            signed_in: true,
            unread: 7,
            messages: vec![
                message("m1", "Sam Rivera", "sam@example.org", "Checkout: the saved cart is lost after login", 12, true, false),
                message("m2", "Payments provider", "noreply@payments.example.com", "Webhook endpoint failing for 2 hours", 47, true, true),
                message("m3", "Maya Chen", "maya@example.org", "Re: SHOP-441 multi-currency prices, the rounding rule", 95, true, false),
                message("m4", "GitLab", "gitlab@example.org", "Pipeline failed for release/2026.10", 180, true, false),
                message("m5", "Jordan Lee", "jordan@example.org", "Retro notes and the actions we kept", 60 * 20, false, true),
            ],
            read_at: Some(self.now),
            ..Default::default()
        }
    }

    fn sessions(&self) -> Vec<ClaudeSession> {
        let session = |id: &str, title: &str, branch: Option<&str>, status: SessionStatus, needs: Option<&str>, minutes: i64| ClaudeSession {
            id: id.into(),
            title: title.into(),
            folder: "shop".into(),
            worktree: None,
            branch: branch.map(String::from),
            ticket: branch.and_then(|b| crate::ticket::find([b])),
            status,
            needs_action: needs.map(String::from),
            read: false,
            last_activity: Some(self.ago(minutes)),
            group: None,
            group_order: None,
            order_in_group: None,
            origin: SessionOrigin::Desktop,
            repo: None,
            transcript: None,
            path: None,
        };
        vec![
            session("demo-s1", "Why does e2e-checkout flake?", Some("release/2026.10"), SessionStatus::NeedsYou, Some("Should I quarantine the test or raise the timeout?"), 6),
            session("demo-s2", "SHOP-433 VAT rounding", Some("fix/SHOP-433-vat-rounding"), SessionStatus::Running, None, 1),
            session("demo-s3", "Draft the 2026.10 release notes", None, SessionStatus::ReviewReady, None, 75),
        ]
    }

    /// The routines of the demo's config, trusted, and their last runs: two to review, a blocked
    /// one, one that found nothing to do, a skipped time. None runs.
    fn routines(&self) -> Routines {
        let cfg: crate::config::Config = toml::from_str(CONFIG).expect("demo config");
        let routines = cfg
            .agents
            .routines
            .iter()
            .map(|r| {
                let (when, qa) = r.check(&cfg.agents.quick_actions).expect("demo routine");
                let mut next = Vec::new();
                let mut at = self.now;
                while next.len() < 5 {
                    let Some(n) = when.next_after(at) else { break };
                    next.push(n);
                    at = n;
                }
                let action = qa.as_deref().map(RoutineAction::of);
                RoutineView { name: r.name.clone(), enabled: r.enabled, next, repos: r.targets(qa.as_deref()), error: None, pending: None, action }
            })
            .collect();
        let run = |id: &str, routine: &str, repo: Option<&str>, hours: i64, minutes: i64, state: RoutineRunState, deliver: &str| RoutineRun {
            id: id.into(),
            routine: routine.into(),
            repo: repo.map(String::from),
            agent: None,
            due: self.ago(hours * 60),
            started: Some(self.ago(hours * 60)),
            ended: (state != RoutineRunState::Blocked).then(|| self.ago(hours * 60 - minutes)),
            state,
            deliver: deliver.into(),
            caught_up: false,
            manual: false,
            note: None,
            said: None,
            files: 0,
            branch: None,
            archived: false,
            cost: None,
        };
        let runs = vec![
            RoutineRun { said: Some("11 alerts: 2 to handle (shop main pipeline red, pod checkout-api in CrashLoop), 3 in progress, 6 noise.".into()), cost: Some(0.08), ..run("r5", "Alerts triage", None, 1, 2, RoutineRunState::Ready, "review") },
            RoutineRun { said: Some("Should I move vite 7 → 8? The tests pass but its config changes.".into()), files: 12, branch: Some("agent/nightly-deps-20261105-0300".into()), ..run("r4", "Nightly deps", Some("helm"), 6, 0, RoutineRunState::Blocked, "review") },
            RoutineRun { said: Some("9 dependencies updated, tests green, draft MR opened. tokio 1.48 → 1.49 left aside (breaking changelog).".into()), files: 3, branch: Some("agent/nightly-deps-20261105-0300".into()), cost: Some(0.31), ..run("r3", "Nightly deps", Some("shop"), 6, 7, RoutineRunState::Ready, "review") },
            RoutineRun { said: Some("Nothing to update since yesterday.".into()), ..run("r2", "Nightly deps", Some("helm"), 30, 3, RoutineRunState::Nothing, "review") },
            RoutineRun { note: Some("the run of 03:00 still goes on".into()), ended: Some(self.ago(29 * 60)), started: None, ..run("r1", "Nightly deps", Some("shop"), 29, 0, RoutineRunState::Skipped, "review") },
        ];
        Routines { routines, runs, keep_running: true }
    }

    /// Prism's agents: their terminal replays a recorded session (`details::terminal`).
    fn agents(&self) -> Vec<Agent> {
        let agent = |id: &str, title: &str, category: &str, branch: &str, ticket: u32, status: SessionStatus, needs: Option<&str>, minutes: i64| Agent {
            provider: Default::default(),
            account: Some("Claude perso".into()),
            id: id.into(),
            title: title.into(),
            category: Some(category.into()),
            tags: if category == "Development" { vec!["checkout".into()] } else { Vec::new() },
            cwd: "/demo/shop".into(),
            worktree: Some(format!("/demo/shop.worktrees/{}", branch.rsplit('/').next().unwrap_or(branch).to_lowercase())),
            worktree_note: None,
            adopted: Vec::new(),
            also: Vec::new(),
            branch: Some(branch.into()),
            ticket: Some(format!("SHOP-{ticket}")),
            ticket_linked: false,
            mr: None,
            prompt: Some(title.split(" · ").last().unwrap_or(title).into()),
            launch: Some(LaunchSpec {
                title: title.into(),
                prompt: Some(title.split(" · ").last().unwrap_or(title).into()),
                worktree: Some(WorktreeRequest { repo: None, name: None, branch: Some(branch.into()), base: None, also: Vec::new() }),
                permission_mode: Some("acceptEdits".into()),
                category: Some(category.into()),
                from: Some(category.into()),
                ..Default::default()
            }),
            created_at: self.ago(minutes + 90),
            state: if status == SessionStatus::Completed { AgentState::Exited } else { AgentState::Running },
            exit_code: (status == SessionStatus::Completed).then_some(0),
            status,
            needs_action: needs.map(String::from),
            read: false,
            last_activity: Some(self.ago(minutes)),
            moved_at: None,
            // The longer it has worked, the fuller its window.
            context: Some(crate::agents::context::ContextUse { used: 40_000 + 1_100 * (90 + minutes as u64).min(150), size: Some(200_000) }),
            suggestion: None,
            gate: None,
            looping: None,
            legs: Vec::new(),
            // The review agent proposes plans (details.rs).
            planned: id == "demo-a2",
            model: Some("claude-opus-5-5".into()),
            // About 20k tokens a minute of work, mostly read back from the cache.
            usage: Some(crate::agents::usage::AgentUsage {
                input: 1_200 * (90 + minutes as u64),
                output: 900 * (90 + minutes as u64),
                cache_read: 16_000 * (90 + minutes as u64),
                cache_write: 1_800 * (90 + minutes as u64),
                replies: (90 + minutes as u32) / 3,
                cost: Some(0.034 * (90 + minutes) as f64),
                model: Some("claude-opus-5-5".into()),
                recent: (0..30u64).map(|i| 30_000 + (i * 7_919) % 45_000 + i * 2_500).collect(),
            }),
        };
        let mut list = vec![
            agent("demo-a1", "SHOP-433 · Fix VAT rounding on invoices", "Development", "fix/SHOP-433-vat-rounding", 433, SessionStatus::WaitingTool, Some("Allow Bash: npm run test:invoices?"), 4),
            agent("demo-a2", "SHOP-412 · Review-fix", "Review", "feat/SHOP-412-saved-cart", 412, SessionStatus::ReviewReady, None, 22),
            agent("demo-a3", "SHOP-441 · Refine multi-currency prices", "Refinement", "feat/SHOP-441-multi-currency", 441, SessionStatus::Completed, None, 140),
        ];
        // The VAT agent runs the same failing test again and again.
        list[0].looping = Some(crate::state::Looping { command: "npm run test:invoices".into(), count: 4 });
        // The review on Codex: the plan usage then shows both CLIs.
        let sol = Some("gpt-6.1-sol".to_string());
        if let Some(u) = list[1].usage.as_mut() {
            u.model = sol.clone();
            u.cost = Some(0.6 * u.cost.unwrap_or_default());
        }
        list[1].provider = crate::agents::provider::Provider::Codex;
        list[1].account = None;
        list[1].model = sol;
        // The refinement under the work account, its plan used up: Continue with… offers the other one.
        list[2].account = Some("Claude pro".into());
        // Begun on Claude, handed over to Codex at its limit (Continue with…).
        list[1].legs = vec![crate::state::AgentLeg {
            provider: crate::agents::provider::Provider::Claude,
            account: Some("Claude perso".into()),
            session: Some("demo-a2".into()),
            transcript: None,
            model: Some("claude-opus-5-5".into()),
            started: list[1].created_at,
            ended: self.ago(60),
            usage: Some(crate::agents::usage::AgentUsage { input: 61_000, output: 38_000, cache_read: 820_000, cache_write: 90_000, replies: 47, cost: Some(3.12), model: Some("claude-opus-5-5".into()), recent: Vec::new() }),
            reason: "limit".into(),
            handoff: "prism".into(),
            message: "# Picking up work in progress\n\nYou continue the work of another agent (Claude) in this folder: it stopped at its usage limit. Do not redo what is done: check where it is with git first, then carry on.\n\n## The task\nReview-fix\n\n## The plan\n1. ✓ Read the review threads\n2. → Fix the saved cart's expiry\n3. ◻ Answer each thread".into(),
        }];
        list
    }

    /// What the agents cost: those listed, and closed ones of the sprint's tickets and of none.
    fn agent_costs(&self, agents: &[Agent]) -> Vec<AgentCost> {
        let closed = |id: &str, ticket: Option<u32>, kind: &str, provider: crate::agents::provider::Provider, model: &str, minutes: u64, cost: f64, days: i64| AgentCost {
            id: id.into(),
            ticket: ticket.map(|n| format!("SHOP-{n}")),
            branch: None,
            title: format!("{kind} {}", ticket.map(|n| format!("SHOP-{n}")).unwrap_or_default()).trim().to_string(),
            kind: Some(kind.into()),
            provider,
            model: Some(model.into()),
            started: self.ago(days * 1440 + minutes as i64 + 30),
            last: self.ago(days * 1440),
            active_ms: minutes * 60_000,
            tokens: minutes * 21_000,
            cost: Some(cost),
        };
        let (claude, codex) = (crate::agents::provider::Provider::Claude, crate::agents::provider::Provider::Codex);
        let live = agents.iter().filter_map(|a| {
            let u = a.usage.as_ref()?;
            let minutes = (self.now - a.created_at).num_minutes().max(0) as u64 / 2;
            Some(AgentCost { id: a.id.clone(), ticket: a.ticket.clone(), branch: a.branch.clone(), title: a.title.clone(), kind: a.category.clone(), provider: a.provider, model: a.model.clone(), started: a.created_at, last: a.last_activity.unwrap_or(a.created_at), active_ms: minutes * 60_000, tokens: u.input + u.output + u.cache_read + u.cache_write, cost: u.cost })
        });
        live.chain([
            closed("demo-c1", Some(430), "Development", claude, "claude-opus-5-5", 58, 3.10, 4),
            closed("demo-c2", Some(430), "Review", claude, "claude-sonnet-5", 24, 0.90, 3),
            closed("demo-c3", Some(430), "Diagnose", codex, "gpt-6.1-sol", 18, 0.80, 3),
            closed("demo-c4", Some(421), "Development", claude, "claude-opus-5-5", 42, 2.20, 2),
            closed("demo-c5", Some(427), "Development", codex, "gpt-6.1-sol", 35, 1.20, 2),
            closed("demo-c6", None, "Exploration", claude, "claude-sonnet-5", 20, 0.60, 1),
        ])
        .collect()
    }

    fn write(&self, s: &mut Snapshot) {
        s.me = Some("alex".into());
        s.me_elsewhere = Some("alex-dev".into());
        // The blocks with a source of their own: their sample state.
        for b in crate::blocks::all() {
            if let Some(state) = b.demo(self.now) {
                s.blocks.insert(b.id().to_string(), state);
            }
        }
        s.merge_queue_label = Some("merge-queue".into());
        s.merge_priority_prefix = Some("merge-priority::".into());
        (s.pods, s.pod_usage, s.pod_nodes) = self.pods();
        s.mail = Some(self.mail());
        s.worktrees = self.worktrees();
        s.worktree_stale_days = 14;
        s.repos = vec![
            Repo { name: "shop".into(), path: "/demo/shop".into(), project: Some(PROJECT.into()), branch: Some("release/2026.10".into()), base: "main".into(), behind: 0, ahead: 0, dirty: false, is_main: true, error: None, forge: None },
            Repo { name: "helm".into(), path: "/demo/helm".into(), project: Some(HELM.into()), branch: Some("main".into()), base: "main".into(), behind: 3, ahead: 0, dirty: false, is_main: false, error: None, forge: None },
        ];
        s.repositories = vec![
            RepoInfo { name: "shop".into(), project: Some(PROJECT.into()), primary: true, clone: true, clone_missing: false, worktrees: true, merge_queue: true, ticket_required: true, pipelines: true, agent: None, forge: None, mr_web: Some(format!("{HOST}/{PROJECT}/-/merge_requests/")), workspace: Some("Work".into()) },
            RepoInfo { name: "helm".into(), project: Some(HELM.into()), primary: false, clone: true, clone_missing: false, worktrees: false, merge_queue: false, ticket_required: false, pipelines: false, agent: None, forge: None, mr_web: Some(format!("{HOST}/{HELM}/-/merge_requests/")), workspace: Some("Work".into()) },
            RepoInfo { name: "toolkit".into(), project: None, primary: false, clone: false, clone_missing: false, worktrees: false, merge_queue: false, ticket_required: false, pipelines: false, agent: None, forge: Some(crate::forge::Provider::Github), mr_web: None, workspace: None },
        ];
        s.features = Features { vulnerabilities: true, renovate: true, ci_health: true };
        // CI health and Renovate read the shop, Vulnerabilities the shop and the Helm charts; a
        // team's block that reads repositories, the shop.
        let block = |id: &str, uses: blocks::RepoUse, key: &str, repos: &[&str]| blocks::RepoBlock { id: id.into(), uses, key: key.split('.').map(String::from).collect(), repos: repos.iter().map(|r| r.to_string()).collect() };
        s.repo_blocks = [block("ci_health", blocks::RepoUse::One, "forge.ci_health_repo", &["shop"]), block("renovate", blocks::RepoUse::One, "renovate.repo", &["shop"]), block("vulnerabilities", blocks::RepoUse::Many, "vulnerabilities.repos", &["shop", "helm"])]
            .into_iter()
            .chain(blocks::all().iter().filter(|b| b.repos() != blocks::RepoUse::None).map(|b| block(b.id(), b.repos(), &format!("{}.{}", b.id(), if b.repos() == blocks::RepoUse::Many { "repos" } else { "repo" }), &["shop"])))
            .collect();
        s.cluster = true;
        s.pod_shell = true;
        s.pod_history = true;
        s.gitlab = true;
        s.calendar = true;
        s.tracker = Some(crate::tracker::Provider::Clickup);
        s.tracker_features = crate::tracker::Provider::Clickup.features();
        s.agents = self.agents();
        s.routines = self.routines();
        s.overlaps = self.overlaps();
        s.asides = self.asides();
        // Agents count as sessions too (header, rail, Alerts), as the real ones do.
        s.sessions = self.sessions();
        s.sessions.extend(s.agents.iter().map(|a| ClaudeSession {
            id: a.id.clone(),
            title: a.title.clone(),
            folder: "shop".into(),
            worktree: a.worktree.clone(),
            branch: a.branch.clone(),
            ticket: a.ticket.clone(),
            status: a.status,
            needs_action: a.needs_action.clone(),
            read: false,
            last_activity: a.last_activity,
            group: None,
            group_order: None,
            order_in_group: None,
            origin: SessionOrigin::Agent,
            repo: None,
            transcript: None,
            path: None,
        }));
        s.usage = Some(ClaudeUsage {
            five_hour: Some(UsageWindow { utilization: 42.0, resets_at: Some(self.in_(130)) }),
            seven_day: Some(UsageWindow { utilization: 18.0, resets_at: Some(self.in_(4 * 1440)) }),
            others: Vec::new(),
            fetched_at: self.now,
        });
        s.usage_account = Some("Claude perso".into());
        s.other_usage = vec![crate::state::ProviderUsage {
            provider: crate::agents::provider::Provider::Claude,
            account: Some("Claude pro".into()),
            usage: Some(ClaudeUsage {
                five_hour: Some(UsageWindow { utilization: 100.0, resets_at: Some(self.in_(95)) }),
                seven_day: Some(UsageWindow { utilization: 63.0, resets_at: Some(self.in_(2 * 1440)) }),
                others: Vec::new(),
                fetched_at: self.now,
            }),
            error: None,
        }, crate::state::ProviderUsage {
            provider: crate::agents::provider::Provider::Codex,
            account: None,
            usage: Some(ClaudeUsage {
                five_hour: Some(UsageWindow { utilization: 74.0, resets_at: Some(self.in_(70)) }),
                seven_day: Some(UsageWindow { utilization: 31.0, resets_at: Some(self.in_(5 * 1440 + 300)) }),
                others: Vec::new(),
                fetched_at: self.now,
            }),
            error: None,
        }];
        s.merge_requests = self.merge_requests();
        s.pipelines = self.pipelines();
        s.pending_jobs = vec![PendingJob { repo: None, name: "e2e-checkout".into(), target: "release/2026.10".into(), queued_since: self.ago(7) }];
        s.ci_health = Some(self.ci_health());
        s.vulnerabilities = Some(self.vulnerabilities());
        let (envs, argocd) = self.environments();
        s.environments = Some(envs);
        s.argocd = Some(argocd);
        s.renovate = Some(self.renovate());
        s.sprint = Some(self.sprint());
        s.next_sprint = Some(self.next_sprint());
        s.outside_sprint = self.outside();
        s.time_tasks = Some(self.time_tasks());
        s.time = self.time();
        s.archived_sessions = vec![ArchivedSession {
            id: "demo-s0".into(),
            title: "SHOP-401 Wishlist".into(),
            worktree: "/demo/shop.worktrees/shop-401-wishlist".into(),
            branch: Some("feat/SHOP-401-wishlist".into()),
            last_activity: Some(self.ago(21 * 1440)),
        }];
        s.refinement = self.refinement();
        s.pickup = self.pickup();
        s.agenda = Some(self.agenda());
        s.tracker_me = Some(Self::alex());
        s.clickup_team = Some("demo".into());
        s.updated_at = Some(self.now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo config is a valid profile config, in demo mode.
    #[test]
    fn config_is_valid() {
        let cfg: crate::config::Config = toml::from_str(CONFIG).unwrap();
        assert!(cfg.demo.enabled && !cfg.views.is_empty());
        // Its tickets follow its own workflow, not the neutral defaults.
        assert!(cfg.tracker.review_statuses.contains(&"to merge".to_string()));
        // Its boards are those its data shows.
        let names: Vec<String> = cfg.clickup.boards.clone().into_iter().map(|b| b.name).collect();
        assert_eq!(names, ["Hotfix", "Bugs", "Architecture", "Shop", "Team"]);
    }

    /// Every block has something to show, and it serializes for the window.
    #[test]
    fn fills_every_block() {
        let mut s = Snapshot::default();
        Demo { now: Utc::now() }.write(&mut s);
        assert!(s.merge_requests.len() > 5 && s.pipelines.len() == 3 && s.sprint.as_ref().is_some_and(|x| x.tickets.len() > 5));
        assert!(s.environments.is_some() && s.vulnerabilities.is_some() && !s.worktrees.is_empty());
        assert!(serde_json::to_string(&s).is_ok());
    }
}
