//! Prism's assistant's tools, the running Prism's end (`prism mcp` in an assistant's
//! conversation, `PRISM_ASSISTANT`): what it reads of Prism (the repositories it may see, the
//! tickets, MRs, agents, sprint, its memory), its read-only exploration of a repository, and its
//! proposals — a ticket, an agent, a note, a repository, a fact — each waiting for the user's click
//! in its Chat (`answer`). Nothing of a repository out of its reach (`[[repos]] assistant = false`)
//! is listed, read nor remembered.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, LazyLock, Mutex,
};

use anyhow::{bail, ensure, Context, Result};
use chrono::Utc;
use prism_core::assistant::{self as core, AssistantProposal, AssistantState, Fact, FactKind, ProposalAction};
use prism_core::config::Config;
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::app::AppState;

/// How long a proposal's call waits for the click before it says it still waits (`assistant_wait`).
const WAIT: std::time::Duration = std::time::Duration::from_secs(240);

/// A proposal and the call waiting for its answer.
struct Pending {
    view: AssistantProposal,
    client: u64,
    reply: Option<oneshot::Sender<Value>>,
    answered: Option<Value>,
}

static PENDING: LazyLock<Mutex<Vec<Pending>>> = LazyLock::new(Default::default);
static NEXT: AtomicU64 = AtomicU64::new(1);

/// The state a request is answered in.
pub struct Ask<'a> {
    pub state: &'a Arc<AppState>,
    pub cfg: &'a Config,
    pub client: u64,
    pub agent: Option<String>,
}

