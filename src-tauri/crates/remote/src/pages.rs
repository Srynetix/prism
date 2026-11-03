//! An agent's pages on a phone (its Web tab): a mockup, a report, an artifact of its session,
//! served for real (its scripts, the files next to it) at `/page/<token>/<path>`, as `prismfile:`
//! serves them on the computer. The token is the right to read: one agent's folder, or its scratchpad,
//! for an hour after it was last used. The page never runs as the remote's page: its CSP's
//! `sandbox` gives it an opaque origin, so the session's cookie (SameSite=Strict) does not go with
//! its requests and the socket refuses its origin; it fetches only under its own token.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use anyhow::Context;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{json, Value};

use crate::{lock, server::Shared, Inner};

/// How long an address stays good after its last request.
const KEPT: Duration = Duration::from_secs(60 * 60);
/// Addresses kept at most: the oldest goes first.
const MAX: usize = 64;

/// A root of pages opened to a phone.
pub(crate) struct Open {
    agent: String,
    /// `@scratchpad`, or empty for the agent's folder.
    root: String,
    until: Instant,
}

/// The address of `{id, scratchpad}`'s pages: `/page/<token>/`, a path of the root after it.
pub(crate) fn open(inner: &Inner, args: &Value) -> anyhow::Result<Value> {
    let agent = args.get("id").and_then(Value::as_str).context("an agent")?.to_string();
    anyhow::ensure!(!agent.is_empty() && !agent.contains(['/', '\\', '%']), "an agent id");
    let root = if args.get("scratchpad").and_then(Value::as_bool).unwrap_or(false) { "@scratchpad" } else { "" }.to_string();
    let mut open = lock(&inner.pages);
    sweep(&mut open);
    // The same root keeps its address: a page reloaded keeps its place in the phone's history.
    let token = match open.iter_mut().find(|(_, o)| o.agent == agent && o.root == root) {
        Some((token, o)) => {
            o.until = Instant::now() + KEPT;
            token.clone()
        }
        None => {
            if open.len() >= MAX && let Some(oldest) = open.iter().min_by_key(|(_, o)| o.until).map(|(t, _)| t.clone()) {
                open.remove(&oldest);
            }
            let token = crate::download::token();
            open.insert(token.clone(), Open { agent, root, until: Instant::now() + KEPT });
            token
        }
    };
    Ok(json!({ "url": format!("{}/page/{token}/", inner.base()) }))
}

fn sweep(open: &mut HashMap<String, Open>) {
    let now = Instant::now();
    open.retain(|_, o| o.until > now);
}

/// The public CDNs an agent's page may load scripts, styles, fonts and pictures from: hosts
/// that serve packages and fonts, whose request logs a page's author cannot read.
const PAGE_CDNS: &str = "https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com https://esm.sh https://cdn.tailwindcss.com https://code.jquery.com https://fonts.googleapis.com https://fonts.gstatic.com";

/// The CSP of an agent's page (the desktop's `prismfile:` and phone `/page/`), `own` being its own
/// files' source and `connect` what it may fetch. Sandboxed (an opaque origin, no popup), it
/// reads its folder but has no way to send what it read out: no `https:` at large, so no
/// image, script or frame address to carry it, no form, no socket.
pub fn page_csp(own: &str, connect: &str) -> String {
    format!(
        "sandbox allow-scripts allow-modals; default-src {own} data: blob: 'unsafe-inline' 'unsafe-eval'; script-src {own} data: blob: 'unsafe-inline' 'unsafe-eval' {PAGE_CDNS}; style-src {own} data: blob: 'unsafe-inline' {PAGE_CDNS}; font-src {own} data: {PAGE_CDNS}; img-src {own} data: blob: {PAGE_CDNS}; connect-src {connect}; form-action 'none'; base-uri 'none'"
    )
}

/// `GET /page/<token>/<path>`: the file, as `prismfile:` would serve it, in its sandbox.
pub(crate) async fn serve(State(inner): State<Shared>, req: Request) -> Response {
    // The path as sent, percent-encoded: the agents' side decodes it once.
    let rest = req.uri().path().strip_prefix("/page/").unwrap_or_default();
    let (token, path) = rest.split_once('/').unwrap_or((rest, ""));
    let target = {
        let mut open = lock(&inner.pages);
        sweep(&mut open);
        open.get_mut(token).map(|o| {
            o.until = Instant::now() + KEPT;
            if o.root.is_empty() { format!("/{}/{path}", o.agent) } else { format!("/{}/{}/{path}", o.agent, o.root) }
        })
    };
    let Some(target) = target else { return (StatusCode::NOT_FOUND, "no such page, or no longer: open it again from Prism").into_response() };
    // Relayed: framed by the hub's page, under this profile's path there (`relay`).
    let origin = {
        let live = inner.live();
        if live.relay { live.hub_origin.clone() } else { live.party.as_ref().map(|p| p.origin.clone()) }
    }
    .unwrap_or_default();
    let base = inner.base();
    let host = inner.host.clone();
    let served = tokio::task::spawn_blocking(move || host.page(&target)).await.unwrap_or_else(|e| Err((500, e.to_string())));
    let mut res = match served {
        Ok((body, mime)) => ([(header::CONTENT_TYPE, mime)], Body::from(body)).into_response(),
        Err((code, msg)) => (StatusCode::from_u16(code).unwrap_or(StatusCode::NOT_FOUND), [(header::CONTENT_TYPE, "text/plain; charset=utf-8".to_string())], msg).into_response(),
    };
    let h = res.headers_mut();
    // Framed by the remote's page only, whose frame-src keeps the page from navigating away.
    let csp = format!("{}; frame-ancestors {origin}", page_csp("'self'", &format!("{origin}{base}/page/{token}/")));
    if let Ok(v) = HeaderValue::from_str(&csp) {
        h.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page reads its folder but has no address to send it to: no `https:` at large, no popup,
    /// no form; scripts and pictures from the package CDNs only.
    #[test]
    fn a_page_sends_nothing_out() {
        let csp = page_csp("'self'", "https://mac.tail.ts.net/page/t/");
        assert!(!csp.split([' ', ';']).any(|s| s == "https:"), "{csp}");
        assert!(!csp.contains("allow-popups") && !csp.contains("allow-forms"));
        assert!(csp.contains("form-action 'none'") && csp.contains("connect-src https://mac.tail.ts.net/page/t/;"));
        assert!(csp.contains("script-src 'self'") && csp.contains("https://cdn.jsdelivr.net"));
        assert!(csp.contains("img-src 'self' data: blob: https://cdnjs"));
    }
}
