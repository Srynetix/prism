//! Another CLI takes an agent on (Continue with…): same agent, folder, branch, ticket and links,
//! its former CLI stopped and the new one started there with a relay, the words it starts from.
//! No CLI resumes another's conversation: the relay is what passes, built by Prism from the
//! transcript and git (`material`), written by the former agent itself, or the task only — the
//! window writes it, in the prompts' language, and shows it before it goes.

use super::*;

/// The turns a relay recalls at most, and the characters kept of a prompt and of a reply.
const MAX_TURNS: u32 = 30;
const PROMPT_CHARS: usize = 600;
const REPLY_CHARS: usize = 900;
/// The last reply in full (the former agent's own summary), up to this.
const SUMMARY_CHARS: usize = 12_000;
/// `git status` and `git diff --stat`, their first lines.
const GIT_LINES: usize = 40;

/// What a relay is written from: the agent's task, its plan, its last turns, its branch's state.
#[derive(Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "HandoffMaterial"))]
pub struct Material {
    pub provider: Provider,
    pub title: String,
    /// Its first prompt.
    pub prompt: Option<String>,
    /// Its plan as last written (its CLI's to-do list), none when it keeps none.
    pub plan: Vec<conversation::PlanStep>,
    /// Its last turns, oldest first: your words and the end of its answer, each cut.
    pub turns: Vec<RelayTurn>,
    /// Its last answer whole: the summary it was asked for (relay written by the agent).
    pub last_reply: Option<String>,
    /// Its conversation could be read (not OpenCode's): turns, plan and last answer are known.
    pub readable: bool,
    pub branch: Option<String>,
    /// The branch it started from, as its worktree noted it (`origin/…`).
    pub base: Option<String>,
    /// Its commits since its base, newest first (`git log --oneline`).
    pub commits: Vec<String>,
    /// Files changed since its base, committed or not (`git diff --stat`), and what is not
    /// committed (`git status --short`); their first lines.
    pub diffstat: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "HandoffTurn"))]
pub struct RelayTurn {
    pub prompt: String,
    pub reply: String,
}

/// What the window asks: the CLI to continue with, the relay as it will be sent, and how.
#[derive(Debug, Deserialize)]
pub struct Switch {
    pub to: Provider,
    pub message: String,
    /// `prism`, `agent` or `task`: how the relay was written.
    pub handoff: String,
    /// `limit` (its plan used up) or `choice`.
    #[serde(default)]
    pub reason: Option<String>,
    /// The new CLI's model, effort and permission mode; absent, its own (`[agents.<cli>]`).
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
}

/// The first `n` characters, `…` when cut.
fn cut(text: &str, n: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(n) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text.to_string(),
    }
}

/// The end of a turn's answer: its last words written (not a tool's).
fn reply_of(t: &conversation::Turn) -> Option<&str> {
    t.entries.iter().rev().find_map(|e| match e {
        conversation::Entry::Text { text } if !text.trim().is_empty() => Some(text.as_str()),
        _ => None,
    })
}

/// The first `n` lines, and how many more.
fn head(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = lines.iter().take(n).copied().collect::<Vec<_>>().join("\n");
    if lines.len() > n {
        out.push_str(&format!("\n… +{}", lines.len() - n));
    }
    out
}

impl Agents {
    /// The CLIs found on this computer: their executable (`[agents.<cli>] bin`, or in the PATH).
    pub fn installed(&self) -> Vec<Provider> {
        Provider::ALL.into_iter().filter(|p| self.has_cli(*p)).collect()
    }

    fn has_cli(&self, provider: Provider) -> bool {
        let bin = self.inner.settings.lock().unwrap().cli(provider).bin.clone();
        match bin.map(PathBuf::from) {
            Some(b) if b.components().count() > 1 => config::expand_home(&b).is_file(),
            Some(b) => prism_core::detect::which(&b.to_string_lossy()).is_some(),
            None => prism_core::detect::which(provider.program()).is_some(),
        }
    }

