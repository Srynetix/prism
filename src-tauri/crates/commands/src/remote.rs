//! Prism's side of the remote (`prism-remote`): the commands a phone may call, answered by the
//! window's own (`commands`), their classes (`commands::remote`), and the page's files from the
//! app (`Shell::asset`).

use prism_base::{i18n::tr, tr};
use anyhow::{bail, Result};
use futures::future::BoxFuture;
use serde_json::Value;
use std::sync::{Arc, OnceLock, Weak};

use crate::{
    app::AppState,
    commands::{self, args, reply, State},
};

/// The remote's host: the state, once started (the remote starts with it).
#[derive(Clone, Default)]
pub struct Host(Arc<OnceLock<Weak<AppState>>>);

impl Host {
    /// Answers the phones from `state` on.
    pub(crate) fn serve(&self, state: &Arc<AppState>) {
        let _ = self.0.set(Arc::downgrade(state));
    }

    fn state(&self) -> Result<Arc<AppState>> {
        self.0.get().and_then(Weak::upgrade).ok_or_else(|| anyhow::anyhow!(tr("Prism is starting")))
    }
}

/// Commands never answered from a phone, biometrics or not: the terminal (`agent_write`,
/// `agent_resize`) has its socket; `agent_run` is answered only for a task the agent's folder
/// declares.
#[cfg_attr(not(test), allow(dead_code))]
pub const NOT_ROUTED: &[&str] = &["agent_write", "agent_resize"];

/// A launch from a phone: in a new worktree, or in a worktree or an agent's folder Prism already
/// shows; in a mode where the agent still asks before running a command (not `auto`).
fn phone_launch(snap: &crate::state::Snapshot, launch: &crate::agents::Launch) -> Result<()> {
    if let Some(mode) = launch.permission_mode.as_deref()
        && !matches!(mode, "default" | "plan" | "acceptEdits")
    {
        bail!(tr!("from a phone, an agent starts in default, plan or acceptEdits mode, not {mode}", mode = mode));
    }
    if launch.worktree.is_none()
        && let Some(cwd) = &launch.cwd
        && !shown_folder(snap, &cwd.display().to_string())
    {
        bail!(tr("from a phone, an agent starts in a new worktree or a folder Prism shows"));
    }
    Ok(())
}

/// The folders the launcher offers: a worktree, a repository's clone, an agent's folder.
fn shown_folder(snap: &crate::state::Snapshot, cwd: &str) -> bool {
    snap.worktrees.iter().any(|w| w.path == cwd) || snap.repos.iter().any(|r| r.path == cwd) || snap.agents.iter().any(|a| a.cwd == cwd)
}

