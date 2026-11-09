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

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Answer>>>>>;

/// A call's answer: its value, or the machine asking the person at this computer to confirm it
/// first (what it classes critical), with the purpose to call it again with.
#[derive(Debug)]
pub enum Answer {
    Value(Value),
    Confirm(String),
}

/// A machine's address and its token.
#[derive(Clone)]
pub struct Target {
    /// `https://build-srv.tail1234.ts.net`, `http://127.0.0.1:7777` (`ssh -L`).
    pub url: String,
    pub token: String,
    /// One of its profiles reached through it (`/peer/p/<profile>/`); `None`, the one it answers at.
    pub profile: Option<String>,
    /// A profile of this computer, through its local socket (`Remote::serve_local`): no token,
    /// `url` unused.
    pub socket: Option<std::path::PathBuf>,
}

impl Target {
    /// A profile of this computer, its engine listening at `socket`.
    pub fn local(socket: std::path::PathBuf) -> Self {
        Self { url: "http://localhost".into(), token: String::new(), profile: None, socket: Some(socket) }
    }

    /// An HTTP client for it: through its socket when local.
    fn client(&self, timeout: Option<Duration>) -> Result<reqwest::Client> {
        let mut b = reqwest::Client::builder();
        if let Some(t) = timeout {
            b = b.timeout(t);
        }
        #[cfg(unix)]
        if let Some(path) = &self.socket {
            b = b.unix_socket(path.clone());
        }
        Ok(b.build()?)
    }

    /// A peer route (`ws`, `tty?…`) of its profile.
    fn peer(&self, rest: &str) -> String {
        match &self.profile {
            Some(id) => format!("/peer/p/{}/{rest}", url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>()),
            None => format!("/peer/{rest}"),
        }
    }
    /// Its address, checked: HTTPS, or HTTP to this computer only (a tunnel's end, `ssh -L`):
    /// the token never crosses a network unencrypted. Whether it is HTTPS, and the address.
    fn base(&self) -> Result<(bool, &str)> {
        if self.socket.is_some() {
            return Ok((false, "http://localhost"));
        }
        let base = self.url.trim().trim_end_matches('/');
        let parsed = url::Url::parse(base).with_context(|| format!("{base}: not an address"))?;
        match parsed.scheme() {
            "https" => Ok((true, base)),
            "http" if matches!(parsed.host(), Some(url::Host::Ipv4(ip)) if ip.is_loopback()) || matches!(parsed.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback()) || parsed.host_str() == Some("localhost") => Ok((false, base)),
            _ => bail!("{base}: https, or http to 127.0.0.1 only (a tunnel's end)"),
        }
    }

    /// `path` (`/peer/pair`) on the machine, over HTTPS or to a tunnel's end.
    fn http(&self, path: &str) -> Result<String> {
        Ok(format!("{}{path}", self.base()?.1))
    }

    /// `path` (`/peer/ws`) as a socket's request, the token in it.
    fn request(&self, path: &str) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request> {
        let (secure, base) = self.base()?;
        let rest = base.split_once("://").map_or(base, |(_, r)| r);
        let ws = format!("{}://{rest}{path}", if secure { "wss" } else { "ws" });
        let mut req = ws.into_client_request().with_context(|| format!("{base}: not an address"))?;
        if self.socket.is_some() {
            return Ok(req);
        }
        req.headers_mut().insert("authorization", HeaderValue::from_str(&format!("Bearer {}", self.token.trim())).context("the token")?);
        Ok(req)
    }
}

/// Pairs with the machine at `url` (`prism peer pair` there, its code given here as `code`), this
/// computer named `name` there: its token once a person accepted it there, within five minutes.
pub async fn pair(url: &str, code: &str, name: &str) -> Result<String> {
    let target = Target { url: url.to_string(), token: String::new(), profile: None, socket: None };
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
    /// Its profiles, the hub first (none from an older Prism).
    #[serde(default)]
    pub profiles: Vec<prism_core::state::MachineProfile>,
}