    /// What a relay is written from (`Material`), its last `turns` turns recalled.
    pub async fn handoff_material(&self, id: &str, turns: u32) -> Result<Material> {
        let record = self.record(id)?;
        let dir = self.dir(id)?;
        let transcript = self.transcript(id)?;
        let mut m = Material { provider: record.provider, title: record.title.clone(), prompt: record.prompt.clone(), readable: transcript.is_some(), ..Default::default() };
        if let Some(t) = transcript {
            let (cwd, n) = (dir.clone(), turns.clamp(1, MAX_TURNS) as usize);
            let c = tokio::task::spawn_blocking(move || conversation::read(&t, Some(&cwd), n)).await?;
            m.plan = c.plan;
            m.last_reply = c.turns.iter().rev().find_map(reply_of).map(|r| cut(r, SUMMARY_CHARS));
            m.turns = c.turns.iter().filter(|t| !t.prompt.trim().is_empty() || reply_of(t).is_some()).map(|t| RelayTurn { prompt: cut(&t.prompt, PROMPT_CHARS), reply: reply_of(t).map(|r| cut(r, REPLY_CHARS)).unwrap_or_default() }).collect();
        }
        m.branch = git_branch(&dir);
        let noted = match &m.branch {
            Some(b) => git::run(&dir, &["config", "--get", &format!("branch.{b}.{}", collectors::worktrees::BASE_KEY)]).await.ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            None => None,
        };
        let base = match noted {
            Some(b) => Some(if b.starts_with("origin/") { b } else { format!("origin/{b}") }),
            None => git::run(&dir, &["rev-parse", "--abbrev-ref", "origin/HEAD"]).await.ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        };
        let fork = match &base {
            Some(b) => git::run(&dir, &["merge-base", "HEAD", b]).await.ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            None => None,
        };
        if let Some(fork) = &fork {
            m.commits = git::run(&dir, &["log", "--oneline", "--no-decorate", "-n", "30", &format!("{fork}..HEAD")]).await.map(|s| s.lines().map(String::from).collect()).unwrap_or_default();
            m.diffstat = git::run(&dir, &["diff", "--stat=100", fork]).await.map(|s| head(s.trim_end(), GIT_LINES)).unwrap_or_default();
        }
        m.status = git::run(&dir, &["status", "--short"]).await.map(|s| head(s.trim_end(), GIT_LINES)).unwrap_or_default();
        m.base = base;
        Ok(m)
    }