/// One of the assistant's methods (`assistant_…`).
pub async fn handle(ask: Ask<'_>, method: &str, params: Value) -> Result<Value> {
    ensure!(ask.cfg.agents.assistant.enabled, "[agents.assistant] enabled is off in this Prism");
    // The state, read by the arms that need it only.
    let snap = || ask.state.hub.subscribe().borrow().clone();
    // Its tools answer its own conversations only: not any agent given Prism's MCP server.
    let mine = ask.agent.as_deref().is_some_and(|a| ask.state.agents.is_assistant(a));
    ensure!(mine, "these tools are the assistant's: ask Prism from its palette (« Ask Prism »)");
    let s = |k: &str| params.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    let cfg = ask.cfg;
    match method {
        "assistant_repos" => Ok(repos(cfg, &snap())),
        "assistant_find_repo" => {
            let text = s("text").context("text")?;
            // Its memory read once for both.
            let facts = core::visible_facts(cfg);
            let found = core::find_repo_in(cfg, &text, &facts);
            let remembered: Vec<Value> = core::remembered_in(&text, facts).into_iter().map(|f| json!({ "id": f.id, "kind": f.kind, "text": f.text })).collect();
            let excluded = core::excluded_in(cfg, &text);
            let mut out = json!({ "candidates": found, "remembered": remembered });
            if let Some(r) = excluded {
                out["out_of_reach"] = json!(format!("the request names {r}, a repository out of your reach: say you have no access to it, nothing more"));
            }
            if found.is_empty() {
                out["note"] = json!("no repository fits: ask the user which one, or propose to add it (assistant_propose_repo)");
            } else if found.len() > 1 && found[0].score - found[1].score < 0.15 {
                out["note"] = json!("several repositories fit as well: ask the user which one, never guess");
            }
            Ok(out)
        }
        "assistant_ticket" => {
            let id = s("id").context("id")?;
            let view = ask.state.tracker().ticket(&id).await?;
            let v = serde_json::to_value(&view)?;
            let text = format!("{} {}", view.name, view.description);
            if let Some(r) = core::excluded_in(cfg, &text) {
                bail!("this ticket is about {r}, a repository out of your reach: it may not be read");
            }
            // Its repository as the tracker files it, which its text may not name.
            let filed = ask.state.tracker().search(&view.id).await.unwrap_or_default().into_iter().find(|t| t.id.eq_ignore_ascii_case(&view.id)).and_then(|t| t.repo);
            if let Some(r) = filed.filter(|r| !visible(cfg, r)) {
                bail!("this ticket is about {r}, a repository out of your reach: it may not be read");
            }
            Ok(pick(&v, &["id", "name", "status", "task_type", "priority", "assignees", "tags", "list", "parent", "url", "description"]))
        }
        "assistant_search_tickets" => {
            let text = s("text").context("text")?;
            let found = ask.state.tracker().search(&text).await?;
            let list: Vec<Value> = found
                .iter()
                .filter(|t| t.repo.as_deref().is_none_or(|r| visible(cfg, r)) && core::excluded_in(cfg, &t.name).is_none())
                .map(|t| json!({ "id": t.id, "name": t.name, "status": t.status, "closed": t.closed, "url": t.url, "repo": t.repo }))
                .collect();
            Ok(json!({ "tickets": list, "note": if list.is_empty() { "nothing found: try other words" } else { "" } }))
        }
        "assistant_agents" => Ok(agents(cfg, &snap())),
        "assistant_mrs" => {
            let repo = s("repo");
            if let Some(r) = &repo {
                core::repo(cfg, r)?;
            }
            let list: Vec<Value> = snap()
                .merge_requests
                .iter()
                .filter(|m| m.repo.as_deref().is_none_or(|r| visible(cfg, r)))
                .filter(|m| repo.as_deref().is_none_or(|r| m.repo.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(r))))
                .map(|m| json!({ "repo": m.repo, "iid": m.iid, "title": m.title, "url": m.url, "state": m.state, "draft": m.draft, "branch": m.source_branch, "ticket": m.ticket, "author": m.author_name, "approved_by": m.approved_by, "reviewers": m.reviewers }))
                .collect();
            Ok(json!({ "merge_requests": list }))
        }
        "assistant_sprint" => {
            let snap = snap();
            let Some(sprint) = &snap.sprint else { return Ok(json!({ "sprint": null, "note": "no sprint read (no tracker, or not yet)" })) };
            let me = snap.tracker_me.as_ref().map(|p| p.name.clone());
            let tickets: Vec<Value> = sprint
                .tickets
                .iter()
                .filter(|t| t.repo.as_deref().is_none_or(|r| visible(cfg, r)) && core::excluded_in(cfg, &t.name).is_none())
                .map(|t| json!({ "id": t.id, "name": t.name, "status": t.status, "closed": t.closed, "mine": me.as_ref().is_some_and(|m| t.assignees.iter().any(|a| &a.name == m)), "repo": t.repo }))
                .collect();
            Ok(json!({ "name": sprint.name, "start": sprint.start, "end": sprint.end, "url": sprint.url, "boards": sprint.boards.iter().map(|b| &b.name).collect::<Vec<_>>(), "tickets": tickets }))
        }
        "assistant_memory" => match s("id") {
            Some(id) => {
                let f = core::visible_facts(cfg).into_iter().find(|f| f.id == id.trim_end_matches(".md")).context("no such fact: see the index")?;
                Ok(json!({ "id": f.id, "kind": f.kind, "text": f.text, "source": f.source, "at": f.at }))
            }
            None => Ok(json!(core::index_text(cfg))),
        },
        "assistant_remember" => {
            let fact = Fact { kind: s("kind").map(|k| FactKind::parse(&k)).unwrap_or_default(), text: s("text").context("text")?, source: s("source").unwrap_or_else(|| "« remember that… »".into()), ..Default::default() };
            let saved = core::save(cfg, fact)?;
            refresh_home(cfg);
            Ok(json!({ "status": "remembered", "id": saved.id }))
        }
        "assistant_forget" => {
            core::forget(cfg, &s("id").context("id")?)?;
            refresh_home(cfg);
            Ok(json!({ "status": "forgotten" }))
        }
        "assistant_explore" => {
            let repo = core::repo(cfg, &s("repo").context("repo")?)?.clone();
            let question = s("question").context("question")?;
            let dir = repo.path.as_deref().map(prism_core::config::expand_home).context("this repository has no local clone ([[repos]] path): nothing to explore")?;
            let provider = cfg.agents.assistant.provider.unwrap_or(cfg.agents.provider);
            // Under the CLI's own timeout for a tool (Codex's for Prism's server), past which nobody reads the answer.
            let most = if provider == prism_core::agents::provider::Provider::Codex { prism_core::mcp::CODEX_TOOL_SECONDS - 20 } else { 1800 };
            let limit = std::time::Duration::from_secs(u64::from(cfg.agents.assistant.explore_seconds.clamp(30, most)));
            let answer = ask.state.agents.explore(provider, None, &repo.name, &dir, &question, limit).await?;
            Ok(json!({ "repo": repo.name, "synthesis": answer }))
        }
        "assistant_wait" => wait(params.get("id").and_then(Value::as_u64).context("id")?).await,
        "assistant_propose_ticket" => {
            let repo = s("repo");
            if let Some(r) = &repo {
                core::repo(cfg, r)?;
            }
            let p = proposal(&ask, ProposalAction::Ticket, repo, s("title").context("title")?, s("description").unwrap_or_default(), s("why"));
            let p = AssistantProposal { ticket_type: s("type"), ..p };
            propose(&ask, p).await
        }
        "assistant_propose_launch" => {
            let repo = core::repo(cfg, &s("repo").context("repo")?)?.clone();
            let prompt = s("prompt").context("prompt")?;
            let ticket = s("ticket");
            let title = s("title").or_else(|| ticket.clone()).unwrap_or_else(|| prompt.lines().next().unwrap_or("Agent").chars().take(60).collect());
            let worktree = params.get("worktree").and_then(Value::as_bool).unwrap_or(true);
            let kind = s("kind").or_else(|| ticket.as_ref().map(|_| "Implement".to_string()));
            let category = kind.and_then(|k| cfg.agents.kinds.iter().find(|x| x.name.eq_ignore_ascii_case(&k) || x.when.eq_ignore_ascii_case(&k) || x.name.to_lowercase().starts_with(&k.to_lowercase())).map(|x| x.name.clone()));
            ensure!(!worktree || repo.path.is_some(), "{} has no local clone ([[repos]] path): no worktree to launch in", repo.name);
            let then: Vec<prism_core::state::LaunchStep> = params
                .get("then")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(|step| match step.trim().to_lowercase().as_str() {
                    "review" => Some(prism_core::state::LaunchStep { kind: prism_core::state::StepKind::Agent, when: prism_core::state::StepWhen::Ok, command: None, text: None }),
                    "mr" | "draft_mr" => Some(prism_core::state::LaunchStep { kind: prism_core::state::StepKind::Mr, when: prism_core::state::StepWhen::Ok, command: None, text: None }),
                    _ => None,
                })
                .collect();
            // The draft MR before its review, which reads it.
            let mut then = then;
            then.sort_by_key(|s| s.kind != prism_core::state::StepKind::Mr);
            let launch = prism_core::state::LaunchSpec {
                title: title.clone(),
                cwd: if worktree { None } else { repo.path.as_ref().map(|p| p.display().to_string()) },
                prompt: Some(prompt.clone()),
                // Named after its ticket and title, as a ticket's own Implement names it.
                worktree: worktree.then(|| prism_core::state::WorktreeRequest {
                    repo: Some(repo.name.clone()),
                    name: s("branch").or_else(|| ticket.as_ref().map(|t| if title.starts_with(t.as_str()) { title.clone() } else { format!("{t} {title}") })),
                    branch: None,
                    base: s("base"),
                    also: Vec::new(),
                }),
                category,
                then,
                from: None,
                ..Default::default()
            };
            let p = proposal(&ask, ProposalAction::Launch, Some(repo.name.clone()), title, prompt, s("why"));
            propose(&ask, AssistantProposal { launch: Some(launch), ticket, ..p }).await
        }
        "assistant_propose_note" => {
            let repo = s("repo");
            if let Some(r) = &repo {
                core::repo(cfg, r)?;
            }
            let text = s("text").context("text")?;
            let title = s("title").unwrap_or_else(|| text.lines().next().unwrap_or_default().chars().take(60).collect());
            let p = proposal(&ask, ProposalAction::Note, repo, title, text, s("why"));
            propose(&ask, p).await
        }
        "assistant_propose_repo" => {
            let source = s("source").context("source")?;
            let name = s("name").unwrap_or_else(|| source.trim_end_matches('/').trim_end_matches(".git").rsplit(['/', ':']).next().unwrap_or(&source).to_string());
            ensure!(!cfg.repos.iter().any(|r| r.name.eq_ignore_ascii_case(&name)), "a repository named {name} is configured already (or out of your reach)");
            let p = proposal(&ask, ProposalAction::Repo, Some(name.clone()), name, s("description").unwrap_or_default(), s("why"));
            propose(&ask, AssistantProposal { source: Some(source), ..p }).await
        }
        "assistant_propose_memory" => {
            let text = s("text").context("text")?;
            if let Some(r) = core::excluded_in(cfg, &text) {
                bail!("{r} is out of your reach: nothing of it is remembered");
            }
            let kind = s("kind").map(|k| FactKind::parse(&k)).unwrap_or_default();
            let p = proposal(&ask, ProposalAction::Memory, None, String::new(), text, s("why"));
            propose(&ask, AssistantProposal { fact_kind: Some(kind), ..p }).await
        }
        other => bail!("no method {other}"),
    }
}

