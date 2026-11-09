//! Another Prism's window on this one (`[remote.peers]`): who it is, its socket, its summary, its
//! terminals, the pairing by code, and what its rights let it call.

use super::*;

/// The peer a request comes from: its token checked here (its name and token); relayed, the name
/// the hub let in (no token: the hub watches it, `relay::pipe_peer`).
/// Local (`Local`): this computer's app, no token.
pub(crate) fn peer_of(inner: &Inner, headers: &HeaderMap, local: bool) -> Option<(String, Option<String>)> {
    if local {
        return Some((LOCAL_PEER.to_string(), None));
    }
    if inner.live().relay {
        return crate::relay::peer_name(headers).map(|n| (n, None));
    }
    crate::peers::of(&inner.dir, headers).map(|(n, t)| (n, Some(t)))
}

/// Whether a peer's token still opens this Prism (relayed: the hub's to say).
pub(super) fn peer_alive(inner: &Inner, token: &Option<String>) -> bool {
    token.as_ref().is_none_or(|t| crate::peers::check(&inner.dir, t).is_some())
}

/// Another Prism's window (`peers`), its token checked.
pub(super) async fn peer_socket(State(inner): State<Shared>, local: Option<Extension<Local>>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    let local = local.is_some();
    let Some((name, token)) = peer_of(&inner, &headers, local) else { return refuse(StatusCode::UNAUTHORIZED, tr("not a peer of this Prism")) };
    ws.max_message_size(MAX_MESSAGE).on_upgrade(move |socket| converse_peer(inner, socket, name, token, local))
}

/// A peer's window connected: the snapshot then its changes, every command of the window but its
/// own answered (`Host::peer_call`), until its token is revoked or it goes. Local (this computer's
/// app, a window of this profile): `Host::local_call`, neither `[remote.peers]` nor confirmation,
/// and the chains' notices too.
pub(super) async fn converse_peer(inner: Shared, socket: WebSocket, name: String, token: Option<String>, local: bool) {
    tracing::info!("remote: peer {name} connected");
    let seen = format!("{}{name}", crate::peers::CONNECTED);
    inner.connect(&seen, true);
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
    let _ = out.send(text(json!({ "t": "hello", "protocol": PROTOCOL, "peer": name, "profile": inner.profile }))).await;
    let snaps = {
        let token = token.clone();
        snapshots(inner.clone(), out.clone(), move |inner| peer_alive(inner, &token), |_| true)
    };
    // What the chains and the inbox tell: the window's toast, and the system's notification.
    let notices = local.then(|| inner.host.notices()).flatten().map(|mut notices| {
        let out = out.clone();
        tokio::spawn(async move {
            while let Some(n) = notices.next().await {
                if out.send(text(json!({ "t": "notice", "text": n.text, "ms": n.ms, "system": n.system }))).await.is_err() {
                    break;
                }
            }
        })
    });
    let calls = Arc::new(tokio::sync::Semaphore::new(MAX_CALLS * 2));
    while let Some(Ok(msg)) = rx.next().await {
        // Each message checks the token again: revoked, nothing more is answered, a peer that
        // ignores the close included.
        if !peer_alive(&inner, &token) {
            let _ = out.send(Message::Close(Some(axum::extract::ws::CloseFrame { code: 4401, reason: "session ended".into() }))).await;
            break;
        }
        let Message::Text(body) = msg else { continue };
        let Ok(FromPage::Call { id, command, args, confirmed, .. }) = serde_json::from_str::<FromPage>(&body) else { continue };
        let Ok(permit) = calls.clone().try_acquire_owned() else {
            let _ = out.send(text(json!({ "t": "result", "id": id, "error": "too many calls at once" }))).await;
            continue;
        };
        let (inner, out, name) = (inner.clone(), out.clone(), name.clone());
        tokio::spawn(async move {
            // What changes something is logged, with the peer.
            let class = inner.host.class(&command, &args);
            if !matches!(class, Some(Class::Read)) {
                tracing::info!("remote: peer {name}: {command}");
            }
            let allowed = if local { Ok(()) } else { peer_allowed(&inner.live().peers, &command, class) };
            // What this Prism classes critical is confirmed at the desk first, as it classes it
            // (a block of its own, a command the desk does not know): the window's app asks the
            // person there, then calls again with this purpose. Advisory against a token in other
            // hands, which can send the purpose itself: `[remote.peers] critical` is what holds.
            let purpose = purpose(&command, &args);
            if !local && allowed.is_ok() && class == Some(Class::Critical) && confirmed.as_deref() != Some(purpose.as_str()) {
                let _ = out.send(text(json!({ "t": "result", "id": id, "stepUp": { "purpose": purpose } }))).await;
                drop(permit);
                return;
            }
            let download = command == "agent_files_download";
            let answer = match allowed {
                Ok(()) if local => inner.host.local_call(command, args).await,
                Ok(()) => inner.host.peer_call(command, args).await,
                Err(e) => Err(e),
            };
            // An agent's files: prepared here, then fetched once by the window's app (`download`).
            let answer = match answer {
                Ok(prepared) if download => crate::download::offer_peer(&inner, &name, prepared),
                other => other,
            };
            let reply = match answer {
                Ok(v) => json!({ "t": "result", "id": id, "ok": v }),
                Err(e) => json!({ "t": "result", "id": id, "error": format!("{e:#}") }),
            };
            let _ = out.send(text(reply)).await;
            drop(permit);
        });
    }
    snaps.abort();
    if let Some(n) = notices {
        n.abort();
    }
    drop(out);
    let _ = writer.await;
    inner.connect(&seen, false);
    tracing::info!("remote: peer {name} gone");
}

