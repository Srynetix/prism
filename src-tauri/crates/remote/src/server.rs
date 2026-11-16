//! The remote's HTTP side: the page and its files, pairing, login, and the WebSocket a paired
//! phone's page talks over. Every request first goes through `guard`: the expected host, the
//! tailnet owner (tunnel tailscale), the page's own origin for anything that changes something.

use std::sync::Arc;

use anyhow::Context;
use prism_base::{i18n::tr, tr};
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Extension, Request, State,
    },
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use prism_core::remote::Class;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::PairState,
    snapshot,
    webauthn::{Assertion, Registration},
    Inner, PROTOCOL,
};

mod peer;
mod tty;
pub(crate) use peer::peer_of;
use peer::*;
use tty::*;

pub(crate) type Shared = Arc<Inner>;

/// The session's cookie: `__Host-` keeps it to this host, HTTPS, path `/`.
pub(crate) const COOKIE: &str = "__Host-prism";
/// Largest message a page may send: an image attached from a phone (resized to 2048 px, in base64)
/// fits.
const MAX_MESSAGE: usize = 4 << 20;
/// Calls of one page answered at once.
const MAX_CALLS: usize = 8;

/// The remote's routes. Relayed (`relay`): the page's socket, its terminals, downloads and
/// pages, and its summary for the hub; nothing of pairing nor login, which are the hub's.
pub fn router(inner: Shared, relayed: bool) -> Router {
    if relayed {
        return Router::new()
            .route("/api/ws", get(socket))
            .route("/api/tty", get(tty))
            .route("/api/download/{token}", get(crate::download::serve))
            .route("/page/{*rest}", get(crate::pages::serve))
            .route("/modules/{*rest}", get(crate::modules::serve))
            .route("/relay/summary", get(crate::relay::own_summary))
            // A peer's window, through the hub (`relay::peer_proxy`), which checked its token.
            .route("/peer/ws", get(peer_socket))
            .route("/peer/tty", get(peer_tty))
            .route("/peer/summary", get(peer_summary))
            .route("/peer/download/{token}", get(crate::download::serve_peer))
            .layer(middleware::from_fn_with_state(inner.clone(), guard))
            .with_state(inner);
    }
    Router::new()
        .route("/api/profiles", get(crate::relay::profiles))
        .route("/api/profiles/open", post(crate::relay::open_profile))
        .route("/api/wake", get(crate::relay::last_wake))
        .route("/relay/wake", post(crate::relay::wake_from))
        // `prism pair`, from this machine (the endpoint's secret).
        .route("/relay/pair", get(crate::pair::state))
        .route("/relay/pair/start", post(crate::pair::start))
        .route("/relay/pair/confirm", post(crate::pair::confirm))
        .route("/relay/pair/cancel", post(crate::pair::cancel))
        // `prism peer pair`: a computer's pairing, from this machine.
        .route("/relay/peer", get(crate::pair::peer_state))
        .route("/relay/peer/start", post(crate::pair::peer_start))
        .route("/relay/peer/answer", post(crate::pair::peer_answer))
        .route("/relay/peer/cancel", post(crate::pair::peer_cancel))
        .route("/p/{id}/{*rest}", axum::routing::any(crate::relay::proxy))
        .route("/api/pair/claim", post(pair_claim))
        .route("/api/pair/finish", post(pair_finish))
        .route("/api/pair/state", post(pair_state))
        .route("/api/login/begin", post(login_begin))
        .route("/api/login/finish", post(login_finish))
        .route("/api/logout", post(logout))
        .route("/api/session", get(session))
        .route("/api/ws", get(socket))
        .route("/api/tty", get(tty))
        .route("/api/download/{token}", get(crate::download::serve))
        .route("/page/{*rest}", get(crate::pages::serve))
        .route("/modules/{*rest}", get(crate::modules::serve))
        // Another Prism's window (`peers`): its token, no page nor cookie.
        .route("/peer/ws", get(peer_socket))
        .route("/peer/tty", get(peer_tty))
        .route("/peer/summary", get(peer_summary))
        .route("/peer/download/{token}", get(crate::download::serve_peer))
        .route("/peer/p/{id}/{*rest}", axum::routing::any(crate::relay::peer_proxy))
        .route("/peer/pair", post(peer_pair))
        .route("/peer/pair/state", post(peer_pair_state))
        .fallback(get(asset))
        .layer(middleware::from_fn_with_state(inner.clone(), guard))
        .with_state(inner)
}

/// A window of this computer's app showing this profile (`Remote::serve_local`): the same account,
/// through the profile's own socket (0600, in its private data folder), so neither token nor
/// confirmation of the peers'. What its page classes critical it confirms as the main window does.
#[derive(Clone, Copy)]
pub(crate) struct Local;

/// The name a local window goes by (logs, its downloads).
pub(crate) const LOCAL_PEER: &str = "this computer's app";

/// The profile's local socket (a named pipe on Windows) as axum serves it: each connection the
/// listener accepts, an error waited out a second.
pub(crate) struct LocalListener(pub(crate) prism_sys::ipc::Listener);

