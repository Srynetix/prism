//! Several profiles, one phone. Each profile is its own Prism (its agents, tmux, config); the
//! one whose `[remote]` has the tunnel (`tailscale` or `other`) is the hub: the phone pairs with
//! it, logs in to it, and reaches the others through it, at `/p/<profile>/`. A profile with
//! `tunnel = "relay"` listens on loopback only, on a free port, and tells it, with a secret, in
//! its data folder (`remote/endpoint.json`, the owner's only); the hub reads it and forwards the
//! phone's requests and sockets there, the secret and the phone's id added.
//!
//! The hub does all the checking: the session's cookie, the tailnet user, the origin, and the
//! passkey. A relayed Prism has no phones nor passkeys of its own; it keeps its own `[remote]`
//! rights (actions, critical, typing…) and its own step-ups' purposes. Its confirmations go
//! through the hub: where it would ask the phone's passkey, it answers `stepUp` with the purpose
//! only; the hub makes it a challenge of the phone's passkey, checks the answer (its credential
//! store, its relying party), and sends the call again with `confirmed` set to that purpose,
//! which the relayed Prism trusts since only the hub, holding its secret, reaches it. A
//! `confirmed` a phone would send itself is dropped by the hub. So a phone's passkey is checked in
//! one place, against one store, whichever profile asks.
//!
//! Without the hub's Prism open, no profile is reached from the phone: a relayed one has no
//! tunnel. (Any open Prism taking the hub's place when it closes is left for later: the phones'
//! pairing and passkeys would have to be shared.)

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade}, FromRequestParts, Path as UrlPath, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use prism_base::{i18n::tr, tr};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

use crate::{lock, server, webauthn::Assertion, Inner};

type Shared = Arc<Inner>;

/// A Prism's remote, as the others find it: its loopback port, the secret a request to it
/// carries, and whether it is the hub (with the address phones open).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Endpoint {
    pub port: u16,
    pub secret: String,
    pub hub: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

const FILE: &str = "endpoint.json";
/// The secret of the Prism a request goes to (the hub's on `/relay/`, the relayed one's otherwise).
pub(crate) const SECRET: &str = "x-prism-relay";
/// The phone the hub let in: its id, and its name (percent-encoded).
pub(crate) const DEVICE: &str = "x-prism-device";
pub(crate) const DEVICE_NAME: &str = "x-prism-device-name";
/// The address phones open the hub at: an agent's page is framed by it only.
pub(crate) const ORIGIN: &str = "x-prism-origin";

/// An agent that started waiting, here or in a relayed profile: what a woken phone says.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Wake {
    pub profile: String,
    pub agent: String,
    pub title: String,
    /// Not an agent waiting but something to tell (`Remote::notify`): the notification's text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default = "Utc::now")]
    pub at: DateTime<Utc>,
}

/// A woken phone asks what woke it this long after, at most.
const WAKE_KEPT: chrono::Duration = chrono::Duration::minutes(15);

/// The profiles' data folders' root: this one's folder for the default profile, the folder two
/// levels up for another (`<root>/profiles/<id>`), as `prism_core::profile` lays them out.
fn data_base(inner: &Inner) -> PathBuf {
    if inner.profile == prism_core::profile::DEFAULT {
        inner.dir.clone()
    } else {
        inner.dir.parent().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default()
    }
}

fn dir_of(inner: &Inner, id: &str) -> PathBuf {
    let base = data_base(inner);
    if id == prism_core::profile::DEFAULT { base } else { base.join("profiles").join(id) }
}

/// This Prism's endpoint, told: written the owner's only, whole or not at all.
pub(crate) fn write(inner: &Inner, e: &Endpoint) -> Result<()> {
    let dir = inner.dir.join("remote");
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    let mut open = std::fs::OpenOptions::new();
    open.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o600);
    }
    std::io::Write::write_all(&mut open.open(&tmp)?, &serde_json::to_vec(e)?)?;
    std::fs::rename(&tmp, dir.join(FILE)).context("remote/endpoint.json")
}

/// This Prism's endpoint, no longer: its server stopped.
pub(crate) fn remove(inner: &Inner) {
    let _ = std::fs::remove_file(inner.dir.join("remote").join(FILE));
}

pub(crate) fn read(dir: &Path) -> Option<Endpoint> {
    serde_json::from_slice(&std::fs::read(dir.join("remote").join(FILE)).ok()?).ok()
}

/// The ids of this machine's profiles with a data folder, this one's left out.
fn others(inner: &Inner) -> Vec<String> {
    let base = data_base(inner);
    let listed = std::fs::read_dir(base.join("profiles")).into_iter().flatten().flatten().filter_map(|e| e.file_name().to_str().map(String::from));
    std::iter::once(prism_core::profile::DEFAULT.to_string()).chain(listed).filter(|id| *id != inner.profile && (id == prism_core::profile::DEFAULT || prism_core::profile::valid_id(id))).collect()
}

/// The relayed profile `id`'s endpoint, if its Prism told one (open, or left by a crash: the
/// connection says).
pub(crate) fn relayed(inner: &Inner, id: &str) -> Option<Endpoint> {
    if id == inner.profile || !(id == prism_core::profile::DEFAULT || prism_core::profile::valid_id(id)) {
        return None;
    }
    read(&dir_of(inner, id)).filter(|e| !e.hub)
}

