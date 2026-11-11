//! « In passing » (`[agents.aside]`): what an agent flags out of its task while it works, with
//! `suggest_task` (Prism's MCP server, `prism_commands::mcp`) or, in a Claude Desktop session,
//! `spawn_task`. One line per subject (`asides.json`): the same repository, files that meet and
//! close titles make one, "seen by" two agents. A subject dealt with (launched, a ticket,
//! dismissed) is not proposed again: the agent is told. What becomes of a proposal reaches its
//! agent at its next tool call (`news`, read by its hook or its next MCP call).

use super::*;
use crate::state::{Aside, AsideSeen, AsideState};
use serde_json::{json, Value};

/// What an agent proposes (`suggest_task`'s arguments).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Suggest {
    pub title: String,
    pub tldr: String,
    pub prompt: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub needs_current_branch: bool,
}

/// The list, as kept.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Book {
    next: u64,
    items: Vec<Aside>,
    /// Lines a ticket is being created for: not twice (the computer and a phone at once).
    #[serde(skip)]
    ticketing: std::collections::HashSet<u64>,
}

/// The longest a title is kept (Desktop's rule: under 60 characters, said to the agent).
const TITLE_CHARS: usize = 80;
/// A prompt kept whole up to this: a proposal is no place for a file.
const PROMPT_CHARS: usize = 8000;

impl Book {
    pub(crate) fn load(dir: &Path) -> Self {
        std::fs::read(dir.join("asides.json")).ok().and_then(|raw| serde_json::from_slice(&raw).map_err(|e| tracing::warn!("asides.json: {e}")).ok()).unwrap_or_default()
    }

    /// The owner's only: it holds the agents' prompts, branches and folders.
    fn write(&self, dir: &Path) {
        let path = dir.join("asides.json");
        let done = serde_json::to_vec_pretty(self).map_err(anyhow::Error::from).and_then(|raw| Ok(prism_base::files::write_private(&path, raw)?));
        if let Err(e) = done {
            tracing::warn!("agents: writing {}: {e:#}", path.display());
        }
    }

    /// The line `s` is about in `repo`, if one is (`similar`).
    fn find(&self, repo: Option<&str>, title: &str, files: &[String]) -> Option<usize> {
        self.items.iter().position(|a| a.repo.as_deref() == repo && similar(&a.title, &a.files, title, files))
    }
}

/// Two subjects are one: their files meet and their titles are close, or, one of them naming
/// no file, their titles are closer still.
fn similar(a_title: &str, a_files: &[String], b_title: &str, b_files: &[String]) -> bool {
    let close = jaccard(&words(a_title), &words(b_title));
    let norm = |f: &String| f.trim_start_matches("./").to_string();
    if a_files.is_empty() || b_files.is_empty() {
        return close >= 0.6;
    }
    let meet = a_files.iter().map(norm).any(|f| b_files.iter().map(norm).any(|g| g == f));
    meet && close >= 0.34
}

/// A title's words that tell: lower case, letters and digits, the short and common ones left out.
fn words(text: &str) -> Vec<String> {
    const COMMON: &[&str] = &["the", "and", "for", "from", "with", "into", "that", "this", "les", "des", "une", "pour", "dans", "avec", "qui", "est"];
    let mut w: Vec<String> = text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() > 2 && !COMMON.contains(w)).map(|w| w.trim_end_matches('s').to_string()).collect();
    w.sort();
    w.dedup();
    w
}

fn jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.iter().filter(|w| b.contains(w)).count() as f64;
    shared / ((a.len() + b.len()) as f64 - shared)
}

fn cut(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max).collect::<String>().trim_end())
}

/// A CLI's name, as `AsideSeen::cli` says it.
fn cli_name(p: Provider) -> String {
    serde_json::to_value(p).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "agent".into())
}