/// The machine's summary, within a few seconds.
pub async fn summary(target: &Target) -> Result<Summary> {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let local;
    let http = match target.socket {
        Some(_) => {
            local = target.client(Some(Duration::from_secs(5)))?;
            &local
        }
        None => CLIENT.get_or_init(|| reqwest::Client::builder().timeout(Duration::from_secs(5)).build().expect("an HTTP client")),
    };
    let res = http.get(target.http(&target.peer("summary"))?).bearer_auth(target.token.trim()).send().await.context("out of reach")?;
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

    /// A command of the window, answered by the machine; one it asks to confirm first fails.
    pub async fn call(&self, command: &str, args: Value) -> Result<Value> {
        match self.ask(command, args, None).await? {
            Answer::Value(v) => Ok(v),
            Answer::Confirm(_) => bail!("{command}: to be confirmed at this computer first"),
        }
    }

    /// A command of the window, `confirmed` the purpose the machine asked to confirm (`Confirm`).
    pub async fn ask(&self, command: &str, args: Value, confirmed: Option<&str>) -> Result<Answer> {
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
        let mut msg = json!({ "t": "call", "id": id, "command": command, "args": args });
        if let Some(c) = confirmed {
            msg["confirmed"] = json!(c);
        }
        let msg = msg.to_string();
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

    /// A file it prepared (`agent_files_download`, its `token`) written to `dest` on this computer,
    /// streamed: whole, or not at all (a part file first, renamed once done).
    pub async fn download(&self, token: &str, dest: &std::path::Path) -> Result<()> {
        anyhow::ensure!(!token.is_empty() && token.chars().all(|c| c.is_ascii_hexdigit()), "not a download");
        let url = self.target.http(&self.target.peer(&format!("download/{token}")))?;
        let res = self.target.client(None)?.get(url).bearer_auth(self.target.token.trim()).send().await.context("the machine does not answer")?;
        anyhow::ensure!(res.status().is_success(), "the machine refused the download ({})", res.status());
        let part = dest.with_extension(format!("{}.part", dest.extension().and_then(|e| e.to_str()).unwrap_or("")));
        let written = async {
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::File::create(&part).await.with_context(|| part.display().to_string())?;
            let mut body = res.bytes_stream();
            while let Some(chunk) = body.next().await {
                file.write_all(&chunk.context("the download was cut")?).await?;
            }
            file.flush().await?;
            anyhow::Ok(())
        }
        .await;
        match written {
            Ok(()) => tokio::fs::rename(&part, dest).await.with_context(|| dest.display().to_string()),
            Err(e) => {
                let _ = tokio::fs::remove_file(&part).await;
                Err(e)
            }
        }
    }

    /// An agent's terminal on the machine, typed in, at `cols` × `rows`: its output through
    /// `on_data` until `on_end`.
    pub async fn attach(&self, agent: &str, cols: u16, rows: u16, on_data: Box<dyn Fn(Vec<u8>) + Send>, on_end: Box<dyn FnOnce() + Send>) -> Result<Tty> {
        let agent_q: String = url::form_urlencoded::byte_serialize(agent.as_bytes()).collect();
        let req = self.target.request(&self.target.peer(&format!("tty?agent={agent_q}&cols={cols}&rows={rows}")))?;
        let (keys, queue) = mpsc::unbounded_channel::<Message>();
        let task = match &self.target.socket {
            #[cfg(unix)]
            Some(path) => {
                let io = tokio::net::UnixStream::connect(path).await.context("the profile's terminal")?;
                let (socket, _) = tokio_tungstenite::client_async(req, io).await.context("the profile's terminal")?;
                tokio::spawn(pump(socket, queue, on_data, on_end))
            }
            _ => {
                let (socket, _) = tokio_tungstenite::connect_async(req).await.context("the machine's terminal")?;
                tokio::spawn(pump(socket, queue, on_data, on_end))
            }
        };
        Ok(Tty { keys, task })
    }
}

/// A terminal's socket: the keys queued sent to it, its output to `on_data`, until either side ends.
async fn pump<S>(socket: tokio_tungstenite::WebSocketStream<S>, mut queue: mpsc::UnboundedReceiver<Message>, on_data: Box<dyn Fn(Vec<u8>) + Send>, on_end: Box<dyn FnOnce() + Send>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
        let (mut tx, mut rx) = socket.split();
        {
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

/// Why a socket closed or did not open, for the window to say in words (its detail aside).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum MachineReason {
    /// Its token refused, revoked.
    Token,
    /// Nothing listens: its Prism closed, its remote off.
    Refused,
    /// Its name not found: the tunnel down.
    UnknownHost,
    /// No route, a timeout, silent.
    Unreachable,
    /// Its HTTPS refused.
    Certificate,
    /// Anything else.
    Dropped,
}

/// What the system said of a socket, as a reason.
fn reason(detail: &str) -> MachineReason {
    let d = detail.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| d.contains(w));
    if has(&["token"]) {
        MachineReason::Token
    } else if has(&["connection refused"]) {
        MachineReason::Refused
    } else if has(&["lookup address", "nodename nor servname", "name or service not known", "no such host", "dns"]) {
        MachineReason::UnknownHost
    } else if has(&["certificate", "tls alert", "tls handshake"]) {
        MachineReason::Certificate
    } else if has(&["timed out", "silent", "unreachable", "network is down", "no route"]) {
        MachineReason::Unreachable
    } else {
        MachineReason::Dropped
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
        let reason = error.as_deref().map(reason);
        on_message(json!({ "t": "state", "connected": false, "error": error, "reason": reason }));
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(10));
        if error.is_none() {
            delay = Duration::from_secs(1);
        }
    }
}

