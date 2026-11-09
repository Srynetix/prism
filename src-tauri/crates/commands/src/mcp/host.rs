//! The running Prism's end of `prism mcp`: a Unix socket of the profile (`mcp::socket_path`, the
//! owner's only; a named pipe on Windows, `mcp::pipe_name`), one JSON request per line, one answer
//! per line. Prism reads, checks and applies: the MCP server never reads a file itself. What it changes waits, as a proposal, for the user's
//! answer in the window (`answer`).

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex, OnceLock,
};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use prism_base::tr;
use prism_core::mcp::{McpClient, McpState, Proposal, ProposalKind};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::oneshot,
};

use super::merge;
use crate::app::AppState;

/// How long a proposal's call waits for the answer before it says it still waits (the agent asks
/// again with `prism_wait_proposal`): under the tool timeout of the CLIs.
const WAIT: std::time::Duration = std::time::Duration::from_secs(240);

/// The proposals and the connected `prism mcp` of this Prism.
struct Host {
    state: Arc<AppState>,
    next: AtomicU64,
    clients: Mutex<Vec<McpClient>>,
    proposals: Mutex<Vec<Pending>>,
}

/// A proposal and what answers it.
struct Pending {
    view: Proposal,
    change: Change,
    /// `config.toml` when the proposal was computed.
    before: String,
    /// The waiting call, gone when it gave up (`WAIT`): the answer then waits for `wait`.
    reply: Option<oneshot::Sender<Value>>,
    /// The answer given while no call waited.
    answered: Option<Value>,
}

#[derive(Clone)]
enum Change {
    /// A fragment merged in (or the whole file, `whole`).
    Config { toml: String, whole: bool },
    /// `[[views]]` replaced.
    Layout(String),
    /// A custom block's declaration, written and trusted as Settings saves one.
    Block(Value),
}

static HOST: OnceLock<Arc<Host>> = OnceLock::new();

fn host() -> Result<&'static Arc<Host>> {
    HOST.get().context("prism mcp: not serving")
}

/// Serves `prism mcp` on the profile's socket (a named pipe on Windows), for as long as Prism runs.
pub fn serve(state: Arc<AppState>) {
    #[cfg(unix)]
    let Some(listener) = bind() else { return };
    let host = Arc::new(Host { state, next: AtomicU64::new(1), clients: Mutex::new(Vec::new()), proposals: Mutex::new(Vec::new()) });
    let _ = HOST.set(host.clone());
    prism_base::rt::Generation::outside(|| {
        #[cfg(unix)]
        prism_base::rt::spawn(accept(host, listener));
        #[cfg(windows)]
        prism_base::rt::spawn(accept(host));
    });
}

/// The socket, bound here (no runtime needed) to be served from the runtime.
#[cfg(unix)]
fn bind() -> Option<std::os::unix::net::UnixListener> {
    let path = prism_core::mcp::socket_path();
    if let Err(e) = prism_core::mcp::private_dir(&path) {
        tracing::warn!("mcp: no socket: {e:#}");
        return None;
    }
    // One Prism per profile: a socket left there is a former one's.
    let _ = std::fs::remove_file(&path);
    let listener = match std::os::unix::net::UnixListener::bind(&path).and_then(|l| l.set_nonblocking(true).map(|()| l)) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("mcp: no socket at {}: {e}", path.display());
            return None;
        }
    };
    if let Err(e) = prism_core::mcp::owner_only(&path) {
        let _ = std::fs::remove_file(&path);
        tracing::warn!("mcp: {e:#}");
        return None;
    }
    Some(listener)
}