/// The hub's endpoint, from a relayed Prism: the other profile's whose remote has the tunnel.
pub(crate) fn hub(inner: &Inner) -> Option<Endpoint> {
    others(inner).into_iter().find_map(|id| read(&dir_of(inner, &id)).filter(|e| e.hub))
}

/// Two secrets the same, in a time that does not tell how much of them matches.
pub(crate) fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A request carries this Prism's secret.
pub(crate) fn trusted(inner: &Inner, headers: &HeaderMap) -> bool {
    let theirs = headers.get(SECRET).and_then(|h| h.to_str().ok()).unwrap_or_default();
    lock(&inner.secret).as_deref().is_some_and(|mine| !theirs.is_empty() && same(mine, theirs))
}

/// The phone's name, as the hub sent it to a relayed Prism, or as this one knows it.
pub(crate) fn device_name(inner: &Inner, headers: &HeaderMap, device: &str) -> String {
    if inner.live().relay {
        let raw = headers.get(DEVICE_NAME).and_then(|h| h.to_str().ok()).unwrap_or_default();
        return url::form_urlencoded::parse(format!("n={raw}").as_bytes()).find(|(k, _)| k == "n").map(|(_, v)| v.into_owned()).unwrap_or_default();
    }
    inner.auth().devices().iter().find(|d| d.id == device).map(|d| d.name.clone()).unwrap_or_default()
}

pub(crate) fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).no_proxy().build().expect("an HTTP client"))
}

// The hub's side.

/// What a relayed Prism says of itself (`/relay/summary`): its agents, those waiting, its dev
/// servers' ports (the preview forwards to one of them only).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Summary {
    pub agents: usize,
    pub waiting: usize,
    #[serde(default)]
    pub dev_ports: Vec<u16>,
}

/// A relayed profile's summary, if its Prism answers.
async fn summary(e: &Endpoint) -> Option<Summary> {
    let res = client().get(format!("http://127.0.0.1:{}/relay/summary", e.port)).header(SECRET, &e.secret).timeout(Duration::from_millis(1500)).send().await.ok()?;
    res.status().is_success().then_some(())?;
    res.json().await.ok()
}

/// A relayed profile's summary, read again at most every few seconds (the preview asks at each
/// request of the app it shows).
pub(crate) async fn summary_cached(inner: &Inner, id: &str) -> Option<Summary> {
    if let Some((at, s)) = lock(&inner.summaries).get(id)
        && at.elapsed() < Duration::from_secs(5)
    {
        return s.clone();
    }
    let s = match relayed(inner, id) {
        Some(e) => summary(&e).await,
        None => None,
    };
    lock(&inner.summaries).insert(id.to_string(), (Instant::now(), s.clone()));
    s
}

fn counts(snap: &prism_core::state::Snapshot) -> Summary {
    use prism_core::state::{AgentState, SessionStatus::*};
    let running: Vec<_> = snap.agents.iter().filter(|a| a.state == AgentState::Running).collect();
    Summary { agents: running.len(), waiting: running.iter().filter(|a| matches!(a.status, WaitingTool | NeedsYou | ReviewReady)).count(), dev_ports: crate::preview::dev_ports(snap) }
}

/// `GET /api/profiles`: this machine's profiles, for the phone's switcher: which this hub is,
/// which answer here (relayed), with their agents and those waiting; the others, closed or with
/// their remote off.
pub(crate) async fn profiles(State(inner): State<Shared>, headers: HeaderMap) -> Response {
    if server::device_of(&inner, &headers).is_none() {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": tr("no session") }))).into_response();
    }
    let mine = counts(&inner.hub.subscribe().borrow());
    let mut out = Vec::new();
    for p in inner.host.profiles().await {
        let (reachable, s) = if p.id == inner.profile {
            (true, Some(mine.clone()))
        } else {
            match relayed(&inner, &p.id) {
                Some(e) => {
                    let s = summary(&e).await;
                    (s.is_some(), s)
                }
                None => (false, None),
            }
        };
        let s = s.unwrap_or_default();
        out.push(json!({ "id": p.id, "name": p.name, "color": p.color, "hub": p.id == inner.profile, "open": p.id == inner.profile || p.open, "reachable": reachable, "agents": if reachable { s.agents as u32 } else { p.agents }, "waiting": s.waiting }));
    }
    Json(json!({ "own": inner.profile, "profiles": out })).into_response()
}

#[derive(Deserialize)]
pub(crate) struct OpenProfile {
    id: String,
    #[serde(default)]
    assertion: Option<Assertion>,
}