async fn connect(target: &Target, out: &mut mpsc::UnboundedReceiver<String>, pending: &Pending, connected: &tokio::sync::watch::Sender<bool>, on_message: &OnMessage) -> Result<()> {
    let req = target.request(&target.peer("ws"))?;
    #[cfg(unix)]
    if let Some(path) = &target.socket {
        let io = tokio::net::UnixStream::connect(path).await.with_context(|| format!("the profile's Prism ({})", path.display()))?;
        let (socket, _) = tokio_tungstenite::client_async(req, io).await?;
        return converse(socket, out, pending, connected, on_message).await;
    }
    let (socket, _) = tokio_tungstenite::connect_async(req).await.map_err(|e| match e {
        tokio_tungstenite::tungstenite::Error::Http(r) if r.status() == 401 => anyhow!("the machine does not know this token (prism peer add there)"),
        e => anyhow!(e),
    })?;
    converse(socket, out, pending, connected, on_message).await
}

/// The socket open: calls sent, answers handed back, what the machine pushes to `on_message`.
async fn converse<S>(socket: tokio_tungstenite::WebSocketStream<S>, out: &mut mpsc::UnboundedReceiver<String>, pending: &Pending, connected: &tokio::sync::watch::Sender<bool>, on_message: &OnMessage) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut tx, mut rx) = socket.split();
    // Calls queued for the socket that dropped: failed now (their pending entry may have come in
    // after the drop's), never sent to a new one that knows nothing of them.
    while let Ok(m) = out.try_recv() {
        let id = serde_json::from_str::<Value>(&m).ok().and_then(|v| v["id"].as_u64());
        if let Some(tx) = id.and_then(|id| pending.lock().unwrap().remove(&id)) {
            let _ = tx.send(Err(anyhow!("the connection to the machine dropped: try again")));
        }
    }
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
                    let answer = match (v.get("error"), v["stepUp"]["purpose"].as_str()) {
                        (Some(e), _) => Err(anyhow!(e.as_str().map(String::from).unwrap_or_else(|| e.to_string()))),
                        (None, Some(p)) => Ok(Answer::Confirm(p.to_string())),
                        (None, None) => Ok(Answer::Value(v.get("ok").cloned().unwrap_or(Value::Null))),
                    };
                    let _ = tx.send(answer);
                } else {
                    on_message(v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// What the window says of a socket closed, from what the system said.
    #[test]
    fn tells_why_a_machine_is_out_of_reach() {
        use super::{reason, MachineReason::*};
        assert_eq!(reason("IO error: Connection refused (os error 61)"), Refused);
        assert_eq!(reason("IO error: failed to lookup address information: nodename nor servname provided"), UnknownHost);
        assert_eq!(reason("the machine went silent"), Unreachable);
        assert_eq!(reason("IO error: invalid peer certificate: UnknownIssuer"), Certificate);
        assert_eq!(reason("the machine does not know this token (prism peer add there)"), Token);
        assert_eq!(reason("Connection reset without closing handshake"), Dropped);
        assert_eq!(serde_json::to_value(UnknownHost).unwrap(), "unknown_host");
    }
}
