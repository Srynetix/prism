//! An agent's shells, and a worktree's: login shells in its folder kept by tmux; the commands Prism runs there (Tasks).

use super::*;

impl Agents {
    /// The agent's shells, login shells in its folder kept alive by tmux like the agent.
    pub async fn shells(&self, id: &str) -> Result<Vec<Shell>> {
        self.dir(id)?;
        let shells = self.inner.tmux.shells(id).await?;
        Ok(shells.into_iter().map(|(n, command)| Shell { key: shell_key(id, n), n, busy: busy(&command), command }).collect())
    }

    /// Opens the agent's shell `n` (started again after an `exit`), or a new one (`None`).
    /// Returns its terminal key (`shell:<agent>:<n>`).
    pub async fn shell(&self, id: &str, n: Option<u32>) -> Result<String> {
        Ok(self.open_shell(id, n).await?.0)
    }

    /// `shell`, and whether it was started just now.
    async fn open_shell(&self, id: &str, n: Option<u32>) -> Result<(String, bool)> {
        let dir = self.dir(id)?;
        let tmux = &self.inner.tmux;
        let n = match n {
            Some(n) => n,
            None => tmux.shells(id).await?.iter().map(|(n, _)| n + 1).max().unwrap_or(1),
        };
        let key = shell_key(id, n);
        if tmux.pane(&key).await?.is_some() {
            return Ok((key, false));
        }
        // No program: tmux starts the default shell, as a login shell.
        tmux.new_session(&key, &dir, &[], &[]).await?;
        tmux.end_with_program(&key).await?;
        Ok((key, true))
    }

