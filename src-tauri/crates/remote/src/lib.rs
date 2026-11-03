//! Prism from a phone (`[remote]`): a server on loopback, behind a tunnel (`tailscale serve`, or
//! one of the user's), serving the remote's page and talking to it over a WebSocket. A phone is
//! paired from the desk and unlocks with a passkey (`auth`); it gets the snapshot filtered
//! (`snapshot`) and may call the window's commands its class allows (`prism_core::remote`): for
//! now, the ones that read.
//!
//! The app gives what only it has through `Host`: its commands, their classes, the page's files.

pub mod auth;
pub mod away;
mod awake;
mod download;
pub mod machine;
mod pages;
pub mod peers;
pub use pages::page_csp;
pub mod pair;
mod preview;
pub mod push;
mod relay;
mod server;
pub mod snapshot;
mod tailscale;
pub mod webauthn;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use anyhow::{bail, Context, Result};
use prism_base::{i18n::tr, tr};
use chrono::Utc;
use futures::future::BoxFuture;
use prism_core::{
    config::{RemoteConfig, RemoteTunnel},
    remote::Class,
    state::{replace, Hub, RemoteDevice, RemotePairing, RemoteStatus},
};
use serde_json::Value;

/// The version of the conversation between the page and the server: a page of another version
/// is told to reload.
/// 2: an array of the same length changes item by item (an index in the path).
/// 3: an agent's files download as an address to fetch (`download`), no longer as base64.
pub const PROTOCOL: u32 = 3;

/// What the app gives the remote.
pub trait Host: Send + Sync + 'static {
    /// A command of the window called from a phone, answered as the window's would be.
    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>>;
    /// Its class; `None` for a command this Prism does not have. A block's command
    /// (`block_call`) has its block's class.
    fn class(&self, command: &str, args: &Value) -> Option<Class>;
    /// A file of the page (`remote.html`, its scripts and styles, its icons): its bytes and type.
    fn asset(&self, path: &str) -> Option<(Vec<u8>, String)>;
    /// An agent's terminal for a phone: watched read-only, or controlled (`control`), at the
    /// phone's `size` (without one, an older page: watched at the window's size), its output
    /// through `on_data` until `on_end`. Dropping the `Watch` ends it.
    fn watch(&self, agent: String, size: Option<(u16, u16)>, control: bool, on_data: Box<dyn Fn(Vec<u8>) + Send>, on_end: Box<dyn FnOnce() + Send>) -> BoxFuture<'static, Result<Watch>>;
    /// An agent's file for its pages (`/<agent>/<path>`, `/<agent>/@scratchpad/<path>`,
    /// percent-encoded), as `prismfile:` serves it: its bytes and type, or the error status.
    fn page(&self, path: &str) -> std::result::Result<(Vec<u8>, String), (u16, String)> {
        Err((404, format!("{path}: no pages here")))
    }
    /// A command of the window called by another Prism's window (`peers`): any but the window's
    /// own, answered as the window's would be.
    fn peer_call(&self, command: String, _args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move { bail!(tr!("{command} is not answered from another Prism", command = command)) })
    }
    /// This machine's profiles, for a phone's switcher (`relay`): none, a single Prism.
    fn profiles(&self) -> BoxFuture<'static, Vec<prism_core::profile::Profile>> {
        Box::pin(async { Vec::new() })
    }
    /// Opens profile `id` in its own Prism, beside this one (a phone's Open).
    fn open_profile(&self, id: &str) -> Result<()> {
        bail!(tr!("unknown profile {id}", id = id))
    }
}

/// What a phone's keys and size reach: nothing, for a terminal watched read-only.
pub trait Term: Send {
    fn write(&mut self, data: &[u8]) -> Result<()>;
    fn resize(&mut self, cols: u16, rows: u16) -> Result<()>;
}

/// A terminal for a phone: its size, and the terminal itself, attached while it lives.
pub struct Watch {
    pub cols: u16,
    pub rows: u16,
    pub term: Box<dyn Term>,
}

