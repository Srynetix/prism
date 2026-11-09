//! `prism sessions`: the process that holds Prism's own sessions (`local::Server`) where there is
//! no tmux (Windows), so that the agents outlive the window as they do in tmux's server. Prism
//! reaches it through the profile's named pipe (a Unix socket elsewhere, to try it and test it),
//! and starts it when it first needs a session (`Remote`); it ends a few seconds after its last
//! session.
//!
//! On the wire, frames: a kind (`JSON`, `DATA`), a length (u32, little-endian), the bytes. A
//! connection asks a request at a time (`Request`, JSON) and is answered (`Reply`). An `Attach`
//! turns it into a client's: its id answered, then what the session shows, as data frames, until
//! the session or the client ends (the connection closed). What the client types and its size go
//! through another connection (`Write`, `Resize`): Windows serializes a pipe's reads and writes
//! when it is opened without overlapped I/O, as here.

use std::{
    collections::HashMap,
    io::{self, Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::{
    local::{self, Attached, Engine, OnData, OnEnd},
    Pane, ShellSession,
};

/// Changed with a request or an answer: a Prism and a `prism sessions` of two versions must not
/// misread each other.
pub const PROTOCOL: u32 = 1;

const JSON: u8 = b'J';
const DATA: u8 = b'D';
/// More is a broken peer, not a frame.
const MAX_FRAME: usize = 64 << 20;

/// How long `prism sessions` waits, without a session, before it ends.
const IDLE: Duration = Duration::from_secs(10);

/// How long Prism waits for the `prism sessions` it started to answer.
const STARTING: Duration = Duration::from_secs(10);

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Hello { protocol: u32 },
    NewSession { name: String, cwd: PathBuf, env: Vec<String>, argv: Vec<String> },
    Respawn { name: String, cwd: PathBuf, env: Vec<String>, argv: Vec<String> },
    Kill { name: String },
    Panes,
    Pane { name: String },
    Processes,
    Shells,
    SetNote { name: String, key: String, value: String },
    EndWithProgram { name: String },
    SendKeys { name: String, keys: Vec<String> },
    Paste { name: String, text: String },
    History { name: String, lines: u32, screen: bool },
    Text { name: String, lines: u32 },
    Screen { name: String },
    WindowSize { name: String },
    Attach { name: String, size: Option<(u16, u16)>, read_only: bool },
    /// Followed by a data frame: what the client types.
    Write { client: u64 },
    Resize { client: u64, cols: u16, rows: u16 },
    Detach { client: u64 },
}

#[derive(Serialize, Deserialize, Debug)]
enum Reply {
    Ok(Value),
    Err(String),
}

/// Where `prism sessions` listens: the profile's named pipe on Windows, its socket elsewhere.
#[derive(Clone, Debug)]
pub struct Endpoint(String);

impl Endpoint {
    pub fn profile() -> Self {
        if cfg!(windows) {
            Self(prism_core::mcp::sessions_pipe_name())
        } else {
            Self(prism_core::mcp::sessions_socket_path().display().to_string())
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Frames

fn frame(kind: u8, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + bytes.len());
    out.push(kind);
    out.extend((bytes.len() as u32).to_le_bytes());
    out.extend(bytes);
    out
}

fn write_frame(w: &mut impl Write, kind: u8, bytes: &[u8]) -> io::Result<()> {
    w.write_all(&frame(kind, bytes))?;
    w.flush()
}

fn read_frame(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut head = [0u8; 5];
    r.read_exact(&mut head)?;
    let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too long"));
    }
    let mut bytes = vec![0u8; len];
    r.read_exact(&mut bytes)?;
    Ok((head[0], bytes))
}

async fn write_frame_async(w: &mut (impl AsyncWrite + Unpin), kind: u8, bytes: &[u8]) -> io::Result<()> {
    w.write_all(&frame(kind, bytes)).await?;
    w.flush().await
}

async fn read_frame_async(r: &mut (impl AsyncRead + Unpin)) -> io::Result<(u8, Vec<u8>)> {
    let mut head = [0u8; 5];
    r.read_exact(&mut head).await?;
    let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too long"));
    }
    let mut bytes = vec![0u8; len];
    r.read_exact(&mut bytes).await?;
    Ok((head[0], bytes))
}

// ---------------------------------------------------------------------------------------------
// `prism sessions`

/// `prism sessions`: serves the profile's sessions until it has held none for `IDLE`.
pub fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    rt.block_on(serve(Endpoint::profile(), true))
}