#[cfg(unix)]
async fn accept(host: Arc<Host>, listener: std::os::unix::net::UnixListener) {
    let listener = match tokio::net::UnixListener::from_std(listener) {
        Ok(l) => l,
        Err(e) => return tracing::warn!("mcp: {e}"),
    };
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                prism_base::rt::spawn(host.clone().connection(stream));
            }
            Err(e) => {
                tracing::warn!("mcp: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

/// The profile's named pipe: an instance per connection, the next one made before the connected
/// one is served. The first one claims the name (`first_pipe_instance`): another process cannot
/// have made it first; clients of this computer and this account only.
#[cfg(windows)]
async fn accept(host: Arc<Host>) {
    let name = prism_core::mcp::pipe_name();
    let make = |first: bool| prism_sys::windows::pipe_server(&name, first);
    let mut server = match make(true) {
        Ok(s) => s,
        Err(e) => return tracing::warn!("mcp: no pipe {name}: {e}"),
    };
    loop {
        if let Err(e) = server.connect().await {
            tracing::warn!("mcp: {e}");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        let next = match make(false) {
            Ok(s) => s,
            Err(e) => return tracing::warn!("mcp: no pipe {name}: {e}"),
        };
        let connected = std::mem::replace(&mut server, next);
        prism_base::rt::spawn(host.clone().connection(connected));
    }
}

impl Host {
    async fn connection<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + 'static>(self: Arc<Self>, stream: S) {
        let (read, write) = tokio::io::split(stream);
        let write = Arc::new(tokio::sync::Mutex::new(write));
        let mut lines = BufReader::new(read).lines();
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        while let Ok(Some(line)) = lines.next_line().await {
            let me = self.clone();
            let write = write.clone();
            tokio::spawn(async move {
                let req: Value = serde_json::from_str(&line).unwrap_or_default();
                let rid = req.get("id").cloned().unwrap_or(Value::Null);
                let method = req.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
                let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
                let out = match me.handle(id, &method, params).await {
                    Ok(result) => json!({ "id": rid, "result": result }),
                    Err(e) => json!({ "id": rid, "error": format!("{e:#}") }),
                };
                let mut w = write.lock().await;
                let _ = w.write_all(format!("{out}\n").as_bytes()).await;
            });
        }
        // Gone: its proposals with it, nobody waits for their answer any more.
        self.clients.lock().unwrap().retain(|c| c.id != id);
        self.proposals.lock().unwrap().retain(|p| p.view.client_id != Some(id));
        self.publish();
    }

    fn publish(&self) {
        // Read only for proposals to compare it with.
        let waiting = !self.proposals.lock().unwrap().is_empty();
        let file = if waiting { std::fs::read_to_string(crate::config_path()).unwrap_or_default() } else { String::new() };
        let clients = self.clients.lock().unwrap().clone();
        let proposals: Vec<Proposal> = self
            .proposals
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|p| p.answered.is_none())
            .map(|p| {
                p.view.stale = matches!(p.change, Change::Config { .. } | Change::Layout(_)) && p.before != file;
                p.view.clone()
            })
            .collect();
        let state = McpState { clients, proposals };
        self.state.hub.update(|s| {
            let changed = s.mcp != state;
            s.mcp = state.clone();
            changed
        });
    }

    fn count(&self, client: u64, proposal: bool) {
        if let Some(c) = self.clients.lock().unwrap().iter_mut().find(|c| c.id == client) {
            if proposal {
                c.proposals += 1;
            } else {
                c.reads += 1;
            }
        }
    }

    async fn handle(&self, client: u64, method: &str, params: Value) -> Result<Value> {
        // Read once per request: its switches, and what the handlers need of it.
        let cfg = crate::config::load(&crate::config_path()).ok();
        let str_param = |k: &str| params.get(k).and_then(Value::as_str).map(str::to_string);
        // « In passing » answers by its own switch, whatever `[mcp] enabled` says.
        let aside = matches!(method, "suggest_task" | "dismiss_task");
        if method != "hello" && !aside {
            anyhow::ensure!(cfg.as_ref().is_none_or(|c| c.mcp.enabled), "[mcp] enabled is off in this Prism");
        }
        let proposal = method.starts_with("propose") || method == "create_custom_block";
        if method != "hello" {
            self.count(client, proposal);
        }
        match method {
            "hello" => {
                let name = str_param("client").unwrap_or_else(|| "agent".into());
                let agent = str_param("agent").filter(|a| !a.is_empty());
                self.clients.lock().unwrap().push(McpClient { id: client, name, agent, since: Utc::now(), reads: 0, proposals: 0 });
                self.publish();
                let c = cfg.as_ref();
                Ok(json!({
                    "profile": crate::profile::active(),
                    "demo": crate::demo::active(),
                    "version": crate::packs::PRISM,
                    "language": c.as_ref().map(|c| c.appearance.language.clone()).unwrap_or_default(),
                    "enabled": if str_param("mode").as_deref() == Some("aside") { c.as_ref().is_none_or(|c| c.agents.aside.enabled) } else { c.as_ref().is_none_or(|c| c.mcp.enabled) },
                    "proposals": c.as_ref().is_none_or(|c| c.mcp.proposals),
                }))
            }
            "config_current" => Ok(json!(merge::mask(&std::fs::read_to_string(crate::config_path()).unwrap_or_default()))),
            "config_merged" => {
                let text = std::fs::read_to_string(crate::config_path()).unwrap_or_default();
                Ok(json!(merge::mask(&crate::settings::merged(&crate::config_path(), &text)?)))
            }
            "schema" => Ok(super::schema().clone()),
            "blocks" => Ok(self.blocks()),
            "packs" => packs(),
            "problems" => Ok(self.problems()),
            "layout" => layout(),
            "explain_key" => explain(&str_param("path").context("path")?),
            "validate" => {
                let toml = str_param("toml").context("toml")?;
                let whole = params.get("whole").and_then(Value::as_bool).unwrap_or(false);
                Ok(serde_json::to_value(validate(&toml, whole))?)
            }
            "propose_config" => {
                let toml = str_param("toml").context("toml")?;
                let whole = params.get("whole").and_then(Value::as_bool).unwrap_or(false);
                self.propose(cfg.as_ref(), client, ProposalKind::Config, Change::Config { toml, whole }, str_param("why").unwrap_or_default(), None).await
            }
            "propose_layout" => {
                let views = str_param("views").context("views")?;
                self.propose(cfg.as_ref(), client, ProposalKind::Layout, Change::Layout(views), str_param("why").unwrap_or_default(), None).await
            }
            "run_custom_block" => run_custom_block(&params).await,
            "create_custom_block" => {
                let def = block_def(&params)?;
                let test = params.get("test").cloned().filter(|t| !t.is_null());
                self.propose(cfg.as_ref(), client, ProposalKind::CustomBlock, Change::Block(def), str_param("why").unwrap_or_default(), test).await
            }
            "wait_proposal" => self.wait(params.get("id").and_then(Value::as_u64).context("id")?).await,
            "suggest_task" | "dismiss_task" => {
                let (name, agent) = self.clients.lock().unwrap().iter().find(|c| c.id == client).map(|c| (c.name.clone(), c.agent.clone())).unwrap_or_else(|| ("agent".into(), None));
                let agents = &self.state.agents;
                let mut answer = if method == "suggest_task" {
                    let s: prism_agents::asides::Suggest = serde_json::from_value(params).context("title, tldr and prompt (strings), files (a list of paths)")?;
                    agents.suggest(agent.as_deref(), &name, s)?
                } else {
                    let id = params.get("task_id").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim_start_matches('#').parse().ok()))).context("task_id")?;
                    agents.dismiss_by_agent(agent.as_deref(), id, str_param("why"))?
                };
                // What became of its earlier ones, if its hook has not told it yet.
                let news = agent.as_deref().map(|a| agents.take_news(a)).unwrap_or_default();
                if !news.is_empty() {
                    answer["news"] = json!(news);
                }
                Ok(answer)
            }
            other => bail!("no method {other}"),
        }
    }

    /// The blocks known: those with a source (on, off, in error), the custom ones (waiting for
    /// trust or not), and where the pages place them.
    fn blocks(&self) -> Value {
        let cfg = crate::config::load(&crate::config_path()).ok();
        let snap = self.state.hub.subscribe().borrow().clone();
        let placed = |id: &str| -> Vec<String> {
            let Some(cfg) = &cfg else { return Vec::new() };
            cfg.views.iter().flat_map(|v| v.columns.iter().enumerate().filter(|(_, c)| c.split_whitespace().any(|b| b == id)).map(move |(i, _)| format!("{} — column {}", v.name, i + 1))).collect()
        };
        let mut out = Vec::new();
        for b in crate::blocks::all() {
            let id = b.id();
            let error = snap.errors.get(id).cloned();
            let state = if error.is_some() { "error" } else if snap.blocks.contains_key(id) { "on" } else { "off" };
            let repos = cfg.as_ref().map(|c| c.block_repos(id).0.into_iter().map(|r| r.name).collect::<Vec<_>>()).unwrap_or_default();
            out.push(json!({ "id": id, "table": id, "kind": "source", "state": state, "error": error, "pages": placed(id), "repos": repos }));
        }
        let custom = snap.blocks.get("custom_blocks").and_then(|s| s.get("blocks")).and_then(Value::as_array).cloned().unwrap_or_default();
        for c in custom {
            let id = c.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
            let state = if c.get("pending").is_some_and(|p| !p.is_null()) {
                "waiting for trust"
            } else if c.get("error").is_some_and(|e| !e.is_null()) {
                "error"
            } else if c.get("enabled").and_then(Value::as_bool) == Some(false) {
                "off"
            } else {
                "on"
            };
            let page_id = format!("custom:{id}");
            out.push(json!({ "id": page_id, "table": "custom_blocks", "kind": "custom", "label": c.get("label"), "state": state, "error": c.get("error"), "source": c.get("source"), "pages": placed(&page_id) }));
        }
        // The pages' other blocks: Prism's own, read by the core (MRs, Pipelines…).
        let known: Vec<String> = out.iter().filter_map(|b| b["id"].as_str().map(String::from)).collect();
        let mut others: Vec<String> = cfg.iter().flat_map(|c| c.views.iter()).flat_map(|v| v.columns.iter()).flat_map(|c| c.split_whitespace().map(String::from)).filter(|b| b != "---" && !known.contains(b)).collect();
        others.sort();
        others.dedup();
        for id in others {
            out.push(json!({ "id": id, "kind": "core", "pages": placed(&id) }));
        }
        json!({ "blocks": out, "pages": cfg.as_ref().map_or(0, |c| c.views.len()), "default_pages": cfg.as_ref().is_some_and(|c| c.views.is_empty()) })
    }

    fn problems(&self) -> Value {
        let snap = self.state.hub.subscribe().borrow().clone();
        let problems: Vec<Value> = snap.problems.iter().map(|p| json!({ "level": p.level, "where": p.target, "message": p.message, "count": p.count, "at": p.at })).collect();
        let errors: Vec<Value> = snap.errors.iter().map(|(source, e)| json!({ "source": source, "error": e })).collect();
        let view = crate::settings::read(&crate::config_path()).ok();
        json!({
            "config_error": view.as_ref().and_then(|v| v.error.clone()),
            "unread_tables": view.map(|v| v.unread).unwrap_or_default(),
            "sources_in_error": errors,
            "problems": problems,
            "waiting_restart": snap.waiting_restart,
        })
    }

    async fn propose(&self, cfg: Option<&crate::config::Config>, client: u64, kind: ProposalKind, change: Change, why: String, test: Option<Value>) -> Result<Value> {
        if cfg.is_some_and(|c| !c.mcp.proposals) {
            bail!("read-only: [mcp] proposals is off in this Prism. Give the user the TOML to paste in Settings › General › Config file instead.");
        }
        let before = std::fs::read_to_string(crate::config_path()).unwrap_or_default();
        let computed = compute(&before, &change)?;
        if !computed.check.errors.is_empty() {
            return Ok(json!({ "status": "invalid", "errors": computed.check.errors, "warnings": computed.check.warnings }));
        }
        let (client_name, agent) = self.clients.lock().unwrap().iter().find(|c| c.id == client).map(|c| (c.name.clone(), c.agent.clone())).unwrap_or_else(|| ("agent".into(), None));
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let view = Proposal {
            id,
            kind,
            client: client_name,
            agent,
            why,
            at: Utc::now(),
            text: computed.text,
            diff: computed.diff,
            touches: computed.touches,
            overrides: computed.overrides,
            stale: false,
            test,
            block: computed.block,
            warnings: computed.check.warnings.clone(),
            client_id: Some(client),
        };
        let (tx, rx) = oneshot::channel();
        self.proposals.lock().unwrap().push(Pending { view, change, before, reply: Some(tx), answered: None });
        self.publish();
        self.await_answer(id, rx).await
    }

    async fn await_answer(&self, id: u64, rx: oneshot::Receiver<Value>) -> Result<Value> {
        match tokio::time::timeout(WAIT, rx).await {
            Ok(Ok(answer)) => Ok(answer),
            Ok(Err(_)) => Ok(json!({ "status": "dropped", "id": id })),
            Err(_) => Ok(json!({ "status": "waiting", "id": id, "note": "still waiting for the user in Prism: call prism_wait_proposal with this id to keep waiting" })),
        }
    }

    /// Waits again for a proposal's answer (its first call gave up), or gives the one already there.
    async fn wait(&self, id: u64) -> Result<Value> {
        let rx = {
            let mut list = self.proposals.lock().unwrap();
            let p = list.iter_mut().find(|p| p.view.id == id).context("no such proposal (answered and read already, or gone)")?;
            if let Some(a) = p.answered.take() {
                list.retain(|p| p.view.id != id);
                return Ok(a);
            }
            let (tx, rx) = oneshot::channel();
            p.reply = Some(tx);
            rx
        };
        self.await_answer(id, rx).await
    }

    /// The user's answer, from the window.
    async fn answer(&self, id: u64, action: &str, reason: Option<String>, text: Option<String>) -> Result<()> {
        let (change, before, view) = {
            let list = self.proposals.lock().unwrap();
            let p = list.iter().find(|p| p.view.id == id && p.answered.is_none()).context("no such proposal")?;
            (p.change.clone(), p.before.clone(), p.view.clone())
        };
        let path = crate::config_path();
        let answer = match action {
            "refuse" => json!({ "status": "refused", "reason": reason.unwrap_or_default() }),
            "apply" => {
                if let Change::Block(def) = &change {
                    let b = crate::blocks::find("custom_blocks").context("no custom blocks in this Prism")?;
                    anyhow::ensure!(!crate::demo::active(), "{}", crate::demo::READ_ONLY);
                    b.call("save".into(), json!({ "def": def })).await?;
                    json!({ "status": "applied", "note": "added and trusted: it runs now" })
                } else {
                    let current = std::fs::read_to_string(&path).unwrap_or_default();
                    if current != before {
                        // Changed meanwhile: computed again on the file as it is, to apply again.
                        let computed = compute(&current, &change)?;
                        anyhow::ensure!(computed.check.errors.is_empty(), "on the current config.toml, the proposal no longer holds: {}", computed.check.errors.join("; "));
                        let mut list = self.proposals.lock().unwrap();
                        if let Some(p) = list.iter_mut().find(|p| p.view.id == id) {
                            (p.before, p.view.text, p.view.diff, p.view.touches, p.view.overrides) = (current, computed.text, computed.diff, computed.touches, computed.overrides);
                        }
                        drop(list);
                        self.publish();
                        bail!("config.toml changed since the proposal: it is computed again on the current version, check it and apply again");
                    }
                    crate::settings::write(&path, &current, &view.text)?;
                    crate::commands::config::apply_live(&self.state, &path);
                    json!({ "status": "applied", "note": "applied live: check prism_problems() and prism://blocks" })
                }
            }
            // Saved from the TOML panel, as the user changed it.
            "modified" => {
                let text = text.context("text")?;
                let current = std::fs::read_to_string(&path).unwrap_or_default();
                crate::settings::write(&path, &current, &text)?;
                crate::commands::config::apply_live(&self.state, &path);
                let diff: Vec<String> = prism_core::textdiff::around(prism_core::textdiff::lines(&view.text, &text), 2).into_iter().map(|l| format!("{}{}", l.op, l.text)).collect();
                json!({ "status": "applied_modified", "note": "the user changed it before saving: here is what differs from the proposal", "diff_from_proposal": diff.join("\n") })
            }
            other => bail!("no answer {other}"),
        };
        let mut list = self.proposals.lock().unwrap();
        if let Some(i) = list.iter().position(|p| p.view.id == id) {
            match list[i].reply.take().map(|tx| tx.send(answer.clone())) {
                Some(Ok(())) => {
                    list.remove(i);
                }
                // Nobody waits now: kept for `wait_proposal`.
                _ => list[i].answered = Some(answer),
            }
        }
        drop(list);
        self.publish();
        Ok(())
    }
}

