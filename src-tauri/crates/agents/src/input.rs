//! What is typed to an agent: prompts, keys, answers to its questions, quick answers, slash commands.

use super::*;

impl Agents {
    /// Compacts the agent's conversation: `/compact` typed in its CLI, once its turn is over.
    pub async fn compact(&self, id: &str) -> Result<()> {
        self.slash(id, tr("compact"), "/compact").await
    }

    /// Clears an agent's conversation once its turn is over (`/clear`, Claude Code's and Codex's):
    /// its transcript stays on disk, the agent starts afresh in the same terminal.
    pub async fn clear(&self, id: &str) -> Result<()> {
        self.slash(id, tr("clear"), "/clear").await
    }

    /// One of the CLI's own commands (`/compact`, `/clear`), typed at its prompt: Claude Code and
    /// Codex only, never while it works or asks for a permission.
    pub(crate) async fn slash(&self, id: &str, what: &str, command: &str) -> Result<()> {
        let provider = self.provider(id)?;
        if !provider.has_transcript() {
            bail!(tr!("only Claude Code and Codex agents {what} from Prism", what = what));
        }
        let status = self.inner.folds.lock().unwrap().get(id).map(|f| f.status);
        if matches!(status, Some(SessionStatus::Running | SessionStatus::WaitingTool)) {
            bail!(tr!("the agent is working: {what} it once its turn is over", what = what));
        }
        let tmux = &self.inner.tmux;
        tmux.leave_copy_mode(id).await?;
        tmux.send_keys(id, &["-l", command]).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        tmux.send_keys(id, &["Enter"]).await
    }

    /// Types a prompt in the agent's CLI (the Diff tab's review notes): pasted whole, then
    /// submitted with `enter`, otherwise left in its prompt to complete.
    pub async fn prompt(&self, id: &str, text: &str, enter: bool) -> Result<()> {
        if self.inner.registry.lock().unwrap().get(id).is_none() {
            bail!(tr("unknown agent"));
        }
        self.paste(id, text, enter).await
    }

    /// A quick answer from a phone (`agent_quick_answer`): the config's answer named `label`, filled
    /// with the agent's values here, sent only when it is the `text` the phone showed and the agent
    /// is at its prompt. No passkey: the text is the config's, not the phone's.
    pub async fn quick_answer(&self, id: &str, label: &str, text: &str) -> Result<()> {
        let (settings, language) = {
            let s = self.inner.settings.lock().unwrap();
            (s.quick_answers.clone(), s.prompt_language.clone())
        };
        ensure!(settings.enabled, "quick answers are off (Settings › Agents › Quick answers)");
        let agent = self.inner.hub.subscribe().borrow().agents.iter().find(|a| a.id == id).cloned().context(tr("unknown agent"))?;
        ensure!(quick::at_prompt(agent.state, agent.status), "{} is not at its prompt any more", agent.title);
        let q = settings.list(&language).into_iter().find(|q| q.label == label).with_context(|| tr!("no quick answer “{label}” any more", label = label))?;
        let github = prism_core::conventions::get().forge == forge::Provider::Github;
        let sent = quick::fill(&q.text, &quick::Values::of(&agent, github)).context(tr("this answer is not offered to this agent"))?;
        ensure!(sent == text, "“{label}” changed meanwhile: try again");
        self.paste(id, &sent, q.submit).await
    }

    /// The prompt Claude Code suggests, sent (`agent_send_suggestion`): read again off its screen
    /// and sent only when it is still `text` and the agent at its prompt. No passkey from a phone:
    /// the text is the agent's own suggestion, not the phone's.
    pub async fn send_suggestion(&self, id: &str, text: &str) -> Result<()> {
        let agent = self.inner.hub.subscribe().borrow().agents.iter().find(|a| a.id == id).cloned().context(tr("unknown agent"))?;
        ensure!(quick::at_prompt(agent.state, agent.status), "{} is not at its prompt any more", agent.title);
        let now = suggestion::read(&self.inner.tmux.screen(id).await?);
        ensure!(now.as_deref() == Some(text), "its suggestion changed meanwhile");
        self.paste(id, text, true).await
    }

    /// A text typed from a phone's answer box (`agent_type`, the passkey's): to an agent, or to
    /// one of its shells (`shell:<agent>:<n>`), submitted.
    pub async fn type_text(&self, id: &str, text: &str) -> Result<()> {
        self.phone_key(id)?;
        self.paste(id, text, true).await
    }

    pub(crate) async fn paste(&self, id: &str, text: &str, enter: bool) -> Result<()> {
        let tmux = &self.inner.tmux;
        tmux.leave_copy_mode(id).await?;
        // A prompt ending on a mention (an attachment's `@path`) opens the CLI's file suggestions,
        // where Enter picks one instead of submitting: a space after it closes them (trimmed at submit).
        let text = if enter && text.contains('@') && !text.ends_with(char::is_whitespace) { &format!("{text} ") } else { text };
        tmux.paste(id, text).await?;
        if enter {
            // The CLI takes the pasted block in first, then the key that submits it: a long one
            // takes it longer.
            tokio::time::sleep(Duration::from_millis(250 + (text.len() as u64 / 200).min(1750))).await;
            tmux.send_keys(id, &["Enter"]).await?;
        }
        Ok(())
    }

