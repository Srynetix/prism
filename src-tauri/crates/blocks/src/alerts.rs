//! Prism's own blocks' lines in Alerts (`Block::alerts`), from the state each publishes: read as
//! JSON, as the window read it, so that a state published by an older Prism still reads.

use chrono::{DateTime, Utc};
use prism_base::{i18n::tr, tn, tr};
use prism_core::alerts::{js_number, round, to_fixed, Alert, Fix, Severity};
use serde_json::{json, Value};

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn opt_text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn date(v: &Value, key: &str) -> Option<DateTime<Utc>> {
    opt_text(v, key).and_then(|d| DateTime::parse_from_rfc3339(d).ok()).map(|d| d.with_timezone(&Utc))
}

/* ---------- Docker ---------- */

/// A container in trouble: unhealthy, restarting in a loop, or exited with an error (137 and 143,
/// a stop's kill and term, are no error); and the reclaimable disk beyond `[docker] reclaimable_gb`.
pub fn docker(d: &Value) -> Vec<Alert> {
    if opt_text(d, "server").is_none_or(str::is_empty) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for c in list(d, "containers") {
        let (state, health, code) = (text(c, "state"), opt_text(c, "health"), num(c, "exit_code").filter(|c| *c != 0.0));
        let failing = health == Some("unhealthy") || state == "restarting" || state == "dead" || (state == "exited" && code.is_some_and(|c| c != 137.0 && c != 143.0));
        if !failing {
            continue;
        }
        let name = match opt_text(c, "project").filter(|p| !p.is_empty()) {
            Some(project) => format!("{} ({project})", text(c, "name")),
            None => text(c, "name").to_string(),
        };
        let said = match (health, state) {
            (Some("unhealthy"), _) => tr("unhealthy").to_string(),
            (_, "exited") => tr!("exited ({code})", code = num(c, "exit_code").map(js_number).unwrap_or_default()),
            (_, "restarting") => tr("restarting").to_string(),
            (_, "dead") => tr("dead").to_string(),
            (_, other) => other.to_string(),
        };
        // One notification per container turning bad, whatever its detail.
        out.push(Alert::new("Docker", Severity::Ko, tr!("Container {name} is {state}", name = name, state = said)).notification(format!("docker:{}", text(c, "name")), "Docker"));
    }
    let reclaimable: f64 = list(d, "disk").iter().filter_map(|k| num(k, "reclaimable_bytes")).sum();
    let limit = num(d, "reclaimable_gb").unwrap_or(0.0);
    if limit > 0.0 && reclaimable > limit * 1e9 {
        // Not worth a notification: a cleanup to do when convenient.
        out.push(Alert::new("Docker", Severity::Warn, tr!("{gb} GB of Docker data unused and reclaimable", gb = to_fixed(reclaimable / 1e9, 1))).notify(false));
    }
    out
}

/* ---------- Dev servers ---------- */

/// A server whose shell went back to its prompt on an error: one alert each.
pub fn dev_servers(d: &Value) -> Vec<Alert> {
    list(d, "servers")
        .iter()
        .filter(|s| text(s, "state") == "stopped" && flag(s, "failed"))
        .map(|s| {
            let command = text(s, "command");
            let shell = num(s, "shell");
            let said = match shell {
                Some(n) => tr!("Dev server {command} stopped on an error (shell {shell})", command = command, shell = js_number(n)),
                None => tr!("Dev server {command} stopped on an error", command = command),
            };
            let fixes = match opt_text(s, "tail").filter(|t| !t.is_empty()) {
                Some(tail) => vec![Fix::DevServer { agent: text(s, "agent").into(), command: command.into(), place: shell.map_or_else(|| "started by the agent".to_string(), |n| format!("shell {}", js_number(n))), tail: tail.into() }],
                None => Vec::new(),
            };
            Alert::new(tr("Dev servers"), Severity::Ko, said).notification(format!("dev_servers:{}", text(s, "key")), tr("Dev server")).fixes(fixes)
        })
        .collect()
}

/* ---------- Custom blocks ---------- */

