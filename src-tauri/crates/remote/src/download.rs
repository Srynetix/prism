//! Files a phone downloads (an agent's Files tab): the computer writes them to a file of its own
//! (`agent_files_download`), the phone fetches it once over HTTP, streamed, at an address only
//! it may open and only for a few minutes, rather than as base64 through its socket (a 60 MB zip
//! took ~395 MB on the computer, and its decoding seconds on the phone).

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::Context;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{json, Value};

use crate::{
    lock,
    server::{device_of, Shared},
    Inner,
};

/// How long an address stays good, unfetched.
const KEPT: Duration = Duration::from_secs(5 * 60);

/// A download waiting for its phone.
pub(crate) struct Pending {
    device: String,
    name: String,
    path: PathBuf,
    until: Instant,
}

/// The file the computer prepared (`{name, path}`) offered to `device`: the name to save it under and
/// the address to fetch it from (`AgentDownload`).
pub(crate) fn offer(inner: &Inner, device: &str, prepared: Value) -> anyhow::Result<Value> {
    let name = prepared["name"].as_str().context("a name")?.to_string();
    let path = PathBuf::from(prepared["path"].as_str().context("a file")?);
    let token = token();
    let mut pending = lock(&inner.downloads);
    sweep(&mut pending);
    pending.insert(token.clone(), Pending { device: device.to_string(), name: name.clone(), path, until: Instant::now() + KEPT });
    Ok(json!({ "name": name, "url": format!("{}/api/download/{token}", inner.base()) }))
}

/// The file this Prism prepared offered to a peer's window (`peers`, `name`): the name to save it
/// under and the token of its address (`/peer/download/<token>`, its profile's path added there).
pub(crate) fn offer_peer(inner: &Inner, peer: &str, prepared: Value) -> anyhow::Result<Value> {
    let name = prepared["name"].as_str().context("a name")?.to_string();
    let path = PathBuf::from(prepared["path"].as_str().context("a file")?);
    let token = token();
    let mut pending = lock(&inner.downloads);
    sweep(&mut pending);
    pending.insert(token.clone(), Pending { device: format!("{}{peer}", crate::peers::CONNECTED), name: name.clone(), path, until: Instant::now() + KEPT });
    Ok(json!({ "name": name, "token": token }))
}

/// Those not fetched in time: their file goes.
fn sweep(pending: &mut std::collections::HashMap<String, Pending>) {
    let now = Instant::now();
    pending.retain(|_, p| {
        let keep = p.until > now;
        if !keep {
            let _ = std::fs::remove_file(&p.path);
        }
        keep
    });
}

pub(crate) fn token() -> String {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).expect("the system's random source");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `GET /api/download/<token>`: the file, once, to the phone it was offered to, streamed; removed
/// once sent (or the transfer cut).
pub(crate) async fn serve(State(inner): State<Shared>, headers: HeaderMap, Path(token): Path<String>) -> Response {
    let Some(device) = device_of(&inner, &headers) else { return (StatusCode::UNAUTHORIZED, "no session").into_response() };
    send(&inner, &device, &token).await
}

/// `GET /peer/download/<token>`: the same, to the peer's window it was offered to.
pub(crate) async fn serve_peer(State(inner): State<Shared>, headers: HeaderMap, Path(token): Path<String>) -> Response {
    let Some((name, _)) = crate::server::peer_of(&inner, &headers) else { return (StatusCode::UNAUTHORIZED, "not a peer of this Prism").into_response() };
    send(&inner, &format!("{}{name}", crate::peers::CONNECTED), &token).await
}

async fn send(inner: &Inner, device: &str, token: &str) -> Response {
    let taken = {
        let mut pending = lock(&inner.downloads);
        sweep(&mut pending);
        match pending.get(token) {
            Some(p) if p.device == device => pending.remove(token),
            _ => None,
        }
    };
    let Some(p) = taken else { return (StatusCode::NOT_FOUND, "no such download, or no longer").into_response() };
    let gone = Removed(p.path.clone());
    let file = match tokio::fs::File::open(&p.path).await {
        Ok(f) => f,
        Err(e) => return (StatusCode::NOT_FOUND, format!("{e}")).into_response(),
    };
    let len = file.metadata().await.map(|m| m.len()).ok();
    let body = futures::stream::unfold((file, gone), |(mut file, gone)| async move {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 256 << 10];
        match file.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((Ok::<_, std::io::Error>(buf), (file, gone)))
            }
            Err(e) => Some((Err(e), (file, gone))),
        }
    });
    let mut res = Body::from_stream(body).into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename*=UTF-8''{}", encode(&p.name))) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    if let Some(len) = len {
        h.insert(header::CONTENT_LENGTH, len.into());
    }
    res
}

/// The file removed once its stream is done with, sent whole or not.
struct Removed(PathBuf);

impl Drop for Removed {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A file name for `filename*` (RFC 5987): its bytes outside the safe ones percent-encoded.
fn encode(name: &str) -> String {
    name.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_are_encoded_for_their_header() {
        assert_eq!(super::encode("rapport été.zip"), "rapport%20%C3%A9t%C3%A9.zip");
    }
}