/// A repository the assistant may see (by name; unknown names are not refused here).
fn visible(cfg: &Config, name: &str) -> bool {
    cfg.repos.iter().find(|r| r.name.eq_ignore_ascii_case(name)).is_none_or(|r| r.assistant)
}

/// Some keys of an object.
fn pick(v: &Value, keys: &[&str]) -> Value {
    let mut out = serde_json::Map::new();
    for k in keys {
        if let Some(x) = v.get(*k).filter(|x| !x.is_null()) {
            out.insert((*k).to_string(), x.clone());
        }
    }
    Value::Object(out)
}

fn repos(cfg: &Config, snap: &prism_core::state::Snapshot) -> Value {
    let list: Vec<Value> = core::visible(cfg)
        .iter()
        .map(|r| {
            let (main, _, base) = cfg.branches(r);
            json!({
                "name": r.name,
                "aliases": r.aliases,
                "description": r.description,
                "path": r.path,
                "project": r.project,
                "forge": r.forge.unwrap_or(cfg.forge.provider),
                "worktrees": r.worktrees || r.primary,
                "main_branch": main,
                "worktree_base": base,
                "agent": r.agent,
            })
        })
        .collect();
    let boards: Vec<&String> = snap.sprint.iter().flat_map(|s| s.boards.iter().map(|b| &b.name)).collect();
    json!({
        "repositories": list,
        "tracker": { "provider": cfg.tracker.provider, "ticket_prefix": cfg.tracker.ticket_prefix, "boards": boards, "sprint": snap.sprint.as_ref().map(|s| &s.name) },
        "note": "a ticket is created on the tracker's boards, whatever the repository: name the repository in its description",
    })
}

