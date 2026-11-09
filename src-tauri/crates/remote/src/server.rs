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

/// The routes of the profile's local socket: a peer's, nothing of the phone's.
#[cfg_attr(not(unix), allow(dead_code))]
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
mod tests {
    use std::sync::Arc;

    use futures::{SinkExt, StreamExt};
    use prism_core::{config::{RemoteConfig, RemoteTunnel}, remote::Class, state::Hub};
    use serde_json::{json, Value};
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

    #[test]
    fn gzips_the_page_s_text_files_for_a_browser_that_takes_it() {
        use std::io::Read;
        let mut h = axum::http::HeaderMap::new();
        assert!(!super::accepts_gzip(&h));
        h.insert(axum::http::header::ACCEPT_ENCODING, "br, gzip;q=0.9, deflate".parse().unwrap());
        assert!(super::accepts_gzip(&h));
        assert!(super::compressible("text/javascript", 5000) && !super::compressible("font/woff2", 5000) && !super::compressible("text/css", 200));
        let js = "export const a = 1;\n".repeat(400);
        let zipped = super::gzipped("assets/a-1.js", js.as_bytes()).unwrap();
        assert!(zipped.len() < js.len() / 10);
        assert!(Arc::ptr_eq(&zipped, &super::gzipped("assets/a-1.js", js.as_bytes()).unwrap()), "kept for a bundle's file");
        let mut back = String::new();
        flate2::read::GzDecoder::new(&zipped[..]).read_to_string(&mut back).unwrap();
        assert_eq!(back, js);
    }

    use crate::{webauthn::{tests::Authenticator, Party}, Host, Remote};

    #[derive(Default)]
    struct Fake {
        typed: Arc<std::sync::Mutex<Vec<u8>>>,
    }

