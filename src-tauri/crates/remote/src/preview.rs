//! A dev server's preview on a phone (`[remote] preview`): the app an agent's folder serves
//! (Dev servers block), as the phone's browser runs it, hot reload included. A second loopback
//! port (`preview_port`), behind a second address of the tunnel (Tailscale: the Mac's name on port
//! 8443), forwards each request to the dev server the phone chose last (`preview_open`).
//!
//! Its own origin, so the app's scripts never run in the remote's page; the same host, so the
//! phone's session cookie comes along: no session, nothing forwarded. Only a port a running dev
//! server of the block listens on, checked at each request. The dev server sees a local request
//! (`Host: localhost:<port>`, its own origin), never the session cookie nor the tailnet user's
//! identity; its answers cannot set Prism's cookie.

use std::sync::{Arc, OnceLock};

use axum::{
    body::Body,
    extract::{ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade}, FromRequestParts, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    Router,
};
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

use crate::{server, Inner};

type Shared = Arc<Inner>;

/// The tailnet port of the preview's address (`tailscale serve --https=8443`).
pub const TAILNET_PORT: u16 = 8443;
/// Largest request body forwarded (an upload to the app).
const MAX_BODY: usize = 64 << 20;

/// The headers that belong to one connection, never forwarded.
const HOP: [&str; 9] = ["connection", "keep-alive", "proxy-connection", "proxy-authenticate", "proxy-authorization", "te", "trailer", "transfer-encoding", "upgrade"];

pub fn router(inner: Shared) -> Router {
    Router::new().fallback(proxy).with_state(inner)
}

/// The ports the Dev servers block shows a running server of Prism's listening on (an agent's
/// shell, the agent, a container of its folder); not the ports held outside Prism.
pub fn dev_ports(snap: &prism_core::state::Snapshot) -> Vec<u16> {
    let servers = snap.blocks.get("dev_servers").and_then(|b| b.get("servers")).and_then(Value::as_array);
    servers
        .into_iter()
        .flatten()
        .filter(|s| s.get("state").and_then(Value::as_str) == Some("running"))
        .flat_map(|s| s.get("ports").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64))
        .filter_map(|p| u16::try_from(p).ok())
        .collect()
}

