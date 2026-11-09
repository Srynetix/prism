//! Agents and their worktrees: launch, terminal, files, the worktrees view.


use prism_base::{i18n::tr, tr};
use super::State;

use crate::state::{AgentState, SessionOrigin};

use crate::app::AppState;

use super::Cmd;

/// Agents view: runs `claude` in a folder or a new worktree, with an optional starting prompt.
#[prism_macros::command]
pub async fn agent_spawn(state: State<'_, AppState>, launch: crate::agents::Launch) -> Cmd<String> {
    state.agents.spawn(launch).await.map_err(Into::into)
}

/// Agents view: declares an agent without starting it (Prepare); `agent_start` starts it.
#[prism_macros::command]
pub async fn agent_prepare(state: State<'_, AppState>, launch: crate::agents::Launch) -> Cmd<String> {
    state.agents.prepare(launch).await.map_err(Into::into)
}

/// Starts a prepared agent.
#[prism_macros::command]
pub async fn agent_start(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.start(&id).await.map_err(Into::into)
}

/// Launcher: the new worktree a launch would create, or why it cannot be; none in the demo.
#[prism_macros::command]
pub async fn agent_worktree_plan(state: State<'_, AppState>, title: String, worktree: crate::agents::NewWorktree) -> Cmd<Option<crate::agents::worktree::Plan>> {
    if crate::demo::active() {
        return Ok(None);
    }
    Ok(Some(state.agents.plan_worktree(&title, &worktree).await?.1))
}

/// Removes a stopped agent's worktree, if there is nothing to lose.
#[prism_macros::command]
pub async fn agent_remove_worktree(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.remove_worktree(&id).await.map_err(Into::into)
}

/// Worktrees view: removes a worktree and its local branch, if there is nothing to lose and
/// nobody is working in it (agent running, non-archived desktop session). If the branch's MR
/// is merged, its commits from before the merge count as delivered (squash). `force`: removed
/// even with changes or unpushed commits, at my request; never when a session
/// is working in it.
#[prism_macros::command]
pub async fn worktree_remove(state: State<'_, AppState>, path: String, force: Option<bool>) -> Cmd {
    let force = force.unwrap_or(false);
    let merged_at = {
        let snap = state.hub.subscribe();
        let snap = snap.borrow();
        let w = snap.worktrees.iter().find(|w| w.path == path).ok_or(tr!("{path} is not a worktree of the repository", path = path))?;
        if w.is_main {
            return Err(tr("a repository's main checkout is not removed").into());
        }
        if w.locked {
            return Err(tr("this worktree is locked (git worktree unlock)").into());
        }
        let inside = |dir: &str| dir == path || dir.starts_with(&format!("{path}/"));
        if let Some(a) = snap.agents.iter().find(|a| a.state == AgentState::Running && (a.worktree.as_deref() == Some(&path) || a.adopted.contains(&path) || inside(&a.cwd))) {
            return Err(tr!("the agent “{title}” is running in it: stop it first", title = a.title).into());
        }
        if let Some(c) = snap.sessions.iter().find(|c| c.origin == SessionOrigin::Desktop && c.worktree.as_deref().is_some_and(inside)) {
            return Err(tr!("the Claude Desktop session “{title}” uses it: archive it first", title = c.title).into());
        }
        snap.merged_at(w)
    };
    // Its shells (Worktrees view) would be left in a folder that no longer exists.
    state.agents.close_folder_shells(&path).await;
    let dir = std::path::PathBuf::from(&path);
    match tokio::task::spawn_blocking(move || crate::agents::worktree::remove(&dir, merged_at, force)).await? {
        Ok(Ok(())) => {
            state.hub.update(|s| {
                s.worktrees.retain(|w| w.path != path);
                true
            });
            Ok(())
        }
        Ok(Err(reason)) => Err(tr!("kept: {reason}", reason = reason).into()),
        Err(e) => Err(e.into()),
    }
}

/// Worktrees view: `git worktree prune`, for worktrees whose folder has disappeared.
/// The branches a worktree can be switched to (origin fetched first).
#[prism_macros::command]
pub async fn worktree_branches(state: State<'_, AppState>, path: String) -> Cmd<Vec<crate::agents::vcs::BranchChoice>> {
    state.agents.worktree_branches(&path).await.map_err(Into::into)
}

/// A worktree's branch pulled, fast-forward only; what happened.
#[prism_macros::command]
pub async fn worktree_pull(state: State<'_, AppState>, path: String) -> Cmd<String> {
    state.agents.worktree_pull(&path).await.map_err(Into::into)
}

/// A worktree switched to another branch, nothing uncommitted and no agent running in it.
#[prism_macros::command]
pub async fn worktree_switch(state: State<'_, AppState>, path: String, branch: String) -> Cmd<String> {
    state.agents.worktree_switch(&path, &branch).await.map_err(Into::into)
}

#[prism_macros::command]
pub async fn worktree_prune(state: State<'_, AppState>) -> Cmd {
    // Every repository with a folder gone: from its main checkout.
    let repos: Vec<String> = {
        let snap = state.hub.subscribe();
        let wts = &snap.borrow().worktrees;
        wts.iter().filter(|w| w.is_main && wts.iter().any(|p| p.prunable && p.repo == w.repo)).map(|w| w.path.clone()).collect()
    };
    if repos.is_empty() {
        return Err(tr("no worktree to prune").into());
    }
    for repo in repos {
        crate::git::run(std::path::Path::new(&repo), &["worktree", "prune"]).await?;
    }
    state.hub.update(|s| {
        s.worktrees.retain(|w| !w.prunable);
        true
    });
    Ok(())
}

#[prism_macros::command]
pub async fn agent_resume(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.resume(&id).await.map_err(Into::into)
}

/// The last agent closed in a repository back in the list, resumed (Agents view by project, Reopen).
#[prism_macros::command]
pub async fn agent_reopen(state: State<'_, AppState>, repo: String) -> Cmd<String> {
    state.agents.reopen(&repo).await.map_err(Into::into)
}

