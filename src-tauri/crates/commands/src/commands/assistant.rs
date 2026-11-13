//! Prism's assistant (`[agents.assistant]`), from the window: a request asked (« Ask Prism »), a new
//! conversation each time; its proposals answered from their cards; its memory read, changed and
//! emptied from the Assistant panel.

use super::*;
use prism_core::assistant::{self as core, Fact};

fn config() -> anyhow::Result<crate::config::Config> {
    crate::config::load(&crate::config_path())
}

/// A request to the assistant: a new conversation, in its folder, with its CLI; its agent's id.
#[prism_macros::command]
pub async fn assistant_ask(state: State<'_, AppState>, text: String) -> Cmd<String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(anyhow::anyhow!("{}", prism_base::i18n::tr("ask something")).into());
    }
    let cfg = config()?;
    let a = &cfg.agents.assistant;
    if !a.enabled {
        return Err(anyhow::anyhow!("[agents.assistant] enabled is off").into());
    }
    let home = tokio::task::spawn_blocking({
        let cfg = cfg.clone();
        move || core::write_home(&cfg)
    })
    .await??;
    let provider = a.provider.unwrap_or(cfg.agents.provider);
    let title: String = {
        let line = text.lines().next().unwrap_or_default().trim();
        if line.chars().count() > 60 { format!("{}…", line.chars().take(59).collect::<String>()) } else { line.to_string() }
    };
    let launch = crate::agents::Launch {
        title,
        cwd: Some(home),
        prompt: Some(text),
        worktree: None,
        // Codex in its read-only sandbox: the assistant writes nowhere (Claude Code goes without
        // the tools that would, `assistant_args`).
        permission_mode: (provider == crate::agents::provider::Provider::Codex).then(|| "plan".to_string()),
        category: None,
        tags: Vec::new(),
        provider: Some(provider),
        account: None,
        model: a.model.clone(),
        effort: a.effort.clone(),
        then: Vec::new(),
        from: Some("Ask Prism".into()),
        prism_mcp: true,
        assistant: true,
    };
    state.agents.spawn(launch).await.map_err(Into::into)
}

/// A proposal of the assistant answered from its card: what became of it (`created`, `launched`,
/// `added`, `written`, `remembered`, `modified`, `dismissed`) and what goes back with it.
#[prism_macros::command]
pub fn assistant_answer(state: State<'_, AppState>, id: u64, status: String, detail: Option<serde_json::Value>) -> Cmd {
    crate::mcp::assistant::answer(&state, id, &status, detail.unwrap_or_default()).map_err(Into::into)
}

/// The assistant's memory: its facts, and the folder they are in.
#[derive(serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AssistantMemory {
    pub dir: String,
    pub facts: Vec<Fact>,
}

#[prism_macros::command]
pub fn assistant_memory() -> Cmd<AssistantMemory> {
    let cfg = config()?;
    Ok(AssistantMemory { dir: core::memory_dir(&cfg).display().to_string(), facts: core::facts(&cfg) })
}

/// A fact written (a new one without id), from the Memory tab.
#[prism_macros::command]
pub fn assistant_memory_save(fact: Fact) -> Cmd<Fact> {
    let cfg = config()?;
    let saved = core::save(&cfg, fact)?;
    crate::mcp::assistant::refresh_home(&cfg);
    Ok(saved)
}

#[prism_macros::command]
pub fn assistant_memory_forget(id: String) -> Cmd {
    let cfg = config()?;
    core::forget(&cfg, &id)?;
    crate::mcp::assistant::refresh_home(&cfg);
    Ok(())
}

/// Every fact forgotten; how many.
#[prism_macros::command]
pub fn assistant_memory_clear() -> Cmd<usize> {
    let cfg = config()?;
    let n = core::forget_all(&cfg)?;
    crate::mcp::assistant::refresh_home(&cfg);
    Ok(n)
}
