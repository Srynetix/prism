//! The remote server's tests: its pages, its sessions, its sockets, what a phone may do.

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
    assert!(super::peer_allowed(&r, "merge_mr", &Value::Null, Some(Class::Critical)).is_ok());
    r = prism_core::config::PeerRights { actions: false, critical: false, typing: false, settings: false };
    assert!(super::peer_allowed(&r, "mr_detail", &Value::Null, Some(Class::Read)).is_ok());
    assert!(super::peer_allowed(&r, "read_config", &Value::Null, Some(Class::Never)).is_err(), "never for a phone: acting, unless read");
    assert!(super::peer_allowed(&r, "rebase_mr", &Value::Null, Some(Class::Act)).is_err());
    assert!(super::peer_allowed(&r, "merge_mr", &Value::Null, Some(Class::Critical)).is_err());
    assert!(super::peer_allowed(&r, "agent_type", &Value::Null, Some(Class::Critical)).unwrap_err().to_string().contains("typing"));
    assert!(super::peer_allowed(&r, "save_config", &Value::Null, Some(Class::Never)).unwrap_err().to_string().contains("settings"));
    // Acting allowed, the settings not: what writes them or trusts a command by another way.
    r.actions = true;
    r.critical = true;
    for command in ["save_config_text", "pack_import", "mcp_proposal_answer", "routine_trust", "mcp_install", "cli_command_install", "quick_action_add"] {
        assert!(super::peer_allowed(&r, command, &Value::Null, Some(Class::Never)).unwrap_err().to_string().contains("settings"), "{command}");
    }
    assert!(super::peer_allowed(&r, "block_call", &json!({ "block": "custom_blocks", "command": "try" }), Some(Class::Never)).unwrap_err().to_string().contains("settings"), "a custom block tried");
    assert!(super::peer_allowed(&r, "block_call", &json!({ "block": "changes", "command": "alerts" }), Some(Class::Never)).is_ok(), "the window's alerts fed: acting");
    assert!(super::peer_allowed(&r, "block_call", &Value::Null, Some(Class::Act)).is_ok());
    assert!(super::peer_allowed(&r, "no_such_command", &Value::Null, None).is_err());
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
    // Nor the modules (the team's code) without one.
    for path in ["/modules/index.json", "/modules/hello/defs.js"] {
        let modules = phone.http.get(format!("{}{path}", phone.base)).header("host", format!("localhost:{port}")).send().await.unwrap();
        assert_eq!(modules.status(), 401, "{path}: no session");
    }

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

    // With the session, the modules' list (a missing module's file is not found, not refused).
    let modules = phone.http.get(format!("{}/modules/index.json", phone.base)).header("host", format!("localhost:{port}")).header("cookie", phone.cookie.clone().unwrap()).send().await.unwrap();
    assert_eq!(modules.status(), 200);
    assert!(modules.json::<Value>().await.unwrap().is_array());
    let missing = phone.http.get(format!("{}/modules/no-such-module/defs.js", phone.base)).header("host", format!("localhost:{port}")).header("cookie", phone.cookie.clone().unwrap()).send().await.unwrap();
    assert_eq!(missing.status(), 404);

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

/// The remote on the demo's data, to look at its page in a browser: `cargo test -p prism-remote
/// remote_preview -- --ignored --nocapture`, after `pnpm --dir web build`. A phone is paired by
/// itself; the session's cookie to set in the browser is printed.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn remote_preview() {
    use chrono::Utc;

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
