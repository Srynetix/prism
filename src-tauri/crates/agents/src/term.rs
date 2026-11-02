//! An agent's terminal: attached from the window (a pseudo-terminal on its tmux session), watched or controlled from a phone, its history.

use super::*;

impl Agents {
    /// Attaches a terminal to the agent, or to one of its shells (`shell:<agent>:<n>`, opened by
    /// `shell`); a previous attachment of the same one is replaced.
    pub fn attach(&self, id: &str, cols: u16, rows: u16, on_data: impl Fn(Vec<u8>) + Send + 'static, on_end: impl FnOnce() + Send + 'static) -> Result<()> {
        let agent = id.strip_prefix(tmux::SHELL).and_then(|s| s.split(':').next()).unwrap_or(id);
        // A worktree's shell has no agent: its owner is the worktree (`folder_owner`).
        let folder = id.starts_with(tmux::SHELL) && is_folder_owner(agent);
        if !folder && self.inner.registry.lock().unwrap().get(agent).is_none() {
            bail!(tr("unknown agent"));
        }
        let token = self.inner.next_token.fetch_add(1, Ordering::Relaxed);
        let inner = self.inner.clone();
        let key = id.to_string();
        let label: String = id.chars().take(8).collect();
        let attachment = Attachment::spawn(&label, &self.inner.tmux.attach_argv(id), cols, rows, token, on_data, move || {
            let mut map = inner.attachments.lock().unwrap();
            if map.get(&key).is_some_and(|a| a.token == token) {
                map.remove(&key);
            }
            drop(map);
            on_end();
        })?;
        // The old tmux client is killed when leaving the map (Drop), outside the lock.
        let previous = self.inner.attachments.lock().unwrap().insert(id.to_string(), attachment);
        drop(previous);
        Ok(())
    }

    /// What a phone may reach: an agent's terminal, or a shell of an agent (`shell:<agent>:<n>`),
    /// never a worktree's shell nor anything else.
    pub(crate) fn phone_key(&self, id: &str) -> Result<()> {
        let agent = match id.strip_prefix(tmux::SHELL) {
            Some(rest) => {
                let (agent, n) = rest.rsplit_once(':').context("not a shell")?;
                n.parse::<u32>().context("not a shell")?;
                agent
            }
            None => id,
        };
        if self.inner.registry.lock().unwrap().get(agent).is_none() {
            bail!(tr("unknown agent"));
        }
        Ok(())
    }

    /// What the agent's terminal (or one of its shells) shows and has shown, as text with its
    /// colours: a phone scrolls it, where a watching client has no history. Without `screen`, only
    /// what scrolled off it: the phone shows the history above its live terminal.
    pub async fn history(&self, id: &str, lines: u32, screen: bool) -> Result<String> {
        self.phone_key(id)?;
        self.inner.tmux.history(id, lines.clamp(100, 10_000), screen).await
    }

    /// A phone watching the agent's terminal: a tmux client of its own, read-only, beside the
    /// desk's (or one of its shells'); its output through `on_data`. At the phone's `size`, the
    /// window takes it while the phone is the latest client; without one, at the window's size.
    /// Dropped, it detaches.
    pub async fn watch(&self, id: &str, size: Option<(u16, u16)>, on_data: impl Fn(Vec<u8>) + Send + 'static, on_end: impl FnOnce() + Send + 'static) -> Result<(Attachment, u16, u16)> {
        self.phone_key(id)?;
        let (cols, rows) = match size {
            Some(s) => s,
            None => self.inner.tmux.window_size(id).await?,
        };
        let token = self.inner.next_token.fetch_add(1, Ordering::Relaxed);
        let label: String = format!("phone:{}", id.chars().take(8).collect::<String>());
        let attachment = Attachment::spawn(&label, &self.inner.tmux.watch_argv(id, size.is_some()), cols, rows, token, on_data, on_end)?;
        Ok((attachment, cols, rows))
    }

    /// A phone typing in the agent's terminal, or one of its shells: a tmux client of its own at
    /// the phone's size, beside the desk's (the window takes the size of the last client to type).
    /// Dropped, it detaches.
    pub fn control(&self, id: &str, cols: u16, rows: u16, on_data: impl Fn(Vec<u8>) + Send + 'static, on_end: impl FnOnce() + Send + 'static) -> Result<Attachment> {
        self.phone_key(id)?;
        let token = self.inner.next_token.fetch_add(1, Ordering::Relaxed);
        let label: String = format!("phone:{}", id.chars().take(8).collect::<String>());
        Attachment::spawn(&label, &self.inner.tmux.attach_argv(id), cols, rows, token, on_data, on_end)
    }

    pub fn detach(&self, id: &str) {
        let previous = self.inner.attachments.lock().unwrap().remove(id);
        drop(previous);
    }

    pub fn write(&self, id: &str, data: &[u8]) -> Result<()> {
        self.inner.attachments.lock().unwrap().get_mut(id).context("agent not attached")?.write(data)?;
        // A choice, Enter or Esc typed in its permission's menu answers it (the arrows only move in it).
        if self.provider(id).is_ok_and(|p| answers_a_menu(p, data)) && self.inner.folds.lock().unwrap().get(id).is_some_and(|f| f.status == SessionStatus::WaitingTool) {
            self.permission_answered(id);
        }
        Ok(())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        self.inner.attachments.lock().unwrap().get(id).context("agent not attached")?.resize(cols, rows)
    }

    /// Takes the agent's pane, or one of its shells', out of tmux's copy mode before a key.
    pub async fn leave_copy_mode(&self, key: &str) -> Result<()> {
        self.inner.tmux.leave_copy_mode(key).await
    }

    /// The agent's pane, or one of its shells', is in tmux's copy mode (its view frozen).
    pub async fn in_copy_mode(&self, key: &str) -> Result<bool> {
        self.inner.tmux.in_copy_mode(key).await
    }
}

/// A key that picks in a menu: a choice's digit, Enter, Esc — alone, as a terminal sends one key;
/// in Codex's, the letters of its choices too (`y` proceed, `a` always, `n` no).
fn answers_a_menu(provider: Provider, data: &[u8]) -> bool {
    matches!(data, [b'1'..=b'9'] | b"\r" | b"\x1b") || (provider == Provider::Codex && matches!(data, b"y" | b"a" | b"n"))
}
