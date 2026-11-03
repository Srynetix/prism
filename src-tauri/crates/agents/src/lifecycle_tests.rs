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
            model: None,
            effort: None,
            then: Vec::new(),
            from: None,
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
    let to = |p: Provider, message: &str| switch::Switch { to: p, message: message.into(), handoff: "prism".into(), reason: Some("limit".into()), model: None, effort: None, permission_mode: None };
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
    switch::Switch { to, message: message.into(), handoff: "prism".into(), reason: None, model: None, effort: None, permission_mode: None }
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
    };
    assert_eq!(
        b.agents.command_line(preview(Provider::Claude, "plan", "Why's it slow?")).unwrap(),
        "claude --add-dir …/api.worktrees/fix-it --model opus --session-id … -n 'Fix it' --settings … --permission-mode plan --worktree fix-it -- 'Why'\\''s it slow?'"
    );
    assert_eq!(b.agents.command_line(preview(Provider::Codex, "auto", "go")).unwrap(), "codex --add-dir …/api.worktrees/fix-it --model opus --approve-for-me -- go");
    assert_eq!(b.agents.command_line(preview(Provider::OpenCode, "default", "")).unwrap(), "opencode --model opus");
    assert!(b.agents.command_line(preview(Provider::CommandCode, "auto", "go")).is_err(), "no auto mode");
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