impl Agents {
    fn asides(&self) -> std::sync::MutexGuard<'_, Book> {
        self.inner.asides.lock().unwrap()
    }

    /// The list written and given to the snapshot, the newest first.
    fn publish_asides(&self, book: &Book) {
        book.write(self.inner.work.parent().unwrap_or(&self.inner.work));
        let mut items = book.items.clone();
        items.sort_by_key(|a| std::cmp::Reverse(a.at));
        self.inner.hub.update(|s| {
            let changed = s.asides != items;
            s.asides = items;
            changed
        });
    }

    /// The snapshot told what is kept (at start).
    pub(crate) fn publish_asides_now(&self) {
        let book = self.asides();
        self.publish_asides(&book);
    }

    /// The repository of a folder: a worktree's, or the clone it is in.
    fn repo_of_dir(&self, dir: &str) -> Option<String> {
        let snap = self.inner.hub.subscribe().borrow().clone();
        if let Some(w) = snap.worktrees.iter().find(|w| w.path == dir) {
            return Some(w.repo.clone());
        }
        let inside = |root: &str| dir == root || dir.starts_with(&format!("{root}/")) || dir.starts_with(&format!("{root}.worktrees/"));
        let mut repos: Vec<_> = snap.repos.iter().collect();
        repos.sort_by_key(|r| std::cmp::Reverse(r.path.len()));
        repos.into_iter().find(|r| inside(&r.path)).map(|r| r.name.clone())
    }

    /// `[agents.aside]`, as configured now.
    pub fn aside_settings(&self) -> config::AsideConfig {
        self.inner.settings.lock().unwrap().aside.clone()
    }

    /// An agent's `suggest_task`: a new line, or a sighting of one there already; what it is told.
    pub fn suggest(&self, agent: Option<&str>, client: &str, s: Suggest) -> Result<Value> {
        ensure!(self.aside_settings().enabled, "[agents.aside] enabled is off in this Prism");
        let title = cut(&s.title.split_whitespace().collect::<Vec<_>>().join(" "), TITLE_CHARS);
        ensure!(!title.is_empty(), "title: an imperative sentence under 60 characters");
        ensure!(!s.prompt.trim().is_empty(), "prompt: a self-contained prompt for the agent that will do it");
        let a = agent.and_then(|id| self.inner.hub.subscribe().borrow().agents.iter().find(|a| a.id == id).cloned());
        let dir = a.as_ref().map(|a| a.worktree.clone().unwrap_or_else(|| a.cwd.clone()));
        let repo = dir.as_deref().and_then(|d| self.repo_of_dir(d));
        let files: Vec<String> = s.files.iter().map(|f| f.trim().trim_start_matches("./").to_string()).filter(|f| !f.is_empty()).collect();
        let seen = AsideSeen {
            agent: agent.map(String::from),
            session: None,
            call: None,
            name: a.as_ref().map(|a| a.title.clone()).unwrap_or_else(|| client.to_string()),
            cli: a.as_ref().map(|a| cli_name(a.provider)).unwrap_or_else(|| client.to_string()),
            branch: a.as_ref().and_then(|a| a.branch.clone()),
            cwd: dir,
            effort: a.as_ref().and_then(|a| a.launch.as_ref()).and_then(|l| l.effort.clone()),
            tldr: cut(&s.tldr, 600),
            at: Utc::now(),
        };
        let mut book = self.asides();
        if let Some(i) = book.find(repo.as_deref(), &title, &files) {
            let line = &mut book.items[i];
            let id = line.id;
            let answer = match line.state {
                AsideState::Open | AsideState::Later => {
                    if !line.seen.iter().any(|x| x.agent.is_some() && x.agent == seen.agent) {
                        line.seen.push(seen);
                    }
                    for f in files {
                        if !line.files.contains(&f) {
                            line.files.push(f);
                        }
                    }
                    json!({ "task_id": id, "status": "open", "note": "already proposed (by you or another agent): one line in Prism, seen by each. Do not propose it again." })
                }
                state => json!({ "task_id": id, "already": already(state), "note": already_note(line) }),
            };
            self.publish_asides(&book);
            return Ok(answer);
        }
        book.next += 1;
        let id = book.next;
        book.items.push(Aside {
            id,
            repo,
            title,
            tldr: cut(&s.tldr, 600),
            prompt: cut(&s.prompt, PROMPT_CHARS),
            files,
            kind: s.kind.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()),
            needs_current_branch: s.needs_current_branch,
            seen: vec![seen],
            state: AsideState::Open,
            at: Utc::now(),
            changed_at: None,
            agent: None,
            agent_title: None,
            from_branch: None,
            ticket: None,
            dismissed_by: None,
            why: None,
            files_changed: false,
        });
        self.publish_asides(&book);
        Ok(json!({ "task_id": id, "status": "open", "note": "shown to the user in Prism; carry on with your own task. Call dismiss_task with this id if it stops being worth doing (you did it after all, or were wrong)." }))
    }

    /// An agent's `dismiss_task`: its proposal withdrawn (struck out in its Chat). Only one it saw.
    pub fn dismiss_by_agent(&self, agent: Option<&str>, id: u64, why: Option<String>) -> Result<Value> {
        let mut book = self.asides();
        let line = book.items.iter_mut().find(|a| a.id == id).with_context(|| format!("no task {id}"))?;
        // Only an agent that proposed it withdraws it; a client without an agent of Prism's, none.
        ensure!(agent.is_some() && line.seen.iter().any(|s| s.agent.as_deref() == agent), "task {id} was not proposed by you");
        if !line.state.open() {
            return Ok(json!({ "task_id": id, "already": already(line.state), "note": already_note(line) }));
        }
        // Seen by another agent too: only this one's sighting goes.
        if line.seen.len() > 1 {
            line.seen.retain(|s| s.agent.as_deref() != agent);
            self.publish_asides(&book);
            return Ok(json!({ "task_id": id, "status": "withdrawn", "note": "withdrawn for you; another agent proposed it too, it stays for them" }));
        }
        line.state = AsideState::Dismissed;
        line.dismissed_by = Some("agent".into());
        line.why = why.map(|w| cut(&w, 300)).filter(|w| !w.is_empty());
        line.changed_at = Some(Utc::now());
        self.publish_asides(&book);
        Ok(json!({ "task_id": id, "status": "dismissed" }))
    }

    /// One line, as it is now.
    pub fn aside(&self, id: u64) -> Option<Aside> {
        self.asides().items.iter().find(|a| a.id == id).cloned()
    }

    /// The user's answer from the window (or a phone): `later`, `reopen`, `dismiss` (with a reason,
    /// told to the agents that saw it), `launched` (the new agent's id), `ticket` (its key),
    /// `forget` (a line dealt with, out of the list).
    pub fn aside_action(&self, id: u64, action: &str, why: Option<String>, target: Option<String>) -> Result<()> {
        let mut book = self.asides();
        let line = book.items.iter_mut().find(|a| a.id == id).with_context(|| tr("this proposal is gone"))?;
        let now = Some(Utc::now());
        let mut news: Option<String> = None;
        // What each answer may follow: an action on a line dealt with meanwhile (another device) is refused.
        let allowed = match action {
            "later" => line.state == AsideState::Open,
            "reopen" => matches!(line.state, AsideState::Dismissed | AsideState::Then),
            "dismiss" => line.state.open() || line.state == AsideState::Then,
            "launched" => line.state.open() || line.state == AsideState::Then,
            "ticket" => line.state.open(),
            _ => true,
        };
        ensure!(allowed, "{}", tr("this proposal was dealt with already"));
        match action {
            "later" => line.state = AsideState::Later,
            "reopen" => {
                line.state = AsideState::Open;
                (line.dismissed_by, line.why, line.agent, line.agent_title, line.ticket, line.from_branch) = (None, None, None, None, None, None);
            }
            "dismiss" => {
                line.state = AsideState::Dismissed;
                line.dismissed_by = Some("user".into());
                line.why = why.map(|w| cut(&w, 300)).filter(|w| !w.is_empty());
                news = Some(match &line.why {
                    Some(w) => format!("the user dismissed it: {w}. Do not propose it again."),
                    None => "the user dismissed it. Do not propose it again.".into(),
                });
            }
            "launched" => {
                let agent = target.context("the new agent")?;
                let title = self.inner.hub.subscribe().borrow().agents.iter().find(|a| a.id == agent).map(|a| a.title.clone());
                line.state = AsideState::Launched;
                news = Some(format!("the user launched it in another agent ({}): leave it to that agent, and avoid its files: {}.", title.as_deref().unwrap_or(&agent), if line.files.is_empty() { "none named".to_string() } else { line.files.join(", ") }));
                (line.agent, line.agent_title) = (Some(agent), title);
            }
            "ticket" => {
                line.state = AsideState::Ticket;
                line.ticket = target.clone();
                news = Some(format!("the user made it a ticket ({}): it will be done later, not by you.", target.unwrap_or_default()));
            }
            "forget" => {
                ensure!(!line.state.open(), "{}", tr("only a proposal dealt with leaves the list"));
                book.items.retain(|a| a.id != id);
                self.publish_asides(&book);
                return Ok(());
            }
            other => bail!("no action {other}"),
        }
        line.changed_at = now;
        let (title, told) = (line.title.clone(), line.seen.iter().filter_map(|s| s.agent.clone()).collect::<Vec<_>>());
        self.publish_asides(&book);
        drop(book);
        if let Some(n) = news {
            for agent in told {
                self.tell(&agent, &format!("Task #{id} you proposed in passing (\"{title}\"): {n}"));
            }
        }
        Ok(())
    }

    /// A ticket about to be created for it: the line as it is, if it is still open and no ticket is
    /// being created for it already. `end_ticket` once done, whatever came of it.
    pub fn begin_ticket(&self, id: u64) -> Result<Aside> {
        let mut book = self.asides();
        let line = book.items.iter().find(|a| a.id == id).cloned().with_context(|| tr("this proposal is gone"))?;
        ensure!(line.state.open() && book.ticketing.insert(id), "{}", tr("this proposal was dealt with already"));
        Ok(line)
    }

    pub fn end_ticket(&self, id: u64) {
        self.asides().ticketing.remove(&id);
    }

    /// Made a ticket (`key`): its agents are told.
    pub fn aside_ticket(&self, id: u64, key: &str) -> Result<()> {
        self.aside_action(id, "ticket", None, Some(key.to_string()))
    }

    /// "After this agent": launched when the agent that saw it is done (a "Then" step of its chain),
    /// from its branch as it is then (`from_branch`) or from the repository's base.
    pub fn aside_then(&self, id: u64, from_branch: bool) -> Result<()> {
        let (seen, title) = {
            let book = self.asides();
            let line = book.items.iter().find(|a| a.id == id).with_context(|| tr("this proposal is gone"))?;
            ensure!(line.state.open(), "{}", tr("this proposal was dealt with already"));
            ensure!(line.seen.iter().any(|s| s.agent.is_some()), "{}", tr("no agent of Prism's saw it: launch it instead"));
            (line.seen.clone(), line.title.clone())
        };
        // The last one that saw it and is still there, as its card shows it.
        let a = {
            let snap = self.inner.hub.subscribe();
            let agents = &snap.borrow().agents;
            seen.iter().rev().filter_map(|s| s.agent.as_deref()).find_map(|id| agents.iter().find(|a| a.id == id).cloned())
        };
        let a = a.context(tr("its agent is closed: launch it instead"))?;
        let agent = a.id.clone();
        let step = state::LaunchStep { kind: state::StepKind::Aside, when: state::StepWhen::Done, command: None, text: Some(id.to_string()) };
        self.add_step(&a, step);
        {
            let mut book = self.asides();
            if let Some(line) = book.items.iter_mut().find(|x| x.id == id) {
                line.state = AsideState::Then;
                line.from_branch = Some(from_branch);
                line.changed_at = Some(Utc::now());
            }
            self.publish_asides(&book);
        }
        self.tell(&agent, &format!("Task #{id} you proposed in passing (\"{title}\"): the user chained it after you; another agent does it once you are done. Do not do it yourself."));
        Ok(())
    }

    /// The launch of a proposal: its prompt, headed by where it comes from; a new worktree (from
    /// the agent's branch, or the base), or its agent's folder; its agent's CLI and effort, its kind.
    pub fn aside_launch(&self, id: u64, from_branch: bool) -> Result<Launch> {
        let line = self.aside(id).context(tr("this proposal is gone"))?;
        let seen = line.seen.iter().rev().find(|s| s.agent.is_some()).or(line.seen.last()).cloned();
        let a = seen.as_ref().and_then(|s| s.agent.as_deref()).and_then(|id| self.inner.hub.subscribe().borrow().agents.iter().find(|a| a.id == id).cloned());
        let by = seen.as_ref().map(|s| s.name.clone()).unwrap_or_default();
        let header = if by.is_empty() { "Spotted in passing by another agent.".to_string() } else { format!("Spotted in passing by the agent \"{by}\", while it worked on something else.") };
        let files = if line.files.is_empty() { String::new() } else { format!("\n\nFiles concerned: {}", line.files.join(", ")) };
        let prompt = format!("{header}\n\n{}{files}", line.prompt);
        let cfg = self.aside_settings();
        let branch = seen.as_ref().and_then(|s| s.branch.clone());
        let worktree = (cfg.worktree && line.repo.is_some()).then(|| NewWorktree { repo: line.repo.clone(), name: None, branch: None, base: if from_branch { branch.clone() } else { None }, also: Vec::new() });
        let cwd = if worktree.is_some() { None } else { seen.as_ref().and_then(|s| s.cwd.clone()).map(PathBuf::from) };
        Ok(Launch {
            title: line.title.clone(),
            cwd,
            prompt: Some(prompt),
            worktree,
            permission_mode: None,
            category: line.kind.clone(),
            tags: Vec::new(),
            provider: a.as_ref().map(|a| a.provider),
            // The account of the agent that saw it, as its CLI.
            account: a.as_ref().and_then(|a| a.account.clone()),
            model: a.as_ref().and_then(|a| a.launch.as_ref()).and_then(|l| l.model.clone()),
            effort: seen.and_then(|s| s.effort),
            then: Vec::new(),
            from: Some(format!("aside:{id}")),
            prism_mcp: false,
        })
    }

    /// `aside_launch` as the launcher is given it.
    pub fn aside_launch_spec(&self, id: u64, from_branch: bool) -> Result<state::LaunchSpec> {
        Ok(self.aside_launch(id, from_branch)?.spec())
    }

    /// A "Then" step of `aside` kind: the proposal launched now, its agent being done.
    /// Its title once launched; `None` when it was dealt with meanwhile (dismissed, made a ticket,
    /// reopened, gone): the step is skipped, not failed.
    pub(crate) async fn run_aside_step(&self, text: &str) -> Result<Option<String>> {
        let id: u64 = text.trim().parse().context("the proposal's id")?;
        let Some(line) = self.aside(id).filter(|l| l.state == AsideState::Then) else { return Ok(None) };
        let launch = self.aside_launch(id, line.from_branch.unwrap_or(false))?;
        let title = launch.title.clone();
        // `spawn` marks it launched (`from`).
        self.spawn(launch).await?;
        Ok(Some(title))
    }

    /// `text` for the agent at its next tool call: its hook (Claude Code, Codex) or its next call
    /// of Prism's MCP server, whichever comes first, takes it.
    pub fn tell(&self, agent: &str, text: &str) {
        if !valid_agent_id(agent) {
            return;
        }
        let path = news_path(&self.inner.events, agent);
        let _ = std::fs::create_dir_all(&self.inner.events);
        let mut open = std::fs::OpenOptions::new();
        open.create(true).append(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
        let done = open.open(&path).and_then(|mut f| std::io::Write::write_all(&mut f, format!("{}\n", text.replace('\n', " ")).as_bytes()));
        if let Err(e) = done {
            tracing::warn!("agents: news for {agent}: {e}");
        }
    }

    /// What was told to `agent` since, taken (its MCP server's next answer carries it).
    pub fn take_news(&self, agent: &str) -> Vec<String> {
        take_news(&self.inner.events, agent)
    }

    /// The `spawn_task` / `dismiss_task` calls of a Claude Desktop session (`[agents.aside] desktop`),
    /// read from its transcript: lines seen by it, read only (its own chip stays Desktop's).
    pub fn desktop_asides(&self, session: &ClaudeSession, calls: Vec<DesktopCall>) {
        if calls.is_empty() || !self.aside_settings().desktop {
            return;
        }
        let repo = session.path.as_deref().and_then(|p| self.repo_of_dir(p));
        let mut book = self.asides();
        let mut changed = false;
        for call in calls {
            let known = |a: &Aside| a.seen.iter().any(|s| s.session.as_deref() == Some(&session.id) && (s.call.as_deref() == Some(&call.call) || call.task.as_deref().is_some_and(|t| s.call.as_deref() == Some(t))));
            match call.kind {
                DesktopKind::Spawn { title, tldr, prompt } => {
                    if book.items.iter().any(known) {
                        continue;
                    }
                    let seen = AsideSeen { agent: None, session: Some(session.id.clone()), call: Some(call.task.clone().unwrap_or(call.call.clone())), name: session.title.clone(), cli: "desktop".into(), branch: session.branch.clone(), cwd: session.path.clone(), effort: None, tldr: cut(&tldr, 600), at: call.at };
                    let title = cut(&title, TITLE_CHARS);
                    if let Some(i) = book.find(repo.as_deref(), &title, &[]) {
                        book.items[i].seen.push(seen);
                    } else {
                        book.next += 1;
                        let id = book.next;
                        book.items.push(Aside { id, repo: repo.clone(), title, tldr: cut(&tldr, 600), prompt: cut(&prompt, PROMPT_CHARS), files: Vec::new(), kind: None, needs_current_branch: false, seen: vec![seen], state: AsideState::Open, at: call.at, changed_at: None, agent: None, agent_title: None, from_branch: None, ticket: None, dismissed_by: None, why: None, files_changed: false });
                    }
                    changed = true;
                }
                DesktopKind::Answer { task } => {
                    for line in book.items.iter_mut() {
                        for s in line.seen.iter_mut().filter(|s| s.session.as_deref() == Some(&session.id) && s.call.as_deref() == Some(&call.call)) {
                            s.call = Some(task.clone());
                            changed = true;
                        }
                    }
                }
                DesktopKind::Dismiss { task } => {
                    let of = |a: &Aside| a.seen.iter().any(|s| s.session.as_deref() == Some(&session.id) && s.call.as_deref() == Some(&task));
                    if let Some(line) = book.items.iter_mut().find(|a| of(a) && a.state.open()) {
                        line.state = AsideState::Dismissed;
                        line.dismissed_by = Some("agent".into());
                        line.changed_at = Some(call.at);
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.publish_asides(&book);
        }
    }

    /// The open lines whose files a commit of another branch changed since they were seen
    /// ("files changed since"), checked in their repositories now and then.
    pub async fn check_aside_files(&self) {
        let open: Vec<Watched> = {
            let book = self.asides();
            book.items.iter().filter(|a| a.state.open() && !a.files_changed && !a.files.is_empty()).filter_map(|a| Some((a.id, a.repo.clone()?, a.files.clone(), a.seen.iter().filter_map(|s| s.branch.clone()).collect(), a.at))).collect()
        };
        if open.is_empty() {
            return;
        }
        let paths: HashMap<String, String> = self.inner.hub.subscribe().borrow().repos.iter().map(|r| (r.name.clone(), r.path.clone())).collect();
        let mut changed = Vec::new();
        for (id, repo, files, branches, at) in open {
            let Some(path) = paths.get(&repo).cloned() else { continue };
            let since = at.to_rfc3339();
            let found = tokio::task::spawn_blocking(move || {
                let mut args = vec!["log".to_string(), "--all".into(), format!("--since={since}"), "--format=%H".into()];
                // Its agents' own commits do not count: the others'. A branch deleted since is left
                // out (git would refuse the whole command).
                let repo = Path::new(&path);
                let exists = |b: &String| git::run_blocking(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{b}")]).is_ok();
                args.extend(branches.iter().filter(|b| exists(b)).map(|b| format!("^refs/heads/{b}")));
                args.push("--".into());
                args.extend(files);
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                git::run_blocking(Path::new(&path), &args).map(|out| !out.trim().is_empty()).unwrap_or(false)
            })
            .await
            .unwrap_or(false);
            if found {
                changed.push(id);
            }
        }
        if changed.is_empty() {
            return;
        }
        let mut book = self.asides();
        for line in book.items.iter_mut().filter(|a| changed.contains(&a.id)) {
            line.files_changed = true;
        }
        self.publish_asides(&book);
    }

    /// Reads the Desktop sessions' transcripts from where it stopped (`read`: each one's offset),
    /// for their `spawn_task` and `dismiss_task` calls.
    async fn scan_desktop(&self, read: &mut HashMap<String, u64>) {
        if !self.aside_settings().desktop {
            return;
        }
        let sessions: Vec<ClaudeSession> = self.inner.hub.subscribe().borrow().sessions.iter().filter(|s| s.origin == SessionOrigin::Desktop && s.transcript.is_some()).cloned().collect();
        for session in sessions {
            let Some(path) = session.transcript.clone() else { continue };
            let from = read.get(&path).copied().unwrap_or(0);
            // Its calls whose answer (the task's id) was not read yet: still `toolu_…`.
            let waiting: std::collections::HashSet<String> = self.asides().items.iter().flat_map(|a| a.seen.iter()).filter(|s| s.session.as_deref() == Some(&session.id)).filter_map(|s| s.call.clone()).filter(|c| c.starts_with("toolu_")).collect();
            let p = path.clone();
            let Ok(Ok((calls, to))) = tokio::task::spawn_blocking(move || desktop_calls(Path::new(&p), from, &waiting)).await else { continue };
            read.insert(path, to);
            self.desktop_asides(&session, calls);
        }
    }

    /// Checks the files of the open lines every few minutes, and the Desktop sessions' calls every
    /// half minute, while Prism runs.
    pub async fn run_asides(self) {
        self.publish_asides_now();
        let mut read: HashMap<String, u64> = HashMap::new();
        let mut n: u32 = 0;
        loop {
            self.scan_desktop(&mut read).await;
            if n % 10 == 9 {
                self.check_aside_files().await;
            }
            n = n.wrapping_add(1);
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    }
}

type DateTime = chrono::DateTime<Utc>;

/// An open line whose files are watched: its id, repository, files, its agents' branches, since when.
type Watched = (u64, String, Vec<String>, Vec<String>, DateTime);

/// A Claude Desktop session's call, from its transcript.
#[derive(Debug, Clone)]
pub struct DesktopCall {
    /// The call's id (`toolu_…`), and the task's id its result gave, if read.
    pub call: String,
    pub task: Option<String>,
    pub at: DateTime,
    pub kind: DesktopKind,
}

#[derive(Debug, Clone)]
pub enum DesktopKind {
    Spawn { title: String, tldr: String, prompt: String },
    Dismiss { task: String },
    /// A `spawn_task`'s answer read after its call (a later scan): its task's id, for `call`.
    Answer { task: String },
}

/// Claude Desktop's tools, as its sessions' transcripts name them.
const DESKTOP_SPAWN: &str = "mcp__ccd_session__spawn_task";
const DESKTOP_DISMISS: &str = "mcp__ccd_session__dismiss_task";

/// The `spawn_task` and `dismiss_task` calls of a transcript from byte `from` (a line cut at the end
/// waits for the next time), and where it stopped. Only those lines are parsed.
/// `waiting`: the calls of earlier scans whose answer was not read yet.
fn desktop_calls(path: &Path, from: u64, waiting: &std::collections::HashSet<String>) -> std::io::Result<(Vec<DesktopCall>, u64)> {
    use std::io::{BufRead, Seek};
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let from = if from > len { 0 } else { from };
    f.seek(std::io::SeekFrom::Start(from))?;
    let mut reader = std::io::BufReader::new(f);
    let (mut calls, mut at, mut line) = (Vec::<DesktopCall>::new(), from, String::new());
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 || !line.ends_with('\n') {
            break;
        }
        at += n as u64;
        let answers = line.contains("tool_result") && (!calls.is_empty() || waiting.iter().any(|w| line.contains(w.as_str())));
        if !line.contains("ccd_session__") && !answers {
            continue;
        }
        let Ok(r) = serde_json::from_str::<Value>(&line) else { continue };
        let when = r.get("timestamp").and_then(Value::as_str).and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&Utc)).unwrap_or_else(Utc::now);
        for b in r.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
            match b.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let (name, input) = (b.get("name").and_then(Value::as_str).unwrap_or_default(), b.get("input").cloned().unwrap_or(Value::Null));
                    let id = b.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                    let text = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                    if name == DESKTOP_SPAWN && !text("title").is_empty() {
                        calls.push(DesktopCall { call: id, task: None, at: when, kind: DesktopKind::Spawn { title: text("title"), tldr: text("tldr"), prompt: text("prompt") } });
                    } else if name == DESKTOP_DISMISS {
                        let task = input.get("task_id").map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).unwrap_or_default();
                        calls.push(DesktopCall { call: id, task: None, at: when, kind: DesktopKind::Dismiss { task } });
                    }
                }
                Some("tool_result") => {
                    let of = b.get("tool_use_id").and_then(Value::as_str).unwrap_or_default();
                    if let Some(c) = calls.iter_mut().find(|c| c.call == of && matches!(c.kind, DesktopKind::Spawn { .. })) {
                        c.task = task_id_in(&result_text(b.get("content").unwrap_or(&Value::Null)));
                    } else if waiting.contains(of)
                        && let Some(task) = task_id_in(&result_text(b.get("content").unwrap_or(&Value::Null)))
                    {
                        calls.push(DesktopCall { call: of.to_string(), task: None, at: when, kind: DesktopKind::Answer { task } });
                    }
                }
                _ => {}
            }
        }
    }
    Ok((calls, at))
}