    /// Hands the agent over to another CLI: its current one stopped (in place: the terminal stays
    /// attached), the new one started in its folder with `message`; the stretch just over kept with
    /// its session, conversation and figures. Back on a CLI it ran before, Claude Code and Codex
    /// pick their conversation up again with the relay as their next prompt; the others start a new one.
    pub async fn switch(&self, id: &str, s: Switch) -> Result<()> {
        let record = self.record(id)?;
        if record.prepared {
            bail!(tr("this agent has not started yet: Start it"));
        }
        ensure!(s.to != record.provider, "{}", tr!("it already runs {cli}", cli = s.to.program()));
        let message = s.message.trim();
        ensure!(!message.is_empty(), "{}", tr("nothing to tell the next agent"));
        ensure!(matches!(s.handoff.as_str(), "prism" | "agent" | "task"), "unknown relay {}", s.handoff);
        let reason = s.reason.as_deref().unwrap_or("choice");
        ensure!(matches!(reason, "limit" | "choice"), "unknown reason {reason}");
        let pick = |v: &Option<String>| v.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(String::from);
        let (model, effort, mode) = (pick(&s.model), pick(&s.effort), pick(&s.permission_mode));
        if let Some(mode) = mode.as_deref().filter(|m| *m != "default") {
            s.to.permission_mode(mode)?;
        }
        if let Some(m) = model.as_deref().filter(|m| !prism_core::agents::models::valid(m)) {
            bail!(tr!("{m} is not a model name", m = m));
        }
        if let Some(e) = effort.as_deref().filter(|e| !prism_core::agents::models::valid_effort(e)) {
            bail!(tr!("{e} is not an effort level", e = e));
        }
        // Not found: the former CLI keeps the agent, rather than a terminal saying "not found".
        ensure!(self.has_cli(s.to), "{}", tr!("{program} not found in the PATH", program = s.to.program()));
        let extra: Vec<PathBuf> = record.extra.iter().filter(|p| p.is_dir()).cloned().collect();
        if !extra.is_empty() && !s.to.takes_dirs() {
            bail!(tr!("{cli} works in one folder only: launch it in a single repository", cli = s.to.program()));
        }
        let cwd = self.dir(id)?;
        ensure!(cwd.is_dir(), "{}", tr!("{path} is not a directory", path = cwd.display()));

        // The stretch now over, as the list shows it.
        let shown = self.inner.hub.subscribe().borrow().agents.iter().find(|a| a.id == id).cloned();
        let (session, transcript) = self.inner.folds.lock().unwrap().get(id).map(|f| (f.session.clone(), f.transcript.clone())).unwrap_or_default();
        let session = session.or_else(|| (record.provider == Provider::Claude).then(|| record.id.clone()));
        let leg = state::AgentLeg {
            provider: record.provider,
            session,
            transcript: transcript.or_else(|| self.transcript(id).ok().flatten().map(|p| p.display().to_string())),
            model: shown.as_ref().and_then(|a| a.model.clone()),
            started: record.legs.last().map_or(record.created_at, |l| l.ended),
            ended: Utc::now(),
            usage: shown.and_then(|a| a.usage),
            reason: reason.into(),
            handoff: s.handoff.clone(),
            message: message.to_string(),
        };

        // Its spec, for the new CLI: kept on resume.
        let mut spec = record.launch.clone().unwrap_or_default();
        (spec.provider, spec.model, spec.effort, spec.permission_mode) = (Some(s.to), model, effort, mode.clone());
        // Back on a CLI it ran before: its conversation, when that CLI takes a prompt on resume.
        let earlier = record.legs.iter().rev().find(|l| l.provider == s.to).and_then(|l| l.session.clone());
        let wiring = self.wiring(s.to);
        let (argv, resumed) = match earlier.as_deref().and_then(|e| s.to.resume_with(e, &wiring, mode.as_deref(), message)) {
            Some(argv) => (argv, earlier.clone()),
            // Claude Code takes the agent's id as its session: one already written under it is resumed.
            None if s.to == Provider::Claude && claude_conversation_exists(id) => (s.to.resume_with(id, &wiring, mode.as_deref(), message).context("resume")?, Some(id.to_string())),
            None => (s.to.launch(id, &record.title, &wiring, mode.as_deref(), None, Some(message))?, None),
        };
        let argv = s.to.with_dirs(s.to.with_cli(argv, &self.cli(s.to, Some(&spec))), &extra);

        // The log says it first: what the new CLI reports comes after, the former one's session forgotten.
        let line = hook::Line { t: Utc::now().timestamp_millis(), event: status::SWITCH.into(), what: Some(s.to.program().into()), ..Default::default() };
        hook::append(&self.inner.events, id, &line)?;
        let env = self.env(id, s.to, mode.as_deref());
        match self.inner.tmux.panes().await?.get(id) {
            Some(_) => self.inner.tmux.respawn(id, &cwd, &env, &argv).await?,
            None => self.inner.tmux.new_session(id, &cwd, &env, &argv).await?,
        }
        self.inner.usages.lock().unwrap().remove(id);
        let to = s.to;
        self.update_registry(|reg| {
            if let Some(r) = reg.records.iter_mut().find(|r| r.id == id) {
                r.provider = to;
                r.launch = Some(spec);
                r.legs.push(leg);
                r.resumed = resumed;
            }
        })?;
        tracing::info!("agents: {id} continued with {}", to.program());
        Ok(())
    }

    /// The conversation of its stretch `leg` (`Agent::legs`), its last `turns`: the Chat's earlier turns.
    pub async fn leg_conversation(&self, id: &str, leg: usize, turns: u32) -> Result<Option<conversation::Conversation>> {
        let record = self.record(id)?;
        let l = record.legs.get(leg).context("no such stretch")?;
        let Some(t) = l.transcript.clone().map(PathBuf::from).filter(|t| t.is_file()) else { return Ok(None) };
        let cwd = self.dir(id).ok();
        let turns = turns.clamp(1, 5000) as usize;
        Ok(Some(tokio::task::spawn_blocking(move || conversation::read(&t, cwd.as_deref(), turns)).await?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_and_heads() {
        assert_eq!(cut("  abc  ", 5), "abc");
        assert_eq!(cut("abcdef", 3), "abc…");
        assert_eq!(cut("éèêë", 2), "éè…");
        assert_eq!(head("a\nb\nc", 2), "a\nb\n… +1");
        assert_eq!(head("a", 2), "a");
    }
}