/// A terminal watched only: keys and sizes go nowhere.
pub struct ReadOnly<T: Send>(pub T);

impl<T: Send> Term for ReadOnly<T> {
    fn write(&mut self, _: &[u8]) -> Result<()> {
        bail!(tr("this terminal is read-only"))
    }
    fn resize(&mut self, _: u16, _: u16) -> Result<()> {
        Ok(())
    }
}

/// The remote, on or off.
#[derive(Clone)]
pub struct Remote {
    inner: Arc<Inner>,
}

struct Inner {
    hub: Hub,
    /// This Prism's data folder and profile: where the others' remotes are found (`relay`).
    dir: std::path::PathBuf,
    profile: String,
    /// The secret of this Prism's `remote/endpoint.json`, while its server runs.
    secret: Mutex<Option<String>>,
    /// The last agent that started waiting, here or in a relayed profile: what a woken phone says.
    last_wake: Mutex<Option<relay::Wake>>,
    /// When each phone was last woken.
    pushed: Mutex<relay::Pushed>,
    /// The dev server a relayed profile opened to the preview: its profile and port.
    relayed_preview: Mutex<Option<(String, u16)>>,
    /// The relayed profiles' summaries, read last (`relay::summary_cached`).
    summaries: Mutex<HashMap<String, (std::time::Instant, Option<relay::Summary>)>>,
    /// The key pushes are signed with.
    vapid: push::Vapid,
    host: Arc<dyn Host>,
    auth: Mutex<auth::Auth>,
    /// What the server reads at each request, from the config applied last.
    live: Mutex<Live>,
    running: tokio::sync::Mutex<Option<Running>>,
    /// Pages connected now, by phone.
    connected: Mutex<HashMap<String, usize>>,
    /// Terminals phones control now, each with what makes its socket give it back.
    controls: Mutex<Vec<Control>>,
    /// Where the remote answers, or why it does not.
    url: Mutex<Option<String>>,
    error: Mutex<Option<String>>,
    /// The dev server the preview forwards to: the port a phone chose last.
    preview: Mutex<Option<u16>>,
    /// Files prepared for a phone, by the token of their address (`download`).
    downloads: Mutex<HashMap<String, download::Pending>>,
    /// Agents' pages opened to a phone, by the token of their address (`pages`).
    pages: Mutex<HashMap<String, pages::Open>>,
    /// A computer's pairing under way (`peers`).
    peer_pairing: Mutex<Option<peers::Pairing>>,
}

/// Each lock taken even after a panic while it was held: the state it guards stays usable, where
/// a poisoned lock would make every later request panic until the app restarts.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Inner {
    /// Where this Prism's addresses start for a phone: relayed, its path at the hub (`/p/<profile>`).
    fn base(&self) -> String {
        if self.live().relay { format!("/p/{}", self.profile) } else { String::new() }
    }
    fn auth(&self) -> MutexGuard<'_, auth::Auth> {
        lock(&self.auth)
    }
    fn live(&self) -> MutexGuard<'_, Live> {
        lock(&self.live)
    }
    fn connected(&self) -> MutexGuard<'_, HashMap<String, usize>> {
        lock(&self.connected)
    }
    fn controls(&self) -> MutexGuard<'_, Vec<Control>> {
        lock(&self.controls)
    }
    fn url(&self) -> MutexGuard<'_, Option<String>> {
        lock(&self.url)
    }
    fn error(&self) -> MutexGuard<'_, Option<String>> {
        lock(&self.error)
    }
    fn preview(&self) -> MutexGuard<'_, Option<u16>> {
        lock(&self.preview)
    }
    fn peer_pairing(&self) -> MutexGuard<'_, Option<peers::Pairing>> {
        lock(&self.peer_pairing)
    }
}

