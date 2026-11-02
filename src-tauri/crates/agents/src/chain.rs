//! What an agent's chain runs after it (`chains` drives the steps): a task
//! in its folder, its exit code and the end of its output; and its branch pushed with a draft MR.
//! Never a merge, never a push to a protected branch.

use std::{path::Path, time::Duration};

use anyhow::{bail, Context, Result};
use prism_base::{i18n::tr, tr};
use serde::{Deserialize, Serialize};

use crate::state::Snapshot;

/// A task taking longer is stopped: a chain does not wait forever.
const TASK_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// The end of a task's output kept: what goes back to the agent when it fails.
const TAIL_LINES: usize = 60;
const TAIL_BYTES: usize = 8 * 1024;

#[derive(Debug, Serialize, PartialEq)]
pub struct TaskRun {
    /// The exit code; absent when it was stopped (timeout).
    pub code: Option<i32>,
    /// The end of its output (standard output and error together).
    pub tail: String,
    pub ms: u64,
}

/// Runs `command` in `dir` through the user's login shell (its PATH, its tools), up to 30 minutes.
pub async fn task(dir: &Path, command: &str) -> Result<TaskRun> {
    let started = std::time::Instant::now();
    let mut cmd = if cfg!(windows) {
        let mut c = crate::platform::command("cmd");
        c.args(["/C", command]);
        c
    } else {
        let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/sh".into());
        let mut c = crate::platform::command(shell);
        c.args(["-lc", &format!("{command} 2>&1")]);
        c
    };
    cmd.current_dir(dir).stdin(std::process::Stdio::null()).kill_on_drop(true);
    let out = match tokio::time::timeout(TASK_TIMEOUT, cmd.output()).await {
        Ok(out) => out.with_context(|| format!("running {command}"))?,
        Err(_) => return Ok(TaskRun { code: None, tail: format!("Stopped after {} minutes.", TASK_TIMEOUT.as_secs() / 60), ms: started.elapsed().as_millis() as u64 }),
    };
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    Ok(TaskRun { code: out.status.code(), tail: tail(&text), ms: started.elapsed().as_millis() as u64 })
}

