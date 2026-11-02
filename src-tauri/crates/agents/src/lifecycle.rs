//! An agent's life: launched (or prepared, then started) in its folder or a new worktree, resumed or restarted, stopped, closed with its worktree.

use super::*;

/// A launch as the launcher has it so far, for its command line (`Agents::command_line`).
#[derive(Debug, Deserialize)]
pub struct LaunchPreview {
    pub provider: Provider,
    pub title: String,
    #[serde(default)]
    pub permission_mode: Option<String>,
    /// Its own, picked in the launcher; absent: the CLI's (`[agents.<cli>]`).
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    /// The new worktree's name, when it gets one.
    #[serde(default)]
    pub worktree: Option<String>,
    /// The other repositories it works in too.
    #[serde(default)]
    pub also: Vec<String>,
    #[serde(default)]
    pub prompt: Option<String>,
}

impl Agents {
    /// The command line `spawn` runs for `p`, as the launcher shows it: built the same way, the
    /// session's id and Prism's wiring shown as `…` (Codex's hooks left out), the other
    /// repositories' worktrees as `…/<repo>.worktrees/<name>`, a word the shell would split quoted.
    pub fn command_line(&self, p: LaunchPreview) -> Result<String> {
        let prompt = p.prompt.as_deref().map(str::trim).filter(|t| !t.is_empty());
        let worktree = p.worktree.as_deref().filter(|_| p.provider.delegates_worktree());
        let argv = p.provider.launch("…", &p.title, Path::new("…"), p.permission_mode.as_deref(), worktree, prompt)?;
        let spec = state::LaunchSpec { model: p.model, effort: p.effort, ..Default::default() };
        let name = p.worktree.as_deref().unwrap_or("…");
        let dirs: Vec<PathBuf> = p.also.iter().map(|r| PathBuf::from(format!("…/{r}.worktrees/{name}"))).collect();
        let argv = p.provider.with_dirs(p.provider.with_cli(argv, &self.cli(p.provider, Some(&spec))), &dirs);
        let plain = |w: &str| !w.is_empty() && w.chars().all(|c| c.is_alphanumeric() || "-_./:=@%+,…".contains(c));
        Ok(argv.iter().map(|w| if plain(w) { w.clone() } else { prism_base::platform::quote(w) }).collect::<Vec<_>>().join(" "))
    }

    /// Launches the agent CLI picked for it, failing that its repository's (`[[repos]] agent`), failing
    /// that `[agents] provider`: in a folder, or in a new worktree, which Claude Code requests and Prism's hook
    /// creates, or Prism creates first for the others. Returns the agent's identifier.
    /// Its "Then" steps, if any, run once it has done its work (`chains`).
    pub async fn spawn(&self, launch: Launch) -> Result<String> {
        let then = launch.then.clone();
        let id = self.spawn_as(uuid::Uuid::new_v4().to_string(), launch, None).await?;
        self.start_chain(&id, then);
        Ok(id)
    }