/// The window's answer to a proposal: `apply`, `refuse` (with the reason the agent gets) or
/// `modified` (the text saved from the TOML panel).
pub async fn answer(id: u64, action: &str, reason: Option<String>, text: Option<String>) -> Result<()> {
    host()?.answer(id, action, reason, text).await
}

/// `config.toml` changed (Settings, an editor): the proposals made on the former one say so.
pub fn refresh() {
    if let Some(h) = HOST.get() {
        h.publish();
    }
}

struct Computed {
    text: String,
    diff: Vec<prism_core::textdiff::Line>,
    touches: Vec<String>,
    overrides: Vec<String>,
    block: Option<String>,
    check: Validation,
}

/// What a change gives on `file`: the new text, checked, its diff, what it touches.
fn compute(file: &str, change: &Change) -> Result<Computed> {
    let (text, set, block) = match change {
        Change::Config { toml, whole: true } => (toml.clone(), Vec::new(), None),
        Change::Config { toml, whole: false } => {
            let (text, set) = merge::merge(file, toml)?;
            (text, set, None)
        }
        Change::Layout(views) => (merge::replace_views(file, views)?, vec!["views".to_string()], None),
        // Replaced whole, as Apply (`save`) writes it: the diff is what will be written.
        Change::Block(def) => {
            let def: prism_blocks::custom::Def = serde_json::from_value(def.clone()).context("spec")?;
            let entry = prism_blocks::custom::entry(&def)?;
            let text = crate::settings::entry_text(file, "custom_blocks", "id", &def.id, Some(&entry))?;
            (text, vec![format!("custom_blocks[{}]", def.id)], Some(def.id))
        }
    };
    let guarded = guarded_changes(file, &text);
    anyhow::ensure!(guarded.is_empty(), "{}", tr!("an agent does not change {keys}: what runs programs here, opens this computer or what agents may do — change it yourself in Settings", keys = guarded.join(", ")));
    let check = check_text(&text);
    let set: Vec<String> = if set.is_empty() { changed_tables(file, &text) } else { set };
    let mut touches: Vec<String> = set.iter().map(|p| p.split(['.', '[']).next().unwrap_or(p).to_string()).collect();
    touches.sort();
    touches.dedup();
    let blocks: Vec<(String, String)> = crate::blocks::all().iter().map(|b| (b.id().to_string(), b.id().to_string())).collect();
    let mut readers: Vec<String> = touches.iter().flat_map(|t| prism_core::schema::read_by(t, &blocks)).collect();
    readers.dedup();
    let touches = touches.into_iter().map(|t| format!("[{t}]")).chain(readers).collect();
    let inherited = crate::packs::layered(&crate::config_path(), file).map(|(_, i, _)| i).unwrap_or_default();
    let overrides = set.iter().filter(|p| in_table(&inherited, p)).cloned().collect();
    let diff = prism_core::textdiff::around(prism_core::textdiff::lines(file, &text), 3);
    Ok(Computed { text, diff, touches, overrides, block, check })
}

