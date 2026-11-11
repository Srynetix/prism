//! What the window does on GitLab: merge requests (rebase, merge, auto-merge, labels, review,
//! comments, people, its pipeline) and pipelines (cancel, retry, play a job), plus the reads
//! that go with them (detail, diff, logs). The snapshot follows right away.

use super::*;

/// GitLab actions triggered from the window. The project is only known once the config is read.
/// Projects whose pipelines a block reads (a deployment project): their logs can be read too.
static LOG_PROJECTS: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

/// Lets the window read the logs of another project's pipelines, which a block shows.
#[allow(dead_code, reason = "for the blocks of another repository (`blocks::sdk`)")]
pub fn allow_log_project(project: &str) {
    let mut list = LOG_PROJECTS.lock().expect("log projects");
    if !list.contains(&project) {
        list.push(Box::leak(project.to_string().into_boxed_str()));
    }
}

#[derive(Clone, Default)]
pub struct Actions {
    project: Arc<prism_base::rt::Swap<String>>,
    merge_queue: Arc<prism_base::rt::Swap<String>>,
    /// Prefix of the labels that order the merge queue (`merge-priority::`), empty for none.
    merge_priority: Arc<prism_base::rt::Swap<String>>,
    /// Non-primary repositories whose MRs Prism follows: (name, GitLab project).
    repos: Arc<prism_base::rt::Swap<Vec<(String, String)>>>,
    /// Every repository's switches (merge queue).
    infos: Arc<prism_base::rt::Swap<Vec<crate::config::RepoInfo>>>,
}

impl Actions {
    pub fn configure(&self, cfg: &GitlabConfig, repos: Vec<(String, String)>, infos: Vec<crate::config::RepoInfo>) {
        self.project.set(cfg.project.clone());
        self.infos.set(infos);
        self.repos.set(repos);
        self.merge_queue.set(cfg.merge_queue_label.clone());
        self.merge_priority.set(cfg.merge_priority_prefix.clone());
    }

    /// Project targeted by a logs command: the primary repository's by default, one a block allows
    /// (`allow_log_project`: a deployment project), or
    /// that of a repository whose MRs Prism follows. No other: the front end does not pick an arbitrary project.
    fn project_for(&self, requested: Option<&str>) -> Result<&str> {
        match requested {
            None => self.project(),
            Some(p) if self.project.get().is_some_and(|main| main == p) => self.project(),
            Some(p) => match LOG_PROJECTS.lock().expect("log projects").iter().copied().find(|h| *h == p) {
                Some(other) => Ok(other),
                _ => self
                    .repos()
                    .iter()
                    .map(|(_, project)| project.as_str())
                    // A repository followed for its pipelines only (`[[repos]] pipelines`).
                    .chain(self.infos.get().into_iter().flatten().filter(|i| i.pipelines).filter_map(|i| i.project.as_deref()))
                    .find(|project| *project == p)
                    .with_context(|| tr!("{p} is not a project Prism follows", p = p)),
            },
        }
    }

    fn repos(&self) -> &[(String, String)] {
        self.repos.get().map(Vec::as_slice).unwrap_or_default()
    }

    /// Project of an MR: the primary repository (`None`), or another repository whose MRs Prism follows.
    fn project_of(&self, repo: Option<&str>) -> Result<&str> {
        match repo {
            None => self.project(),
            Some(r) => self.repos().iter().find(|(name, _)| name == r).map(|(_, project)| project.as_str()).with_context(|| tr!("{r} is not a repository whose MRs Prism follows", r = r)),
        }
    }

    /// Project of a pipeline Prism displays: that of its repository's protected branch, or of the MR
    /// carrying it.
    fn pipeline_project(&self, hub: &Hub, id: u64) -> Result<String> {
        let (branch, mr_repo) = {
            let snap = hub.subscribe();
            let snap = snap.borrow();
            let branch = snap.pipelines.iter().find(|p| p.id == id).map(|p| (p.repo.clone(), p.project.clone()));
            let mr_repo = snap.merge_requests.iter().find(|m| m.head_pipeline.as_ref().is_some_and(|p| p.id == id)).and_then(|m| m.repo.clone());
            (branch, mr_repo)
        };
        if let Some((_, Some(project))) = branch {
            return Ok(project);
        }
        self.project_of(mr_repo.as_deref()).map(String::from)
    }

