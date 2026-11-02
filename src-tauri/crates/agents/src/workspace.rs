//! An agent's folder: its files, diff, previews, scratchpad pages, attachments, and git (commit, push, sync, MR).

use super::*;

impl Agents {
    /// The folder the agent works in: its worktree if it still exists, otherwise its launch folder.
    pub fn dir(&self, id: &str) -> Result<PathBuf> {
        let record = self.record(id)?;
        Ok(self.inner.folds.lock().unwrap().get(id).and_then(|f| effective_worktree(&record, f)).unwrap_or(record.cwd))
    }

    /// The files of the agent's folder the Web view can show (`ignored`: those `.gitignore` leaves out too).
    pub async fn pages(&self, id: &str, ignored: bool) -> Result<Vec<files::Page>> {
        files::pages(&self.dir(id)?, ignored).await
    }

    /// The files of the agent's folder, with their git state (`ignored`: those `.gitignore` leaves out too).
    pub async fn files(&self, id: &str, ignored: bool) -> Result<files::Listing> {
        files::list(&self.dir(id)?, ignored).await
    }

    /// Files of the agent's folder written to `dest` (the Mac's save dialog): one as it is,
    /// several or a folder as a zip, as it goes. The name to save it under.
    pub async fn export(&self, id: &str, paths: Vec<String>, name: String, max: u64, dest: PathBuf) -> Result<String> {
        let dir = self.dir(id)?;
        tokio::task::spawn_blocking(move || files::export(&dir, &paths, &name, max, true, &dest, |p| std::fs::File::create(p))).await?
    }