#[derive(Clone, Default)]
struct Live {
    party: Option<webauthn::Party>,
    /// Tunnel `relay`: reached through the hub only, which checks the phones (`relay`).
    relay: bool,
    /// Tunnel `relay`: the address phones open the hub at, as it last said (`relay::ORIGIN`).
    hub_origin: Option<String>,
    /// Tunnel tailscale: the tailnet user let in.
    owner: Option<String>,
    port: u16,
    mail: bool,
    /// `[remote] actions`: the class `act` goes through.
    actions: bool,
    /// `[remote] critical`, `typing`: the class `critical`, and the terminals' control, each
    /// confirmed by the passkey.
    critical: bool,
    typing: bool,
    /// `[remote] power`: restarting or shutting down the Mac, each time confirmed by the passkey.
    power: bool,
    /// `[remote] settings`: the settings read, and the few of `PHONE_SETTINGS` changed, each save
    /// confirmed by the passkey.
    settings: bool,
    /// `[remote] projects`: a new project in the working folder, each confirmed by the passkey.
    projects: bool,
    /// `[remote] push`.
    push: bool,
    /// `[remote] control_idle_minutes`: 0, never.
    control_idle: Option<std::time::Duration>,
    tabs: Vec<String>,
    /// The dev servers' preview, when it answers: its address (`https://mac.tail1.ts.net:8443`)
    /// and its loopback port.
    preview_origin: Option<String>,
    preview_port: u16,
    /// `[remote.peers]`: what another Prism's window may do.
    peers: prism_core::config::PeerRights,
}

/// A terminal a phone controls.
struct Control {
    agent: String,
    device_name: String,
    since: chrono::DateTime<Utc>,
    /// Sent when the desk takes it back.
    release: tokio::sync::oneshot::Sender<()>,
}

/// What the server was started with: a change restarts it.
#[derive(PartialEq)]
struct Key {
    tunnel: RemoteTunnel,
    port: u16,
    url: String,
    socket: String,
    preview: Option<(u16, String)>,
}

/// The preview's server as started.
struct Preview {
    stop: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
    port: u16,
}

/// The server as started: what makes it the same, and what stops it.
struct Running {
    key: Key,
    cli: tailscale::Cli,
    stop: tokio::sync::oneshot::Sender<()>,
    /// The pushes' watcher, ended with the server.
    wake: tokio::task::JoinHandle<()>,
    task: tokio::task::JoinHandle<()>,
    node: Option<tailscale::Node>,
    preview: Option<Preview>,
    _awake: Option<awake::Awake>,
}

impl Remote {
    /// The remote of the active profile, whose data folder is `dir` (its paired phones in
    /// `remote/`); off until `apply` turns it on.
    pub fn new(dir: &std::path::Path, hub: Hub, host: Arc<dyn Host>) -> Result<Self> {
        Self::of_profile(dir, prism_core::profile::active(), hub, host)
    }

    /// The remote of profile `profile`, whose data folder is `dir`.
    pub fn of_profile(dir: &std::path::Path, profile: &str, hub: Hub, host: Arc<dyn Host>) -> Result<Self> {
        let auth = auth::Auth::load(&dir.join("remote"))?;
        let vapid = push::Vapid::load(&dir.join("remote"))?;
        let inner = Inner {
            hub,
            dir: dir.to_path_buf(),
            profile: profile.to_string(),
            secret: Default::default(),
            last_wake: Default::default(),
            pushed: Default::default(),
            relayed_preview: Default::default(),
            summaries: Default::default(),
            vapid,
            host,
            auth: Mutex::new(auth),
            live: Mutex::new(Live::default()),
            running: Default::default(),
            connected: Default::default(),
            controls: Default::default(),
            url: Default::default(),
            error: Default::default(),
            preview: Default::default(),
            downloads: Default::default(),
            pages: Default::default(),
            peer_pairing: Default::default(),
        };
        Ok(Self { inner: Arc::new(inner) })
    }

    /// Applies `[remote]`: starts, restarts or stops the server; durations, tabs and mail apply
    /// to the open sessions at once.
    pub async fn apply(&self, cfg: &RemoteConfig) {
        self.apply_with(cfg, false).await
    }

    /// Stops the server and starts it again, its config unchanged: the tunnel read anew (Tailscale
    /// off at launch, or its `serve` lost since). The phones' sessions end, their pairing stays.
    pub async fn restart(&self, cfg: &RemoteConfig) {
        self.apply_with(cfg, true).await
    }