impl axum::serve::Listener for LocalListener {
    type Io = prism_sys::ipc::Server;
    type Addr = ();

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.0.accept().await {
                Ok(io) => return (io, ()),
                Err(e) => {
                    tracing::warn!("local socket {}: {e}", self.0.at().display());
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(())
    }
}

/// The routes of the profile's local socket: a peer's, nothing of the phone's.
pub fn local_router(inner: Shared) -> Router {
    Router::new()
        .route("/peer/ws", get(peer_socket))
        .route("/peer/tty", get(peer_tty))
        .route("/peer/summary", get(peer_summary))
        .route("/peer/download/{token}", get(crate::download::serve_peer))
        .layer(Extension(Local))
        .with_state(inner)
}

fn refuse(status: StatusCode, why: &str) -> Response {
    (status, Json(json!({ "error": why }))).into_response()
}

fn error(e: anyhow::Error) -> Response {
    refuse(StatusCode::BAD_REQUEST, &format!("{e:#}"))
}

/// Before anything: the host the page is served under (no DNS rebinding of loopback), the
/// tailnet's owner, and for a change or the socket, the page's origin. Then the headers that keep
/// the page to itself.
///
/// Relayed (`relay`): only the hub, its secret in each request, which checked the rest; its
/// address remembered for the pages' framing. On the hub, `/relay/` is for the relayed Prisms,
/// their secret in it, the rest of the checks theirs to skip.
async fn guard(State(inner): State<Shared>, req: Request, next: Next) -> Response {
    let live = inner.live().clone();
    if live.relay || req.uri().path().starts_with("/relay/") {
        if !crate::relay::trusted(&inner, req.headers()) {
            return refuse(StatusCode::FORBIDDEN, "not the relay");
        }
        if live.relay
            && let Some(origin) = req.headers().get(crate::relay::ORIGIN).and_then(|h| h.to_str().ok()).filter(|o| !o.is_empty())
            && live.hub_origin.as_deref() != Some(origin)
        {
            inner.live().hub_origin = Some(origin.to_string());
        }
        return next.run(req).await;
    }
    let Some(party) = live.party.as_ref() else { return refuse(StatusCode::SERVICE_UNAVAILABLE, tr("starting")) };
    let headers = req.headers();
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or_default();
    let host_name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    let loopback = [format!("127.0.0.1:{}", live.port), format!("localhost:{}", live.port)];
    // A peer reaches it through a tunnel's end too (`ssh -L 9000:127.0.0.1:<port>`): any loopback
    // port. No cookie nor page there, its token sent by hand: nothing for a rebound name to borrow.
    let peer_route = req.uri().path().starts_with("/peer/");
    let peer_loopback = peer_route && matches!(host_name, "127.0.0.1" | "localhost" | "[::1]");
    if host_name != party.id && !loopback.iter().any(|l| l == host) && !peer_loopback {
        return refuse(StatusCode::MISDIRECTED_REQUEST, "unexpected host");
    }
    if let Some(owner) = &live.owner {
        let user = headers.get("tailscale-user-login").and_then(|h| h.to_str().ok());
        // Through `tailscale serve`, the tailnet's user, checked; a peer straight on loopback (an
        // SSH tunnel's end) has none, its token checked instead. Pairing, before any token, is the
        // tailnet user's only: without that header it may as well come through Funnel or a tagged
        // node (`prism peer add` pastes a token through an SSH tunnel instead).
        let pairing = req.uri().path().starts_with("/peer/pair");
        let unproxied_peer = peer_route && !pairing && user.is_none();
        if user != Some(owner.as_str()) && !unproxied_peer {
            tracing::warn!("remote: refused a request from {}", user.unwrap_or("no tailnet user"));
            return refuse(StatusCode::FORBIDDEN, tr("only this computer's tailnet user"));
        }
    }
    // A relayed profile's path as its Prism sees it (`/p/<profile>/api/ws`: `/api/ws`).
    let path = relayed_path(req.uri().path()).to_string();
    let changes = req.method() != Method::GET || matches!(path.as_str(), "/api/ws" | "/api/tty");
    // A peer is no browser: its token is sent by hand, never by a page another site opens.
    let peer = path.starts_with("/peer/");
    if changes && !peer && headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) != Some(party.origin.as_str()) {
        return refuse(StatusCode::FORBIDDEN, "another origin");
    }
    let api = path.starts_with("/api/");
    // An agent's page sets its own headers (`pages`): sandboxed, framed by this page.
    if path.starts_with("/page/") {
        return next.run(req).await;
    }
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    let set = |h: &mut HeaderMap, k: header::HeaderName, v: &'static str| {
        h.insert(k, HeaderValue::from_static(v));
    };
    set(h, header::CONTENT_SECURITY_POLICY, "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; manifest-src 'self'; worker-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'");
    set(h, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(h, header::REFERRER_POLICY, "no-referrer");
    set(h, header::X_FRAME_OPTIONS, "DENY");
    set(h, header::HeaderName::from_static("permissions-policy"), "camera=(), microphone=(), geolocation=(), publickey-credentials-get=(self), publickey-credentials-create=(self)");
    if api {
        set(h, header::CACHE_CONTROL, "no-store");
    }
    res
}