/// Restarts Claude on its conversation, even mid-work.
#[prism_macros::command]
pub async fn agent_restart(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.restart(&id).await.map_err(Into::into)
}

#[prism_macros::command]
pub fn agent_detach(state: State<'_, AppState>, id: String) {
    state.agents.detach(&id);
}

/// Whether what the desk's terminal sends was typed: not a focus change, a mouse event or an
/// answer to the terminal's queries, which xterm sends by itself.
fn typed_by_hand(data: &str) -> bool {
    static AUTO: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let auto = AUTO.get_or_init(|| regex::Regex::new(r"^(\x1b\[[IO]|\x1b\[<[\d;]*[Mm]|\x1b\[M...|\x1b\[[?>]?[\d;]*[cnRt]|\x1b\][^\x07\x1b]*(\x07|\x1b\\))+$").unwrap());
    !data.is_empty() && !auto.is_match(data)
}

#[prism_macros::command]
pub fn agent_write(state: State<'_, AppState>, id: String, data: String) -> Cmd {
    // A key on the desk takes the terminal back from a phone typing in it; not what xterm sends
    // by itself (focus, mouse, answers to the terminal's queries).
    if typed_by_hand(&data) {
        state.remote.release(&id);
    }
    // A demo terminal is a recording: what is typed goes nowhere.
    if crate::demo::active() {
        return Ok(());
    }
    state.agents.write(&id, data.as_bytes()).map_err(Into::into)
}

#[prism_macros::command]
pub fn agent_resize(state: State<'_, AppState>, id: String, cols: u16, rows: u16) -> Cmd {
    if crate::demo::active() {
        return Ok(());
    }
    state.agents.resize(&id, cols, rows).map_err(Into::into)
}

#[prism_macros::command]
pub async fn agent_stop(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.stop(&id).await.map_err(Into::into)
}

/// An agent's terminal (or one of its shells) is in tmux's copy mode: the terminal says so.
#[prism_macros::command]
pub async fn agent_in_copy_mode(state: State<'_, AppState>, id: String) -> Cmd<bool> {
    state.agents.in_copy_mode(&id).await.map_err(Into::into)
}

/// Takes an agent's terminal (or one of its shells) out of tmux's copy mode, before a key.
#[prism_macros::command]
pub async fn agent_leave_copy_mode(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.leave_copy_mode(&id).await.map_err(Into::into)
}

/// One key of the agent's menu (a choice's digit, Enter, Esc, an arrow, Tab), pressed.
#[prism_macros::command]
pub async fn agent_key(state: State<'_, AppState>, id: String, key: String) -> Cmd {
    state.agents.key(&id, &key).await.map_err(Into::into)
}

/// Interrupts the agent's current turn (Esc).
#[prism_macros::command]
pub async fn agent_interrupt(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.interrupt(&id).await.map_err(Into::into)
}

/// Also opens the agent in the `[open_with]` terminal (the system's by default).
#[prism_macros::command]
pub async fn agent_open_external(state: State<'_, AppState>, id: String) -> Cmd {
    let app = state.hub.subscribe().borrow().open_with.terminal.clone().unwrap_or_else(|| crate::platform::DEFAULT_TERMINAL.into());
    state.agents.open_external(&id, &app).await.map_err(Into::into)
}

/// Answers the question the agent asks (its `AskUserQuestion`, `call`): the choices picked and
/// your words, as its menu's keys. From a phone, an answer with words is typing (its passkey).
#[prism_macros::command]
pub async fn agent_answer(state: State<'_, AppState>, id: String, call: String, answers: Vec<crate::agents::question::Answer>) -> Cmd {
    if crate::demo::active() {
        return Ok(());
    }
    state.agents.answer(&id, &call, &answers).await.map_err(Into::into)
}

/// Answers the plan the agent proposes (its `ExitPlanMode`, `call`): approved, or with your words —
/// refused, it plans again, or approved with them. From a phone, words are typing (its passkey).
#[prism_macros::command]
pub async fn agent_plan_answer(state: State<'_, AppState>, id: String, call: String, how: crate::agents::plan::How, text: Option<String>) -> Cmd {
    if crate::demo::active() {
        return Ok(());
    }
    state.agents.answer_plan(&id, &call, how, text.as_deref()).await.map_err(Into::into)
}

/// Grants once the permission the agent asks for (key 1 of Claude's question).
#[prism_macros::command]
pub async fn agent_allow(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.allow(&id).await.map_err(Into::into)
}

/// Renames an agent in Prism.
#[prism_macros::command]
pub fn agent_rename(state: State<'_, AppState>, id: String, title: String) -> Cmd {
    state.agents.rename(&id, &title).map_err(Into::into)
}

/// A file dropped, pasted or chosen for an agent's prompt (`id`) or for the launcher's, its bytes in
/// base64 (a third more than raw, where a JSON array of numbers is three and a half times: a phone's
/// photo fits in its socket's message): saved where the agent reads it; the path to type in its prompt.
/// Off the main thread: decoding 25 MB and writing it held the window 10 ms and more.
#[prism_macros::command(async)]
pub fn agent_attach_file(state: State<'_, AppState>, id: Option<String>, name: String, data: String) -> Cmd<String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).map_err(|e| tr!("{name}: unreadable data ({error})", name = name, error = e))?;
    state.agents.attach_file(id.as_deref(), &name, &bytes).map_err(Into::into)
}

/// Its Git bar: where its folder stands.
#[prism_macros::command]
pub async fn agent_git_state(state: State<'_, AppState>, id: String) -> Cmd<crate::agents::vcs::GitState> {
    if crate::demo::active() {
        return Err("no git in the demo".into());
    }
    state.agents.git_state(&id).await.map_err(Into::into)
}

