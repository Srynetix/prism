//! CI health over the window, for the main branch, the release branches and merge request
//! pipelines: pipeline duration and success, runner wait, slowest jobs and flaky jobs (failed then
//! succeeded on retry within the same pipeline). The forge's collector reads the pipelines
//! (`gitlab::ci_health`, `github::ci_health`); the figures, the last 7 days against the 7
//! before, are computed here alike.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};

use crate::state::{CiDay, CiScope, FlakyJob, SlowJob};

pub const SOURCE: &str = "ci_health";

/// CI health window (`[forge] ci_health_days`), set when the collector starts.
static WINDOW: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(14);
pub fn window_days() -> i64 {
    WINDOW.load(std::sync::atomic::Ordering::Relaxed)
}
pub fn set_window(days: u32) {
    WINDOW.store(days.max(1).into(), std::sync::atomic::Ordering::Relaxed);
}
/// Slowest jobs and flaky jobs displayed, per scope.
const TOP: usize = 6;
/// A job must have run this many times over the week to be ranked among the slowest.
const MIN_RUNS: usize = 3;

/// A pipeline of the window, with its jobs (retried attempts included).
pub struct Run {
    pub at: DateTime<Utc>,
    pub status: String,
    pub duration: Option<f64>,
    pub jobs: Vec<Job>,
}

pub struct Job {
    pub name: String,
    pub status: String,
    pub duration: Option<f64>,
    /// Seconds waited for a runner.
    pub queued: Option<f64>,
    /// An attempt a later one replaced.
    pub retried: bool,
    pub url: Option<String>,
}

pub const SCOPES: [&str; 3] = ["main", "release", "mr"];

/// Label of a scope: the main branch, the release branches, or the MRs.
pub fn label(key: &str, main: &str, release_prefix: &str) -> String {
    let c = crate::conventions::get();
    match key {
        "main" => main.to_string(),
        "release" => format!("{release_prefix}*"),
        _ if c.forge == crate::forge::Provider::Github => "PRs".into(),
        _ => "MRs".into(),
    }
}

/// A pipeline that went all the way: `manual` only waits for its manual jobs (deployments).
const PASSED: [&str; 2] = ["success", "manual"];
/// A job that actually ran.
const RAN: [&str; 2] = ["success", "failed"];

pub fn scope(key: &str, label: &str, runs: Vec<&Run>, now: DateTime<Utc>) -> CiScope {
    let week = now - chrono::Duration::days(7);
    let (cur, prev): (Vec<&Run>, Vec<&Run>) = runs.iter().partition(|r| r.at >= week);
    let durations = |rs: &[&Run]| rs.iter().filter(|r| PASSED.contains(&r.status.as_str())).filter_map(|r| r.duration).collect::<Vec<_>>();
    let queued = |rs: &[&Run]| rs.iter().flat_map(|r| &r.jobs).filter(|j| RAN.contains(&j.status.as_str())).filter_map(|j| j.queued).collect::<Vec<_>>();
    let today = now.date_naive();
    let days = (0..window_days())
        .rev()
        .map(|n| {
            let date = today - chrono::Duration::days(n);
            let rs: Vec<&Run> = runs.iter().copied().filter(|r| r.at.date_naive() == date).collect();
            CiDay { date, pipelines: rs.len() as u32, duration: median(durations(&rs)), success: success(&rs), queue: median(queued(&rs)) }
        })
        .collect();
    CiScope {
        key: key.into(),
        label: label.into(),
        pipelines: cur.len() as u32,
        pipelines_prev: prev.len() as u32,
        duration: median(durations(&cur)),
        duration_prev: median(durations(&prev)),
        success: success(&cur),
        success_prev: success(&prev),
        queue: median(queued(&cur)),
        queue_p90: percentile(queued(&cur), 0.9),
        days,
        slow_jobs: slow_jobs(&runs, week, today),
        flaky: flaky(&runs),
    }
}

/// Share of pipelines that went all the way among those that succeeded or failed (canceled excluded).
fn success(runs: &[&Run]) -> Option<f64> {
    let passed = runs.iter().filter(|r| PASSED.contains(&r.status.as_str())).count();
    let failed = runs.iter().filter(|r| r.status == "failed").count();
    (passed + failed > 0).then(|| passed as f64 / (passed + failed) as f64)
}

/// Longest successful jobs over the week (median), with the week before and each day's median.
fn slow_jobs(runs: &[&Run], week: DateTime<Utc>, today: NaiveDate) -> Vec<SlowJob> {
    let mut by_name: HashMap<&str, Vec<(DateTime<Utc>, f64)>> = HashMap::new();
    for r in runs {
        for j in r.jobs.iter().filter(|j| j.status == "success") {
            if let Some(d) = j.duration {
                by_name.entry(&j.name).or_default().push((r.at, d));
            }
        }
    }
    let mut out: Vec<SlowJob> = by_name
        .into_iter()
        .filter_map(|(name, all)| {
            let cur: Vec<f64> = all.iter().filter(|(at, _)| *at >= week).map(|(_, d)| *d).collect();
            if cur.len() < MIN_RUNS {
                return None;
            }
            let days = (0..window_days())
                .rev()
                .map(|n| median(all.iter().filter(|(at, _)| at.date_naive() == today - chrono::Duration::days(n)).map(|(_, d)| *d).collect()))
                .collect();
            Some(SlowJob {
                name: name.to_string(),
                runs: cur.len() as u32,
                duration: median(cur)?,
                duration_prev: median(all.iter().filter(|(at, _)| *at < week).map(|(_, d)| *d).collect()),
                days,
            })
        })
        .collect();
    out.sort_by(|a, b| b.duration.total_cmp(&a.duration));
    out.truncate(TOP);
    out
}