/// What a proposal may not change, from `a` to `b`: what runs a program here (`[open_with]`,
/// `[agents.<cli>] bin`/`args`, `[updates] source`), opens this computer (`[remote]`,
/// `[[machines]]`), what its agents may do (`[mcp]`, `[agents] permission_mode`,
/// `handoff_review`), its packs and profile. Applied, the agent would have widened its own rights.
fn guarded_changes(a: &str, b: &str) -> Vec<String> {
    let (ta, tb) = (a.parse::<toml::Table>().unwrap_or_default(), b.parse::<toml::Table>().unwrap_or_default());
    let at = |t: &toml::Table, path: &[&str]| -> Option<toml::Value> {
        let mut v = t.get(path[0])?.clone();
        for k in &path[1..] {
            v = v.as_table()?.get(*k)?.clone();
        }
        Some(v)
    };
    let mut paths: Vec<Vec<String>> = ["open_with", "remote", "machines", "mcp", "packs", "profile", "updates"].iter().map(|k| vec![k.to_string()]).collect();
    paths.extend([["agents", "permission_mode"], ["agents", "handoff_review"], ["worktrees", "repo"], ["feedback", "repo"]].map(|p| p.map(String::from).to_vec()));
    // Each CLI's table, one added later too.
    let clis: std::collections::BTreeSet<String> = [&ta, &tb].iter().filter_map(|t| t.get("agents")?.as_table()).flat_map(|a| a.iter().filter(|(_, v)| v.is_table()).map(|(k, _)| k.clone())).collect();
    for cli in clis {
        paths.extend(["bin", "args"].map(|k| vec!["agents".to_string(), cli.clone(), k.to_string()]));
    }
    paths
        .into_iter()
        .filter(|p| {
            let p: Vec<&str> = p.iter().map(String::as_str).collect();
            at(&ta, &p) != at(&tb, &p)
        })
        .map(|p| if p.len() == 1 { format!("[{}]", p[0]) } else { format!("[{}] {}", p[..p.len() - 1].join("."), p[p.len() - 1]) })
        .collect()
}