/// Its Git bar: its changes committed (no hook runs), never on a protected branch; its short hash.
#[prism_macros::command]
pub async fn agent_git_commit(state: State<'_, AppState>, id: String, message: String) -> Cmd<String> {
    state.agents.git_commit(&id, &message).await.map_err(Into::into)
}

/// Its Git bar: its branch pushed (no hook runs), never a protected one.
#[prism_macros::command]
pub async fn agent_git_push(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.git_push(&id, false).await.map_err(Into::into)
}

/// Its Git bar: its branch pushed over origin's, after a rebase (`--force-with-lease`).
#[prism_macros::command]
pub async fn agent_git_force_push(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.git_push(&id, true).await.map_err(Into::into)
}

/// Its Git bar: brought up to date with its base (rebased; a conflict leaves it as it was).
#[prism_macros::command]
pub async fn agent_git_sync(state: State<'_, AppState>, id: String) -> Cmd<String> {
    state.agents.git_sync(&id).await.map_err(Into::into)
}

/// Its Files preview, edited: a text file written back, if unchanged since it was read.
#[prism_macros::command]
pub fn agent_file_save(state: State<'_, AppState>, id: String, path: String, original: String, text: String) -> Cmd {
    state.agents.save_file(&id, &path, &original, &text).map_err(Into::into)
}

/// A chain's MR step: the agent's branch pushed and a draft MR opened (never merged).
#[prism_macros::command]
pub async fn agent_open_mr(state: State<'_, AppState>, id: String) -> Cmd<crate::agents::chain::OpenedMr> {
    state.agents.open_mr(&id).await.map_err(Into::into)
}

/// What every agent cost, running or closed, the most recent first: the Actual sections' book,
/// read when they show and again when `Snapshot.agent_costs_rev` changes.
#[prism_macros::command]
pub fn agent_costs(state: State<'_, AppState>) -> Cmd<Vec<prism_core::state::AgentCost>> {
    if crate::demo::active() {
        return Ok(crate::demo::agent_costs(&state.hub.subscribe().borrow().agents));
    }
    Ok(state.agents.costs())
}

/// What an agent did (Activity tab): its hook log as a timeline, metadata only.
// Off the main thread (where Tauri runs a plain `fn`): it reads files, milliseconds each time.
#[prism_macros::command(async)]
pub fn agent_activity(state: State<'_, AppState>, id: String) -> Cmd<crate::agents::activity::Activity> {
    if crate::demo::active() {
        return Ok(crate::demo::details::activity(&id));
    }
    state.agents.activity(&id).map_err(Into::into)
}

/// What an agent's context is made of (its context gauge's detail): Claude Code's or Codex's,
/// from its transcript; sizes and names only.
#[prism_macros::command]
pub async fn agent_context_detail(state: State<'_, AppState>, id: String) -> Cmd<Option<crate::agents::breakdown::Breakdown>> {
    if crate::demo::active() {
        let context = state.hub.subscribe().borrow().agents.iter().find(|a| a.id == id).and_then(|a| a.context);
        return Ok(context.map(crate::demo::details::breakdown));
    }
    state.agents.breakdown(&id).await.map_err(Into::into)
}

/// The slash commands an agent's CLI takes, for its Chat box's autocomplete: names, descriptions
/// and argument hints only.
#[prism_macros::command]
pub async fn agent_slash_commands(state: State<'_, AppState>, id: String) -> Cmd<Vec<crate::agents::slash::SlashCommand>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    state.agents.slash_commands(&id).await.map_err(Into::into)
}

/// Settings: the models an agent CLI offers (`claude`, `codex`, `command-code`, `opencode`), for
/// its model field; any other name may be typed.
#[prism_macros::command]
pub async fn agent_models(state: State<'_, AppState>, cli: crate::agents::provider::Provider) -> Cmd<Vec<String>> {
    Ok(state.agents.models(cli).await)
}

/// The command line a launch would run, as the launcher shows it (`Agents::command_line`).
#[prism_macros::command]
pub fn agent_command_line(state: State<'_, AppState>, preview: crate::agents::LaunchPreview) -> Cmd<String> {
    state.agents.command_line(preview).map_err(Into::into)
}

/// The reasoning efforts an agent CLI takes (for `model`: Codex's are each model's), lowest
/// first, for Settings, the launcher and an agent's header; empty for a CLI without.
#[prism_macros::command]
pub async fn agent_efforts(state: State<'_, AppState>, cli: crate::agents::provider::Provider, model: Option<String>) -> Cmd<Vec<String>> {
    Ok(state.agents.efforts(cli, model.as_deref()).await)
}

/// Sets an agent's reasoning effort (`null`: its CLI's), kept for its resumes; true when it took
/// it at once (Claude Code at its prompt), false when it will from its next resume.
#[prism_macros::command]
pub async fn agent_set_effort(state: State<'_, AppState>, id: String, effort: Option<String>) -> Cmd<bool> {
    state.agents.set_effort(&id, effort).await.map_err(Into::into)
}

/// Compacts an agent's conversation (`/compact`), once its turn is over.
#[prism_macros::command]
pub async fn agent_compact(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.compact(&id).await.map_err(Into::into)
}

/// Clears an agent's conversation (`/clear`), once its turn is over.
#[prism_macros::command]
pub async fn agent_clear(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.clear(&id).await.map_err(Into::into)
}

/// Types a prompt in an agent's CLI (Diff tab › Send / Insert): `enter` submits it.
#[prism_macros::command]
pub async fn agent_prompt(state: State<'_, AppState>, id: String, text: String, enter: bool) -> Cmd {
    state.agents.prompt(&id, &text, enter).await.map_err(Into::into)
}

/// The prompt Claude Code suggests, sent as it is (read again off its screen first).
#[prism_macros::command]
pub async fn agent_send_suggestion(state: State<'_, AppState>, id: String, text: String) -> Cmd {
    state.agents.send_suggestion(&id, &text).await.map_err(Into::into)
}

