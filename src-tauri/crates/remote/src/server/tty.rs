//! A phone's or another Prism's view of an agent's terminal: its screen streamed, typed in when it may.

use super::*;

#[derive(Deserialize)]
pub(super) struct TtyQuery {
    agent: String,
    /// Typing: its control confirmed (`remote_control`) within the step-up window.
    #[serde(default)]
    control: bool,
    /// The phone's size, watched or typed in (0: an older page, watched at the window's size).
    #[serde(default)]
    cols: u16,
    #[serde(default)]
    rows: u16,
}

/// An agent's terminal on a phone, at the phone's size: watched by a read-only tmux client, so
/// nothing the page sends reaches the agent, or typed in once taken. First its size, then its
/// output as binary frames.
pub(super) async fn tty(State(inner): State<Shared>, headers: HeaderMap, axum::extract::Query(q): axum::extract::Query<TtyQuery>, ws: WebSocketUpgrade) -> Response {
    let (Some(token), Some(device)) = (session_of(&inner, &headers), device_of(&inner, &headers)) else { return refuse(StatusCode::UNAUTHORIZED, tr("no session")) };
    if inner.host.class("agent_attach", &json!({ "id": q.agent })) != Some(Class::Read) {
        return refuse(StatusCode::FORBIDDEN, tr("terminals are not shown on a phone"));
    }
    let size = (q.cols > 0 && q.rows > 0).then(|| (q.cols.clamp(20, 400), q.rows.clamp(5, 200)));
    let control = if q.control {
        let Some((cols, rows)) = size else { return refuse(StatusCode::BAD_REQUEST, "a terminal is taken at the phone's size") };
        let typing = inner.live().typing;
        let confirmed = inner.auth().stepped_up(&device, &format!("control:{}", q.agent), Utc::now());
        if !typing || !confirmed {
            return refuse(StatusCode::FORBIDDEN, tr("take the terminal first (remote_control)"));
        }
        Some((cols, rows, crate::relay::device_name(&inner, &headers, &device)))
    } else {
        None
    };
    ws.max_message_size(64 * 1024).on_upgrade(move |socket| watch(inner, socket, token, q.agent, size, control, None))
}

/// `peer`: a peer's token (`peers`), its window's terminal: neither in the desk's sight nor
/// given back when idle, as the desk's own.
pub(super) async fn watch(inner: Shared, mut socket: WebSocket, token: Option<String>, agent: String, size: Option<(u16, u16)>, control: Option<(u16, u16, String)>, peer: Option<Option<String>>) {
    let (out, mut queue) = tokio::sync::mpsc::unbounded_channel::<Option<Vec<u8>>>();
    let ended = out.clone();
    // Only `on_end` ends the terminal: a source done writing (a demo's recorded screen) stays shown.
    let _open = out.clone();
    let watched = inner
        .host
        .watch(
            agent.clone(),
            size,
            control.is_some(),
            Box::new(move |bytes| {
                let _ = out.send(Some(bytes));
            }),
            Box::new(move || {
                let _ = ended.send(None);
            }),
        )
        .await;
    let mut w = match watched {
        Ok(w) => w,
        Err(e) => {
            let _ = socket.send(text(json!({ "t": "error", "error": format!("{e:#}") }))).await;
            return;
        }
    };
    if socket.send(text(json!({ "t": "size", "cols": w.cols, "rows": w.rows, "control": control.is_some(), "sized": size.is_some() }))).await.is_err() {
        return;
    }
    // Controlled: in the desk's sight (status bar), until released, given back when idle, or
    // taken back by the desk.
    let (release_tx, mut released) = tokio::sync::oneshot::channel::<()>();
    if let Some((_, _, name)) = control.as_ref().filter(|_| peer.is_none()) {
        inner.controls().push(crate::Control { agent: agent.clone(), device_name: name.clone(), since: Utc::now(), release: release_tx });
        inner.publish();
    } else {
        drop(release_tx);
    }
    let idle = inner.live().control_idle;
    let mut last_key = std::time::Instant::now();
    // Its first tick a period away: the screen goes first.
    let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + std::time::Duration::from_secs(10), std::time::Duration::from_secs(10));
    let close = |code: u16, why: &'static str| Message::Close(Some(axum::extract::ws::CloseFrame { code, reason: why.into() }));
    loop {
        tokio::select! {
            data = queue.recv() => match data {
                Some(Some(bytes)) => if socket.send(Message::Binary(bytes.into())).await.is_err() { break },
                _ => break,
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                // Keys: only a controlled terminal takes them (a watched one's are dropped).
                Some(Ok(Message::Binary(keys))) if control.is_some() => {
                    // A peer's typing turned off since: the terminal closes, nothing typed.
                    if peer.is_some() && !inner.live().peers.typing {
                        let _ = socket.send(Message::Close(Some(axum::extract::ws::CloseFrame { code: 4403, reason: "typing from another Prism is off".into() }))).await;
                        break;
                    }
                    last_key = std::time::Instant::now();
                    if w.term.write(&keys).is_err() { break }
                }
                // A turn of the phone: its terminal follows, watched or typed in.
                Some(Ok(Message::Text(t))) if size.is_some() => {
                    if let Ok(v) = serde_json::from_str::<Value>(&t)
                        && v["t"] == "resize"
                    {
                        let (c, r) = (v["cols"].as_u64().unwrap_or(80) as u16, v["rows"].as_u64().unwrap_or(24) as u16);
                        let _ = w.term.resize(c.clamp(20, 400), r.clamp(5, 200));
                    }
                }
                Some(Ok(_)) => {}
            },
            _ = &mut released, if control.is_some() && peer.is_none() => {
                let _ = socket.send(close(4409, "the desk took the terminal back")).await;
                break;
            }
            _ = tick.tick() => {
                // A sign of life, even on a quiet screen: the phone tells a still terminal from a
                // dead socket (it reconnects), and a dead one ends here.
                if socket.send(text(json!({ "t": "alive" }))).await.is_err() {
                    break;
                }
                let still = match &peer {
                    Some(t) => peer_alive(&inner, t),
                    None => alive(&inner, &token),
                };
                if !still {
                    let _ = socket.send(close(4401, "session ended")).await;
                    break;
                }
                if control.is_some() && peer.is_none() && idle.is_some_and(|d| last_key.elapsed() > d) {
                    let _ = socket.send(close(4408, "given back after a while without a key")).await;
                    break;
                }
            }
        }
    }
    drop(w.term);
    // Its own receiver dropped, this socket's entry is the one whose sender is closed.
    drop(released);
    if control.is_some() && peer.is_none() {
        let mut c = inner.controls();
        let before = c.len();
        // This socket's own entry (the desk's take-back already removed it).
        if let Some(i) = c.iter().position(|x| x.agent == agent && x.release.is_closed()) {
            c.remove(i);
        }
        let changed = c.len() != before;
        drop(c);
        if changed {
            inner.publish();
        }
    }
}