/// `/p/<profile>/<rest>` as `/<rest>`; any other path as it is.
fn relayed_path(path: &str) -> &str {
    path.strip_prefix("/p/").and_then(|rest| rest.find('/').map(|i| &rest[i..])).unwrap_or(path)
}

/// The page and its files, from the app's bundle. Any page path is the page: it routes itself.
async fn asset(State(inner): State<Shared>, req: Request) -> Response {
    let path = req.uri().path().trim_start_matches('/');
    let file = if path.is_empty() || path == "pair" || !path.contains('.') { "remote.html" } else { path };
    // The page's own files only: not whatever else the bundle holds (the preview's snapshot).
    let served = file == "remote.html" || file == "favicon.svg" || ((file.starts_with("assets/") || file.starts_with("remote/")) && !file.contains(".."));
    if !served {
        return refuse(StatusCode::NOT_FOUND, "no such file");
    }
    match inner.host.asset(file) {
        Some((bytes, mime)) => {
            let cache = if file.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
            let zipped = (accepts_gzip(req.headers()) && compressible(&mime, bytes.len())).then(|| gzipped(file, &bytes)).flatten();
            let gzip = zipped.is_some();
            let body = zipped.unwrap_or_else(|| Arc::new(bytes));
            let mut res = ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache.to_string())], Body::from(body.to_vec())).into_response();
            res.headers_mut().insert(header::VARY, HeaderValue::from_static("accept-encoding"));
            if gzip {
                res.headers_mut().insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
            }
            // The service worker lives in `remote/` but serves the whole page.
            if file == "remote/sw.js" {
                res.headers_mut().insert(header::HeaderName::from_static("service-worker-allowed"), HeaderValue::from_static("/"));
            }
            res
        }
        None => refuse(StatusCode::NOT_FOUND, "no such file"),
    }
}

/// The phone takes gzip (every browser does): the page's scripts and styles a third of their size.
fn accepts_gzip(headers: &HeaderMap) -> bool {
    headers.get_all(header::ACCEPT_ENCODING).iter().filter_map(|v| v.to_str().ok()).any(|v| v.split(',').any(|e| e.trim().split(';').next() == Some("gzip")))
}

/// Text worth compressing: scripts, styles, the page, SVG, JSON; not fonts or images (already).
fn compressible(mime: &str, len: usize) -> bool {
    len > 1024 && (mime.starts_with("text/") || mime.contains("javascript") || mime.contains("json") || mime.contains("svg"))
}

/// `bytes` gzipped, once for a bundle's file (`assets/`, named by its content); the page and its
/// worker, which change with a build in `just dev`, each time.
fn gzipped(file: &str, bytes: &[u8]) -> Option<Arc<Vec<u8>>> {
    use std::io::Write;
    static DONE: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, Arc<Vec<u8>>>>> = std::sync::LazyLock::new(Default::default);
    let keep = file.starts_with("assets/");
    if keep && let Some(done) = DONE.lock().unwrap().get(file) {
        return Some(done.clone());
    }
    let mut enc = flate2::write::GzEncoder::new(Vec::with_capacity(bytes.len() / 3), flate2::Compression::new(6));
    let out = Arc::new(enc.write_all(bytes).and_then(|()| enc.finish()).ok()?);
    if keep {
        DONE.lock().unwrap().insert(file.to_string(), out.clone());
    }
    Some(out)
}

pub(crate) fn cookie_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
}

fn with_session(mut res: Response, session: &str, max_hours: i64) -> Response {
    let cookie = format!("{COOKIE}={session}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={}", max_hours * 3600);
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        res.headers_mut().append(header::SET_COOKIE, v);
    }
    res
}

/// The phone a request's session belongs to, if it is open. Relayed: the one the hub let in.
pub(crate) fn device_of(inner: &Inner, headers: &HeaderMap) -> Option<String> {
    if inner.live().relay {
        return headers.get(crate::relay::DEVICE).and_then(|h| h.to_str().ok()).filter(|d| !d.is_empty()).map(String::from);
    }
    let token = cookie_of(headers)?;
    inner.auth().check(&token, Utc::now())
}

#[derive(Deserialize)]
struct Claim {
    token: String,
    name: String,
}

