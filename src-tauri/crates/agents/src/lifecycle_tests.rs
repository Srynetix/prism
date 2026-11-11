//! An agent's life on a tmux in memory: launched, prepared then started, exited, resumed, closed.

use std::sync::Arc;

use super::*;
use tmux::fake::Server;

/// Agents in a folder of their own, on a tmux in memory; the folder agents launch in.
struct Bench {
    agents: Agents,
    tmux: Arc<Server>,
    hub: Hub,
    root: PathBuf,
    project: PathBuf,
}

impl Bench {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("prism-agents-{}", uuid::Uuid::new_v4()));
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        let tmux = Arc::new(Server::default());
        let hub = Hub::new();
        let data = root.join("data");
        let agents = Agents::with_tmux(data.clone(), hub.clone(), Vec::new(), Vec::new(), Tmux::fake(&data.join("agents"), tmux.clone()).unwrap()).unwrap();
        // The command lines compared below without Prism's « in passing » server (its own test).
        agents.inner.settings.lock().unwrap().aside.enabled = false;
        Self { agents, tmux, hub, root, project }
    }

    fn launch(&self, title: &str) -> Launch {
        Launch {
            title: title.into(),
            cwd: Some(self.project.clone()),
            prompt: Some("Fix the build.".into()),
            worktree: None,
            permission_mode: None,
            category: None,
            tags: Vec::new(),
            provider: None,
            account: None,
            model: None,
            effort: None,
            then: Vec::new(),
            from: None,
            prism_mcp: false,
        }
    }

    /// The agent as the snapshot shows it, after a refresh.
    async fn shown(&self, id: &str) -> Option<Agent> {
        self.agents.refresh().await;
        self.hub.subscribe().borrow().agents.iter().find(|a| a.id == id).cloned()
    }

    fn registered(&self, id: &str) -> bool {
        self.agents.inner.registry.lock().unwrap().get(id).is_some()
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn a_launched_agent_runs_then_exits() {
    let b = Bench::new();
    let id = b.agents.spawn(b.launch(" Build ")).await.unwrap();
    let argv = b.tmux.argv(&id).expect("its tmux session");
    assert_eq!(argv[..3], ["claude".to_string(), "--session-id".into(), id.clone()]);
    assert_eq!(argv.last().map(String::as_str), Some("Fix the build."));
    let agent = b.shown(&id).await.unwrap();
    assert_eq!((agent.title.as_str(), agent.state, agent.cwd.as_str()), ("Build", AgentState::Running, b.project.to_str().unwrap()));
    b.tmux.exit(&id, 1);
    let agent = b.shown(&id).await.unwrap();
    assert_eq!((agent.state, agent.exit_code), (AgentState::Exited, Some(1)));
    b.tmux.crash();
    assert_eq!(b.shown(&id).await.unwrap().state, AgentState::Gone);
}

#[tokio::test]
async fn a_prepared_agent_starts_under_its_id() {
    let b = Bench::new();
    let id = b.agents.prepare(b.launch("Later")).await.unwrap();
    assert!(b.tmux.argv(&id).is_none(), "nothing runs before Start");
    assert_eq!(b.shown(&id).await.unwrap().state, AgentState::Prepared);
    b.agents.rename(&id, "Now").unwrap();
    b.agents.start(&id).await.unwrap();
    let agent = b.shown(&id).await.unwrap();
    assert_eq!((agent.title.as_str(), agent.state), ("Now", AgentState::Running));
    assert!(b.agents.start(&id).await.unwrap_err().to_string().contains("already started"));
}

#[tokio::test]
async fn a_launch_in_a_missing_folder_is_refused() {
    let b = Bench::new();
    let launch = Launch { cwd: Some(b.root.join("nowhere")), ..b.launch("Lost") };
    assert!(b.agents.spawn(launch).await.unwrap_err().to_string().contains("is not a directory"));
    assert!(b.tmux.calls_of("new-session").is_empty());
}

#[tokio::test]
async fn resume_relaunches_only_a_stopped_agent() {
    let b = Bench::new();
    let id = b.agents.spawn(b.launch("Resume")).await.unwrap();
    b.agents.resume(&id).await.unwrap();
    assert!(b.tmux.calls_of("respawn-pane").is_empty(), "still running: nothing to do");
    b.tmux.exit(&id, 0);
    b.agents.resume(&id).await.unwrap();
    assert_eq!(b.tmux.calls_of("respawn-pane").len(), 1, "its pane is reused");
    assert_eq!(b.shown(&id).await.unwrap().state, AgentState::Running);
    // Its tmux session lost (a computer restart): a new one.
    b.tmux.crash();
    b.agents.resume(&id).await.unwrap();
    assert_eq!(b.tmux.calls_of("new-session").len(), 2);
}

#[tokio::test]
async fn close_stops_kills_and_forgets() {
    let b = Bench::new();
    let id = b.agents.spawn(b.launch("Close")).await.unwrap();
    let log = b.agents.inner.events.join(format!("{id}.jsonl"));
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, "").unwrap();
    assert_eq!(b.agents.close(&id).await.unwrap(), None);
    let keys: Vec<Vec<String>> = b.tmux.calls_of("send-keys");
    assert_eq!(keys.iter().filter(|k| k.last().is_some_and(|k| k == "C-c")).count(), 2, "stopped as from the keyboard first");
    assert!(b.tmux.argv(&id).is_none(), "its session killed");
    assert!(!b.registered(&id) && !log.exists());
    assert!(b.shown(&id).await.is_none());
}

#[tokio::test]
async fn a_running_agent_keeps_its_worktree_and_asks_nothing_unasked() {
    let b = Bench::new();
    let id = b.agents.spawn(b.launch("Busy")).await.unwrap();
    assert!(b.agents.remove_worktree(&id).await.unwrap_err().to_string().contains("stop the agent first"));
    assert!(b.agents.allow(&id).await.unwrap_err().to_string().contains("not waiting"));
    assert!(b.tmux.calls_of("send-keys").is_empty());
}

/// A clone with an `origin` (a bare repository next to it) on `main`, one commit: agents make
/// worktrees from it.
fn clone_with_origin(root: &Path, name: &str) -> WorktreeRepo {
    let run = |dir: &Path, args: &[&str]| {
        let out = std::process::Command::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    let origin = root.join(format!("{name}.git"));
    let path = root.join(name);
    std::fs::create_dir_all(&path).unwrap();
    run(root, &["init", "--bare", "-b", "main", origin.to_str().unwrap()]);
    run(&path, &["init", "-b", "main"]);
    run(&path, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "start"]);
    run(&path, &["remote", "add", "origin", origin.to_str().unwrap()]);
    run(&path, &["push", "-u", "origin", "main"]);
    WorktreeRepo { name: name.into(), path, main_branch: "main".into(), release_prefix: String::new(), base_branch: "main".into() }
}

