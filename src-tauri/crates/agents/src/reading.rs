//! What an agent's transcript tells (Claude Code, Codex; Command Code: its conversation only): its conversation, context, activity, slash commands; its CLI's models.

use super::*;

impl Agents {
    /// What the agent did, from its hook log (the Activity tab).
    pub fn activity(&self, id: &str) -> Result<activity::Activity> {
        if self.inner.registry.lock().unwrap().get(id).is_none() {
            bail!(tr("unknown agent"));
        }
        Ok(activity::read(&self.inner.events.join(format!("{id}.jsonl"))))
    }

    /// What the agent's context is made of (the context gauge's detail), from its transcript:
    /// Claude Code's or Codex's, absent for the others or before a first reply.
    pub async fn breakdown(&self, id: &str) -> Result<Option<breakdown::Breakdown>> {
        let provider = self.provider(id)?;
        if !provider.has_transcript() {
            return Ok(None);
        }
        let Some(transcript) = self.inner.folds.lock().unwrap().get(id).and_then(|f| f.transcript.clone()) else { return Ok(None) };
        let cwd = self.dir(id).ok();
        Ok(tokio::task::spawn_blocking(move || breakdown::read(Path::new(&transcript), cwd.as_deref())).await?)
    }

    /// The slash commands its CLI takes (its Chat box's autocomplete), as the CLI tells them for
    /// its folder, with its executable and arguments (`[agents.<cli>]`).
    pub async fn slash_commands(&self, id: &str) -> Result<Vec<slash::SlashCommand>> {
        let provider = self.provider(id)?;
        let cwd = self.dir(id)?;
        let argv = provider.with_cli(vec![provider.program().into()], self.inner.settings.lock().unwrap().cli(provider));
        slash::list(provider, argv, cwd).await
    }

    /// How `provider` is launched (`[agents.<cli>]`), with the model and the effort picked for the
    /// agent in place of the CLI's own when there are.
    pub(crate) fn cli(&self, provider: Provider, launch: Option<&state::LaunchSpec>) -> prism_core::config::CliConfig {
        let mut cli = self.inner.settings.lock().unwrap().cli(provider).clone();
        let picked = |v: Option<&String>| v.map(|s| s.trim()).filter(|s| !s.is_empty()).map(String::from);
        if let Some(m) = picked(launch.and_then(|l| l.model.as_ref())) {
            cli.model = Some(m);
        }
        if let Some(e) = picked(launch.and_then(|l| l.effort.as_ref())) {
            cli.effort = Some(e);
        }
        // Prism's MCP server and skill: Claude Code's plugin, Codex's options (OpenCode's in its
        // environment, `env`); or its « in passing » mode alone (`suggest_task`, `dismiss_task`).
        // Nothing written here: the launcher's preview builds this too.
        if self.prism_mcp(launch) {
            let command = prism_core::mcp::command();
            match provider {
                // Written with the rest of the wiring, as the agent launches (`write_wiring`).
                Provider::Claude => cli.args.extend(["--plugin-dir".to_string(), prism_core::mcp::claude_plugin_dir(&self.inner.work).display().to_string()]),
                Provider::Codex => cli.args.extend(prism_core::mcp::codex_args(&command)),
                Provider::OpenCode | Provider::CommandCode => {}
            }
        } else if self.aside_mcp() {
            match provider {
                Provider::Claude => cli.args.extend(["--mcp-config".to_string(), prism_core::mcp::claude_aside_file(&self.inner.work).display().to_string()]),
                Provider::Codex => cli.args.extend(prism_core::mcp::codex_args(&prism_core::mcp::aside_command())),
                Provider::OpenCode | Provider::CommandCode => {}
            }
        }
        // `suggest_task` only records a proposal: never a permission prompt that would stop the agent.
        if provider == Provider::Claude && self.aside_mcp() {
            // Its tools as named by `--mcp-config`, and by the plugin of the whole server (`plugin_<plugin>_<server>`).
            let s = prism_core::mcp::SERVER;
            cli.args.push(format!("--allowedTools=mcp__{s}__suggest_task,mcp__{s}__dismiss_task,mcp__plugin_{s}_{s}__suggest_task,mcp__plugin_{s}_{s}__dismiss_task"));
        }
        cli
    }

    /// The agent gets Prism's MCP server: `[agents] prism_mcp`, or its launch asked for it.
    pub(crate) fn prism_mcp(&self, launch: Option<&state::LaunchSpec>) -> bool {
        launch.is_some_and(|l| l.prism_mcp) || self.inner.settings.lock().unwrap().prism_mcp
    }