/// A new launch's "Then" steps from a phone (one prepared at the desk keeps its own): the
/// launcher's kinds only, and a task step is a line its folder declares, as
/// `agent_run`'s from a phone — never a command line the page would write.
async fn phone_then(state: &AppState, launch: &crate::agents::Launch) -> Result<()> {
    let tasks = if launch.then.iter().any(|s| s.kind == crate::state::StepKind::Task) {
        let dir = {
            let rx = state.hub.subscribe();
            let snap = rx.borrow();
            let cwd = launch.cwd.as_ref().filter(|_| launch.worktree.is_none()).map(|p| p.display().to_string());
            let repo = launch.worktree.as_ref().and_then(|w| w.repo.clone());
            commands::agents::launch_dir(&snap, cwd.as_deref(), repo.as_deref())
        };
        match dir {
            Some(dir) => commands::agents::launch_task_lines(state, &dir).await,
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    then_allowed(&launch.then, &tasks)
}

fn then_allowed(then: &[crate::state::LaunchStep], tasks: &[String]) -> Result<()> {
    for step in then {
        let command = step.command.as_deref().unwrap_or_default();
        if step.kind == crate::state::StepKind::Task && !tasks.iter().any(|c| c == command) {
            bail!(tr!("from a phone, a “Then” task is one its folder declares, not {command}", command = command));
        }
    }
    Ok(())
}

/// A key a phone may not press for an agent (its shells, behind the passkey, take any): only a
/// choice's digit, Enter and Esc. The arrows and Tab would reach a permission's choices past
/// "once" (Down, Enter), shift+tab cycle its permission modes up to auto. Waiting for a
/// permission, a digit other than 1 is refused too.
fn refused_key(snap: &crate::state::Snapshot, a: &Value) -> bool {
    let (id, key) = (a.get("id").and_then(Value::as_str).unwrap_or_default(), a.get("key").and_then(Value::as_str).unwrap_or_default());
    if id.starts_with(crate::agents::SHELL) {
        return false;
    }
    let digit = key.len() == 1 && key.chars().all(|c| c.is_ascii_digit());
    if !digit && key != "Enter" && key != "Escape" {
        return true;
    }
    digit && key != "1" && snap.agents.iter().any(|x| x.id == id && x.status == crate::state::SessionStatus::WaitingTool)
}

/// The pods asked for (`workload_pods`' deployment, `pod_log`'s pod) are a workload's of an
/// environment the Environments block shows: a phone reads no other context, namespace or pod
/// the kube config reaches (a production's logs, its secrets in them).
fn shown_workload(snap: &crate::state::Snapshot, a: &Value) -> bool {
    let field = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default();
    let (context, namespace) = (field("context"), field("namespace"));
    let envs = snap.environments.iter().flat_map(|e| &e.envs).filter(|e| e.context == context && e.namespace == namespace);
    let workloads: Vec<&str> = envs.flat_map(|e| &e.workloads).map(|w| w.name.as_str()).collect();
    match (a.get("deployment").and_then(Value::as_str), a.get("pod").and_then(Value::as_str)) {
        (Some(deployment), _) => workloads.contains(&deployment),
        // A pod's name as Kubernetes makes them (DNS-1123): nothing of a path reaches the API.
        (_, Some(pod)) => workloads.iter().any(|w| pod.strip_prefix(w).is_some_and(|rest| rest.starts_with('-') && rest.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.'))),
        _ => false,
    }
}

/// The commands that read a file of an agent's folder by its path (`path`, `paths`).
const FILE_READS: [&str; 4] = ["agent_file_text", "agent_file_preview", "agent_page_text", "agent_files_download"];

/// The hidden paths asked for (`.env`, `.github/ci.yml`, `.git/config`): a file or a folder whose
/// name starts with a dot.
fn hidden_paths(a: &Value) -> Vec<&str> {
    let hidden = |p: &&str| p.split(['/', '\\']).any(|part| part.starts_with('.') && part != ".");
    let one = a.get("path").and_then(Value::as_str);
    let many = a.get("paths").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str);
    one.into_iter().chain(many).filter(hidden).collect()
}

/// A secret a phone reads only with its passkey: a hidden file of an agent's folder that git does
/// not track (`.env`, a key), or git's own (`.git/config`, a token in a remote's address). A
/// tracked one (`.github/…`, `.gitlab-ci.yml`) is the repository's, the team's already.
fn secret_file(state: &AppState, command: &str, a: &Value) -> bool {
    if !FILE_READS.contains(&command) {
        return false;
    }
    let hidden = hidden_paths(a);
    if hidden.is_empty() {
        return false;
    }
    if hidden.iter().any(|p| p.split(['/', '\\']).any(|part| part == ".git")) {
        return true;
    }
    let id = a.get("id").and_then(Value::as_str).unwrap_or_default();
    let Ok(dir) = state.agents.dir(id) else { return true };
    // `agent_file_text`'s path is from the repository's top (the diff's), the others' from the folder.
    let magic = if command == "agent_file_text" { "top,literal" } else { "literal" };
    let specs: Vec<String> = hidden.iter().map(|p| format!(":({magic}){p}")).collect();
    let mut args = vec!["ls-files", "--error-unmatch", "--"];
    args.extend(specs.iter().map(String::as_str));
    prism_base::git::run_blocking(&dir, &args).is_err()
}

/// The pod whose usage is asked for (`pod_usage_history`) is one of a workload the Environments
/// block shows, or of the Pods block, whose usage of the moment the phone already has.
fn usage_shown(snap: &crate::state::Snapshot, a: &Value) -> bool {
    let field = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default();
    shown_workload(snap, a) || snap.pods.iter().any(|p| p.context == field("context") && p.namespace == field("namespace") && p.name == field("pod"))
}

async fn dispatch(state: Arc<AppState>, command: String, a: Value) -> Result<Value> {
    use commands::{agents as ag, feedback as fb, infra as inf};
    // The window's guard (`commands::handler`), which a phone's calls do not go through.
    if commands::refused_in_demo(&command) {
        bail!(crate::demo::READ_ONLY);
    }
    match command.as_str() {
        // Answered first: the relaunch closes the socket, and the phone reconnects to the new Prism.
        "restart_app" => {
            prism_base::rt::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                state.shell.restart()
            });
            Ok(Value::Null)
        }
        "workload_pods" | "pod_log" if !shown_workload(&State(&state).hub.subscribe().borrow(), &a) => {
            bail!(tr("from a phone, only the pods of a workload the Environments block shows"))
        }
        // A pod's usage: also a Pods block's, whose usage of the moment the phone already has.
        "pod_usage_history" if !usage_shown(&State(&state).hub.subscribe().borrow(), &a) => {
            bail!(tr("from a phone, only the usage of a pod the Pods or Environments block shows"))
        }
        // A task of the agent's folder, its line as detected, never one the page would write.
        "agent_run" => {
            #[derive(serde::Deserialize)]
            struct A {
                id: String,
                n: Option<u32>,
                command: String,
            }
            let A { id, n, command } = args(a)?;
            let task = state.agents.tasks(&id).await?.into_iter().flat_map(|s| s.tasks).find(|t| t.command == command);
            let Some(task) = task else { bail!(tr("from a phone, only a task of the agent's folder runs")) };
            reply(ag::agent_run(State(&state), id, n, task.command, !task.args, false).await)
        }
        "rescan_vulnerabilities" => {
            inf::rescan_vulnerabilities();
            Ok(Value::Null)
        }
        "agent_spawn" => {
            #[derive(serde::Deserialize)]
            struct A {
                launch: crate::agents::Launch,
            }
            let A { launch } = args(a)?;
            phone_launch(&State(&state).hub.subscribe().borrow(), &launch)?;
            phone_then(&State(&state), &launch).await?;
            reply(ag::agent_spawn(State(&state), launch).await)
        }
        "agent_prepare" => {
            #[derive(serde::Deserialize)]
            struct A {
                launch: crate::agents::Launch,
            }
            let A { launch } = args(a)?;
            phone_launch(&State(&state).hub.subscribe().borrow(), &launch)?;
            phone_then(&State(&state), &launch).await?;
            reply(ag::agent_prepare(State(&state), launch).await)
        }
        "agent_start" => {
            #[derive(serde::Deserialize)]
            struct A {
                id: String,
            }
            let A { id } = args(a)?;
            // Its launch passes the same checks as one made from the phone; its steps, set when it
            // was prepared, run once it starts.
            let launch = State(&state).hub.subscribe().borrow().agents.iter().find(|x| x.id == id).and_then(|x| x.launch.clone());
            if let Some(l) = launch {
                phone_launch(&State(&state).hub.subscribe().borrow(), &crate::agents::Launch::from(l))?;
            }
            reply(ag::agent_start(State(&state), id).await)
        }
        "feedback_body" => Ok(Value::String({
            #[derive(serde::Deserialize)]
            struct A {
                text: String,
                context: Vec<String>,
            }
            let A { text, context } = args(a)?;
            fb::feedback_body(text, context)
        })),
        "feedback_draft" => Ok(serde_json::to_value(fb::feedback_draft(State(&state)))?),
        // A permission's choices past "once" ("don't ask again", "no, and…") are answered at the
        // desk: from a phone, Allow once, Esc, or the terminal with its passkey.
        // A folder's tasks are listed by running its task runner there: one Prism shows only.
        "launch_tasks" if a.get("cwd").and_then(Value::as_str).is_some_and(|cwd| !shown_folder(&State(&state).hub.subscribe().borrow(), cwd)) => {
            bail!(tr("from a phone, the tasks of a folder Prism shows only"))
        }
        // Auto mode runs its commands without asking: never from a phone, as at its launch.
        "agent_plan_answer" if a.get("how").and_then(Value::as_str) == Some("approve_auto") => {
            bail!(tr("from a phone, a plan is not approved in auto mode: at the desk"))
        }
        "agent_key" if refused_key(&State(&state).hub.subscribe().borrow(), &a) => {
            bail!(tr("from a phone, an agent takes a choice's digit, Enter or Esc, a permission Allow once or Esc: the rest at the desk"))
        }
        "worktree_remove" => {
            #[derive(serde::Deserialize)]
            struct A {
                path: String,
                force: Option<bool>,
            }
            let A { path, force } = args(a)?;
            // Never forced from a phone: a worktree with something to lose stays.
            if force == Some(true) {
                bail!(tr("from a phone, a worktree is only removed when there is nothing to lose"));
            }
            reply(ag::worktree_remove(State(&state), path, None).await)
        }
        // The others as the window calls them, their arguments from the table (`[phone: …]`).
        other => commands::phone_call(&state, other, a).await,
    }
}

/// What another Prism's window never gets answered here: what opens something on this machine's
/// own screen (an editor, the Finder, a calendar), writes where a dialog of the other machine said,
/// restarts or powers this machine, or updates its Prism; and the window's own (`DESK`), which that
/// window's app answers.
pub const PEER_NEVER: &[&str] = &[
    "open_repo",
    "open_logs",
    "open_config",
    "mail_open",
    "open_in_calendar",
    "agent_open",
    "agent_open_file",
    "agent_open_external",
    "agent_files_export",
    "save_csv",
    "mac_power",
    "update_install",
    // The snapshot comes over the peer's socket.
    "snapshot_sync",
];

/// A call from another Prism's window (`prism_remote::peers`): any command of the window but its
/// own and `PEER_NEVER`, answered as this machine's window would (the demo's guard included).
async fn peer_dispatch(state: Arc<AppState>, command: String, a: Value) -> Result<Value> {
    if commands::refused_in_demo(&command) {
        bail!(crate::demo::READ_ONLY);
    }
    if commands::DESK.contains(&command.as_str()) || PEER_NEVER.contains(&command.as_str()) {
        bail!(tr!("{command} is not answered from another Prism", command = command));
    }
    if command == "restart_app" {
        prism_base::rt::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            state.shell.restart()
        });
        return Ok(Value::Null);
    }
    commands::peer_call(&state, &command, a).await
}