/// `GET /peer/summary`: what a peer's status bar shows of this Prism, read every few seconds
/// whether its window is open or not: its agents running, those waiting for you, longest first.
/// The hub's, with its profiles (those relayed here, their agents and those waiting); a relayed
/// profile's, its own only.
pub(super) async fn peer_summary(State(inner): State<Shared>, local: Option<Extension<Local>>, headers: HeaderMap) -> Response {
    if peer_of(&inner, &headers, local.is_some()).is_none() {
        return refuse(StatusCode::UNAUTHORIZED, tr("not a peer of this Prism"));
    }
    let mine = crate::relay::counts(&inner.hub.subscribe().borrow());
    if local.is_some() || inner.live().relay {
        return Json(json!({ "agents": mine.agents, "waiting": mine.waiting_agents, "profiles": [] })).into_response();
    }
    let (profiles, waiting) = crate::relay::peer_profiles(&inner).await;
    let agents: u32 = profiles.iter().filter(|p| p.reachable).map(|p| p.agents).sum();
    Json(json!({ "agents": agents, "waiting": waiting, "profiles": profiles })).into_response()
}

#[derive(Deserialize)]
pub(super) struct PeerTtyQuery {
    agent: String,
    cols: u16,
    rows: u16,
}

/// An agent's terminal in a peer's window, typed in at the window's size, as the desk's own.
pub(super) async fn peer_tty(State(inner): State<Shared>, local: Option<Extension<Local>>, headers: HeaderMap, axum::extract::Query(q): axum::extract::Query<PeerTtyQuery>, ws: WebSocketUpgrade) -> Response {
    let Some((name, token)) = peer_of(&inner, &headers, local.is_some()) else { return refuse(StatusCode::UNAUTHORIZED, tr("not a peer of this Prism")) };
    if local.is_none() && !inner.live().peers.typing {
        return refuse(StatusCode::FORBIDDEN, tr("typing in a terminal from another Prism is off (Settings › Remote › Machines here)"));
    }
    let size = (q.cols.clamp(20, 500), q.rows.clamp(5, 300));
    ws.max_message_size(1 << 20).on_upgrade(move |socket| watch(inner, socket, None, q.agent, Some(size), Some((size.0, size.1, name)), Some(token)))
}