#[tokio::test]
async fn an_agent_in_several_repositories_gets_a_worktree_in_each() {
    let b = Bench::new();
    let (helm, docker) = (clone_with_origin(&b.root, "helm"), clone_with_origin(&b.root, "dockerfiles"));
    b.agents.set_repos(vec![helm.clone(), docker.clone()], Vec::new());
    let worktree = NewWorktree { repo: Some("helm".into()), name: Some("redis-7".into()), branch: None, base: None, also: vec!["dockerfiles".into(), "helm".into()] };
    let launch = Launch { worktree: Some(worktree), provider: Some(Provider::Codex), ..b.launch("Redis 7") };
    let id = b.agents.spawn(launch).await.unwrap();
    let extra = b.agents.inner.registry.lock().unwrap().get(&id).unwrap().extra.clone();
    assert_eq!(extra.len(), 1, "its own repository is not made twice: {extra:?}");
    assert!(extra[0].is_dir() && extra[0].ends_with("redis-7"), "{extra:?}");
    let argv = b.tmux.argv(&id).unwrap();
    let at = argv.iter().position(|a| a == "--add-dir").expect("the other worktree given to the CLI");
    assert_eq!(argv[at + 1], extra[0].display().to_string());
    let branch = std::process::Command::new("git").args(["branch", "--show-current"]).current_dir(&extra[0]).output().unwrap();
    assert!(String::from_utf8_lossy(&branch.stdout).trim().ends_with("redis-7"));
    // Resumed, the CLI gets it again.
    b.tmux.exit(&id, 0);
    b.agents.refresh().await;
    b.agents.resume(&id).await.unwrap();
    assert!(b.tmux.argv(&id).unwrap().iter().any(|a| a == "--add-dir"));
    // Closed with nothing done: both worktrees go.
    b.agents.close(&id).await.unwrap();
    assert!(!extra[0].exists(), "the other repository's worktree removed too");
}

#[tokio::test]
async fn a_cli_without_extra_folders_is_refused_several_repositories() {
    let b = Bench::new();
    b.agents.set_repos(vec![clone_with_origin(&b.root, "helm"), clone_with_origin(&b.root, "dockerfiles")], Vec::new());
    let worktree = NewWorktree { repo: Some("helm".into()), name: Some("redis-7".into()), branch: None, base: None, also: vec!["dockerfiles".into()] };
    let launch = Launch { worktree: Some(worktree), provider: Some(Provider::OpenCode), ..b.launch("Redis 7") };
    let e = b.agents.spawn(launch).await.unwrap_err().to_string();
    assert!(e.contains("one folder only"), "{e}");
    assert!(b.tmux.calls_of("new-session").is_empty(), "nothing made, nothing run");
    assert!(!b.root.join("helm.worktrees").exists() && !b.root.join("dockerfiles.worktrees").exists(), "no worktree made");
}

#[tokio::test]
async fn another_cli_takes_it_on_and_back() {
    let b = Bench::new();
    // Codex "installed": its executable named in its settings.
    b.agents.inner.settings.lock().unwrap().codex.bin = Some("/bin/sh".into());
    let id = b.agents.spawn(b.launch("Build")).await.unwrap();
    let to = |p: Provider, message: &str| switch::Switch { to: p, account: None, message: message.into(), handoff: "prism".into(), reason: Some("limit".into()), model: None, effort: None, permission_mode: None };
    assert!(b.agents.switch(&id, to(Provider::Claude, "x")).await.is_err(), "it runs Claude already");
    assert!(b.agents.switch(&id, to(Provider::Codex, "  ")).await.is_err(), "nothing to tell");

    b.agents.switch(&id, to(Provider::Codex, "-- where it is")).await.unwrap();
    let argv = b.tmux.argv(&id).unwrap();
    assert_eq!(argv[0], "/bin/sh");
    assert_eq!(argv[argv.len() - 2..], ["--".to_string(), "-- where it is".into()]);
    let agent = b.shown(&id).await.unwrap();
    assert_eq!((agent.provider, agent.legs.len()), (Provider::Codex, 1));
    let leg = &agent.legs[0];
    assert_eq!((leg.provider, leg.session.as_deref(), leg.reason.as_str(), leg.message.as_str()), (Provider::Claude, Some(id.as_str()), "limit", "-- where it is"));
    let log = std::fs::read_to_string(b.agents.inner.events.join(format!("{id}.jsonl"))).unwrap();
    assert!(log.contains(status::SWITCH), "the log says it");

    // Back to Claude: its own conversation, the relay as its next prompt.
    b.agents.switch(&id, to(Provider::Claude, "since then")).await.unwrap();
    let argv = b.tmux.argv(&id).unwrap();
    assert_eq!(argv[..3], ["claude".to_string(), "--resume".into(), id.clone()]);
    assert_eq!(argv.last().map(String::as_str), Some("since then"));
    let record = b.agents.record(&id).unwrap();
    assert_eq!((record.provider, record.legs.len(), record.resumed.as_deref()), (Provider::Claude, 2, Some(id.as_str())));
    assert_eq!(record.launch.unwrap().provider, Some(Provider::Claude));
}

/// A switch's words, as the window sends them.
fn switch(to: Provider, message: &str) -> switch::Switch {
    switch::Switch { to, account: None, message: message.into(), handoff: "prism".into(), reason: None, model: None, effort: None, permission_mode: None }
}

impl Bench {
    /// Codex and Claude Code found: their `bin`, a file in the bench's folder.
    fn with_clis(self) -> Self {
        let bin = |name: &str| {
            let p = self.root.join(name);
            std::fs::write(&p, "").unwrap();
            Some(p.display().to_string())
        };
        let mut cfg = AgentsConfig::default();
        cfg.aside.enabled = false;
        cfg.codex.bin = bin("codex");
        cfg.claude.cli.bin = bin("claude");
        self.agents.configure(cfg);
        self
    }
}