async fn pair_claim(State(inner): State<Shared>, Json(c): Json<Claim>) -> Response {
    let party = inner.live().party.clone();
    let Some(party) = party else { return refuse(StatusCode::SERVICE_UNAVAILABLE, tr("starting")) };
    let r = inner.auth().pair_claim(&c.token, &c.name, Utc::now());
    inner.publish();
    match r {
        Ok((code, challenge)) => Json(json!({ "code": code, "challenge": challenge, "rpId": party.id, "user": crate::webauthn::random(), "name": c.name })).into_response(),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct Finish {
    token: String,
    registration: Registration,
}

async fn pair_finish(State(inner): State<Shared>, Json(f): Json<Finish>) -> Response {
    let party = inner.live().party.clone();
    let Some(party) = party else { return refuse(StatusCode::SERVICE_UNAVAILABLE, tr("starting")) };
    let r = inner.auth().pair_finish(&f.token, &f.registration, &party, Utc::now());
    inner.publish();
    match r {
        Ok(()) => Json(json!({})).into_response(),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct Token {
    token: String,
    /// The challenge `pair_claim` gave the phone: what tells it from another that saw the QR code.
    #[serde(default)]
    claim: String,
}

async fn pair_state(State(inner): State<Shared>, Json(t): Json<Token>) -> Response {
    let (state, max) = {
        let mut auth = inner.auth();
        (auth.pair_state(&t.token, &t.claim, Utc::now()), auth.max.num_hours())
    };
    match state {
        PairState::Waiting => Json(json!({ "state": "waiting" })).into_response(),
        PairState::Gone => Json(json!({ "state": "gone" })).into_response(),
        PairState::Paired { session } => {
            inner.publish();
            with_session(Json(json!({ "state": "paired" })).into_response(), &session, max)
        }
    }
}

async fn login_begin(State(inner): State<Shared>) -> Response {
    let party = inner.live().party.clone();
    let Some(party) = party else { return refuse(StatusCode::SERVICE_UNAVAILABLE, tr("starting")) };
    match inner.auth().login_begin(Utc::now()) {
        Ok((challenge, allow)) => Json(json!({ "challenge": challenge, "rpId": party.id, "allow": allow })).into_response(),
        Err(e) => error(e),
    }
}

async fn login_finish(State(inner): State<Shared>, Json(a): Json<Assertion>) -> Response {
    let party = inner.live().party.clone();
    let Some(party) = party else { return refuse(StatusCode::SERVICE_UNAVAILABLE, tr("starting")) };
    let (r, max) = {
        let mut auth = inner.auth();
        (auth.login_finish(&a, &party, Utc::now()), auth.max.num_hours())
    };
    match r {
        Ok((session, _)) => {
            inner.publish();
            with_session(Json(json!({})).into_response(), &session, max)
        }
        Err(e) => {
            tracing::warn!("remote: a login failed: {e:#}");
            refuse(StatusCode::UNAUTHORIZED, &format!("{e:#}"))
        }
    }
}

async fn logout(State(inner): State<Shared>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie_of(&headers) {
        inner.auth().logout(&token);
    }
    let mut res = Json(json!({})).into_response();
    res.headers_mut().append(header::SET_COOKIE, HeaderValue::from_static("__Host-prism=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0"));
    res
}

/// Whether the page has an open session, and whether a phone is paired at all.
async fn session(State(inner): State<Shared>, headers: HeaderMap) -> Response {
    let device = device_of(&inner, &headers);
    let auth = inner.auth();
    let name = device.as_ref().and_then(|d| auth.devices().iter().find(|x| &x.id == d)).map(|d| d.name.clone());
    Json(json!({ "device": name, "paired": !auth.devices().is_empty(), "protocol": PROTOCOL })).into_response()
}

/// The session a socket lives as long as: the phone's here; relayed, none (the hub watches it).
fn session_of(inner: &Inner, headers: &HeaderMap) -> Option<Option<String>> {
    if inner.live().relay { Some(None) } else { cookie_of(headers).map(Some) }
}

/// A socket's session still open (relayed: the hub closes it otherwise).
fn alive(inner: &Inner, session: &Option<String>) -> bool {
    session.as_ref().is_none_or(|t| inner.auth().alive(t, Utc::now()))
}

async fn socket(State(inner): State<Shared>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    let (Some(session), Some(device)) = (session_of(&inner, &headers), device_of(&inner, &headers)) else { return refuse(StatusCode::UNAUTHORIZED, tr("no session")) };
    let name = crate::relay::device_name(&inner, &headers, &device);
    ws.max_message_size(MAX_MESSAGE).on_upgrade(move |socket| converse(inner, socket, session, device, name))
}

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum FromPage {
    /// `assertion`: the passkey's answer to a step-up this call asked for. `confirmed`: relayed,
    /// the purpose the hub checked the passkey's answer for (`relay`).
    Call { id: u64, command: String, #[serde(default)] args: Value, #[serde(default)] assertion: Option<Assertion>, #[serde(default)] confirmed: Option<String> },
}

/// A call's answer: its value, or a step-up to pass first (the passkey's challenge; relayed, its
/// purpose only, which the hub makes a challenge).
pub(crate) enum Reply {
    Value(Value),
    StepUp(Value),
}

/// What confirms a call's step-up: the phone's passkey answer, checked here; or, relayed, the
/// purpose the hub checked it for.
pub(crate) enum Proof {
    Phone(Option<Assertion>),
    Relayed(Option<String>),
}

/// A phone's page connected: the snapshot then its changes, its calls answered, until its
/// session ends or it goes.
async fn converse(inner: Shared, socket: WebSocket, token: Option<String>, device: String, name: String) {
    inner.connect(&device, true);
    let (mut tx, mut rx) = socket.split();
    let (out, mut queue) = tokio::sync::mpsc::channel::<Message>(64);
    let writer = tokio::spawn(async move {
        while let Some(m) = queue.recv().await {
            if tx.send(m).await.is_err() {
                break;
            }
        }
        let _ = tx.close().await;
    });
    let (push, vapid) = (inner.live().push, inner.vapid.public());
    let (tabs, mail, classes, typing, power, settings, projects, preview) = {
        let live = inner.live();
        let classes: Vec<&str> = ["read"].into_iter().chain(live.actions.then_some("act")).chain(live.critical.then_some("critical")).collect();
        (live.tabs.clone(), live.mail, classes, live.typing, live.power, live.settings, live.projects, live.preview_origin.is_some())
    };
    // Whether a restart from the phone brings the Mac back by itself (FileVault).
    let unlock = if power { Some(prism_base::platform::restart_unlock().await) } else { None };
    let _ = out.send(text(json!({ "t": "hello", "protocol": PROTOCOL, "device": name, "tabs": tabs, "mail": mail, "classes": classes, "typing": typing, "power": power, "unlock": unlock, "settings": settings, "projects": projects, "preview": preview, "push": push.then_some(vapid) }))).await;

    // The snapshot: whole first, then its changes; ended with the session.
    let snaps = {
        let token = token.clone();
        snapshots(inner.clone(), out.clone(), move |inner| alive(inner, &token), |inner| inner.live().mail)
    };

    let calls = Arc::new(tokio::sync::Semaphore::new(MAX_CALLS));
    while let Some(Ok(msg)) = rx.next().await {
        // Each message renews the session; an ended one closes the socket.
        if token.as_ref().is_some_and(|t| inner.auth().check(t, Utc::now()).is_none()) {
            let _ = out.send(Message::Close(Some(axum::extract::ws::CloseFrame { code: 4401, reason: "session ended".into() }))).await;
            break;
        }
        let Message::Text(body) = msg else { continue };
        let Ok(FromPage::Call { id, command, args, assertion, confirmed }) = serde_json::from_str::<FromPage>(&body) else { continue };
        // Relayed, only the hub's word confirms; a phone's own `confirmed` never counts here.
        let proof = if inner.live().relay { Proof::Relayed(confirmed) } else { Proof::Phone(assertion) };
        let Ok(permit) = calls.clone().try_acquire_owned() else {
            let _ = out.send(text(json!({ "t": "result", "id": id, "error": "too many calls at once" }))).await;
            continue;
        };
        let (inner, out, name, device) = (inner.clone(), out.clone(), name.clone(), device.clone());
        tokio::spawn(async move {
            let reply = match call(&inner, &device, &name, command, args, proof).await {
                Ok(Reply::Value(v)) => json!({ "t": "result", "id": id, "ok": v }),
                Ok(Reply::StepUp(s)) => json!({ "t": "result", "id": id, "stepUp": s }),
                Err(e) => json!({ "t": "result", "id": id, "error": format!("{e:#}") }),
            };
            let _ = out.send(text(reply)).await;
            drop(permit);
        });
    }
    snaps.abort();
    drop(out);
    let _ = writer.await;
    inner.connect(&device, false);
}

/// The snapshot to a socket: whole first, then its changes, at most twice a second, until `still`
/// says its session ended (the socket then closes, 4401). `mail`: whether the mail goes out.
fn snapshots(inner: Shared, out: tokio::sync::mpsc::Sender<Message>, still: impl Fn(&Inner) -> bool + Send + 'static, mail: impl Fn(&Inner) -> bool + Send + 'static) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut rx = inner.hub.subscribe();
        let mut sent: Option<Value> = None;
        // Ten seconds without a change: a sign of life, so that the phone tells a quiet computer
        // from a socket dead without a word (asleep, a network change, a proxy), and that
        // nothing on the way closes it for being idle.
        let mut quiet = false;
        loop {
            // A session ended (idle, revoked, the desk's Stop): the socket closes.
            if !still(&inner) {
                let _ = out.send(Message::Close(Some(axum::extract::ws::CloseFrame { code: 4401, reason: "session ended".into() }))).await;
                break;
            }
            let mail = mail(&inner);
            // Copied, then serialized: the snapshot's lock is not held while it turns into JSON
            // (milliseconds with a long Tokens history), which would hold every collector's update back.
            let snap = rx.borrow_and_update().clone();
            let now = snapshot::filter(serde_json::to_value(&snap).unwrap_or_default(), mail);
            let msg = match &sent {
                None => Some(json!({ "t": "snapshot", "value": now })),
                Some(old) => {
                    let changes = snapshot::diff(old, &now);
                    (!changes.is_empty()).then(|| json!({ "t": "changes", "changes": changes }))
                }
            };
            let msg = msg.or_else(|| quiet.then(|| json!({ "t": "alive" })));
            if let Some(m) = msg
                && out.send(text(m)).await.is_err()
            {
                break;
            }
            sent = Some(now);
            quiet = tokio::select! {
                changed = rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    false
                }
                // Even without a change, a session's end is noticed.
                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => true,
            };
        }
    })
}

/// What a step-up confirms: the command and its very arguments.
pub(crate) fn purpose(command: &str, args: &Value) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{command}\n{args}").as_bytes());
    format!("{command}:{}", digest.iter().take(16).map(|b| format!("{b:02x}")).collect::<String>())
}