    /// Sets or removes the merge queue label on an open MR that Prism displays, in a repository
    /// with a merge queue (`[[repos]] merge_queue`); the other labels do not move. The CI job then
    /// takes care of the rebase and the auto-merge.
    pub async fn set_merge_queue(&self, hub: &Hub, repo: Option<&str>, iid: u64, on: bool) -> Result<()> {
        let project = self.project_of(repo)?;
        let label = self.merge_queue.get().filter(|l| !l.is_empty()).context(tr("no merge queue label ([gitlab] merge_queue_label)"))?;
        self.queued_mr(hub, repo, iid)?;
        tracing::info!("gitlab: !{iid} {} {label}", if on { "+" } else { "-" });
        let field = if on { "add_labels" } else { "remove_labels" };
        self.put_labels(hub, project, repo, iid, &[format!("{field}={label}")]).await
    }

    /// Sets an open MR's merge priority (`priority`: the label `<prefix><priority>`, `None`: no
    /// priority), in a repository with a merge queue: the MR's other priority labels go, the rest
    /// do not move. The queue's CI job takes the lowest number first.
    pub async fn set_merge_priority(&self, hub: &Hub, repo: Option<&str>, iid: u64, priority: Option<u32>) -> Result<()> {
        let project = self.project_of(repo)?;
        let prefix = self.merge_priority.get().filter(|p| !p.is_empty()).context(tr("no merge priority labels ([gitlab] merge_priority_prefix)"))?;
        let labels = self.queued_mr(hub, repo, iid)?;
        let wanted = priority.map(|n| format!("{prefix}{n}"));
        let gone: Vec<&str> = labels.iter().map(String::as_str).filter(|l| l.starts_with(prefix.as_str()) && Some(*l) != wanted.as_deref()).collect();
        let mut fields = Vec::new();
        if let Some(w) = wanted.as_ref().filter(|w| !labels.contains(w)) {
            fields.push(format!("add_labels={w}"));
        }
        if !gone.is_empty() {
            fields.push(format!("remove_labels={}", gone.join(",")));
        }
        if fields.is_empty() {
            return Ok(());
        }
        tracing::info!("gitlab: !{iid} merge priority {}", wanted.as_deref().unwrap_or("-"));
        self.put_labels(hub, project, repo, iid, &fields).await
    }

    /// An open MR's labels, the whole list: only those that move are sent (`add_labels`,
    /// `remove_labels`), so a label set elsewhere meanwhile stays; a new name creates the label.
    pub async fn set_labels(&self, hub: &Hub, repo: Option<&str>, iid: u64, labels: Vec<String>) -> Result<()> {
        let project = self.project_of(repo)?;
        crate::forge::check_labels(&labels)?;
        let current = hub.subscribe().borrow().merge_requests.iter().find(|m| shown::is(m, repo, iid) && m.state == MrState::Opened).map(|m| m.labels.clone());
        let current = current.with_context(|| format!("{}{}", repo.unwrap_or(""), tr!("!{iid} is not an open merge request Prism displays", iid = iid)))?;
        let (add, remove) = crate::forge::label_changes(&current, &labels);
        let mut fields = Vec::new();
        if !add.is_empty() {
            fields.push(format!("add_labels={}", add.join(",")));
        }
        if !remove.is_empty() {
            fields.push(format!("remove_labels={}", remove.join(",")));
        }
        if fields.is_empty() {
            return Ok(());
        }
        tracing::info!("gitlab: !{iid} labels +{add:?} -{remove:?}");
        self.put_labels(hub, project, repo, iid, &fields).await?;
        if !add.is_empty() {
            review::forget_labels(project);
        }
        Ok(())
    }