/// The top tables that differ from `a` to `b`.
fn changed_tables(a: &str, b: &str) -> Vec<String> {
    let (ta, tb) = (a.parse::<toml::Table>().unwrap_or_default(), b.parse::<toml::Table>().unwrap_or_default());
    let mut keys: Vec<String> = ta.keys().chain(tb.keys()).filter(|k| ta.get(*k) != tb.get(*k)).cloned().collect();
    keys.sort();
    keys.dedup();
    keys
}

/// `forge.bare_mr_repo` (or `repos[web].forge`) is set in `t`.
fn in_table(t: &toml::Table, path: &str) -> bool {
    let mut at = toml::Value::Table(t.clone());
    for part in path.split('.') {
        let (name, id) = part.split_once('[').map_or((part, None), |(n, r)| (n, Some(r.trim_end_matches(']'))));
        at = match at.get(name) {
            Some(v) => v.clone(),
            None => return false,
        };
        if let Some(id) = id {
            let found = at.as_array().and_then(|list| list.iter().find(|e| ["id", "name"].iter().any(|k| e.get(k).and_then(|v| v.as_str()) == Some(id))).cloned());
            at = match found {
                Some(v) => v,
                None => return false,
            };
        }
    }
    true
}

/// What a check found.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Validation {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// The keys the fragment sets.
    pub sets: Vec<String>,
}