fn result_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// The task's id in `spawn_task`'s answer: a JSON `task_id` (or `taskId`, `id`), or the word after
/// "task_id" / "task ID".
fn task_id_in(text: &str) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        for k in ["task_id", "taskId", "id"] {
            if let Some(id) = v.get(k).map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())) {
                return Some(id);
            }
        }
    }
    let lower = text.to_lowercase();
    let at = lower.find("task_id").map(|i| i + 7).or_else(|| lower.find("task id").map(|i| i + 7))?;
    let id: String = text[at..].trim_start_matches(|c: char| c == '"' || c == ':' || c == '=' || c.is_whitespace()).chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    (!id.is_empty()).then_some(id)
}

fn already(state: AsideState) -> &'static str {
    match state {
        AsideState::Launched => "launched",
        AsideState::Then => "chained",
        AsideState::Ticket => "ticket",
        AsideState::Dismissed => "dismissed",
        AsideState::Open | AsideState::Later => "open",
    }
}

fn already_note(a: &Aside) -> String {
    match a.state {
        AsideState::Launched => format!("already launched in another agent ({}): do not do it, nor propose it again", a.agent_title.as_deref().unwrap_or("?")),
        AsideState::Then => "already chained after an agent: do not do it, nor propose it again".into(),
        AsideState::Ticket => format!("already a ticket ({}): do not propose it again", a.ticket.as_deref().unwrap_or("?")),
        AsideState::Dismissed => match &a.why {
            Some(w) => format!("dismissed ({w}): do not propose it again"),
            None => "dismissed: do not propose it again".into(),
        },
        AsideState::Open | AsideState::Later => "open".into(),
    }
}