fn agents(cfg: &Config, snap: &prism_core::state::Snapshot) -> Value {
    let list: Vec<Value> = snap
        .agents
        .iter()
        .filter(|a| !a.launch.as_ref().is_some_and(|l| l.assistant))
        .filter(|a| [Some(a.cwd.as_str()), a.worktree.as_deref()].into_iter().flatten().all(|dir| snap.repo_of_dir(dir).is_none_or(|r| visible(cfg, &r))))
        .map(|a| json!({ "id": a.id, "title": a.title, "status": a.status, "kind": a.category, "folder": a.worktree.as_deref().unwrap_or(&a.cwd), "branch": a.branch, "ticket": a.ticket, "since": a.created_at }))
        .collect();
    json!({ "agents": list })
}

fn proposal(ask: &Ask<'_>, action: ProposalAction, repo: Option<String>, title: String, text: String, why: Option<String>) -> AssistantProposal {
    AssistantProposal { id: NEXT.fetch_add(1, Ordering::SeqCst), action, agent: ask.agent.clone(), at: Utc::now(), repo, title, text, ticket_type: None, launch: None, ticket: None, source: None, fact_kind: None, why: why.unwrap_or_default() }
}

async fn propose(ask: &Ask<'_>, p: AssistantProposal) -> Result<Value> {
    let id = p.id;
    let (tx, rx) = oneshot::channel();
    PENDING.lock().unwrap().push(Pending { view: p, client: ask.client, reply: Some(tx), answered: None });
    publish(ask.state);
    await_answer(id, rx).await
}

