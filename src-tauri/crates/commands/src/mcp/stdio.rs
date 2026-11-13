//! `prism mcp`: the MCP server an agent CLI starts (JSON-RPC 2.0, one message per line on standard
//! input and output). What it knows of Prism itself — the help, the reference, the schema, the
//! guide — it carries; what is live — the config, Problems, the blocks, the proposals — it asks the
//! running Prism of the profile, on its socket (`host`). Prism stopped, the live tools say so.

use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc},
};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{oneshot, Mutex},
};

use super::docs;

/// The protocol versions it speaks, newest first.
const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// The running Prism, reached on its socket: one connection, its answers matched by id.
struct Link {
    write: Mutex<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>,
    pending: Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next: AtomicU64,
    hello: Value,
    /// Prism closed the connection.
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl Link {
    async fn open(client: &str, aside: bool) -> Result<Self> {
        let (read, write) = connect().await?;
        let pending: Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Arc::default();
        let waiting = pending.clone();
        let closed: Arc<std::sync::atomic::AtomicBool> = Arc::default();
        let gone = closed.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if let Some(tx) = v.get("id").and_then(Value::as_u64).and_then(|id| waiting.lock().unwrap().remove(&id)) {
                    let _ = tx.send(v);
                }
            }
            // Prism went: whoever waits learns it.
            gone.store(true, std::sync::atomic::Ordering::SeqCst);
            waiting.lock().unwrap().clear();
        });
        let mut link = Self { write: Mutex::new(write), pending, next: AtomicU64::new(1), hello: Value::Null, closed };
        link.hello = link.call("hello", json!({ "client": client, "agent": std::env::var("PRISM_AGENT").unwrap_or_default(), "mode": if aside { "aside" } else { "" } })).await?;
        Ok(link)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let line = format!("{}\n", json!({ "id": id, "method": method, "params": params }));
        self.write.lock().await.write_all(line.as_bytes()).await.context("Prism stopped")?;
        let answer = rx.await.context("Prism stopped while answering")?;
        if let Some(e) = answer.get("error").and_then(Value::as_str) {
            bail!("{e}");
        }
        Ok(answer.get("result").cloned().unwrap_or(Value::Null))
    }

    fn alive(&self) -> bool {
        !self.closed.load(std::sync::atomic::Ordering::SeqCst)
    }
}

type Halves = (Box<dyn tokio::io::AsyncRead + Send + Unpin>, Box<dyn tokio::io::AsyncWrite + Send + Unpin>);

/// The running Prism's socket (a named pipe on Windows), this account's only (`prism_sys::ipc`):
/// another account's under our name never hears the agents, nor answers them.
async fn connect() -> Result<Halves> {
    let at = prism_core::mcp::endpoint();
    let stream = match prism_sys::ipc::connect(&at, &prism_core::profile::data_base()).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(e.into()),
        Err(e) => return Err(e).with_context(|| format!("Prism is not running for the profile {} (no {})", prism_core::profile::active(), at.display())),
    };
    let (read, write) = tokio::io::split(stream);
    Ok((Box::new(read), Box::new(write)))
}

/// The server's state across requests.
struct Server {
    client: std::sync::Mutex<String>,
    /// `prism mcp aside`: the « in passing » tools only (`suggest_task`, `dismiss_task`, `done_task`).
    aside: bool,
    link: Mutex<Option<Arc<Link>>>,
    out: Mutex<tokio::io::Stdout>,
}

impl Server {
    /// The running Prism, connected now if it was not.
    async fn link(&self) -> Result<Arc<Link>> {
        let mut link = self.link.lock().await;
        if let Some(l) = link.as_ref().filter(|l| l.alive()) {
            return Ok(l.clone());
        }
        let client = self.client.lock().unwrap().clone();
        let l = Arc::new(Link::open(&client, self.aside).await?);
        if l.hello.get("enabled") == Some(&Value::Bool(false)) {
            bail!("{}", if self.aside { "[agents.aside] enabled is off in this Prism" } else { "[mcp] enabled is off in this Prism: prism mcp answers nothing" });
        }
        *link = Some(l.clone());
        Ok(l)
    }