    async fn apply_with(&self, cfg: &RemoteConfig, restart: bool) {
        let inner = &self.inner;
        {
            let mut auth = inner.auth();
            auth.idle = chrono::Duration::minutes(cfg.session_idle_minutes.max(1) as i64);
            auth.max = chrono::Duration::hours(cfg.session_max_hours.max(1) as i64);
            auth.step_up = chrono::Duration::minutes(cfg.step_up_minutes as i64);
        }
        {
            let mut live = inner.live();
            live.mail = cfg.mail;
            live.actions = cfg.actions;
            live.critical = cfg.critical;
            live.typing = cfg.typing;
            live.power = cfg.power;
            live.settings = cfg.settings;
            live.projects = cfg.projects;
            live.push = cfg.push;
            live.peers = cfg.peers.clone();
            live.control_idle = (cfg.control_idle_minutes > 0).then(|| std::time::Duration::from_secs(60 * cfg.control_idle_minutes as u64));
            live.tabs = cfg.tabs().into_iter().map(String::from).collect();
        }
        let mut running = inner.running.lock().await;
        let key = Key {
            tunnel: cfg.tunnel,
            port: cfg.port,
            url: cfg.url.trim().to_string(),
            socket: cfg.tailscale_socket.trim().to_string(),
            preview: cfg.preview.then(|| (cfg.preview_port, cfg.preview_url.trim().to_string())),
        };
        if !restart && cfg.enabled && running.as_ref().is_some_and(|r| r.key == key) {
            self.publish();
            return;
        }
        if let Some(r) = running.take() {
            Self::shut(inner, r).await;
        }
        *inner.url() = None;
        *inner.error() = None;
        if cfg.enabled {
            match self.start(cfg, key).await {
                Ok(r) => *running = Some(r),
                Err(e) => {
                    tracing::warn!("remote: {e:#}");
                    *inner.error() = Some(format!("{e:#}"));
                }
            }
        } else {
            inner.auth().end_sessions();
        }
        drop(running);
        self.publish();
    }

