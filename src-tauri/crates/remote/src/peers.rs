//! Other Prisms that open this one in their own window (`[[machines]]` on a desk): each with a
//! token made here (`prism peer add <name>`), kept hashed in `remote/peers.json`. A peer's socket
//! (`/peer/ws`) answers every command of the window but its own, and its terminals (`/peer/tty`)
//! are typed in: a token is worth this machine's account, as an SSH key is. Revoked
//! (`prism peer remove <name>`), its sockets close within ten seconds.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use axum::http::{header, HeaderMap};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const FILE: &str = "peers.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub name: String,
    /// The token's SHA-256, hex: the token itself is shown once, when made.
    hash: String,
    pub created: DateTime<Utc>,
}

fn file(dir: &Path) -> PathBuf {
    dir.join("remote").join(FILE)
}

/// The peers of the data folder `dir`; none if the file is missing.
pub fn list(dir: &Path) -> Result<Vec<Peer>> {
    match std::fs::read(file(dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| file(dir).display().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e).with_context(|| file(dir).display().to_string()),
    }
}

fn save(dir: &Path, peers: &[Peer]) -> Result<()> {
    std::fs::create_dir_all(dir.join("remote"))?;
    prism_base::files::write_private(&file(dir), serde_json::to_vec_pretty(peers)?).with_context(|| file(dir).display().to_string())
}

fn hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// A new peer `name` (one of that name replaced): its token, shown this once.
pub fn add(dir: &Path, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 64 || name.chars().any(char::is_control) {
        bail!("a peer's name: 1 to 64 characters");
    }
    let token = crate::webauthn::random();
    let mut peers = list(dir)?;
    peers.retain(|p| p.name != name);
    peers.push(Peer { name: name.to_string(), hash: hash(&token), created: Utc::now() });
    save(dir, &peers)?;
    Ok(token)
}

/// Peer `name` revoked: whether there was one.
pub fn remove(dir: &Path, name: &str) -> Result<bool> {
    let mut peers = list(dir)?;
    let before = peers.len();
    peers.retain(|p| p.name != name);
    save(dir, &peers)?;
    Ok(peers.len() != before)
}

/// The peer a token is, if one is (the file read each time: a peer revoked from the terminal is
/// at once).
pub(crate) fn check(dir: &Path, token: &str) -> Option<String> {
    let h = hash(token);
    list(dir).ok()?.into_iter().find(|p| p.hash == h).map(|p| p.name)
}

/// The peer a request comes from: its `Authorization: Bearer <token>`.
pub(crate) fn of(dir: &Path, headers: &HeaderMap) -> Option<(String, String)> {
    let token = headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ")?.trim().to_string();
    check(dir, &token).map(|name| (name, token))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_peer_is_its_token_until_removed() {
        let dir = std::env::temp_dir().join(format!("prism-peers-{}", std::process::id()));
        let dir = &dir;
        let token = super::add(dir, "mac").unwrap();
        assert_eq!(super::check(dir, &token).as_deref(), Some("mac"));
        assert_eq!(super::check(dir, "another"), None);
        // Made again: the old token no longer is.
        let again = super::add(dir, "mac").unwrap();
        assert_eq!(super::check(dir, &token), None);
        assert!(super::remove(dir, "mac").unwrap());
        assert_eq!(super::check(dir, &again), None);
        assert!(super::add(dir, " ").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