impl prism_remote::Host for Host {
    fn peer_call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>> {
        let host = self.clone();
        Box::pin(async move { peer_dispatch(host.state()?, command, args).await })
    }

    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>> {
        let host = self.clone();
        Box::pin(async move { dispatch(host.state()?, command, args).await })
    }

    fn class(&self, command: &str, args: &Value) -> Option<prism_core::remote::Class> {
        if command == "block_call" {
            let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or_default();
            return Some(commands::remote::block_class(s("block"), s("command"), args.get("args").unwrap_or(&Value::Null)));
        }
        // Prism still starting: nothing it cannot class yet (a hidden file would pass as Read).
        let state = self.state().ok()?;
        if secret_file(&state, command, args) {
            return Some(prism_core::remote::Class::Critical);
        }
        commands::remote::class(command)
    }

    fn profiles(&self) -> BoxFuture<'static, Vec<prism_core::profile::Profile>> {
        Box::pin(prism_core::profile::list())
    }

    fn open_profile(&self, id: &str) -> Result<()> {
        prism_core::profile::launch(id)
    }

    fn watch(&self, agent: String, size: Option<(u16, u16)>, control: bool, on_data: Box<dyn Fn(Vec<u8>) + Send>, on_end: Box<dyn FnOnce() + Send>) -> BoxFuture<'static, Result<prism_remote::Watch>> {
        let host = self.clone();
        Box::pin(async move {
            let state = host.state()?;
            // The demo replays its agents' recorded screens, and types in nothing.
            if crate::demo::active() {
                let screen = crate::demo::details::terminal(&state.hub.subscribe().borrow(), &agent)?;
                on_data(screen);
                return Ok(prism_remote::Watch { cols: 120, rows: 40, term: Box::new(prism_remote::ReadOnly(())) });
            }
            match (size, control) {
                (Some((cols, rows)), true) => {
                    let attachment = state.agents.control(&agent, cols, rows, on_data, on_end)?;
                    Ok(prism_remote::Watch { cols, rows, term: Box::new(Typed(attachment)) })
                }
                (Some(_), false) => {
                    let (attachment, cols, rows) = state.agents.watch(&agent, size, on_data, on_end).await?;
                    Ok(prism_remote::Watch { cols, rows, term: Box::new(Watched(attachment)) })
                }
                (None, _) => {
                    let (attachment, cols, rows) = state.agents.watch(&agent, None, on_data, on_end).await?;
                    Ok(prism_remote::Watch { cols, rows, term: Box::new(prism_remote::ReadOnly(attachment)) })
                }
            }
        })
    }

    fn page(&self, path: &str) -> std::result::Result<(Vec<u8>, String), (u16, String)> {
        let state = self.state().map_err(|e| (503, e.to_string()))?;
        state.agents.serve(path).map(|(body, mime)| (body, mime.to_string()))
    }

    fn asset(&self, path: &str) -> Option<(Vec<u8>, String)> {
        self.state().ok()?.shell.asset(path)
    }
}