/// What Continue with… refuses leaves the agent as it was: no terminal touched, no stretch noted.
#[tokio::test]
async fn a_switch_asked_wrong_changes_nothing() {
    let b = Bench::new().with_clis();
    let id = b.agents.spawn(b.launch("Stay")).await.unwrap();
    let refused = |s: switch::Switch| {
        let agents = b.agents.clone();
        let id = id.clone();
        async move { agents.switch(&id, s).await.unwrap_err().to_string() }
    };
    assert!(refused(switch(Provider::Claude, "go")).await.contains("already runs"));
    assert!(refused(switch(Provider::Codex, "  ")).await.contains("nothing to tell"));
    assert!(refused(switch::Switch { handoff: "magic".into(), ..switch(Provider::Codex, "go") }).await.contains("unknown relay"));
    assert!(refused(switch::Switch { reason: Some("whim".into()), ..switch(Provider::Codex, "go") }).await.contains("unknown reason"));
    assert!(refused(switch::Switch { model: Some("gpt; rm -rf ~".into()), ..switch(Provider::Codex, "go") }).await.contains("not a model name"));
    assert!(refused(switch::Switch { effort: Some("--yolo".into()), ..switch(Provider::Codex, "go") }).await.contains("not an effort level"));
    let prepared = b.agents.prepare(b.launch("Later")).await.unwrap();
    assert!(b.agents.switch(&prepared, switch(Provider::Codex, "go")).await.unwrap_err().to_string().contains("has not started yet"));
    assert!(b.tmux.calls_of("respawn-pane").is_empty());
    let record = b.agents.record(&id).unwrap();
    assert_eq!((record.provider, record.legs.len()), (Provider::Claude, 0));
}

/// Continue with Codex: Claude Code's stretch kept with its session and the relay, Codex started
/// in the same pane with the relay as its prompt; back on Claude Code, its conversation resumed.
#[tokio::test]
async fn a_switch_hands_over_and_back() {
    let b = Bench::new().with_clis();
    let id = b.agents.spawn(b.launch("Relay")).await.unwrap();
    b.agents.switch(&id, switch::Switch { reason: Some("limit".into()), ..switch(Provider::Codex, "Carry on with the tests.") }).await.unwrap();
    let argv = b.tmux.argv(&id).unwrap();
    assert!(argv[0].ends_with("/codex") && argv.last().map(String::as_str) == Some("Carry on with the tests."), "{argv:?}");
    assert_eq!(b.tmux.calls_of("respawn-pane").len(), 1, "the same pane");
    let record = b.agents.record(&id).unwrap();
    assert_eq!(record.provider, Provider::Codex);
    let leg = &record.legs[0];
    assert_eq!((leg.provider, leg.session.as_deref(), leg.reason.as_str(), leg.handoff.as_str(), leg.message.as_str()), (Provider::Claude, Some(id.as_str()), "limit", "prism", "Carry on with the tests."));

    b.agents.switch(&id, switch(Provider::Claude, "Back to you.")).await.unwrap();
    let argv = b.tmux.argv(&id).unwrap();
    assert_eq!(argv[1..3], ["--resume".to_string(), id.clone()], "{argv:?}");
    assert_eq!(argv.last().map(String::as_str), Some("Back to you."));
    let record = b.agents.record(&id).unwrap();
    assert_eq!((record.provider, record.legs.len(), record.resumed.as_deref()), (Provider::Claude, 2, Some(id.as_str())));
}

/// The launcher's command line is the one `spawn` runs, its id and Prism's wiring shown as `…`.
#[tokio::test]
async fn the_launchers_command_line_is_the_launchs() {
    let b = Bench::new();
    let preview = |provider, mode: &str, prompt: &str| LaunchPreview {
        provider,
        title: "Fix it".into(),
        permission_mode: Some(mode.into()),
        model: Some("opus".into()),
        effort: None,
        worktree: Some("fix-it".into()),
        also: vec!["api".into()],
        prompt: Some(prompt.into()),
        prism_mcp: false,
    };
    assert_eq!(
        b.agents.command_line(preview(Provider::Claude, "plan", "Why's it slow?")).unwrap(),
        "claude --add-dir …/api.worktrees/fix-it --model opus --session-id … -n 'Fix it' --settings … --permission-mode plan --worktree fix-it -- 'Why'\\''s it slow?'"
    );
    assert_eq!(b.agents.command_line(preview(Provider::Codex, "auto", "go")).unwrap(), "codex --add-dir …/api.worktrees/fix-it --model opus --approve-for-me -- go");
    assert_eq!(b.agents.command_line(preview(Provider::OpenCode, "default", "")).unwrap(), "opencode --model opus");
    assert!(b.agents.command_line(preview(Provider::CommandCode, "auto", "go")).is_err(), "no auto mode");
}