async fn await_answer(id: u64, rx: oneshot::Receiver<Value>) -> Result<Value> {
    match tokio::time::timeout(WAIT, rx).await {
        Ok(Ok(answer)) => Ok(answer),
        Ok(Err(_)) => Ok(json!({ "status": "dropped", "id": id })),
        Err(_) => Ok(json!({ "status": "waiting", "id": id, "note": "still waiting for the user's click: call assistant_wait with this id to keep waiting" })),
    }
}

async fn wait(id: u64) -> Result<Value> {
    let rx = {
        let mut list = PENDING.lock().unwrap();
        let p = list.iter_mut().find(|p| p.view.id == id).context("no such proposal (answered and read already, or gone)")?;
        if let Some(a) = p.answered.take() {
            list.retain(|p| p.view.id != id);
            return Ok(a);
        }
        let (tx, rx) = oneshot::channel();
        p.reply = Some(tx);
        rx
    };
    await_answer(id, rx).await
}

fn publish(state: &AppState) {
    let proposals: Vec<AssistantProposal> = PENDING.lock().unwrap().iter().filter(|p| p.answered.is_none()).map(|p| p.view.clone()).collect();
    let next = AssistantState { proposals };
    state.hub.update(|s| {
        let changed = s.assistant != next;
        s.assistant = next.clone();
        changed
    });
}

/// The connection `client` came by is gone: its proposals with it.
pub fn gone(state: &AppState, client: u64) {
    let removed = {
        let mut list = PENDING.lock().unwrap();
        let before = list.len();
        list.retain(|p| p.client != client);
        list.len() != before
    };
    if removed {
        publish(state);
    }
}

/// The user's answer to a proposal, from its card: `status` (`created`, `launched`, `added`,
/// `remembered`, `written`, `dismissed`, `modified`…) and what goes back with it (a ticket's key,
/// an agent's id, a reason, the text as changed). The assistant's waiting call gets it.
pub fn answer(state: &AppState, id: u64, status: &str, detail: Value) -> Result<()> {
    ensure!(!status.trim().is_empty(), "no status");
    let mut answer = json!({ "status": status.trim() });
    if let Value::Object(extra) = detail {
        for (k, v) in extra {
            if k != "status" && !v.is_null() {
                answer[k] = v;
            }
        }
    }
    let mut list = PENDING.lock().unwrap();
    let i = list.iter().position(|p| p.view.id == id && p.answered.is_none()).context("no such proposal (answered already, or its conversation ended)")?;
    match list[i].reply.take().map(|tx| tx.send(answer.clone())) {
        Some(Ok(())) => {
            list.remove(i);
        }
        _ => list[i].answered = Some(answer),
    }
    drop(list);
    publish(state);
    Ok(())
}

