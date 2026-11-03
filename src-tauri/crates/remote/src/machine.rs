//! Another machine's Prism, opened in this one's window (`[[machines]]`): its peer socket
//! (`/peer/ws`, `peers` there) with the token it made for this one, the calls of the window sent
//! over it, and what it pushes (its snapshot, then its changes) handed to the window; its agents'
//! terminals each over their own socket (`/peer/tty`). The socket reconnects by itself; a call made
//! meanwhile waits for it a moment.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};

/// A call waits this long for the socket, then this long for its answer.
const WAIT_OPEN: Duration = Duration::from_secs(12);
const WAIT_ANSWER: Duration = Duration::from_secs(120);
/// Nothing heard for this long (the other side says something every 10 s): the socket is dead.
const SILENT: Duration = Duration::from_secs(30);

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;

/// A machine's address and its token.
#[derive(Clone)]
pub struct Target {
    /// `https://build-srv.tail1234.ts.net`, `http://127.0.0.1:7777` (`ssh -L`).
    pub url: String,
    pub token: String,
}

impl Target {
    /// `path` (`/peer/pair`) on the machine, over HTTPS or to a tunnel's end.
    fn http(&self, path: &str) -> Result<String> {
        let base = self.url.trim().trim_end_matches('/');
        let host = base.strip_prefix("http://").map(|r| r.split(['/', ':']).next().unwrap_or_default());
        if !base.starts_with("https://") && !matches!(host, Some("127.0.0.1" | "localhost" | "[::1]")) {
            bail!("{base}: https, or http to 127.0.0.1 only (a tunnel's end)");
        }
        Ok(format!("{base}{path}"))
    }

    /// `path` (`/peer/ws`) as a socket's request, the token in it.
    fn request(&self, path: &str) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request> {
        let base = self.url.trim().trim_end_matches('/');
        let ws = if let Some(rest) = base.strip_prefix("https://") {
            format!("wss://{rest}{path}")
        } else if let Some(rest) = base.strip_prefix("http://") {
            // Clear only to this machine (a tunnel's end, `ssh -L`): the token never crosses a network unencrypted.
            let host = rest.split(['/', ':']).next().unwrap_or_default();
            if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
                bail!("{base}: https, or http to 127.0.0.1 only (a tunnel's end)");
            }
            format!("ws://{rest}{path}")
        } else {
            bail!("{base}: an https:// address");
        };
        let mut req = ws.into_client_request().with_context(|| format!("{base}: not an address"))?;
        req.headers_mut().insert("authorization", HeaderValue::from_str(&format!("Bearer {}", self.token.trim())).context("the token")?);
        Ok(req)
    }
}

/// Pairs with the machine at `url` (`prism peer pair` there, its code given here as `code`), this
/// computer named `name` there: its token once a person accepted it there, within five minutes.
pub async fn pair(url: &str, code: &str, name: &str) -> Result<String> {
    let target = Target { url: url.to_string(), token: String::new() };
    let http = reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?;
    let claim = crate::webauthn::random();
    let res = http.post(target.http("/peer/pair")?).json(&json!({ "code": code, "name": name, "claim": claim })).send().await.context("the machine does not answer")?;
    if !res.status().is_success() {
        let body: Value = res.json().await.unwrap_or_default();
        bail!("{}", body["error"].as_str().unwrap_or("refused"));
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5 * 60);
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let Ok(res) = http.post(target.http("/peer/pair/state")?).json(&json!({ "claim": claim })).send().await else { continue };
        let v: Value = res.json().await.unwrap_or_default();
        match v["state"].as_str() {
            Some("paired") => return v["token"].as_str().map(String::from).context("no token"),
            Some("waiting") => {}
            _ => bail!("refused there, or the pairing ended"),
        }
    }
    bail!("not accepted there in time")
}

/// What a machine says of itself (`/peer/summary`): its agents running, those waiting for you.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Summary {
    pub agents: u32,
    pub waiting: Vec<prism_core::state::WaitingAgent>,
}

/// The machine's summary, within a few seconds.
pub async fn summary(target: &Target) -> Result<Summary> {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let http = CLIENT.get_or_init(|| reqwest::Client::builder().timeout(Duration::from_secs(5)).build().expect("an HTTP client"));
    let res = http.get(target.http("/peer/summary")?).bearer_auth(target.token.trim()).send().await.context("out of reach")?;
    if res.status() == 401 {
        bail!("the machine does not know this token");
    }
    anyhow::ensure!(res.status().is_success(), "{}", res.status());
    Ok(res.json().await?)
}

/// What the window hears of the machine: its messages (`hello`, `snapshot`, `changes`, `alive`) as
/// they come, and `{"t":"state","connected":…,"error":…}` when the socket opens or drops.
pub type OnMessage = Arc<dyn Fn(Value) + Send + Sync>;

/// The socket to a machine, kept open until dropped.
pub struct Machine {
    target: Target,
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    next: Mutex<u64>,
    connected: Arc<tokio::sync::watch::Sender<bool>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Machine {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Machine {
    /// Opens the socket to `target`, `on_message` hearing what comes, from the snapshot on.
    pub fn open(target: Target, on_message: OnMessage) -> Self {
        let (out, rx) = mpsc::unbounded_channel();
        let pending: Pending = Default::default();
        let connected = Arc::new(tokio::sync::watch::Sender::new(false));
        let task = tokio::spawn(run(target.clone(), rx, pending.clone(), connected.clone(), on_message));
        Self { target, out, pending, next: Mutex::new(1), connected, task }
    }

    /// A command of the window, answered by the machine.
    pub async fn call(&self, command: &str, args: Value) -> Result<Value> {
        let mut open = self.connected.subscribe();
        if !*open.borrow() {
            tokio::time::timeout(WAIT_OPEN, open.wait_for(|c| *c)).await.map_err(|_| anyhow!("the machine is out of reach"))?.map_err(|_| anyhow!("the machine is closed"))?;
        }
        let id = {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            *n
        };
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let msg = json!({ "t": "call", "id": id, "command": command, "args": args }).to_string();
        if self.out.send(msg).is_err() {
            self.pending.lock().unwrap().remove(&id);
            bail!("the machine is closed");
        }
        match tokio::time::timeout(WAIT_ANSWER, rx).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => bail!("the connection to the machine dropped: try again"),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                bail!("no answer from the machine")
            }
        }
    }