    async fn live(&self, method: &str, params: Value) -> Result<Value> {
        self.link().await?.call(method, params).await
    }

    async fn language(&self) -> String {
        match self.link().await {
            Ok(l) => l.hello.get("language").and_then(Value::as_str).filter(|l| *l == "fr").unwrap_or("en").to_string(),
            Err(_) => "en".into(),
        }
    }

    async fn send(&self, msg: Value) {
        let mut out = self.out.lock().await;
        let _ = out.write_all(format!("{msg}\n").as_bytes()).await;
        let _ = out.flush().await;
    }
}

/// Serves MCP on standard input and output until the CLI closes it; `aside`: the « in passing »
/// tools only.
pub async fn run(aside: bool) -> Result<()> {
    let server = Arc::new(Server { client: std::sync::Mutex::new("agent".into()), aside, link: Mutex::new(None), out: Mutex::new(tokio::io::stdout()) });
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            server.send(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "parse error" } })).await;
            continue;
        };
        let server = server.clone();
        tokio::spawn(async move {
            let id = msg.get("id").cloned();
            let method = msg.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
            let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
            // A notification (no id): nothing answered.
            let Some(id) = id else { return };
            let reply = match server.request(&method, params).await {
                Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                Err(RpcError(code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
            };
            server.send(reply).await;
        });
    }
    Ok(())
}

struct RpcError(i64, String);