    async fn start(&self, cfg: &RemoteConfig, key: Key) -> Result<Running> {
        let inner = &self.inner;
        let relayed = cfg.tunnel == RemoteTunnel::Relay;
        // Relayed: a free port, told to the hub only (`relay`): two profiles never collide.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", if relayed { 0 } else { cfg.port })).await.with_context(|| tr!("port {port} on 127.0.0.1 is taken ([remote] port)", port = cfg.port))?;
        let port = listener.local_addr()?.port();
        let home = |p: &str| match (p.strip_prefix("~/"), std::env::var("HOME")) {
            (Some(rest), Ok(h)) => format!("{h}/{rest}"),
            _ => p.to_string(),
        };
        let cli = tailscale::Cli { socket: Some(home(&key.socket)).filter(|s| !s.is_empty()) };
        let (url, node) = match cfg.tunnel {
            RemoteTunnel::Relay => (String::new(), None),
            RemoteTunnel::Tailscale => {
                let node = tailscale::node(&cli).await?;
                tailscale::serve(&cli, &node, cfg.port).await?;
                (node.url.clone(), Some(node))
            }
            RemoteTunnel::Other => {
                let url = cfg.url.trim().trim_end_matches('/').to_string();
                if url.is_empty() {
                    bail!(tr("tunnel other: [remote] url names the HTTPS address the phone opens"));
                }
                (url, None)
            }
        };
        let party = if relayed { None } else { Some(webauthn::Party::of(&url)?) };
        {
            let mut live = inner.live();
            live.party = party;
            live.relay = relayed;
            live.owner = node.as_ref().map(|n| n.owner.clone());
            live.port = port;
        }
        let secret = download::token();
        relay::write(inner, &relay::Endpoint { port, secret: secret.clone(), hub: !relayed, url: (!relayed).then(|| url.clone()) })?;
        *lock(&inner.secret) = Some(secret);
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let app = server::router(inner.clone(), relayed);
        let task = prism_base::rt::spawn(async move {
            let serve = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = stopped.await;
            });
            if let Err(e) = serve.await {
                tracing::warn!("remote: server: {e}");
            }
        });
        // Agents that start waiting wake the phones that have Prism closed.
        let wake = prism_base::rt::spawn(wake_phones(inner.clone()));
        if relayed {
            tracing::info!("remote: on, relayed by the hub's profile (loopback :{port})");
        } else {
            tracing::info!("remote: on at {url}");
        }
        // The preview fails alone: the remote stays on without it.
        // Another tunnel without `preview_url`: no preview, as documented. Relayed: the hub's.
        let preview = match &key.preview {
            _ if relayed => None,
            Some((_, preview_url)) if node.is_none() && preview_url.is_empty() => None,
            Some((port, preview_url)) => match self.start_preview(&cli, node.as_ref(), *port, preview_url).await {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!("remote: preview: {e:#}");
                    None
                }
            },
            None => None,
        };
        *inner.url() = (!relayed).then_some(url);
        Ok(Running { key, cli, stop, wake, task, node, preview, _awake: cfg.keep_awake.then(awake::Awake::hold) })
    }

    /// The dev servers' preview: its loopback port, and its address on the tunnel (Tailscale: the
    /// Mac's name on port 8443; another tunnel: `[remote] preview_url`, on the remote's host).
    async fn start_preview(&self, cli: &tailscale::Cli, node: Option<&tailscale::Node>, port: u16, preview_url: &str) -> Result<Preview> {
        let inner = &self.inner;
        let origin = match node {
            Some(_) => None,
            None => Some(preview_url.trim_end_matches('/').to_string()),
        };
        let party = inner.live().party.clone().context("starting")?;
        if let Some(o) = &origin {
            let host = url::Url::parse(o).ok().and_then(|u| u.host_str().map(String::from));
            if host.as_deref() != Some(party.id.as_str()) {
                bail!(tr!("[remote] preview_url must be on {host}, the remote's host (another port): its session cookie goes there", host = party.id));
            }
        }
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.with_context(|| tr!("port {port} on 127.0.0.1 is taken ([remote] preview_port)", port = port))?;
        let origin = match (node, origin) {
            (Some(node), _) => {
                tailscale::serve_on(cli, node, preview::TAILNET_PORT, port).await?;
                format!("{}:{}", node.url, preview::TAILNET_PORT)
            }
            (None, Some(o)) => o,
            (None, None) => unreachable!(),
        };
        {
            let mut live = inner.live();
            live.preview_origin = Some(origin.clone());
            live.preview_port = port;
        }
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let app = preview::router(inner.clone());
        let task = prism_base::rt::spawn(async move {
            let serve = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = stopped.await;
            });
            if let Err(e) = serve.await {
                tracing::warn!("remote: preview: {e}");
            }
        });
        tracing::info!("remote: dev servers' preview at {origin}");
        Ok(Preview { stop, task, port })
    }

    async fn shut(inner: &Arc<Inner>, mut r: Running) {
        relay::remove(inner);
        *lock(&inner.secret) = None;
        let _ = r.stop.send(());
        r.wake.abort();
        {
            let mut live = inner.live();
            live.preview_origin = None;
        }
        *inner.preview() = None;
        if let Some(mut p) = r.preview.take() {
            let _ = p.stop.send(());
            // A page's hot reload socket would hold a graceful stop.
            if tokio::time::timeout(std::time::Duration::from_secs(3), &mut p.task).await.is_err() {
                p.task.abort();
                let _ = (&mut p.task).await;
            }
            if let Some(node) = &r.node {
                tailscale::unserve_on(&r.cli, node, preview::TAILNET_PORT, p.port).await;
            }
        }
        // The open sockets end with their sessions.
        inner.auth().end_sessions();
        // A phone's socket still open would hold a graceful stop: the port is freed anyway.
        if tokio::time::timeout(std::time::Duration::from_secs(3), &mut r.task).await.is_err() {
            r.task.abort();
            let _ = (&mut r.task).await;
        }
        if let Some(node) = &r.node {
            tailscale::unserve(&r.cli, node, r.key.port).await;
        }
        tracing::info!("remote: off");
    }

    /// Settings › Remote › Devices: opens a pairing, shown as a QR code.
    pub fn pair_start(&self) -> Result<()> {
        let inner = &self.inner;
        if inner.live().relay {
            bail!(tr("this profile is reached through another one's remote: pair the phone there"));
        }
        if inner.url().is_none() {
            bail!(tr("the remote is off or does not answer"));
        }
        let (_, expires) = inner.auth().pair_start(Utc::now());
        self.publish();
        // Its end shows without anything else happening. On the app's runtime: the desk's
        // commands run on the window's thread, outside it.
        let me = self.clone();
        let wait = (expires - Utc::now()).to_std().unwrap_or_default() + std::time::Duration::from_secs(1);
        prism_base::rt::spawn(async move {
            tokio::time::sleep(wait).await;
            me.publish();
        });
        Ok(())
    }

    /// The desk confirms the code the phone shows; a terminal, the `code` it showed.
    pub fn pair_confirm(&self, code: Option<&str>) -> Result<()> {
        let r = self.inner.auth().pair_confirm(Utc::now(), code);
        self.publish();
        r
    }

    pub fn pair_cancel(&self) {
        self.inner.auth().pair_cancel();
        self.publish();
    }

    /// Pair a computer (`prism peer pair`, Settings › Remote › Devices): a code another Prism's
    /// window is given, for five minutes; any other pairing of a computer dropped.
    pub fn peer_pair_start(&self) -> Result<String> {
        let inner = &self.inner;
        if inner.live().relay {
            bail!(tr("this profile is reached through another one's remote: pair the computer there"));
        }
        if inner.url().is_none() {
            bail!(tr("the remote is off or does not answer"));
        }
        let pairing = peers::Pairing::new(Utc::now());
        let (code, expires) = (pairing.code.clone(), pairing.expires);
        *inner.peer_pairing() = Some(pairing);
        self.publish();
        let me = self.clone();
        let wait = (expires - Utc::now()).to_std().unwrap_or_default() + std::time::Duration::from_secs(1);
        prism_base::rt::spawn(async move {
            tokio::time::sleep(wait).await;
            me.publish();
        });
        Ok(code)
    }

    /// The computer that gave the code is accepted (its token made, for it to read once), or refused.
    pub fn peer_pair_answer(&self, accept: bool) -> Result<()> {
        let r = match self.inner.peer_pairing().as_mut() {
            Some(p) => p.answer(&self.inner.dir, accept),
            None => Err(anyhow::anyhow!(tr("no computer to accept"))),
        };
        if r.is_ok() {
            tracing::info!("remote: a computer's pairing {}", if accept { "accepted" } else { "refused" });
        }
        self.publish();
        r
    }

    pub fn peer_pair_cancel(&self) {
        *self.inner.peer_pairing() = None;
        self.publish();
    }

    /// Forgets another Prism: its token opens nothing anymore, its windows close.
    pub fn peer_revoke(&self, name: &str) -> Result<()> {
        let removed = peers::remove(&self.inner.dir, name)?;
        self.publish();
        anyhow::ensure!(removed, "no peer {name}");
        Ok(())
    }

    /// The desk takes an agent's terminal back (Take back, or a key typed on the desk): the
    /// phones controlling it go back to watching.
    pub fn release(&self, agent: &str) {
        let released: Vec<Control> = {
            let mut c = self.inner.controls();
            if !c.iter().any(|x| x.agent == agent) {
                return;
            }
            let (out, keep) = c.drain(..).partition(|x| x.agent == agent);
            *c = keep;
            out
        };
        for c in released {
            let _ = c.release.send(());
        }
        self.publish();
    }

    /// Something to tell the phones as a notification (a chain's "Notify me" step): those without
    /// the page open are woken, as when an agent starts waiting (`[remote] push`); relayed, through
    /// the hub.
    pub async fn notify(&self, text: &str) {
        let inner = &self.inner;
        let wake = relay::Wake { profile: inner.profile.clone(), agent: String::new(), title: String::new(), text: Some(text.to_string()), at: Utc::now() };
        if inner.live().relay {
            relay::tell_hub(inner, &wake).await;
        } else {
            inner.wake(wake).await;
        }
    }

    /// Forgets a phone: its passkey opens nothing anymore.
    pub fn revoke(&self, device: &str) -> Result<()> {
        let r = self.inner.auth().revoke(device);
        self.publish();
        r
    }

    pub fn rename(&self, device: &str, name: &str) -> Result<()> {
        let r = self.inner.auth().rename(device, name);
        self.publish();
        r
    }

    fn publish(&self) {
        self.inner.publish();
    }
}

