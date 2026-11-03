//! A phone paired from a terminal (`prism pair`), for a Prism without a window to show the QR code:
//! the running Prism's remote answers `/relay/pair/…` on loopback to whoever holds its endpoint's
//! secret (`remote/endpoint.json`, the owner's only), as Settings › Remote › Devices does on the desk.
//! The phone's code is still confirmed by a person, in the terminal.

use std::{path::Path, time::Duration};

use anyhow::{bail, Context, Result};
use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use chrono::{DateTime, Utc};
use prism_base::i18n::tr;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{relay, server::Shared, Remote};

// The running Prism's side, behind `guard`'s check of the secret.

fn pairing(inner: &Shared) -> Value {
    let pairing = inner.hub.subscribe().borrow().remote.as_ref().and_then(|r| r.pairing.clone());
    json!({ "pairing": pairing })
}

fn answer(inner: &Shared, done: Result<()>) -> Response {
    match done {
        Ok(()) => Json(pairing(inner)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

/// `POST /relay/pair/start`: a pairing opened, as the desk's Pair a phone.
pub(crate) async fn start(State(inner): State<Shared>) -> Response {
    let done = Remote { inner: inner.clone() }.pair_start();
    answer(&inner, done)
}

/// `GET /relay/pair`: the pairing under way, if any.
pub(crate) async fn state(State(inner): State<Shared>) -> Response {
    Json(pairing(&inner)).into_response()
}

#[derive(Deserialize)]
pub(crate) struct Confirm {
    code: String,
}

/// `POST /relay/pair/confirm`: the code the phone shows is the one the terminal shows (`code`,
/// still the pairing's: not another phone's claim of a pairing opened since).
pub(crate) async fn confirm(State(inner): State<Shared>, Json(c): Json<Confirm>) -> Response {
    let done = Remote { inner: inner.clone() }.pair_confirm(Some(&c.code));
    answer(&inner, done)
}

/// `POST /relay/pair/cancel`.
pub(crate) async fn cancel(State(inner): State<Shared>) -> Response {
    Remote { inner: inner.clone() }.pair_cancel();
    answer(&inner, Ok(()))
}

/// A computer's pairing under way, as the snapshot has it.
fn peer_pairing(inner: &Shared) -> Value {
    let pairing = inner.hub.subscribe().borrow().remote.as_ref().and_then(|r| r.peer_pairing.clone());
    json!({ "pairing": pairing })
}

fn peer_answer_of(inner: &Shared, done: Result<()>) -> Response {
    match done {
        Ok(()) => Json(peer_pairing(inner)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

/// `POST /relay/peer/start`: a computer's pairing opened, its code shown.
pub(crate) async fn peer_start(State(inner): State<Shared>) -> Response {
    let done = Remote { inner: inner.clone() }.peer_pair_start().map(drop);
    peer_answer_of(&inner, done)
}

/// `GET /relay/peer`.
pub(crate) async fn peer_state(State(inner): State<Shared>) -> Response {
    Json(peer_pairing(&inner)).into_response()
}

#[derive(Deserialize)]
pub(crate) struct Answer {
    accept: bool,
}

/// `POST /relay/peer/answer`: the computer that gave the code accepted, or refused.
pub(crate) async fn peer_answer(State(inner): State<Shared>, Json(a): Json<Answer>) -> Response {
    let done = Remote { inner: inner.clone() }.peer_pair_answer(a.accept);
    peer_answer_of(&inner, done)
}

/// `POST /relay/peer/cancel`.
pub(crate) async fn peer_cancel(State(inner): State<Shared>) -> Response {
    Remote { inner: inner.clone() }.peer_pair_cancel();
    peer_answer_of(&inner, Ok(()))
}

// The terminal's side.

/// A pairing under way, as the terminal shows it.
#[derive(Debug, Clone, Deserialize)]
pub struct Pairing {
    /// The address the phone opens (the QR code's).
    pub url: String,
    pub expires_at: DateTime<Utc>,
    /// Once a phone has opened it: the code it shows, and its name.
    pub code: Option<String>,
    pub device: Option<String>,
    /// The phone has made its passkey: its code may be confirmed.
    pub ready: bool,
}

/// The running Prism of a profile, reached on loopback with its endpoint's secret.
pub struct Local(relay::Endpoint);

impl Local {
    /// The Prism whose data folder is `dir`, if its remote runs.
    pub fn find(dir: &Path) -> Result<Self> {
        let Some(e) = relay::read(dir) else { bail!(tr("the remote is off or does not answer")) };
        if !e.hub {
            bail!(tr("this profile is reached through another one's remote: pair the phone there"));
        }
        Ok(Self(e))
    }

    async fn call(&self, method: reqwest::Method, path: &str, body: Value) -> Result<Option<Pairing>> {
        let url = format!("http://127.0.0.1:{}/relay/pair{path}", self.0.port);
        let res = relay::client().request(method, url).header(relay::SECRET, &self.0.secret).json(&body).timeout(Duration::from_secs(10)).send().await.context(tr("the remote is off or does not answer"))?;
        let ok = res.status().is_success();
        let body: Value = res.json().await.unwrap_or_default();
        if !ok {
            bail!("{}", body.get("error").and_then(Value::as_str).unwrap_or("refused"));
        }
        Ok(serde_json::from_value(body.get("pairing").cloned().unwrap_or_default()).ok())
    }

    /// Opens a pairing (any other one is dropped).
    pub async fn start(&self) -> Result<Pairing> {
        self.call(reqwest::Method::POST, "/start", Value::Null).await?.context(tr("the remote is off or does not answer"))
    }

    /// The pairing under way; none once it expired, was cancelled or picked up.
    pub async fn state(&self) -> Result<Option<Pairing>> {
        self.call(reqwest::Method::GET, "", Value::Null).await
    }

    /// Confirms the pairing whose phone showed `code`.
    pub async fn confirm(&self, code: &str) -> Result<()> {
        self.call(reqwest::Method::POST, "/confirm", json!({ "code": code })).await.map(drop)
    }

    pub async fn cancel(&self) -> Result<()> {
        self.call(reqwest::Method::POST, "/cancel", Value::Null).await.map(drop)
    }

    async fn peer(&self, method: reqwest::Method, path: &str, body: Value) -> Result<Option<PeerPairing>> {
        let url = format!("http://127.0.0.1:{}/relay/peer{path}", self.0.port);
        let res = relay::client().request(method, url).header(relay::SECRET, &self.0.secret).json(&body).timeout(Duration::from_secs(10)).send().await.context(tr("the remote is off or does not answer"))?;
        let ok = res.status().is_success();
        let body: Value = res.json().await.unwrap_or_default();
        if !ok {
            bail!("{}", body.get("error").and_then(Value::as_str).unwrap_or("refused"));
        }
        Ok(serde_json::from_value(body.get("pairing").cloned().unwrap_or_default()).ok())
    }

    /// Opens a computer's pairing: its code.
    pub async fn peer_start(&self) -> Result<PeerPairing> {
        self.peer(reqwest::Method::POST, "/start", Value::Null).await?.context(tr("the remote is off or does not answer"))
    }

    /// The computer's pairing under way; none once over.
    pub async fn peer_state(&self) -> Result<Option<PeerPairing>> {
        self.peer(reqwest::Method::GET, "", Value::Null).await
    }

    /// The computer that gave the code accepted, or refused.
    pub async fn peer_answer(&self, accept: bool) -> Result<()> {
        self.peer(reqwest::Method::POST, "/answer", json!({ "accept": accept })).await.map(drop)
    }

    pub async fn peer_cancel(&self) -> Result<()> {
        self.peer(reqwest::Method::POST, "/cancel", Value::Null).await.map(drop)
    }
}

/// A computer's pairing, as the terminal shows it.
#[derive(Debug, Clone, Deserialize)]
pub struct PeerPairing {
    pub code: String,
    pub expires_at: DateTime<Utc>,
    /// Once a computer gave the code: its name.
    pub name: Option<String>,
}

/// `link` as a QR code a terminal shows: two rows of modules per line of text.
pub fn qr_text(link: &str) -> Result<String> {
    let code = qrcode::QrCode::new(link.as_bytes())?;
    Ok(code.render::<qrcode::render::unicode::Dense1x2>().dark_color(qrcode::render::unicode::Dense1x2::Light).light_color(qrcode::render::unicode::Dense1x2::Dark).quiet_zone(true).build())
}