/// A fragment (merged into `config.toml`) or a whole file checked, nothing written: its TOML, its
/// packs, its types against the schema, what the config's reading refuses (a repository named
/// twice, `bare_mr_repo` naming none), the repositories the blocks name, the custom blocks.
pub fn validate(toml: &str, whole: bool) -> Validation {
    let file = std::fs::read_to_string(crate::config_path()).unwrap_or_default();
    let (text, sets) = if whole {
        (toml.to_string(), Vec::new())
    } else {
        match merge::merge(&file, toml) {
            Ok(m) => m,
            Err(e) => return Validation { errors: vec![format!("{e:#}")], ..Default::default() },
        }
    };
    let mut v = check_text(&text);
    v.sets = sets;
    v
}

fn check_text(text: &str) -> Validation {
    let path = crate::config_path();
    let mut v = Validation::default();
    if let Err(e) = text.parse::<toml::Table>() {
        v.errors.push(format!("not valid TOML: {e}"));
        return v;
    }
    match crate::packs::layered(&path, text) {
        Ok((table, _, _)) => {
            let json = serde_json::to_value(&table).unwrap_or_default();
            for issue in prism_core::schema::check(super::schema(), &json) {
                if issue.warning { v.warnings.push(issue.message) } else { v.errors.push(issue.message) }
            }
        }
        Err(e) => v.errors.push(format!("{e:#}")),
    }
    match crate::config::load_text(&path, text) {
        Ok(cfg) => {
            v.errors.extend(crate::blocks::missing_repos(&cfg));
            v.errors.extend(prism_blocks::custom::read_defs(&cfg.table).1);
            let customs: Vec<String> = prism_blocks::custom::read_defs(&cfg.table).0.into_iter().map(|d| format!("custom:{}", d.id)).collect();
            for view in &cfg.views {
                for id in view.columns.iter().flat_map(|c| c.split_whitespace()).filter(|b| b.starts_with("custom:") && !customs.iter().any(|c| c == b)) {
                    v.warnings.push(format!("[[views]] {}: {id} names no [[custom_blocks]] entry", view.name));
                }
            }
        }
        Err(e) => {
            let e = format!("{e:#}");
            if !v.errors.iter().any(|x| e.contains(x.as_str())) {
                v.errors.push(e);
            }
        }
    }
    v.ok = v.errors.is_empty();
    v
}

