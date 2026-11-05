//! The demo profile's custom blocks: one of each kind, and one waiting for trust.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};

use super::{ActionView, Origin, Pending, Reason, State, View};

fn read(text: &str) -> super::Output {
    super::output::read(text).0
}

fn view(id: &str, label: &str, source: &str, every: u64, out: &str, now: DateTime<Utc>) -> View {
    View {
        id: id.into(),
        label: label.into(),
        origin: Origin::Config,
        enabled: true,
        on_demand: false,
        every_seconds: every,
        remote: true,
        source: source.into(),
        def: None,
        pending: None,
        running: false,
        ran_at: Some(now - Duration::seconds(40)),
        took_ms: Some(320),
        output: Some(read(out)),
        error: None,
        stderr: None,
        history: BTreeMap::new(),
        actions: Vec::new(),
    }
}

pub fn state(now: DateTime<Utc>) -> State {
    let mut backups = view(
        "backups",
        "Backups",
        "~/bin/check-backups --json",
        300,
        r#"{"kind":"list","items":[{"id":"nas","title":"NAS","sub":"2 h ago","state":"ok"},{"id":"laptop","title":"Laptop","sub":"3 days ago","state":"ko","badge":"3 d"},{"id":"photos","title":"Photos (B2)","sub":"yesterday","state":"warn","badge":"26 h"}],"alerts":[{"severity":"ko","text":"Laptop not backed up for 3 days","key":"laptop"}],"status":{"text":"1 KO","state":"ko"}}"#,
        now,
    );
    backups.actions = vec![
        ActionView { id: "run".into(), label: "Back up now".into(), kind: "command".into(), critical: false },
        ActionView { id: "fix".into(), label: "Fix with an agent".into(), kind: "agent".into(), critical: false },
    ];
    if let Some(super::Body::List { items }) = backups.output.as_mut().map(|o| &mut o.body) {
        for i in items {
            i.actions = if i.state == Some(super::Level::Ok) { vec!["run".into()] } else { vec!["run".into(), "fix".into()] };
        }
    }
    let mut quotas = view(
        "quotas",
        "API quotas",
        "script (uv run)",
        3600,
        r#"{"kind":"stats","stats":[{"key":"maps","label":"Maps","value":72,"max":100,"unit":"%","warn":80,"ko":95},{"key":"llm","label":"LLM","value":41,"max":100,"unit":"%","warn":80,"ko":95},{"key":"sms","label":"SMS left","value":180,"warn":100,"ko":20}],"status":"Maps 72 %"}"#,
        now,
    );
    quotas.history = BTreeMap::from([("maps".into(), vec![40.0, 44.0, 51.0, 55.0, 60.0, 63.0, 66.0, 70.0, 72.0]), ("llm".into(), vec![60.0, 52.0, 47.0, 45.0, 44.0, 41.0, 41.0])]);
    let latency = view(
        "latency",
        "Latency",
        "script (bash)",
        60,
        r#"{"kind":"table","columns":[{"key":"host","label":"Host"},{"key":"ms","label":"ms"},{"key":"loss","label":"Loss"}],"rows":[{"host":"api.example.com","ms":38,"loss":"0 %","state":"ok"},{"host":"eu.example.com","ms":112,"loss":"2 %","state":"warn"},{"host":"db.internal","ms":4,"loss":"0 %","state":"ok"}]}"#,
        now,
    );
    let notes = view("oncall", "On call", "~/bin/oncall", 900, r#"{"kind":"markdown","text":"**This week:** Camille\n\n- Runbook: [Incidents](https://example.com/runbook)\n- Escalation: *ops* channel"}"#, now);
    let mut pending = view("disk-alert", "Disk alert", "script (python3)", 600, "{}", now);
    pending.output = None;
    pending.ran_at = None;
    pending.took_ms = None;
    pending.pending = Some(Pending {
        reason: Reason::New,
        hash: "demo".into(),
        since: now - Duration::minutes(12),
        agents: vec!["Tidy the config".into()],
        diff: super::diff::lines("", "id = \"disk-alert\"\nscript = '''\n#!/usr/bin/env python3\nimport json, shutil\n…\n'''\nevery_seconds = 600\n"),
        runs_trusted: false,
    });
    State { blocks: vec![backups, quotas, latency, notes, pending], errors: Vec::new() }
}

#[cfg(test)]
mod tests {
    /// The demo shows each kind, and a block waiting for trust.
    #[test]
    fn one_of_each_kind() {
        let s = super::state(chrono::Utc::now());
        let kinds: Vec<String> = s.blocks.iter().filter_map(|b| b.output.as_ref()).map(|o| serde_json::to_value(&o.body).unwrap()["kind"].as_str().unwrap().to_string()).collect();
        assert_eq!(kinds, ["list", "stats", "table", "markdown"]);
        assert!(s.blocks.iter().any(|b| b.pending.is_some()));
    }
}