    /// One key of an agent's menu, pressed (not pasted): a choice's digit, Enter, Esc, the arrows,
    /// Tab. Nothing else: the phone's answers, which type no text this way. In one of its shells,
    /// Ctrl-C too (to an agent, twice, it quits).
    pub async fn key(&self, id: &str, key: &str) -> Result<()> {
        const KEYS: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9", "Enter", "Escape", "Up", "Down", "Left", "Right", "Tab", "BTab"];
        if !KEYS.contains(&key) && !(key == "C-c" && id.starts_with(tmux::SHELL)) {
            bail!("not a menu key: {key}");
        }
        self.phone_key(id)?;
        // Its CLI's question before its session (its folder's trust): only from its card, read on its screen.
        if self.gate(id).is_some() && !matches!(key, "Up" | "Down" | "Left" | "Right" | "Tab" | "BTab") {
            bail!(tr("it asks before it starts: answer from its card"));
        }
        // A choice, Enter or Esc on its permission's menu answers it; the arrows only move in it.
        let answers = !matches!(key, "Up" | "Down" | "Left" | "Right" | "Tab" | "BTab" | "C-c") && self.inner.folds.lock().unwrap().get(id).is_some_and(|f| f.status == SessionStatus::WaitingTool);
        self.inner.tmux.leave_copy_mode(id).await?;
        self.inner.tmux.send_keys(id, &[key]).await?;
        if answers {
            self.permission_answered(id);
        }
        Ok(())
    }

    /// Answers the question the agent asks (`call`, its `AskUserQuestion`), as its menu's keys:
    /// the choices picked and your words, checked against the question its transcript holds,
    /// still unanswered.
    pub async fn answer(&self, id: &str, call: &str, answers: &[question::Answer]) -> Result<()> {
        let convo = self.conversation(id, 3, &[]).await?.context(tr("no conversation to read"))?;
        let ask = convo
            .turns
            .iter()
            .flat_map(|t| &t.entries)
            .find_map(|e| match e {
                conversation::Entry::Tool(t) if t.state == "running" => t.ask.as_ref().filter(|a| a.call == call),
                _ => None,
            })
            .context(tr("this question is no longer asked"))?;
        let menu = if self.provider(id)? == Provider::CommandCode { question::Menu::CommandCode } else { question::Menu::Claude };
        let steps = ask.keys(answers, menu)?;
        let tmux = &self.inner.tmux;
        tmux.leave_copy_mode(id).await?;
        ensure!(ask.on_screen(&tmux.screen(id).await?), "its question's menu is not on its screen: answer it in the terminal");
        self.steps(id, steps).await?;
        // Sent is not taken: its menu gone says so, a menu still open is said rather than "sent".
        for _ in 0..12 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if !ask.on_screen(&tmux.screen(id).await?) {
                return Ok(());
            }
        }
        tracing::warn!("agents: {id}: its question's menu still open after the keys of its answer");
        bail!(tr("The agent did not take the answer: its menu is still open, answer it in the terminal"))
    }

    /// Answers the plan the agent proposes (`call`, its `ExitPlanMode`) as its menu's keys:
    /// approved, or refused or approved with your words; only while that plan waits for its answer
    /// and its menu is on its screen.
    pub async fn answer_plan(&self, id: &str, call: &str, how: plan::How, text: Option<&str>) -> Result<()> {
        let convo = self.conversation(id, 3, &[]).await?.context(tr("no conversation to read"))?;
        let waiting = convo.turns.iter().flat_map(|t| &t.entries).any(|e| matches!(e, conversation::Entry::Tool(t) if t.proposal.as_ref().is_some_and(|p| p.call == call && p.verdict == "waiting")));
        ensure!(waiting, "this plan is no longer waiting for an answer");
        let tmux = &self.inner.tmux;
        tmux.leave_copy_mode(id).await?;
        let steps = plan::keys(&tmux.screen(id).await?, how, text)?;
        self.steps(id, steps).await
    }

    /// Keys and texts for its menu, one at a time: it redraws between keys, a pasted text taken in
    /// before the next.
    async fn steps(&self, id: &str, steps: Vec<question::Step>) -> Result<()> {
        let tmux = &self.inner.tmux;
        for step in steps {
            match step {
                question::Step::Key(k) => {
                    tmux.send_keys(id, &[k]).await?;
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
                question::Step::Type(t) => {
                    tmux.send_keys(id, &["-l", &t]).await?;
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
                question::Step::Paste(t) => {
                    tmux.paste(id, &t).await?;
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            }
        }
        Ok(())
    }

    /// Interrupts the agent's CLI like Esc on the keyboard: the current turn stops, the session stays.
    pub async fn interrupt(&self, id: &str) -> Result<()> {
        let provider = self.provider(id)?;
        self.inner.tmux.send_keys(id, provider.interrupt_keys()).await
    }

    /// Also attaches the agent in an external terminal (`app`, see `platform::run_in_terminal`).
    pub async fn open_external(&self, id: &str, app: &str) -> Result<()> {
        if self.inner.registry.lock().unwrap().get(id).is_none() {
            bail!(tr("unknown agent"));
        }
        if self.inner.tmux.is_local() {
            bail!(tr("on this computer, an agent's terminal opens in Prism only"));
        }
        let argv = self.inner.tmux.attach_argv(id);
        crate::platform::run_in_terminal(app, &argv, &self.inner.work, &format!("attach-{id}")).await
    }
}
