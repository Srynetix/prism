//! The Changes block in the demo profile: a morning on staging (an MR merged, its sync, a
//! CrashLoop right after, the fix, an alert, a CVE), and what the demo's prod and preprod show.

use chrono::{DateTime, Duration as Span, Utc};

use super::{ChangeEvent, ChangeKind as K, ChangeLink, ChangeMr, ChangeSeverity as S, ChangesState, Closed};

pub fn state(now: DateTime<Utc>) -> ChangesState {
    let ago = |m: i64| now - Span::minutes(m);
    let ev = |id: &str, m: i64, env: &str, source: &str, kind: K, severity: S, title: &str| {
        let mut e = ChangeEvent::new(id, ago(m), source, kind, severity, title);
        e.env = Some(env.into());
        e
    };
    let argo = |e: &mut ChangeEvent, app: &str, deployment: &str| {
        e.app = Some(deployment.into());
        e.namespace = Some("shop".into());
        e.workloads = vec![deployment.into()];
        e.link = Some(ChangeLink::Argo { cluster: "demo-ops".into(), app: format!("demo-ops/shop/{app}") });
    };
    let pod = |e: &mut ChangeEvent, env: &str, deployment: &str| {
        e.app = Some(deployment.into());
        e.namespace = Some("shop".into());
        e.workloads = vec![deployment.into()];
        e.link = Some(ChangeLink::Pod { context: format!("demo-{env}"), namespace: "shop".into(), deployment: deployment.into() });
    };
    let mr = |iid: u64, title: &str| ChangeMr { repo: Some("shop".into()), iid, title: title.into(), url: Some(format!("https://gitlab.example.org/shop/shop/-/merge_requests/{iid}")) };
    let merged = |id: &str, m: i64, iid: u64, title: &str, branch: &str, env: &str| {
        let mut e = ev(id, m, env, "mrs", K::Merged, S::Info, title);
        e.app = Some("shop".into());
        e.detail = Some(branch.into());
        e.link = Some(ChangeLink::Mr { repo: Some("shop".into()), iid });
        e.mr = Some(mr(iid, title));
        e
    };
    let sync = |id: &str, m: i64, env: &str, app: &str, deployment: &str, rev: &str, image: &str, mr_of: Option<ChangeMr>| {
        let mut e = ev(id, m, env, "argocd", K::Sync, S::Info, app);
        argo(&mut e, app, deployment);
        e.revision = Some(rev.into());
        e.images = vec![format!("{deployment}:{image}")];
        e.detail = Some("auto".into());
        e.mr = mr_of;
        e
    };
    let mut events = vec![
        // Yesterday, prod.
        merged("mr:shop:1281:prod", 60 * 22, 1281, "Catalog: cache the price lists", "release/2.13", "prod"),
        sync("sync:prod:catalog", 60 * 21, "prod", "catalog-api-prod", "catalog-api", "51b0a7c2", "2.13.2", Some(mr(1281, "Catalog: cache the price lists"))),
        // This morning, staging: a merge, its sync, a crash right after.
        merged("mr:shop:1288:staging", 190, 1288, "Orders: retry the payment webhook", "main", "staging"),
        sync("sync:staging:orders:1", 175, "staging", "orders-api-staging", "orders-api", "4f2a9c1e", "release-2026-10-4f2a9c1e", Some(mr(1288, "Orders: retry the payment webhook"))),
        {
            let mut e = ev("rollout:staging:orders:8", 174, "staging", "pods", K::Rollout, S::Info, "orders-api");
            pod(&mut e, "staging", "orders-api");
            e.images = vec!["orders-api:release-2026-10-4f2a9c1e".into()];
            e.detail = Some("rev 8".into());
            e
        },
        {
            let mut e = ev("restart:staging:orders:2", 168, "staging", "pods", K::Restart, S::Warn, "orders-api");
            pod(&mut e, "staging", "orders-api");
            e.seen = true;
            e.detail = Some("+2 · OOMKilled".into());
            e.related = Some("rollout:staging:orders:8".into());
            e
        },
        {
            let mut e = ev("crash:staging:orders", 166, "staging", "pods", K::CrashLoop, S::Ko, "orders-api");
            pod(&mut e, "staging", "orders-api");
            e.detail = Some("CrashLoopBackOff · OOMKilled".into());
            e.related = Some("rollout:staging:orders:8".into());
            e
        },
        {
            let mut e = ev("health:staging:orders:1", 165, "staging", "argocd", K::Health, S::Ko, "orders-api-staging");
            argo(&mut e, "orders-api-staging", "orders-api");
            e.seen = true;
            e.detail = Some("Healthy → Degraded".into());
            e.related = Some("rollout:staging:orders:8".into());
            e
        },
        {
            let mut e = ev("alert:on:orders", 164, "staging", "alerts", K::AlertOn, S::Ko, "orders-api: CrashLoopBackOff (staging)");
            e.seen = true;
            e.app = Some("orders-api".into());
            e
        },
        // The fix.
        merged("mr:shop:1291:staging", 120, 1291, "fix(orders): raise the memory limit to 768 Mi", "main", "staging"),
        sync("sync:staging:orders:2", 110, "staging", "orders-api-staging", "orders-api", "9b3e1d04", "release-2026-10-9b3e1d04", Some(mr(1291, "fix(orders): raise the memory limit to 768 Mi"))),
        {
            let mut e = ev("rollout:staging:orders:9", 109, "staging", "pods", K::Rollout, S::Info, "orders-api");
            pod(&mut e, "staging", "orders-api");
            e.images = vec!["orders-api:release-2026-10-9b3e1d04".into()];
            e.detail = Some("rev 9".into());
            e
        },
        {
            let mut e = ev("health:staging:orders:2", 105, "staging", "argocd", K::Health, S::Ok, "orders-api-staging");
            argo(&mut e, "orders-api-staging", "orders-api");
            e.seen = true;
            e.detail = Some("Degraded → Healthy".into());
            e
        },
        {
            let mut e = ev("alert:off:orders", 104, "staging", "alerts", K::AlertOff, S::Ok, "orders-api: CrashLoopBackOff (staging)");
            e.seen = true;
            e.app = Some("orders-api".into());
            e
        },
        {
            let mut e = ev("vuln:new:staging", 60, "staging", "vulnerabilities", K::VulnNew, S::Ko, "CVE-2026-31337 · openssl 3.0.13");
            e.seen = true;
            e.app = Some("catalog-api".into());
            e.detail = Some("openssl: buffer overread in the X.509 name check".into());
            e.link = Some(ChangeLink::Url { url: "https://avd.aquasec.com/nvd/cve-2026-31337".into() });
            e
        },
        sync("sync:staging:catalog", 30, "staging", "catalog-api-staging", "catalog-api", "9b3e1d04", "release-2026-10-9b3e1d04", Some(mr(1291, "fix(orders): raise the memory limit to 768 Mi"))),
        // Preprod and prod, as the demo's other blocks show them.
        {
            let mut e = ev("rollout:preprod:orders:12", 600, "preprod", "pods", K::Rollout, S::Info, "orders-api");
            pod(&mut e, "preprod", "orders-api");
            e.images = vec!["orders-api:rc-1.8.0".into()];
            e.detail = Some("rev 12".into());
            e
        },
        {
            let mut e = ev("oom:preprod:orders", 35, "preprod", "pods", K::OomKilled, S::Ko, "orders-api");
            pod(&mut e, "preprod", "orders-api");
            e.detail = Some("3 restarts".into());
            e
        },
        {
            let mut e = ev("drift:prod:payments", 40, "prod", "argocd", K::Drift, S::Warn, "payments-api-prod");
            argo(&mut e, "payments-api-prod", "payments-api");
            e.seen = true;
            e.detail = Some("Deployment payments-api".into());
            e
        },
        {
            let mut e = ev("sync_failed:prod:cert-manager", 14, "prod", "argocd", K::SyncFailed, S::Ko, "cert-manager-prod");
            e.app = Some("cert-manager".into());
            e.namespace = Some("cert-manager".into());
            e.link = Some(ChangeLink::Argo { cluster: "demo-ops".into(), app: "demo-ops/cert-manager/cert-manager-prod".into() });
            e.detail = Some("one or more synchronization tasks completed unsuccessfully".into());
            e
        },
    ];
    events.sort_by_key(|e| e.at);
    ChangesState {
        events,
        envs: vec!["prod".into(), "preprod".into(), "staging".into()],
        sources: super::default_sources(),
        // The night: Prism closed.
        closed: vec![Closed { from: ago(60 * 15), to: ago(60 * 9) }],
        correlate_minutes: 30,
        default_period: "since_yesterday".into(),
        retention_days: 14,
    }
}
