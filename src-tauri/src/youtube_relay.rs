//! The YouTube player's relay. YouTube refuses an embedded player without a `Referer` (error 153),
//! and the app's page, on `tauri://localhost`, sends none. The player is therefore embedded in a
//! page of this relay, on `http://localhost:<port>`, which forwards the app's commands to it and
//! its events to the app (`postMessage`). `localhost`, not `127.0.0.1`: from an address, YouTube
//! refuses some videos (error 150) it plays from a name.
//!
//! Only that static page is served: no command, no data, nothing read from the request but its
//! line and `Host` (a page reaching the port through another name, DNS rebinding, gets a 404).
//! Bound to the loopback (IPv4, and IPv6 when it can, `localhost` resolving to either), on a port
//! the system picks, started at the player's first use.

use std::time::Duration;

use anyhow::{Context, Result};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::OnceCell,
};

static ORIGIN: OnceCell<String> = OnceCell::const_new();

/// `http://localhost:<port>`: where the relay's page is, once started.
pub async fn origin() -> Result<String> {
    ORIGIN
        .get_or_try_init(|| async {
            let listener = TcpListener::bind("127.0.0.1:0").await.context("starting the YouTube relay")?;
            let port = listener.local_addr()?.port();
            tokio::spawn(serve(listener, port));
            // `localhost` may resolve to ::1 first: the same port there, if the system lets it.
            match TcpListener::bind(("::1", port)).await {
                Ok(v6) => {
                    tokio::spawn(serve(v6, port));
                }
                Err(e) => tracing::debug!("youtube: relay without IPv6: {e}"),
            }
            tracing::info!("youtube: relay on localhost:{port}");
            Ok(format!("http://localhost:{port}"))
        })
        .await
        .cloned()
}

async fn serve(listener: TcpListener, port: u16) {
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        tokio::spawn(async move {
            if let Err(e) = tokio::time::timeout(Duration::from_secs(5), answer(stream, port)).await {
                tracing::debug!("youtube: relay request: {e}");
            }
        });
    }
}

async fn answer(mut stream: TcpStream, port: u16) -> Result<()> {
    let mut buf = vec![0u8; 8192];
    let mut len = 0;
    // The request's head: up to its blank line (a GET has no body).
    while len < buf.len() {
        let n = stream.read(&mut buf[len..]).await?;
        if n == 0 {
            break;
        }
        len += n;
        if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf[..len]);
    let response = if serves(&head, port) {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: strict-origin-when-cross-origin\r\nContent-Security-Policy: {CSP}\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{PAGE}",
            PAGE.len()
        )
    } else {
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
    };
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await.ok();
    Ok(())
}

/// A `GET /youtube…` addressed to the loopback by its name or address: anything else is not for the relay.
fn serves(head: &str, port: u16) -> bool {
    let mut lines = head.lines();
    let request = lines.next().unwrap_or_default();
    let mut parts = request.split_whitespace();
    let (method, path) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    let host = lines.find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case("host")).map(|(_, v)| v.trim().to_string()));
    let ours = [format!("localhost:{port}"), format!("127.0.0.1:{port}"), format!("[::1]:{port}")];
    method == "GET" && (path == "/youtube" || path.starts_with("/youtube?")) && host.is_some_and(|h| ours.contains(&h))
}

/// Only its own inline script and style, and YouTube's player in a frame.
const CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; frame-src https://www.youtube.com https://www.youtube-nocookie.com";

/// The relay's page: the player of `?src=` (a YouTube embed URL, nothing else), and the messages
/// passed between it and the app.
const PAGE: &str = r#"<!doctype html>
<meta charset="utf-8">
<meta name="referrer" content="strict-origin-when-cross-origin">
<title>YouTube</title>
<style>html,body{margin:0;height:100%;background:#000;overflow:hidden}iframe{border:0;width:100%;height:100%}</style>
<iframe id="player" allow="autoplay; encrypted-media" referrerpolicy="strict-origin-when-cross-origin"></iframe>
<script>
  const src = new URLSearchParams(location.search).get('src') || ''
  const player = document.getElementById('player')
  if (/^https:\/\/www\.youtube(-nocookie)?\.com\/embed\/[\w-]+\?/.test(src)) {
    const yt = new URL(src).origin
    player.src = src
    // The player only emits its events once subscribed to.
    player.addEventListener('load', () => player.contentWindow.postMessage(JSON.stringify({ event: 'listening', id: 'prism', channel: 'widget' }), yt))
    window.addEventListener('message', (e) => {
      if (e.source === player.contentWindow) parent.postMessage(e.data, '*')
      else if (e.source === parent) player.contentWindow.postMessage(e.data, yt)
    })
  }
</script>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_its_page_by_its_address() {
        let req = |line: &str, host: &str| format!("{line}\r\nHost: {host}\r\nUser-Agent: x\r\n\r\n");
        assert!(serves(&req("GET /youtube?src=x HTTP/1.1", "127.0.0.1:4567"), 4567));
        assert!(serves(&req("GET /youtube?src=x HTTP/1.1", "localhost:4567"), 4567));
        assert!(!serves(&req("GET /youtube HTTP/1.1", "evil.example:4567"), 4567), "DNS rebinding");
        assert!(!serves(&req("GET /youtube HTTP/1.1", "127.0.0.1:1"), 4567));
        assert!(!serves(&req("POST /youtube HTTP/1.1", "127.0.0.1:4567"), 4567));
        assert!(!serves(&req("GET /etc/passwd HTTP/1.1", "127.0.0.1:4567"), 4567));
    }

    #[tokio::test]
    async fn serves_the_page_over_http() {
        let base = origin().await.unwrap();
        assert!(base.starts_with("http://localhost:"));
        let port: u16 = base.rsplit(':').next().unwrap().parse().unwrap();
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(format!("GET /youtube?src=x HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n").as_bytes()).await.unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        assert!(out.starts_with("HTTP/1.1 200") && out.contains("id=\"player\"") && out.contains("Referrer-Policy"));
    }
}