/// What types to an agent or runs a command of its own: `[remote.peers] typing`.
pub(super) const PEER_TYPING: &[&str] = &["agent_write", "agent_resize", "agent_type", "agent_send", "agent_prompt", "agent_switch", "agent_file_save", "agent_run", "agent_shell", "agent_close_shell", "worktree_shell", "worktree_close_shell", "pod_shell", "pod_close_shell", "chain_go"];
/// What writes the settings, packs and tokens: `[remote.peers] settings`.
pub(super) const PEER_SETTINGS: &[&str] = &[
    "save_config",
    "store_token",
    "pack_import",
    "pack_update",
    "pack_remove",
    "reload_collectors",
    "remote_settings_save",
    "mail_sign_in_start",
    "mail_sign_in_wait",
    "mail_sign_out",
    "google_sign_in",
    "google_sign_out",
    "remote_pair_start",
    "remote_pair_confirm",
    "remote_pair_cancel",
    "remote_revoke",
    "remote_rename",
    "remote_restart",
    "remote_release",
    "remote_peer_pair_start",
    "remote_peer_pair_answer",
    "remote_peer_pair_cancel",
    "remote_peer_revoke",
    // What writes the settings or trusts a command to run, by another way.
    "mcp_proposal_answer",
    "mcp_install",
    "routine_trust",
    "routine_remove",
    "quick_action_add",
    "cli_command_install",
    "cli_command_remove",
    "update_install",
];

/// Whether `[remote.peers]` lets another Prism's window call `command` (of class `class`): it
/// always reads; acting, what is critical, typing and the settings each have their switch. A
/// command classed `never` for a phone is the window's to call: acting, unless it types or writes
/// the settings.
/// A command it does not know is refused; a block's command its block keeps from a phone (a custom
/// block tried, saved, trusted: a command run) is a setting's.
pub(super) fn peer_allowed(rights: &prism_core::config::PeerRights, command: &str, class: Option<Class>) -> anyhow::Result<()> {
    let Some(class) = class else { anyhow::bail!("{}", tr!("{command}: not a command of this Prism", command = command)) };
    let (on, what) = if PEER_TYPING.contains(&command) {
        (rights.typing, "typing")
    } else if PEER_SETTINGS.contains(&command) || (command == "block_call" && class == Class::Never) {
        (rights.settings, "settings")
    } else {
        match class {
            Class::Read => return Ok(()),
            Class::Critical => (rights.critical, "critical"),
            _ => (rights.actions, "actions"),
        }
    };
    anyhow::ensure!(on, "{}", tr!("{command}: [remote.peers] {what} is off on that computer", command = command, what = what));
    Ok(())
}

#[derive(Deserialize)]
pub(super) struct PeerClaim {
    code: String,
    name: String,
    claim: String,
}

/// `POST /peer/pair`: a computer gives the code shown here, its name, and a secret of its own.
pub(super) async fn peer_pair(State(inner): State<Shared>, Json(c): Json<PeerClaim>) -> Response {
    let r = match inner.peer_pairing().as_mut() {
        Some(p) => p.claim(&c.code, &c.name, &c.claim, Utc::now()),
        None => Err(anyhow::anyhow!("no pairing open: run prism peer pair there")),
    };
    inner.publish();
    match r {
        Ok(()) => Json(json!({ "state": "waiting" })).into_response(),
        Err(e) => {
            tracing::warn!("remote: a computer's pairing: {e:#}");
            refuse(StatusCode::FORBIDDEN, &format!("{e:#}"))
        }
    }
}

#[derive(Deserialize)]
pub(super) struct PeerClaimState {
    claim: String,
}

/// `POST /peer/pair/state`: what the computer that gave the code reads: waiting, its token (once),
/// or gone.
pub(super) async fn peer_pair_state(State(inner): State<Shared>, Json(c): Json<PeerClaimState>) -> Response {
    let state = {
        let mut p = inner.peer_pairing();
        let s = p.as_mut().map_or(crate::peers::PairState::Gone, |p| p.state(&c.claim, Utc::now()));
        // Its token read: the pairing is over.
        if matches!(s, crate::peers::PairState::Paired(_)) {
            *p = None;
        }
        s
    };
    match state {
        crate::peers::PairState::Waiting => Json(json!({ "state": "waiting" })).into_response(),
        crate::peers::PairState::Paired(token) => {
            inner.publish();
            Json(json!({ "state": "paired", "token": token })).into_response()
        }
        crate::peers::PairState::Gone => Json(json!({ "state": "gone" })).into_response(),
    }
}