    /// The agent gets the « in passing » tools (`[agents.aside] enabled`): with the whole server,
    /// or alone.
    pub(crate) fn aside_mcp(&self) -> bool {
        self.inner.settings.lock().unwrap().aside.enabled
    }

    /// The command line of Prism's MCP server the agent gets, if any: the whole one, or its
    /// « in passing » mode alone.
    pub(crate) fn mcp_command(&self, launch: Option<&state::LaunchSpec>) -> Option<Vec<String>> {
        if self.prism_mcp(launch) {
            Some(prism_core::mcp::command())
        } else {
            self.aside_mcp().then(prism_core::mcp::aside_command)
        }
    }

    /// The reasoning efforts `provider` takes (for `model`, Codex's levels being each model's), for
    /// Settings, the launcher and an agent's header; empty for a CLI without.
    pub async fn efforts(&self, provider: Provider, model: Option<&str>) -> Vec<String> {
        let cli = self.inner.settings.lock().unwrap().cli(provider).clone();
        let program = cli.bin.unwrap_or_else(|| provider.program().into());
        let model = model.map(String::from).or(cli.model);
        prism_core::agents::models::efforts(provider, &program, model.as_deref()).await
    }

    /// The models a CLI offers, for Settings' model field: from its catalog, or asked of it with
    /// its executable (`[agents.<cli>] bin`).
    pub async fn models(&self, provider: Provider) -> Vec<String> {
        let program = self.inner.settings.lock().unwrap().cli(provider).bin.clone().unwrap_or_else(|| provider.program().into());
        prism_core::agents::models::list(provider, &program).await
    }

    /// Its transcript (Claude Code's, Command Code's) or rollout (Codex's); none for OpenCode, or
    /// before it has one. Command Code's found by the session its mod reported.
    pub(crate) fn transcript(&self, id: &str) -> Result<Option<PathBuf>> {
        let provider = self.provider(id)?;
        if !provider.has_conversation() {
            return Ok(None);
        }
        let (session, transcript) = self.inner.folds.lock().unwrap().get(id).map(|f| (f.session.clone(), f.transcript.clone())).unwrap_or_default();
        let transcript = match transcript {
            Some(t) => Some(PathBuf::from(t)),
            None if provider.finds_transcript() => usage::find_transcript(session.as_deref().unwrap_or(id)),
            None if provider == Provider::CommandCode => session.as_deref().and_then(usage::find_command_code_transcript),
            None => None,
        };
        Ok(transcript.filter(|t| t.is_file()))
    }

    /// Image `n` pasted in one of its prompts (`Turn::pasted`), as a `data:` URL, for its Chat.
    pub fn pasted_image(&self, id: &str, line: u64, n: usize) -> Result<String> {
        let transcript = self.transcript(id)?.context("no transcript")?;
        conversation::pasted_image(&transcript, line, n)
    }

    /// The agent's conversation as a phone reads it (its Chat tab), its last `turns`: from Claude
    /// Code's or Command Code's transcript, or Codex's rollout; absent for OpenCode, or before it has one.
    /// `have`: the signatures of the turns the reader holds, left out (`Conversation::kept`).
    pub async fn conversation(&self, id: &str, turns: u32, have: &[String]) -> Result<Option<conversation::Conversation>> {
        // Its question asked or plan proposed, not in its transcript yet (Command Code): a conversation of its own
        // before its first turn is written.
        let waiting = self.inner.folds.lock().unwrap().get(id).and_then(|f| f.waiting.clone());
        let transcript = self.transcript(id)?;
        if transcript.is_none() && waiting.is_none() {
            return Ok(None);
        }
        let cwd = self.dir(id).ok();
        // All of a session's turns, at most: the Chat's All (ChatView's ALL).
        let turns = turns.clamp(1, 5000) as usize;
        // Signed off the async workers too: all of a long session's turns are tens of ms.
        let have = have.to_vec();
        let mut c = tokio::task::spawn_blocking(move || {
            let mut c = transcript.map(|t| conversation::read(&t, cwd.as_deref(), turns)).unwrap_or_default();
            if let Some(w) = waiting {
                c.waiting(w);
            }
            c.only_new(&have);
            c
        })
        .await?;
        // What it is doing right now: its screen has it, its transcript not yet. Only while it
        // works, the Chat shows nothing of it otherwise (a tmux process a read saved).
        let working = self.inner.folds.lock().unwrap().get(id).is_none_or(|f| matches!(f.status, SessionStatus::Running | SessionStatus::WaitingTool | SessionStatus::Background));
        if working {
            c.live = self.inner.tmux.screen(id).await.ok().and_then(|s| live::read(&s));
        }
        Ok(Some(c))
    }
}