/// What the custom blocks print in `alerts`, and each block waiting for trust.
pub fn custom(s: &Value) -> Vec<Alert> {
    let kind = tr("Custom blocks");
    let mut out = Vec::new();
    for b in list(s, "blocks") {
        let (id, label) = (text(b, "id"), text(b, "label"));
        if let Some(p) = b.get("pending").filter(|p| p.is_object()) {
            // Added while an agent ran: someone (something) else wrote it, it is for now.
            let agent = !list(p, "agents").is_empty();
            let a = Alert::new(kind, if agent { Severity::Ko } else { Severity::Warn }, tr!("Custom block {name} waits for your trust before it runs", name = label)).since(date(p, "since")).item(id);
            out.push(if agent { a.notification(format!("custom:{id}:trust"), tr("Custom block to check")) } else { a.notify(false) });
            continue;
        }
        for x in b.get("output").map(|o| list(o, "alerts")).unwrap_or_default() {
            let severity = match text(x, "severity") {
                "ok" => continue,
                "ko" => Severity::Ko,
                "warn" => Severity::Warn,
                _ => Severity::Info,
            };
            let said = text(x, "text");
            let a = Alert::new(kind, severity, format!("{label}: {said}")).short(said).item(id);
            out.push(if severity == Severity::Ko { a.notification(format!("custom:{id}:{}", text(x, "key")), label) } else { a.notify(false) });
        }
    }
    out
}

/* ---------- Sound ---------- */

/// The microphone too low (muted is not an alert): in Alerts at once (orange), notified (red) while
/// an application records from it, the moment it matters. "Raise" fixes it from the row.
pub fn sound(s: &Value) -> Vec<Alert> {
    let Some(input) = s.get("input").filter(|i| i.is_object()) else { return Vec::new() };
    if opt_text(s, "updated_at").is_none_or(str::is_empty) || input.get("muted").and_then(Value::as_bool) == Some(true) {
        return Vec::new();
    }
    let limits = s.get("limits").cloned().unwrap_or_default();
    let (min, under, restore) = (num(&limits, "min_input_gain").unwrap_or(0.0), num(&limits, "muted_under").unwrap_or(0.0), num(&limits, "restore_gain").unwrap_or(0.0));
    let Some(volume) = num(input, "volume") else { return Vec::new() };
    if volume < under || min <= 0.0 || volume >= min {
        return Vec::new();
    }
    let name = text(input, "name");
    let recording = list(s, "recording");
    let fix = Fix::Block {
        block: "sound".into(),
        command: "volume".into(),
        args: json!({ "side": "input", "value": limits.get("restore_gain").cloned().unwrap_or(Value::Null) }),
        label: tr!("Raise to {n} %", n = js_number(restore)),
        title: tr!("Set {name}’s input volume to {n} %", name = name, n = js_number(restore)),
    };
    let (volume, min) = (js_number(volume), js_number(min));
    if recording.is_empty() {
        let said = tr!("Microphone {name}’s input volume is at {volume} % (under {min} %)", name = name, volume = volume, min = min);
        return vec![Alert::new(tr("Sound"), Severity::Warn, said).fixes(vec![fix]).notify(false)];
    }
    let who = recording.iter().map(|r| text(r, "app")).collect::<Vec<_>>().join(tr(" and "));
    let said = tr!("Microphone {name}’s input volume is at {volume} % (under {min} %) while {who} records", name = name, volume = volume, min = min, who = who);
    // Its own key while recording: one notification when a call starts with the microphone low.
    vec![Alert::new(tr("Sound"), Severity::Ko, said).fixes(vec![fix]).notification("sound:gain", tr("Microphone"))]
}

/* ---------- Status pages ---------- */