    /// The same made ready for a phone: written to a file of Prism's own, which the remote serves
    /// once, then removes; in its data folder (the owner's only, not a shared /tmp), a new file
    /// never one there already (a link). A folder brings no hidden file (`.env`…) unless named.
    pub async fn prepare_download(&self, id: &str, paths: Vec<String>, name: String) -> Result<files::Prepared> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let folder = self.dir(id)?;
        let dir = self.inner.work.join("downloads");
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&dir)?;
        // Those a Prism quit before serving (kept 5 minutes at most otherwise).
        for e in std::fs::read_dir(&dir)?.flatten() {
            if e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().unwrap_or_default() > std::time::Duration::from_secs(3600)) {
                let _ = std::fs::remove_file(e.path());
            }
        }
        // Those still waiting to be fetched: a phone asking in a loop fills no disk.
        let waiting: u64 = std::fs::read_dir(&dir)?.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum();
        if waiting > 2 * files::MAX_DOWNLOAD {
            bail!(tr("downloads are still waiting to be fetched: try again in a few minutes"));
        }
        let dest = dir.join(format!("{}-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed), uuid::Uuid::new_v4().simple()));
        let create = |p: &std::path::Path| {
            let mut open = std::fs::OpenOptions::new();
            open.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
            open.open(p)
        };
        let to = dest.clone();
        let name = tokio::task::spawn_blocking(move || files::export(&folder, &paths, &name, files::MAX_DOWNLOAD, false, &to, create)).await??;
        Ok(files::Prepared { name, path: dest.display().to_string() })
    }

    /// Files of its folder's dependencies written since it was launched (`files::deps_changed`).
    pub async fn deps_changed(&self, id: &str) -> Result<Vec<String>> {
        let dir = self.dir(id)?;
        let since: std::time::SystemTime = self.inner.registry.lock().unwrap().get(id).map(|r| r.created_at).context(tr("unknown agent"))?.into();
        Ok(tokio::task::spawn_blocking(move || files::deps_changed(&dir, since)).await?)
    }

    /// The diff of the agent's folder: against `HEAD`, or (`branch`) against its worktree's base.
    pub async fn diff(&self, id: &str, branch: bool) -> Result<files::Diff> {
        let dir = self.dir(id)?;
        let base = if branch { Some(self.base(&dir)?) } else { None };
        files::diff(&dir, base.as_deref()).await
    }

    /// Every path the agent's branch changes against its base, untracked files included, the
    /// diff's size cap aside; and a fingerprint of what `of` of them hold now.
    pub async fn changed_paths(&self, id: &str) -> Result<Vec<String>> {
        let dir = self.dir(id)?;
        let base = self.base(&dir)?;
        files::changed_paths(&dir, Some(&base)).await
    }

    pub async fn fingerprint(&self, id: &str, of: &[String]) -> Result<String> {
        files::fingerprint(&self.dir(id)?, of).await
    }

    /// An image of that diff, as a `data:` URL: its new version, or (`old`) the one it is compared to.
    pub async fn diff_image(&self, id: &str, path: &str, branch: bool, old: bool) -> Result<String> {
        let dir = self.dir(id)?;
        let base = if branch { Some(self.base(&dir)?) } else { None };
        files::diff_image(&dir, path, base.as_deref(), old).await
    }

    /// The base branch of a folder's worktree, otherwise of the repository it is in (a folder of a main checkout).
    fn base(&self, dir: &std::path::Path) -> Result<String> {
        let snap = self.inner.hub.subscribe();
        let path = dir.display().to_string();
        let wts = &snap.borrow().worktrees;
        let base = wts.iter().find(|w| w.path == path).or_else(|| wts.iter().filter(|w| w.is_main && dir.starts_with(&w.path)).max_by_key(|w| w.path.len()));
        base.map(|w| w.base.clone()).context(tr("no base branch known for this folder"))
    }

    /// A file of the agent's diff, whole, as it is in its working tree.
    pub async fn text(&self, id: &str, path: &str) -> Result<String> {
        files::text(&self.dir(id)?, path).await
    }

    /// A file of the agent's folder, checked to be inside it: to open it in the editor.
    pub fn file_path(&self, id: &str, path: &str) -> Result<PathBuf> {
        files::resolve(&self.dir(id)?, path)
    }

    /// A file of the agent's folder for the Files preview.
    pub async fn preview(&self, id: &str, path: &str) -> Result<files::Preview> {
        files::preview(&self.dir(id)?, path).await
    }

    /// Lines of the agent's folder matching the Files tab's search.
    pub async fn search(&self, id: &str, query: &files::Query) -> Result<files::Found> {
        files::search(&self.dir(id)?, query).await
    }

    /// The version on disk of files of the agent's folder open in its Files tab.
    pub fn stamps(&self, id: &str, paths: &[String]) -> Result<Vec<Option<String>>> {
        Ok(files::stamps(&self.dir(id)?, paths))
    }

    /// The agent's artifacts: the pages of its CLI session's scratchpad.
    pub fn artifacts(&self, id: &str) -> Result<Vec<files::Artifact>> {
        self.dir(id)?;
        Ok(self.scratchpad(id).map(|d| files::artifacts(&d)).unwrap_or_default())
    }

    /// The scratchpad of the agent's session: Claude Code's is Prism's id, or the conversation's
    /// since a `/clear`; Command Code's its own (reported by the mod at start); OpenCode and Codex have none.
    pub(crate) fn scratchpad(&self, id: &str) -> Option<PathBuf> {
        let provider = self.inner.registry.lock().unwrap().get(id)?.provider;
        let session = match provider {
            Provider::Claude => self.inner.folds.lock().unwrap().get(id).and_then(|f| f.session.clone()).unwrap_or_else(|| id.to_string()),
            Provider::CommandCode => self.inner.folds.lock().unwrap().get(id)?.session.clone()?,
            Provider::OpenCode | Provider::Codex => return None,
        };
        // It names a folder: nothing but a UUID.
        if session.is_empty() || !session.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return None;
        }
        files::scratchpad(&provider.tmp_dir()?, &session)
    }

    /// A file from the agent's folder for the `prismfile:` scheme (`<agent>/<path>`).
    pub fn serve(&self, url_path: &str) -> std::result::Result<(Vec<u8>, &'static str), (u16, String)> {
        let path = files::percent_decode(url_path.trim_start_matches('/'));
        let (id, rel) = path.split_once('/').unwrap_or((&path, ""));
        let dir = self.dir(id).map_err(|e| (404, format!("{e:#}")))?;
        match rel.split_once('/') {
            Some((files::SCRATCHPAD, rel)) => files::serve(&self.scratchpad(id).ok_or((404, "no scratchpad for this agent".to_string()))?, rel),
            _ => files::serve(&dir, rel),
        }
    }

    /// A text page of the Web view (`<path>` of the folder, or `@scratchpad/<path>`), resolved as `serve` does.
    pub fn page_text(&self, id: &str, path: &str) -> Result<String> {
        let dir = self.dir(id)?;
        match path.split_once('/') {
            Some((files::SCRATCHPAD, rel)) => files::page_text(&self.scratchpad(id).context("no scratchpad for this agent")?, rel),
            _ => files::page_text(&dir, path),
        }
    }

    /// Saves a file attached to a prompt: in a running agent's folder (`id`), its path relative to
    /// it; for an agent to launch, in Prism's working folder, its absolute path.
    pub fn attach_file(&self, id: Option<&str>, name: &str, data: &[u8]) -> Result<String> {
        match id {
            Some(id) => attach::into_folder(&self.dir(id)?, name, data),
            None => attach::for_launch(&self.inner.work, name, data),
        }
    }

    /// An image attached to one of its prompts (its path as typed there), for its Chat.
    pub fn attachment(&self, id: &str, path: &str) -> Result<String> {
        attach::image(&self.dir(id)?, &self.inner.work, path)
    }

    /// Runs a chain's task in the agent's folder: its exit code and the end of its output.
    pub async fn task(&self, id: &str, command: &str) -> Result<chain::TaskRun> {
        chain::task(&self.dir(id)?, command).await
    }

    /// A chain's MR step: the agent's branch pushed, a draft MR opened into its base.
    pub async fn open_mr(&self, id: &str) -> Result<chain::OpenedMr> {
        let dir = self.dir(id)?;
        let record = self.record(id)?;
        let snap = self.inner.hub.subscribe().borrow().clone();
        let ticket = snap.agents.iter().find(|a| a.id == id).and_then(|a| a.ticket.clone());
        // Its own folder, then those of the other repositories it works in: an MR in each that has
        // commits, linked to one another.
        let mut opened = Vec::new();
        for folder in std::iter::once(dir.clone()).chain(record.extra.iter().filter(|p| p.is_dir()).cloned()) {
            if let Some(mr) = chain::open_mr(&folder, &record.title, ticket.as_deref(), &snap).await.with_context(|| folder.display().to_string())? {
                opened.push(mr);
            }
        }
        if opened.is_empty() {
            let branch = snap.agents.iter().find(|a| a.id == id).and_then(|a| a.branch.clone()).unwrap_or_default();
            bail!(tr!("no commit on {branch} beyond its base: nothing to open an MR for", branch = branch));
        }
        if opened.len() > 1
            && let Err(e) = chain::link_mrs(&opened, &snap).await
        {
            tracing::warn!("agents: linking the MRs of {id}: {e:#}");
        }
        let mut first = opened.remove(0);
        first.also = opened;
        Ok(first)
    }

    /// Its folder under git (its Git bar): where it stands.
    pub async fn git_state(&self, id: &str) -> Result<vcs::GitState> {
        let dir = self.dir(id)?;
        let snap = self.inner.hub.subscribe().borrow().clone();
        vcs::state(&dir, &snap).await
    }

    /// Its changes committed as `message`; its short hash.
    pub async fn git_commit(&self, id: &str, message: &str) -> Result<String> {
        self.not_working(id, tr("committed"))?;
        let snap = self.inner.hub.subscribe().borrow().clone();
        vcs::commit(&self.dir(id)?, message, &snap).await
    }

    /// Its branch pushed; `force`: over a branch rewritten here.
    pub async fn git_push(&self, id: &str, force: bool) -> Result<()> {
        let snap = self.inner.hub.subscribe().borrow().clone();
        vcs::push(&self.dir(id)?, force, &snap).await
    }

    /// Brought up to date with its base.
    pub async fn git_sync(&self, id: &str) -> Result<String> {
        self.not_working(id, tr("rebased"))?;
        let snap = self.inner.hub.subscribe().borrow().clone();
        vcs::sync(&self.dir(id)?, &snap).await
    }

    /// A text file of its folder written back, as it was read (`original`) until now.
    pub fn save_file(&self, id: &str, path: &str, original: &str, text: &str) -> Result<()> {
        self.not_working(id, tr("edited"))?;
        files::save(&self.dir(id)?, path, original, text)
    }

    /// Nothing changes its folder under it while it works: a half-written change, a file it writes.
    /// A worktree of the Worktrees block, by its path: the branches it can be switched to.
    pub async fn worktree_branches(&self, path: &str) -> Result<Vec<vcs::BranchChoice>> {
        vcs::branches(&self.shown_worktree(path)?).await
    }

    /// A worktree's branch brought up to date with its upstream (fast-forward only).
    pub async fn worktree_pull(&self, path: &str) -> Result<String> {
        vcs::pull(&self.shown_worktree(path)?).await
    }

    /// A worktree switched to `branch`; refused while an agent runs in it (its work would land on
    /// another branch than the one it was given).
    pub async fn worktree_switch(&self, path: &str, branch: &str) -> Result<String> {
        let dir = self.shown_worktree(path)?;
        let inside = |d: &str| d == path || d.starts_with(&format!("{path}/"));
        if let Some(a) = self.inner.hub.subscribe().borrow().agents.iter().find(|a| a.state == AgentState::Running && (a.worktree.as_deref() == Some(path) || a.adopted.iter().any(|p| p == path) || inside(&a.cwd))) {
            bail!(tr!("the agent “{title}” is running in it: stop it first", title = a.title));
        }
        vcs::switch(&dir, branch).await
    }

    /// `path`, if it is a worktree the Worktrees block shows, its folder there.
    fn shown_worktree(&self, path: &str) -> Result<std::path::PathBuf> {
        let known = self.inner.hub.subscribe().borrow().worktrees.iter().any(|w| w.path == path && !w.prunable);
        if !known || !std::path::Path::new(path).is_dir() {
            bail!(tr!("not a worktree Prism shows: {path}", path = path));
        }
        Ok(path.into())
    }

    pub(crate) fn not_working(&self, id: &str, what: &str) -> Result<()> {
        let busy = self.inner.folds.lock().unwrap().get(id).is_some_and(|f| matches!(f.status, SessionStatus::Running | SessionStatus::WaitingTool));
        if busy {
            bail!(tr!("it is working: its folder is {what} once its turn is over", what = what));
        }
        Ok(())
    }
}