/// `POST /api/profiles/open`: a closed profile opened in its own Prism beside the hub's, from the
/// phone (`[remote] actions`), its passkey each time: first the challenge, then its answer.
pub(crate) async fn open_profile(State(inner): State<Shared>, headers: HeaderMap, Json(o): Json<OpenProfile>) -> Response {
    let refuse = |status: StatusCode, why: String| (status, Json(json!({ "error": why }))).into_response();
    let Some(device) = server::device_of(&inner, &headers) else { return refuse(StatusCode::UNAUTHORIZED, tr("no session").into()) };
    if !inner.live().actions {
        return refuse(StatusCode::FORBIDDEN, tr("actions from a phone are off (Settings › Remote › Exposure)").into());
    }
    if o.id == inner.profile || !inner.host.profiles().await.iter().any(|p| p.id == o.id) {
        return refuse(StatusCode::NOT_FOUND, tr!("unknown profile {id}", id = o.id));
    }
    let purpose = server::purpose("remote_profile_open", &json!({ "id": o.id }));
    match server::step_up(&inner, &device, &purpose, server::Proof::Phone(o.assertion)) {
        Ok(Some(server::Reply::StepUp(s))) => return Json(json!({ "stepUp": s })).into_response(),
        Ok(_) => {}
        Err(e) => return refuse(StatusCode::FORBIDDEN, format!("{e:#}")),
    }
    tracing::info!("remote: opens the profile {} beside this Prism (confirmed by its passkey)", o.id);
    match inner.host.open_profile(&o.id) {
        Ok(()) => Json(json!({})).into_response(),
        Err(e) => refuse(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

/// `GET /api/wake`: what woke this phone (its service worker asks, the push carrying nothing):
/// the agent and its profile, while recent.
pub(crate) async fn last_wake(State(inner): State<Shared>, headers: HeaderMap) -> Response {
    if server::device_of(&inner, &headers).is_none() {
        return (StatusCode::UNAUTHORIZED, Json(json!({}))).into_response();
    }
    let wake = lock(&inner.last_wake).clone().filter(|w| Utc::now() - w.at < WAKE_KEPT);
    let Some(w) = wake else { return Json(json!({})).into_response() };
    let profiles = inner.host.profiles().await;
    let name = profiles.iter().find(|p| p.id == w.profile).map(|p| p.name.clone());
    Json(json!({ "profile": w.profile, "name": if profiles.len() > 1 { name } else { None }, "own": w.profile == inner.profile, "agent": w.agent, "title": w.title, "text": w.text })).into_response()
}

/// `POST /relay/wake`, from a relayed Prism (its secret checked by `guard`): one of its agents
/// started waiting.
pub(crate) async fn wake_from(State(inner): State<Shared>, Json(w): Json<Wake>) -> Response {
    if relayed(&inner, &w.profile).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    inner.wake(Wake { at: Utc::now(), ..w }).await;
    Json(json!({})).into_response()
}

/// From a relayed Prism: one of its agents started waiting, told to the hub, which wakes the phones.
pub(crate) async fn tell_hub(inner: &Inner, w: &Wake) {
    let Some(hub) = hub(inner) else { return };
    let sent = client().post(format!("http://127.0.0.1:{}/relay/wake", hub.port)).header(SECRET, &hub.secret).json(w).timeout(Duration::from_secs(5)).send().await;
    if let Err(e) = sent {
        tracing::warn!("remote: telling the relay an agent waits: {e}");
    }
}

/// `GET /relay/summary` on a relayed Prism.
pub(crate) async fn own_summary(State(inner): State<Shared>) -> Response {
    let s = counts(&inner.hub.subscribe().borrow());
    Json(s).into_response()
}

/// The headers that belong to one connection, never forwarded.
const HOP: [&str; 9] = ["connection", "keep-alive", "proxy-connection", "proxy-authenticate", "proxy-authorization", "te", "trailer", "transfer-encoding", "upgrade"];
/// Largest request body forwarded.
const MAX_BODY: usize = 8 << 20;

/// The phone's headers as the relayed Prism gets them: its cookie, the tailnet's identity and
/// anything of ours left out; the secret, the phone and the hub's origin added.
fn upstream(inner: &Inner, e: &Endpoint, headers: &HeaderMap, device: &str) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (k, v) in headers {
        let name = k.as_str();
        if HOP.contains(&name) || matches!(name, "host" | "cookie" | "origin" | "referer" | "content-length") || name.starts_with("tailscale-") || name.starts_with("sec-websocket-") || name.starts_with("x-prism-") {
            continue;
        }
        out.append(k.clone(), v.clone());
    }
    let name: String = url::form_urlencoded::byte_serialize(device_name(inner, headers, device).as_bytes()).collect();
    let origin = inner.live().party.as_ref().map(|p| p.origin.clone()).unwrap_or_default();
    for (k, v) in [(SECRET, e.secret.as_str()), (DEVICE, device), (DEVICE_NAME, name.as_str()), (ORIGIN, origin.as_str())] {
        if let Ok(v) = HeaderValue::from_str(v) {
            out.insert(k, v);
        }
    }
    if let Ok(h) = HeaderValue::from_str(&format!("127.0.0.1:{}", e.port)) {
        out.insert(header::HOST, h);
    }
    out
}

fn refuse(status: StatusCode, why: &str) -> Response {
    (status, Json(json!({ "error": why }))).into_response()
}

/// `/p/<profile>/<rest>`: a relayed profile's page requests and sockets, forwarded to its Prism
/// once the phone's session is checked here.
pub(crate) async fn proxy(State(inner): State<Shared>, UrlPath((id, rest)): UrlPath<(String, String)>, req: Request) -> Response {
    let headers = req.headers();
    let (Some(token), Some(device)) = (server::cookie_of(headers), server::device_of(&inner, headers)) else { return refuse(StatusCode::UNAUTHORIZED, tr("no session")) };
    let Some(e) = relayed(&inner, &id) else { return refuse(StatusCode::NOT_FOUND, tr("this profile’s Prism is closed, or its remote is not relayed here")) };
    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    let path = format!("/{rest}{query}");
    let websocket = headers.get(header::UPGRADE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let up = upstream(&inner, &e, headers, &device);
    if !websocket {
        return forward(&e, path, up, req).await;
    }
    let (mut parts, _) = req.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    let Ok(mut request) = format!("ws://127.0.0.1:{}{path}", e.port).into_client_request() else { return refuse(StatusCode::BAD_REQUEST, "bad address") };
    for (k, v) in up {
        if let Some(k) = k {
            request.headers_mut().insert(k, v);
        }
    }
    let theirs = match tokio_tungstenite::connect_async(request).await {
        Ok((s, _)) => s,
        Err(err) => {
            tracing::debug!("remote: relay to {id}: {err}");
            return refuse(StatusCode::BAD_GATEWAY, tr("this profile’s Prism does not answer"));
        }
    };
    let main = rest == "api/ws";
    upgrade.max_message_size(4 << 20).on_upgrade(move |ours| pipe(inner, ours, theirs, Phone { id, device, token, main }))
}

async fn forward(e: &Endpoint, path: String, headers: HeaderMap, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let body = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(b) => b,
        Err(_) => return refuse(StatusCode::PAYLOAD_TOO_LARGE, "too large"),
    };
    let res = match client().request(parts.method, format!("http://127.0.0.1:{}{path}", e.port)).headers(headers).body(body).send().await {
        Ok(r) => r,
        Err(_) => return refuse(StatusCode::BAD_GATEWAY, tr("this profile’s Prism does not answer")),
    };
    let status = res.status();
    let mut h = HeaderMap::new();
    for (k, v) in res.headers() {
        if !HOP.contains(&k.as_str()) && k != header::SET_COOKIE {
            h.append(k.clone(), v.clone());
        }
    }
    let mut out = Response::new(Body::from_stream(res.bytes_stream()));
    *out.status_mut() = status;
    *out.headers_mut() = h;
    out
}

type Upstream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Whose socket is relayed: the profile, the phone, its session, and whether it is the page's
/// own (`/api/ws`, its calls looked into) or a terminal's (passed as is).
struct Phone {
    id: String,
    device: String,
    token: String,
    main: bool,
}

/// What becomes of a phone's message on its way to the relayed Prism.
enum Up {
    Send(String),
    /// Answered here: never forwarded.
    Answer(Value),
    Drop,
}

/// A phone's call, looked into: a `confirmed` of its own dropped; the passkey's answer to a
/// confirmation checked here and replaced by the purpose it confirms; the phone's push
/// subscription (the hub's), and the dev servers' preview opened, remembered to rewrite the answer.
fn up(inner: &Inner, p: &Phone, text: &str, previews: &mut HashSet<u64>) -> Up {
    let Ok(Value::Object(mut v)) = serde_json::from_str::<Value>(text) else { return Up::Drop };
    v.remove("confirmed");
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    let error = |e: String| Up::Answer(json!({ "t": "result", "id": id, "error": e }));
    match v.get("command").and_then(Value::as_str) {
        Some("remote_push") => {
            if !inner.live().push {
                return error("notifications are off (Settings › Remote › Sessions)".into());
            }
            let endpoint = v.get("args").and_then(|a| a.get("endpoint")).and_then(Value::as_str).map(String::from);
            return match inner.auth().set_push(&p.device, endpoint) {
                Ok(()) => Up::Answer(json!({ "t": "result", "id": id, "ok": null })),
                Err(e) => error(format!("{e:#}")),
            };
        }
        Some("preview_open") => {
            if let Some(n) = id.as_u64() {
                previews.insert(n);
            }
        }
        _ => {}
    }
    if let Some(a) = v.remove("assertion").filter(|a| !a.is_null()) {
        let Ok(a) = serde_json::from_value::<Assertion>(a) else { return error("a passkey's answer".into()) };
        let Some(party) = inner.live().party.clone() else { return error(tr("starting").into()) };
        let mut auth = inner.auth();
        let Some(purpose) = auth.step_up_purpose(&p.device, &a) else { return error(tr("an unknown or expired confirmation").into()) };
        if let Err(e) = auth.step_up_finish(&p.device, &purpose, &a, &party, Utc::now()) {
            return error(format!("{e:#}"));
        }
        tracing::info!("remote: confirmed by its passkey for the profile {}: {purpose}", p.id);
        v.insert("confirmed".into(), Value::String(purpose));
    }
    Up::Send(Value::Object(v).to_string())
}

/// The relayed Prism's message on its way to the phone: a step-up's purpose made the phone's
/// passkey challenge; the preview's port made the hub's preview address; its hello telling the
/// hub's preview and push.
fn down(inner: &Inner, p: &Phone, text: &str, previews: &mut HashSet<u64>) -> String {
    // Looked at closer only when it may be one of those: a string's own quotes are escaped.
    let result = text.contains(r#""t":"result""#);
    if !text.contains(r#""t":"hello""#) && !(result && (text.contains(r#""stepUp""#) || !previews.is_empty())) {
        return text.to_string();
    }
    let Ok(mut v) = serde_json::from_str::<Value>(text) else { return text.to_string() };
    if v["t"] == "hello" {
        let live = inner.live();
        v["preview"] = Value::Bool(live.preview_origin.is_some());
        v["push"] = if live.push { Value::String(inner.vapid.public()) } else { Value::Null };
        return v.to_string();
    }
    if v["t"] == "result" && let Some(purpose) = v["stepUp"]["purpose"].as_str().map(String::from) {
        let party = inner.live().party.clone();
        let begun = party.context("starting").and_then(|party| inner.auth().step_up_begin(&p.device, &purpose, Utc::now()).map(|(c, allow)| (c, allow, party)));
        match begun {
            Ok((challenge, allow, party)) => v["stepUp"] = json!({ "challenge": challenge, "rpId": party.id, "allow": allow }),
            Err(e) => v = json!({ "t": "result", "id": v["id"], "error": format!("{e:#}") }),
        }
        return v.to_string();
    }
    if let Some(n) = v["id"].as_u64()
        && v["t"] == "result"
        && previews.remove(&n)
        && let Some(port) = v["ok"]["previewPort"].as_u64().and_then(|p| u16::try_from(p).ok())
    {
        match inner.live().preview_origin.clone() {
            Some(origin) => {
                *lock(&inner.relayed_preview) = Some((p.id.clone(), port));
                *inner.preview() = Some(port);
                tracing::info!("remote: previews the dev server on port {port} of the profile {}", p.id);
                v["ok"] = json!({ "url": format!("{origin}/") });
            }
            None => v = json!({ "t": "result", "id": n, "error": tr("the dev servers' preview is off (Settings › Remote › Exposure)") }),
        }
    }
    v.to_string()
}

fn to_axum(m: tungstenite::Message) -> Option<Message> {
    Some(match m {
        tungstenite::Message::Text(t) => Message::Text(t.as_str().into()),
        tungstenite::Message::Binary(b) => Message::Binary(b),
        tungstenite::Message::Ping(b) => Message::Ping(b),
        tungstenite::Message::Pong(b) => Message::Pong(b),
        tungstenite::Message::Close(f) => Message::Close(f.map(|f| CloseFrame { code: f.code.into(), reason: f.reason.as_str().into() })),
        tungstenite::Message::Frame(_) => return None,
    })
}

/// A phone's socket and the relayed Prism's, joined; the page's own looked into (`up`, `down`).
/// The phone's session is checked at each of its messages and every few seconds, as the hub's
/// own socket does: ended, both close.
async fn pipe(inner: Shared, ours: WebSocket, theirs: Upstream, p: Phone) {
    if p.main {
        inner.connect(&p.device, true);
    }
    let (mut ours_tx, mut ours_rx) = ours.split();
    let (mut theirs_tx, mut theirs_rx) = theirs.split();
    let (to_phone, mut queue) = tokio::sync::mpsc::channel::<Message>(64);
    let writer = tokio::spawn(async move {
        while let Some(m) = queue.recv().await {
            if ours_tx.send(m).await.is_err() {
                break;
            }
        }
        let _ = ours_tx.close().await;
    });
    let ended = || Message::Close(Some(CloseFrame { code: 4401, reason: "session ended".into() }));
    let mut previews = HashSet::new();
    let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + Duration::from_secs(10), Duration::from_secs(10));
    loop {
        tokio::select! {
            m = ours_rx.next() => match m {
                Some(Ok(Message::Text(t))) => {
                    if inner.auth().check(&p.token, Utc::now()).is_none() {
                        let _ = to_phone.send(ended()).await;
                        break;
                    }
                    let sent = if p.main {
                        match up(&inner, &p, t.as_str(), &mut previews) {
                            Up::Send(s) => theirs_tx.send(tungstenite::Message::Text(s.into())).await.is_ok(),
                            Up::Answer(v) => to_phone.send(Message::Text(v.to_string().into())).await.is_ok(),
                            Up::Drop => true,
                        }
                    } else {
                        theirs_tx.send(tungstenite::Message::Text(t.as_str().into())).await.is_ok()
                    };
                    if !sent { break }
                }
                Some(Ok(Message::Binary(b))) => if theirs_tx.send(tungstenite::Message::Binary(b)).await.is_err() { break },
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            m = theirs_rx.next() => match m {
                Some(Ok(tungstenite::Message::Text(t))) => {
                    let t = if p.main { down(&inner, &p, t.as_str(), &mut previews) } else { t.as_str().to_string() };
                    if to_phone.send(Message::Text(t.into())).await.is_err() { break }
                }
                Some(Ok(m @ tungstenite::Message::Close(_))) => {
                    if let Some(m) = to_axum(m) { let _ = to_phone.send(m).await; }
                    break;
                }
                Some(Ok(m)) => if let Some(m) = to_axum(m) && to_phone.send(m).await.is_err() { break },
                Some(Err(_)) | None => break,
            },
            _ = tick.tick() => {
                if !inner.auth().alive(&p.token, Utc::now()) {
                    let _ = to_phone.send(ended()).await;
                    break;
                }
            }
        }
    }
    let _ = theirs_tx.close().await;
    drop(to_phone);
    let _ = writer.await;
    if p.main {
        inner.connect(&p.device, false);
    }
}

/// The dev server's port the preview may forward to: one of the hub's own, or the one a relayed
/// profile opened, while that profile still runs it.
pub(crate) async fn preview_allowed(inner: &Inner, port: u16) -> bool {
    if crate::preview::dev_ports(&inner.hub.subscribe().borrow()).contains(&port) {
        return true;
    }
    let opened = lock(&inner.relayed_preview).clone();
    match opened {
        Some((id, p)) if p == port => summary_cached(inner, &id).await.is_some_and(|s| s.dev_ports.contains(&port)),
        _ => false,
    }
}

/// Phones woken at most once a minute each.
pub(crate) const PUSH_EVERY: Duration = Duration::from_secs(60);

pub(crate) type Pushed = HashMap<String, Instant>;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::{SinkExt, StreamExt};
    use prism_core::{config::{RemoteConfig, RemoteTunnel}, remote::Class, state::Hub};
    use serde_json::{json, Value};
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

    use crate::{webauthn::{tests::Authenticator, Party}, Host, Remote};

    /// A profile's Prism: its commands echoed with its name.
    struct Fake(&'static str);

    impl Host for Fake {
        fn call(&self, command: String, _args: Value) -> futures::future::BoxFuture<'static, anyhow::Result<Value>> {
            let me = self.0;
            Box::pin(async move { Ok(json!({ "by": me, "command": command })) })
        }
        fn class(&self, command: &str, _args: &Value) -> Option<Class> {
            match command {
                "mr_detail" => Some(Class::Read),
                "merge_mr" => Some(Class::Critical),
                _ => None,
            }
        }
        fn asset(&self, _: &str) -> Option<(Vec<u8>, String)> {
            None
        }
        fn watch(&self, _: String, _: Option<(u16, u16)>, _: bool, _: Box<dyn Fn(Vec<u8>) + Send>, _: Box<dyn FnOnce() + Send>) -> futures::future::BoxFuture<'static, anyhow::Result<crate::Watch>> {
            Box::pin(async { anyhow::bail!("none") })
        }
        fn profiles(&self) -> futures::future::BoxFuture<'static, Vec<prism_core::profile::Profile>> {
            let p = |id: &str, name: &str| prism_core::profile::Profile { id: id.into(), name: name.into(), color: "pink".into(), config: String::new(), active: false, packs: Vec::new(), agents: 0, open: true, error: None };
            let list = vec![p("default", "Work"), p("perso", "Perso"), p("closed", "Closed")];
            Box::pin(async move { list })
        }
    }

    type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

    async fn next(ws: &mut Socket, t: &str) -> Value {
        loop {
            if let Message::Text(m) = ws.next().await.unwrap().unwrap() {
                let v: Value = serde_json::from_str(&m).unwrap();
                if v["t"] == t {
                    return v;
                }
            }
        }
    }

    async fn call(ws: &mut Socket, id: u64, command: &str, extra: Value) -> Value {
        let mut m = json!({ "t": "call", "id": id, "command": command, "args": { "iid": 7 } });
        m.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        ws.send(Message::Text(m.to_string().into())).await.unwrap();
        loop {
            let v = next(ws, "result").await;
            if v["id"] == id {
                return v;
            }
        }
    }

    fn answer(a: crate::webauthn::Assertion) -> Value {
        json!({ "credentialId": a.credential_id, "clientDataJson": a.client_data_json, "authenticatorData": a.authenticator_data, "signature": a.signature })
    }

    /// A phone paired with the hub reaches a relayed profile through it: its calls answered there,
    /// its critical ones confirmed by its passkey checked by the hub, a `confirmed` of its own
    /// worth nothing; the relayed Prism refuses whoever lacks its secret; the hub lists it.
    #[tokio::test]
    async fn a_phone_reaches_a_relayed_profile_through_the_hub() {
        let base = std::env::temp_dir().join(format!("prism-relay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let url = format!("http://localhost:{port}");
        let hub = Hub::new();
        let work = Remote::of_profile(&base, "default", hub.clone(), Arc::new(Fake("work"))).unwrap();
        work.apply(&RemoteConfig { enabled: true, tunnel: RemoteTunnel::Other, port, url: url.clone(), keep_awake: false, preview: false, ..Default::default() }).await;
        let perso = Remote::of_profile(&base.join("profiles/perso"), "perso", Hub::new(), Arc::new(Fake("perso"))).unwrap();
        perso.apply(&RemoteConfig { enabled: true, tunnel: RemoteTunnel::Relay, actions: true, critical: true, keep_awake: false, ..Default::default() }).await;
        let told = super::read(&base.join("profiles/perso")).expect("the relayed Prism tells its endpoint");
        assert!(!told.hub && told.port != port);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(base.join("profiles/perso/remote/endpoint.json")).unwrap().permissions().mode() & 0o777, 0o600);
        }

        // Without the secret, the relayed Prism answers nothing.
        let http = reqwest::Client::new();
        assert_eq!(http.get(format!("http://127.0.0.1:{}/relay/summary", told.port)).send().await.unwrap().status(), 403);
        let mut req = format!("ws://127.0.0.1:{}/api/ws", told.port).into_client_request().unwrap();
        req.headers_mut().insert(super::DEVICE, "someone".parse().unwrap());
        assert!(tokio_tungstenite::connect_async(req).await.is_err(), "a socket without the secret");

        // A phone paired with the hub.
        let post = |path: &str, body: Value| http.post(format!("http://127.0.0.1:{port}{path}")).header("origin", &url).json(&body).send();
        work.pair_start().unwrap();
        let link = hub.subscribe().borrow().remote.clone().unwrap().pairing.unwrap().url;
        let token = link.split_once('#').unwrap().1.to_string();
        let claim: Value = post("/api/pair/claim", json!({ "token": token, "name": "Pixel été" })).await.unwrap().json().await.unwrap();
        let party = Party::of(&url).unwrap();
        let phone = Authenticator::new();
        let reg = phone.create(claim["challenge"].as_str().unwrap(), &party);
        post("/api/pair/finish", json!({ "token": token, "registration": { "clientDataJson": reg.client_data_json, "attestationObject": reg.attestation_object } })).await.unwrap();
        work.pair_confirm(None).unwrap();
        let res = post("/api/pair/state", json!({ "token": token, "claim": claim["challenge"] })).await.unwrap();
        let cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap().split(';').next().unwrap().to_string();

        // The switcher's list: the hub, the relayed profile answering, a closed one.
        let list: Value = http.get(format!("http://127.0.0.1:{port}/api/profiles")).header("cookie", &cookie).send().await.unwrap().json().await.unwrap();
        assert_eq!(list["own"], "default");
        let reach: Vec<(String, bool)> = list["profiles"].as_array().unwrap().iter().map(|p| (p["id"].as_str().unwrap().to_string(), p["reachable"].as_bool().unwrap())).collect();
        assert_eq!(reach, [("default".into(), true), ("perso".into(), true), ("closed".into(), false)]);
        assert_eq!(http.get(format!("http://127.0.0.1:{port}/api/profiles")).send().await.unwrap().status(), 401);

        // Its socket, at the hub, to the relayed profile.
        let mut req = format!("ws://127.0.0.1:{port}/p/perso/api/ws").into_client_request().unwrap();
        req.headers_mut().insert("origin", url.parse().unwrap());
        req.headers_mut().insert("cookie", cookie.parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        let hello = next(&mut ws, "hello").await;
        assert_eq!(hello["device"], "Pixel été", "the phone's name, as the hub knows it");
        assert_eq!(hello["classes"], json!(["read", "act", "critical"]), "the relayed profile's own rights");
        assert_eq!(call(&mut ws, 1, "mr_detail", json!({})).await["ok"]["by"], "perso");

        // Critical: the hub's passkey challenge, then the call again with its answer.
        let purpose = crate::server::purpose("merge_mr", &json!({ "iid": 7 }));
        let forged = call(&mut ws, 2, "merge_mr", json!({ "confirmed": purpose })).await;
        let step = forged["stepUp"].clone();
        assert!(step["challenge"].is_string() && step["purpose"].is_null(), "a confirmation of the phone's own counts for nothing: {forged}");
        assert_eq!(step["rpId"], "localhost");
        let done = call(&mut ws, 3, "merge_mr", json!({ "assertion": answer(phone.get(step["challenge"].as_str().unwrap(), &party)) })).await;
        assert_eq!(done["ok"]["by"], "perso", "confirmed: {done}");
        // Another's passkey: refused by the hub.
        let again = call(&mut ws, 4, "merge_mr", json!({})).await;
        let other = Authenticator { id: b"other".to_vec(), key: p256::ecdsa::SigningKey::from_bytes(&[3u8; 32].into()).unwrap(), ..Authenticator::new() };
        let stranger = answer(other.get(again["stepUp"]["challenge"].as_str().unwrap(), &party));
        let refused = call(&mut ws, 5, "merge_mr", json!({ "assertion": stranger })).await;
        assert!(refused["error"].is_string(), "{refused}");

        // A relayed agent waiting, told to the hub: what the woken phone reads.
        let hub_secret = super::read(&base).unwrap().secret;
        let woke = http.post(format!("http://127.0.0.1:{port}/relay/wake")).header(super::SECRET, &hub_secret).json(&json!({ "profile": "perso", "agent": "a1", "title": "Synth" })).send().await.unwrap();
        assert_eq!(woke.status(), 200);
        let forged = http.post(format!("http://127.0.0.1:{port}/relay/wake")).header(super::SECRET, "nope").json(&json!({ "profile": "perso", "agent": "a2", "title": "x" })).send().await.unwrap();
        assert_eq!(forged.status(), 403);
        let wake: Value = http.get(format!("http://127.0.0.1:{port}/api/wake")).header("cookie", &cookie).send().await.unwrap().json().await.unwrap();
        assert_eq!((wake["profile"].as_str(), wake["name"].as_str(), wake["agent"].as_str()), (Some("perso"), Some("Perso"), Some("a1")));

        // The relayed Prism stopped: its endpoint goes, the hub no longer reaches it.
        drop(ws);
        perso.apply(&RemoteConfig::default()).await;
        assert!(super::read(&base.join("profiles/perso")).is_none());
        let gone = http.get(format!("http://127.0.0.1:{port}/p/perso/api/download/x")).header("cookie", &cookie).send().await.unwrap();
        assert_eq!(gone.status(), 404);
        work.apply(&RemoteConfig::default()).await;
        let _ = std::fs::remove_dir_all(base);
    }

    /// The hub reaches a profile by its id only: never itself, never a folder outside the
    /// profiles' (`..`, a path), never another hub; a relayed Prism finds the hub among the others.
    #[test]
    fn a_profile_is_reached_by_its_id_only() {
        let base = std::env::temp_dir().join(format!("prism-relay-ids-{}", std::process::id()));
        let work = Remote::of_profile(&base, "default", Hub::new(), Arc::new(Fake("work"))).unwrap();
        let perso = Remote::of_profile(&base.join("profiles/perso"), "perso", Hub::new(), Arc::new(Fake("perso"))).unwrap();
        let endpoint = |port, hub| super::Endpoint { port, secret: format!("s{port}"), hub, url: None };
        super::write(&work.inner, &endpoint(1, true)).unwrap();
        super::write(&perso.inner, &endpoint(2, false)).unwrap();
        // Endpoints a crafted id would point at: next to the profiles, and above them.
        for dir in [base.join("escape"), base.join("profiles/escape-x/../../above")] {
            std::fs::create_dir_all(dir.join("remote")).unwrap();
            std::fs::write(dir.join("remote/endpoint.json"), serde_json::to_vec(&endpoint(3, false)).unwrap()).unwrap();
        }

        assert_eq!(super::relayed(&work.inner, "perso").map(|e| e.port), Some(2));
        for id in ["default", "../escape", "../above", "../../escape", "profiles/../escape", "Perso", "", "-x"] {
            assert!(super::relayed(&work.inner, id).is_none(), "{id:?}");
        }
        assert!(super::relayed(&perso.inner, "perso").is_none(), "not itself");
        assert!(super::relayed(&perso.inner, "default").is_none(), "the hub is not relayed");
        assert_eq!(super::hub(&perso.inner).map(|e| e.port), Some(1));
        assert!(super::hub(&work.inner).is_none(), "the hub has no hub");
        let _ = std::fs::remove_dir_all(base);
    }

    /// What reaches the relayed Prism: the phone's request without its cookie, the tailnet's
    /// identity, the connection's headers nor any `x-prism-` of its own; the secret, the phone the
    /// hub let in and the loopback host set by the hub.
    #[test]
    fn upstream_keeps_the_hubs_word_only() {
        let base = std::env::temp_dir().join(format!("prism-relay-up-{}", std::process::id()));
        let work = Remote::of_profile(&base, "default", Hub::new(), Arc::new(Fake("work"))).unwrap();
        let e = super::Endpoint { port: 4321, secret: "relayed-secret".into(), hub: false, url: None };
        let mut phone = axum::http::HeaderMap::new();
        for (k, v) in [
            ("cookie", "prism_session=abc"),
            ("tailscale-user-login", "me@example.com"),
            ("x-prism-relay", "forged"),
            ("x-prism-device", "someone-else"),
            ("x-prism-device-name", "Forged"),
            ("x-prism-origin", "https://evil.example"),
            ("origin", "https://mac.tail.ts.net"),
            ("host", "mac.tail.ts.net"),
            ("connection", "upgrade"),
            ("sec-websocket-key", "k"),
            ("content-length", "12"),
            ("accept", "application/json"),
            ("content-type", "application/json"),
        ] {
            phone.append(axum::http::HeaderName::from_static(k), v.parse().unwrap());
        }
        let up = super::upstream(&work.inner, &e, &phone, "phone-1");
        let get = |k: &str| up.get_all(k).iter().map(|v| v.to_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(get(super::SECRET), ["relayed-secret"]);
        assert_eq!(get(super::DEVICE), ["phone-1"]);
        assert_eq!(get("host"), ["127.0.0.1:4321"]);
        assert_eq!(get(super::DEVICE_NAME), [""], "a name the hub does not know is none");
        assert_eq!((get("accept"), get("content-type")), (vec!["application/json".to_string()], vec!["application/json".to_string()]));
        for k in ["cookie", "tailscale-user-login", "origin", "connection", "sec-websocket-key", "content-length"] {
            assert!(up.get(k).is_none(), "{k} forwarded");
        }
        assert!(get(super::ORIGIN).iter().all(|o| o != "https://evil.example"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn secrets_compare_whole() {
        assert!(super::same("abc", "abc"));
        assert!(!super::same("abc", "abd") && !super::same("abc", "ab") && !super::same("", "a"));
    }
}