/// An incident under way: major and critical ones notified, minor ones and maintenance not.
pub fn status(s: &Value) -> Vec<Alert> {
    if !flag(s, "alerts") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for svc in list(s, "services") {
        let service = text(svc, "name");
        for i in list(svc, "incidents").iter().filter(|i| !flag(i, "maintenance")) {
            let serious = matches!(text(i, "impact"), "major" | "critical");
            let mut said = tr!("{service} incident: {name}", service = service, name = text(i, "name"));
            if let Some(st) = opt_text(i, "status").filter(|s| !s.is_empty()) {
                said += &format!(" ({st})");
            }
            let a = Alert::new(tr("Status pages"), if serious { Severity::Ko } else { Severity::Warn }, said);
            out.push(if serious { a.notification(format!("status:{}:{}", text(svc, "id"), text(i, "id")), tr!("{service} status", service = service)) } else { a.notify(false) });
        }
    }
    out
}

/* ---------- System ---------- */

/// Five minutes at 90 % and more: the CPU is hot.
const HOT_SECONDS: f64 = 300.0;
/// Beyond this share of the handshakes lost, the network is bad.
const LOSS: f64 = 5.0;
const GIB: f64 = 1_073_741_824.0;

/// `29.4 GB`, `840 MB`.
fn bytes(n: f64) -> String {
    if n >= GIB {
        return tr!("{n} GB", n = to_fixed(n / GIB, if n >= 100.0 * GIB { 0 } else { 1 }));
    }
    tr!("{n} MB", n = round(n / 1_048_576.0))
}

/// The memory pressure red (notified), the CPU at 90 % for 5 minutes, the home disk getting full,
/// handshakes lost.
pub fn system(s: &Value) -> Vec<Alert> {
    if !flag(s, "alerts") || opt_text(s, "updated_at").is_none_or(str::is_empty) {
        return Vec::new();
    }
    let kind = tr("System");
    let mut out = Vec::new();
    let empty = Value::Null;
    let (cpu, memory, network) = (s.get("cpu").unwrap_or(&empty), s.get("memory").unwrap_or(&empty), s.get("network").unwrap_or(&empty));
    if text(memory, "pressure") == "critical" {
        out.push(Alert::new(kind, Severity::Ko, tr("Memory pressure is critical: the computer is swapping, close something")).notification("system:memory", tr("Memory pressure")));
    }
    let hot = num(cpu, "hot_seconds").unwrap_or(0.0);
    if hot >= HOT_SECONDS {
        out.push(Alert::new(kind, Severity::Warn, tr!("CPU at {n} % for {min} min", n = round(num(cpu, "usage").unwrap_or(0.0)), min = round(hot / 60.0))).notify(false));
    }
    if let Some(disk) = list(s, "disks").first() {
        let limits = s.get("disk_limits").filter(|l| l.is_object());
        let limit = |key: &str, default: f64| limits.and_then(|l| num(l, key)).unwrap_or(default);
        let (total, available) = (num(disk, "total").unwrap_or(0.0), num(disk, "available").unwrap_or(0.0));
        let pct = if total > 0.0 { available / total * 100.0 } else { 100.0 };
        let under = |gb: f64, percent: f64| (percent > 0.0 && pct < percent) || (gb > 0.0 && available < gb * GIB);
        let red = under(limit("disk_alert_gb", 10.0), limit("disk_alert_percent", 5.0));
        if red || under(limit("disk_warn_gb", 25.0), limit("disk_warn_percent", 10.0)) {
            let name = Some(text(disk, "name")).filter(|n| !n.is_empty()).unwrap_or(text(disk, "mount"));
            let a = Alert::new(kind, if red { Severity::Ko } else { Severity::Warn }, tr!("Disk {name}: {free} free of {total}", name = name, free = bytes(available), total = bytes(total)));
            out.push(if red { a.notification("system:disk", tr("Disk almost full")) } else { a.notify(false) });
        }
    }
    let loss = num(network, "loss").unwrap_or(0.0);
    if loss > LOSS {
        out.push(Alert::new(kind, Severity::Warn, tr!("Network: {n} % of the handshakes lost over 5 min", n = round(loss))).notify(false));
    }
    out
}

/* ---------- Data ---------- */

const CONSOLE: &str = "https://console.cloud.google.com";
const TIB: f64 = 1_099_511_627_776.0;

/// What is wrong with one job, table, scheduled query or stream: broken (`ko`) or drifting.
struct Problem {
    ko: bool,
    label: String,
    why: Option<String>,
}