/// A quick answer from a phone: the config's, filled on the computer and checked against what the
/// phone showed — no free text, so no passkey.
#[prism_macros::command]
pub async fn agent_quick_answer(state: State<'_, AppState>, id: String, label: String, text: String) -> Cmd {
    state.agents.quick_answer(&id, &label, &text).await.map_err(Into::into)
}

/// A text typed to an agent from a phone's answer box, and submitted: what `agent_prompt` does,
/// asked by typing (`[remote] typing`) and confirmed by the phone's passkey (the remote's server).
/// To one of its shells too (`shell:<agent>:<n>`), which `agent_prompt` never reaches.
#[prism_macros::command]
pub async fn agent_type(state: State<'_, AppState>, id: String, text: String) -> Cmd {
    state.agents.type_text(&id, &text).await.map_err(Into::into)
}

/// Sets an agent's tags (an empty list: none).
#[prism_macros::command]
pub fn agent_set_tags(state: State<'_, AppState>, id: String, tags: Vec<String>) -> Cmd {
    state.agents.set_tags(&id, tags).map_err(Into::into)
}

/// Links an agent to a ticket by hand (`None`: back to the one found in its title or branch).
#[prism_macros::command]
pub fn agent_link_ticket(state: State<'_, AppState>, id: String, ticket: Option<String>) -> Cmd {
    state.agents.link_ticket(&id, ticket).map_err(Into::into)
}

/// Links an agent to an MR by hand (`None`: back to its branch's).
#[prism_macros::command]
pub fn agent_link_mr(state: State<'_, AppState>, id: String, mr: Option<crate::state::MrLink>) -> Cmd {
    state.agents.link_mr(&id, mr).map_err(Into::into)
}

/// A kind or a tag renamed in the Agents view: the agents carrying it follow (`to` empty: taken off).
#[prism_macros::command]
pub fn agent_relabel(state: State<'_, AppState>, kind: crate::agents::Label, from: String, to: String) -> Cmd {
    state.agents.relabel(kind, &from, &to).map_err(Into::into)
}

/// Gives an agent a kind (`null`: none).
#[prism_macros::command]
pub fn agent_set_category(state: State<'_, AppState>, id: String, category: Option<String>) -> Cmd {
    state.agents.set_category(&id, category).map_err(Into::into)
}

#[prism_macros::command]
pub async fn agent_close(state: State<'_, AppState>, id: String) -> Cmd<Option<String>> {
    state.agents.close(&id).await.map_err(Into::into)
}

/// Opens an agent's folder (its worktree, or its folder) in the editor, the terminal or the Finder.
#[prism_macros::command]
pub async fn agent_open(state: State<'_, AppState>, id: String, with: String) -> Cmd {
    let dir = state.agents.dir(&id)?;
    let apps = state.hub.subscribe().borrow().open_with.clone();
    super::system::open_folder(&dir.display().to_string(), &with, apps).await
}

/// The agent's shells still open (its folder, kept by tmux).
#[prism_macros::command]
pub async fn agent_shells(state: State<'_, AppState>, id: String) -> Cmd<Vec<crate::agents::Shell>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    state.agents.shells(&id).await.map_err(Into::into)
}

/// Opens the agent's shell `n`, or a new one (`null`): its terminal key.
#[prism_macros::command]
pub async fn agent_shell(state: State<'_, AppState>, id: String, n: Option<u32>) -> Cmd<String> {
    if crate::demo::active() {
        return Ok(format!("{}{id}:{}", crate::agents::SHELL, n.unwrap_or(1)));
    }
    state.agents.shell(&id, n).await.map_err(Into::into)
}

/// An agent's conversation as a phone reads it (its Chat tab): its last `turns` (30), from its
/// transcript; null for a CLI without one. `have`: the signatures of the turns already held, not
/// sent again.
#[prism_macros::command]
pub async fn agent_conversation(state: State<'_, AppState>, id: String, turns: Option<u32>, have: Option<Vec<String>>) -> Cmd<Option<crate::agents::conversation::Conversation>> {
    let have = have.unwrap_or_default();
    if crate::demo::active() {
        let mut c = crate::demo::details::conversation(&id);
        c.only_new(&have);
        return Ok(Some(c));
    }
    state.agents.conversation(&id, turns.unwrap_or(30), &have).await.map_err(Into::into)
}

/// What a relay to another CLI is written from (Continue with…): the agent's task, plan, last
/// `turns` turns (8) and its branch's state.
#[prism_macros::command]
pub async fn agent_handoff_material(state: State<'_, AppState>, id: String, turns: Option<u32>) -> Cmd<crate::agents::switch::Material> {
    if crate::demo::active() {
        return Ok(demo_material(&state, &id));
    }
    state.agents.handoff_material(&id, turns.unwrap_or(8)).await.map_err(Into::into)
}

/// Trusts what the agent's CLI asks before it starts (its folder; Codex: Prism's hooks), from its
/// card; `parent`: its repository's worktrees' folder first, the next ones not asked.
#[prism_macros::command]
pub async fn agent_trust(state: State<'_, AppState>, id: String, kind: crate::state::GateKind, parent: Option<bool>) -> Cmd {
    state.agents.answer_gate(&id, kind, true, parent.unwrap_or(false)).await.map_err(Into::into)
}

/// Refuses what the agent's CLI asks before it starts: it exits.
#[prism_macros::command]
pub async fn agent_refuse_gate(state: State<'_, AppState>, id: String, kind: crate::state::GateKind) -> Cmd {
    state.agents.answer_gate(&id, kind, false, false).await.map_err(Into::into)
}

/// A past session as a quick action is made from it (Create a quick action…): an agent of the
/// list, the last one closed in a repository, or a Claude Desktop session.
#[prism_macros::command]
pub async fn session_source(state: State<'_, AppState>, source: crate::agents::session_source::Source) -> Cmd<crate::agents::session_source::SessionSource> {
    state.agents.session_source(source).await.map_err(Into::into)
}