    /// The program holding the shell of `key`, `None` at its prompt. A shell still starting
    /// (`starting`) is asked again 100 ms later, for up to 2 s, before `tmux` counts.
    async fn holder(&self, key: &str) -> Result<Option<String>> {
        let tmux = &self.inner.tmux;
        for _ in 0..HOLDER_TRIES {
            let cmd = tmux.pane(key).await?.map(|(_, cmd)| cmd).filter(|cmd| !is_shell(cmd));
            if !cmd.as_deref().is_some_and(starting) {
                return Ok(cmd);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(Some("tmux".into()))
    }

    /// Closes the agent's shell `n`, whatever it runs.
    pub async fn close_shell(&self, id: &str, n: u32) -> Result<()> {
        let key = shell_key(id, n);
        self.detach(&key);
        self.inner.tmux.kill(&key).await
    }

    /// A worktree's shells (the Worktrees view), with no agent: login shells in its folder, kept by
    /// tmux like an agent's. Only in a worktree Prism shows (`[[repos]] worktrees`, main checkouts included).
    pub async fn folder_shells(&self, path: &str) -> Result<Vec<Shell>> {
        let id = self.folder_owner(path)?;
        let shells = self.inner.tmux.shells(&id).await?;
        Ok(shells.into_iter().map(|(n, command)| Shell { key: shell_key(&id, n), n, busy: busy(&command), command }).collect())
    }

    /// Opens the worktree's shell `n` (again after an `exit`), or a new one (`None`): its terminal key.
    pub async fn folder_shell(&self, path: &str, n: Option<u32>) -> Result<String> {
        let id = self.folder_owner(path)?;
        let tmux = &self.inner.tmux;
        let n = match n {
            Some(n) => n,
            None => tmux.shells(&id).await?.iter().map(|(n, _)| n + 1).max().unwrap_or(1),
        };
        let key = shell_key(&id, n);
        if tmux.pane(&key).await?.is_none() {
            tmux.new_session(&key, Path::new(path), &[], &[]).await?;
            tmux.end_with_program(&key).await?;
        }
        Ok(key)
    }

    /// Closes the worktree's shell `n`, whatever it runs.
    pub async fn close_folder_shell(&self, path: &str, n: u32) -> Result<()> {
        let key = shell_key(&self.folder_owner(path)?, n);
        self.detach(&key);
        self.inner.tmux.kill(&key).await
    }

    /// Closes every shell of a worktree about to be removed: they would be left in a folder that no longer exists.
    pub async fn close_folder_shells(&self, path: &str) {
        let id = folder_owner(path);
        for (n, _) in self.inner.tmux.shells(&id).await.unwrap_or_default() {
            let key = shell_key(&id, n);
            self.detach(&key);
            if let Err(e) = self.inner.tmux.kill(&key).await {
                tracing::warn!("agents: closing shell {n} of {path}: {e:#}");
            }
        }
    }

    /// Every shell with no agent still open, a worktree's or a pod's (the status bar's list), oldest
    /// first. A pod's shell opened before Prism noted its pod (`@prism-pod`) cannot be named: left out.
    pub async fn open_shells(&self) -> Result<Vec<OpenShell>> {
        let mut shells: Vec<OpenShell> = self
            .inner
            .tmux
            .all_shells()
            .await?
            .into_iter()
            .filter(|s| is_folder_owner(&s.owner))
            .filter_map(|s| {
                let pod = if s.owner.starts_with("kp") { Some(serde_json::from_str::<PodRef>(&s.pod).ok()?) } else { None };
                Some(OpenShell {
                    key: shell_key(&s.owner, s.n),
                    n: s.n,
                    folder: pod.is_none().then_some(s.path),
                    // A pod's shell runs kubectl: anything else in its foreground, kubectl has exited (its wrapper waits for Enter).
                    ended: pod.is_some() && s.command != "kubectl",
                    busy: pod.is_none() && busy(&s.command),
                    pod,
                    command: s.command,
                    started: s.created,
                })
            })
            .collect();
        shells.sort_by_key(|s| (s.started, s.n));
        Ok(shells)
    }

    /// Closes a shell with no agent by its key (`open_shells`), whatever it runs: its worktree or
    /// pod may be gone, or no longer shown.
    pub async fn close_open_shell(&self, key: &str) -> Result<()> {
        let owner = key.strip_prefix(SHELL).and_then(|k| k.rsplit_once(':')).filter(|(o, n)| is_folder_owner(o) && n.parse::<u32>().is_ok());
        ensure!(owner.is_some(), "not a worktree's or a pod's shell: {key}");
        self.detach(key);
        self.inner.tmux.kill(key).await
    }

    /// The owner of a worktree's shells: `wt` and a fingerprint of its path, stable across restarts
    /// (tmux keeps the shells meanwhile). Refused for a folder that is not one of the worktrees shown.
    pub(crate) fn folder_owner(&self, path: &str) -> Result<String> {
        let known = self.inner.hub.subscribe().borrow().worktrees.iter().any(|w| w.path == path && !w.prunable);
        if !known || !Path::new(path).is_dir() {
            bail!(tr!("not a worktree Prism shows: {path}", path = path));
        }
        Ok(folder_owner(path))
    }

    /// Runs a command line in the agent's shell `n`, or a new one (`None`), as typed (`enter`:
    /// false leaves it there to finish). Refused while a program holds the shell, unless
    /// `interrupt`: Ctrl-C first. A shell started for it holds nothing yet: the line waits in
    /// its terminal until it reads. Returns the shell's key.
    pub async fn run_in_shell(&self, id: &str, n: Option<u32>, command: &str, enter: bool, interrupt: bool) -> Result<String> {
        let (key, new) = self.open_shell(id, n).await?;
        let tmux = &self.inner.tmux;
        if let Some(cmd) = if new { None } else { self.holder(&key).await? } {
            if !interrupt {
                bail!(tr!("the shell is busy with {cmd}", cmd = cmd));
            }
            tmux.send_keys(&key, &["C-c"]).await?;
            tokio::time::sleep(Duration::from_millis(400)).await;
            if let Some(cmd) = self.holder(&key).await? {
                bail!(tr!("{cmd} still holds the shell after Ctrl-C", cmd = cmd));
            }
        }
        tmux.send_keys(&key, &["-l", command]).await?;
        if enter {
            tmux::ran_in_shell(&key, command);
            tmux.send_keys(&key, &["Enter"]).await?;
        }
        Ok(key)
    }

    /// The tasks its folder declares: `just`, `task`, `package.json` scripts.
    pub async fn tasks(&self, id: &str) -> Result<Vec<tasks::Source>> {
        let depth = self.inner.settings.lock().unwrap().tasks_depth;
        tasks::detect(&self.dir(id)?, depth).await
    }

    /// The tasks a folder declares, before an agent runs there (a launch's "Then" from a phone).
    pub async fn tasks_in(&self, dir: &Path) -> Result<Vec<tasks::Source>> {
        let depth = self.inner.settings.lock().unwrap().tasks_depth;
        tasks::detect(dir, depth).await
    }
}

/// Looks at a shell still starting this many times, 100 ms apart.
const HOLDER_TRIES: u32 = 20;

/// Whether `pane_current_command` says a program holds the shell: neither the shell at its
/// prompt, nor the shell still starting.
fn busy(command: &str) -> bool {
    !is_shell(command) && !starting(command)
}

/// A shell's pane named `tmux`, on Linux: tmux names the pane after its foreground process, and
/// until the shell has started that is the child tmux has just forked. Elsewhere (macOS reads
/// the name otherwise) `tmux` is a tmux run in the shell. The tests' fake tmux does as Linux.
fn starting(command: &str) -> bool {
    cfg!(any(target_os = "linux", test)) && command == "tmux"
}