impl Problem {
    fn ko(label: impl Into<String>, why: impl Into<String>) -> Option<Self> {
        Some(Problem { ko: true, label: label.into(), why: Some(why.into()) })
    }

    fn warn(label: impl Into<String>, why: Option<String>) -> Option<Self> {
        Some(Problem { ko: false, label: label.into(), why })
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let m = values.len() / 2;
    Some(if values.len() % 2 == 1 { values[m] } else { (values[m - 1] + values[m]) / 2.0 })
}

/// Duration of a run in milliseconds; while running, up to now.
fn run_duration(e: &Value, now: DateTime<Utc>) -> Option<f64> {
    let start = date(e, "started_at")?;
    Some((date(e, "finished_at").unwrap_or(now) - start).num_milliseconds() as f64)
}

/// A Cloud Run job: its last run failed (and how many in a row), a run missed, or far longer than usual.
fn job_problem(j: &Value, slow_factor: f64, now: DateTime<Utc>) -> Option<Problem> {
    let runs = list(j, "executions");
    if let Some(error) = j.get("trigger").and_then(|t| opt_text(t, "error")).filter(|e| !e.is_empty()) {
        return Problem::ko(tr("run missed"), tr!("Cloud Scheduler could not start it: {error}", error = error));
    }
    let last = runs.first()?;
    let status = text(last, "status");
    if status == "failed" {
        let streak = runs.iter().position(|e| text(e, "status") != "failed").unwrap_or(runs.len());
        let failed = tn!(num(last, "tasks").unwrap_or(0.0) as i64, "{failed}/{n} task failed", "{failed}/{n} tasks failed", failed = js_number(num(last, "failed_tasks").unwrap_or(0.0)));
        let why = match num(last, "exit_code") {
            Some(code) => tr!("{failed}, exit code {code}", failed = failed, code = js_number(code)),
            None => failed,
        };
        return Problem::ko(if streak > 1 { tr!("failed ×{n}", n = streak) } else { tr("failed").to_string() }, why);
    }
    let med = median(runs.iter().skip(1).filter(|e| text(e, "status") == "succeeded").filter_map(|e| run_duration(e, now)).collect()).filter(|m| *m != 0.0);
    let slow = match (med, run_duration(last, now)) {
        (Some(m), Some(d)) if d > m * slow_factor => Some(d / m),
        _ => None,
    };
    let times = |x: f64| tr!("{x}× median", x = to_fixed(x, 1));
    match (status, slow) {
        ("running", Some(x)) => Problem::warn(times(x), Some(tr("still running, far longer than usual").into())),
        ("cancelled", _) => Problem::warn(tr("cancelled"), None),
        (_, Some(x)) if status != "running" => Problem::warn(times(x), Some(tr("the last run took far longer than usual").into())),
        _ => None,
    }
}

/// A BigQuery table: unreadable, or late (red when the job that feeds it failed).
fn table_problem(tb: &Value, jobs: &[Value], slow_factor: f64, now: DateTime<Utc>) -> Option<Problem> {
    if let Some(error) = opt_text(tb, "error").filter(|e| !e.is_empty()) {
        return Problem::warn(tr("unreadable"), Some(error.into()));
    }
    let age = (now - date(tb, "last_modified")?).num_milliseconds() as f64 / 3_600_000.0;
    let max = num(tb, "max_age_hours").unwrap_or(0.0);
    if age <= max {
        return None;
    }
    let late = tr!("late {h}h", h = round(age - max));
    let producer = opt_text(tb, "fed_by").filter(|f| !f.is_empty()).and_then(|f| jobs.iter().find(|j| text(j, "name") == f));
    match producer.filter(|p| job_problem(p, slow_factor, now).is_some_and(|x| x.ko)) {
        Some(p) => Problem::ko(late, tr!("its producer {name} failed", name = text(p, "name"))),
        None => Problem::warn(late, Some(tr!("not modified for {age}h (expected every {max}h)", age = round(age), max = js_number(max)))),
    }
}

/// A scheduled query: its last run failed or was cancelled.
fn transfer_problem(q: &Value) -> Option<Problem> {
    if flag(q, "disabled") {
        return None;
    }
    match text(q, "state") {
        "FAILED" => Problem::ko(tr("failed"), tr("its last run failed")),
        "CANCELLED" => Problem::warn(tr("cancelled"), None),
        _ => None,
    }
}

/// `18 min`, `42 s`, `2 h 05`.
fn lag(seconds: f64) -> String {
    if seconds < 90.0 {
        return format!("{} s", round(seconds));
    }
    let m = round(seconds / 60.0);
    if m < 90 { format!("{m} min") } else { format!("{} h {:02}", m / 60, m % 60) }
}

/// A Datastream stream: failed, lagging beyond `[data] stream_lag_minutes`, or with errors.
fn stream_problem(s: &Value, lag_minutes: f64) -> Option<Problem> {
    let state = text(s, "state");
    let errors: Vec<&str> = list(s, "errors").iter().filter_map(Value::as_str).collect();
    if state.starts_with("FAILED") {
        return Problem::ko(state, errors.first().map_or_else(|| tr("the stream stopped on an error").to_string(), |e| e.to_string()));
    }
    if state == "PAUSED" || state == "NOT_STARTED" {
        return None;
    }
    let limit = lag_minutes * 60.0;
    if let Some(fresh) = num(s, "freshness_seconds").filter(|f| *f > limit) {
        return Problem::warn(tr!("lag {lag}", lag = lag(fresh)), Some(tr!("{lag} behind (threshold {limit})", lag = lag(fresh), limit = lag(limit))));
    }
    (!errors.is_empty()).then(|| Problem { ko: false, label: state.into(), why: Some(errors.join(", ")) })
}

/// `$1234`, `$12.40`.
fn money(v: f64) -> String {
    if v >= 100.0 { format!("${}", round(v)) } else { format!("${}", to_fixed(v, 2)) }
}

/// Data: what is broken (job failed or not started, late table whose job failed, stream failed)
/// notifies; what drifts (slow job, late table, lagging stream, cost spike) does not.
pub fn data(d: &Value) -> Vec<Alert> {
    let now = Utc::now();
    let th = d.get("thresholds").cloned().unwrap_or_default();
    let (slow_factor, lag_minutes, spike_factor) = (num(&th, "slow_factor").unwrap_or(0.0), num(&th, "stream_lag_minutes").unwrap_or(0.0), num(&th, "cost_spike_factor").unwrap_or(0.0));
    let jobs = list(d, "jobs");
    let mut found: Vec<(String, Problem, Option<String>)> = Vec::new();
    for j in jobs {
        if let Some(p) = job_problem(j, slow_factor, now) {
            found.push((text(j, "name").into(), p, Some(format!("{CONSOLE}/run/jobs/details/{}/{}/executions?project={}", text(j, "region"), text(j, "name"), text(j, "project")))));
        }
    }
    for tb in list(d, "tables") {
        if let Some(p) = table_problem(tb, jobs, slow_factor, now) {
            let table = text(tb, "table");
            let mut parts = table.split('.');
            let (p_, ds, t_) = (parts.next().unwrap_or_default(), parts.next().unwrap_or("undefined"), parts.next().unwrap_or("undefined"));
            found.push((table.into(), p, Some(format!("{CONSOLE}/bigquery?project={p_}&ws=!1m5!1m4!4m3!1s{p_}!2s{ds}!3s{t_}"))));
        }
    }
    for q in list(d, "transfers") {
        if let Some(p) = transfer_problem(q) {
            found.push((text(q, "name").into(), p, Some(format!("https://console.cloud.google.com/bigquery/scheduled-queries?project={}", text(q, "project")))));
        }
    }
    for s in list(d, "streams") {
        if let Some(p) = stream_problem(s, lag_minutes) {
            found.push((text(s, "display_name").into(), p, Some(format!("{CONSOLE}/datastream/streams/locations/{}/instances/{}?project={}", text(s, "region"), text(s, "name"), text(s, "project")))));
        }
    }
    if let Some(cost) = d.get("cost").filter(|c| c.is_object()) {
        let days = list(cost, "days");
        let today = days.last().and_then(|x| num(x, "bytes")).unwrap_or(0.0);
        let med = median(days.iter().take(days.len().saturating_sub(1)).filter(|x| num(x, "jobs").unwrap_or(0.0) > 0.0).filter_map(|x| num(x, "bytes")).collect()).filter(|m| *m != 0.0);
        let price = num(cost, "price_per_tib").unwrap_or(0.0);
        if let Some(spike) = med.map(|m| today / m).filter(|r| *r > spike_factor) {
            let why = tr!("{today} today, median {median}", today = money(today / TIB * price), median = money(med.unwrap_or(0.0) / TIB * price));
            found.push((tr("BigQuery cost").into(), Problem { ko: false, label: tr!("{x}× median", x = to_fixed(spike, 1)), why: Some(why) }, None));
        }
    }
    // The broken first.
    found.sort_by_key(|(_, p, _)| !p.ko);
    found
        .into_iter()
        .map(|(name, p, url)| {
            let mut said = tr!("{name}: {state}", name = name, state = p.label);
            if let Some(why) = p.why.filter(|w| !w.is_empty()) {
                said += &format!(" — {why}");
            }
            let a = Alert::new(tr("Data"), if p.ko { Severity::Ko } else { Severity::Warn }, said).notify(p.ko).notification(format!("data:{name}"), tr("Data pipeline"));
            match url {
                Some(u) => a.url(u),
                None => a,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_containers_failing_and_disk_to_reclaim() {
        let d = json!({ "server": "28.0", "containers": [
            { "name": "db", "project": "shop", "state": "running", "health": "unhealthy" },
            { "name": "job", "state": "exited", "exit_code": 2 },
            { "name": "stopped", "state": "exited", "exit_code": 137 },
            { "name": "fine", "state": "exited", "exit_code": 0 }
        ], "disk": [{ "reclaimable_bytes": 30e9 }], "reclaimable_gb": 20 });
        let list = docker(&d);
        assert_eq!(list.iter().map(|a| a.text.as_str()).collect::<Vec<_>>(), ["Container db (shop) is unhealthy", "Container job is exited (2)", "30.0 GB of Docker data unused and reclaimable"]);
        assert_eq!(list[0].notification.as_ref().unwrap().key, "docker:db");
        assert!(docker(&json!({ "server": null, "containers": [{ "name": "x", "state": "dead" }] })).is_empty());
    }

    #[test]
    fn a_dev_server_stopped_on_an_error() {
        let d = json!({ "servers": [{ "key": "k", "agent": "a", "shell": 2, "command": "npm run dev", "state": "stopped", "failed": true, "tail": "Error" }, { "key": "j", "agent": "a", "shell": null, "command": "x", "state": "running", "failed": false }] });
        let list = dev_servers(&d);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].text, "Dev server npm run dev stopped on an error (shell 2)");
        assert_eq!(list[0].fixes, Some(vec![Fix::DevServer { agent: "a".into(), command: "npm run dev".into(), place: "shell 2".into(), tail: "Error".into() }]));
    }

    #[test]
    fn custom_blocks_waiting_for_trust_and_their_alerts() {
        let s = json!({ "blocks": [
            { "id": "nas", "label": "NAS", "output": { "alerts": [{ "severity": "ko", "text": "Laptop not backed up", "key": "laptop" }, { "severity": "ok", "text": "fine", "key": "x" }] } },
            { "id": "new", "label": "New", "pending": { "since": "2026-11-09T10:00:00Z", "agents": ["a1"] } }
        ] });
        let list = custom(&s);
        assert_eq!(list.iter().map(|a| a.text.as_str()).collect::<Vec<_>>(), ["NAS: Laptop not backed up", "Custom block New waits for your trust before it runs"]);
        assert_eq!((list[0].short.as_deref(), list[0].item.as_deref()), (Some("Laptop not backed up"), Some("nas")));
        assert_eq!(list[1].notification.as_ref().unwrap().key, "custom:new:trust");
        assert!(list[1].since.is_some());
    }

    #[test]
    fn the_microphone_too_low_red_while_recording() {
        let mut s = json!({ "updated_at": "2026-11-09T10:00:00Z", "input": { "name": "MacBook", "volume": 20, "muted": false }, "recording": [], "limits": { "min_input_gain": 40, "muted_under": 3, "restore_gain": 70 } });
        let list = sound(&s);
        assert_eq!(list[0].text, "Microphone MacBook’s input volume is at 20 % (under 40 %)");
        assert_eq!((list[0].severity, list[0].notify), (Severity::Warn, Some(false)));
        s["recording"] = json!([{ "app": "Zoom" }, { "app": "Slack" }]);
        let list = sound(&s);
        assert_eq!(list[0].text, "Microphone MacBook’s input volume is at 20 % (under 40 %) while Zoom and Slack records");
        assert_eq!(list[0].severity, Severity::Ko);
        assert!(matches!(&list[0].fixes.as_ref().unwrap()[0], Fix::Block { args, .. } if args["value"] == 70));
        s["input"]["volume"] = json!(2);
        assert!(sound(&s).is_empty(), "a boom folded up");
    }

    #[test]
    fn incidents_under_way() {
        let s = json!({ "alerts": true, "services": [{ "id": "gh", "name": "GitHub", "incidents": [{ "id": "1", "name": "Actions slow", "impact": "major", "status": "investigating", "maintenance": false }, { "id": "2", "name": "Upgrade", "impact": "minor", "status": "", "maintenance": true }] }] });
        let list = status(&s);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].text, "GitHub incident: Actions slow (investigating)");
        assert_eq!(list[0].notification.as_ref().unwrap().title, "GitHub status");
    }