/// When an agent starts waiting (a permission, a question), an empty push to each paired phone
/// subscribed (`[remote] push`) that has no page open: at most one a minute per phone. Relayed,
/// the hub is told, which wakes them (`relay`).
async fn wake_phones(inner: Arc<Inner>) {
    use prism_core::state::SessionStatus::{NeedsYou, WaitingTool};
    let mut rx = inner.hub.subscribe();
    let waiting = |s: &prism_core::state::Snapshot| -> HashMap<String, String> { s.agents.iter().filter(|a| matches!(a.status, WaitingTool | NeedsYou)).map(|a| (a.id.clone(), a.title.clone())).collect() };
    let mut before = waiting(&rx.borrow_and_update());
    loop {
        if rx.changed().await.is_err() {
            return;
        }
        let now_waiting = waiting(&rx.borrow_and_update());
        let started = now_waiting.iter().find(|(id, _)| !before.contains_key(*id)).map(|(id, title)| (id.clone(), title.clone()));
        before = now_waiting;
        let Some((agent, title)) = started else { continue };
        let wake = relay::Wake { profile: inner.profile.clone(), agent, title, text: None, at: Utc::now() };
        if inner.live().relay {
            relay::tell_hub(&inner, &wake).await;
        } else {
            inner.wake(wake).await;
        }
    }
}