/// A terminal a phone types in.
struct Typed(crate::agents::Attachment);

impl prism_remote::Term for Typed {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.0.write(data)
    }
    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.0.resize(cols, rows)
    }
}

/// A terminal a phone watches at its own size: no key reaches it, the phone's turns resize it.
struct Watched(crate::agents::Attachment);

impl prism_remote::Term for Watched {
    fn write(&mut self, _: &[u8]) -> Result<()> {
        bail!(tr("this terminal is read-only"))
    }
    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.0.resize(cols, rows)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use prism_core::remote::Class;

    /// The commands a phone gets answered: the table's (`[phone: …]`), and `dispatch`'s own arms.
    fn routed() -> BTreeSet<&'static str> {
        let source = include_str!("remote.rs");
        let body = &source[source.find("async fn dispatch").unwrap()..source.find("impl prism_remote::Host").unwrap()];
        let own = body.lines().filter_map(|l| l.trim().strip_prefix('"')?.split_once("\" =>").map(|(name, _)| name));
        own.chain(crate::commands::PHONE_ROUTED.iter().copied()).collect()
    }

    /// A peer gets answered every command of the window but its own and those it never is.
    #[test]
    fn a_peer_reaches_every_command_but_the_desks() {
        for name in super::PEER_NEVER {
            assert!(crate::commands::COMMANDS.iter().any(|(n, _, _)| n == name), "{name}: no such command");
        }
    }

    #[test]
    fn tells_the_hidden_paths_asked_for() {
        let hidden = |v: serde_json::Value| super::hidden_paths(&v).into_iter().map(String::from).collect::<Vec<_>>();
        assert_eq!(hidden(serde_json::json!({ "path": ".env" })), [".env"]);
        assert_eq!(hidden(serde_json::json!({ "path": "app/.git/config" })), ["app/.git/config"]);
        assert_eq!(hidden(serde_json::json!({ "paths": ["src", "../.ssh/id_ed25519"] })), ["../.ssh/id_ed25519"]);
        assert!(hidden(serde_json::json!({ "path": "./src/main.rs" })).is_empty());
        assert!(hidden(serde_json::json!({ "paths": ["src", "README.md"] })).is_empty());
    }

    #[test]
    fn a_phone_launches_in_a_known_folder_and_asking() {
        let launch = |v: serde_json::Value| serde_json::from_value::<crate::agents::Launch>(v).unwrap();
        let snap = crate::state::Snapshot::default();
        let ok = |v| super::phone_launch(&snap, &launch(v)).is_ok();
        assert!(ok(serde_json::json!({ "title": "Fix", "worktree": {} })), "a new worktree");
        assert!(ok(serde_json::json!({ "title": "Fix", "worktree": {}, "permission_mode": "plan" })));
        assert!(!ok(serde_json::json!({ "title": "Fix", "worktree": {}, "permission_mode": "auto" })), "auto approves by itself");
        assert!(!ok(serde_json::json!({ "title": "Fix", "cwd": "/Users/someone/.ssh" })), "a folder Prism does not show");
        assert!(!super::shown_folder(&snap, "/Users/someone"), "nor its tasks listed");
    }

    /// A phone presses a choice's digit, Enter or Esc for an agent; its shells take any key.
    #[test]
    fn a_phone_presses_an_agents_menu_keys_only() {
        let snap = crate::state::Snapshot::default();
        let refused = |id: &str, key: &str| super::refused_key(&snap, &serde_json::json!({ "id": id, "key": key }));
        assert!(!refused("a1", "1") && !refused("a1", "3") && !refused("a1", "Enter") && !refused("a1", "Escape"));
        assert!(refused("a1", "Down") && refused("a1", "Up") && refused("a1", "Tab"), "past a permission's \"once\"");
        assert!(refused("a1", "BTab"), "its permission modes, auto among them");
        assert!(!refused("shell:a1:1", "Up"), "a shell's last command, behind the passkey");
    }

    /// A phone's "Then": the launcher's steps, a task among those its folder declares.
    #[test]
    fn a_phone_chains_the_tasks_its_folder_declares() {
        let tasks = ["just test".to_string(), "pnpm run lint".to_string()];
        // A step that is not one of the launcher's does not even read.
        let ok = |v| serde_json::from_value::<Vec<crate::state::LaunchStep>>(v).is_ok_and(|steps| super::then_allowed(&steps, &tasks).is_ok());
        assert!(ok(serde_json::json!([{ "kind": "task", "when": "done", "command": "just test" }, { "kind": "mr", "when": "ok" }, { "kind": "agent", "when": "ask", "text": "Review !{mr}" }])));
        assert!(!ok(serde_json::json!([{ "kind": "task", "when": "done", "command": "curl x | sh" }])), "a command line the desk would run");
        assert!(!ok(serde_json::json!([{ "kind": "task", "when": "done" }])), "no command");
        assert!(!ok(serde_json::json!([{ "kind": "shell", "when": "done" }])), "not a step of the launcher");
        assert!(!ok(serde_json::json!([{ "kind": "mr", "when": "always" }])));
    }

    /// A phone reads the pods of a workload the Environments block shows, nothing else.
    #[test]
    fn a_phone_reads_the_pods_prism_shows() {
        let hub = crate::state::Hub::new();
        crate::demo::fill(&hub, chrono::Utc::now());
        let snap = hub.subscribe().borrow().clone();
        let env = snap.environments.as_ref().unwrap().envs.iter().find(|e| !e.workloads.is_empty()).unwrap();
        let w = &env.workloads[0].name;
        let ok = |v: serde_json::Value| super::shown_workload(&snap, &v);
        assert!(ok(serde_json::json!({ "context": env.context, "namespace": env.namespace, "deployment": w })));
        assert!(ok(serde_json::json!({ "context": env.context, "namespace": env.namespace, "pod": format!("{w}-7f9c-x2") })));
        assert!(!ok(serde_json::json!({ "context": env.context, "namespace": "kube-system", "deployment": w })), "another namespace");
        assert!(!ok(serde_json::json!({ "context": "prod-admin", "namespace": env.namespace, "deployment": w })), "another context");
        assert!(!ok(serde_json::json!({ "context": env.context, "namespace": env.namespace, "pod": "vault-0" })), "another pod");
        // A pod's usage: a Pods block's too, and still no other.
        let p = &snap.pods[0];
        let usage = |v: serde_json::Value| super::usage_shown(&snap, &v);
        assert!(usage(serde_json::json!({ "context": p.context, "namespace": p.namespace, "pod": p.name })));
        assert!(usage(serde_json::json!({ "context": env.context, "namespace": env.namespace, "pod": format!("{w}-7f9c-x2") })));
        assert!(!usage(serde_json::json!({ "context": p.context, "namespace": "kube-system", "pod": p.name })), "another namespace");
    }

    /// Every command that reads or acts is answered from a phone, but the snapshot and the
    /// terminal, which the remote answers itself; nothing else is (critical, never).
    #[test]
    fn the_reads_and_only_them_are_routed() {
        let routed = routed();
        let own = ["get_snapshot", "agent_attach", "agent_detach"];
        let phone = |n: &str| matches!(crate::commands::remote::class(n), Some(Class::Read | Class::Act | Class::Critical)) && !super::NOT_ROUTED.contains(&n);
        for name in &routed {
            if *name != "block_call" {
                assert!(phone(name), "{name} is routed but is never done from a phone");
            }
        }
        assert!(routed.len() > 80, "{} commands read in dispatch", routed.len());
        for (name, _, _) in crate::commands::COMMANDS.iter().filter(|(n, _, _)| phone(n)) {
            assert!(routed.contains(name) || own.contains(name), "{name} may be done from a phone but is not routed (remote.rs)");
        }
    }

    /// `route!` passes a phone's arguments to the command by position: the table's (`[phone: …]`)
    /// must be named and ordered as the command's own, or two of a kind (a task and a comment id)
    /// would swap silently.
    #[test]
    fn a_routed_command_gets_its_arguments_in_its_order() {
        let sources = [
            include_str!("commands/agents.rs"),
            include_str!("commands/config.rs"),
            include_str!("commands/feedback.rs"),
            include_str!("commands/forge.rs"),
            include_str!("commands/infra.rs"),
            include_str!("commands/integrations.rs"),
            include_str!("commands/system.rs"),
            include_str!("commands/tracker.rs"),
        ]
        .concat();
        let names = |list: &str| -> Vec<String> {
            let mut depth = 0;
            let mut parts = vec![String::new()];
            for c in list.chars() {
                match c {
                    '<' | '(' => depth += 1,
                    '>' | ')' => depth -= 1,
                    ',' if depth == 0 => parts.push(String::new()),
                    _ => {}
                }
                if c != ',' || depth != 0 {
                    parts.last_mut().unwrap().push(c);
                }
            }
            parts.iter().filter_map(|p| p.split_once(':').map(|(n, _)| n.trim().to_string())).filter(|n| !n.is_empty() && n != "state" && !n.contains(' ')).collect()
        };
        let mut checked = 0;
        for line in include_str!("commands/table.rs").lines().filter(|l| l.contains("[phone: ")) {
            let function = line.trim().split(':').nth(2).unwrap().trim();
            let args = line.split_once("[phone: ").unwrap().1.trim_end_matches("],").trim_end_matches("sync").trim_end_matches("; ");
            let routed: Vec<String> = names(args.trim().trim_start_matches("state").trim_start_matches(','));
            let start = sources.find(&format!("fn {function}(")).unwrap_or_else(|| panic!("no command {function}"));
            let signature = &sources[start + function.len() + 4..];
            let mut depth = 1;
            let end = signature.char_indices().find(|&(_, c)| {
                depth += match c {
                    '(' => 1,
                    ')' => -1,
                    _ => 0,
                };
                depth == 0
            });
            let own: Vec<String> = names(&signature[..end.unwrap().0]);
            assert_eq!(routed, own, "{function}: routed with {routed:?}, takes {own:?}");
            checked += 1;
        }
        assert!(checked > 60, "{checked} routes read");
    }
}