    #[test]
    fn the_computer_short_of_memory_cpu_disk_or_network() {
        let s = json!({ "alerts": true, "updated_at": "2026-11-09T10:00:00Z", "memory": { "pressure": "critical" }, "cpu": { "usage": 93.4, "hot_seconds": 420 }, "disks": [{ "name": "Macintosh HD", "mount": "/", "total": 500.0 * GIB, "available": 8.0 * GIB }], "network": { "loss": 7.6 } });
        let list = system(&s);
        assert_eq!(list.iter().map(|a| a.text.as_str()).collect::<Vec<_>>(), ["Memory pressure is critical: the computer is swapping, close something", "CPU at 93 % for 7 min", "Disk Macintosh HD: 8.0 GB free of 500 GB", "Network: 8 % of the handshakes lost over 5 min"]);
        assert_eq!(list[2].severity, Severity::Ko);
    }

    #[test]
    fn data_jobs_failing_tables_late_and_streams_lagging() {
        let d = json!({
            "jobs": [{ "project": "p", "region": "eu", "name": "load", "executions": [{ "status": "failed", "tasks": 2, "failed_tasks": 1, "exit_code": 1 }, { "status": "failed", "tasks": 1, "failed_tasks": 1, "exit_code": null }] }],
            "tables": [{ "table": "p.d.t", "max_age_hours": 2, "fed_by": "load", "last_modified": "2020-01-01T00:00:00Z", "error": null }],
            "transfers": [],
            "streams": [{ "project": "p", "region": "eu", "name": "s", "display_name": "orders", "state": "RUNNING", "errors": [], "freshness_seconds": 1800 }],
            "cost": null,
            "thresholds": { "slow_factor": 3, "stream_lag_minutes": 15, "cost_spike_factor": 3 }
        });
        let list = data(&d);
        assert_eq!(list[0].text, "load: failed ×2 — 1/2 tasks failed, exit code 1");
        assert!(list[1].text.starts_with("p.d.t: late ") && list[1].text.ends_with("— its producer load failed"));
        assert_eq!(list[2].text, "orders: lag 30 min — 30 min behind (threshold 15 min)");
        assert_eq!((list[2].severity, list[2].notify), (Severity::Warn, Some(false)));
        assert_eq!(list[0].url.as_deref(), Some("https://console.cloud.google.com/run/jobs/details/eu/load/executions?project=p"));
    }
}
