//! A pipeline's jobs and a job's log, read on demand when the window displays them: never
//! collected in the background nor kept in the snapshot (a log may contain badly masked
//! secrets, it never leaves the panel's memory).

use prism_base::tr;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use anyhow::bail;

use super::{api, encode, glab, glab_text};
use crate::forge::{JobInfo, JobLog};

/// Beyond this, only the end of the log is kept: that is where the error is.
const LOG_MAX_BYTES: usize = 512 * 1024;

/// A pipeline's jobs, in GitLab's order (without retried attempts).
pub async fn pipeline_jobs(project: &str, pipeline: u64) -> Result<Vec<JobInfo>> {
    let jobs: Vec<RawJob> = api(&format!("projects/{}/pipelines/{pipeline}/jobs?per_page=100", encode(project))).await?;
    Ok(jobs.into_iter().map(JobInfo::from).collect())
}

pub async fn job_log(project: &str, job: u64) -> Result<JobLog> {
    let p = encode(project);
    let info: RawJob = api(&format!("projects/{p}/jobs/{job}")).await?;
    let log = glab_text(&format!("projects/{p}/jobs/{job}/trace")).await?;
    let (log, truncated) = tail(log, LOG_MAX_BYTES);
    Ok(JobLog { job: info.into(), log, truncated })
}

/// Retries a failed or canceled job; returns the new job (GitLab creates one on each retry).
pub async fn retry_job(project: &str, job: u64) -> Result<JobInfo> {
    let p = encode(project);
    // The job is re-read from GitLab: only a finished job of the project (failed, canceled, or successful:
    // redeploying staging, for example) is retried.
    let current: RawJob = api(&format!("projects/{p}/jobs/{job}")).await?;
    if !matches!(current.status.as_str(), "failed" | "canceled" | "success") {
        bail!("{}", tr!("job {job} is {status}: nothing to retry", job = current.name, status = current.status));
    }
    tracing::info!("gitlab: retry job {job} ({})", current.name);
    let next: RawJob = glab(&["api".into(), "--method".into(), "POST".into(), format!("projects/{p}/jobs/{job}/retry")]).await?;
    Ok(next.into())
}

/// End of the text, cut at the start of a line so as not to begin in the middle of an ANSI code.
pub fn tail(text: String, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let start = text[start..].find('\n').map(|i| start + i + 1).unwrap_or(start);
    (text[start..].to_string(), true)
}

#[derive(Deserialize)]
struct RawJob {
    id: u64,
    name: String,
    stage: String,
    status: String,
    #[serde(default)]
    allow_failure: bool,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    duration: Option<f64>,
    web_url: String,
    #[serde(default)]
    r#ref: String,
    pipeline: RawPipelineRef,
}

#[derive(Deserialize)]
struct RawPipelineRef {
    id: u64,
}

impl From<RawJob> for JobInfo {
    fn from(j: RawJob) -> Self {
        JobInfo {
            id: j.id,
            name: j.name,
            stage: j.stage,
            status: crate::state::PipelineStatus::of(&j.status),
            allow_failure: j.allow_failure,
            started_at: j.started_at,
            finished_at: j.finished_at,
            duration: j.duration,
            url: j.web_url,
            pipeline_id: j.pipeline.id,
            r#ref: j.r#ref,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_logs_are_kept_whole() {
        assert_eq!(tail("a\nb\n".into(), 10), ("a\nb\n".into(), false));
    }

    #[test]
    fn long_logs_keep_their_last_whole_lines() {
        let (log, truncated) = tail("first line\nsecond\nthird\n".into(), 10);
        assert!(truncated);
        assert_eq!(log, "third\n");
    }

    #[test]
    fn cut_respects_multibyte_characters() {
        let (log, truncated) = tail("ééééé\nfin\n".into(), 7);
        assert!(truncated);
        assert_eq!(log, "fin\n");
    }
}