fn packs() -> Result<Value> {
    let path = crate::config_path();
    let view = crate::settings::read(&path)?;
    let list: Vec<Value> = view
        .packs
        .iter()
        .map(|p| {
            let sets: Vec<String> = view.pack_files.iter().find(|f| f.name == p.name).and_then(|f| f.text.parse::<toml::Table>().ok()).map(|t| t.keys().filter(|k| *k != "pack").cloned().collect()).unwrap_or_default();
            json!({ "name": p.name, "version": p.version, "description": p.description, "source": p.source, "commit": p.commit, "missing": p.missing, "sets_tables": sets })
        })
        .collect();
    Ok(json!({ "packs": list, "note": "a pack is never edited: a key it sets is overridden in config.toml" }))
}

fn layout() -> Result<Value> {
    let cfg = crate::config::load(&crate::config_path())?;
    let views = toml::to_string(&json!({ "views": cfg.views })).unwrap_or_default();
    Ok(json!({ "views": views, "default": cfg.views.is_empty(), "note": "columns: each column's block ids, top to bottom, separated by spaces; \"---\" starts a band below. Propose a new [[views]] with prism_propose_layout." }))
}

/// A key explained: its schema, its value, where it comes from, what reads it, its Settings page.
fn explain(path: &str) -> Result<Value> {
    let schema = super::schema();
    let entry = prism_core::schema::key(schema, path).with_context(|| format!("no key {path} in the schema (prism_search_docs finds the right one)"))?;
    let summary = prism_core::schema::summary(schema, entry);
    let config = crate::config_path();
    let text = std::fs::read_to_string(&config).unwrap_or_default();
    let own = text.parse::<toml::Table>().map(|t| serde_json::to_value(t).unwrap_or_default()).unwrap_or_default();
    let (merged, inherited) = crate::packs::layered(&config, &text).map(|(m, i, _)| (serde_json::to_value(m).unwrap_or_default(), serde_json::to_value(i).unwrap_or_default())).unwrap_or_default();
    let effective = crate::settings::read(&config).map(|v| v.effective).unwrap_or_default();
    let defaults = crate::config::defaults();
    let table = path.split(['.', '[']).next().unwrap_or(path);
    let second = path.split('.').nth(1).map(|k| k.split('[').next().unwrap_or(k));
    let from = |v: &Value| values_at(v, path);
    let current = from(&effective);
    let origin = if !from(&own).is_empty() {
        "config.toml"
    } else if !from(&inherited).is_empty() {
        "a pack"
    } else {
        "the default"
    };
    let packs: Vec<String> = crate::settings::read(&config)
        .map(|v| v.pack_files.into_iter().filter(|f| f.text.parse::<toml::Table>().ok().is_some_and(|t| !values_at(&serde_json::to_value(t).unwrap_or_default(), path).is_empty())).map(|f| f.name).collect())
        .unwrap_or_default();
    let blocks: Vec<(String, String)> = crate::blocks::all().iter().map(|b| (b.id().to_string(), b.id().to_string())).collect();
    // As the config's resources show it: no secret in clear.
    let secret = path.split(['.', '[', ']']).any(merge::secret_key);
    let hidden = |found: Vec<(String, Value)>| merge::mask_json(shown(found), secret);
    Ok(json!({
        "key": path,
        "schema": summary,
        "default": values_at(&defaults, path).into_iter().next().map(|(_, v)| v),
        "current": hidden(current),
        "in_config_toml": hidden(from(&own)),
        "from_packs": packs,
        "merged": hidden(from(&merged)),
        "comes_from": origin,
        "read_by": prism_core::schema::read_by(table, &blocks),
        "settings_page": prism_core::schema::settings_page(table, second),
    }))
}

/// One value, or each entry's (`repos[].forge`: by the entry's name).
fn shown(found: Vec<(String, Value)>) -> Value {
    match found.as_slice() {
        [] => Value::Null,
        [(label, v)] if label.is_empty() => v.clone(),
        many => Value::Object(many.iter().map(|(l, v)| (l.clone(), v.clone())).collect()),
    }
}