impl Server {
    async fn request(&self, method: &str, params: Value) -> std::result::Result<Value, RpcError> {
        match method {
            "initialize" => {
                if let Some(name) = params.pointer("/clientInfo/name").and_then(Value::as_str) {
                    *self.client.lock().unwrap() = name.to_string();
                }
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(VERSIONS[0]);
                let version = VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(VERSIONS[0]);
                if self.aside {
                    return Ok(json!({
                        "protocolVersion": version,
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "prism", "title": "Prism", "version": prism_core::packs::PRISM },
                        "instructions": ASIDE_INSTRUCTIONS,
                    }));
                }
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {}, "resources": {} },
                    "serverInfo": { "name": "prism", "title": "Prism", "version": prism_core::packs::PRISM },
                    "instructions": format!("{}{}", prism_core::mcp::skill_body(), if in_agent() { format!("\n\n{ASIDE_INSTRUCTIONS}") } else { String::new() }),
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": if self.aside { aside_tools() } else { let mut t = tools(); if in_agent() && let (Value::Array(t), Value::Array(a)) = (&mut t, aside_tools()) { t.extend(a); } t } })),
            "resources/list" => Ok(json!({ "resources": if self.aside { json!([]) } else { resources() } })),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": [{ "uriTemplate": "prism://help/{page}", "name": "help", "description": "A page of Prism's help (prism://help/05-mrs), in Prism's language", "mimeType": "text/markdown" }] })),
            "resources/read" => {
                let uri = params.get("uri").and_then(Value::as_str).unwrap_or_default().to_string();
                match self.read(&uri).await {
                    Ok((mime, text)) => Ok(json!({ "contents": [{ "uri": uri, "mimeType": mime, "text": text }] })),
                    Err(e) => Err(RpcError(-32002, format!("{e:#}"))),
                }
            }
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                Ok(match self.call(&name, args).await {
                    Ok(v) => json!({ "content": [{ "type": "text", "text": text_of(&v) }] }),
                    Err(e) => json!({ "content": [{ "type": "text", "text": format!("{e:#}") }], "isError": true }),
                })
            }
            other => Err(RpcError(-32601, format!("no method {other}"))),
        }
    }

    async fn read(&self, uri: &str) -> Result<(&'static str, String)> {
        let md = "text/markdown";
        let toml = "application/toml";
        Ok(match uri {
            "prism://config/example" => (toml, docs::EXAMPLE.to_string()),
            "prism://config/pack-example" => (toml, docs::PACK_EXAMPLE.to_string()),
            "prism://config/schema" => ("application/json", serde_json::to_string_pretty(super::schema())?),
            "prism://config/current" => (toml, string(self.live("config_current", json!({})).await?)),
            "prism://config/merged" => (toml, string(self.live("config_merged", json!({})).await?)),
            "prism://blocks" => ("application/json", serde_json::to_string_pretty(&self.live("blocks", json!({})).await?)?),
            "prism://packs" => ("application/json", serde_json::to_string_pretty(&self.live("packs", json!({})).await?)?),
            "prism://skill" => (md, prism_core::mcp::SKILL.to_string()),
            "prism://guide/modules" => (md, docs::guide_modules()),
            "prism://guide/sdk" => ("text/x-rust", docs::SDK.to_string()),
            "prism://guide/web-sdk" => ("text/typescript", docs::WEB_SDK.to_string()),
            other => {
                if let Some(page) = other.strip_prefix("prism://help/") {
                    let (lang, page) = match page.split_once('/') {
                        Some((l, p)) if l.len() == 2 => (l.to_string(), p),
                        _ => (self.language().await, page),
                    };
                    (md, docs::help(&lang, page).with_context(|| format!("no help page {page}: {}", docs::help_pages().join(", ")))?.to_string())
                } else if let Some(file) = other.strip_prefix("prism://guide/example-block/") {
                    let (_, text) = docs::EXAMPLE_BLOCK.iter().find(|(p, _)| p.ends_with(&format!("/{file}"))).with_context(|| format!("no file {file} in the example module"))?;
                    ("text/plain", text.to_string())
                } else {
                    bail!("no resource {other}")
                }
            }
        })
    }

    async fn call(&self, name: &str, args: Value) -> Result<Value> {
        let s = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
        if self.aside && !matches!(name, "suggest_task" | "dismiss_task" | "done_task") {
            bail!("no tool {name}");
        }
        // « In passing » is for the agents Prism launches: not listed, not answered elsewhere.
        if !self.aside && !in_agent() && matches!(name, "suggest_task" | "dismiss_task" | "done_task") {
            bail!("no tool {name}");
        }
        match name {
            "suggest_task" | "dismiss_task" | "done_task" => self.live(name, args).await,
            "prism_search_docs" => {
                let query = s("query").context("query")?;
                let found = docs::search(&query, &self.language().await, super::schema(), 8);
                Ok(if found.is_empty() { json!("nothing found: other words, or read prism://config/example") } else { serde_json::to_value(found)? })
            }
            "prism_explain_key" => {
                let path = s("path").context("path")?;
                match self.live("explain_key", json!({ "path": path })).await {
                    Ok(v) => Ok(v),
                    // Prism stopped: what the schema alone tells.
                    Err(e) if !self.running().await => {
                        let schema = super::schema();
                        let entry = prism_core::schema::key(schema, &path).with_context(|| format!("no key {path} in the schema"))?;
                        let table = path.split(['.', '[']).next().unwrap_or(&path);
                        Ok(json!({ "key": path, "schema": prism_core::schema::summary(schema, entry), "settings_page": prism_core::schema::settings_page(table, path.split('.').nth(1)), "current": format!("unknown: {e:#}") }))
                    }
                    Err(e) => Err(e),
                }
            }
            "prism_validate" => self.live("validate", json!({ "toml": s("toml").context("toml")?, "whole": args.get("whole").and_then(Value::as_bool).unwrap_or(false) })).await,
            "prism_problems" => self.live("problems", json!({})).await,
            "prism_layout" => self.live("layout", json!({})).await,
            "prism_propose_config" => self.live("propose_config", json!({ "toml": s("toml").context("toml")?, "why": s("why").unwrap_or_default(), "whole": args.get("whole").and_then(Value::as_bool).unwrap_or(false) })).await,
            "prism_propose_layout" => self.live("propose_layout", json!({ "views": s("views").context("views")?, "why": s("why").unwrap_or_default() })).await,
            "prism_wait_proposal" => self.live("wait_proposal", json!({ "id": args.get("id").and_then(Value::as_u64).context("id")? })).await,
            "prism_run_custom_block" => self.live("run_custom_block", args).await,
            "prism_create_custom_block" => self.live("create_custom_block", args).await,
            other => bail!("no tool {other}"),
        }
    }

    async fn running(&self) -> bool {
        self.link().await.is_ok()
    }
}