    /// Checks that `repo` has a merge queue and that `!iid` is one of its open MRs Prism displays: its labels.
    fn queued_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<Vec<String>> {
        let queued = self.infos.get().into_iter().flatten().any(|i| i.merge_queue && if i.primary { repo.is_none() } else { repo == Some(i.name.as_str()) });
        ensure!(queued, "{}", tr!("{repo} has no merge queue ([[repos]] merge_queue)", repo = repo.unwrap_or(tr("the primary repository"))));
        let labels = hub.subscribe().borrow().merge_requests.iter().find(|m| shown::is(m, repo, iid) && m.state == MrState::Opened).map(|m| m.labels.clone());
        labels.with_context(|| format!("{}{}", repo.unwrap_or(""), tr!("!{iid} is not an open merge request Prism displays", iid = iid)))
    }

    /// Changes an MR's labels (`fields`: `add_labels=…`, `remove_labels=…`) and shows the labels GitLab answers.
    async fn put_labels(&self, hub: &Hub, project: &str, repo: Option<&str>, iid: u64, fields: &[String]) -> Result<()> {
        let mut args: Vec<String> = vec!["api".into(), "--method".into(), "PUT".into(), format!("projects/{}/merge_requests/{iid}", encode(project))];
        for f in fields {
            args.extend(["-f".into(), f.clone()]);
        }
        let mr: RawLabels = glab(&args).await?;
        hub.update(|s| {
            let mut changed = false;
            for m in s.merge_requests.iter_mut().filter(|m| shown::is(m, repo, iid)) {
                changed |= m.labels != mr.labels;
                m.labels.clone_from(&mr.labels);
            }
            changed
        });
        Ok(())
    }

    /// Stops a pipeline. Only those displayed by Prism and still running are accepted: the
    /// window cannot target an arbitrary id.
    pub async fn cancel_pipeline(&self, hub: &Hub, id: u64) -> Result<()> {
        let project = &self.pipeline_project(hub, id)?;
        match shown::pipeline_status(hub, id) {
            None => bail!("{}", tr!("pipeline {id} is not one Prism displays", id = id)),
            Some(status) if !status.can_cancel() => bail!("{}", tr!("pipeline {id} is already {status}", id = id, status = status)),
            Some(_) => {}
        }
        tracing::info!("gitlab: cancel pipeline {id}");
        let raw: RawPipeline = glab(&[
            "api".into(), "--method".into(), "POST".into(),
            format!("projects/{}/pipelines/{id}/cancel", encode(project)),
        ])
        .await?;
        // Without waiting for the next collection: the window sees the stop right away.
        shown::set_pipeline_status(hub, id, PipelineStatus::of(&raw.status), false);
        Ok(())
    }
}

impl Actions {
    /// Retries the failed or canceled jobs of a pipeline (`POST …/pipelines/:id/retry`). Only
    /// those Prism displays, failed or canceled, are accepted.
    pub async fn retry_pipeline(&self, hub: &Hub, id: u64) -> Result<()> {
        let project = &self.pipeline_project(hub, id)?;
        match shown::pipeline_status(hub, id) {
            None => bail!("{}", tr!("pipeline {id} is not one Prism displays", id = id)),
            Some(status) if !status.can_retry() => bail!("{}", tr!("pipeline {id} is {status}: nothing to retry", id = id, status = status)),
            Some(_) => {}
        }
        tracing::info!("gitlab: retry pipeline {id}");
        let raw: RawPipeline = glab(&[
            "api".into(), "--method".into(), "POST".into(),
            format!("projects/{}/pipelines/{id}/retry", encode(project)),
        ])
        .await?;
        shown::set_pipeline_status(hub, id, PipelineStatus::of(&raw.status), true);
        Ok(())
    }