/// Jobs failed then succeeded on retry within the same pipeline, over the whole window.
fn flaky(runs: &[&Run]) -> Vec<FlakyJob> {
    let mut by_name: HashMap<&str, FlakyJob> = HashMap::new();
    // Each job's failures, to keep the most recent.
    let mut failures: HashMap<&str, Vec<(DateTime<Utc>, String)>> = HashMap::new();
    for r in runs {
        let mut attempts: HashMap<&str, Vec<&Job>> = HashMap::new();
        for j in &r.jobs {
            attempts.entry(&j.name).or_default().push(j);
        }
        for (name, tries) in attempts {
            if !tries.iter().any(|j| RAN.contains(&j.status.as_str())) {
                continue;
            }
            let entry = by_name.entry(name).or_insert_with(|| FlakyJob { name: name.to_string(), flakes: 0, runs: 0, last_at: r.at, urls: Vec::new() });
            entry.runs += 1;
            let failed = tries.iter().find(|j| j.retried && j.status == "failed");
            if let (Some(f), true) = (failed, tries.iter().any(|j| !j.retried && j.status == "success")) {
                entry.flakes += 1;
                if entry.flakes == 1 || r.at > entry.last_at {
                    entry.last_at = r.at;
                }
                failures.entry(name).or_default().extend(f.url.clone().map(|u| (r.at, u)));
            }
        }
    }
    let mut out: Vec<FlakyJob> = by_name.into_values().filter(|f| f.flakes > 0).collect();
    for f in &mut out {
        let mut urls = failures.remove(f.name.as_str()).unwrap_or_default();
        urls.sort_by_key(|a| std::cmp::Reverse(a.0));
        f.urls = urls.into_iter().take(3).map(|(_, u)| u).collect();
    }
    out.sort_by(|a, b| b.flakes.cmp(&a.flakes).then(b.last_at.cmp(&a.last_at)));
    out.truncate(TOP);
    out
}

fn median(values: Vec<f64>) -> Option<f64> {
    percentile(values, 0.5)
}

/// Value at rank `p` (0 to 1) of the sorted values, the nearest.
fn percentile(mut values: Vec<f64>, p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let i = ((values.len() - 1) as f64 * p).round() as usize;
    Some(values[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(name: &str, status: &str, retried: bool, duration: f64) -> Job {
        Job { name: name.into(), status: status.into(), duration: Some(duration), queued: Some(10.0), retried, url: Some(format!("u-{name}-{status}")) }
    }

    fn run(days_ago: i64, status: &str, jobs: Vec<Job>) -> Run {
        Run { at: Utc::now() - chrono::Duration::days(days_ago), status: status.into(), duration: Some(600.0), jobs }
    }

    #[test]
    fn a_job_failed_then_passed_on_retry_is_flaky() {
        let runs = [
            run(1, "success", vec![job("e2e", "failed", true, 300.0), job("e2e", "success", false, 310.0), job("lint", "success", false, 20.0)]),
            run(2, "failed", vec![job("e2e", "failed", false, 300.0)]),
            run(3, "success", vec![job("e2e", "success", false, 290.0)]),
        ];
        let f = flaky(&runs.iter().collect::<Vec<_>>());
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].name.as_str(), f[0].flakes, f[0].runs), ("e2e", 1, 3));
        assert_eq!(f[0].urls, ["u-e2e-failed"]);
    }

    #[test]
    fn success_rate_ignores_canceled_and_counts_manual_as_passed() {
        let runs = [run(1, "success", vec![]), run(1, "manual", vec![]), run(1, "failed", vec![]), run(1, "canceled", vec![])];
        assert_eq!(success(&runs.iter().collect::<Vec<_>>()), Some(2.0 / 3.0));
    }

    #[test]
    fn slow_jobs_need_enough_runs_this_week() {
        let runs: Vec<Run> = (0..4).map(|d| run(d, "success", vec![job("test", "success", false, 400.0 + d as f64), job("build", "success", false, 900.0)])).collect();
        let mut few = runs;
        few[0].jobs.pop();
        few[1].jobs.pop();
        let slow = slow_jobs(&few.iter().collect::<Vec<_>>(), Utc::now() - chrono::Duration::days(7), Utc::now().date_naive());
        assert_eq!(slow.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["test"]);
    }

    #[test]
    fn percentiles() {
        assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(percentile((1..=10).map(f64::from).collect(), 0.9), Some(9.0));
        assert_eq!(median(Vec::new()), None);
    }
}