fn string(v: Value) -> String {
    v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())
}

/// A tool's answer as text: a string as it is, anything else as indented JSON.
fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        v => serde_json::to_string_pretty(v).unwrap_or_default(),
    }
}

/// Launched by Prism: an agent of its own (`PRISM_AGENT`), which may flag tasks in passing.
fn in_agent() -> bool {
    std::env::var("PRISM_AGENT").is_ok_and(|a| !a.is_empty())
}

/// What the « in passing » tools are for, as the server's instructions.
const ASIDE_INSTRUCTIONS: &str = "Prism shows the user what you flag with suggest_task: a task worth doing that is OUT of what you were asked (dead code, a stale doc, a missing test, a TODO you confirmed, the same bug elsewhere, a flaw seen in passing). The user launches it in another agent, chains it after you, sends it back to you in this session, makes it a ticket or dismisses it; you are told at your next tool call. Do not do such a task yourself, and do not mention it in your final summary instead: call suggest_task, then carry on with your own task. One exception: a task the user sends back to you comes as a prompt naming its task_id; do that one when the prompt says, then call done_task.";

/// `suggest_task` and `dismiss_task`, as Claude Desktop's `spawn_task` and `dismiss_task`; and
/// `done_task`, for one sent back to the agent's session.
fn aside_tools() -> Value {
    let obj = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    json!([
        {
            "name": "suggest_task",
            "title": "Flag a task in passing",
            "description": "Flag a task you noticed that is out of the scope of what you were asked: Prism shows it to the user, who can launch it in another agent, run it after you, make it a ticket, or dismiss it. Returns at once with a task_id: carry on with your own task. Use it for real, concrete work: dead code, a stale doc, a missing test, a confirmed TODO, the same bug elsewhere, a security flaw seen in passing. Do NOT use it for vague remarks, for trivial fixes you can make right now within your task, for unsure hunches, nor for what your task already covers. Use this tool rather than a sentence of your final summary for such a task. The prompt must stand on its own: another agent, without your context, does it from that prompt alone (paths, what is wrong, what done means). If an answer says already: launched, ticket or dismissed, it was dealt with: do not propose it again.",
            "inputSchema": obj(json!({
                "title": { "type": "string", "description": "Imperative, under 60 characters (\"Remove the unused VAT helper\")" },
                "tldr": { "type": "string", "description": "One or two sentences for the user: what and why" },
                "prompt": { "type": "string", "description": "A self-contained prompt for the agent that will do it: files, what is wrong, what done means" },
                "files": { "type": "array", "items": { "type": "string" }, "description": "The paths concerned, relative to the repository" },
                "kind": { "type": "string", "description": "The kind of work, if one of Prism's kinds fits (Development, Review…)" },
                "needs_current_branch": { "type": "boolean", "description": "True if it depends on your changes not merged yet: it then starts from your branch" }
            }), &["title", "tldr", "prompt"]),
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false }
        },
        {
            "name": "dismiss_task",
            "title": "Withdraw a task flagged in passing",
            "description": "Withdraw a task you flagged with suggest_task that is no longer worth doing: you did it after all within your task, or you were wrong.",
            "inputSchema": obj(json!({ "task_id": { "type": "integer" }, "why": { "type": "string", "description": "A few words for the user" } }), &["task_id"]),
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false }
        },
        {
            "name": "done_task",
            "title": "Mark a task in passing done",
            "description": "Mark done a task in passing the user sent back to you in this session (its prompt names its task_id), once you have done it: Prism shows it done to the user.",
            "inputSchema": obj(json!({ "task_id": { "type": "integer" }, "summary": { "type": "string", "description": "One line on what you did" } }), &["task_id"]),
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false }
        }
    ])
}