/// The agent CLIs found on this computer, those an agent can continue with.
#[prism_macros::command]
pub async fn agent_installed(state: State<'_, AppState>) -> Cmd<Vec<crate::agents::provider::Provider>> {
    if crate::demo::active() {
        return Ok(crate::agents::provider::Provider::ALL.to_vec());
    }
    Ok(state.agents.installed())
}

/// The demo profile's relay material: its agent and recorded conversation, a branch made up.
fn demo_material(state: &AppState, id: &str) -> crate::agents::switch::Material {
    use crate::agents::{conversation::Entry, switch};
    let agent = state.hub.subscribe().borrow().agents.iter().find(|a| a.id == id).cloned();
    let c = crate::demo::details::conversation(id);
    let reply = |t: &crate::agents::conversation::Turn| t.entries.iter().rev().find_map(|e| match e { Entry::Text { text } => Some(text.clone()), _ => None });
    switch::Material {
        provider: agent.as_ref().map(|a| a.provider).unwrap_or_default(),
        title: agent.as_ref().map(|a| a.title.clone()).unwrap_or_default(),
        prompt: agent.as_ref().and_then(|a| a.prompt.clone()),
        plan: c.plan.clone(),
        last_reply: c.turns.iter().rev().find_map(reply),
        turns: c.turns.iter().rev().take(4).rev().map(|t| switch::RelayTurn { prompt: t.prompt.clone(), reply: reply(t).unwrap_or_default() }).collect(),
        readable: true,
        branch: agent.as_ref().and_then(|a| a.branch.clone()),
        base: Some("origin/main".into()),
        commits: vec!["4c1e2a9 fix: round VAT per line, then sum".into()],
        diffstat: " src/invoices/vat.ts      | 18 +++++++----\n src/invoices/vat.test.ts | 41 +++++++++++++++++++++\n 2 files changed, 52 insertions(+), 7 deletions(-)".into(),
        status: " M src/invoices/vat.ts".into(),
    }
}

/// Hands an agent over to another CLI (Continue with…), which starts from `switch.message`.
#[prism_macros::command]
pub async fn agent_switch(state: State<'_, AppState>, id: String, switch: crate::agents::switch::Switch) -> Cmd {
    state.agents.switch(&id, switch).await.map_err(Into::into)
}

/// The conversation of an agent's earlier stretch on another CLI (`Agent::legs[leg]`), its last
/// `turns` (30): its Chat's turns before the relay.
#[prism_macros::command]
pub async fn agent_leg_conversation(state: State<'_, AppState>, id: String, leg: u32, turns: Option<u32>) -> Cmd<Option<crate::agents::conversation::Conversation>> {
    if crate::demo::active() {
        return Ok(Some(crate::demo::details::conversation(&id)));
    }
    state.agents.leg_conversation(&id, leg as usize, turns.unwrap_or(30)).await.map_err(Into::into)
}

/// The history of an agent's terminal, or of one of its shells (`shell:<agent>:<n>`), as text:
/// the phone's scrollable view of it. `screen: false`: what scrolled off its screen only.
#[prism_macros::command]
pub async fn agent_history(state: State<'_, AppState>, id: String, lines: Option<u32>, screen: Option<bool>) -> Cmd<String> {
    if crate::demo::active() {
        return Ok(String::new());
    }
    state.agents.history(&id, lines.unwrap_or(2000), screen.unwrap_or(true)).await.map_err(Into::into)
}

/// Closes the agent's shell `n`, whatever it runs.
#[prism_macros::command]
pub async fn agent_close_shell(state: State<'_, AppState>, id: String, n: u32) -> Cmd {
    state.agents.close_shell(&id, n).await.map_err(Into::into)
}

/// Every shell with no agent still open, a worktree's or a pod's (the status bar's list).
#[prism_macros::command]
pub async fn open_shells(state: State<'_, AppState>) -> Cmd<Vec<crate::agents::OpenShell>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    state.agents.open_shells().await.map_err(Into::into)
}

/// Closes one of `open_shells`, whatever it runs, even when its worktree or pod is no longer shown.
#[prism_macros::command]
pub async fn close_open_shell(state: State<'_, AppState>, key: String) -> Cmd {
    state.agents.close_open_shell(&key).await.map_err(Into::into)
}

/// A worktree's shells still open (Worktrees view: a shell with no agent, kept by tmux).
#[prism_macros::command]
pub async fn worktree_shells(state: State<'_, AppState>, path: String) -> Cmd<Vec<crate::agents::Shell>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    state.agents.folder_shells(&path).await.map_err(Into::into)
}

/// Opens the worktree's shell `n`, or a new one (`null`): its terminal key.
#[prism_macros::command]
pub async fn worktree_shell(state: State<'_, AppState>, path: String, n: Option<u32>) -> Cmd<String> {
    if crate::demo::active() {
        return Ok(format!("{}wt0000000000000000:{}", crate::agents::SHELL, n.unwrap_or(1)));
    }
    state.agents.folder_shell(&path, n).await.map_err(Into::into)
}

/// Closes the worktree's shell `n`, whatever it runs.
#[prism_macros::command]
pub async fn worktree_close_shell(state: State<'_, AppState>, path: String, n: u32) -> Cmd {
    state.agents.close_folder_shell(&path, n).await.map_err(Into::into)
}

/// Runs a task in the agent's shell `n`, or a new one (`null`); `enter`: false, typed only;
/// `interrupt`: Ctrl-C a busy shell first. Returns the shell's terminal key.
#[prism_macros::command]
pub async fn agent_run(state: State<'_, AppState>, id: String, n: Option<u32>, command: String, enter: bool, interrupt: bool) -> Cmd<String> {
    state.agents.run_in_shell(&id, n, &command, enter, interrupt).await.map_err(Into::into)
}

/// The tasks the agent's folder declares (`just`, `task`, `package.json`).
#[prism_macros::command]
pub async fn agent_tasks(state: State<'_, AppState>, id: String) -> Cmd<Vec<crate::agents::tasks::Source>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    state.agents.tasks(&id).await.map_err(Into::into)
}