/// A short page for the phone's browser: why nothing shows.
fn page(status: StatusCode, why: &str) -> Response {
    let body = format!("<!doctype html><meta name=viewport content=\"width=device-width\"><title>Prism preview</title><body style=\"font:16px -apple-system,system-ui,sans-serif;padding:24px;color:#444\"><h3>Prism preview</h3><p>{why}</p>");
    (status, [(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

async fn proxy(State(inner): State<Shared>, req: Request) -> Response {
    let live = inner.live().clone();
    let Some(party) = live.party.as_ref() else { return page(StatusCode::SERVICE_UNAVAILABLE, "Prism is starting.") };
    let headers = req.headers();
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or_default();
    let host_name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    let loopback = [format!("127.0.0.1:{}", live.preview_port), format!("localhost:{}", live.preview_port)];
    if host_name != party.id && !loopback.iter().any(|l| l == host) {
        return page(StatusCode::MISDIRECTED_REQUEST, "Unexpected host.");
    }
    if let Some(owner) = &live.owner
        && headers.get("tailscale-user-login").and_then(|h| h.to_str().ok()) != Some(owner.as_str())
    {
        return page(StatusCode::FORBIDDEN, "Only this Mac's tailnet user.");
    }
    if server::device_of(&inner, headers).is_none() {
        return page(StatusCode::UNAUTHORIZED, "Unlock Prism on this phone first, then open the preview again from Dev servers.");
    }
    let Some(port) = *inner.preview() else { return page(StatusCode::NOT_FOUND, "No dev server chosen: open one from Dev servers on the phone.") };
    // One of this Prism's dev servers, or the one a relayed profile opened (`relay`).
    if !crate::relay::preview_allowed(&inner, port).await {
        return page(StatusCode::NOT_FOUND, &format!("The dev server on port {port} is no longer running."));
    }
    let origin = live.preview_origin.clone().unwrap_or_default();
    let websocket = req.headers().get(header::UPGRADE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    if websocket {
        relay(port, &origin, req).await
    } else {
        forward(port, &origin, req).await
    }
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    // Redirects go back to the phone, as the dev server sent them; nothing through a system proxy.
    CLIENT.get_or_init(|| reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).no_proxy().build().expect("an HTTP client"))
}

/// The phone's headers as the dev server gets them: a request of its own origin on loopback, the
/// session cookie and the tailnet user's identity left out.
fn upstream(headers: &HeaderMap, port: u16, origin: &str) -> HeaderMap {
    let local = format!("http://localhost:{port}");
    let mut out = HeaderMap::new();
    for (k, v) in headers {
        let name = k.as_str();
        if HOP.contains(&name) || name == "host" || name == "content-length" || name.starts_with("tailscale-") || name.starts_with("sec-websocket-") {
            continue;
        }
        let v = match name {
            "cookie" => {
                let kept: Vec<&str> = v.to_str().unwrap_or_default().split(';').map(str::trim).filter(|c| !c.is_empty() && !c.starts_with(&format!("{}=", server::COOKIE))).collect();
                if kept.is_empty() {
                    continue;
                }
                HeaderValue::from_str(&kept.join("; ")).ok()
            }
            "origin" | "referer" => match v.to_str() {
                Ok(s) if !origin.is_empty() && s.starts_with(origin) => HeaderValue::from_str(&format!("{local}{}", &s[origin.len()..])).ok(),
                _ => Some(v.clone()),
            },
            _ => Some(v.clone()),
        };
        if let Some(v) = v {
            out.append(k.clone(), v);
        }
    }
    if let Ok(h) = HeaderValue::from_str(&format!("localhost:{port}")) {
        out.insert(header::HOST, h);
    }
    out
}

/// The dev server's headers as the phone gets them: its redirects to itself made relative, no
/// cookie named as Prism's session.
fn downstream(headers: &HeaderMap, port: u16) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (k, v) in headers {
        let name = k.as_str();
        if HOP.contains(&name) {
            continue;
        }
        if name == "set-cookie" && v.to_str().unwrap_or_default().trim_start().starts_with(&format!("{}=", server::COOKIE)) {
            continue;
        }
        let v = if name == "location" {
            let s = v.to_str().unwrap_or_default();
            let local = [format!("http://localhost:{port}"), format!("http://127.0.0.1:{port}")];
            match local.iter().find_map(|l| s.strip_prefix(l.as_str())) {
                Some(rest) => HeaderValue::from_str(if rest.is_empty() { "/" } else { rest }).unwrap_or_else(|_| v.clone()),
                None => v.clone(),
            }
        } else {
            v.clone()
        };
        out.append(k.clone(), v);
    }
    out
}

async fn forward(port: u16, origin: &str, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let path = parts.uri.path_and_query().map_or("/", |p| p.as_str());
    let body = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(b) => b,
        Err(_) => return page(StatusCode::PAYLOAD_TOO_LARGE, "This request is too large for the preview."),
    };
    let sent = client().request(parts.method, format!("http://127.0.0.1:{port}{path}")).headers(upstream(&parts.headers, port, origin)).body(body).send().await;
    let res = match sent {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("remote: preview of port {port}: {e}");
            return page(StatusCode::BAD_GATEWAY, &format!("The dev server on port {port} does not answer."));
        }
    };
    let (status, headers) = (res.status(), downstream(res.headers(), port));
    let mut out = Response::new(Body::from_stream(res.bytes_stream()));
    *out.status_mut() = status;
    *out.headers_mut() = headers;
    out
}

/// A WebSocket (hot reload): opened to the dev server first, with the phone's subprotocols, then
/// the phone's accepted with the one the dev server chose, and the two relayed.
async fn relay(port: u16, origin: &str, req: Request) -> Response {
    let (mut parts, _) = req.into_parts();
    let path = parts.uri.path_and_query().map_or("/", |p| p.as_str()).to_string();
    let protocols = parts.headers.get(header::SEC_WEBSOCKET_PROTOCOL).cloned();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    let Ok(mut request) = format!("ws://127.0.0.1:{port}{path}").into_client_request() else { return page(StatusCode::BAD_REQUEST, "Bad address.") };
    for (k, v) in upstream(&parts.headers, port, origin) {
        if let Some(k) = k {
            request.headers_mut().insert(k, v);
        }
    }
    if let Some(p) = protocols {
        request.headers_mut().insert(header::SEC_WEBSOCKET_PROTOCOL, p);
    }
    let (theirs, response) = match tokio_tungstenite::connect_async(request).await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("remote: preview of port {port}: socket: {e}");
            return page(StatusCode::BAD_GATEWAY, &format!("The dev server on port {port} refused the socket."));
        }
    };
    let chosen = response.headers().get(header::SEC_WEBSOCKET_PROTOCOL).and_then(|v| v.to_str().ok()).map(String::from);
    let upgrade = match chosen {
        Some(p) => upgrade.protocols([p]),
        None => upgrade,
    };
    upgrade.on_upgrade(move |ours| pipe(ours, theirs))
}

type Upstream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn pipe(ours: WebSocket, theirs: Upstream) {
    let (mut ours_tx, mut ours_rx) = ours.split();
    let (mut theirs_tx, mut theirs_rx) = theirs.split();
    let up = async {
        while let Some(Ok(m)) = ours_rx.next().await {
            let m = match m {
                Message::Text(t) => tungstenite::Message::Text(t.as_str().into()),
                Message::Binary(b) => tungstenite::Message::Binary(b),
                Message::Ping(b) => tungstenite::Message::Ping(b),
                Message::Pong(b) => tungstenite::Message::Pong(b),
                Message::Close(f) => {
                    let f = f.map(|f| tungstenite::protocol::CloseFrame { code: f.code.into(), reason: f.reason.as_str().into() });
                    let _ = theirs_tx.send(tungstenite::Message::Close(f)).await;
                    break;
                }
            };
            if theirs_tx.send(m).await.is_err() {
                break;
            }
        }
    };
    let down = async {
        while let Some(Ok(m)) = theirs_rx.next().await {
            let m = match m {
                tungstenite::Message::Text(t) => Message::Text(t.as_str().into()),
                tungstenite::Message::Binary(b) => Message::Binary(b),
                tungstenite::Message::Ping(b) => Message::Ping(b),
                tungstenite::Message::Pong(b) => Message::Pong(b),
                tungstenite::Message::Close(f) => {
                    let f = f.map(|f| CloseFrame { code: f.code.into(), reason: f.reason.as_str().into() });
                    let _ = ours_tx.send(Message::Close(f)).await;
                    break;
                }
                tungstenite::Message::Frame(_) => continue,
            };
            if ours_tx.send(m).await.is_err() {
                break;
            }
        }
    };
    // Either side gone, both end.
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }
}