/// The attached clients, by id.
#[derive(Default)]
struct Daemon {
    clients: Mutex<HashMap<u64, Box<dyn Attached>>>,
    next: AtomicU64,
}

/// Serves `endpoint`; with `idle`, ends (the process) once it has held no session for `IDLE`.
async fn serve(endpoint: Endpoint, idle: bool) -> Result<()> {
    let daemon = Arc::new(Daemon::default());
    eprintln!("prism sessions {} (protocol {PROTOCOL}), pid {}, on {}", env!("CARGO_PKG_VERSION"), std::process::id(), endpoint.0);
    if idle {
        tokio::spawn(async {
            let mut empty_since = Instant::now();
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if !local::server().is_empty() {
                    empty_since = Instant::now();
                } else if empty_since.elapsed() >= IDLE {
                    eprintln!("prism sessions: no session left, ending");
                    std::process::exit(0);
                }
            }
        });
    }
    accept(daemon, endpoint).await
}

#[cfg(unix)]
async fn accept(daemon: Arc<Daemon>, endpoint: Endpoint) -> Result<()> {
    let path = PathBuf::from(&endpoint.0);
    prism_core::mcp::private_dir(&path)?;
    // Another one answering there: it holds the sessions.
    if std::os::unix::net::UnixStream::connect(&path).is_ok() {
        bail!("prism sessions already runs on {}", path.display());
    }
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
    prism_core::mcp::owner_only(&path)?;
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(daemon.clone().connection(stream));
            }
            Err(e) => {
                eprintln!("prism sessions: {e}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

/// The profile's named pipe, an instance per connection (as `prism mcp`'s): the first one claims
/// the name (another process cannot have made it first, and a second `prism sessions` fails);
/// clients of this computer and this account only (`prism_sys::windows::pipe_server`).
#[cfg(windows)]
async fn accept(daemon: Arc<Daemon>, endpoint: Endpoint) -> Result<()> {
    let name = endpoint.0;
    let make = |first: bool| prism_sys::windows::pipe_server(&name, first);
    let mut server = make(true).with_context(|| format!("prism sessions: no pipe {name}"))?;
    loop {
        if let Err(e) = server.connect().await {
            eprintln!("prism sessions: {e}");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let next = make(false).with_context(|| format!("prism sessions: no pipe {name}"))?;
        let connected = std::mem::replace(&mut server, next);
        tokio::spawn(daemon.clone().connection(connected));
    }
}

impl Daemon {
    async fn connection<S: AsyncRead + AsyncWrite + Send + 'static>(self: Arc<Self>, stream: S) {
        let (mut r, mut w) = tokio::io::split(stream);
        loop {
            let Ok((JSON, bytes)) = read_frame_async(&mut r).await else { return };
            let reply = match serde_json::from_slice::<Request>(&bytes) {
                Err(e) => Reply::Err(format!("prism sessions: a request not understood: {e}")),
                Ok(Request::Attach { name, size, read_only }) => return self.attached(r, w, name, size, read_only).await,
                Ok(Request::Write { client }) => match read_frame_async(&mut r).await {
                    Ok((DATA, data)) => {
                        let me = self.clone();
                        answer(tokio::task::spawn_blocking(move || me.write(client, &data).map(|()| Value::Null)).await)
                    }
                    _ => return,
                },
                Ok(req) => {
                    let me = self.clone();
                    answer(tokio::task::spawn_blocking(move || me.handle(req)).await)
                }
            };
            let Ok(bytes) = serde_json::to_vec(&reply) else { return };
            if write_frame_async(&mut w, JSON, &bytes).await.is_err() {
                return;
            }
        }
    }

    fn handle(&self, req: Request) -> Result<Value> {
        let s = local::server();
        Ok(match req {
            Request::Hello { protocol } => {
                if protocol != PROTOCOL {
                    bail!("prism sessions {} speaks protocol {PROTOCOL}, not {protocol}", env!("CARGO_PKG_VERSION"));
                }
                json!({ "protocol": PROTOCOL, "version": env!("CARGO_PKG_VERSION"), "pid": std::process::id() })
            }
            Request::NewSession { name, cwd, env, argv } => to(s.new_session(&name, &cwd, &env, &argv)?),
            Request::Respawn { name, cwd, env, argv } => to(s.respawn(&name, &cwd, &env, &argv)?),
            Request::Kill { name } => {
                s.kill(&name);
                Value::Null
            }
            Request::Panes => to(s.panes()),
            Request::Pane { name } => to(s.pane(&name)),
            Request::Processes => to(s.processes()),
            Request::Shells => to(s.shells()),
            Request::SetNote { name, key, value } => to(s.set_note(&name, &key, &value)?),
            Request::EndWithProgram { name } => to(s.end_with_program(&name)?),
            Request::SendKeys { name, keys } => to(s.send_keys(&name, &keys.iter().map(String::as_str).collect::<Vec<_>>())?),
            Request::Paste { name, text } => to(s.paste(&name, &text)?),
            Request::History { name, lines, screen } => to(s.history(&name, lines, screen)?),
            Request::Text { name, lines } => to(s.text(&name, lines)?),
            Request::Screen { name } => to(s.screen(&name)?),
            Request::WindowSize { name } => to(s.window_size(&name)?),
            Request::Resize { client, cols, rows } => {
                if let Some(c) = self.clients.lock().unwrap().get(&client) {
                    c.resize(cols, rows);
                }
                Value::Null
            }
            Request::Detach { client } => {
                let gone = self.clients.lock().unwrap().remove(&client);
                drop(gone);
                Value::Null
            }
            Request::Attach { .. } | Request::Write { .. } => bail!("not here"),
        })
    }

    fn write(&self, client: u64, data: &[u8]) -> Result<()> {
        self.clients.lock().unwrap().get(&client).context("no such client")?.write(data)
    }

    /// A client's connection: its id, then what its session shows, until the session ends, the
    /// client detaches (`Detach`) or its connection closes.
    async fn attached<R, W>(self: Arc<Self>, mut r: R, mut w: W, name: String, size: Option<(u16, u16)>, read_only: bool)
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Option<Vec<u8>>>();
        let ended = tx.clone();
        let on_data: OnData = Box::new(move |b| {
            let _ = tx.send(Some(b));
        });
        let on_end: OnEnd = Box::new(move || {
            let _ = ended.send(None);
        });
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let reply = match local::server().attach(&name, size, read_only, on_data, on_end) {
            Ok(client) => {
                self.clients.lock().unwrap().insert(id, Box::new(client));
                Reply::Ok(json!(id))
            }
            Err(e) => Reply::Err(format!("{e:#}")),
        };
        let attached = matches!(reply, Reply::Ok(_));
        let sent = match serde_json::to_vec(&reply) {
            Ok(bytes) => write_frame_async(&mut w, JSON, &bytes).await.is_ok(),
            Err(_) => false,
        };
        if attached && sent {
            loop {
                tokio::select! {
                    msg = rx.recv() => match msg {
                        Some(Some(bytes)) => if write_frame_async(&mut w, DATA, &bytes).await.is_err() { break },
                        _ => break,
                    },
                    // The client says nothing here: anything, or its end, is its going.
                    _ = read_frame_async(&mut r) => break,
                }
            }
        }
        let gone = self.clients.lock().unwrap().remove(&id);
        drop(gone);
        let _ = w.shutdown().await;
    }
}

fn to(v: impl Serialize) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn answer(done: Result<Result<Value>, tokio::task::JoinError>) -> Reply {
    match done {
        Ok(Ok(v)) => Reply::Ok(v),
        Ok(Err(e)) => Reply::Err(format!("{e:#}")),
        Err(e) => Reply::Err(format!("prism sessions: {e}")),
    }
}

// ---------------------------------------------------------------------------------------------
// Prism's end

#[cfg(unix)]
type Stream = std::os::unix::net::UnixStream;
#[cfg(windows)]
type Stream = std::fs::File;

/// Prism's sessions in `prism sessions`, as an `Engine`: started when a session is made and it
/// does not run; without it, there is no session (as without a tmux server).
pub struct Remote {
    inner: Arc<Inner>,
}

struct Inner {
    endpoint: Endpoint,
    /// May start `prism sessions` (not in the tests: this executable is not Prism).
    start: bool,
    /// The connection requests go through, one at a time.
    control: Mutex<Option<Stream>>,
}

impl Remote {
    /// The profile's.
    pub fn profile() -> Self {
        Self::at(Endpoint::profile(), true)
    }

    fn at(endpoint: Endpoint, start: bool) -> Self {
        Self { inner: Arc::new(Inner { endpoint, start, control: Mutex::new(None) }) }
    }
}

impl Inner {
    fn open(&self) -> io::Result<Stream> {
        #[cfg(unix)]
        {
            let path = std::path::Path::new(&self.endpoint.0);
            if path.exists() {
                prism_core::mcp::owned_socket(path).map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, format!("{e:#}")))?;
            }
            Stream::connect(path)
        }
        #[cfg(windows)]
        {
            /// `ERROR_PIPE_BUSY`: all its instances taken an instant.
            const BUSY: i32 = 231;
            let mut tries = 0;
            loop {
                match std::fs::OpenOptions::new().read(true).write(true).open(&self.endpoint.0) {
                    Err(e) if e.raw_os_error() == Some(BUSY) && tries < 50 => tries += 1,
                    // Another account's pipe under our name: never told a session, nor typed in.
                    Ok(pipe) if !prism_sys::windows::pipe_server_is_mine(&pipe) => return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{}: not this account's", self.endpoint.0))),
                    other => return other,
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    /// A connection that said hello; `start`: `prism sessions` started if it does not run.
    fn connect(&self, start: bool) -> Result<Stream> {
        let stream = match self.open() {
            Ok(s) => s,
            Err(_) if start && self.start => {
                spawn()?;
                let deadline = Instant::now() + STARTING;
                loop {
                    match self.open() {
                        Ok(s) => break s,
                        Err(e) if Instant::now() > deadline => return Err(e).context("prism sessions did not start"),
                        Err(_) => std::thread::sleep(Duration::from_millis(50)),
                    }
                }
            }
            // As tmux says it: no session then.
            Err(e) => return Err(anyhow!("no server running ({e})")),
        };
        let mut stream = stream;
        exchange(&mut stream, &Request::Hello { protocol: PROTOCOL }, None).context("prism sessions")??;
        Ok(stream)
    }

    fn call(&self, req: &Request, body: Option<&[u8]>, start: bool) -> Result<Value> {
        let mut control = self.control.lock().unwrap();
        let mut last = None;
        // Its connection may have closed (it ended, idle, and started again): once more anew.
        for _ in 0..2 {
            if control.is_none() {
                *control = Some(self.connect(start)?);
            }
            match exchange(control.as_mut().unwrap(), req, body) {
                Ok(reply) => return reply,
                Err(e) => {
                    *control = None;
                    last = Some(e);
                }
            }
        }
        Err(last.map_or_else(|| anyhow!("prism sessions"), |e| anyhow!("prism sessions: {e}")))
    }

    fn get<T: DeserializeOwned>(&self, req: Request) -> Result<T> {
        Ok(serde_json::from_value(self.call(&req, None, false)?)?)
    }
}

/// A request and its answer: the outer error is the connection's, the inner one the request's.
fn exchange(stream: &mut Stream, req: &Request, body: Option<&[u8]>) -> io::Result<Result<Value>> {
    let bytes = serde_json::to_vec(req).map_err(io::Error::other)?;
    write_frame(stream, JSON, &bytes)?;
    if let Some(body) = body {
        write_frame(stream, DATA, body)?;
    }
    let (kind, bytes) = read_frame(stream)?;
    if kind != JSON {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not an answer"));
    }
    Ok(match serde_json::from_slice::<Reply>(&bytes).map_err(io::Error::other)? {
        Reply::Ok(v) => Ok(v),
        Reply::Err(e) => Err(anyhow!(e)),
    })
}

/// `prism sessions` started for the profile, on its own: out of Prism's process group (job on
/// Windows), without a window, what it says in `sessions.log` of the profile's data folder.
fn spawn() -> Result<()> {
    let exe = std::env::current_exe().context("Prism's executable")?;
    #[cfg(windows)]
    let exe = own_copy(&exe).unwrap_or_else(|e| {
        tracing::warn!("agents: prism sessions from prism.exe itself: {e:#}");
        exe
    });
    let log = prism_core::profile::data_dir(&prism_core::profile::data_base()).join("sessions.log");
    let stderr = std::fs::File::create(&log).map(std::process::Stdio::from).unwrap_or_else(|_| std::process::Stdio::null());
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(["--profile", prism_core::profile::active(), "sessions"]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(stderr);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    let child = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        // Out of Prism's job if it is in one that lets go (closing it would end the sessions);
        // as it is, if it does not.
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
        match cmd.spawn() {
            Ok(c) => c,
            Err(_) => {
                cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
                cmd.spawn().with_context(|| format!("starting {} sessions", exe.display()))?
            }
        }
    };
    #[cfg(unix)]
    let child = cmd.spawn().with_context(|| format!("starting {} sessions", exe.display()))?;
    // Waited for, not to leave a zombie behind when it ends while Prism runs.
    let mut child = child;
    std::thread::Builder::new().name("prism sessions".into()).spawn(move || {
        let _ = child.wait();
    })?;
    tracing::info!("agents: started prism sessions ({})", log.display());
    Ok(())
}

/// On Windows, a copy of `prism.exe` for `prism sessions` to run from, in the local data folder:
/// an update's installer replaces `prism.exe` and closes whatever runs from it (the Restart
/// Manager), the agents with it otherwise. One per build (its size and time); those of former
/// builds are removed once nothing runs from them.
#[cfg(windows)]
fn own_copy(exe: &std::path::Path) -> Result<PathBuf> {
    let meta = std::fs::metadata(exe)?;
    let stamp = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let dir = dirs::data_local_dir().context("no local data folder")?.join("app.prism.desktop").join("sessions");
    std::fs::create_dir_all(&dir)?;
    let name = format!("prism-sessions-{}-{:x}-{stamp:x}.exe", env!("CARGO_PKG_VERSION"), meta.len());
    let copy = dir.join(&name);
    if !copy.exists() {
        let part = dir.join(format!("{name}.part"));
        std::fs::copy(exe, &part).with_context(|| format!("copying {} to {}", exe.display(), part.display()))?;
        std::fs::rename(&part, &copy)?;
    }
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path != copy && path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("prism-sessions-")) {
            // Still running for a former build's sessions: kept until the next time.
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(copy)
}

impl Engine for Remote {
    fn new_session(&self, name: &str, cwd: &std::path::Path, env: &[String], argv: &[String]) -> Result<()> {
        let req = Request::NewSession { name: name.into(), cwd: cwd.into(), env: env.into(), argv: argv.into() };
        self.inner.call(&req, None, true).map(drop)
    }
    fn respawn(&self, name: &str, cwd: &std::path::Path, env: &[String], argv: &[String]) -> Result<()> {
        let req = Request::Respawn { name: name.into(), cwd: cwd.into(), env: env.into(), argv: argv.into() };
        self.inner.call(&req, None, false).map(drop)
    }
    fn kill(&self, name: &str) {
        let _ = self.inner.call(&Request::Kill { name: name.into() }, None, false);
    }
    fn panes(&self) -> Vec<(String, Pane)> {
        self.inner.get(Request::Panes).unwrap_or_default()
    }
    fn pane(&self, name: &str) -> Option<(Pane, String)> {
        self.inner.get(Request::Pane { name: name.into() }).ok().flatten()
    }
    fn processes(&self) -> Vec<(String, Option<u32>, String, bool)> {
        self.inner.get(Request::Processes).unwrap_or_default()
    }
    fn shells(&self) -> Vec<ShellSession> {
        self.inner.get(Request::Shells).unwrap_or_default()
    }
    fn set_note(&self, name: &str, key: &str, value: &str) -> Result<()> {
        self.inner.call(&Request::SetNote { name: name.into(), key: key.into(), value: value.into() }, None, false).map(drop)
    }
    fn end_with_program(&self, name: &str) -> Result<()> {
        self.inner.call(&Request::EndWithProgram { name: name.into() }, None, false).map(drop)
    }
    fn send_keys(&self, name: &str, keys: &[&str]) -> Result<()> {
        self.inner.call(&Request::SendKeys { name: name.into(), keys: keys.iter().map(|k| k.to_string()).collect() }, None, false).map(drop)
    }
    fn paste(&self, name: &str, text: &str) -> Result<()> {
        self.inner.call(&Request::Paste { name: name.into(), text: text.into() }, None, false).map(drop)
    }
    fn history(&self, name: &str, lines: u32, screen: bool) -> Result<String> {
        self.inner.get(Request::History { name: name.into(), lines, screen })
    }
    fn text(&self, name: &str, lines: u32) -> Result<String> {
        self.inner.get(Request::Text { name: name.into(), lines })
    }
    fn screen(&self, name: &str) -> Result<String> {
        self.inner.get(Request::Screen { name: name.into() })
    }
    fn window_size(&self, name: &str) -> Result<(u16, u16)> {
        self.inner.get(Request::WindowSize { name: name.into() })
    }

    /// A connection of its own, read by a thread: what the session shows to `on_data`, its end to
    /// `on_end`.
    fn attach(&self, name: &str, size: Option<(u16, u16)>, read_only: bool, on_data: OnData, on_end: OnEnd) -> Result<Box<dyn Attached>> {
        let mut stream = self.inner.connect(false)?;
        let id: u64 = serde_json::from_value(exchange(&mut stream, &Request::Attach { name: name.into(), size, read_only }, None)??)?;
        std::thread::Builder::new().name(format!("session {name}")).spawn(move || {
            while let Ok((DATA, bytes)) = read_frame(&mut stream) {
                on_data(bytes);
            }
            on_end();
        })?;
        Ok(Box::new(RemoteClient { inner: self.inner.clone(), id, read_only }))
    }
}

/// A terminal attached to a session of `prism sessions`.
struct RemoteClient {
    inner: Arc<Inner>,
    id: u64,
    read_only: bool,
}

impl Attached for RemoteClient {
    fn write(&self, data: &[u8]) -> Result<()> {
        if self.read_only {
            bail!("client is read-only");
        }
        self.inner.call(&Request::Write { client: self.id }, Some(data), false).map(drop)
    }

    fn resize(&self, cols: u16, rows: u16) {
        let _ = self.inner.call(&Request::Resize { client: self.id, cols, rows }, None, false);
    }
}

impl Drop for RemoteClient {
    fn drop(&mut self) {
        let _ = self.inner.call(&Request::Detach { client: self.id }, None, false);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn wait(what: &str, mut ok: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ok() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A folder of its own for a test's socket, made by `private_dir` (0700); short, as a socket's
    /// path must be.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("prism-sd-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// `prism sessions` on a socket of its own, in a thread; a `Remote` that does not start it.
    fn daemon(dir: &std::path::Path) -> Remote {
        let endpoint = Endpoint(dir.join("sessions.sock").display().to_string());
        let served = endpoint.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
            rt.block_on(serve(served, false)).unwrap();
        });
        let remote = Remote::at(endpoint, false);
        wait("prism sessions listening", || remote.inner.open().is_ok());
        remote
    }

    /// Without `prism sessions`: no session, and nothing started.
    #[test]
    fn without_it_there_is_no_session() {
        let dir = scratch("none");
        let remote = Remote::at(Endpoint(dir.join("sessions.sock").display().to_string()), false);
        assert!(remote.panes().is_empty() && remote.shells().is_empty() && remote.pane("prism-x").is_none());
        let e = remote.new_session("prism-x", &std::env::temp_dir(), &[], &["true".into()]).unwrap_err();
        assert!(super::super::is_no_server(&e), "{e:#}");
    }

    /// A session's whole path through the pipe: made, attached, typed into, its output read,
    /// resized, its program's exit seen, killed; a client detached when dropped.
    #[test]
    fn a_session_through_prism_sessions() {
        let dir = scratch("path");
        let remote = daemon(&dir);
        let argv: Vec<String> = vec!["/bin/sh".into(), "-c".into(), "echo ready; read line; echo got $line; exit 4".into()];
        remote.new_session("prism-daemon-1", &std::env::temp_dir(), &[], &argv).unwrap();
        assert!(remote.new_session("prism-daemon-1", &std::env::temp_dir(), &[], &argv).is_err(), "a duplicate refused");
        wait("ready", || remote.screen("prism-daemon-1").is_ok_and(|s| s.contains("ready")));
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let (end_tx, end_rx) = mpsc::channel::<()>();
        let client = remote.attach("prism-daemon-1", Some((90, 25)), false, Box::new(move |b| { let _ = tx.send(b); }), Box::new(move || { let _ = end_tx.send(()); })).unwrap();
        let first = String::from_utf8_lossy(&rx.recv_timeout(Duration::from_secs(5)).unwrap()).into_owned();
        assert!(first.contains("ready"), "the screen as it stands: {first}");
        assert_eq!(remote.window_size("prism-daemon-1").unwrap(), (90, 25));
        client.resize(100, 30);
        wait("resized", || remote.window_size("prism-daemon-1").ok() == Some((100, 30)));
        client.write(b"there\r").unwrap();
        let mut seen = String::new();
        wait("its answer streamed", || {
            while let Ok(b) = rx.try_recv() {
                seen.push_str(&String::from_utf8_lossy(&b));
            }
            seen.contains("got there")
        });
        wait("its exit", || remote.pane("prism-daemon-1").map(|p| p.0) == Some(Pane::Exited(4)));
        assert!(remote.history("prism-daemon-1", 50, true).unwrap().contains("got there"));
        assert_eq!(local::agent_panes(remote.panes()).get("daemon-1"), Some(&Pane::Exited(4)));
        drop(client);
        end_rx.recv_timeout(Duration::from_secs(5)).expect("detached");
        remote.kill("prism-daemon-1");
        assert!(remote.pane("prism-daemon-1").is_none());
    }

    /// A shell's session ending with its program ends its clients' connections.
    #[test]
    fn a_client_is_told_its_session_ended() {
        let dir = scratch("ended");
        let remote = daemon(&dir);
        let argv: Vec<String> = vec!["/bin/sh".into(), "-c".into(), "read line".into()];
        remote.new_session("prismsh-daemon-1", &std::env::temp_dir(), &[], &argv).unwrap();
        remote.end_with_program("prismsh-daemon-1").unwrap();
        let (end_tx, end_rx) = mpsc::channel::<()>();
        let _client = remote.attach("prismsh-daemon-1", None, false, Box::new(|_| {}), Box::new(move || { let _ = end_tx.send(()); })).unwrap();
        remote.send_keys("prismsh-daemon-1", &["Enter"]).unwrap();
        end_rx.recv_timeout(Duration::from_secs(10)).expect("told the session ended");
        assert!(remote.shells().iter().all(|s| s.owner != "daemon"));
    }

    /// A read-only client (a phone watching) cannot type.
    #[test]
    fn a_watching_client_reads_only() {
        let dir = scratch("ro");
        let remote = daemon(&dir);
        remote.new_session("prism-daemon-ro", &std::env::temp_dir(), &[], &["/bin/sh".into(), "-c".into(), "read line".into()]).unwrap();
        let client = remote.attach("prism-daemon-ro", None, true, Box::new(|_| {}), Box::new(|| {})).unwrap();
        assert!(client.write(b"x").is_err());
        remote.kill("prism-daemon-ro");
    }
}