/// The folder a launch runs in, before it does: the one asked for, or a new worktree's
/// repository (its main checkout; absent, the primary one).
pub fn launch_dir(snap: &crate::state::Snapshot, cwd: Option<&str>, repo: Option<&str>) -> Option<std::path::PathBuf> {
    if let Some(cwd) = cwd {
        return Some(crate::config::expand_home(cwd.as_ref()));
    }
    let main = |w: &&crate::state::Worktree| w.is_main && repo.is_none_or(|r| w.repo == r);
    let checkout = snap.worktrees.iter().find(main).map(|w| w.path.clone());
    let repo = snap.repos.iter().find(|r| repo.map_or(r.is_main, |name| r.name == name)).map(|r| r.path.clone());
    checkout.or(repo).map(Into::into)
}

/// The task lines a launch's "Then" may run from a phone: those its folder declares, without
/// arguments to finish.
pub async fn launch_task_lines(state: &AppState, dir: &std::path::Path) -> Vec<String> {
    let sources = state.agents.tasks_in(dir).await.unwrap_or_default();
    sources.into_iter().flat_map(|s| s.tasks).filter(|t| !t.args).map(|t| t.command).collect()
}

/// Launcher: the tasks of the folder a launch would run in (its task steps, on a phone a choice
/// among them).
#[prism_macros::command]
pub async fn launch_tasks(state: State<'_, AppState>, cwd: Option<String>, repo: Option<String>) -> Cmd<Vec<String>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    let dir = launch_dir(&state.hub.subscribe().borrow(), cwd.as_deref(), repo.as_deref());
    Ok(match dir {
        Some(dir) => launch_task_lines(&state, &dir).await,
        None => Vec::new(),
    })
}

/// The user's answer to a task an agent flagged in passing: `later`, `reopen`, `dismiss` (`why`
/// told to its agents), `launched` (`target`: the new agent), `forget` (a line dealt with).
#[prism_macros::command]
pub fn aside_action(state: State<'_, AppState>, id: u64, action: String, why: Option<String>, target: Option<String>) -> Cmd {
    if !matches!(action.as_str(), "later" | "reopen" | "dismiss" | "launched" | "forget") {
        return Err(format!("no action {action}").into());
    }
    state.agents.aside_action(id, &action, why, target).map_err(Into::into)
}

/// The launch of a task flagged in passing, for the launcher: its prompt headed by where it comes
/// from, a new worktree (from its agent's branch, or the base) or its agent's folder, its agent's
/// CLI, model and effort, its kind.
#[prism_macros::command]
pub fn aside_launch(state: State<'_, AppState>, id: u64, from_branch: bool) -> Cmd<crate::state::LaunchSpec> {
    state.agents.aside_launch_spec(id, from_branch).map_err(Into::into)
}

/// "After this agent": the task flagged in passing launched once its agent is done (a step of its
/// chain), from its branch as it is then, or from the repository's base.
#[prism_macros::command]
pub fn aside_then(state: State<'_, AppState>, id: u64, from_branch: bool) -> Cmd {
    state.agents.aside_then(id, from_branch).map_err(Into::into)
}

/// A task flagged in passing made a ticket of the tracker: on the board of its repository (failing
/// that, the first of the sprint's), in its first status to pick; its tldr, prompt and origin as
/// its description. Its key.
#[prism_macros::command]
pub async fn aside_ticket(state: State<'_, AppState>, id: u64) -> Cmd<String> {
    // Claimed first: a second click (here or on a phone) meanwhile makes no second ticket.
    let a = state.agents.begin_ticket(id)?;
    let made = make_ticket(&state, &a).await;
    state.agents.end_ticket(id);
    let key = made?;
    state.agents.aside_ticket(id, &key)?;
    Ok(key)
}

async fn make_ticket(state: &AppState, a: &crate::state::Aside) -> anyhow::Result<String> {
    let (board, status) = {
        let snap = state.hub.subscribe().borrow().clone();
        let sprint = snap.sprint.ok_or_else(|| anyhow::anyhow!(tr("no sprint read from the tracker: no board to create the ticket on")))?;
        let repo = a.repo.as_deref().unwrap_or_default().to_lowercase();
        let board = sprint.boards.iter().find(|b| !repo.is_empty() && b.name.to_lowercase().contains(&repo)).or(sprint.boards.first()).cloned();
        let board = board.ok_or_else(|| anyhow::anyhow!(tr("the tracker offers no board to create the ticket on")))?;
        let pick = crate::config::load(&crate::config_path()).map(|c| c.tracker.pick_statuses).unwrap_or_default();
        let open: Vec<_> = board.statuses.iter().filter(|s| !s.closed).collect();
        let status = open.iter().find(|s| pick.contains(&s.status)).or(open.first()).map(|s| s.status.clone()).unwrap_or_default();
        (board.id, status)
    };
    let new = crate::collectors::tracker::NewTicket { board, status, title: a.title.clone(), description: aside_description(a), assign_me: false };
    let key = state.tracker().create(&state.hub, &new).await?;
    Ok(key)
}

/// What a task flagged in passing says as a ticket: its tldr, who spotted it, the prompt for the
/// agent that does it, the files it touches.
fn aside_description(a: &crate::state::Aside) -> String {
    let by = a.seen.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
    let files = if a.files.is_empty() { String::new() } else { format!("\n\n{}: {}", tr("Files"), a.files.join(", ")) };
    format!("{}\n\n{}\n\n{}{files}", a.tldr, tr!("Spotted in passing by {agents}. The prompt for the agent that does it:", agents = by), a.prompt)
}

/// An answer to an agent's chain: `skip` its held step, `stop` it, `forget` it.
#[prism_macros::command]
pub fn chain_order(state: State<'_, AppState>, id: String, action: String) -> Cmd {
    if !matches!(action.as_str(), "skip" | "stop" | "forget") {
        return Err(format!("no chain action {action}").into());
    }
    state.agents.chain_order(&id, &action).map_err(Into::into)
}