/// The last lines of `text`, at most `TAIL_BYTES`.
fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n");
    if out.len() > TAIL_BYTES {
        let cut = out.len() - TAIL_BYTES;
        let cut = (cut..out.len()).find(|i| out.is_char_boundary(*i)).unwrap_or(out.len());
        out = out[cut..].to_string();
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenedMr {
    pub iid: u64,
    pub url: String,
    pub target: String,
    /// Its repository (`[[repos]] name`); absent: the primary one.
    #[serde(default)]
    pub repo: Option<String>,
    /// An agent in several repositories: the MRs opened in the others, each linking to the rest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<OpenedMr>,
    /// Where it lives and what it says, to link it to the others after.
    #[serde(skip)]
    project: String,
    #[serde(skip)]
    body: String,
}

/// The output of `git`, trimmed: a branch, a commit.
async fn git(dir: &Path, args: &[&str]) -> Result<String> {
    Ok(crate::git::run(dir, args).await?.trim().to_string())
}

/// Pushes the branch of the agent's folder and opens a draft MR (a draft pull request on GitHub)
/// into the branch it was made from: titled from its ticket and title, described by its commits.
/// None: no commit beyond that branch, nothing to open.
pub async fn open_mr(dir: &Path, title: &str, ticket: Option<&str>, snap: &Snapshot) -> Result<Option<OpenedMr>> {
    let branch = git(dir, &["branch", "--show-current"]).await?;
    let c = &snap.conventions;
    if branch.is_empty() {
        bail!(tr("the agent's folder is on no branch"));
    }
    if branch == c.main_branch || (!c.release_prefix.is_empty() && branch.starts_with(&c.release_prefix)) {
        bail!(tr!("{branch} is a protected branch: Prism pushes no MR from it", branch = branch));
    }
    // The branch it was made from (the worktree's base), or the main branch.
    let path = dir.display().to_string();
    let wt = snap.worktrees.iter().find(|w| w.path == path);
    let target = wt.map(|w| w.base.trim_start_matches("origin/").to_string()).filter(|b| !b.is_empty() && *b != branch).unwrap_or_else(|| c.main_branch.clone());
    let repo = wt.map(|w| w.repo.clone());
    let project = snap
        .repositories
        .iter()
        .find(|r| repo.as_deref().map_or(r.primary, |n| r.name == n))
        .and_then(|r| r.project.clone())
        .context(tr("the agent's repository has no project ([[repos]] project)"))?;
    let commits = git(dir, &["log", "--no-merges", "--format=- %s", &format!("origin/{target}..HEAD")]).await.unwrap_or_default();
    if commits.is_empty() {
        return Ok(None);
    }
    // No hook: a `core.hooksPath` relative to the worktree (Husky's `.husky/_`) would run a
    // pre-push the branch or the agent put there.
    git(dir, &["-c", "core.hooksPath=/dev/null", "push", "--no-verify", "-u", "origin", &branch]).await?;
    let name = title.split(" · ").last().unwrap_or(title).trim();
    let title = match ticket {
        Some(t) if !name.contains(t) => format!("{t} {name}"),
        _ => name.to_string(),
    };
    let body = format!("{commits}\n\nOpened by Prism at the end of an agent's run; still a draft until reviewed.");
    match snap.forge {
        crate::forge::Provider::Github => {
            #[derive(Deserialize)]
            struct Pull {
                number: u64,
                html_url: String,
            }
            let p: Pull = crate::collectors::github::gh_input("POST", &format!("repos/{project}/pulls"), &serde_json::json!({ "head": branch, "base": target, "title": title, "body": body, "draft": true })).await?;
            Ok(Some(OpenedMr { iid: p.number, url: p.html_url, target, repo, also: Vec::new(), project, body }))
        }
        _ => {
            #[derive(Deserialize)]
            struct Mr {
                iid: u64,
                web_url: String,
            }
            let path = format!("projects/{}/merge_requests", crate::collectors::gitlab::encode(&project));
            let m: Mr = crate::collectors::gitlab::glab_input("POST", &path, &serde_json::json!({ "source_branch": branch, "target_branch": target, "title": format!("Draft: {title}"), "description": body, "remove_source_branch": true })).await?;
            Ok(Some(OpenedMr { iid: m.iid, url: m.web_url, target, repo, also: Vec::new(), project, body }))
        }
    }
}

/// The MRs of one agent in several repositories: each description ends with the others, so that
/// a reviewer sees what goes with what. A link that fails is said, the MRs stay.
pub async fn link_mrs(mrs: &[OpenedMr], snap: &Snapshot) -> Result<()> {
    for m in mrs {
        let others: Vec<String> = mrs.iter().filter(|o| o.url != m.url).map(|o| format!("- {} ({})", o.url, o.repo.as_deref().unwrap_or("?"))).collect();
        let body = format!("{}\n\nGoes with:\n{}", m.body, others.join("\n"));
        match snap.forge {
            crate::forge::Provider::Github => {
                let _: serde_json::Value = crate::collectors::github::gh_input("PATCH", &format!("repos/{}/pulls/{}", m.project, m.iid), &serde_json::json!({ "body": body })).await?;
            }
            _ => {
                let path = format!("projects/{}/merge_requests/{}", crate::collectors::gitlab::encode(&m.project), m.iid);
                let _: serde_json::Value = crate::collectors::gitlab::glab_input("PUT", &path, &serde_json::json!({ "description": body })).await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_end_of_the_output() {
        let text = (1..=100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let t = tail(&text);
        assert!(t.starts_with("line 41") && t.ends_with("line 100"));
    }

    #[tokio::test]
    async fn a_task_s_exit_code_and_output() {
        let dir = std::env::temp_dir();
        let ok = task(&dir, "echo hello").await.unwrap();
        assert_eq!((ok.code, ok.tail.as_str()), (Some(0), "hello"));
        let failed = task(&dir, "echo boom; exit 3").await.unwrap();
        assert_eq!((failed.code, failed.tail.as_str()), (Some(3), "boom"));
    }
}