    /// A controlled terminal: what the phone types.
    struct Keys(Arc<std::sync::Mutex<Vec<u8>>>);
    impl crate::Term for Keys {
        fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
            self.0.lock().unwrap().extend(data);
            Ok(())
        }
        fn resize(&mut self, _: u16, _: u16) -> anyhow::Result<()> {
            Ok(())
        }
    }

    impl Host for Fake {
        fn peer_call(&self, command: String, args: Value) -> futures::future::BoxFuture<'static, anyhow::Result<Value>> {
            Box::pin(async move {
                anyhow::ensure!(command != "mac_power", "{command} is not answered from another Prism");
                if command == "agent_files_download" {
                    let path = std::env::temp_dir().join(format!("prism-peer-download-{}", std::process::id()));
                    std::fs::write(&path, b"the zip").unwrap();
                    return Ok(json!({ "name": "work.zip", "path": path }));
                }
                Ok(json!({ "peer": command, "args": args }))
            })
        }
        fn call(&self, command: String, _args: Value) -> futures::future::BoxFuture<'static, anyhow::Result<Value>> {
            Box::pin(async move {
                if command == "agent_files_download" {
                    let path = std::env::temp_dir().join(format!("prism-remote-download-{}", std::process::id()));
                    std::fs::write(&path, b"the zip").unwrap();
                    return Ok(json!({ "name": "work é.zip", "path": path }));
                }
                Ok(json!({ "echo": command }))
            })
        }
        fn class(&self, command: &str, _args: &Value) -> Option<Class> {
            match command {
                "rebase_mr" | "agent_spawn" | "agent_answer" | "agent_plan_answer" => Some(Class::Act),
                "mr_detail" | "agent_attach" | "remote_settings_read" | "agent_files_download" | "agent_page_text" => Some(Class::Read),
                "remote_project_search" => Some(Class::Read),
                "merge_mr" | "agent_write" | "agent_type" | "agent_file_save" | "mac_power" | "remote_settings_save" | "remote_project_create" | "remote_project_publish" | "remote_project_clone" => Some(Class::Critical),
                "save_config" => Some(Class::Never),
                _ => None,
            }
        }
        fn page(&self, path: &str) -> Result<(Vec<u8>, String), (u16, String)> {
            Ok((format!("page {path}").into_bytes(), "text/html".into()))
        }
        fn asset(&self, path: &str) -> Option<(Vec<u8>, String)> {
            match path {
                "remote.html" => Some((b"<html>the page".to_vec(), "text/html".into())),
                "remote/sw.js" => Some((b"// sw".to_vec(), "text/javascript".into())),
                _ => None,
            }
        }
        fn watch(&self, agent: String, size: Option<(u16, u16)>, control: bool, on_data: Box<dyn Fn(Vec<u8>) + Send>, _on_end: Box<dyn FnOnce() + Send>) -> futures::future::BoxFuture<'static, anyhow::Result<crate::Watch>> {
            let typed = self.typed.clone();
            Box::pin(async move {
                on_data(format!("screen of {agent}").into_bytes());
                let (cols, rows) = size.unwrap_or((120, 40));
                Ok(crate::Watch { cols, rows, term: if control { Box::new(Keys(typed)) } else { Box::new(crate::ReadOnly(())) } })
            })
        }
    }

    struct Phone {
        base: String,
        origin: String,
        http: reqwest::Client,
        cookie: Option<String>,
    }

    impl Phone {
        async fn post(&self, path: &str, body: Value) -> reqwest::Response {
            let mut r = self.http.post(format!("{}{path}", self.base)).header("origin", &self.origin).json(&body);
            if let Some(c) = &self.cookie {
                r = r.header("cookie", c);
            }
            r.send().await.unwrap()
        }
    }

    fn cookie(res: &reqwest::Response) -> String {
        res.headers().get("set-cookie").unwrap().to_str().unwrap().split(';').next().unwrap().to_string()
    }

    /// A dev server's preview: a running server of the Dev servers block only, the session
    /// required, the dev server asked as on loopback, its socket relayed with its subprotocol.
    #[tokio::test]
    async fn a_phone_previews_a_dev_server() {
        use axum::{extract::ws::{Message as WsMessage, WebSocketUpgrade}, http::{header, HeaderMap, StatusCode}, routing::get};
        // The dev server: what it was asked with, a cookie named as Prism's, a redirect to itself, an echo socket.
        let dev = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dev_port = dev.local_addr().unwrap().port();
        let seen = |h: HeaderMap| async move {
            let get = |k: &str| h.get(k).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
            ([(header::SET_COOKIE, "__Host-prism=forged; Path=/"), (header::SET_COOKIE, "app=1; Path=/")], format!("host={} origin={} cookie={} user={}", get("host"), get("origin"), get("cookie"), get("tailscale-user-login")))
        };
        let app = axum::Router::new()
            .route("/", get(seen))
            .route("/go", get(move || async move { (StatusCode::FOUND, [(header::LOCATION, format!("http://localhost:{dev_port}/there"))]) }))
            .route("/hmr", get(|ws: WebSocketUpgrade| async move {
                ws.protocols(["vite-hmr"]).on_upgrade(|mut s| async move {
                    while let Some(Ok(m)) = s.recv().await {
                        if matches!(m, WsMessage::Text(_)) && s.send(m).await.is_err() {
                            break;
                        }
                    }
                })
            }));
        tokio::spawn(async move { axum::serve(dev, app).await.unwrap() });

        let free = || std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let dir = std::env::temp_dir().join(format!("prism-remote-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let hub = Hub::new();
        let remote = Remote::new(&dir, hub.clone(), Arc::new(Fake::default())).unwrap();
        // Same tell as `a_terminal_pairs_a_phone`'s: retried until the server has both ports.
        let mut tries = 0;
        let (port, pport, url, porigin) = loop {
            tries += 1;
            assert!(tries <= 10, "the remote server never started");
            let (port, pport) = (free(), free());
            let (url, porigin) = (format!("http://localhost:{port}"), format!("http://localhost:{pport}"));
            remote.apply(&RemoteConfig { enabled: true, tunnel: RemoteTunnel::Other, port, url: url.clone(), preview_port: pport, preview_url: porigin.clone(), keep_awake: false, ..Default::default() }).await;
            if crate::relay::read(&dir).is_some() {
                break (port, pport, url, porigin);
            }
        };

        // A phone paired.
        let mut phone = Phone { base: format!("http://127.0.0.1:{port}"), origin: url.clone(), http: reqwest::Client::new(), cookie: None };
        remote.pair_start().unwrap();
        let link = hub.subscribe().borrow().remote.clone().unwrap().pairing.unwrap().url;
        let token = link.split_once('#').unwrap().1.to_string();
        let claim: Value = phone.post("/api/pair/claim", json!({ "token": token, "name": "Pixel 9" })).await.json().await.unwrap();
        let party = Party::of(&url).unwrap();
        let reg = Authenticator::new().create(claim["challenge"].as_str().unwrap(), &party);
        let reg = json!({ "clientDataJson": reg.client_data_json, "attestationObject": reg.attestation_object });
        phone.post("/api/pair/finish", json!({ "token": token, "registration": reg })).await;
        remote.pair_confirm(None).unwrap();
        phone.cookie = Some(cookie(&phone.post("/api/pair/state", json!({ "token": token, "claim": claim["challenge"] })).await));
        let session = phone.cookie.clone().unwrap();

        let mut req = format!("ws://127.0.0.1:{port}/api/ws").into_client_request().unwrap();
        req.headers_mut().insert("origin", url.parse().unwrap());
        req.headers_mut().insert("cookie", session.parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        let hello = loop {
            if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                break serde_json::from_str::<Value>(&t).unwrap();
            }
        };
        assert_eq!(hello["preview"], true);
        let mut ask = async |id: u64, port: u16| -> Value {
            ws.send(Message::Text(json!({ "t": "call", "id": id, "command": "preview_open", "args": { "port": port } }).to_string().into())).await.unwrap();
            loop {
                if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                    let v: Value = serde_json::from_str(&t).unwrap();
                    if v["id"] == id { return v }
                }
            }
        };
        assert!(ask(1, dev_port).await["error"].as_str().unwrap().contains("not a running dev server"), "a port the block does not show");
        hub.update(|s| {
            s.blocks.insert("dev_servers".into(), json!({ "servers": [{ "state": "running", "ports": [dev_port] }], "outside": [{ "port": 5432 }] }));
            true
        });
        assert!(ask(2, 5432).await["error"].is_string(), "a port held outside Prism");
        assert_eq!(ask(3, dev_port).await["ok"]["url"], format!("{porigin}/"));

        let get = |path: &str, cookie: Option<&str>| {
            let mut r = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap().get(format!("http://127.0.0.1:{pport}{path}")).header("host", format!("localhost:{pport}")).header("origin", &porigin).header("tailscale-user-login", "me@example.com");
            if let Some(c) = cookie {
                r = r.header("cookie", c);
            }
            r.send()
        };
        assert_eq!(get("/", None).await.unwrap().status(), 401, "no session, nothing forwarded");
        assert_eq!(get("/", Some("__Host-prism=made-up")).await.unwrap().status(), 401);
        let both = format!("{session}; app=1");
        let page = get("/", Some(&both)).await.unwrap();
        assert_eq!(page.status(), 200);
        let cookies: Vec<&str> = page.headers().get_all("set-cookie").iter().map(|v| v.to_str().unwrap()).collect();
        assert_eq!(cookies, ["app=1; Path=/"], "the dev server never sets Prism's cookie");
        assert_eq!(page.text().await.unwrap(), format!("host=localhost:{dev_port} origin=http://localhost:{dev_port} cookie=app=1 user="));
        let moved = get("/go", Some(&both)).await.unwrap();
        assert_eq!((moved.status().as_u16(), moved.headers()["location"].to_str().unwrap()), (302, "/there"));

        // Hot reload: the socket, its subprotocol, its messages.
        let mut hmr = format!("ws://127.0.0.1:{pport}/hmr").into_client_request().unwrap();
        hmr.headers_mut().insert("host", format!("localhost:{pport}").parse().unwrap());
        hmr.headers_mut().insert("origin", porigin.parse().unwrap());
        hmr.headers_mut().insert("cookie", session.parse().unwrap());
        hmr.headers_mut().insert("sec-websocket-protocol", "vite-hmr".parse().unwrap());
        let (mut hot, res) = tokio_tungstenite::connect_async(hmr).await.unwrap();
        assert_eq!(res.headers()["sec-websocket-protocol"], "vite-hmr");
        hot.send(Message::Text("ping".into())).await.unwrap();
        let echoed = loop {
            if let Message::Text(t) = hot.next().await.unwrap().unwrap() {
                break t.to_string();
            }
        };
        assert_eq!(echoed, "ping");

        // The server stopped: its port no longer forwarded.
        hub.update(|s| {
            s.blocks.insert("dev_servers".into(), json!({ "servers": [{ "state": "stopped", "ports": [dev_port] }] }));
            true
        });
        assert_eq!(get("/", Some(&both)).await.unwrap().status(), 404);
        remote.apply(&RemoteConfig::default()).await;
        let _ = std::fs::remove_dir_all(dir);
    }

    /// `prism pair`: the pairing opened, read and confirmed from a terminal of this machine, with
    /// the endpoint's secret only; the phone's side unchanged.
    #[tokio::test]
    async fn a_terminal_pairs_a_phone() {
        let dir = std::env::temp_dir().join(format!("prism-remote-pair-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let hub = Hub::new();
        let remote = Remote::new(&dir, hub.clone(), Arc::new(Fake::default())).unwrap();
        // The port grabbed then released can be taken by another test's bind in between (`apply`
        // keeps the error to itself): the relay file is the tell, tried until the server is up.
        let mut tries = 0;
        let (port, url) = loop {
            tries += 1;
            assert!(tries <= 10, "the remote server never started");
            let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
            let url = format!("http://localhost:{port}");
            remote.apply(&RemoteConfig { enabled: true, tunnel: RemoteTunnel::Other, port, url: url.clone(), keep_awake: false, ..Default::default() }).await;
            if crate::relay::read(&dir).is_some() {
                break (port, url);
            }
        };
        let phone = Phone { base: format!("http://127.0.0.1:{port}"), origin: url.clone(), http: reqwest::Client::new(), cookie: None };

        // Without the secret, nothing.
        let forged = phone.http.post(format!("{}/relay/pair/start", phone.base)).send().await.unwrap();
        assert_eq!(forged.status(), 403);

        let local = crate::pair::Local::find(&dir).unwrap();
        let pairing = local.start().await.unwrap();
        assert!(pairing.url.starts_with(&format!("{url}/pair#")) && pairing.code.is_none());
        assert!(crate::pair::qr_text(&pairing.url).unwrap().lines().count() > 10);
        let token = pairing.url.split_once('#').unwrap().1.to_string();
        let claim: Value = phone.post("/api/pair/claim", json!({ "token": token, "name": "Pixel 9" })).await.json().await.unwrap();
        let reg = Authenticator::new().create(claim["challenge"].as_str().unwrap(), &Party::of(&url).unwrap());
        let reg = json!({ "clientDataJson": reg.client_data_json, "attestationObject": reg.attestation_object });
        assert_eq!(phone.post("/api/pair/finish", json!({ "token": token, "registration": reg })).await.status(), 200);
        let shown = local.state().await.unwrap().unwrap();
        assert_eq!((shown.code.as_deref(), shown.device.as_deref(), shown.ready), (claim["code"].as_str(), Some("Pixel 9"), true));
        assert!(local.confirm("000000").await.is_err(), "another code than the phone's");
        local.confirm(shown.code.as_deref().unwrap()).await.unwrap();
        let paired = phone.post("/api/pair/state", json!({ "token": token, "claim": claim["challenge"] })).await;
        let session = cookie(&paired);
        assert!(session.starts_with("__Host-prism="));
        assert_eq!(hub.subscribe().borrow().remote.clone().unwrap().devices.len(), 1);

        // What Prism tells (a chain's "Notify me"): what a woken phone reads.
        remote.notify("Fix: its chain reached its end").await;
        let woke: Value = phone.http.get(format!("{}/api/wake", phone.base)).header("host", format!("localhost:{port}")).header("cookie", &session).send().await.unwrap().json().await.unwrap();
        assert_eq!((woke["text"].as_str(), woke["agent"].as_str()), (Some("Fix: its chain reached its end"), Some("")));

        // Cancelled: no pairing left.
        local.start().await.unwrap();
        local.cancel().await.unwrap();
        assert!(local.state().await.unwrap().is_none());
        remote.apply(&RemoteConfig::default()).await;
        assert!(crate::pair::Local::find(&dir).is_err(), "the server stopped, its endpoint gone");
    }

    /// Another Prism's window (`peers`, `machine`): its token, the snapshot then its changes, any
    /// command answered, a terminal typed in at its size; nothing without the token, nor once revoked.
    #[tokio::test]
    async fn a_peer_opens_this_prism() {
        use crate::machine::{Machine, Target};
        let dir = std::env::temp_dir().join(format!("prism-remote-peer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let hub = Hub::new();
        let fake = Fake::default();
        let typed = fake.typed.clone();
        let remote = Remote::new(&dir, hub.clone(), Arc::new(fake)).unwrap();
        let mut tries = 0;
        let port = loop {
            tries += 1;
            assert!(tries <= 10, "the remote server never started");
            let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
            remote.apply(&RemoteConfig { enabled: true, tunnel: RemoteTunnel::Other, port, url: format!("http://localhost:{port}"), keep_awake: false, ..Default::default() }).await;
            if crate::relay::read(&dir).is_some() {
                break port;
            }
        };
        let url = format!("http://127.0.0.1:{port}");

        // A token this Prism did not make: refused, and said so.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let stranger = Machine::open(Target { url: url.clone(), token: "forged".into(), profile: None, socket: None }, Arc::new(move |v| drop(tx.send(v))));
        let state = recv(&mut rx, "state").await;
        assert_eq!(state["connected"], false);
        assert!(state["error"].as_str().unwrap().contains("does not know this token"), "{state}");
        drop(stranger);

        let token = crate::peers::add(&dir, "mac").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let machine = Machine::open(Target { url: url.clone(), token: token.clone(), profile: None, socket: None }, Arc::new(move |v| drop(tx.send(v))));
        assert_eq!(recv(&mut rx, "state").await["connected"], true);
        assert_eq!(recv(&mut rx, "hello").await["peer"], "mac");
        let snap = recv(&mut rx, "snapshot").await;
        assert!(snap["value"].is_object());
        hub.update(|s| {
            s.me = Some("someone".into());
            true
        });
        let changes = recv(&mut rx, "changes").await;
        assert_eq!(changes["changes"][0]["path"], json!(["me"]), "{changes}");

        // Its summary, for the desk's status bar; nothing without the token.
        let summary = crate::machine::summary(&Target { url: url.clone(), token: token.clone(), profile: None, socket: None }).await.unwrap();
        assert_eq!((summary.agents, summary.waiting.len()), (0, 0));
        assert!(crate::machine::summary(&Target { url: url.clone(), token: "forged".into(), profile: None, socket: None }).await.unwrap_err().to_string().contains("token"));

        // Any command of the window, its arguments as sent; one it never answers, its error.
        assert_eq!(machine.call("save_config", json!({ "edits": [] })).await.unwrap(), json!({ "peer": "save_config", "args": { "edits": [] } }));
        // What it classes critical (its own class, not the desk's): the desk's confirmation first,
        // the purpose it gave, then the call goes through (here refused by the host itself).
        let crate::machine::Answer::Confirm(purpose) = machine.ask("mac_power", json!({ "action": "restart" }), None).await.unwrap() else { panic!("a confirmation first") };
        assert_eq!(purpose, super::purpose("mac_power", &json!({ "action": "restart" })));
        assert!(matches!(machine.ask("mac_power", json!({ "action": "restart" }), Some("another")).await.unwrap(), crate::machine::Answer::Confirm(_)), "another purpose confirms nothing");
        assert!(machine.ask("mac_power", json!({ "action": "restart" }), Some(&purpose)).await.unwrap_err().to_string().contains("not answered from another Prism"));

        // An agent's files: prepared there, fetched once here, whole.
        let offered = machine.call("agent_files_download", json!({ "id": "a1", "paths": ["src/"], "name": "work" })).await.unwrap();
        assert_eq!(offered["name"], "work.zip");
        let token = offered["token"].as_str().unwrap().to_string();
        let dest = dir.join("saved.zip");
        machine.download(&token, &dest).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"the zip");
        assert!(machine.download(&token, &dir.join("again.zip")).await.is_err(), "once");
        assert!(!dir.join("again.zip").exists() && !dir.join("again.zip.part").exists(), "nothing left of a refused one");

        // A terminal: its screen, then what is typed reaches the agent.
        let (dtx, mut drx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let tty = machine.attach("a1", 100, 30, Box::new(move |b| drop(dtx.send(b))), Box::new(|| {})).await.unwrap();
        assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(5), drx.recv()).await.unwrap().unwrap(), b"screen of a1");
        tty.write(b"ls\r").unwrap();
        tty.resize(90, 20).unwrap();
        for _ in 0..50 {
            if typed.lock().unwrap().as_slice() == b"ls\r" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(typed.lock().unwrap().as_slice(), b"ls\r");

        // Typing turned off: the open terminal closes at its next key.
        remote.apply(&RemoteConfig { enabled: true, tunnel: RemoteTunnel::Other, port, url: format!("http://localhost:{port}"), keep_awake: false, peers: prism_core::config::PeerRights { typing: false, ..Default::default() }, ..Default::default() }).await;
        assert!(machine.attach("a2", 100, 30, Box::new(|_| {}), Box::new(|| {})).await.is_err(), "no new terminal without typing");
        let before = typed.lock().unwrap().len();
        let _ = tty.write(b"x");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(typed.lock().unwrap().len(), before, "nothing typed once typing is off");

        // Revoked: no new socket opens with it, and the open one answers nothing more.
        assert!(crate::peers::remove(&dir, "mac").unwrap());
        assert!(machine.call("save_config", json!({})).await.is_err(), "a revoked peer's call");
        drop((tty, machine));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let _revoked = Machine::open(Target { url: url.clone(), token, profile: None, socket: None }, Arc::new(move |v| drop(tx.send(v))));
        assert!(recv(&mut rx, "state").await["error"].as_str().unwrap().contains("does not know this token"));
        remote.apply(&RemoteConfig::default()).await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A computer pairs by the code shown here: a wrong code refused, its name shown to the desk,
    /// accepted there, its token read by it alone and good to open a window.
    #[tokio::test]
    async fn a_computer_pairs_by_its_code() {
        let dir = std::env::temp_dir().join(format!("prism-remote-peer-pair-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let hub = Hub::new();
        let remote = Remote::new(&dir, hub.clone(), Arc::new(Fake::default())).unwrap();
        let mut tries = 0;
        let port = loop {
            tries += 1;
            assert!(tries <= 10, "the remote server never started");
            let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
            remote.apply(&RemoteConfig { enabled: true, tunnel: RemoteTunnel::Other, port, url: format!("http://localhost:{port}"), keep_awake: false, ..Default::default() }).await;
            if crate::relay::read(&dir).is_some() {
                break port;
            }
        };
        let url = format!("http://127.0.0.1:{port}");
        assert!(crate::machine::pair(&url, "000000", "Mac").await.is_err(), "no pairing open");
        // From the terminal, as `prism peer pair`.
        let local = crate::pair::Local::find(&dir).unwrap();
        let code = local.peer_start().await.unwrap().code;
        let wrong = if code == "111111" { "222222" } else { "111111" };
        assert!(crate::machine::pair(&url, wrong, "Mac").await.unwrap_err().to_string().contains("not the code"));
        let pairing = tokio::spawn({
            let (url, code) = (url.clone(), code.clone());
            async move { crate::machine::pair(&url, &code, "MacBook").await }
        });
        let mut asked = None;
        for _ in 0..100 {
            asked = local.peer_state().await.unwrap().and_then(|p| p.name);
            if asked.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }
        assert_eq!(asked.as_deref(), Some("MacBook"));
        local.peer_answer(true).await.unwrap();
        let token = tokio::time::timeout(std::time::Duration::from_secs(5), pairing).await.unwrap().unwrap().unwrap();
        assert_eq!(crate::peers::check(&dir, &token).as_deref(), Some("MacBook"));
        assert_eq!(hub.subscribe().borrow().remote.as_ref().unwrap().peers[0].name, "MacBook");
        assert!(local.peer_state().await.unwrap().is_none(), "over once read");

        // Tunnel tailscale: a peer with its token comes straight on loopback (an SSH tunnel's end),
        // without the tailnet's header; a pairing never does (Funnel, a tagged node).
        remote.inner.live().owner = Some("me@example.com".into());
        let http = reqwest::Client::new();
        let summary = http.get(format!("{url}/peer/summary")).bearer_auth(&token).send().await.unwrap();
        assert_eq!(summary.status(), 200);
        let pair = http.post(format!("{url}/peer/pair")).json(&json!({ "code": "123456", "name": "x", "claim": "a-secret-of-its-own-1234" })).send().await.unwrap();
        assert_eq!(pair.status(), 403, "a pairing without the tailnet's user");
        let other = http.post(format!("{url}/peer/pair")).header("tailscale-user-login", "someone@else.com").json(&json!({ "code": "123456", "name": "x", "claim": "a-secret-of-its-own-1234" })).send().await.unwrap();
        assert_eq!(other.status(), 403, "another tailnet user");
        remote.inner.live().owner = None;
        remote.apply(&RemoteConfig::default()).await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `[remote.peers]`: reads always; acting, critical, typing and settings each their switch.
    #[test]
    fn a_peer_does_what_its_rights_say() {
        let mut r = prism_core::config::PeerRights::default();
        assert!(super::peer_allowed(&r, "merge_mr", Some(Class::Critical)).is_ok());
        r = prism_core::config::PeerRights { actions: false, critical: false, typing: false, settings: false };
        assert!(super::peer_allowed(&r, "mr_detail", Some(Class::Read)).is_ok());
        assert!(super::peer_allowed(&r, "read_config", Some(Class::Never)).is_err(), "never for a phone: acting, unless read");
        assert!(super::peer_allowed(&r, "rebase_mr", Some(Class::Act)).is_err());
        assert!(super::peer_allowed(&r, "merge_mr", Some(Class::Critical)).is_err());
        assert!(super::peer_allowed(&r, "agent_type", Some(Class::Critical)).unwrap_err().to_string().contains("typing"));
        assert!(super::peer_allowed(&r, "save_config", Some(Class::Never)).unwrap_err().to_string().contains("settings"));
        // Acting allowed, the settings not: what writes them or trusts a command by another way.
        r.actions = true;
        r.critical = true;
        for command in ["mcp_proposal_answer", "routine_trust", "mcp_install", "cli_command_install", "quick_action_add"] {
            assert!(super::peer_allowed(&r, command, Some(Class::Never)).unwrap_err().to_string().contains("settings"), "{command}");
        }
        assert!(super::peer_allowed(&r, "block_call", Some(Class::Never)).unwrap_err().to_string().contains("settings"), "a custom block tried");
        assert!(super::peer_allowed(&r, "block_call", Some(Class::Act)).is_ok());
        assert!(super::peer_allowed(&r, "no_such_command", None).is_err());
    }

    /// The next message of kind `t` a machine sent.
    async fn recv(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Value>, t: &str) -> Value {
        loop {
            let v = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await.unwrap_or_else(|_| panic!("no {t}")).unwrap();
            if v["t"] == t {
                return v;
            }
        }
    }

    /// A profile of this computer, as its Prism without a window answers it (`Remote::serve_local`).
    #[cfg(unix)]
    struct ProfileHost(&'static str);

    #[cfg(unix)]
    impl Host for ProfileHost {
        fn call(&self, _command: String, _args: Value) -> futures::future::BoxFuture<'static, anyhow::Result<Value>> {
            Box::pin(async { anyhow::bail!("not a phone's") })
        }
        fn local_call(&self, command: String, _args: Value) -> futures::future::BoxFuture<'static, anyhow::Result<Value>> {
            let me = self.0;
            Box::pin(async move { Ok(json!({ "profile": me, "command": command })) })
        }
        fn notices(&self) -> Option<futures::stream::BoxStream<'static, crate::Notice>> {
            let n = crate::Notice { text: format!("{}: chain done", self.0), ms: 3000, system: true };
            Some(Box::pin(futures::stream::iter([n]).chain(futures::stream::pending())))
        }
        fn class(&self, command: &str, _args: &Value) -> Option<Class> {
            Some(if command == "merge_mr" { Class::Critical } else { Class::Read })
        }
        fn asset(&self, _path: &str) -> Option<(Vec<u8>, String)> {
            None
        }
        fn watch(&self, agent: String, size: Option<(u16, u16)>, _control: bool, on_data: Box<dyn Fn(Vec<u8>) + Send>, _on_end: Box<dyn FnOnce() + Send>) -> futures::future::BoxFuture<'static, anyhow::Result<crate::Watch>> {
            let me = self.0;
            Box::pin(async move {
                on_data(format!("{me}: screen of {agent}").into_bytes());
                let (cols, rows) = size.unwrap_or((120, 40));
                Ok(crate::Watch { cols, rows, term: Box::new(crate::ReadOnly(())) })
            })
        }
    }

    /// Two profiles of this computer, each its Prism without a window: the app's window of one
    /// reaches it through its own socket (private, no token, no confirmation), its snapshot,
    /// calls, notices and terminals; never the other's.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_profile_opens_in_a_window_of_the_app_through_its_socket() {
        use crate::machine::{Machine, Target};
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("prism-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let socket = |id: &str| root.join("engines").join(format!("{id}.sock"));
        let mut remotes = Vec::new();
        for id in ["work", "perso"] {
            let remote = Remote::of_profile(&root.join(id), id, Hub::new(), Arc::new(ProfileHost(id))).unwrap();
            remote.serve_local(&socket(id)).await.unwrap();
            remotes.push(remote);
        }
        // The owner's only; one Prism per socket.
        assert_eq!(std::fs::metadata(socket("perso")).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(root.join("engines")).unwrap().permissions().mode() & 0o777, 0o700);
        assert!(remotes[1].serve_local(&socket("perso")).await.unwrap_err().to_string().contains("another Prism listens there"));

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let perso = Machine::open(Target::local(socket("perso")), Arc::new(move |v| drop(tx.send(v))));
        assert_eq!(recv(&mut rx, "state").await["connected"], true);
        assert_eq!(recv(&mut rx, "hello").await["profile"], "perso");
        // Its snapshot, and a chain's notice for the window's toast and notification, in either order.
        let (mut snapshot, mut notice) = (None, None);
        while snapshot.is_none() || notice.is_none() {
            let v = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await.expect("the snapshot and the notice").unwrap();
            match v["t"].as_str() {
                Some("snapshot") => snapshot = Some(v),
                Some("notice") => notice = Some(v),
                _ => {}
            }
        }
        assert!(snapshot.unwrap()["value"].is_object());
        let notice = notice.unwrap();
        assert_eq!((notice["text"].as_str(), notice["system"].as_bool()), (Some("perso: chain done"), Some(true)));
        // What it classes critical: answered at once, as in its own window (no Touch ID of the peers').
        let crate::machine::Answer::Value(merged) = perso.ask("merge_mr", json!({ "iid": 1 }), None).await.unwrap() else { panic!("no confirmation asked") };
        assert_eq!(merged, json!({ "profile": "perso", "command": "merge_mr" }));
        assert_eq!(crate::machine::summary(&Target::local(socket("perso"))).await.unwrap().agents, 0);
        let (dtx, mut drx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let _tty = perso.attach("a1", 100, 30, Box::new(move |b| drop(dtx.send(b))), Box::new(|| {})).await.unwrap();
        assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(5), drx.recv()).await.unwrap().unwrap(), b"perso: screen of a1");

        // The other profile's socket, its own Prism: nothing of the first one.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let work = Machine::open(Target::local(socket("work")), Arc::new(move |v| drop(tx.send(v))));
        assert_eq!(recv(&mut rx, "hello").await["profile"], "work");
        assert_eq!(work.call("agent_list", json!({})).await.unwrap()["profile"], "work");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn a_phone_pairs_logs_in_and_only_reads() {
        let dir = std::env::temp_dir().join(format!("prism-remote-server-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let hub = Hub::new();
        let fake = Fake::default();
        let typed = fake.typed.clone();
        let remote = Remote::new(&dir, hub.clone(), Arc::new(fake)).unwrap();
        // Same tell as `a_terminal_pairs_a_phone`'s: retried until the server has its port.
        let mut tries = 0;
        let (port, url) = loop {
            tries += 1;
            assert!(tries <= 10, "the remote server never started");
            let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
            let url = format!("http://localhost:{port}");
            remote.apply(&RemoteConfig { enabled: true, tunnel: RemoteTunnel::Other, port, url: url.clone(), keep_awake: false, ..Default::default() }).await;
            if crate::relay::read(&dir).is_some() {
                break (port, url);
            }
        };
        let cfg = RemoteConfig { enabled: true, tunnel: RemoteTunnel::Other, port, url: url.clone(), keep_awake: false, ..Default::default() };
        let status = hub.subscribe().borrow().remote.clone().unwrap();
        assert_eq!((status.url.as_deref(), status.error), (Some(url.as_str()), None));
        let mut phone = Phone { base: format!("http://127.0.0.1:{port}"), origin: url.clone(), http: reqwest::Client::new(), cookie: None };

        // The page, under the expected host only.
        let page = phone.http.get(format!("{}/", phone.base)).header("host", format!("localhost:{port}")).send().await.unwrap();
        assert_eq!(page.status(), 200);
        assert!(page.headers()["content-security-policy"].to_str().unwrap().contains("frame-ancestors 'none'"));
        assert_eq!(page.text().await.unwrap(), "<html>the page");
        let sw = phone.http.get(format!("{}/remote/sw.js", phone.base)).header("host", format!("localhost:{port}")).send().await.unwrap();
        assert_eq!(sw.headers()["service-worker-allowed"], "/", "the service worker serves the whole page");
        let other = phone.http.get(format!("{}/dev-snapshot.json", phone.base)).header("host", format!("localhost:{port}")).send().await.unwrap();
        assert_eq!(other.status(), 404, "only the page's own files");
        let rebound = phone.http.get(format!("{}/", phone.base)).header("host", "evil.example").send().await.unwrap();
        assert_eq!(rebound.status(), 421);
        // A change from another origin.
        let forged = phone.http.post(format!("{}/api/login/begin", phone.base)).header("origin", "https://evil.example").send().await.unwrap();
        assert_eq!(forged.status(), 403);
        // No socket without a session.
        let mut req = format!("ws://127.0.0.1:{port}/api/ws").into_client_request().unwrap();
        req.headers_mut().insert("origin", url.parse().unwrap());
        assert!(tokio_tungstenite::connect_async(req.clone()).await.is_err());

        // Pairing: the desk opens it, the phone claims it and creates its passkey, the desk confirms.
        remote.pair_start().unwrap();
        let link = hub.subscribe().borrow().remote.clone().unwrap().pairing.unwrap().url;
        let token = link.split_once('#').unwrap().1.to_string();
        let claim: Value = phone.post("/api/pair/claim", json!({ "token": token, "name": "Pixel 9" })).await.json().await.unwrap();
        let authenticator = Authenticator::new();
        let party = Party::of(&url).unwrap();
        let reg = authenticator.create(claim["challenge"].as_str().unwrap(), &party);
        let reg = json!({ "clientDataJson": reg.client_data_json, "attestationObject": reg.attestation_object });
        assert_eq!(phone.post("/api/pair/finish", json!({ "token": token, "registration": reg })).await.status(), 200);
        let waiting: Value = phone.post("/api/pair/state", json!({ "token": token, "claim": claim["challenge"] })).await.json().await.unwrap();
        assert_eq!(waiting["state"], "waiting");
        let pairing = hub.subscribe().borrow().remote.clone().unwrap().pairing.unwrap();
        assert_eq!((pairing.code.as_deref(), pairing.ready), (claim["code"].as_str(), true));
        remote.pair_confirm(None).unwrap();
        let snooper: Value = phone.post("/api/pair/state", json!({ "token": token })).await.json().await.unwrap();
        assert_eq!(snooper["state"], "gone", "the QR code alone picks no session up");
        let paired = phone.post("/api/pair/state", json!({ "token": token, "claim": claim["challenge"] })).await;
        assert!(paired.headers()["set-cookie"].to_str().unwrap().contains("HttpOnly"));
        phone.cookie = Some(cookie(&paired));

        // Its socket: hello, the snapshot, a read answered, a merge refused.
        req.headers_mut().insert("cookie", phone.cookie.clone().unwrap().parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req.clone()).await.unwrap();
        let mut next = async || -> Value { loop { if let Message::Text(t) = ws.next().await.unwrap().unwrap() { return serde_json::from_str(&t).unwrap() } } };
        let hello = next().await;
        assert_eq!((hello["t"].as_str(), hello["device"].as_str()), (Some("hello"), Some("Pixel 9")));
        let snap = next().await;
        assert_eq!(snap["t"], "snapshot");
        assert!(snap["value"].get("remote").is_none(), "the pairing stays on the computer");
        ws.send(Message::Text(json!({ "t": "call", "id": 1, "command": "mr_detail", "args": {} }).to_string().into())).await.unwrap();
        ws.send(Message::Text(json!({ "t": "call", "id": 2, "command": "merge_mr", "args": {} }).to_string().into())).await.unwrap();
        let mut results = std::collections::BTreeMap::new();
        while results.len() < 2 {
            if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["t"] == "result" {
                    results.insert(v["id"].as_u64().unwrap(), v);
                }
            }
        }
        assert_eq!(results[&1]["ok"], json!({ "echo": "mr_detail" }));
        assert!(results[&2]["error"].as_str().unwrap().contains("critical actions from a phone are off"));
        // An agent's files: an address, fetched once over HTTP by this phone only, the file then gone.
        ws.send(Message::Text(json!({ "t": "call", "id": 9, "command": "agent_files_download", "args": {} }).to_string().into())).await.unwrap();
        let offered = loop {
            if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["id"] == 9 {
                    break v["ok"].clone();
                }
            }
        };
        assert_eq!(offered["name"], "work é.zip");
        assert!(offered.get("path").is_none(), "where it lies stays on the computer");
        let address = format!("{}{}", phone.base, offered["url"].as_str().unwrap());
        let get = |cookie: Option<&str>| {
            let mut r = phone.http.get(&address).header("host", format!("localhost:{port}"));
            if let Some(c) = cookie {
                r = r.header("cookie", c);
            }
            r.send()
        };
        assert_eq!(get(None).await.unwrap().status(), 401);
        let fetched = get(phone.cookie.as_deref()).await.unwrap();
        assert_eq!(fetched.headers()["content-disposition"], "attachment; filename*=UTF-8''work%20%C3%A9.zip");
        assert_eq!(fetched.bytes().await.unwrap().as_ref(), b"the zip");
        assert_eq!(get(phone.cookie.as_deref()).await.unwrap().status(), 404, "once");
        assert!(!std::env::temp_dir().join(format!("prism-remote-download-{}", std::process::id())).exists());
        // An agent's pages: an address under which its folder is served, sandboxed, cookie or not.
        ws.send(Message::Text(json!({ "t": "call", "id": 10, "command": "agent_page_open", "args": { "id": "a1", "scratchpad": true } }).to_string().into())).await.unwrap();
        let opened = loop {
            if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["id"] == 10 {
                    break v["ok"].clone();
                }
            }
        };
        let base = opened["url"].as_str().unwrap().to_string();
        assert!(base.starts_with("/page/") && base.ends_with('/'));
        let page = |path: String| phone.http.get(format!("{}{path}", phone.base)).header("host", format!("localhost:{port}")).send();
        let served = page(format!("{base}docs/a%20b.html")).await.unwrap();
        assert_eq!(served.status(), 200);
        let csp = served.headers()["content-security-policy"].to_str().unwrap().to_string();
        assert!(csp.starts_with("sandbox allow-scripts") && !csp.contains("allow-same-origin") && csp.contains(&format!("connect-src http://localhost:{port}{base}")), "{csp}");
        assert!(served.headers().get("x-frame-options").is_none(), "framed by the remote's page");
        assert_eq!(served.text().await.unwrap(), "page /a1/@scratchpad/docs/a%20b.html");
        assert_eq!(page("/page/nope/x.html".into()).await.unwrap().status(), 404);
        // Acting: refused while [remote] actions is off, answered once on.
        ws.send(Message::Text(json!({ "t": "call", "id": 3, "command": "rebase_mr", "args": {} }).to_string().into())).await.unwrap();
        let refused = loop {
            if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["id"] == 3 { break v }
            }
        };
        assert!(refused["error"].as_str().unwrap().contains("actions from a phone are off"));
        remote.apply(&RemoteConfig { actions: true, ..cfg.clone() }).await;
        ws.send(Message::Text(json!({ "t": "call", "id": 4, "command": "rebase_mr", "args": {} }).to_string().into())).await.unwrap();
        let done = loop {
            if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["id"] == 4 { break v }
            }
        };
        assert_eq!(done["ok"], json!({ "echo": "rebase_mr" }));

        // Critical: a step-up first, then the very call with the passkey's answer.
        remote.apply(&RemoteConfig { actions: true, critical: true, typing: true, ..cfg.clone() }).await;
        let ask = async |ws: &mut tokio_tungstenite::WebSocketStream<_>, id: u64, command: &str, args: Value, assertion: Option<Value>| -> Value {
            let mut msg = json!({ "t": "call", "id": id, "command": command, "args": args });
            if let Some(a) = assertion { msg["assertion"] = a }
            ws.send(Message::Text(msg.to_string().into())).await.unwrap();
            loop {
                if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                    let v: Value = serde_json::from_str(&t).unwrap();
                    if v["id"] == id { return v }
                }
            }
        };
        let answer = |step: &Value| {
            let a = authenticator.get(step["stepUp"]["challenge"].as_str().unwrap(), &party);
            json!({ "credentialId": a.credential_id, "clientDataJson": a.client_data_json, "authenticatorData": a.authenticator_data, "signature": a.signature })
        };
        let step = ask(&mut ws, 5, "merge_mr", json!({ "iid": 218 }), None).await;
        assert!(step["stepUp"]["challenge"].is_string(), "{step}");
        // Its answer does not confirm another merge.
        let wrong = ask(&mut ws, 6, "merge_mr", json!({ "iid": 999 }), Some(answer(&step))).await;
        assert!(wrong["error"].as_str().unwrap().contains("something else"), "{wrong}");
        let step = ask(&mut ws, 7, "merge_mr", json!({ "iid": 218 }), None).await;
        let merged = ask(&mut ws, 8, "merge_mr", json!({ "iid": 218 }), Some(answer(&step))).await;
        assert_eq!(merged["ok"], json!({ "echo": "merge_mr" }));

        // A text typed to an agent: its passkey once, then nothing more for a while; its terminal
        // taken after it asks nothing either (the same confirmation).
        let step = ask(&mut ws, 30, "agent_type", json!({ "id": "a2", "text": "go" }), None).await;
        assert!(step["stepUp"]["challenge"].is_string(), "{step}");
        let typed_a2 = ask(&mut ws, 31, "agent_type", json!({ "id": "a2", "text": "go" }), Some(answer(&step))).await;
        assert_eq!(typed_a2["ok"], json!({ "echo": "agent_type" }));
        assert_eq!(ask(&mut ws, 32, "agent_type", json!({ "id": "a2", "text": "and a test" }), None).await["ok"], json!({ "echo": "agent_type" }));
        assert!(ask(&mut ws, 33, "remote_control", json!({ "agent": "a2" }), None).await["error"].is_null());
        assert!(ask(&mut ws, 34, "agent_type", json!({ "id": "a3", "text": "go" }), None).await["stepUp"].is_object(), "another agent asks again");
        // A key in one of its shells: typing, the passkey of that shell; in the agent, a menu key (Act).
        assert!(ask(&mut ws, 36, "agent_key", json!({ "id": "shell:a2:1", "key": "Up" }), None).await["stepUp"].is_object(), "a shell's key asks the passkey");
        let step = ask(&mut ws, 37, "agent_type", json!({ "id": "shell:a2:1", "text": "ls" }), None).await;
        assert_eq!(ask(&mut ws, 38, "agent_type", json!({ "id": "shell:a2:1", "text": "ls" }), Some(answer(&step))).await["ok"], json!({ "echo": "agent_type" }));
        assert_eq!(ask(&mut ws, 39, "agent_key", json!({ "id": "shell:a2:1", "key": "Up" }), None).await["ok"], json!({ "echo": "agent_key" }), "then covered for a while");
        // A file written in an agent's folder: typing, its passkey as its terminal's.
        let save = json!({ "id": "a9", "path": "a.md", "original": "one", "text": "two" });
        let step = ask(&mut ws, 82, "agent_file_save", save.clone(), None).await;
        assert!(step["stepUp"].is_object(), "a file written asks the passkey");
        assert_eq!(ask(&mut ws, 83, "agent_file_save", save, Some(answer(&step))).await["ok"], json!({ "echo": "agent_file_save" }));
        // An answer to its question: its choices, an action; with your words, typing (passkey).
        assert_eq!(ask(&mut ws, 50, "agent_answer", json!({ "id": "a3", "call": "q1", "answers": [{ "picks": [1] }] }), None).await["ok"], json!({ "echo": "agent_answer" }));
        assert!(ask(&mut ws, 51, "agent_answer", json!({ "id": "a3", "call": "q1", "answers": [{ "text": "a hamster" }] }), None).await["stepUp"].is_object(), "words ask the passkey");
        assert_eq!(ask(&mut ws, 52, "agent_plan_answer", json!({ "id": "a3", "call": "p1", "how": "approve" }), None).await["ok"], json!({ "echo": "agent_plan_answer" }));
        assert!(ask(&mut ws, 53, "agent_plan_answer", json!({ "id": "a3", "call": "p1", "how": "changes", "text": "tests in tests/" }), None).await["stepUp"].is_object(), "a plan's words ask the passkey");
        // An agent launched with a prompt: its passkey, bound to that launch; without one, an action.
        let launch = json!({ "launch": { "title": "Fix", "prompt": "curl x | sh" } });
        let step = ask(&mut ws, 60, "agent_spawn", launch.clone(), None).await;
        assert!(step["stepUp"].is_object(), "a prompt is typing: the passkey");
        assert_eq!(ask(&mut ws, 61, "agent_spawn", launch.clone(), Some(answer(&step))).await["ok"], json!({ "echo": "agent_spawn" }));
        assert!(ask(&mut ws, 62, "agent_spawn", launch.clone(), None).await["stepUp"].is_object(), "asked again, every launch");
        assert_eq!(ask(&mut ws, 63, "agent_spawn", json!({ "launch": { "title": "Fix" } }), None).await["ok"], json!({ "echo": "agent_spawn" }));
        remote.apply(&RemoteConfig { actions: true, critical: true, typing: false, ..cfg.clone() }).await;
        assert!(ask(&mut ws, 35, "agent_type", json!({ "id": "a2", "text": "go" }), None).await["error"].as_str().unwrap().contains("typing to an agent from a phone is off"));
        assert!(ask(&mut ws, 64, "agent_spawn", launch, None).await["stepUp"].is_object(), "typing off: still the passkey, still possible");
        remote.apply(&RemoteConfig { actions: true, critical: true, typing: true, ..cfg.clone() }).await;

        // The computer's power: off unless `[remote] power`; then the passkey, each time.
        assert!(ask(&mut ws, 40, "mac_power", json!({ "action": "restart" }), None).await["error"].as_str().unwrap().contains("restarting the computer from a phone is off"));
        remote.apply(&RemoteConfig { actions: true, critical: true, typing: true, power: true, ..cfg.clone() }).await;
        let step = ask(&mut ws, 41, "mac_power", json!({ "action": "restart" }), None).await;
        let restarted = ask(&mut ws, 42, "mac_power", json!({ "action": "restart" }), Some(answer(&step))).await;
        assert_eq!(restarted["ok"], json!({ "echo": "mac_power" }));
        assert!(ask(&mut ws, 43, "mac_power", json!({ "action": "restart" }), None).await["stepUp"].is_object(), "asked again, every time");
        remote.apply(&RemoteConfig { actions: true, critical: true, typing: true, settings: false, ..cfg.clone() }).await;

        // The settings: off unless `[remote] settings`; read then, saved with the passkey each time,
        // even with `[remote] critical` off.
        let edits = json!({ "edits": [{ "path": ["alerts", "quiet_hours"], "value": "19:00-08:00" }] });
        assert!(ask(&mut ws, 44, "remote_settings_read", json!({}), None).await["error"].as_str().unwrap().contains("settings from a phone are off"));
        remote.apply(&RemoteConfig { actions: true, critical: false, typing: true, settings: true, ..cfg.clone() }).await;
        assert_eq!(ask(&mut ws, 45, "remote_settings_read", json!({}), None).await["ok"], json!({ "echo": "remote_settings_read" }));
        let step = ask(&mut ws, 46, "remote_settings_save", edits.clone(), None).await;
        assert!(step["stepUp"].is_object(), "a save asks the passkey");
        assert_eq!(ask(&mut ws, 47, "remote_settings_save", edits.clone(), Some(answer(&step))).await["ok"], json!({ "echo": "remote_settings_save" }));
        assert!(ask(&mut ws, 48, "remote_settings_save", edits, None).await["stepUp"].is_object(), "asked again, every time");

        // A new project: off unless `[remote] projects`; then the passkey each time, even with
        // `[remote] critical` off.
        let project = json!({ "name": "fresh", "remote": null });
        remote.apply(&RemoteConfig { actions: true, critical: false, projects: false, ..cfg.clone() }).await;
        assert!(ask(&mut ws, 70, "remote_project_create", project.clone(), None).await["error"].as_str().unwrap().contains("projects from a phone are off"));
        assert!(ask(&mut ws, 74, "remote_project_publish", json!({ "name": "fresh", "remote": { "visibility": "private" } }), None).await["error"].as_str().unwrap().contains("projects from a phone are off"));
        assert!(ask(&mut ws, 77, "remote_project_search", json!({ "workspace": "", "query": "x" }), None).await["error"].as_str().unwrap().contains("projects from a phone are off"));
        assert!(ask(&mut ws, 78, "remote_project_clone", json!({ "workspace": "", "project": "g/p" }), None).await["error"].as_str().unwrap().contains("projects from a phone are off"));
        remote.apply(&RemoteConfig { actions: true, critical: false, projects: true, ..cfg.clone() }).await;
        let step = ask(&mut ws, 71, "remote_project_create", project.clone(), None).await;
        assert!(step["stepUp"].is_object(), "a new project asks the passkey");
        assert_eq!(ask(&mut ws, 72, "remote_project_create", project.clone(), Some(answer(&step))).await["ok"], json!({ "echo": "remote_project_create" }));
        assert!(ask(&mut ws, 73, "remote_project_create", project, None).await["stepUp"].is_object(), "asked again, every time");
        assert_eq!(ask(&mut ws, 79, "remote_project_search", json!({ "workspace": "", "query": "x" }), None).await["ok"], json!({ "echo": "remote_project_search" }), "a search reads");
        let clone = json!({ "workspace": "", "project": "g/p" });
        let step = ask(&mut ws, 80, "remote_project_clone", clone.clone(), None).await;
        assert!(step["stepUp"].is_object(), "a clone asks the passkey");
        assert_eq!(ask(&mut ws, 81, "remote_project_clone", clone, Some(answer(&step))).await["ok"], json!({ "echo": "remote_project_clone" }));
        let publish = json!({ "name": "fresh", "remote": { "namespace": "", "visibility": "private" } });
        let step = ask(&mut ws, 75, "remote_project_publish", publish.clone(), None).await;
        assert!(step["stepUp"].is_object(), "a publish asks the passkey");
        assert_eq!(ask(&mut ws, 76, "remote_project_publish", publish, Some(answer(&step))).await["ok"], json!({ "echo": "remote_project_publish" }));
        remote.apply(&RemoteConfig { actions: true, critical: true, typing: true, ..cfg.clone() }).await;

        // Taking a terminal: refused before its step-up, then typed in, then taken back by the desk.
        let tty_req = |control: bool| {
            let mut r = format!("ws://127.0.0.1:{port}/api/tty?agent=a1{}", if control { "&control=true&cols=50&rows=20" } else { "" }).into_client_request().unwrap();
            r.headers_mut().insert("origin", url.parse().unwrap());
            r.headers_mut().insert("cookie", phone.cookie.clone().unwrap().parse().unwrap());
            r
        };
        assert!(tokio_tungstenite::connect_async(tty_req(true)).await.is_err(), "not confirmed yet");
        let step = ask(&mut ws, 9, "remote_control", json!({ "agent": "a1" }), None).await;
        assert!(ask(&mut ws, 10, "remote_control", json!({ "agent": "a1" }), Some(answer(&step))).await["error"].is_null());
        let (mut ctl, _) = tokio_tungstenite::connect_async(tty_req(true)).await.unwrap();
        let Message::Text(size) = ctl.next().await.unwrap().unwrap() else { panic!("no size") };
        assert_eq!(serde_json::from_str::<Value>(&size).unwrap()["cols"], 50);
        ctl.send(Message::Binary(b"ls\r".to_vec().into())).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(&typed.lock().unwrap()[..], b"ls\r");
        assert_eq!(hub.subscribe().borrow().remote.clone().unwrap().controls.len(), 1);
        remote.release("a1");
        let closed = loop {
            match ctl.next().await {
                Some(Ok(Message::Close(f))) => break f.map(|f| u16::from(f.code)),
                Some(Ok(_)) => continue,
                _ => break None,
            }
        };
        assert_eq!(closed, Some(4409));
        assert!(hub.subscribe().borrow().remote.clone().unwrap().controls.is_empty());
        assert!(hub.subscribe().borrow().remote.clone().unwrap().devices[0].connected);

        // An agent's terminal: its size, then its screen.
        let mut treq = format!("ws://127.0.0.1:{port}/api/tty?agent=a1").into_client_request().unwrap();
        treq.headers_mut().insert("origin", url.parse().unwrap());
        treq.headers_mut().insert("cookie", phone.cookie.clone().unwrap().parse().unwrap());
        let (mut tty, _) = tokio_tungstenite::connect_async(treq).await.unwrap();
        let Message::Text(size) = tty.next().await.unwrap().unwrap() else { panic!("no size") };
        assert_eq!(serde_json::from_str::<Value>(&size).unwrap()["cols"], 120);
        let Message::Binary(screen) = tty.next().await.unwrap().unwrap() else { panic!("no screen") };
        assert_eq!(&screen[..], b"screen of a1");

        // Revoked: its session no longer opens a socket.
        let device = hub.subscribe().borrow().remote.clone().unwrap().devices[0].id.clone();
        remote.revoke(&device).unwrap();
        assert!(tokio_tungstenite::connect_async(req).await.is_err());
        // And the socket it had open closes, even if it only watches.
        let closed = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                match ws.next().await {
                    Some(Ok(Message::Close(frame))) => return frame.map(|f| u16::from(f.code)),
                    Some(Ok(_)) => continue,
                    _ => return None,
                }
            }
        });
        assert_eq!(closed.await.unwrap(), Some(4401));

        remote.apply(&RemoteConfig::default()).await;
        assert!(hub.subscribe().borrow().remote.is_none());
        assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_ok(), "stopped, the port is free again");
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The remote on the demo's data, to look at its page in a browser: `cargo test -p prism-remote
/// remote_preview -- --ignored --nocapture`, after `pnpm --dir web build`. A phone is paired by
/// itself; the session's cookie to set in the browser is printed.
#[cfg(test)]
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn remote_preview() {
    use std::sync::Arc;

    use prism_core::{config::{RemoteConfig, RemoteTunnel}, state::Hub};

    use crate::webauthn::{tests::Authenticator, Party};

    struct Preview(std::path::PathBuf);
    impl crate::Host for Preview {
        fn call(&self, command: String, args: Value) -> futures::future::BoxFuture<'static, anyhow::Result<Value>> {
            Box::pin(async move {
                let id = args["id"].as_str().unwrap_or_default();
                match command.as_str() {
                    "agent_conversation" => Ok(serde_json::to_value(prism_demo::details::conversation(id))?),
                    "agent_answer" => {
                        println!("preview: answers {id}: {}", args["answers"]);
                        Ok(Value::Null)
                    }
                    "agent_plan_answer" => {
                        println!("preview: plan of {id}: {} {}", args["how"], args["text"]);
                        Ok(Value::Null)
                    }
                    "agent_quick_answer" => {
                        println!("preview: quick answer to {id}: {} ({})", args["label"], args["text"]);
                        Ok(Value::Null)
                    }
                    // What scrolled off a Claude Code screen, its frame and spinner included.
                    "agent_history" => Ok(Value::String((1..=40).map(|i| format!("\x1b[32m⏺\x1b[0m \x1b[1mBash\x1b[0m(npm test -- part {i})\n  ⎿  {i} passed\n")).collect::<String>() + "╭────────────────────────╮\n│ > round per line       │\n╰────────────────────────╯\n\x1b[33m✻ Whirring… (34s · esc to interrupt)\x1b[0m\n")),
                    _ => anyhow::bail!("preview: no computer behind {command}"),
                }
            })
        }
        fn class(&self, command: &str, _args: &Value) -> Option<Class> {
            Some(match command {
                "agent_key" | "agent_interrupt" | "agent_allow" | "agent_answer" | "agent_plan_answer" | "agent_quick_answer" | "rebase_mr" | "retry_job" => Class::Act,
                c if c.starts_with("agent_") || c.starts_with("mr_") || c == "get_ticket" => Class::Read,
                _ => Class::Never,
            })
        }
        fn asset(&self, path: &str) -> Option<(Vec<u8>, String)> {
            let mime = match path.rsplit('.').next().unwrap_or_default() {
                "html" => "text/html; charset=utf-8",
                "js" => "text/javascript",
                "css" => "text/css",
                "svg" => "image/svg+xml",
                "png" => "image/png",
                "webmanifest" => "application/manifest+json",
                "woff2" => "font/woff2",
                _ => "application/octet-stream",
            };
            std::fs::read(self.0.join(path)).ok().map(|b| (b, mime.to_string()))
        }
        fn watch(&self, agent: String, size: Option<(u16, u16)>, _control: bool, on_data: Box<dyn Fn(Vec<u8>) + Send>, _on_end: Box<dyn FnOnce() + Send>) -> futures::future::BoxFuture<'static, anyhow::Result<crate::Watch>> {
            Box::pin(async move {
                on_data(format!("\x1b[1;32m● {agent}\x1b[0m: the preview has no tmux; a computer shows the agent's terminal here.\r\n\r\n\x1b[32m⏺\x1b[0m \x1b[1mBash\x1b[0m(npm run test:invoices -- --verbose)\r\n\x1b[33m✻ Whirring… (12s · esc to interrupt)\x1b[0m\r\n╭──────────────────────╮\r\n│ >                    │\r\n╰──────────────────────╯\r\n  ⏵⏵ accept edits on\r\n").into_bytes());
                let (cols, rows) = size.unwrap_or((100, 30));
                Ok(crate::Watch { cols, rows, term: Box::new(crate::ReadOnly(())) })
            })
        }
    }

    prism_core::blocks::register(prism_blocks::builtin().to_vec());
    let hub = Hub::new();
    prism_demo::fill(&hub, Utc::now());
    let port = std::env::var("PRISM_PREVIEW_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(47801);
    let dir = std::env::temp_dir().join("prism-remote-preview");
    let _ = std::fs::remove_dir_all(&dir);
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../web/dist");
    let remote = crate::Remote::new(&dir, hub.clone(), Arc::new(Preview(dist))).unwrap();
    let url = format!("http://localhost:{port}");
    remote.apply(&RemoteConfig { enabled: true, tunnel: RemoteTunnel::Other, port, url: url.clone(), keep_awake: false, actions: true, ..Default::default() }).await;
    remote.pair_start().unwrap();
    let link = hub.subscribe().borrow().remote.clone().unwrap().pairing.unwrap().url;
    let token = link.split_once('#').unwrap().1.to_string();
    let http = reqwest::Client::new();
    let post = |path: &str, body: Value| http.post(format!("http://127.0.0.1:{port}{path}")).header("origin", &url).header("host", format!("localhost:{port}")).json(&body).send();
    let claim: Value = post("/api/pair/claim", json!({ "token": token, "name": "Preview phone" })).await.unwrap().json().await.unwrap();
    let reg = Authenticator::new().create(claim["challenge"].as_str().unwrap(), &Party::of(&url).unwrap());
    post("/api/pair/finish", json!({ "token": token, "registration": { "clientDataJson": reg.client_data_json, "attestationObject": reg.attestation_object } })).await.unwrap();
    remote.pair_confirm(None).unwrap();
    let paired = post("/api/pair/state", json!({ "token": token, "claim": claim["challenge"] })).await.unwrap();
    let cookie = paired.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_string();
    println!("\nremote preview: {url}\nin the browser's console: document.cookie = '{cookie}; path=/; secure'\n");
    let minutes = std::env::var("PRISM_PREVIEW_MINUTES").ok().and_then(|m| m.parse().ok()).unwrap_or(30);
    tokio::time::sleep(std::time::Duration::from_secs(60 * minutes)).await;
}