/// An agent's id as Prism makes them: nothing that could step out of the events folder.
fn valid_agent_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn news_path(events: &Path, agent: &str) -> PathBuf {
    events.join(format!("{agent}.aside-news"))
}

/// The news waiting for `agent`, taken: renamed first, so a line written meanwhile waits for the
/// next time rather than being lost.
pub fn take_news(events: &Path, agent: &str) -> Vec<String> {
    if !valid_agent_id(agent) {
        return Vec::new();
    }
    let path = news_path(events, agent);
    if !path.exists() {
        return Vec::new();
    }
    let taken = path.with_extension(format!("taken-{}", std::process::id()));
    if std::fs::rename(&path, &taken).is_err() {
        return Vec::new();
    }
    let text = std::fs::read_to_string(&taken).unwrap_or_default();
    let _ = std::fs::remove_file(&taken);
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_titles_on_the_same_files_are_one_subject() {
        let f = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(similar("Remove the dead VAT helper", &f(&["src/vat.rs"]), "Delete dead VAT rounding helper", &f(&["./src/vat.rs"])));
        assert!(!similar("Remove the dead VAT helper", &f(&["src/vat.rs"]), "Add a test for the cart total", &f(&["src/vat.rs"])));
        assert!(!similar("Remove the dead VAT helper", &f(&["src/vat.rs"]), "Remove the dead VAT helper", &f(&["src/cart.rs"])));
        assert!(similar("Update the stale README install steps", &[], "Update stale README install steps", &f(&["README.md"])));
    }

    #[test]
    fn a_desktop_transcript_gives_its_spawn_and_dismiss_calls() {
        let dir = std::env::temp_dir().join(format!("prism-aside-desktop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("s.jsonl");
        let lines = [
            r#"{"type":"user","message":{"content":"hello"}}"#,
            r#"{"type":"assistant","timestamp":"2026-11-05T10:00:00Z","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"mcp__ccd_session__spawn_task","input":{"title":"Remove dead code","tldr":"unused","prompt":"Remove it"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":[{"type":"text","text":"Task created. task_id: tsk_42"}]}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_2","name":"mcp__ccd_session__dismiss_task","input":{"task_id":"tsk_42"}}]}}"#,
        ];
        std::fs::write(&file, format!("{}\n{{\"cut", lines.join("\n"))).unwrap();
        let none = std::collections::HashSet::new();
        let (calls, to) = desktop_calls(&file, 0, &none).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].task.as_deref(), Some("tsk_42"));
        assert!(matches!(&calls[1].kind, DesktopKind::Dismiss { task } if task == "tsk_42"));
        assert_eq!(to as usize, lines.join("\n").len() + 1, "the cut line waits");
        assert!(desktop_calls(&file, to, &none).unwrap().0.is_empty());
        // Its answer read on a later scan: the task's id still found, for the call waiting.
        let (calls, _) = desktop_calls(&file, lines[..2].join("\n").len() as u64 + 1, &std::collections::HashSet::from(["toolu_1".to_string()])).unwrap();
        assert!(matches!(&calls[0].kind, DesktopKind::Answer { task } if task == "tsk_42") && calls[0].call == "toolu_1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn news_are_taken_once() {
        let dir = std::env::temp_dir().join(format!("prism-aside-news-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(news_path(&dir, "a1"), "one\ntwo\n").unwrap();
        assert_eq!(take_news(&dir, "a1"), ["one", "two"]);
        assert!(take_news(&dir, "a1").is_empty());
        assert!(take_news(&dir, "../x").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