    /// An agent's terminal on the machine, typed in, at `cols` × `rows`: its output through
    /// `on_data` until `on_end`.
    pub async fn attach(&self, agent: &str, cols: u16, rows: u16, on_data: Box<dyn Fn(Vec<u8>) + Send>, on_end: Box<dyn FnOnce() + Send>) -> Result<Tty> {
        let agent_q: String = url::form_urlencoded::byte_serialize(agent.as_bytes()).collect();
        let req = self.target.request(&format!("/peer/tty?agent={agent_q}&cols={cols}&rows={rows}"))?;
        let (socket, _) = tokio_tungstenite::connect_async(req).await.context("the machine's terminal")?;
        let (mut tx, mut rx) = socket.split();
        let (keys, mut queue) = mpsc::unbounded_channel::<Message>();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    m = queue.recv() => match m {
                        Some(m) => if tx.send(m).await.is_err() { break },
                        None => break,
                    },
                    m = rx.next() => match m {
                        Some(Ok(Message::Binary(b))) => on_data(b.to_vec()),
                        Some(Ok(Message::Text(t))) => {
                            if serde_json::from_str::<Value>(&t).is_ok_and(|v| v["t"] == "error") {
                                tracing::warn!("machine terminal: {t}");
                                break;
                            }
                        }
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                        Some(Ok(_)) => {}
                    },
                }
            }
            let _ = tx.close().await;
            on_end();
        });
        Ok(Tty { keys, task })
    }
}

/// An agent's terminal on a machine: keys and sizes go to it; dropped, it is detached.
pub struct Tty {
    keys: mpsc::UnboundedSender<Message>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Tty {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Tty {
    pub fn write(&self, data: &[u8]) -> Result<()> {
        self.keys.send(Message::Binary(data.to_vec().into())).map_err(|_| anyhow!("the terminal is closed"))
    }
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.keys.send(Message::Text(json!({ "t": "resize", "cols": cols, "rows": rows }).to_string().into())).map_err(|_| anyhow!("the terminal is closed"))
    }
}

/// The socket, open again and again until the `Machine` is dropped: each time it opens, the
/// machine sends its snapshot whole.
async fn run(target: Target, mut out: mpsc::UnboundedReceiver<String>, pending: Pending, connected: Arc<tokio::sync::watch::Sender<bool>>, on_message: OnMessage) {
    let mut delay = Duration::from_secs(1);
    loop {
        let error = match connect(&target, &mut out, &pending, &connected, &on_message).await {
            Ok(()) => None,
            Err(e) => Some(format!("{e:#}")),
        };
        connected.send_replace(false);
        // The calls it carried may not have reached the machine: they fail.
        for (_, tx) in pending.lock().unwrap().drain() {
            let _ = tx.send(Err(anyhow!("the connection to the machine dropped: try again")));
        }
        on_message(json!({ "t": "state", "connected": false, "error": error }));
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(10));
        if error.is_none() {
            delay = Duration::from_secs(1);
        }
    }
}

async fn connect(target: &Target, out: &mut mpsc::UnboundedReceiver<String>, pending: &Pending, connected: &tokio::sync::watch::Sender<bool>, on_message: &OnMessage) -> Result<()> {
    let (socket, _) = tokio_tungstenite::connect_async(target.request("/peer/ws")?).await.map_err(|e| match e {
        tokio_tungstenite::tungstenite::Error::Http(r) if r.status() == 401 => anyhow!("the machine does not know this token (prism peer add there)"),
        e => anyhow!(e),
    })?;
    let (mut tx, mut rx) = socket.split();
    // Calls queued while it was closed are dropped: they already failed, or will wait for it.
    while out.try_recv().is_ok() {}
    connected.send_replace(true);
    on_message(json!({ "t": "state", "connected": true, "error": null }));
    loop {
        tokio::select! {
            m = out.recv() => {
                let Some(m) = m else { return Ok(()) };
                tx.send(Message::Text(m.into())).await?;
            }
            m = tokio::time::timeout(SILENT, rx.next()) => {
                let m = match m {
                    Err(_) => bail!("the machine went silent"),
                    Ok(None) => return Ok(()),
                    Ok(Some(m)) => m?,
                };
                let text = match m {
                    Message::Text(t) => t,
                    Message::Close(frame) => {
                        if let Some(f) = frame.filter(|f| u16::from(f.code) == 4401) {
                            bail!("the machine revoked this token ({})", f.reason);
                        }
                        return Ok(());
                    }
                    _ => continue,
                };
                let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                if v["t"] == "result" {
                    let Some(tx) = v["id"].as_u64().and_then(|id| pending.lock().unwrap().remove(&id)) else { continue };
                    let answer = match v.get("error") {
                        Some(e) => Err(anyhow!(e.as_str().map(String::from).unwrap_or_else(|| e.to_string()))),
                        None => Ok(v.get("ok").cloned().unwrap_or(Value::Null)),
                    };
                    let _ = tx.send(answer);
                } else {
                    on_message(v);
                }
            }
        }
    }
}