/// A step-up: confirmed by `assertion` if it answers this purpose, else a new challenge.
/// Relayed: confirmed by the hub for this very purpose, else the purpose for the hub to ask.
pub(crate) fn step_up(inner: &Inner, device: &str, purpose: &str, proof: Proof) -> anyhow::Result<Option<Reply>> {
    let assertion = match proof {
        Proof::Relayed(confirmed) => {
            if confirmed.as_deref() == Some(purpose) {
                inner.auth().confirmed(device, purpose, Utc::now());
                return Ok(None);
            }
            return Ok(Some(Reply::StepUp(json!({ "purpose": purpose }))));
        }
        Proof::Phone(a) => a,
    };
    let party = inner.live().party.clone().context("starting")?;
    let mut auth = inner.auth();
    match assertion {
        Some(a) => {
            auth.step_up_finish(device, purpose, &a, &party, Utc::now())?;
            Ok(None)
        }
        None => {
            let (challenge, allow) = auth.step_up_begin(device, purpose, Utc::now())?;
            Ok(Some(Reply::StepUp(json!({ "challenge": challenge, "rpId": party.id, "allow": allow }))))
        }
    }
}

/// A call from a phone, if its class lets it through: a critical one once the phone's passkey
/// confirms it. What acts is logged, with the phone.
async fn call(inner: &Inner, device: &str, name: &str, command: String, args: Value, proof: Proof) -> anyhow::Result<Reply> {
    if command == "get_snapshot" {
        let mail = inner.live().mail;
        return Ok(Reply::Value(snapshot::filter(serde_json::to_value(&*inner.hub.subscribe().borrow())?, mail)));
    }
    let (actions, critical, typing) = {
        let live = inner.live();
        (live.actions, live.critical, live.typing)
    };
    // The phone's own push subscription (or none): an endpoint of the push services only.
    if command == "remote_push" {
        anyhow::ensure!(inner.live().push, "notifications are off (Settings › Remote › Sessions)");
        let endpoint = args.get("endpoint").and_then(Value::as_str).map(String::from);
        inner.auth().set_push(device, endpoint)?;
        return Ok(Reply::Value(Value::Null));
    }
    // Taking an agent's terminal: confirmed by the passkey, then good for `step_up_minutes`.
    if command == "remote_control" {
        let agent = args.get("agent").and_then(Value::as_str).unwrap_or_default().to_string();
        anyhow::ensure!(typing, "typing in a terminal from a phone is off (Settings › Remote › Exposure)");
        anyhow::ensure!(inner.host.class("agent_write", &json!({ "id": agent })) == Some(Class::Critical), "this terminal is not typed in from a phone");
        let purpose = format!("control:{agent}");
        if !inner.auth().stepped_up(device, &purpose, Utc::now())
            && let Some(reply) = step_up(inner, device, &purpose, proof)?
        {
            return Ok(reply);
        }
        tracing::info!("remote: {name}: takes the terminal of {agent}");
        return Ok(Reply::Value(Value::Null));
    }
    // A text typed to an agent (the phone's answer box): typing, as taking its terminal, confirmed
    // by the passkey for the same while — one covers the other — whatever `[remote] critical` says.
    // A key pressed in one of its shells (`shell:<agent>:<n>`) too: a shell runs anything, Up and
    // Enter its last command.
    let shell_key = command == "agent_key" && args.get("id").and_then(Value::as_str).is_some_and(|id| id.starts_with("shell:"));
    // An answer to its question with your own words in it: typed text too.
    let worded = command == "agent_answer" && args["answers"].as_array().is_some_and(|a| a.iter().any(|x| x["text"].as_str().is_some_and(|t| !t.trim().is_empty())))
        || command == "agent_plan_answer" && args["text"].as_str().is_some_and(|t| !t.trim().is_empty());
    // A file of its folder written from the phone (its Files preview, edited): typing too.
    let saved = command == "agent_file_save";
    // Another CLI taking the agent on, started from the relay's words: typed to it too.
    let relayed = command == "agent_switch";
    if command == "agent_type" || shell_key || worded || saved || relayed {
        let agent = args.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
        anyhow::ensure!(typing, "{}", tr("typing to an agent from a phone is off (Settings › Remote › Exposure)"));
        // Its words reach the agent as `agent_type`'s do.
        let typed = if shell_key { "agent_write" } else { "agent_type" };
        anyhow::ensure!(inner.host.class(typed, &json!({ "id": agent })) == Some(Class::Critical), "{}", tr!("{other} is not answered from a phone", other = command));
        let purpose = format!("control:{agent}");
        if !inner.auth().stepped_up(device, &purpose, Utc::now())
            && let Some(reply) = step_up(inner, device, &purpose, proof)?
        {
            return Ok(reply);
        }
        if relayed {
            let to = args["switch"]["to"].as_str().unwrap_or_default();
            tracing::info!("remote: {name}: {agent} continues with {to} (confirmed by its passkey)");
        } else if saved {
            let path = args.get("path").and_then(Value::as_str).unwrap_or_default();
            tracing::info!("remote: {name}: writes {path} in {agent}'s folder (confirmed by its passkey)");
        } else {
            tracing::info!("remote: {name}: types to {agent} (confirmed by its passkey)");
        }
        return inner.host.call(command, args).await.map(Reply::Value);
    }
    // An agent launched with a prompt (the launcher, an agent action), or with "Then" steps (texts
    // sent to it later, a review agent's prompt, a task run): the passkey, bound to this launch,
    // its prompt and steps included. Not `[remote] typing`, which is a terminal's keys: launching
    // agents is what a phone's actions are for. A task step runs a command, as `agent_run`: critical.
    let launching = matches!(command.as_str(), "agent_spawn" | "agent_prepare");
    let then = args["launch"]["then"].as_array().map(Vec::as_slice).unwrap_or_default();
    let prompted = launching && (args["launch"]["prompt"].as_str().is_some_and(|p| !p.trim().is_empty()) || !then.is_empty());
    if prompted {
        anyhow::ensure!(actions, "{}", tr!("{command}: actions from a phone are off (Settings › Remote › Exposure)", command = command));
        anyhow::ensure!(critical || !then.iter().any(|s| s["kind"] == "task"), "{}", tr("a task step runs a command: critical actions from a phone are off (Settings › Remote › Exposure)"));
        anyhow::ensure!(inner.host.class(&command, &args).is_some_and(|c| c != Class::Never), "{}", tr!("{command} is never done from a phone", command = command));
        if let Some(reply) = step_up(inner, device, &purpose(&command, &args), proof)? {
            return Ok(reply);
        }
        tracing::info!("remote: {name}: {command} with a prompt (confirmed by its passkey)");
        return inner.host.call(command, args).await.map(Reply::Value);
    }
    // A dev server's preview (`[remote] preview`): the port of a running server the Dev servers
    // block shows, which the preview's address forwards to from now on; its address answered.
    // Relayed: the port checked here, the hub's preview forwarding to it (`relay`).
    if command == "preview_open" {
        let relayed = inner.live().relay;
        let origin = inner.live().preview_origin.clone();
        anyhow::ensure!(relayed || origin.is_some(), tr("the dev servers' preview is off (Settings › Remote › Exposure)"));
        let port = args.get("port").and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok()).context("a port")?;
        anyhow::ensure!(crate::preview::dev_ports(&inner.hub.subscribe().borrow()).contains(&port), "{}", tr!("port {port} is not a running dev server of the Dev servers block", port = port));
        if relayed {
            return Ok(Reply::Value(json!({ "previewPort": port })));
        }
        let origin = origin.unwrap_or_default();
        *inner.preview() = Some(port);
        tracing::info!("remote: {name}: previews the dev server on port {port}");
        return Ok(Reply::Value(json!({ "url": format!("{origin}/") })));
    }
    // Restarting or shutting down the computer: `[remote] power`, and the passkey every time.
    if command == "mac_power" {
        anyhow::ensure!(inner.live().power, "{}", tr("restarting the computer from a phone is off (Settings › Remote › Exposure)"));
        anyhow::ensure!(inner.host.class(&command, &args) == Some(Class::Critical), "{}", tr!("{other} is not answered from a phone", other = "mac_power"));
        // Bound to the action only: a FileVault password never goes into a purpose (hashed, or
        // sent to the hub when relayed).
        let action = args.get("action").and_then(Value::as_str).unwrap_or_default();
        if let Some(reply) = step_up(inner, device, &purpose(&command, &json!({ "action": action })), proof)? {
            return Ok(reply);
        }
        tracing::warn!("remote: {name}: {action} the computer (confirmed by its passkey)");
        return inner.host.call(command, args).await.map(Reply::Value);
    }
    // The settings: `[remote] settings`; read as they are, changed (the few the computer lets a phone
    // change, checked again there) with the passkey every time, bound to the changes.
    if command == "remote_settings_read" || command == "remote_settings_save" {
        anyhow::ensure!(inner.live().settings, "{}", tr("the settings from a phone are off (Settings › Remote › Exposure)"));
        if command == "remote_settings_save" {
            anyhow::ensure!(inner.host.class(&command, &args) == Some(Class::Critical), "{}", tr!("{other} is not answered from a phone", other = "remote_settings_save"));
            if let Some(reply) = step_up(inner, device, &purpose(&command, &args), proof)? {
                return Ok(reply);
            }
            let keys: Vec<String> = args["edits"].as_array().into_iter().flatten().filter_map(|e| e["path"].as_array()).map(|p| p.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(".")).collect();
            tracing::warn!("remote: {name}: changes the settings {} (confirmed by its passkey)", keys.join(", "));
        }
        return inner.host.call(command, args).await.map(Reply::Value);
    }
    // A new project, or a local one published on the forge: `[remote] projects`, and the passkey
    // every time, bound to its name and forge choice; the computer creates in its working folder only,
    // and publishes only one of its repositories without a project.
    // The forge's projects, to clone one: read, with `[remote] projects`.
    if command == "remote_project_search" {
        anyhow::ensure!(inner.live().projects, "{}", tr("projects from a phone are off (Settings › Remote › Exposure)"));
        return inner.host.call(command, args).await.map(Reply::Value);
    }
    if command == "remote_project_create" || command == "remote_project_publish" || command == "remote_project_clone" {
        anyhow::ensure!(inner.live().projects, "{}", tr("projects from a phone are off (Settings › Remote › Exposure)"));
        anyhow::ensure!(inner.host.class(&command, &args) == Some(Class::Critical), "{}", tr!("{other} is not answered from a phone", other = command));
        if let Some(reply) = step_up(inner, device, &purpose(&command, &args), proof)? {
            return Ok(reply);
        }
        let project = args.get("name").or_else(|| args.get("project")).and_then(Value::as_str).unwrap_or_default();
        if command == "remote_project_clone" {
            tracing::warn!("remote: {name}: clones {project} (confirmed by its passkey)");
        } else if command == "remote_project_publish" {
            tracing::warn!("remote: {name}: publishes {project} on the forge (confirmed by its passkey)");
        } else {
            let forge = if args.get("remote").is_some_and(|r| !r.is_null()) { ", and on the forge" } else { "" };
            let workspace = args.get("workspace").and_then(Value::as_str).unwrap_or_default();
            tracing::warn!("remote: {name}: creates the project {project} in {workspace}{forge} (confirmed by its passkey)");
        }
        return inner.host.call(command, args).await.map(Reply::Value);
    }
    // An agent's pages: an address under which its folder (or its scratchpad) is served, as the
    // pages themselves are read.
    if command == "agent_page_open" {
        anyhow::ensure!(inner.host.class("agent_page_text", &args) == Some(Class::Read), "{}", tr!("{other} is not answered from a phone", other = command));
        return crate::pages::open(inner, &args).map(Reply::Value);
    }
    // An agent's files: prepared on the computer, then fetched once over HTTP (`download`).
    if command == "agent_files_download" {
        match inner.host.class(&command, &args) {
            Some(Class::Read) => {}
            // A secret of its folder (`.env`…): with the passkey, as what is critical.
            Some(Class::Critical) if !critical => anyhow::bail!(tr!("{command}: critical actions from a phone are off (Settings › Remote › Exposure)", command = command)),
            Some(Class::Critical) => {
                if let Some(reply) = step_up(inner, device, &purpose(&command, &args), proof)? {
                    return Ok(reply);
                }
                tracing::info!("remote: {name}: {command} of a hidden file (confirmed by its passkey)");
            }
            _ => anyhow::bail!(tr!("{other} is not answered from a phone", other = command)),
        }
        let prepared = inner.host.call(command, args).await?;
        return crate::download::offer(inner, device, prepared).map(Reply::Value);
    }
    let class = inner.host.class(&command, &args);
    match class {
        Some(Class::Read) => return inner.host.call(command, args).await.map(Reply::Value),
        Some(Class::Act) if !actions => anyhow::bail!(tr!("{command}: actions from a phone are off (Settings › Remote › Exposure)", command = command)),
        Some(Class::Act) => {}
        Some(Class::Critical) if !critical => anyhow::bail!(tr!("{command}: critical actions from a phone are off (Settings › Remote › Exposure)", command = command)),
        Some(Class::Critical) => {
            if let Some(reply) = step_up(inner, device, &purpose(&command, &args), proof)? {
                return Ok(reply);
            }
        }
        Some(Class::Never) => anyhow::bail!(tr!("{command} is never done from a phone", command = command)),
        None => anyhow::bail!(tr!("no command {command}", command = command)),
    }
    let how = if class == Some(Class::Critical) { " (confirmed by its passkey)" } else { "" };
    tracing::info!("remote: {name}: {command}{how}");
    let r = inner.host.call(command.clone(), args).await;
    if let Err(e) = &r {
        tracing::warn!("remote: {name}: {command}: {e:#}");
    }
    r.map(Reply::Value)
}

fn text(v: Value) -> Message {
    Message::Text(v.to_string().into())
}

#[cfg(test)]
mod tests;