/// The go for a chain's held step: what it runs may be a task the agent changed, so from a phone
/// it is critical (the passkey), as running a task is.
#[prism_macros::command]
pub fn chain_go(state: State<'_, AppState>, id: String) -> Cmd {
    state.agents.chain_order(&id, "go").map_err(Into::into)
}

/// A prompt for an agent (its Chat, a note, a quick answer): sent at once, or once the menu on
/// its screen is answered (`Agents::send`).
#[prism_macros::command]
pub async fn agent_send(state: State<'_, AppState>, id: String, text: String) -> Cmd<crate::agents::Sent> {
    state.agents.send(&id, &text).await.map_err(Into::into)
}

/// The files that say what a task runs (package.json…) the agent's branch changed, and those of
/// its folder's dependencies written since its launch: a task run then is the agent's to choose.
#[prism_macros::command]
pub async fn agent_task_files_changed(state: State<'_, AppState>, id: String) -> Cmd<Vec<String>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    Ok(state.agents.task_files_changed(&id).await)
}

/// Files of an agent's folder, with their git status; `ignored`: also those `.gitignore` leaves out.
#[prism_macros::command]
pub async fn agent_files(state: State<'_, AppState>, id: String, ignored: Option<bool>) -> Cmd<crate::agents::files::Listing> {
    if crate::demo::active() {
        return crate::demo::details::agent_files(&state.hub.subscribe().borrow(), &id).map_err(Into::into);
    }
    state.agents.files(&id, ignored.unwrap_or(false)).await.map_err(Into::into)
}

/// Files of an agent's folder saved where the computer's save dialog said (`dest`): one as it is,
/// several or a folder as a zip.
#[prism_macros::command]
pub async fn agent_files_export(state: State<'_, AppState>, id: String, paths: Vec<String>, name: String, dest: String) -> Cmd {
    state.agents.export(&id, paths, name, crate::agents::files::MAX_DOWNLOAD, dest.into()).await?;
    Ok(())
}

/// Files of an agent's folder for a phone: one as it is, several or a folder as a zip, written to
/// a file the remote then serves the phone once over HTTP (`AgentDownload`: where to fetch it).
#[prism_macros::command]
pub async fn agent_files_download(state: State<'_, AppState>, id: String, paths: Vec<String>, name: String) -> Cmd<crate::agents::files::Prepared> {
    if crate::demo::active() {
        return Err(anyhow::anyhow!("no download in the demo").into());
    }
    Ok(state.agents.prepare_download(&id, paths, name).await?)
}

/// The files of an agent's folder the Web view can show, for its picker; `ignored`: also those
/// `.gitignore` leaves out.
#[prism_macros::command]
pub async fn agent_pages(state: State<'_, AppState>, id: String, ignored: bool) -> Cmd<Vec<crate::agents::files::Page>> {
    if crate::demo::active() {
        let listing = crate::demo::details::agent_files(&state.hub.subscribe().borrow(), &id)?;
        return Ok(listing.files.into_iter().filter(|f| crate::agents::files::is_viewable(&f.path)).map(|f| crate::agents::files::Page { path: f.path, ignored: false }).collect());
    }
    state.agents.pages(&id, ignored).await.map_err(Into::into)
}

/// An agent's artifacts: the pages of its session's scratchpad, most recent first.
// Off the main thread (where Tauri runs a plain `fn`): it reads files, milliseconds each time.
#[prism_macros::command(async)]
pub fn agent_artifacts(state: State<'_, AppState>, id: String) -> Cmd<Vec<crate::agents::files::Artifact>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    state.agents.artifacts(&id).map_err(Into::into)
}

/// Diff of an agent's folder: uncommitted (against HEAD), or of its whole branch (`branch`).
#[prism_macros::command]
pub async fn agent_diff(state: State<'_, AppState>, id: String, branch: bool) -> Cmd<crate::agents::files::Diff> {
    if crate::demo::active() {
        return crate::demo::details::agent_diff(&state.hub.subscribe().borrow(), &id, branch).map_err(Into::into);
    }
    state.agents.diff(&id, branch).await.map_err(Into::into)
}

/// The two sides of an overlap (`Snapshot.overlaps`, by its folders): their diffs against their
/// base, the files they share only. Only an overlap Prism found: no other folder is read.
#[prism_macros::command]
pub async fn overlap_diff(state: State<'_, AppState>, a: String, b: String) -> Cmd<crate::agents::files::OverlapDiff> {
    let found = state.hub.subscribe().borrow().overlaps.pairs.iter().find(|o| o.a == a && o.b == b).cloned();
    let o = found.ok_or_else(|| anyhow::anyhow!(tr!("this overlap is gone")))?;
    if crate::demo::active() {
        return Ok(crate::demo::details::overlap_diff(&o));
    }
    Ok(crate::agents::files::overlap_diff(&o).await)
}

/// A file of an agent's diff, whole, from its working tree: the Markdown preview of the diff.
#[prism_macros::command]
pub async fn agent_file_text(state: State<'_, AppState>, id: String, path: String) -> Cmd<String> {
    if crate::demo::active() {
        return crate::demo::details::agent_text(&state.hub.subscribe().borrow(), &id, &path).map_err(Into::into);
    }
    state.agents.text(&id, &path).await.map_err(Into::into)
}

/// An image of an agent's diff, as a `data:` URL: from its working tree, or (`old`) as it is in what the diff compares it to.
#[prism_macros::command]
pub async fn agent_diff_image(state: State<'_, AppState>, id: String, path: String, branch: bool, old: bool) -> Cmd<String> {
    if crate::demo::active() {
        return Err(anyhow::anyhow!("no image in the demo").into());
    }
    state.agents.diff_image(&id, &path, branch, old).await.map_err(Into::into)
}

