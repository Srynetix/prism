//! The context window of the model an agent runs, when its CLI's log does not tell it: OpenCode's catalog, Command Code's status.

use super::*;

impl Agents {
    /// The window of the model an agent runs, when its CLI's log does not tell it.
    pub(crate) fn window(&self, provider: Provider, model: Option<&str>) -> Option<u64> {
        match provider {
            // Codex's transcript tells it with the tokens.
            Provider::Claude | Provider::Codex => None,
            Provider::CommandCode => self.command_code_window(model),
            Provider::OpenCode => self.opencode_window(model?),
        }
    }

    /// The window of an OpenCode model, from its catalog: read in the background, again when a
    /// model is missing from it (10 min apart).
    pub(crate) fn opencode_window(&self, model: &str) -> Option<u64> {
        const AGAIN: std::time::Duration = std::time::Duration::from_secs(600);
        let mut cache = self.inner.opencode_windows.lock().unwrap();
        if let Some(size) = cache.known.get(model) {
            return Some(*size);
        }
        if cache.read.is_none_or(|at| at.elapsed() > AGAIN) {
            cache.read = Some(std::time::Instant::now());
            let this = self.clone();
            tokio::task::spawn_blocking(move || {
                if let Some(found) = context::opencode_windows() {
                    this.inner.opencode_windows.lock().unwrap().known = found;
                    this.inner.changed.notify_one();
                }
            });
        }
        None
    }

    /// The window of the model a Command Code agent runs, when it is the default one; asked of
    /// `cmd status --json` in the background, again when an agent runs another model (10 min apart).
    pub(crate) fn command_code_window(&self, model: Option<&str>) -> Option<u64> {
        const AGAIN: std::time::Duration = std::time::Duration::from_secs(600);
        let model = model?;
        let mut cache = self.inner.command_code_window.lock().unwrap();
        if let Some((_, size)) = cache.known.as_ref().filter(|(default, _)| default == model) {
            return Some(*size);
        }
        if cache.asked.is_none_or(|at| at.elapsed() > AGAIN) {
            cache.asked = Some(std::time::Instant::now());
            let this = self.clone();
            let program = self.inner.settings.lock().unwrap().command_code.bin.clone().unwrap_or_else(|| Provider::CommandCode.program().into());
            tokio::spawn(async move {
                if let Some(found) = context::command_code_window(&program).await {
                    this.inner.command_code_window.lock().unwrap().known = Some(found);
                    this.inner.changed.notify_one();
                }
            });
        }
        None
    }
}