/// Its folder written again (its instructions name its memory and the repositories): after a
/// fact, a change of the config.
/// Not before its first conversation.
pub fn refresh_home(cfg: &Config) {
    if !core::dir(cfg).is_dir() {
        return;
    }
    if let Err(e) = core::write_home(cfg) {
        tracing::warn!("assistant: its folder not written: {e:#}");
    }
}

/// Its tools, as `prism mcp` lists them in an assistant's conversation.
pub fn tools() -> Value {
    let obj = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    let read = json!({ "readOnlyHint": true, "openWorldHint": false });
    let proposes = json!({ "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false });
    let waits = "Waits for the user's click in Prism and gives what became of it: status created / launched / added / written / remembered, dismissed (with the user's reason), modified (with what changed); or waiting (then call assistant_wait with its id).";
    json!([
        { "name": "assistant_repos", "title": "Repositories", "description": "The repositories you may see: name, aliases, description, clone, forge project, whether agents get worktrees there; and the tracker (provider, ticket prefix, boards, sprint).", "inputSchema": obj(json!({}), &[]), "annotations": read },
        { "name": "assistant_find_repo", "title": "Find the repository", "description": "The repositories a request may be about, the likeliest first, with a score (0–1) and why (memory, name, alias, project, description), and the facts of your memory that share its words. Several close scores or none: ask the user, never guess.", "inputSchema": obj(json!({ "text": { "type": "string", "description": "The user's words naming the project (« the shop app », « the back »)" } }), &["text"]), "annotations": read },
        { "name": "assistant_ticket", "title": "Read a ticket", "description": "A ticket of the tracker: name, status, type, assignees, list, description.", "inputSchema": obj(json!({ "id": { "type": "string", "description": "Its id (SHOP-214)" } }), &["id"]), "annotations": read },
        { "name": "assistant_search_tickets", "title": "Search tickets", "description": "Tickets whose id or name has every word: check that one exists before proposing it.", "inputSchema": obj(json!({ "text": { "type": "string" } }), &["text"]), "annotations": read },
        { "name": "assistant_agents", "title": "Agents", "description": "The agents running or waiting in Prism: title, status, kind, folder, branch, ticket.", "inputSchema": obj(json!({}), &[]), "annotations": read },
        { "name": "assistant_mrs", "title": "Merge requests", "description": "The merge requests Prism follows, of one repository or all: title, state, branch, ticket, author, approvals.", "inputSchema": obj(json!({ "repo": { "type": "string" } }), &[]), "annotations": read },
        { "name": "assistant_sprint", "title": "Sprint", "description": "The current sprint: its dates, boards and tickets (status, mine).", "inputSchema": obj(json!({}), &[]), "annotations": read },
        { "name": "assistant_explore", "title": "Explore a repository", "description": "Runs a read-only subagent in a repository's clone on a precise question and gives its synthesis: files and functions (path:line), how it works, leads. Your only way to read code. Takes up to a few minutes; call it for several repositories at once when needed.", "inputSchema": obj(json!({ "repo": { "type": "string" }, "question": { "type": "string", "description": "« Where is the cart's VAT computed, and when is it called? »" } }), &["repo", "question"]), "annotations": { "readOnlyHint": true, "openWorldHint": false } },
        { "name": "assistant_memory", "title": "Read your memory", "description": "A fact's whole text by its id (its file's name in the index), or the index without id.", "inputSchema": obj(json!({ "id": { "type": "string" } }), &[]), "annotations": read },
        { "name": "assistant_remember", "title": "Remember a fact", "description": "Writes a fact to your memory at once, when the user says « remember that… ». One fact, a sentence or two, naming its repository.", "inputSchema": obj(json!({ "text": { "type": "string" }, "kind": { "type": "string", "enum": ["project", "convention", "decision", "you"] }, "source": { "type": "string", "description": "Where it comes from, in a few words" } }), &["text"]), "annotations": proposes },
        { "name": "assistant_forget", "title": "Forget a fact", "description": "Forgets a fact the user says is wrong or outdated.", "inputSchema": obj(json!({ "id": { "type": "string" } }), &["id"]), "annotations": proposes },
        { "name": "assistant_propose_ticket", "title": "Propose a ticket", "description": format!("Proposes a new ticket: the user creates it in a click, or changes it first. The description in Markdown, written after reading the code: goal, leads in the code (paths, functions), acceptance criteria as a list. {waits}"), "inputSchema": obj(json!({ "repo": { "type": "string" }, "title": { "type": "string", "description": "Short, imperative" }, "description": { "type": "string" }, "type": { "type": "string", "description": "feature, bug, task…" }, "why": { "type": "string", "description": "One line for the user" } }), &["title", "description"]), "annotations": proposes },
        { "name": "assistant_propose_launch", "title": "Propose an agent", "description": format!("Proposes to launch an agent that does the work: the user launches it in a click or opens it in the launcher. The prompt must stand on its own: files, functions, what done means. {waits}"), "inputSchema": obj(json!({ "repo": { "type": "string" }, "prompt": { "type": "string" }, "title": { "type": "string" }, "ticket": { "type": "string", "description": "The ticket it implements (SHOP-214): its worktree is named after it" }, "kind": { "type": "string", "description": "One of Prism's kinds (Implement, Review…); with a ticket, Implement" }, "worktree": { "type": "boolean", "description": "A new worktree (default true); false: in the clone" }, "branch": { "type": "string", "description": "The worktree's name, else the ticket's" }, "base": { "type": "string", "description": "The branch it starts from, else the repository's base" }, "then": { "type": "array", "items": { "type": "string", "enum": ["review", "mr"] }, "description": "Steps after it: a review agent, a draft MR" }, "why": { "type": "string" } }), &["repo", "prompt"]), "annotations": proposes },
        { "name": "assistant_propose_note", "title": "Propose a note", "description": format!("Proposes a note in Prism's notes. {waits}"), "inputSchema": obj(json!({ "text": { "type": "string" }, "title": { "type": "string" }, "repo": { "type": "string" }, "why": { "type": "string" } }), &["text"]), "annotations": proposes },
        { "name": "assistant_propose_repo", "title": "Propose to add a repository", "description": format!("Proposes to add a repository Prism does not know: an existing folder, a clone (a forge project or URL) or a new project, as Settings › Repositories does. Nothing is cloned without the click. Once added, carry on with the request. {waits}"), "inputSchema": obj(json!({ "source": { "type": "string", "description": "A folder (~/code/billing), a forge project (acme/billing) or a clone URL" }, "name": { "type": "string" }, "description": { "type": "string", "description": "What it is, in a sentence" }, "why": { "type": "string" } }), &["source"]), "annotations": proposes },
        { "name": "assistant_propose_memory", "title": "Propose to remember", "description": format!("Proposes to remember a fact learnt by asking the user (« the back is the server/ folder of shop »): the user keeps it, changes it or says no. {waits}"), "inputSchema": obj(json!({ "text": { "type": "string" }, "kind": { "type": "string", "enum": ["project", "convention", "decision", "you"] }, "why": { "type": "string" } }), &["text"]), "annotations": proposes },
        { "name": "assistant_wait", "title": "Wait for a click", "description": "Keeps waiting for the user's answer to a proposal whose call returned status waiting.", "inputSchema": obj(json!({ "id": { "type": "integer" } }), &["id"]), "annotations": read },
    ])
}

/// What the assistant is told by the server (its role is in its folder's instructions).
pub const INSTRUCTIONS: &str = "You are Prism's assistant: find the repository (assistant_find_repo, your memory), read code only through assistant_explore, and propose with the assistant_propose_* tools; never write, never launch, never guess a repository (ask). Nothing of a repository out of your reach is listed nor read.";