/// A Markdown or text page of the Web view, read where the `prismfile:` scheme would serve it.
// Off the main thread (where Tauri runs a plain `fn`): it reads files, milliseconds each time.
#[prism_macros::command(async)]
pub fn agent_page_text(state: State<'_, AppState>, id: String, path: String) -> Cmd<String> {
    state.agents.page_text(&id, &path).map_err(Into::into)
}

/// Opens a file of an agent's folder in the `[open_with]` editor (Files preview).
#[prism_macros::command]
pub async fn agent_open_file(state: State<'_, AppState>, id: String, path: String) -> Cmd {
    let file = state.agents.file_path(&id, &path)?;
    let editor = state.hub.subscribe().borrow().open_with.editor.clone().ok_or(tr("no editor in [open_with]"))?;
    crate::platform::open_with(&file, Some(&editor)).await.map_err(Into::into)
}

/// A file of an agent's folder for the Files preview: text with its uncommitted lines, an
/// image, or why it is not shown.
#[prism_macros::command]
pub async fn agent_file_preview(state: State<'_, AppState>, id: String, path: String) -> Cmd<crate::agents::files::Preview> {
    if crate::demo::active() {
        return crate::demo::details::agent_preview(&state.hub.subscribe().borrow(), &id, &path).map_err(Into::into);
    }
    state.agents.preview(&id, &path).await.map_err(Into::into)
}

/// Lines of an agent's folder matching its Files tab's search (`git grep`).
#[prism_macros::command]
pub async fn agent_files_search(state: State<'_, AppState>, id: String, query: crate::agents::files::Query) -> Cmd<crate::agents::files::Found> {
    if crate::demo::active() {
        return crate::demo::details::agent_search(&state.hub.subscribe().borrow(), &id, &query).map_err(Into::into);
    }
    state.agents.search(&id, &query).await.map_err(Into::into)
}

/// The version on disk of files open in an agent's Files tab: those the agent changed are read again.
// Off the main thread (where Tauri runs a plain `fn`): it reads the disk.
#[prism_macros::command(async)]
pub fn agent_files_stamps(state: State<'_, AppState>, id: String, paths: Vec<String>) -> Cmd<Vec<Option<String>>> {
    if crate::demo::active() {
        return Ok(paths.iter().map(|_| Some("demo".into())).collect());
    }
    state.agents.stamps(&id, &paths).map_err(Into::into)
}

/// An image attached to one of the agent's prompts, as a `data:` URL: its Chat shows it.
// Off the main thread (where Tauri runs a plain `fn`): it reads a file of megabytes.
#[prism_macros::command(async)]
pub fn agent_attachment(state: State<'_, AppState>, id: String, path: String) -> Cmd<String> {
    state.agents.attachment(&id, &path).map_err(Into::into)
}

/// Image `n` pasted in one of the agent's prompts (on its transcript's line at byte `line`), as a
/// `data:` URL: its Chat shows it.
#[prism_macros::command(async)]
pub fn agent_pasted_image(state: State<'_, AppState>, id: String, line: u64, n: u32) -> Cmd<String> {
    state.agents.pasted_image(&id, line, n as usize).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_typing_takes_a_terminal_back() {
        for auto in ["\x1b[I", "\x1b[O", "\x1b[<0;12;5M", "\x1b[?1;2c", "\x1b[24;80R", "\x1b]11;rgb:1111/1111/1111\x07"] {
            assert!(!super::typed_by_hand(auto), "{auto:?}");
        }
        for typed in ["a", "\r", "\x1b", "\x1b[A", "ls\r", "\x03"] {
            assert!(super::typed_by_hand(typed), "{typed:?}");
        }
    }
}

/// Settings › Routines' Run now: the routine as saved, its result where it would go at its time.
#[prism_macros::command]
pub fn routine_run(state: State<'_, AppState>, name: String) -> Cmd {
    state.agents.run_routine_now(&name, None).map_err(Into::into)
}

/// "To review"'s Rerun: that run's routine on its repository, now; the row archived.
#[prism_macros::command]
pub fn routine_rerun(state: State<'_, AppState>, run: String) -> Cmd {
    state.agents.rerun_routine(&run).map_err(Into::into)
}

/// Rows out of "To review" (Archive, Archive the ready ones); their agents stay.
#[prism_macros::command]
pub fn routine_archive(state: State<'_, AppState>, runs: Vec<String>) -> Cmd {
    state.agents.archive_runs(&runs);
    Ok(())
}

/// The version of a routine shown (`hash`, the routine and its quick action) trusted, on the computer only.
#[prism_macros::command]
pub fn routine_trust(state: State<'_, AppState>, name: String, hash: String) -> Cmd {
    state.agents.trust_routine(&name, &hash).map_err(Into::into)
}

/// A routine switched on or off in `config.toml` (Settings, a phone with its passkey): its trust
/// holds, the switch is not what it runs.
#[prism_macros::command(async)]
pub fn routine_enable(state: State<'_, AppState>, name: String, on: bool) -> Cmd {
    let path = crate::config_path();
    prism_core::settings::set_nested_entry_key(&path, "agents", "routines", "name", &name, "enabled", &serde_json::Value::Bool(on))?;
    super::config::apply_live(&state, &path);
    Ok(())
}

/// A routine waiting for trust taken out of `config.toml` (Settings' Remove), its trust forgotten.
#[prism_macros::command(async)]
pub fn routine_remove(state: State<'_, AppState>, name: String) -> Cmd {
    let path = crate::config_path();
    prism_core::settings::remove_nested_entry(&path, "agents", "routines", "name", &name)?;
    state.agents.forget_routine(&name);
    super::config::apply_live(&state, &path);
    Ok(())
}

/// What the builtins read, as the window shows it: the open alerts (the retro is the core's, `crate::retro`).
#[prism_macros::command]
pub fn routine_inputs(state: State<'_, AppState>, alerts: Option<String>) -> Cmd {
    state.agents.set_routine_inputs(crate::agents::RoutineInputs { alerts });
    Ok(())
}