/// `[agents.aside]`: Claude Code gets the « in passing » server by `--mcp-config` (its two tools
/// allowed), Codex by its options, the whole server taking its place when asked for.
#[tokio::test]
async fn the_agents_get_the_aside_tools() {
    let b = Bench::new();
    b.agents.inner.settings.lock().unwrap().aside.enabled = true;
    let preview = |provider, prism_mcp| LaunchPreview { provider, title: "Fix it".into(), permission_mode: None, model: None, effort: None, worktree: None, also: Vec::new(), prompt: Some("go".into()), prism_mcp };
    let claude = b.agents.command_line(preview(Provider::Claude, false)).unwrap();
    assert!(claude.contains("--mcp-config") && claude.contains("--allowedTools=mcp__prism__suggest_task,mcp__prism__dismiss_task,mcp__plugin_prism_prism__suggest_task"), "{claude}");
    let codex = b.agents.command_line(preview(Provider::Codex, false)).unwrap();
    assert!(codex.contains("mcp_servers.prism.args=") && codex.contains(r#""mcp", "aside"]"#), "{codex}");
    let whole = b.agents.command_line(preview(Provider::Claude, true)).unwrap();
    assert!(whole.contains("--plugin-dir") && !whole.contains("--mcp-config") && whole.contains("--allowedTools="), "{whole}");
}

/// « In passing »: one line per subject; only its agent withdraws it; an answer to a line dealt
/// with already, or a second ticket meanwhile, is refused.
#[tokio::test]
async fn an_aside_is_answered_once() {
    let b = Bench::new();
    b.agents.inner.settings.lock().unwrap().aside.enabled = true;
    let s = |title: &str| crate::asides::Suggest { title: title.into(), tldr: "t".into(), prompt: "p".into(), files: vec!["src/a.rs".into()], ..Default::default() };
    let first = b.agents.suggest(Some("a1"), "claude", s("Remove the dead helper")).unwrap();
    let again = b.agents.suggest(Some("a2"), "codex", s("Remove dead helper")).unwrap();
    assert_eq!(first["task_id"], again["task_id"]);
    let id = first["task_id"].as_u64().unwrap();
    assert_eq!(b.agents.aside(id).unwrap().seen.len(), 2);
    assert!(b.agents.dismiss_by_agent(None, id, None).is_err(), "no agent: not its proposal");
    b.agents.aside_action(id, "later", None, None).unwrap();
    assert!(b.agents.aside_action(id, "later", None, None).is_err());
    b.agents.begin_ticket(id).unwrap();
    assert!(b.agents.begin_ticket(id).is_err(), "a ticket under way");
    b.agents.end_ticket(id);
    b.agents.aside_ticket(id, "T-9").unwrap();
    assert!(b.agents.aside_action(id, "dismiss", None, None).is_err(), "dealt with already");
    let told = b.agents.take_news("a1");
    assert!(told.len() == 1 && told[0].contains("T-9"), "{told:?}");
    assert_eq!(b.agents.suggest(Some("a1"), "claude", s("Remove the dead helper")).unwrap()["already"], "ticket");
}

/// The launch of a proposal, as the launcher is given it: its prompt headed by where it comes from
/// and its files, `from` naming it.
#[tokio::test]
async fn an_aside_launches_with_where_it_comes_from() {
    let b = Bench::new();
    b.agents.inner.settings.lock().unwrap().aside.enabled = true;
    let s = crate::asides::Suggest { title: "Add a test".into(), tldr: "t".into(), prompt: "Test credit notes.".into(), files: vec!["tests/a.rs".into()], kind: Some("Development".into()), ..Default::default() };
    let id = b.agents.suggest(None, "claude", s).unwrap()["task_id"].as_u64().unwrap();
    let l = b.agents.aside_launch_spec(id, false).unwrap();
    assert_eq!((l.title.as_str(), l.category.as_deref(), l.from), ("Add a test", Some("Development"), Some(format!("aside:{id}"))));
    assert_eq!(l.prompt.as_deref(), Some("Spotted in passing by the agent \"claude\", while it worked on something else.\n\nTest credit notes.\n\nFiles concerned: tests/a.rs"));
}

/// The prompts pasted into agents' terminals (their text goes on tmux's stdin, which the fake keeps not).
fn pasted(tmux: &Server) -> usize {
    tmux.calls_of("paste-buffer").len()
}

#[tokio::test]
async fn a_chain_runs_once_its_agent_has_worked() {
    let b = Bench::new();
    let mut launch = b.launch("Chained");
    launch.then = vec![
        state::LaunchStep { kind: state::StepKind::Prompt, when: state::StepWhen::Done, command: None, text: Some("Review on {branch}.".into()) },
        state::LaunchStep { kind: state::StepKind::Notify, when: state::StepWhen::Ok, command: None, text: None },
    ];
    let mut notices = b.agents.notices();
    let id = b.agents.spawn(launch).await.unwrap();
    let chain = || b.hub.subscribe().borrow().chains.get(&id).cloned().expect("its chain");
    assert_eq!((chain().current, chain().waiting), (-1, Some(state::ChainWait::Work)));
    let mut a = b.shown(&id).await.unwrap();
    // Idle before it was seen working: its first turn has not ended.
    a.status = SessionStatus::Idle;
    b.agents.tick(std::slice::from_ref(&a));
    assert_eq!(chain().waiting, Some(state::ChainWait::Work));
    a.status = SessionStatus::Running;
    b.agents.tick(std::slice::from_ref(&a));
    a.status = SessionStatus::NeedsYou;
    b.agents.tick(std::slice::from_ref(&a));
    // The prompt step: pasted, then a wait for its turn.
    for _ in 0..50 {
        if chain().waiting == Some(state::ChainWait::Prompt) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!((chain().current, chain().runs[0].state), (0, state::RunState::Running));
    assert_eq!(pasted(&b.tmux), 1);
    a.status = SessionStatus::Running;
    b.agents.tick(std::slice::from_ref(&a));
    a.status = SessionStatus::ReviewReady;
    b.agents.tick(std::slice::from_ref(&a));
    let n = tokio::time::timeout(Duration::from_secs(2), notices.recv()).await.unwrap().unwrap();
    assert!(n.system && n.text.contains("its chain reached its end"), "{n:?}");
    let n = tokio::time::timeout(Duration::from_secs(2), notices.recv()).await.unwrap().unwrap();
    assert!(n.text.starts_with("Chain finished"), "{n:?}");
    let c = chain();
    assert!(!c.active() && c.runs.iter().all(|r| r.state == state::RunState::Done));
    // Kept for the next Prism.
    let kept: std::collections::BTreeMap<String, state::Chain> = serde_json::from_slice(&std::fs::read(b.root.join("data/chains.json")).unwrap()).unwrap();
    assert_eq!(kept[&id], c);
    b.agents.chain_order(&id, "forget").unwrap();
    assert!(b.hub.subscribe().borrow().chains.is_empty());
}

#[tokio::test]
async fn a_failed_step_stops_what_needed_it() {
    let b = Bench::new();
    let mut launch = b.launch("Stopped");
    launch.then = vec![
        state::LaunchStep { kind: state::StepKind::Task, when: state::StepWhen::Done, command: Some("  ".into()), text: None },
        state::LaunchStep { kind: state::StepKind::Mr, when: state::StepWhen::Ok, command: None, text: None },
    ];
    let id = b.agents.spawn(launch).await.unwrap();
    let mut a = b.shown(&id).await.unwrap();
    a.state = AgentState::Exited;
    a.exit_code = Some(0);
    b.agents.tick(std::slice::from_ref(&a));
    let chain = || b.hub.subscribe().borrow().chains.get(&id).cloned().unwrap();
    for _ in 0..50 {
        if chain().stopped {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let c = chain();
    assert_eq!((c.runs[0].state, c.runs[0].detail.as_deref()), (state::RunState::Failed, Some("no command")));
    assert_eq!(c.runs[1].state, state::RunState::Skipped);
    assert!(b.agents.chain_order(&id, "go").is_err(), "nothing held");
}

#[tokio::test]
async fn a_prompt_waits_for_its_menu_then_goes() {
    let b = Bench::new();
    let id = b.agents.spawn(b.launch("Asked")).await.unwrap();
    let mut a = b.shown(&id).await.unwrap();
    a.status = SessionStatus::WaitingTool;
    b.hub.update(|s| {
        s.agents = vec![a.clone()];
        true
    });
    assert_eq!(b.agents.send(&id, "and the docs").await.unwrap(), Sent::Queued);
    assert_eq!(b.agents.send(&id, "and the docs ").await.unwrap(), Sent::AlreadyQueued);
    assert_eq!(b.hub.subscribe().borrow().inbox.get(&id), Some(&1));
    b.agents.tick(std::slice::from_ref(&a));
    assert_eq!(pasted(&b.tmux), 0, "held while its menu is open");
    a.status = SessionStatus::NeedsYou;
    b.agents.tick(std::slice::from_ref(&a));
    assert!(b.hub.subscribe().borrow().inbox.is_empty());
    for _ in 0..50 {
        if pasted(&b.tmux) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(pasted(&b.tmux), 1);
    b.hub.update(|s| {
        s.agents = vec![a.clone()];
        true
    });
    assert_eq!(b.agents.send(&id, "and the docs").await.unwrap(), Sent::AlreadySent);
}

/// The runs of agent `id`'s chain once `done` says so (its steps run on spawned tasks).
async fn runs_once(b: &Bench, id: &str, done: impl Fn(&[state::StepRun]) -> bool) -> Vec<state::StepRun> {
    for _ in 0..100 {
        let runs = b.hub.subscribe().borrow().chains.get(id).map(|c| c.runs.clone()).unwrap_or_default();
        if done(&runs) {
            return runs;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("the chain of {id} never got there: {:?}", b.hub.subscribe().borrow().chains.get(id));
}

/// A chain after the agent's work: a notify step, then one held for your go, which runs it.
#[tokio::test]
async fn a_chain_goes_on_after_the_work_and_waits_for_a_go() {
    use state::{RunState, SessionStatus};
    let b = Bench::new();
    let step = |when| state::LaunchStep { kind: state::StepKind::Notify, when, command: None, text: None };
    let mut notices = b.agents.notices();
    let id = b.agents.spawn(Launch { then: vec![step(state::StepWhen::Done), step(state::StepWhen::Ask)], ..b.launch("Chained") }).await.unwrap();
    assert!(b.agents.chain_order(&id, "go").is_err(), "nothing waits for a go yet");
    let mut a = b.shown(&id).await.unwrap();
    // Ready before it worked: nothing happens.
    a.status = SessionStatus::Completed;
    b.agents.tick(std::slice::from_ref(&a));
    assert_eq!(b.hub.subscribe().borrow().chains[&id].runs[0].state, RunState::Pending);
    a.status = SessionStatus::Running;
    b.agents.tick(std::slice::from_ref(&a));
    a.status = SessionStatus::Completed;
    b.agents.tick(std::slice::from_ref(&a));
    let runs = runs_once(&b, &id, |r| r[1].state == RunState::Held).await;
    assert_eq!(runs[0].state, RunState::Done);
    assert!(notices.try_recv().unwrap().system, "the notify step notifies the system");
    b.agents.chain_order(&id, "go").unwrap();
    runs_once(&b, &id, |r| r[1].state == RunState::Done).await;
}

/// A step that runs only if the work succeeded is skipped after a failed exit, and the chain stops.
#[tokio::test]
async fn a_chain_stops_when_the_work_failed() {
    use state::RunState;
    let b = Bench::new();
    let step = |when| state::LaunchStep { kind: state::StepKind::Notify, when, command: None, text: None };
    let id = b.agents.spawn(Launch { then: vec![step(state::StepWhen::Ok), step(state::StepWhen::Done)], ..b.launch("Failing") }).await.unwrap();
    b.tmux.exit(&id, 1);
    let a = b.shown(&id).await.unwrap();
    b.agents.tick(std::slice::from_ref(&a));
    let runs = runs_once(&b, &id, |r| r[0].state == RunState::Skipped).await;
    assert_eq!(runs[1].state, RunState::Skipped);
    assert!(b.hub.subscribe().borrow().chains[&id].stopped);
}

/// A git repository with one commit, known to the snapshot as `api`.
fn routine_bench() -> Bench {
    let b = Bench::new();
    let git = |args: &[&str]| assert!(std::process::Command::new("git").args(args).current_dir(&b.project).output().unwrap().status.success(), "git {args:?}");
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(b.project.join("README"), "hi\n").unwrap();
    git(&["add", "."]);
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "first"]);
    let repo = state::Repo { name: "api".into(), path: b.project.display().to_string(), project: None, branch: Some("main".into()), base: "main".into(), behind: 0, ahead: 0, dirty: false, is_main: true, error: None, forge: None };
    b.hub.update(|s| {
        s.repos = vec![repo];
        true
    });
    b
}

fn routine_config(every: &str) -> AgentsConfig {
    AgentsConfig {
        quick_actions: vec![config::QuickAction { name: "Bump".into(), prompt: "Bump {repo} since {since}.".into(), repos: vec!["api".into()], ..Default::default() }],
        routines: vec![toml::from_str(&format!("name = \"Nightly\"\naction = \"Bump\"\nevery = \"{every}\"\nmax_minutes = 600\n")).unwrap()],
        ..Default::default()
    }
}

fn runs(b: &Bench) -> Vec<state::RoutineRun> {
    b.hub.subscribe().borrow().routines.runs.clone()
}

async fn until(what: &str, f: impl Fn() -> bool) {
    for _ in 0..150 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("never: {what}");
}

#[tokio::test]
async fn a_routine_runs_at_its_time_and_nothing_to_do_is_archived() {
    let b = routine_bench();
    b.agents.inner.routines_live.store(true, std::sync::atomic::Ordering::Relaxed);
    let cfg = routine_config("30m");
    // Saved from Settings: trusted as saved.
    b.agents.routines_saved(None, &cfg);
    b.agents.configure(cfg);
    let t0 = Utc::now();
    b.agents.schedule(t0);
    assert!(runs(&b).is_empty(), "its first sight sets its times, runs nothing");
    let view = b.hub.subscribe().borrow().routines.routines[0].clone();
    assert!(view.pending.is_none() && view.error.is_none() && view.next.len() == 5, "{view:?}");
    b.agents.schedule(t0 + chrono::Duration::minutes(31));
    until("its agent launched", || runs(&b).first().is_some_and(|r| r.agent.is_some())).await;
    let run = runs(&b)[0].clone();
    assert_eq!((run.state, run.repo.as_deref(), run.caught_up), (state::RoutineRunState::Running, Some("api"), false));
    let id = run.agent.clone().unwrap();
    let argv = b.tmux.argv(&id).unwrap();
    assert_eq!(argv.last().map(String::as_str), Some("Bump api since never: this is its first run."));
    // Its next time while it still runs: skipped, and noted.
    b.agents.schedule(t0 + chrono::Duration::minutes(61));
    let r = runs(&b);
    assert_eq!((r[0].state, r.len()), (state::RoutineRunState::Skipped, 2));
    assert!(r[0].note.as_deref().unwrap_or_default().contains("still goes on"));
    // It works, then stops at its prompt: no file changed, nothing to do; its agent closed.
    let mut a = b.shown(&id).await.unwrap();
    a.status = SessionStatus::Running;
    b.agents.watch_runs(std::slice::from_ref(&a), Utc::now()).await;
    a.status = SessionStatus::NeedsYou;
    b.agents.watch_runs(std::slice::from_ref(&a), Utc::now()).await;
    until("its run ended", || runs(&b).iter().any(|r| r.id == run.id && r.state == state::RoutineRunState::Nothing)).await;
    until("its agent closed", || !b.registered(&id)).await;
    assert!(!runs(&b).iter().any(state::RoutineRun::to_review));
    // Kept for the next Prism.
    let kept: serde_json::Value = serde_json::from_slice(&std::fs::read(b.root.join("data/routines.json")).unwrap()).unwrap();
    assert!(kept["slots"]["Nightly"]["ok"].is_string());
}

/// A routine with its own prompt: launched with it, its model and effort, its mode.
#[tokio::test]
async fn a_routine_runs_its_own_prompt() {
    let b = routine_bench();
    b.agents.inner.routines_live.store(true, std::sync::atomic::Ordering::Relaxed);
    let cfg = AgentsConfig {
        routines: vec![toml::from_str("name = \"Week\"\nprompt = \"Sum up {repo} ({base}) since {since}.\"\nrepos = [\"api\"]\nmode = \"plan\"\nmodel = \"opus\"\neffort = \"high\"\nevery = \"30m\"\nmax_minutes = 600\n").unwrap()],
        ..Default::default()
    };
    b.agents.routines_saved(None, &cfg);
    b.agents.configure(cfg);
    b.agents.publish_routines_now();
    let view = b.hub.subscribe().borrow().routines.routines[0].clone();
    assert!(view.error.is_none() && view.action.as_ref().is_some_and(|a| a.prompt.starts_with("Sum up")), "{view:?}");
    b.agents.run_routine_now("Week", None).unwrap();
    until("its agent launched", || runs(&b).first().is_some_and(|r| r.agent.is_some())).await;
    let id = runs(&b)[0].agent.clone().unwrap();
    let argv = b.tmux.argv(&id).unwrap();
    assert_eq!(argv.last().map(String::as_str), Some("Sum up api (main) since never: this is its first run."));
    let joined = argv.join(" ");
    assert!(joined.contains("--model opus") && joined.contains("--effort high") && joined.contains("--permission-mode plan"), "{joined}");
}

#[tokio::test]
async fn a_routine_changed_elsewhere_waits_and_a_late_one_is_caught_up_once() {
    let b = routine_bench();
    b.agents.inner.routines_live.store(true, std::sync::atomic::Ordering::Relaxed);
    let cfg = routine_config("30m");
    b.agents.routines_saved(None, &cfg);
    b.agents.configure(cfg.clone());
    let t0 = Utc::now() - chrono::Duration::hours(3);
    b.agents.schedule(t0);
    // Its quick action changed outside Settings: suspended.
    let mut other = cfg.clone();
    other.quick_actions[0].prompt = "Bump {repo}, then merge it.".into();
    b.agents.configure(other.clone());
    b.agents.schedule(t0 + chrono::Duration::minutes(31));
    assert!(runs(&b).is_empty());
    let pending = b.hub.subscribe().borrow().routines.routines[0].pending.clone().expect("waits for trust");
    assert_eq!(pending.reason, state::RoutinePendingReason::ActionModified);
    assert!(pending.diff.iter().any(|l| l.starts_with("+ prompt") && l.contains("merge")), "{:?}", pending.diff);
    assert!(b.agents.run_routine_now("Nightly", None).is_err(), "nor run by hand");
    assert!(b.agents.trust_routine("Nightly", "stale").is_err());
    // Trusted: its times from now on; asleep three hours, it catches up once.
    b.agents.trust_routine("Nightly", &pending.hash).unwrap();
    b.agents.inner.routines.lock().unwrap().kept.slots.get_mut("Nightly").unwrap().last = t0 - chrono::Duration::minutes(10);
    b.agents.schedule(Utc::now());
    let r = runs(&b);
    assert_eq!(r.len(), 1, "one run for three hours of times");
    assert!(r[0].caught_up);
    // A routine and an every both: said, not run.
    let mut both = other.clone();
    both.routines[0].cron = Some("0 3 * * *".into());
    b.agents.configure(both);
    let view = b.hub.subscribe().borrow().routines.routines[0].clone();
    assert!(view.error.unwrap().contains("exclude") && view.next.is_empty());
}

/// Launched by `schedule` at its first time, and seen working: the run and its agent.
async fn running_routine(b: &Bench, cfg: AgentsConfig) -> (state::RoutineRun, Agent) {
    b.agents.inner.routines_live.store(true, std::sync::atomic::Ordering::Relaxed);
    b.agents.routines_saved(None, &cfg);
    b.agents.configure(cfg);
    let t0 = Utc::now();
    b.agents.schedule(t0);
    b.agents.schedule(t0 + chrono::Duration::minutes(31));
    until("its agent launched", || runs(b).first().is_some_and(|r| r.agent.is_some())).await;
    let run = runs(b)[0].clone();
    let mut a = b.shown(run.agent.as_deref().unwrap()).await.unwrap();
    a.status = SessionStatus::Running;
    b.agents.watch_runs(std::slice::from_ref(&a), Utc::now()).await;
    (run, a)
}

#[tokio::test]
async fn a_run_past_its_time_ends_too_long_not_ready() {
    let b = routine_bench();
    let (run, mut a) = running_routine(&b, routine_config("30m")).await;
    // Past max_minutes: interrupted (Esc), after which its CLI waits as after a turn.
    let late = Utc::now() + chrono::Duration::minutes(601);
    b.agents.schedule(late);
    assert!(b.agents.inner.routines.lock().unwrap().kept.watch[&run.id].interrupted.is_some());
    a.status = SessionStatus::NeedsYou;
    b.agents.watch_runs(std::slice::from_ref(&a), late).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(runs(&b).iter().find(|r| r.id == run.id).unwrap().state, state::RoutineRunState::Running, "not finished as ready");
    // Its grace over: stopped, too long.
    b.agents.schedule(late + chrono::Duration::seconds(GRACE + 1));
    until("too long", || runs(&b).iter().any(|r| r.id == run.id && r.state == state::RoutineRunState::TooLong)).await;
    assert!(runs(&b).iter().find(|r| r.id == run.id).unwrap().to_review());
}

const GRACE: i64 = 30;

#[tokio::test]
async fn run_now_on_a_busy_repository_makes_no_run() {
    let b = routine_bench();
    let (run, _) = running_routine(&b, routine_config("30m")).await;
    let before = runs(&b).len();
    assert!(b.agents.run_routine_now("Nightly", None).unwrap_err().to_string().contains("still goes on"));
    assert_eq!(runs(&b).len(), before, "nothing left half launched");
    assert!(runs(&b).iter().filter(|r| r.under_way()).all(|r| r.id == run.id));
}

#[tokio::test]
async fn archiving_a_blocked_run_frees_its_routine() {
    let b = routine_bench();
    let (run, mut a) = running_routine(&b, routine_config("30m")).await;
    a.status = SessionStatus::WaitingTool;
    a.needs_action = Some("Allow Bash: npm install".into());
    b.agents.watch_runs(std::slice::from_ref(&a), Utc::now()).await;
    assert_eq!(runs(&b).iter().find(|r| r.id == run.id).unwrap().state, state::RoutineRunState::Blocked);
    b.agents.archive_runs(std::slice::from_ref(&run.id));
    let r = runs(&b).into_iter().find(|r| r.id == run.id).unwrap();
    assert!(r.archived && !r.under_way() && !r.to_review(), "{r:?}");
    // Its next time runs again.
    b.agents.run_routine_now("Nightly", None).unwrap();
}

#[tokio::test]
async fn a_task_goes_to_a_shell_just_started_and_a_busy_one_refuses_it() {
    let b = Bench::new();
    let id = b.agents.spawn(b.launch("Build")).await.unwrap();
    // No shell yet: one is started, still `tmux` for a moment on Linux, and takes the line at once.
    b.agents.run_in_shell(&id, None, "just test", true, false).await.unwrap();
    let typed = b.tmux.calls_of("send-keys").into_iter().any(|c| c.iter().any(|a| a == "just test"));
    assert!(typed, "the task typed into the new shell");
    // A shell running a program still refuses it (Ctrl-C offered).
    let vim = shell_key(&id, 2);
    b.tmux.run(&["new-session", "-d", "-s", &Tmux::session(&vim), "--", "vim"]).unwrap();
    let err = b.agents.run_in_shell(&id, Some(2), "just test", true, false).await.unwrap_err();
    assert!(err.to_string().contains("busy with vim"), "{err}");
}

#[tokio::test]
async fn a_shell_still_starting_is_not_busy() {
    let b = Bench::new();
    let id = b.agents.spawn(b.launch("Build")).await.unwrap();
    // Listed while it starts (`tmux`): free, so the Tasks tab takes it rather than another one.
    b.agents.shell(&id, Some(1)).await.unwrap();
    let shells = b.agents.shells(&id).await.unwrap();
    assert_eq!(shells.iter().map(|s| (s.n, s.command.as_str(), s.busy)).collect::<Vec<_>>(), [(1, "tmux", false)]);
    // A task run in a shell still starting waits for it, then goes.
    let key = shell_key(&id, 3);
    b.tmux.run(&["new-session", "-d", "-s", &Tmux::session(&key)]).unwrap();
    b.agents.run_in_shell(&id, Some(3), "just lint", true, false).await.unwrap();
    let typed = b.tmux.calls_of("send-keys").into_iter().any(|c| c.iter().any(|a| a == "just lint"));
    assert!(typed, "the task typed once the shell started");
    let looks = b.tmux.calls_of("list-panes").into_iter().filter(|c| c.contains(&Tmux::session(&key))).count();
    // Opened (tmux), held (tmux), looked again 100 ms later (sh).
    assert_eq!(looks, 3, "looked again until it was no longer tmux");
}

/// Claude Code asking to trust its folder before it starts: no hook, read off its screen; its
/// status that of a permission, Allow refused (its first choice is « No »); answered from its card,
/// the keys sent while the question shows, then seen gone.
#[tokio::test]
async fn a_folders_trust_is_read_off_the_screen_and_answered() {
    let b = Bench::new();
    let id = b.agents.spawn(b.launch("Trusting")).await.unwrap();
    let session = Tmux::session(&id);
    let screen = " Accessing workspace:\n\n /dev/prism.worktrees/fix\n\n Quick safety check: Is this a project you created or one you trust?\n\n ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · Esc to cancel\n";
    b.tmux.show(&session, screen, Some("Enter"));
    let a = b.shown(&id).await.unwrap();
    let gate = a.gate.clone().expect("its trust read off its screen");
    assert_eq!((gate.kind, gate.answerable, gate.folder.as_deref()), (state::GateKind::Folder, true, Some("/dev/prism.worktrees/fix")));
    assert_eq!(a.status, SessionStatus::WaitingTool);
    assert!(b.agents.allow(&id).await.is_err(), "Allow would press its « No »");
    assert!(b.agents.key(&id, "Enter").await.is_err());
    assert!(b.agents.paste(&id, "go on", true).await.is_err(), "its Enter would press its « No »");
    b.agents.answer_gate(&id, state::GateKind::Folder, true, false).await.unwrap();
    let keys: Vec<String> = b.tmux.calls_of("send-keys").into_iter().filter(|c| c.contains(&session)).filter_map(|c| c.last().cloned()).collect();
    assert_eq!(keys, ["Down", "Enter"]);
    assert!(b.shown(&id).await.unwrap().gate.is_none());
    // Gone from its screen meanwhile: nothing sent.
    assert!(b.agents.answer_gate(&id, state::GateKind::Folder, true, false).await.is_err());
}

/// Two accounts of Claude Code: an agent runs under the default one (its folder in its
/// environment), then another takes its conversation whole (copied into its folder, resumed).
#[tokio::test]
async fn another_account_of_the_same_cli_takes_its_conversation() {
    use prism_core::config::AgentAccount;
    let b = Bench::new().with_clis();
    let (perso, pro) = (b.root.join(".claude"), b.root.join(".claude-pro"));
    let mut cfg = b.agents.inner.settings.lock().unwrap().clone();
    cfg.accounts = vec![
        AgentAccount { name: "Claude perso".into(), cli: Provider::Claude, dir: perso.clone(), default: true },
        AgentAccount { name: "Claude pro".into(), cli: Provider::Claude, dir: pro.clone(), default: false },
    ];
    b.agents.configure(cfg);
    let id = b.agents.spawn(b.launch("Build")).await.unwrap();
    let started = b.tmux.calls_of("new-session").pop().unwrap();
    assert!(started.contains(&format!("CLAUDE_CONFIG_DIR={}", perso.display())), "{started:?}");
    assert_eq!(b.agents.record(&id).unwrap().launch.unwrap().account.as_deref(), Some("Claude perso"), "the default one, kept");
    // Its conversation, in its account's folder.
    let t = perso.join("projects/-project").join(format!("{id}.jsonl"));
    std::fs::create_dir_all(t.parent().unwrap()).unwrap();
    std::fs::write(&t, "{}\n").unwrap();

    let to = |account: &str, handoff: &str| switch::Switch { to: Provider::Claude, account: Some(account.into()), message: "Go on.".into(), handoff: handoff.into(), reason: Some("limit".into()), model: None, effort: None, permission_mode: None };
    assert!(b.agents.switch(&id, to("Claude perso", "resume")).await.is_err(), "it runs under it already");
    assert!(b.agents.switch(&id, to("Claude team", "resume")).await.is_err(), "no such account");
    b.agents.switch(&id, to("Claude pro", "resume")).await.unwrap();
    assert!(pro.join("projects/-project").join(format!("{id}.jsonl")).is_file(), "carried to its folder");
    let argv = b.tmux.argv(&id).unwrap();
    assert_eq!(argv[1..3], ["--resume".to_string(), id.clone()], "its own conversation, resumed");
    assert_eq!(argv.last().map(String::as_str), Some("Go on."));
    let respawned = b.tmux.calls_of("respawn-pane").pop().unwrap();
    assert!(respawned.contains(&format!("CLAUDE_CONFIG_DIR={}", pro.display())), "{respawned:?}");
    // Its former CLI stopped first (a shell in its place), then the copy, then the new account.
    assert!(b.tmux.calls_of("respawn-pane").len() >= 2, "stopped before its conversation is copied");
    // Before the last start, a folder not given then is taken out of the session's environment
    // (another CLI's), the one given kept.
    let calls = b.tmux.calls();
    let last = calls.iter().rposition(|c| c[0] == "respawn-pane").unwrap();
    let before = calls[..last].iter().rposition(|c| c[0] == "respawn-pane").unwrap();
    let unset: Vec<&str> = calls[before + 1..last].iter().filter(|c| c[0] == "set-environment").filter_map(|c| c.last().map(String::as_str)).collect();
    assert_eq!(unset, ["CODEX_HOME"]);
    let agent = b.shown(&id).await.unwrap();
    assert_eq!((agent.provider, agent.account.as_deref()), (Provider::Claude, Some("Claude pro")));
    let leg = &agent.legs[0];
    assert_eq!((leg.account.as_deref(), leg.handoff.as_str()), (Some("Claude perso"), "resume"));
    assert_eq!(b.agents.record(&id).unwrap().resumed.as_deref(), Some(id.as_str()), "what it held is counted once");
}

/// "In this session": a line sent back to the agent that saw it (its prompt typed at once: its CLI
/// queues it), done when that agent says so; a subject the user notes, the same; a closed agent's
/// line stays open.
#[tokio::test]
async fn an_aside_goes_back_to_its_session() {
    let b = Bench::new();
    b.agents.inner.settings.lock().unwrap().aside.enabled = true;
    let id = b.agents.spawn(b.launch("Worker")).await.unwrap();
    let mut a = b.shown(&id).await.unwrap();
    a.status = SessionStatus::Running;
    b.hub.update(|s| {
        s.agents = vec![a.clone()];
        true
    });
    let s = |title: &str| crate::asides::Suggest { title: title.into(), tldr: "t".into(), prompt: "p".into(), files: vec![format!("src/{title}.rs")], ..Default::default() };
    let task = b.agents.suggest(Some(&id), "claude", s("Remove the dead helper")).unwrap()["task_id"].as_u64().unwrap();
    let before = pasted(&b.tmux);
    b.agents.aside_session(task).await.unwrap();
    let line = b.agents.aside(task).unwrap();
    assert_eq!((line.state, line.agent.as_deref()), (state::AsideState::Session, Some(id.as_str())));
    assert_eq!(pasted(&b.tmux), before + 1, "typed at once, while it works");
    assert!(b.agents.aside_session(task).await.is_err(), "dealt with already");
    assert_eq!(b.agents.suggest(Some(&id), "claude", s("Remove the dead helper")).unwrap()["already"], "in_session");
    assert!(b.agents.done_by_agent(Some("other"), task, None).is_err(), "not given to it");
    b.agents.done_by_agent(Some(&id), task, Some("removed".into())).unwrap();
    let line = b.agents.aside(task).unwrap();
    assert_eq!((line.state, line.why.as_deref()), (state::AsideState::Done, Some("removed")));

    let note = b.agents.aside_note(&id, "Rename the helper\nIts name misleads").await.unwrap();
    let line = b.agents.aside(note).unwrap();
    assert_eq!((line.title.as_str(), line.state, line.seen[0].cli.as_str()), ("Rename the helper", state::AsideState::Session, "user"));
    b.agents.aside_action(note, "done", None, None).unwrap();
    assert_eq!(b.agents.aside(note).unwrap().state, state::AsideState::Done);

    // Held behind its menu: taken back, its prompt leaves the inbox and the agent hears nothing.
    let held = b.agents.suggest(Some(&id), "claude", s("Rename the flag")).unwrap()["task_id"].as_u64().unwrap();
    a.status = SessionStatus::WaitingTool;
    b.hub.update(|s| {
        s.agents = vec![a.clone()];
        true
    });
    let _ = b.agents.take_news(&id);
    b.agents.aside_session(held).await.unwrap();
    assert_eq!(b.hub.subscribe().borrow().inbox.get(&id), Some(&1));
    b.agents.aside_action(held, "reopen", None, None).unwrap();
    assert!(!b.hub.subscribe().borrow().inbox.contains_key(&id), "out of the inbox");
    assert!(b.agents.take_news(&id).is_empty(), "nothing typed: nothing to tell");
    // Typed already: the agent is told to leave it.
    a.status = SessionStatus::Running;
    b.hub.update(|s| {
        s.agents = vec![a.clone()];
        true
    });
    b.agents.aside_session(held).await.unwrap();
    b.agents.aside_action(held, "reopen", None, None).unwrap();
    let told = b.agents.take_news(&id);
    assert!(told.len() == 1 && told[0].contains("took it back"), "{told:?}");

    // Prepared, never started: it cannot take it, the line stays open.
    let other = b.agents.suggest(Some(&id), "claude", s("Update the stale doc")).unwrap()["task_id"].as_u64().unwrap();
    a.state = state::AgentState::Prepared;
    b.hub.update(|s| {
        s.agents = vec![a.clone()];
        true
    });
    assert!(b.agents.aside_session(other).await.is_err(), "not started");
    assert!(b.agents.aside_note(&id, "Something").await.is_err());
    assert_eq!(b.agents.aside(other).unwrap().state, state::AsideState::Open);
}