    /// Declares an agent without starting it: its launch is kept, checked (its folder, its CLI's
    /// mode), and it shows in the list as prepared; nothing runs and no worktree is made until
    /// `start`. Returns its identifier.
    pub async fn prepare(&self, launch: Launch) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let spec = launch.spec();
        let (place, provider) = self.place_and_provider(&launch).await?;
        if let Some(mode) = launch.permission_mode.as_deref().filter(|m| *m != "default") {
            provider.permission_mode(mode)?;
        }
        if let Some(m) = launch.model.as_deref().map(str::trim).filter(|m| !m.is_empty() && !prism_core::agents::models::valid(m)) {
            bail!(tr!("{m} is not a model name", m = m));
        }
        if let Some(e) = launch.effort.as_deref().map(str::trim).filter(|e| !e.is_empty() && !prism_core::agents::models::valid_effort(e)) {
            bail!(tr!("{e} is not an effort level", e = e));
        }
        if launch.worktree.is_none() && !place.is_dir() {
            bail!(tr!("{path} is not a directory", path = place.display()));
        }
        let record = Record {
            id: id.clone(),
            provider,
            title: if spec.title.is_empty() { "Agent".into() } else { spec.title.clone() },
            cwd: place,
            prompt: spec.prompt.clone(),
            worktree: None,
            category: spec.category.clone().or_else(|| self.kind_of(spec.prompt.as_deref())),
            tags: spec.tags.clone(),
            ticket: None,
            mr: None,
            launch: Some(spec),
            prepared: true,
            extra: Vec::new(),
            legs: Vec::new(),
            resumed: None,
            created_at: Utc::now(),
        };
        self.update_registry(|reg| reg.records.push(record))?;
        Ok(id)
    }

    /// Starts a prepared agent, under the same identifier: its launch as kept, with the title,
    /// kind, tags and links given to it since.
    pub async fn start(&self, id: &str) -> Result<()> {
        let record = self.record(id)?;
        if !record.prepared {
            bail!(tr("this agent has already started"));
        }
        let mut launch = Launch::from(record.launch.clone().unwrap_or_default());
        (launch.title, launch.category, launch.tags) = (record.title.clone(), record.category.clone(), record.tags.clone());
        if launch.worktree.is_none() && launch.cwd.is_none() {
            launch.cwd = Some(record.cwd.clone());
        }
        let then = launch.then.clone();
        self.spawn_as(id.to_string(), launch, Some(record)).await?;
        self.start_chain(id, then);
        Ok(())
    }

    /// Where the agent starts (its folder, or the repository of its new worktree) and the CLI it runs there.
    pub(crate) async fn place_and_provider(&self, launch: &Launch) -> Result<(PathBuf, Provider)> {
        let place = match &launch.worktree {
            Some(wt) => self.worktree_repo(wt.repo.as_deref())?.path.clone(),
            None => launch.cwd.clone().context(tr("no folder to start the agent in"))?,
        };
        let provider = match launch.provider {
            Some(p) => p,
            None => self.provider_of(place.clone()).await,
        };
        Ok((place, provider))
    }

    /// `spawn`, under a given identifier; `prepared`: the record it replaces, whose links stay.
    pub(crate) async fn spawn_as(&self, id: String, launch: Launch, prepared: Option<Record>) -> Result<String> {
        let spec = launch.spec();
        let (_, provider) = self.place_and_provider(&launch).await?;
        let title = if launch.title.trim().is_empty() { "Agent".to_string() } else { launch.title.trim().to_string() };
        let prompt = launch.prompt.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        // Refused before a worktree exists for nothing.
        if let Some(mode) = launch.permission_mode.as_deref().filter(|m| *m != "default") {
            provider.permission_mode(mode)?;
        }
        if let Some(m) = launch.model.as_deref().map(str::trim).filter(|m| !m.is_empty() && !prism_core::agents::models::valid(m)) {
            bail!(tr!("{m} is not a model name", m = m));
        }
        if let Some(e) = launch.effort.as_deref().map(str::trim).filter(|e| !e.is_empty() && !prism_core::agents::models::valid_effort(e)) {
            bail!(tr!("{e} is not an effort level", e = e));
        }
        // The other repositories it works in too, each given a worktree of the same name.
        let also: Vec<String> = launch.worktree.as_ref().map(|w| w.also.iter().map(|r| r.trim().to_string()).filter(|r| !r.is_empty() && Some(r.as_str()) != w.repo.as_deref()).collect()).unwrap_or_default();
        if !also.is_empty() && !provider.takes_dirs() {
            bail!(tr!("{cli} works in one folder only: launch it in a single repository", cli = provider.program()));
        }
        let mut extra: Vec<PathBuf> = Vec::new();
        let (cwd, worktree, requested) = match launch.worktree {
            Some(wt) => {
                let (repo, name, intent) = self.prepare_worktree(&title, wt).await?;
                if let Err(e) = self.make_extra(&title, &name, &also, &mut extra).await {
                    self.drop_extra(&extra).await;
                    return Err(e);
                }
                let dir = intent.dir.clone();
                if provider.delegates_worktree() {
                    worktree::write_intent(&self.inner.work, &name, &intent)?;
                    (repo, Some(dir), Some(name))
                } else {
                    let created = tokio::task::spawn_blocking(move || worktree::create(&repo, &intent)).await??;
                    (created.clone(), Some(created), None)
                }
            }
            None => (launch.cwd.context(tr("no folder to start the agent in"))?, None, None),
        };
        let argv = provider.launch(&id, &title, &self.wiring(provider), launch.permission_mode.as_deref(), requested.as_deref(), prompt.as_deref())?;
        let argv = provider.with_dirs(provider.with_cli(argv, &self.cli(provider, Some(&spec))), &extra);
        if !cwd.is_dir() {
            self.drop_extra(&extra).await;
            bail!(tr!("{path} is not a directory", path = cwd.display()));
        }
        if let Err(e) = self.inner.tmux.new_session(&id, &cwd, &self.env(&id, provider, launch.permission_mode.as_deref()), &argv).await {
            self.drop_extra(&extra).await;
            return Err(e);
        }
        let category = launch.category.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).or_else(|| self.kind_of(prompt.as_deref()));
        let (ticket, mr) = prepared.map(|r| (r.ticket, r.mr)).unwrap_or_default();
        let record = Record { id: id.clone(), provider, title, cwd, prompt, worktree, category, tags: clean_tags(launch.tags), ticket, mr, launch: Some(spec), prepared: false, extra, legs: Vec::new(), resumed: None, created_at: Utc::now() };
        self.update_registry(|reg| {
            reg.records.retain(|r| r.id != id);
            reg.records.push(record)
        })?;
        Ok(id)
    }

    /// The worktrees of the other repositories an agent works in (`also`): the same name as its own
    /// (the next free one where taken), each on a new branch from its repository's base, made now
    /// whatever the CLI. Those made so far are in `made`, for the caller to drop on a failure.
    async fn make_extra(&self, title: &str, name: &str, also: &[String], made: &mut Vec<PathBuf>) -> Result<()> {
        for repo in also {
            let wt = NewWorktree { repo: Some(repo.clone()), name: Some(name.to_string()), branch: None, base: None, also: Vec::new() };
            let (path, _, intent) = self.prepare_worktree(title, wt).await.with_context(|| repo.clone())?;
            let created = tokio::task::spawn_blocking(move || worktree::create(&path, &intent)).await??;
            made.push(created);
        }
        Ok(())
    }

    /// Worktrees made for a launch that failed: removed, when nothing was done in them.
    async fn drop_extra(&self, made: &[PathBuf]) {
        for path in made {
            let (shown, path) = (path.display().to_string(), path.clone());
            match tokio::task::spawn_blocking(move || worktree::remove(&path, None, false)).await {
                Ok(Ok(Ok(()))) => {}
                other => tracing::warn!("agents: {shown} kept after a failed launch: {other:?}"),
            }
        }
    }

    /// Picks a free name and describes the worktree: new branch `<prefix><name>` from the release
    /// base, or the requested branch. Returns the repository, the name and what to create.
    pub(crate) async fn prepare_worktree(&self, title: &str, wt: NewWorktree) -> Result<(PathBuf, String, worktree::Intent)> {
        let (repo, plan) = self.plan_worktree(title, &wt).await?;
        if let Some(e) = plan.error {
            bail!("{e}");
        }
        let intent = match wt.branch {
            Some(branch) => worktree::Intent { dir: plan.dir, branch: worktree::branch_name(&branch)?, base: None },
            None => {
                let base = plan.base.context(tr("no branch to start the worktree from"))?;
                worktree::Intent { dir: plan.dir, branch: plan.branch, base: Some(format!("origin/{}", worktree::branch_name(&base)?)) }
            }
        };
        Ok((repo, plan.name, intent))
    }

    /// The worktree a launch would create (`worktree::plan`), and its repository: the launcher
    /// shows it, and says before launching what is taken or refused.
    pub async fn plan_worktree(&self, title: &str, wt: &NewWorktree) -> Result<(PathBuf, worktree::Plan)> {
        let cfg = self.worktree_repo(wt.repo.as_deref())?;
        let wanted = wt
            .name
            .as_deref()
            .map(worktree::slug)
            .filter(|n| !n.is_empty())
            .or_else(|| wt.branch.as_deref().map(|b| worktree::slug(b.rsplit('/').next().unwrap_or(b))).filter(|n| !n.is_empty()))
            .or_else(|| Some(worktree::slug(title)).filter(|n| !n.is_empty()))
            .unwrap_or_else(|| "agent".into());
        let (dir, prefix) = {
            let s = self.inner.settings.lock().unwrap();
            (s.worktrees_dir.clone(), s.branch_prefix.clone())
        };
        let root = dir.unwrap_or_else(|| worktree::worktrees_dir(&cfg.path));
        let repo = cfg.path.clone();
        let branches = {
            let repo = repo.clone();
            tokio::task::spawn_blocking(move || worktree::branches(&repo)).await??
        };
        let work = self.inner.work.clone();
        let mut plan = worktree::plan(&wanted, &root, &prefix, wt.branch.as_deref(), &branches, |n| worktree::intent_path(&work, n).exists());
        if wt.branch.is_none() {
            // The base asked for, failing that the repository's; offered with the branches on origin.
            let default = crate::collectors::worktrees::resolve_base(&cfg).await?;
            let main = crate::collectors::worktrees::main_branch(&cfg).await;
            let asked = wt.base.as_deref().map(str::trim).map(|b| b.strip_prefix("origin/").unwrap_or(b)).filter(|b| !b.is_empty());
            let bases = {
                let (repo, first, release) = (repo.clone(), [default.clone(), main], cfg.release_prefix.clone());
                tokio::task::spawn_blocking(move || worktree::origin_branches(&repo, &first.iter().map(String::as_str).collect::<Vec<_>>(), &release)).await?
            };
            if let Some(b) = asked
                && plan.error.is_none()
                && !bases.iter().any(|x| x == b)
            {
                plan.error = Some(format!("no branch {b} on origin to start from (as last fetched)"));
            }
            plan.base = Some(asked.map_or(default, String::from));
            plan.bases = bases;
        }
        Ok((repo, plan))
    }

    /// The repository of a new worktree: the one named, absent the first where agents create theirs.
    pub(crate) fn worktree_repo(&self, name: Option<&str>) -> Result<WorktreeRepo> {
        let repos = self.inner.worktrees.lock().unwrap();
        match name {
            Some(name) => repos.iter().find(|r| r.name == name).cloned().with_context(|| tr!("{name} is not a repository where agents create worktrees ([[repos]] worktrees, path)", name = name)),
            None => repos.first().cloned().context(tr("no repository where agents create worktrees ([[repos]] path, and primary = true or worktrees = true)")),
        }
    }

    /// The CLI of an agent launched in `dir`: its repository's (the clone, or one of its worktrees,
    /// found by git), failing that `[agents] provider`.
    pub(crate) async fn provider_of(&self, dir: PathBuf) -> Provider {
        let default = self.inner.settings.lock().unwrap().provider;
        let repos = self.inner.repo_agents.lock().unwrap().clone();
        if repos.is_empty() {
            return default;
        }
        tokio::task::spawn_blocking(move || worktree::repo_of(&dir, &repos)).await.ok().flatten().unwrap_or(default)
    }

    /// Relaunches a stopped agent (its CLI exited, or tmux session lost on Mac restart)
    /// on its conversation, in its folder. Claude still running: nothing to do.
    pub async fn resume(&self, id: &str) -> Result<()> {
        self.relaunch(id, false).await
    }

    /// Restarts the agent's CLI on its conversation, even if it is running (the current turn is
    /// interrupted): it re-reads its settings, hooks, MCPs and instructions, and picks up where it left off.
    pub async fn restart(&self, id: &str) -> Result<()> {
        self.relaunch(id, true).await
    }

    /// The CLI's resume in the agent's session: its process is replaced in place when
    /// the session exists (the terminal stays attached), a new session otherwise.
    pub(crate) async fn relaunch(&self, id: &str, force: bool) -> Result<()> {
        let record = self.record(id)?;
        if record.prepared {
            bail!(tr("this agent has not started yet: Start it"));
        }
        let pane = self.inner.tmux.panes().await?.get(id).copied();
        if pane == Some(Pane::Alive) && !force {
            return Ok(());
        }
        let provider = record.provider;
        let (session, transcript) = self.inner.folds.lock().unwrap().get(id).map(|f| (f.session.clone(), f.transcript.clone())).unwrap_or_default();
        // In its worktree if it still exists: the CLI does not always bring it back there by itself.
        let mut cwd = self.dir(id)?;
        // Claude Code writes a conversation at its first message: an agent that quit before (its
        // worktree not created, a crash at startup) has none to resume, and `--resume` would fail.
        // It starts again instead, under the same id, with its prompt and, when its worktree is
        // still missing, asking for it again.
        // In the permission mode it was launched in (plan stays plan).
        let mode = record.launch.as_ref().and_then(|l| l.permission_mode.as_deref());
        let started = session.is_some() || transcript.as_deref().is_some_and(|t| Path::new(t).is_file()) || claude_conversation_exists(&record.id);
        let argv = if provider == Provider::Claude && !started {
            let missing = record.worktree.as_ref().filter(|w| !w.is_dir()).and_then(|w| w.file_name()).map(|n| n.to_string_lossy().into_owned());
            if missing.is_some() {
                cwd = record.cwd.clone();
            }
            tracing::info!("agents: {id} has no conversation yet: started again{}", if missing.is_some() { ", its worktree asked for again" } else { "" });
            provider.launch(&record.id, &record.title, &self.wiring(provider), mode, missing.as_deref(), record.prompt.as_deref())?
        } else {
            provider.resume(&record.id, session.as_deref(), &self.wiring(provider), mode)
        };
        let extra: Vec<PathBuf> = record.extra.iter().filter(|p| p.is_dir()).cloned().collect();
        let argv = provider.with_dirs(provider.with_cli(argv, &self.cli(provider, record.launch.as_ref())), &extra);
        let env = self.env(id, provider, mode);
        match pane {
            Some(_) => self.inner.tmux.respawn(id, &cwd, &env, &argv).await?,
            None => self.inner.tmux.new_session(id, &cwd, &env, &argv).await?,
        }
        self.inner.changed.notify_one();
        Ok(())
    }

    /// Stops the agent's CLI as from the keyboard: two Ctrl-C in quick succession. The session stays, on its last screen.
    pub async fn stop(&self, id: &str) -> Result<()> {
        self.inner.tmux.send_keys(id, &["C-c"]).await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        self.inner.tmux.send_keys(id, &["C-c"]).await?;
        self.inner.changed.notify_one();
        Ok(())
    }

    /// Grants once the permission the agent is asking for: key 1 of the CLI's question
    /// ("1. Yes"), Enter on OpenCode's ("Allow once"), `y` on Codex's ("Yes, proceed"). With no pending request, nothing is sent.
    pub async fn allow(&self, id: &str) -> Result<()> {
        let waiting = self.inner.folds.lock().unwrap().get(id).is_some_and(|f| f.status == SessionStatus::WaitingTool);
        if !waiting {
            bail!(tr("this agent is not waiting for a permission"));
        }
        let provider = self.provider(id)?;
        self.inner.tmux.send_keys(id, provider.allow_keys()).await?;
        self.permission_answered(id);
        Ok(())
    }

    /// The agent's permission, just answered from Prism: said in its hook log (`PermissionReplied`),
    /// so that it is no longer shown waiting while the tool runs.
    pub(crate) fn permission_answered(&self, id: &str) {
        let line = hook::Line { t: Utc::now().timestamp_millis(), event: "PermissionReplied".into(), ..Default::default() };
        if let Err(e) = hook::append(&self.inner.events, id, &line) {
            tracing::warn!("agents: {id}: {e}");
        }
        self.inner.changed.notify_one();
    }

    /// Closes the agent in one go. Still running, its CLI is first stopped as from the keyboard
    /// (two Ctrl-C) and given a few seconds to exit cleanly, then its tmux session is killed and it
    /// leaves the list, with its hook log. Its worktree goes too, if there is nothing to lose and no
    /// other agent or Claude Desktop session works in it; otherwise it stays, and the reason is
    /// returned. Its conversation stays in the CLI's transcripts.
    pub async fn close(&self, id: &str) -> Result<Option<String>> {
        let record = self.record(id)?;
        let worktree = self.inner.folds.lock().unwrap().get(id).and_then(|f| effective_worktree(&record, f));
        if self.inner.tmux.panes().await?.get(id) == Some(&Pane::Alive) {
            self.stop(id).await?;
            for _ in 0..25 {
                if self.inner.tmux.panes().await?.get(id) != Some(&Pane::Alive) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        self.detach(id);
        self.inner.tmux.kill(id).await?;
        self.close_shells(id).await;
        // The last agent of its repository, remembered with its hook log (its CLI's session): the
        // project says where it was left, and Reopen brings it back.
        match self.left_of(&record).await {
            Some(left) => self.remember_left(left),
            None => {
                let _ = std::fs::remove_file(self.inner.events.join(format!("{id}.jsonl")));
            }
        }
        // An intent left pending (Claude did not start) must not be used by another agent.
        let name = record.worktree.as_ref().and_then(|w| w.file_name()).map(|n| n.to_string_lossy().into_owned());
        if let Some(name) = name {
            let _ = std::fs::remove_file(worktree::intent_path(&self.inner.work, &name));
        }
        // Its last figures written now: it leaves the list, its cost stays in the book.
        self.flush_costs();
        self.update_registry(|reg| reg.records.retain(|r| r.id != id))?;
        let mut kept = Vec::new();
        for path in worktree.into_iter().chain(record.extra.iter().filter(|p| p.is_dir()).cloned()) {
            if let Err(e) = self.remove_closed_worktree(id, path.clone()).await {
                kept.push(if record.extra.contains(&path) { format!("{}: {e:#}", path.display()) } else { format!("{e:#}") });
            }
        }
        Ok((!kept.is_empty()).then(|| format!("worktree kept: {}", kept.join("; "))))
    }

    /// The worktree of an agent just closed, removed unless someone else works in it or it holds
    /// something to lose (a merged MR's commits count as delivered).
    pub(crate) async fn remove_closed_worktree(&self, id: &str, path: PathBuf) -> Result<()> {
        let p = path.display().to_string();
        let merged_at = {
            let snap = self.inner.hub.subscribe();
            let snap = snap.borrow();
            let inside = |dir: &str| dir == p || dir.starts_with(&format!("{p}/"));
            if let Some(a) = snap.agents.iter().find(|a| a.id != id && (a.worktree.as_deref() == Some(&p) || a.adopted.contains(&p) || a.also.contains(&p) || inside(&a.cwd))) {
                bail!(tr!("the agent “{title}” works in it", title = a.title));
            }
            if let Some(c) = snap.sessions.iter().find(|c| c.origin == SessionOrigin::Desktop && c.worktree.as_deref().is_some_and(inside)) {
                bail!(tr!("the Claude Desktop session “{title}” uses it", title = c.title));
            }
            snap.worktrees.iter().find(|w| w.path == p).and_then(|w| snap.merged_at(w))
        };
        self.close_folder_shells(&p).await;
        match tokio::task::spawn_blocking(move || worktree::remove(&path, merged_at, false)).await?? {
            Ok(()) => {
                self.inner.hub.update(|s| {
                    s.worktrees.retain(|w| w.path != p);
                    true
                });
                Ok(())
            }
            Err(reason) => bail!(reason),
        }
    }

    /// Removes a stopped agent's worktree, if there is nothing to lose (changes, unpushed
    /// commits); its local branch goes with it.
    pub async fn remove_worktree(&self, id: &str) -> Result<()> {
        let record = self.record(id)?;
        if self.inner.tmux.panes().await?.get(id) == Some(&Pane::Alive) {
            bail!(tr("stop the agent first"));
        }
        let path = self.inner.folds.lock().unwrap().get(id).and_then(|f| effective_worktree(&record, f)).context(tr("this agent has no worktree"))?;
        // Its shells would be left in a folder that no longer exists.
        self.close_shells(id).await;
        // Those of the other repositories it worked in, first: kept, said along with the rest.
        let mut kept = Vec::new();
        for extra in record.extra.into_iter().filter(|p| p.is_dir()) {
            let shown = extra.display().to_string();
            if let Err(reason) = tokio::task::spawn_blocking(move || worktree::remove(&extra, None, false)).await?? {
                kept.push(format!("{shown}: {reason}"));
            }
        }
        if !kept.is_empty() {
            bail!(tr!("worktree kept: {reason}", reason = kept.join("; ")));
        }
        match tokio::task::spawn_blocking(move || worktree::remove(&path, None, false)).await?? {
            Ok(()) => {
                let _refreshing = self.inner.refreshing.lock().await;
                if let Some(f) = self.inner.folds.lock().unwrap().get_mut(id) {
                    (f.worktree, f.worktree_note) = (None, None);
                }
                self.update_registry(|reg| {
                    if let Some(r) = reg.records.iter_mut().find(|r| r.id == id) {
                        r.worktree = None;
                    }
                })
            }
            Err(reason) => bail!(tr!("worktree kept: {reason}", reason = reason)),
        }
    }

    pub(crate) async fn close_shells(&self, id: &str) {
        for (n, _) in self.inner.tmux.shells(id).await.unwrap_or_default() {
            if let Err(e) = self.close_shell(id, n).await {
                tracing::warn!("agents: closing shell {n} of {id}: {e:#}");
            }
        }
    }

    /// What makes the CLI report to Prism: hooks settings, mod, plugin, or Codex's hooks — written
    /// again for each launch and resume, whatever another program left in its file meanwhile.
    pub(crate) fn wiring(&self, provider: Provider) -> PathBuf {
        if let Err(e) = write_wiring(&self.inner.work, provider) {
            tracing::warn!("agents: writing {}'s wiring: {e:#}", provider.wiring_file());
        }
        self.inner.work.join(provider.wiring_file())
    }

    /// The agent's environment: where Command Code's mod and OpenCode's plugin write its log
    /// (Claude Code's hooks are told by their command line), and OpenCode's plugin itself.
    pub(crate) fn env(&self, id: &str, provider: Provider, mode: Option<&str>) -> Vec<String> {
        let mut env = vec![format!("PRISM_AGENT={id}"), format!("PRISM_EVENTS={}", self.inner.events.display())];
        env.extend(provider.env(&self.wiring(provider), mode));
        env
    }
}