impl Inner {
    /// An agent started waiting, here or in a relayed profile: kept for the phone to ask what
    /// woke it, and the phones without a page open woken.
    async fn wake(&self, w: relay::Wake) {
        *lock(&self.last_wake) = Some(w);
        if !self.live().push {
            return;
        }
        let targets: Vec<(String, String)> = {
            let connected = self.connected();
            let auth = self.auth();
            auth.devices().iter().filter(|d| connected.get(&d.id).copied().unwrap_or(0) == 0).filter_map(|d| d.push.clone().map(|e| (d.id.clone(), e))).collect()
        };
        for (device, endpoint) in targets {
            {
                let mut pushed = lock(&self.pushed);
                if pushed.get(&device).is_some_and(|t| t.elapsed() < relay::PUSH_EVERY) {
                    continue;
                }
                pushed.insert(device.clone(), std::time::Instant::now());
            }
            if let Err(e) = self.vapid.send(&endpoint).await {
                tracing::warn!("remote: {e:#}");
            }
        }
    }
}

impl Inner {
    /// The remote's state into the snapshot, for the desk.
    fn publish(&self) {
        let relayed = self.live().relay;
        // Relayed: its address at the hub, while the hub's Prism is open.
        let url = if relayed { relay::hub(self).and_then(|h| h.url).map(|u| format!("{u}/p/{}", self.profile)) } else { self.url().clone() };
        let error = self.error().clone();
        let on = url.is_some() || error.is_some() || (relayed && lock(&self.secret).is_some());
        let status = on.then(|| {
            let connected = self.connected().clone();
            let mut auth = self.auth();
            let pairing = auth.pairing(Utc::now()).and_then(|(token, expires_at, claim)| {
                let link = format!("{}/pair#{token}", url.as_deref()?);
                let qr_svg = qrcode::QrCode::new(link.as_bytes()).ok()?.render::<qrcode::render::svg::Color>().min_dimensions(200, 200).quiet_zone(true).build();
                let (code, device, ready) = match claim {
                    Some((c, d, r)) => (Some(c), Some(d), r),
                    None => (None, None, false),
                };
                Some(RemotePairing { url: link, qr_svg, expires_at, code, device, ready })
            });
            let devices = auth
                .devices()
                .iter()
                .map(|d| RemoteDevice { id: d.id.clone(), name: d.name.clone(), paired_at: d.paired_at, last_seen: d.last_seen, connected: connected.get(&d.id).is_some_and(|n| *n > 0) })
                .collect();
            let controls = self.controls().iter().map(|c| prism_core::state::RemoteControl { agent: c.agent.clone(), device: c.device_name.clone(), since: c.since }).collect();
            let peers = peers::list(&self.dir)
                .unwrap_or_default()
                .into_iter()
                .map(|p| prism_core::state::RemotePeer { connected: connected.get(&format!("{}{}", peers::CONNECTED, p.name)).is_some_and(|n| *n > 0), name: p.name, paired_at: p.created })
                .collect();
            let now = Utc::now();
            let peer_pairing = self.peer_pairing().as_ref().filter(|p| p.open(now) && !p.done()).map(|p| prism_core::state::PeerPairing { code: p.code.clone(), expires_at: p.expires, name: p.request.as_ref().map(|r| r.name.clone()) });
            RemoteStatus { url: url.clone(), error, devices, pairing, controls, relay: relayed, peers, peer_pairing }
        });
        self.hub.update(|s| replace(&mut s.remote, status));
    }