fn tools() -> Value {
    let obj = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    let proposes = json!({ "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false });
    json!([
        { "name": "prism_search_docs", "title": "Search Prism's docs", "description": "Search Prism's help pages, its commented config reference (config.example.toml) and the config schema. Returns the best passages with where they are.", "inputSchema": obj(json!({ "query": { "type": "string", "description": "Words about what to do or show (any language)" } }), &["query"]), "annotations": read_only },
        { "name": "prism_explain_key", "title": "Explain a setting", "description": "A config key (forge.bare_mr_repo, repos[].forge, agents.claude.model): its type, default, current value, whether it comes from config.toml or a pack, what reads it and its Settings page.", "inputSchema": obj(json!({ "path": { "type": "string", "description": "Dotted path; [] for every entry of a list, [name] for one" } }), &["path"]), "annotations": read_only },
        { "name": "prism_validate", "title": "Check a config change", "description": "Check a TOML fragment (merged into config.toml: tables key by key, [[x]] entries by their id or name, otherwise added) or a whole file, without writing anything: syntax, packs, types against the schema, references (repositories, pages, custom blocks).", "inputSchema": obj(json!({ "toml": { "type": "string" }, "whole": { "type": "boolean", "description": "toml is the whole config.toml, not a fragment" } }), &["toml"]), "annotations": read_only },
        { "name": "prism_problems", "title": "Prism's problems", "description": "Prism's current warnings and errors: the config's, its sources', the tables nothing reads, what waits for a restart.", "inputSchema": obj(json!({}), &[]), "annotations": read_only },
        { "name": "prism_layout", "title": "Prism's pages", "description": "The pages ([[views]]) and their blocks, as TOML.", "inputSchema": obj(json!({}), &[]), "annotations": read_only },
        { "name": "prism_propose_config", "title": "Propose a config change", "description": "Propose a change of config.toml: Prism shows the user the diff and they apply it, refuse it (the reason comes back) or edit it. Waits for that answer (status applied, refused, applied_modified; or waiting: then call prism_wait_proposal). Never write config.toml yourself. An invalid change is not shown: its errors come back.", "inputSchema": obj(json!({ "toml": { "type": "string", "description": "A fragment merged into config.toml, or the whole file with whole" }, "why": { "type": "string", "description": "One sentence for the user: what it does and why" }, "whole": { "type": "boolean" } }), &["toml", "why"]), "annotations": proposes },
        { "name": "prism_propose_layout", "title": "Propose pages", "description": "Propose the pages: [[views]] entries that replace the current ones whole (see prism_layout). Waits for the user's answer, as prism_propose_config.", "inputSchema": obj(json!({ "views": { "type": "string", "description": "TOML with [[views]] entries only" }, "why": { "type": "string" } }), &["views", "why"]), "annotations": proposes },
        { "name": "prism_wait_proposal", "title": "Wait for a proposal's answer", "description": "Keep waiting for the user's answer to a proposal whose call returned status waiting.", "inputSchema": obj(json!({ "id": { "type": "integer" } }), &["id"]), "annotations": read_only },
        { "name": "prism_run_custom_block", "title": "Test a custom block", "description": "Run a custom block once, untrusted, in a sandbox (no home folder, no token, a temporary folder, outbound network, 20 s, 256 MB): its JSON output and whether it follows the contract of its kind. Nothing is shown nor kept. Give spec (the [[custom_blocks]] entry, as TOML or an object) or the id of one in the config.", "inputSchema": obj(json!({ "spec": { "description": "The entry: id, label, command or script, args, every_seconds, env, actions…" }, "id": { "type": "string" } }), &[]), "annotations": { "readOnlyHint": true, "openWorldHint": true } },
        { "name": "prism_create_custom_block", "title": "Propose a custom block", "description": "Propose a new custom block (or a change of one): the user reads its script and trusts it in Prism as they add it. Test it first with prism_run_custom_block and pass that result as test. Waits for the answer, as prism_propose_config. Place it on a page with prism_propose_layout (custom:<id>).", "inputSchema": obj(json!({ "spec": { "description": "The [[custom_blocks]] entry, as TOML or an object" }, "why": { "type": "string" }, "test": { "description": "What prism_run_custom_block gave" } }), &["spec", "why"]), "annotations": proposes },
    ])
}

fn resources() -> Value {
    let r = |uri: &str, name: &str, description: &str, mime: &str| json!({ "uri": uri, "name": name, "description": description, "mimeType": mime });
    let mut list = vec![
        r("prism://skill", "skill", "What Prism is, its words, which tool to call when", "text/markdown"),
        r("prism://config/example", "config.example.toml", "The commented reference of every setting", "application/toml"),
        r("prism://config/pack-example", "packs/example.toml", "A team pack, commented", "application/toml"),
        r("prism://config/schema", "schema", "JSON schema of every table, with what reads it and its Settings page", "application/json"),
        r("prism://config/current", "config.toml", "The user's config.toml (secrets hidden; Prism running)", "application/toml"),
        r("prism://config/merged", "merged config", "config.toml over its packs, as Prism reads it (Prism running)", "application/toml"),
        r("prism://blocks", "blocks", "The blocks known, their state and the pages placing them (Prism running)", "application/json"),
        r("prism://packs", "packs", "The packs installed, their source and the tables they set (Prism running)", "application/json"),
        r("prism://guide/modules", "guide: modules", "Writing a module: a team's blocks loaded at run time", "text/markdown"),
        r("prism://guide/sdk", "prism-module sdk", "What a module's back end (Rust, WebAssembly) may use of Prism", "text/x-rust"),
        r("prism://guide/web-sdk", "@prism/sdk", "What a module's window side may use of Prism", "text/typescript"),
    ];
    for (path, _) in docs::EXAMPLE_BLOCK {
        let file = path.rsplit('/').next().unwrap_or(path);
        let uri = format!("prism://guide/example-block/{file}");
        list.push(r(&uri, &format!("example block: {path}"), "The example module hello, a whole module as a model", "text/plain"));
    }
    for page in docs::help_pages() {
        list.push(r(&format!("prism://help/{page}"), &format!("help: {page}"), "A page of Prism's help", "text/markdown"));
    }
    Value::Array(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server { client: std::sync::Mutex::new("test".into()), aside: false, link: Mutex::new(None), out: Mutex::new(tokio::io::stdout()) }
    }

    #[tokio::test]
    async fn initialize_lists_and_reads_without_prism() {
        let s = server();
        let init = s.request("initialize", json!({ "protocolVersion": "2025-03-26", "clientInfo": { "name": "claude-code" } })).await.ok().unwrap();
        assert_eq!(init["protocolVersion"], "2025-03-26");
        assert!(init["instructions"].as_str().unwrap().starts_with("# Prism"));
        let tools = s.request("tools/list", json!({})).await.ok().unwrap();
        assert!(tools["tools"].as_array().unwrap().iter().any(|t| t["name"] == "prism_propose_config"));
        let example = s.request("resources/read", json!({ "uri": "prism://config/example" })).await.ok().unwrap();
        assert!(example["contents"][0]["text"].as_str().unwrap().contains("[agents]"));
        let mrs = s.request("resources/read", json!({ "uri": "prism://help/fr/05-mrs" })).await.ok().unwrap();
        assert!(!mrs["contents"][0]["text"].as_str().unwrap().is_empty());
        let block = s.request("resources/read", json!({ "uri": "prism://guide/example-block/hello.rs" })).await.ok().unwrap();
        assert!(block["contents"][0]["text"].as_str().unwrap().contains("impl Block"));
        assert!(s.request("nope", json!({})).await.is_err());
    }

    #[tokio::test]
    async fn the_aside_mode_offers_its_own_tools_only() {
        let s = Server { client: std::sync::Mutex::new("test".into()), aside: true, link: Mutex::new(None), out: Mutex::new(tokio::io::stdout()) };
        let init = s.request("initialize", json!({ "protocolVersion": "2025-06-18" })).await.ok().unwrap();
        assert!(init["instructions"].as_str().unwrap().contains("suggest_task"));
        let tools = s.request("tools/list", json!({})).await.ok().unwrap();
        let names: Vec<&str> = tools["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["suggest_task", "dismiss_task", "done_task"]);
        let refused = s.request("tools/call", json!({ "name": "prism_problems" })).await.ok().unwrap();
        assert_eq!(refused["isError"], true);
    }
}
