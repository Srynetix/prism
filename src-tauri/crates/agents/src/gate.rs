//! An agent's CLI asking before it starts (`trust.rs`): read off its screen at each refresh while
//! it may be asking (no hook yet, or at its prompt), shown as its gate with the status of a
//! permission; answered from its card, the keys sent only while that question is still on its
//! screen, then read again to see it gone. « All worktrees of … » trusts the folder of the
//! repository's worktrees first: its CLI started there, out of sight, its question answered, then
//! quit — Prism writes nothing in the CLI's own settings.

use super::*;
use crate::state::{AgentGate, GateKind};

/// How long the CLI started in the worktrees' folder is given to ask, and to start once answered.
const PARENT_WAIT: Duration = Duration::from_secs(12);

impl Agents {
    /// The gates on the agents' screens (`screens`: those read), each one's status and words; the
    /// others' gates forgotten.
    pub(crate) fn apply_gates(&self, list: &mut [Agent], screens: &HashMap<String, String>) {
        let worktrees_dir = self.inner.settings.lock().unwrap().worktrees_dir.clone();
        let mut gates = self.inner.gates.lock().unwrap();
        gates.retain(|id, _| screens.contains_key(id));
        for a in list.iter_mut() {
            let Some(screen) = screens.get(&a.id) else { continue };
            let Some(mut gate) = trust::read(a.provider, screen) else {
                gates.remove(&a.id);
                continue;
            };
            if gate.kind == GateKind::Folder && a.provider == Provider::Claude {
                gate.parent = a.worktree.as_deref().and_then(|w| worktrees_parent(Path::new(w), worktrees_dir.as_deref()));
            }
            a.status = SessionStatus::WaitingTool;
            a.needs_action = Some(match gate.kind {
                GateKind::Folder => tr!("Trust its folder: {folder}", folder = gate.folder.clone().unwrap_or_else(|| a.worktree.clone().unwrap_or_else(|| a.cwd.clone()))),
                GateKind::Hooks => tr("Approve Prism's hooks").to_string(),
                GateKind::Other => tr("An answer in its terminal").to_string(),
            });
            gates.insert(a.id.clone(), gate.clone());
            a.gate = Some(gate);
        }
    }

    /// Its CLI asks before it starts: what (the last refresh's read).
    pub(crate) fn gate(&self, id: &str) -> Option<AgentGate> {
        self.inner.gates.lock().unwrap().get(id).cloned()
    }

    /// Answers the gate `kind` on its screen: `yes` trusts (or approves), no refuses (the CLI
    /// exits). `parent`: its worktrees' folder trusted first, the next ones not asked again.
    pub async fn answer_gate(&self, id: &str, kind: GateKind, yes: bool, parent: bool) -> Result<()> {
        let provider = self.provider(id)?;
        let tmux = &self.inner.tmux;
        tmux.leave_copy_mode(id).await?;
        // Checked before anything: still that question, one Prism knows.
        trust::keys(provider, &tmux.screen(id).await?, kind, yes)?;
        if yes && parent {
            let folder = self.gate(id).and_then(|g| g.parent).context(tr("no worktrees' folder to trust"))?;
            self.trust_folder(provider, Path::new(&folder)).await?;
        }
        // Read again just before the keys: they go to that question only.
        let keys = trust::keys(provider, &tmux.screen(id).await?, kind, yes)?;
        for k in keys {
            tmux.send_keys(id, &[k]).await?;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        for _ in 0..16 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let now = tmux.screen(id).await.unwrap_or_default();
            if trust::read(provider, &now).is_none_or(|g| g.kind != kind) {
                self.inner.gates.lock().unwrap().remove(id);
                self.inner.changed.notify_one();
                return Ok(());
            }
        }
        bail!(tr("its question is still on its screen: answer it in its terminal"))
    }

    /// `folder` trusted by `provider` (Claude Code), as you would: its CLI started there out of
    /// sight, its question answered yes, then quit. Already trusted (no question), nothing to do.
    async fn trust_folder(&self, provider: Provider, folder: &Path) -> Result<()> {
        ensure!(provider == Provider::Claude, tr("only Claude Code trusts a folder for those inside it"));
        ensure!(folder.is_dir(), tr!("{path} is not a directory", path = folder.display()));
        let key = format!("trust-{}", Utc::now().timestamp_millis());
        let argv = provider.with_cli(vec![provider.program().to_string()], &self.cli(provider, None));
        let tmux = &self.inner.tmux;
        tmux.new_session(&key, folder, &[], &argv).await?;
        let result = async {
            let until = std::time::Instant::now() + PARENT_WAIT;
            // Its question, or its prompt (already trusted).
            let keys = loop {
                tokio::time::sleep(Duration::from_millis(400)).await;
                let screen = tmux.screen(&key).await.unwrap_or_default();
                if let Some(g) = trust::read(provider, &screen) {
                    let same = g.folder.as_deref().is_none_or(|f| f.trim_end_matches('/') == folder.display().to_string().trim_end_matches('/'));
                    ensure!(same, tr("its CLI asked about another folder: nothing answered"));
                    break Some(trust::keys(provider, &screen, GateKind::Folder, true)?);
                }
                if suggestion::visible(&screen).contains("? for shortcuts") || std::time::Instant::now() > until {
                    break None;
                }
            };
            let Some(keys) = keys else { return Ok(()) };
            for k in keys {
                tmux.send_keys(&key, &[k]).await?;
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            for _ in 0..20 {
                tokio::time::sleep(Duration::from_millis(250)).await;
                if trust::read(provider, &tmux.screen(&key).await.unwrap_or_default()).is_none() {
                    return Ok(());
                }
            }
            bail!(tr("the worktrees' folder was not trusted: its question is still open"))
        }
        .await;
        let _ = tmux.kill(&key).await;
        result
    }
}

/// The folder of a repository's worktrees holding `worktree`: `<repo>.worktrees`, or
/// `[agents] worktrees_dir`.
fn worktrees_parent(worktree: &Path, worktrees_dir: Option<&Path>) -> Option<String> {
    let parent = worktree.parent()?;
    let named = parent.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".worktrees"));
    (named || worktrees_dir.is_some_and(|d| d == parent)).then(|| parent.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Codex's hook command: the same path whatever Prism's, the script following Prism.
    #[cfg(unix)]
    #[test]
    fn codex_hooks_go_through_a_path_that_stays() {
        let work = std::env::temp_dir().join(format!("prism-stable-exe-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let a = crate::stable_exe(&work, Path::new("/Applications/Prism.app/Contents/MacOS/prism")).unwrap();
        let b = crate::stable_exe(&work, Path::new("/tmp/dev build/prism")).unwrap();
        assert_eq!(a, b);
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "#!/bin/sh\nexec '/tmp/dev build/prism' \"$@\"\n");
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn the_worktrees_folder_is_their_parent() {
        assert_eq!(worktrees_parent(Path::new("/dev/prism.worktrees/fix-scroll"), None).as_deref(), Some("/dev/prism.worktrees"));
        assert_eq!(worktrees_parent(Path::new("/wt/fix"), Some(Path::new("/wt"))).as_deref(), Some("/wt"));
        assert_eq!(worktrees_parent(Path::new("/dev/prism/sub"), None), None);
    }
}