    /// A page of `device` connects (`true`) or goes.
    fn connect(&self, device: &str, on: bool) {
        {
            let mut c = self.connected();
            let n = c.entry(device.to_string()).or_default();
            *n = if on { *n + 1 } else { n.saturating_sub(1) };
        }
        self.hub.set_watching(on);
        self.publish();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use prism_core::{config::{RemoteConfig, RemoteTunnel}, state::Hub};

    /// The desk's commands run on the window's thread, outside the async runtime: opening a
    /// pairing from there must not need one (it once panicked, and Prism with it).
    #[test]
    fn a_pairing_opens_from_the_windows_thread() {
        let rt: &'static tokio::runtime::Runtime = Box::leak(Box::new(tokio::runtime::Runtime::new().unwrap()));
        prism_base::rt::init(rt.handle().clone());
        struct None_;
        impl crate::Host for None_ {
            fn call(&self, _: String, _: serde_json::Value) -> futures::future::BoxFuture<'static, anyhow::Result<serde_json::Value>> {
                Box::pin(async { anyhow::bail!("none") })
            }
            fn class(&self, _: &str, _: &serde_json::Value) -> Option<prism_core::remote::Class> {
                None
            }
            fn asset(&self, _: &str) -> Option<(Vec<u8>, String)> {
                None
            }
            fn watch(&self, _: String, _: Option<(u16, u16)>, _: bool, _: Box<dyn Fn(Vec<u8>) + Send>, _: Box<dyn FnOnce() + Send>) -> futures::future::BoxFuture<'static, anyhow::Result<crate::Watch>> {
                Box::pin(async { anyhow::bail!("none") })
            }
        }
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let dir = std::env::temp_dir().join(format!("prism-remote-thread-{}", std::process::id()));
        let remote = crate::Remote::new(&dir, Hub::new(), Arc::new(None_)).unwrap();
        let cfg = RemoteConfig { enabled: true, tunnel: RemoteTunnel::Other, port, url: format!("http://localhost:{port}"), keep_awake: false, ..Default::default() };
        rt.block_on(remote.apply(&cfg));
        let r = std::thread::spawn({
            let remote = remote.clone();
            move || remote.pair_start()
        })
        .join();
        assert!(matches!(r, Ok(Ok(()))), "opening a pairing off the runtime: {r:?}");
        rt.block_on(remote.apply(&RemoteConfig::default()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