    /// Rebases an MR on GitLab's side (`PUT …/merge_requests/:iid/rebase`). Only an open MR
    /// that Prism sees as behind its target and without conflict: GitLab does not rebase
    /// an MR with conflicts.
    pub async fn rebase_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<()> {
        let project = self.project_of(repo)?;
        let ok = hub
            .subscribe()
            .borrow()
            .merge_requests
            .iter()
            .any(|m| shown::is(m, repo, iid) && m.state == MrState::Opened && !m.conflicts && (m.needs_rebase || m.merge_status == "need_rebase"));
        if !ok {
            bail!("{}", tr!("!{iid} is not an open merge request that needs a rebase", iid = iid));
        }
        tracing::info!("gitlab: rebase !{iid}");
        let _: serde_json::Value = glab(&[
            "api".into(), "--method".into(), "PUT".into(),
            format!("projects/{}/merge_requests/{iid}/rebase", encode(project)),
        ])
        .await?;
        let mark = |in_progress: bool, error: Option<String>| {
            hub.update(|s| {
                let mut changed = false;
                for m in s.merge_requests.iter_mut().filter(|m| shown::is(m, repo, iid)) {
                    m.rebase_in_progress = in_progress;
                    m.merge_error.clone_from(&error);
                    if error.is_none() {
                        m.needs_rebase = false;
                        if m.merge_status == "need_rebase" {
                            m.merge_status = "checking".into();
                        }
                    }
                    changed = true;
                }
                changed
            })
        };
        mark(true, None);
        // GitLab rebases in the background, commit by commit: one may conflict although the
        // branches do not as a whole. Wait for its outcome, and say why it failed.
        let path = format!("projects/{}/merge_requests/{iid}?include_rebase_in_progress=true", encode(project));
        let mut unread = None;
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let state = match glab::<RebaseState>(&["api".into(), path.clone()]).await {
                Ok(state) => {
                    unread = None;
                    state
                }
                Err(e) => {
                    unread = Some(e);
                    continue;
                }
            };
            if state.rebase_in_progress {
                continue;
            }
            let error = state.merge_error.map(|e| e.trim().to_string()).filter(|e| !e.is_empty());
            mark(false, error.clone());
            if let Some(e) = error {
                tracing::warn!("gitlab: rebase !{iid} failed: {e}");
                bail!("{}", tr!("GitLab could not rebase !{iid}: {err}", iid = iid, err = e));
            }
            return Ok(());
        }
        // Still running after a minute: the collection follows it.
        if let Some(e) = unread {
            tracing::warn!("gitlab: rebase !{iid}, its state unreadable: {e:#}");
        }
        Ok(())
    }

    /// Runs a new pipeline for an open MR (`POST …/merge_requests/:iid/pipelines`): a merge
    /// request pipeline, whose rules see the MR as it is now (its labels: a pipeline that only
    /// runs once a label is set). GitLab refuses it when no job would run.
    pub async fn run_mr_pipeline(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<()> {
        let project = self.project_of(repo)?;
        let open = hub.subscribe().borrow().merge_requests.iter().any(|m| shown::is(m, repo, iid) && m.state == MrState::Opened);
        if !open {
            bail!("{}{}", repo.unwrap_or(""), tr!("!{iid} is not an open merge request Prism displays", iid = iid));
        }
        tracing::info!("gitlab: new pipeline for !{iid}");
        let raw: RawNewPipeline = glab(&[
            "api".into(), "--method".into(), "POST".into(),
            format!("projects/{}/merge_requests/{iid}/pipelines", encode(project)),
        ])
        .await?;
        // Without waiting for the next collection: the MR shows its new pipeline, its stages to come.
        let status = PipelineStatus::of(&raw.status);
        hub.update(|s| {
            let mut changed = false;
            for m in s.merge_requests.iter_mut().filter(|m| shown::is(m, repo, iid)) {
                m.pipeline = Some(status);
                m.head_pipeline = Some(Pipeline { id: raw.id, target: format!("!{iid}"), status, url: raw.web_url.clone(), updated_at: Utc::now(), ..Default::default() });
                changed = true;
            }
            changed
        });
        Ok(())
    }

    /// Merges an MR now (`PUT …/merge`), with its own squash and branch-removal settings. Only
    /// an open MR that GitLab sees as mergeable (pipeline, approvals, threads, no draft nor
    /// conflict): GitLab checks again and refuses otherwise.
    pub async fn merge_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<()> {
        let project = self.project_of(repo)?;
        let ok = hub
            .subscribe()
            .borrow()
            .merge_requests
            .iter()
            .any(|m| shown::is(m, repo, iid) && m.state == MrState::Opened && !m.draft && m.merge_status == "mergeable");
        if !ok {
            bail!("{}", tr!("!{iid} is not an open merge request ready to merge", iid = iid));
        }
        tracing::info!("gitlab: merge !{iid}");
        let mr: RawMergeState = glab(&[
            "api".into(), "--method".into(), "PUT".into(),
            format!("projects/{}/merge_requests/{iid}/merge", encode(project)),
        ])
        .await?;
        ensure!(MrState::of(&mr.state) == MrState::Merged, "{}", tr!("GitLab did not merge !{iid} (state: {state})", iid = iid, state = mr.state));
        hub.update(|s| {
            let mut changed = false;
            for m in s.merge_requests.iter_mut().filter(|m| shown::is(m, repo, iid)) {
                m.state = MrState::Merged;
                m.merged_at = Some(Utc::now());
                changed = true;
            }
            changed
        });
        Ok(())
    }

    /// Enables or disables auto-merge on an open MR. When enabled, GitLab merges as soon as everything is green
    /// (pipeline, approvals, threads): right away if that is already the case.
    pub async fn set_auto_merge(&self, hub: &Hub, repo: Option<&str>, iid: u64, enable: bool) -> Result<()> {
        let project = self.project_of(repo)?;
        let ok = hub
            .subscribe()
            .borrow()
            .merge_requests
            .iter()
            .any(|m| shown::is(m, repo, iid) && m.state == MrState::Opened && (if enable { !m.draft && !m.conflicts && !m.auto_merge } else { m.auto_merge }));
        if !ok {
            if enable {
                bail!("{}", tr!("!{iid} is not an open, non-draft merge request without conflicts and auto-merge", iid = iid));
            }
            bail!("{}", tr!("!{iid} has no auto-merge to cancel", iid = iid));
        }
        tracing::info!("gitlab: auto-merge !{iid} {}", if enable { "on" } else { "off" });
        let base = format!("projects/{}/merge_requests/{iid}", encode(project));
        let merged = if enable {
            let mr: RawMergeState = glab(&["api".into(), "--method".into(), "PUT".into(), format!("{base}/merge"), "-f".into(), "auto_merge=true".into()]).await?;
            MrState::of(&mr.state) == MrState::Merged
        } else {
            // The answer is not the MR on every GitLab version: not read. A canceled or failed pipeline may have
            // dropped the auto-merge already (GitLab then refuses to cancel it): the MR read again says so.
            let cancel = format!("{base}/cancel_merge_when_pipeline_succeeds");
            if let Err(e) = Cli::Glab.run(&["api", "--method", "POST", cancel.as_str()]).await {
                let now: Result<RawAutoMerge> = glab(&["api".into(), "--method".into(), "GET".into(), base.clone()]).await;
                let gone = now.is_ok_and(|now| !now.merge_when_pipeline_succeeds || MrState::of(&now.state) != MrState::Opened);
                if !gone {
                    return Err(e);
                }
                tracing::info!("gitlab: !{iid} had no auto-merge left on GitLab");
            }
            false
        };
        // Already green: GitLab merged right away; the next collection files it among the merged ones.
        hub.update(|s| {
            let mut changed = false;
            for m in s.merge_requests.iter_mut().filter(|m| shown::is(m, repo, iid)) {
                m.auto_merge = enable && !merged;
                if merged {
                    m.state = MrState::Merged;
                    m.merged_at = Some(Utc::now());
                }
                changed = true;
            }
            changed
        });
        Ok(())
    }

    /// A project pipeline's jobs, for the logs panel.
    /// The primary repository's project.
    fn project(&self) -> Result<&str> {
        let project = self.project.get().map(String::as_str).context(tr("GitLab is not configured"))?;
        ensure!(!project.is_empty(), "{}", tr("no repository for bare MR references ([forge] bare_mr_repo)"));
        Ok(project)
    }

    /// An MR's description, discussions and approvals, read when its panel opens.
    pub async fn mr_detail(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<crate::forge::MrDetail> {
        review::detail(self.project_of(repo)?, hub, repo, iid).await
    }

    /// An MR's diff, read when the panel displays it.
    pub async fn mr_diff(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<crate::forge::MrDiff> {
        review::diff(self.project_of(repo)?, hub, repo, iid).await
    }

    /// An MR's commits, newest first, read when its Commits tab is shown.
    pub async fn mr_commits(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<Vec<crate::forge::MrCommit>> {
        review::commits(self.project_of(repo)?, hub, repo, iid).await
    }

    /// A file of an MR, whole, on its source branch (Markdown preview of the diff).
    pub async fn mr_file(&self, hub: &Hub, repo: Option<&str>, iid: u64, path: &str) -> Result<String> {
        review::file(self.project_of(repo)?, hub, repo, iid, path).await
    }

    /// An image of an MR's diff, as a `data:` URL: its new version, or (`old`) the target branch's.
    pub async fn mr_image(&self, hub: &Hub, repo: Option<&str>, iid: u64, path: &str, old: bool) -> Result<String> {
        review::file_image(self.project_of(repo)?, hub, repo, iid, path, old).await
    }

    pub async fn approve_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64, on: bool) -> Result<()> {
        review::approve(self.project_of(repo)?, hub, repo, iid, on).await
    }

    pub async fn request_changes(&self, hub: &Hub, repo: Option<&str>, iid: u64, on: bool, body: &str) -> Result<()> {
        review::request_changes(self.project_of(repo)?, hub, repo, iid, on, body).await
    }

    pub async fn comment_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64, discussion: Option<&str>, body: &str) -> Result<()> {
        review::comment(self.project_of(repo)?, hub, repo, iid, discussion, body).await
    }

    pub async fn comment_line(&self, hub: &Hub, repo: Option<&str>, iid: u64, line: crate::forge::LineRef, body: &str) -> Result<()> {
        review::comment_line(self.project_of(repo)?, hub, repo, iid, line, body).await
    }

    pub async fn resolve_thread(&self, hub: &Hub, repo: Option<&str>, iid: u64, discussion: &str, resolved: bool) -> Result<()> {
        review::resolve(self.project_of(repo)?, hub, repo, iid, discussion, resolved).await
    }

    pub async fn set_draft(&self, hub: &Hub, repo: Option<&str>, iid: u64, draft: bool) -> Result<()> {
        review::set_draft(self.project_of(repo)?, hub, repo, iid, draft).await
    }

    /// Active project members (bots excluded): the choices for assignees and reviewers.
    pub async fn members(&self, repo: Option<&str>) -> Result<Vec<crate::forge::Member>> {
        review::members(self.project_of(repo)?).await
    }

    /// The project's labels (its groups' included): the choices for an MR's labels.
    pub async fn labels(&self, repo: Option<&str>) -> Result<Vec<crate::forge::ForgeLabel>> {
        review::labels(self.project_of(repo)?).await
    }

    /// Assignees (`reviewers = false`) or reviewers of an open MR: the whole list, empty to
    /// remove everyone.
    pub async fn set_people(&self, hub: &Hub, repo: Option<&str>, iid: u64, reviewers: bool, usernames: Vec<String>) -> Result<()> {
        review::set_people(self.project_of(repo)?, hub, repo, iid, reviewers, usernames).await
    }

    /// The custom emoji of the project's groups (`project`: as `upload`'s).
    pub async fn custom_emoji(&self, project: Option<&str>) -> Result<Vec<crate::forge::CustomEmoji>> {
        review::custom_emoji(self.project_for(project)?).await
    }

    /// Image uploaded to the project (`/uploads/<secret>/<file>`), as a `data:` URL.
    /// `project`: that of the text (the primary repository by default, or a repository whose MRs Prism follows).
    pub async fn upload(&self, project: Option<&str>, path: &str) -> Result<String> {
        review::upload(self.project_for(project)?, path).await
    }

    pub async fn pipeline_jobs(&self, project: Option<&str>, pipeline: u64) -> Result<Vec<crate::forge::JobInfo>> {
        logs::pipeline_jobs(self.project_for(project)?, pipeline).await
    }

    /// Retries a failed or canceled project job; returns the new job.
    pub async fn retry_job(&self, project: Option<&str>, job: u64) -> Result<crate::forge::JobInfo> {
        logs::retry_job(self.project_for(project)?, job).await
    }

    /// Log of a project job (only the end if it is very long).
    pub async fn job_log(&self, project: Option<&str>, job: u64) -> Result<crate::forge::JobLog> {
        logs::job_log(self.project_for(project)?, job).await
    }

    /// Runs "Renovate (Daily)" now.
    pub async fn play_renovate(&self, hub: &Hub) -> Result<()> {
        renovate::play(&renovate::project()?, hub).await
    }

    /// Ticks an item of Renovate's Dependency Dashboard.
    pub async fn check_renovate(&self, hub: &Hub, marker: &str) -> Result<()> {
        renovate::check(&renovate::project()?, hub, marker).await
    }

    /// The project of the pipeline carrying a job Prism displays in `list`: its followed branch's
    /// repository, or its MR's.
    fn job_project(&self, hub: &Hub, id: u64, list: crate::forge::JobList) -> Option<String> {
        let snap = hub.subscribe();
        let snap = snap.borrow();
        match snap.pipelines.iter().find(|p| list.in_pipeline(p, id)) {
            Some(p) => p.project.clone().or_else(|| self.project().ok().map(String::from)),
            None => snap.merge_requests.iter().find(|m| m.head_pipeline.as_ref().is_some_and(|p| list.in_pipeline(p, id))).and_then(|m| self.project_of(m.repo.as_deref()).ok().map(String::from)),
        }
    }

    /// Runs again a manual job already run (a tag set again after a new base image). Only those
    /// Prism displays as such are accepted.
    pub async fn replay_job(&self, hub: &Hub, id: u64) -> Result<()> {
        let Some(project) = self.job_project(hub, id, crate::forge::JobList::Played) else { bail!("{}", tr!("job {id} is not a job run already Prism displays", id = id)) };
        logs::retry_job(&project, id).await?;
        crate::forge::forget_job(hub, id, crate::forge::JobList::Played);
        Ok(())
    }

    /// Plays a manual job. Only those Prism displays as playable are accepted.
    pub async fn play_job(&self, hub: &Hub, id: u64) -> Result<()> {
        let Some(project) = self.job_project(hub, id, crate::forge::JobList::Manual) else { bail!("{}", tr!("job {id} is not a manual job Prism displays", id = id)) };
        tracing::info!("gitlab: play job {id}");
        let _: serde_json::Value = glab(&[
            "api".into(), "--method".into(), "POST".into(),
            format!("projects/{}/jobs/{id}/play", encode(&project)),
        ])
        .await?;
        crate::forge::forget_job(hub, id, crate::forge::JobList::Manual);
        Ok(())
    }
}

/// GitLab's response to a label change: the MR, of which only the labels are used.
#[derive(Deserialize)]
struct RawLabels {
    #[serde(default)]
    labels: Vec<String>,
}

/// GitLab's response to an auto-merge change: the MR, of which only the state is used.
#[derive(Deserialize)]
struct RawMergeState {
    state: String,
}

/// An MR's auto-merge, as `GET …/merge_requests/:iid` reports it.
#[derive(Deserialize)]
struct RawAutoMerge {
    state: String,
    #[serde(default)]
    merge_when_pipeline_succeeds: bool,
}

#[derive(Deserialize)]
struct RawPipeline {
    status: String,
}

#[derive(Deserialize)]
struct RawNewPipeline {
    id: u64,
    status: String,
    web_url: String,
}

/// An MR's rebase, as `GET …/merge_requests/:iid?include_rebase_in_progress=true` reports it.
#[derive(Deserialize)]
struct RebaseState {
    #[serde(default)]
    rebase_in_progress: bool,
    merge_error: Option<String>,
}
