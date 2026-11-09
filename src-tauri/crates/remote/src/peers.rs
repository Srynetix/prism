//! Other Prisms that open this one in their own window (`[[machines]]` on a desk): each with a
//! token made here (`prism peer add <name>`), kept hashed in `remote/peers.json`. A peer's socket
//! (`/peer/ws`) answers every command of the window but its own, and its terminals (`/peer/tty`)
//! are typed in: a token is worth this machine's account, as an SSH key is. Revoked
//! (`prism peer remove <name>`), its sockets close within ten seconds.
//!
//! Paired rather than pasted (`prism peer pair`, or Settings › Remote › Devices): a code of six
//! digits shown here for five minutes, typed on the other computer with its name and a secret of its
//! own; its name accepted here by a person, the token goes to it once, read with that secret. Five
//! wrong codes end the pairing.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use axum::http::{header, HeaderMap};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const FILE: &str = "peers.json";
/// A peer's key among the connected pages (`Inner::connect`), apart from the phones'.
pub(crate) const CONNECTED: &str = "peer:";

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
    prism_core::textdiff::sha256(token)
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

/// A pairing stays open this long.
const PAIR_MINUTES: i64 = 5;
/// Wrong codes before it ends.
const MAX_TRIES: u8 = 5;

/// A computer's pairing under way.
pub(crate) struct Pairing {
    pub code: String,
    pub expires: DateTime<Utc>,
    tries: u8,
    pub request: Option<Request>,
}

/// The computer that gave the code: its name, its secret, and once accepted (or refused) here, the
/// answer it reads.
pub(crate) struct Request {
    pub name: String,
    claim: String,
    answer: Option<Option<String>>,
}

/// What a computer reads of its pairing.
pub(crate) enum PairState {
    Waiting,
    /// Accepted: its token, given this once.
    Paired(String),
    /// Refused, expired, cancelled, or not its own.
    Gone,
}

impl Pairing {
    pub fn new(now: DateTime<Utc>) -> Self {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).expect("the system's random source");
        let code = format!("{:06}", u32::from_le_bytes(bytes) % 1_000_000);
        Self { code, expires: now + chrono::Duration::minutes(PAIR_MINUTES), tries: 0, request: None }
    }

    pub fn open(&self, now: DateTime<Utc>) -> bool {
        now < self.expires && self.tries < MAX_TRIES
    }

    /// A computer gives the code: kept if it is this one's and none came before.
    pub fn claim(&mut self, code: &str, name: &str, claim: &str, now: DateTime<Utc>) -> Result<()> {
        if !self.open(now) {
            bail!("no pairing open: run prism peer pair there");
        }
        if self.request.is_some() {
            bail!("another computer is pairing: try again in a moment");
        }
        let name = name.trim();
        if name.is_empty() || name.len() > 64 || name.chars().any(char::is_control) || claim.len() < 16 {
            bail!("a name and a secret");
        }
        let given: String = code.chars().filter(char::is_ascii_digit).collect();
        // Every digit compared, whatever the first wrong one.
        let same = given.len() == self.code.len() && given.bytes().zip(self.code.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0;
        if !same {
            self.tries += 1;
            bail!("not the code shown there");
        }
        self.request = Some(Request { name: name.to_string(), claim: claim.to_string(), answer: None });
        Ok(())
    }

    /// What the computer holding `claim` reads; the token taken, it is not given again.
    pub fn state(&mut self, claim: &str, now: DateTime<Utc>) -> PairState {
        let Some(r) = self.request.as_mut().filter(|r| r.claim == claim) else { return PairState::Gone };
        match &r.answer {
            Some(Some(token)) => {
                let token = token.clone();
                r.answer = Some(None);
                PairState::Paired(token)
            }
            Some(None) => PairState::Gone,
            None if now < self.expires => PairState::Waiting,
            None => PairState::Gone,
        }
    }

    /// Accepted here, within its five minutes: its token made (`dir`'s peers) under its name, or
    /// one made unique (`MacBook (2)`) when another computer has it, which keeps its own; or refused.
    pub fn answer(&mut self, dir: &Path, accept: bool, now: DateTime<Utc>) -> Result<()> {
        if now >= self.expires {
            bail!("the pairing expired: open another");
        }
        let Some(r) = self.request.as_mut().filter(|r| r.answer.is_none()) else { bail!("no computer to accept") };
        if accept {
            let taken: Vec<String> = list(dir)?.into_iter().map(|p| p.name).collect();
            let base = r.name.clone();
            let mut n = 1;
            while taken.contains(&r.name) {
                n += 1;
                r.name = format!("{base} ({n})");
            }
        }
        r.answer = Some(if accept { Some(add(dir, &r.name)?) } else { None });
        Ok(())
    }

    /// The name it was accepted under.
    pub fn accepted_as(&self) -> Option<&str> {
        self.request.as_ref().filter(|r| matches!(r.answer, Some(Some(_)))).map(|r| r.name.as_str())
    }

    /// Done with: its answer read, or refused.
    pub fn done(&self) -> bool {
        self.request.as_ref().is_some_and(|r| r.answer.is_some())
    }
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

    /// A pairing: the code, five tries, the name accepted, the token read once by its claim only.
    #[test]
    fn a_computer_pairs_with_the_code_shown() {
        let dir = std::env::temp_dir().join(format!("prism-peers-pair-{}", std::process::id()));
        let now = chrono::Utc::now();
        let mut p = super::Pairing::new(now);
        assert_eq!(p.code.len(), 6);
        let claim = "a-secret-of-its-own-1234";
        assert!(p.claim("000000x", "mac", claim, now).is_err() || p.code == "000000");
        let code = p.code.clone();
        p.claim(&code, "MacBook", claim, now).unwrap();
        assert!(p.claim(&code, "Other", "another-secret-5678901", now).is_err(), "one computer at a time");
        assert!(matches!(p.state("not-its-claim-0000000", now), super::PairState::Gone));
        assert!(matches!(p.state(claim, now), super::PairState::Waiting));
        p.answer(&dir, true, now).unwrap();
        let super::PairState::Paired(token) = p.state(claim, now) else { panic!("paired") };
        assert_eq!(super::check(&dir, &token).as_deref(), Some("MacBook"));
        assert!(matches!(p.state(claim, now), super::PairState::Gone), "given once");
        // Another computer of the same name: its own, the first one kept.
        let mut other = super::Pairing::new(now);
        let code = other.code.clone();
        other.claim(&code, "MacBook", claim, now).unwrap();
        other.answer(&dir, true, now).unwrap();
        assert_eq!(other.accepted_as(), Some("MacBook (2)"));
        assert_eq!(super::check(&dir, &token).as_deref(), Some("MacBook"), "the first one still opens");
        // Accepted too late: nothing made.
        let mut late = super::Pairing::new(now);
        let code = late.code.clone();
        late.claim(&code, "Late", claim, now).unwrap();
        assert!(late.answer(&dir, true, now + chrono::Duration::minutes(6)).is_err());
        assert!(!super::list(&dir).unwrap().iter().any(|p| p.name == "Late"));
        // Five wrong codes end it.
        let mut q = super::Pairing::new(now);
        for _ in 0..5 {
            let _ = q.claim("abc", "x", claim, now);
        }
        assert!(!q.open(now));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
