//! The Tailscale tunnel: `tailscale serve` brings HTTPS on the Mac's tailnet name to Prism's
//! loopback port, and tells Prism who is asking (`Tailscale-User-Login`).

use std::ffi::OsString;

use anyhow::{bail, Context, Result};
use prism_base::{i18n::tr, tr};
use serde::Deserialize;
use serde_json::Value;

/// The Mac as its tailnet knows it.
#[derive(Debug, Clone)]
pub struct Node {
    /// `https://mac.tail1234.ts.net`.
    pub url: String,
    /// The tailnet user owning this Mac: the only one let in.
    pub owner: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Status {
    backend_state: String,
    #[serde(rename = "Self")]
    me: Option<Peer>,
    #[serde(default)]
    user: Option<std::collections::HashMap<String, User>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Peer {
    #[serde(rename = "DNSName")]
    dns_name: String,
    #[serde(rename = "UserID")]
    user_id: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct User {
    login_name: String,
}

/// How Prism talks to Tailscale: the app's daemon, or a `tailscaled` of its own through its
/// socket (`[remote] tailscale_socket`, `just tailscale`), with Homebrew's CLI.
#[derive(Debug, Clone, Default)]
pub struct Cli {
    pub socket: Option<String>,
}

impl Cli {
    fn program(&self) -> OsString {
        const APP: &str = "/Applications/Tailscale.app/Contents/MacOS/Tailscale";
        // With its own socket, Homebrew's CLI: the app's (`/usr/local/bin/tailscale`) only talks to the app.
        if self.socket.is_some() {
            for p in ["/opt/homebrew/bin/tailscale", "/usr/local/opt/tailscale/bin/tailscale", "/home/linuxbrew/.linuxbrew/bin/tailscale"] {
                if std::path::Path::new(p).exists() {
                    return p.into();
                }
            }
        } else if cfg!(target_os = "macos") && which("tailscale").is_none() && std::path::Path::new(APP).exists() {
            return APP.into();
        }
        "tailscale".into()
    }

    async fn run(&self, args: &[&str]) -> Result<String> {
        let mut cmd = prism_base::platform::command(self.program());
        if let Some(socket) = &self.socket {
            cmd.arg(format!("--socket={socket}"));
        }
        // A daemon that does not answer must not hold Prism (its quitting waits for `serve off`).
        cmd.kill_on_drop(true);
        let out = tokio::time::timeout(std::time::Duration::from_secs(10), cmd.args(args).output())
            .await
            .map_err(|_| anyhow::anyhow!(tr!("tailscale {args} did not answer in 10 s", args = args.join(" "))))?
            .with_context(|| tr!("tailscale not found ({hint})", hint = prism_base::platform::install_hint("tailscale")))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            if self.socket.is_some() && (err.contains("connect") || err.contains("no such file")) {
                bail!(tr!("tailscaled is not running on {socket}: just tailscale", socket = self.socket.as_deref().unwrap_or_default()));
            }
            bail!("tailscale {}: {}", args.join(" "), err.trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

fn which(name: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|p| p.join(name)).find(|p| p.is_file()))
}

/// This Mac on its tailnet, which must be up.
pub async fn node(cli: &Cli) -> Result<Node> {
    let status: Status = serde_json::from_str(&cli.run(&["status", "--json"]).await?).context("tailscale status")?;
    if status.backend_state != "Running" {
        bail!(tr!("Tailscale is not connected ({state}): {hint}", state = status.backend_state, hint = if cli.socket.is_some() { "just tailscale" } else { tr("start it on the Mac") }));
    }
    let me = status.me.context("tailscale status without this Mac")?;
    let host = me.dns_name.trim_end_matches('.');
    if host.is_empty() {
        bail!(tr("this Mac has no tailnet name: turn MagicDNS on in Tailscale's admin console"));
    }
    let owner = status.user.and_then(|u| u.get(&me.user_id.to_string()).map(|u| u.login_name.clone())).context("tailscale status without this Mac's user")?;
    Ok(Node { url: format!("https://{host}"), owner })
}

/// What `tailscale serve` sends `https://<host>:<https>/` to, if anything.
fn served(status: &Value, host: &str, https: u16) -> Option<String> {
    status.get("Web")?.get(format!("{host}:{https}"))?.get("Handlers")?.get("/")?.get("Proxy")?.as_str().map(String::from)
}

fn target(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// `tailscale serve` on, to Prism's port; refused if port 443 serves something else already.
pub async fn serve(cli: &Cli, node: &Node, port: u16) -> Result<()> {
    serve_on(cli, node, 443, port).await
}

/// `tailscale serve` on the tailnet's HTTPS port `https`, to the loopback `port`; refused if it
/// serves something else already.
pub async fn serve_on(cli: &Cli, node: &Node, https: u16, port: u16) -> Result<()> {
    let host = node.url.trim_start_matches("https://");
    let status: Value = serde_json::from_str(&cli.run(&["serve", "status", "--json"]).await?).unwrap_or_default();
    match served(&status, host, https) {
        Some(t) if t == target(port) => return Ok(()),
        Some(other) => bail!(tr!("tailscale serve already sends {host}:{https} to {other}: turn it off, or choose the tunnel other", host = host, https = https, other = other)),
        None => {}
    }
    cli.run(&["serve", "--bg", &format!("--https={https}"), &target(port)]).await.map(|_| ())
}

/// `tailscale serve` off, if it is still Prism's.
pub async fn unserve(cli: &Cli, node: &Node, port: u16) {
    unserve_on(cli, node, 443, port).await
}

/// `tailscale serve` off on the tailnet's HTTPS port `https`, if it is still Prism's.
pub async fn unserve_on(cli: &Cli, node: &Node, https: u16, port: u16) {
    let host = node.url.trim_start_matches("https://");
    let Ok(text) = cli.run(&["serve", "status", "--json"]).await else { return };
    let status: Value = serde_json::from_str(&text).unwrap_or_default();
    if served(&status, host, https).as_deref() == Some(target(port).as_str())
        && let Err(e) = cli.run(&["serve", &format!("--https={https}"), "off"]).await
    {
        tracing::warn!("remote: {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_port_443_serves() {
        let status = serde_json::json!({ "Web": { "mac.tail1.ts.net:443": { "Handlers": { "/": { "Proxy": "http://127.0.0.1:47800" } } } } });
        assert_eq!(served(&status, "mac.tail1.ts.net", 443).as_deref(), Some("http://127.0.0.1:47800"));
        assert_eq!(served(&status, "mac.tail1.ts.net", 8443), None);
        assert_eq!(served(&status, "other.tail1.ts.net", 443), None);
        assert_eq!(served(&serde_json::json!({}), "mac.tail1.ts.net", 443), None);
    }
}