/// The values at `path` in `v`: `[]` goes through every entry (labelled by its `name` or `id`),
/// `[n]` and `[name]` through one.
fn values_at(v: &Value, path: &str) -> Vec<(String, Value)> {
    let mut at: Vec<(String, Value)> = vec![(String::new(), v.clone())];
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let (name, index) = part.split_once('[').map_or((part, None), |(n, r)| (n, Some(r.trim_end_matches(']'))));
        at = at.into_iter().filter_map(|(l, v)| v.get(name).cloned().map(|x| (l, x))).collect();
        if let Some(index) = index {
            at = at
                .into_iter()
                .flat_map(|(_, v)| {
                    let list = v.as_array().cloned().unwrap_or_default();
                    let label = |i: usize, e: &Value| e.get("name").or_else(|| e.get("id")).and_then(Value::as_str).map(String::from).unwrap_or_else(|| i.to_string());
                    list.into_iter().enumerate().filter(|(i, e)| index.is_empty() || index == i.to_string() || index == label(*i, e)).map(|(i, e)| (label(i, &e), e)).collect::<Vec<_>>()
                })
                .collect();
        }
    }
    at.into_iter().filter(|(_, v)| !v.is_null()).collect()
}

/// A custom block's declaration from the tool's arguments: `spec` (an object, or its TOML).
fn block_def(params: &Value) -> Result<Value> {
    let spec = params.get("spec").context("spec: the [[custom_blocks]] entry")?;
    let def: prism_blocks::custom::Def = match spec {
        Value::String(text) => {
            let t: toml::Table = text.parse().context("spec: not valid TOML")?;
            // `[[custom_blocks]]` itself, or the entry's keys.
            let entry = t.get("custom_blocks").and_then(|c| c.as_array()).and_then(|a| a.first()).cloned().unwrap_or(toml::Value::Table(t));
            entry.try_into().context("spec")?
        }
        v => serde_json::from_value(v.clone()).context("spec")?,
    };
    def.check()?;
    Ok(serde_json::to_value(def)?)
}

/// A test run in the sandbox: of `spec`, or of a block of the config by `id`.
async fn run_custom_block(params: &Value) -> Result<Value> {
    anyhow::ensure!(!crate::demo::active(), "the demo runs nothing");
    let def = match params.get("id").and_then(Value::as_str) {
        Some(id) if params.get("spec").is_none() => {
            let cfg = crate::config::load(&crate::config_path())?;
            let def = prism_blocks::custom::read_defs(&cfg.table).0.into_iter().find(|d| d.id == id.trim_start_matches("custom:")).with_context(|| format!("no custom block {id}"))?;
            serde_json::to_value(def)?
        }
        _ => block_def(params)?,
    };
    let b = crate::blocks::find("custom_blocks").context("no custom blocks in this Prism")?;
    let tested = b.call("sandbox_try".into(), json!({ "def": def })).await?;
    Ok(json!({ "tested_in_sandbox": tested, "note": "run once without trust in a sandbox: no home folder, no token, a temporary folder, outbound network allowed, 20 s, 256 MB. A block that needs the home or a keychain value fails here and runs once added and trusted." }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A proposal never widens what runs here or what agents may do; the rest it may change.
    #[test]
    fn a_proposal_keeps_off_what_guards_this_computer() {
        let file = "[agents]\npermission_mode = \"default\"\n[agents.claude]\ncost_alert = 5.0\n[clickup]\nsprint_folder = \"S\"\n";
        assert!(guarded_changes(file, &file.replace("\"S\"", "\"T\"")).is_empty());
        assert!(guarded_changes(file, &file.replace("cost_alert = 5.0", "cost_alert = 9.0")).is_empty());
        assert_eq!(guarded_changes(file, &file.replace("\"default\"", "\"acceptEdits\"")), ["[agents] permission_mode"]);
        assert_eq!(guarded_changes(file, &format!("{file}[agents.codex]\nargs = [\"--yolo\"]\n")), ["[agents.codex] args"]);
        assert_eq!(guarded_changes(file, &format!("{file}[[machines]]\nname = \"x\"\nurl = \"https://x\"\n[mcp]\nproposals = true\n")), ["[machines]", "[mcp]"]);
        assert!(compute(file, &Change::Config { toml: "[remote]\nenabled = true\n".into(), whole: false }).is_err());
    }

    #[test]
    fn values_are_found_through_lists() {
        let v = json!({ "forge": { "provider": "gitlab" }, "repos": [{ "name": "api", "forge": "github" }, { "name": "web" }] });
        assert_eq!(shown(values_at(&v, "forge.provider")), json!("gitlab"));
        assert_eq!(shown(values_at(&v, "repos[].forge")), json!({ "api": "github" }));
        assert_eq!(shown(values_at(&v, "repos[web].name")), json!({ "web": "web" }));
        assert_eq!(shown(values_at(&v, "nope.x")), Value::Null);
    }

    #[test]
    fn a_key_set_by_a_pack() {
        let t: toml::Table = "[forge]\nprovider = \"gitlab\"\n[[repos]]\nname = \"web\"\n".parse().unwrap();
        assert!(in_table(&t, "forge.provider"));
        assert!(in_table(&t, "repos[web]"));
        assert!(!in_table(&t, "repos[api]") && !in_table(&t, "forge.enabled"));
    }
}
