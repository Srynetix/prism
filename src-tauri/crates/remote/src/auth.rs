//! Who may talk to the remote: the paired phones (kept on disk), the pairing under way, the
//! login challenges, the open sessions. No password anywhere: a phone is paired from the desk,
//! then unlocks with its passkey.
//!
//! Pairing: the desk opens it (a one-time token, 2 minutes, shown as a QR code); the phone opens
//! the address, gives its name and gets a code to show and a challenge; it creates its passkey;
//! the desk confirms the code; the phone, polling, then gets its first session. A photographed or
//! relayed QR code pairs nothing without the desk's confirmation.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{ensure, Context, Result};
use prism_base::{i18n::tr, tr};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::webauthn::{self, Assertion, Credential, Party, Registration};

/// How long a pairing's token, and a login challenge, stay valid.
const PAIRING_TTL: Duration = Duration::minutes(2);
/// The open sessions, next to `devices.json`.
const SESSIONS: &str = "sessions.json";
/// A pairing's claim as the desk shows it: the phone's code and name, and whether its passkey is there.
pub type ClaimShown = (String, String, bool);

/// A session's token as kept: its SHA-256, the token itself only in the phone's cookie.
fn key(token: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}
const CHALLENGE_TTL: Duration = Duration::minutes(2);
/// Failed confirmations of a phone tolerated in a minute before its confirmations are refused for
/// a minute. Per phone, behind its session: a stranger cannot spend them. Logins have none: a
/// passkey's signature cannot be guessed, and a count anyone could fill would lock its owner out.
const MAX_FAILURES: usize = 5;
/// Login challenges kept at once: past that, the oldest goes (a stranger asking for challenges
/// in a loop wears out its own, never refuses the next login).
const MAX_CHALLENGES: usize = 256;

/// A paired phone, as kept.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub credential: Credential,
    pub paired_at: DateTime<Utc>,
    #[serde(default)]
    pub last_seen: Option<DateTime<Utc>>,
    /// Its push subscription's endpoint (`[remote] push`), one of the push services'.
    #[serde(default)]
    pub push: Option<String>,
}

#[derive(Debug)]
struct Pairing {
    token: String,
    expires: DateTime<Utc>,
    claim: Option<Claim>,
}

#[derive(Debug)]
struct Claim {
    name: String,
    code: String,
    challenge: String,
    credential: Option<Credential>,
    /// Set once the desk confirmed: the phone's first session, handed over once.
    session: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub device: String,
    created: DateTime<Utc>,
    last: DateTime<Utc>,
}

/// What the phone learns when it polls its pairing.
#[derive(Debug, PartialEq, Eq)]
pub enum PairState {
    Waiting,
    Paired { session: String },
    Gone,
}

pub struct Auth {
    file: PathBuf,
    devices: Vec<Device>,
    pairing: Option<Pairing>,
    challenges: HashMap<String, DateTime<Utc>>,
    /// The open sessions, by their token's hash (`key`): the file holds no cookie a process of the
    /// user's could replay.
    sessions: HashMap<String, Session>,
    /// When they were last written: their activity is, every few minutes.
    sessions_saved: DateTime<Utc>,
    /// Failed confirmations, per phone.
    failures: HashMap<String, Vec<DateTime<Utc>>>,
    /// Step-ups under way: their challenge → the phone, what it confirms, until when.
    step_ups: HashMap<String, (String, String, DateTime<Utc>)>,
    /// The last step-up each phone passed, for what, when: the terminal's control within
    /// `step_up` (`[remote] step_up_minutes`).
    passed: HashMap<String, (String, DateTime<Utc>)>,
    pub idle: Duration,
    pub max: Duration,
    pub step_up: Duration,
}

impl Auth {
    /// The phones paired in `dir` (`devices.json`); none if the file is missing.
    pub fn load(dir: &Path) -> Result<Self> {
        let file = dir.join("devices.json");
        let devices = match std::fs::read(&file) {
            Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| tr!("reading {path}", path = file.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e).with_context(|| tr!("reading {path}", path = file.display())),
        };
        // The open sessions, kept across Prism's restarts: a phone that relaunched it reconnects by
        // itself. Their last activity as saved (a few minutes behind at most): Prism quit longer
        // than their idle time, they are over.
        let sessions: HashMap<String, Session> = std::fs::read(dir.join(SESSIONS)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Ok(Self { file, devices, pairing: None, challenges: HashMap::new(), sessions, sessions_saved: Utc::now(), failures: HashMap::new(), step_ups: HashMap::new(), passed: HashMap::new(), step_up: Duration::minutes(5), idle: Duration::hours(2), max: Duration::hours(12) })
    }

    fn save(&self) -> Result<()> {
        let dir = self.file.parent().context("devices file without a folder")?;
        std::fs::create_dir_all(dir)?;
        // The public keys are no secret, but whoever can add one here gets in: the owner only.
        prism_base::files::write_private(&self.file, serde_json::to_vec_pretty(&self.devices)?).with_context(|| tr!("writing {path}", path = self.file.display()))?;
        self.save_sessions();
        Ok(())
    }

    /// The open sessions (`sessions.json`, owner-only, their tokens hashed).
    fn save_sessions(&self) {
        let file = self.file.with_file_name(SESSIONS);
        let written = serde_json::to_vec(&self.sessions).map_err(anyhow::Error::from).and_then(|b| prism_base::files::write_private(&file, b).map_err(Into::into));
        if let Err(e) = written {
            tracing::warn!("remote: writing {}: {e:#}", file.display());
        }
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    /// Whether a session of `device` is open now.
    pub fn has_session(&self, device: &str) -> bool {
        self.sessions.values().any(|s| s.device == device)
    }

    // Pairing, on the desk's side.

    /// Opens a pairing (any other one is dropped): its token and when it expires.
    pub fn pair_start(&mut self, now: DateTime<Utc>) -> (String, DateTime<Utc>) {
        let token = webauthn::random();
        let expires = now + PAIRING_TTL;
        self.pairing = Some(Pairing { token: token.clone(), expires, claim: None });
        (token, expires)
    }

    pub fn pair_cancel(&mut self) {
        self.pairing = None;
    }

    /// The pairing as the desk shows it: its token and expiry, then its claim.
    pub fn pairing(&mut self, now: DateTime<Utc>) -> Option<(String, DateTime<Utc>, Option<ClaimShown>)> {
        self.expire(now);
        let p = self.pairing.as_ref()?;
        Some((p.token.clone(), p.expires, p.claim.as_ref().map(|c| (c.code.clone(), c.name.clone(), c.credential.is_some()))))
    }

    /// The desk confirms the code the phone shows: the phone is paired and gets its session.
    /// `code`: the one the confirmer was shown (a terminal's), which the pairing must still be.
    pub fn pair_confirm(&mut self, now: DateTime<Utc>, code: Option<&str>) -> Result<()> {
        self.expire(now);
        let claim = self.pairing.as_mut().and_then(|p| p.claim.as_mut()).context(tr("no pairing to confirm"))?;
        ensure!(code.is_none_or(|c| c == claim.code), tr("this pairing changed since: open a new one"));
        let credential = claim.credential.clone().context(tr("the phone has not created its passkey yet"))?;
        ensure!(!self.devices.iter().any(|d| d.credential.id == credential.id), "this passkey is already paired");
        let device = Device { id: webauthn::random()[..12].to_string(), name: claim.name.clone(), credential, paired_at: now, last_seen: Some(now), push: None };
        let session = webauthn::random();
        self.sessions.insert(key(&session), Session { device: device.id.clone(), created: now, last: now });
        claim.session = Some(session);
        self.devices.insert(0, device);
        // A pairing confirmed stays only for the phone to pick its session up, briefly.
        if let Some(p) = self.pairing.as_mut() {
            p.expires = p.expires.min(now + Duration::seconds(30));
        }
        self.save()
    }

    /// Forgets a phone: its passkey no longer opens anything, its sessions end.
    pub fn revoke(&mut self, device: &str) -> Result<()> {
        let before = self.devices.len();
        self.devices.retain(|d| d.id != device);
        self.sessions.retain(|_, s| s.device != device);
        ensure!(self.devices.len() < before, "no such phone");
        self.save()
    }

    /// Renames a phone (the desk's Settings): its passkey and sessions stay.
    pub fn rename(&mut self, device: &str, name: &str) -> Result<()> {
        let name = device_name(name);
        ensure!(!name.is_empty(), "a name is needed");
        let d = self.devices.iter_mut().find(|d| d.id == device).context(tr("no such phone"))?;
        d.name = name;
        self.save()
    }

    /// Every session ends (the desk's Stop): the phones unlock again later.
    pub fn end_sessions(&mut self) {
        self.sessions.clear();
        self.save_sessions();
    }

    // Pairing, on the phone's side.

    /// The phone opened the pairing's address: the code to show, and its passkey's challenge.
    pub fn pair_claim(&mut self, token: &str, name: &str, now: DateTime<Utc>) -> Result<(String, String)> {
        self.expire(now);
        let p = self.pairing.as_mut().filter(|p| p.token == token).context(tr("this pairing has expired: open a new one on the Mac"))?;
        ensure!(p.claim.is_none(), "this pairing is already in use: open a new one on the Mac");
        let name = device_name(name);
        let mut digits = [0u8; 2];
        getrandom::fill(&mut digits).expect("the system's random source");
        let code = format!("{:04}", u16::from_le_bytes(digits) % 10_000);
        let challenge = webauthn::random();
        p.claim = Some(Claim { name: if name.is_empty() { "Phone".into() } else { name }, code: code.clone(), challenge: challenge.clone(), credential: None, session: None });
        Ok((code, challenge))
    }

    /// The phone's new passkey, checked and kept until the desk confirms.
    pub fn pair_finish(&mut self, token: &str, reg: &Registration, party: &Party, now: DateTime<Utc>) -> Result<()> {
        self.expire(now);
        let claim = self.pairing.as_mut().filter(|p| p.token == token).and_then(|p| p.claim.as_mut()).context(tr("this pairing has expired"))?;
        ensure!(claim.credential.is_none(), "this pairing already has its passkey");
        claim.credential = Some(webauthn::register(reg, &claim.challenge, party)?);
        Ok(())
    }

    /// The phone polls its pairing: once the desk confirmed, its session, handed over once — to
    /// the phone that claimed it, which alone knows its challenge (`claim`), not to whoever saw
    /// the QR code.
    pub fn pair_state(&mut self, token: &str, claim: &str, now: DateTime<Utc>) -> PairState {
        self.expire(now);
        let Some(p) = self.pairing.as_mut().filter(|p| p.token == token) else { return PairState::Gone };
        let Some(c) = p.claim.as_mut().filter(|c| c.challenge == claim) else { return PairState::Gone };
        match c.session.take() {
            Some(session) => {
                self.pairing = None;
                PairState::Paired { session }
            }
            None => PairState::Waiting,
        }
    }

    // Login.

    /// A login challenge, and the passkeys that may answer it.
    pub fn login_begin(&mut self, now: DateTime<Utc>) -> Result<(String, Vec<String>)> {
        self.expire(now);
        ensure!(!self.devices.is_empty(), "no phone is paired: pair this one from the Mac");
        if self.challenges.len() >= MAX_CHALLENGES
            && let Some(oldest) = self.challenges.iter().min_by_key(|(_, until)| **until).map(|(c, _)| c.clone())
        {
            self.challenges.remove(&oldest);
        }
        let challenge = webauthn::random();
        self.challenges.insert(challenge.clone(), now + CHALLENGE_TTL);
        Ok((challenge, self.devices.iter().map(|d| d.credential.id.clone()).collect()))
    }

    /// A passkey answered: a new session for its phone.
    pub fn login_finish(&mut self, assertion: &Assertion, party: &Party, now: DateTime<Utc>) -> Result<(String, String)> {
        self.expire(now);
        let challenge = webauthn::challenge_of(&assertion.client_data_json)?;
        // Single use, whatever comes next.
        ensure!(self.challenges.remove(&challenge).is_some(), "an unknown or expired challenge");
        let device = self.devices.iter_mut().find(|d| d.credential.id == assertion.credential_id).context("this passkey is not paired (anymore)")?;
        device.credential.sign_count = webauthn::verify(assertion, &device.credential, &challenge, party)?;
        device.last_seen = Some(now);
        let id = device.id.clone();
        let session = webauthn::random();
        self.sessions.insert(key(&session), Session { device: id.clone(), created: now, last: now });
        self.save()?;
        Ok((session, id))
    }

    /// The phone a session belongs to, if it is still open; it counts as activity.
    pub fn check(&mut self, token: &str, now: DateTime<Utc>) -> Option<String> {
        self.expire(now);
        let s = self.sessions.get_mut(&key(token))?;
        s.last = now;
        let device = s.device.clone();
        if let Some(d) = self.devices.iter_mut().find(|d| d.id == device) {
            d.last_seen = Some(now);
        }
        if now - self.sessions_saved > Duration::minutes(5) {
            self.sessions_saved = now;
            self.save_sessions();
        }
        Some(device)
    }

    /// Whether a session is still open, without counting as activity: a page that only
    /// watches does not keep its session alive.
    pub fn alive(&mut self, token: &str, now: DateTime<Utc>) -> bool {
        self.expire(now);
        self.sessions.contains_key(&key(token))
    }

    // Step-up: biometrics again, for one action.

    /// A challenge for `device`'s passkey, bound to `purpose` (a critical command and its
    /// arguments, or a terminal's control): answered, it confirms that and nothing else.
    pub fn step_up_begin(&mut self, device: &str, purpose: &str, now: DateTime<Utc>) -> Result<(String, Vec<String>)> {
        self.expire(now);
        ensure!(self.failed(device) < MAX_FAILURES, "too many failed confirmations: wait a minute");
        let d = self.devices.iter().find(|d| d.id == device).context(tr("this phone is not paired (anymore)"))?;
        ensure!(self.step_ups.len() < 64, "too many confirmations under way");
        let challenge = webauthn::random();
        self.step_ups.insert(challenge.clone(), (device.to_string(), purpose.to_string(), now + CHALLENGE_TTL));
        Ok((challenge, vec![d.credential.id.clone()]))
    }

    /// The phone's passkey answered a step-up: for this phone, this purpose, by its own passkey.
    pub fn step_up_finish(&mut self, device: &str, purpose: &str, assertion: &Assertion, party: &Party, now: DateTime<Utc>) -> Result<()> {
        self.expire(now);
        ensure!(self.failed(device) < MAX_FAILURES, "too many failed confirmations: wait a minute");
        let r = (|| {
            let challenge = webauthn::challenge_of(&assertion.client_data_json)?;
            let (who, what, _) = self.step_ups.remove(&challenge).context(tr("an unknown or expired confirmation"))?;
            ensure!(who == device && what == purpose, "this confirmation was for something else");
            let d = self.devices.iter_mut().find(|d| d.id == device).context(tr("this phone is not paired (anymore)"))?;
            d.credential.sign_count = webauthn::verify(assertion, &d.credential, &challenge, party)?;
            self.passed.insert(device.to_string(), (purpose.to_string(), now));
            self.save()
        })();
        if r.is_err() {
            self.failures.entry(device.to_string()).or_default().push(now);
        }
        r
    }

    /// What the step-up `assertion` answers was asked for: the relay checks a phone's answer to a
    /// confirmation another profile's Prism asked for (`relay`), bound to that purpose.
    pub fn step_up_purpose(&self, device: &str, assertion: &Assertion) -> Option<String> {
        let challenge = webauthn::challenge_of(&assertion.client_data_json).ok()?;
        self.step_ups.get(&challenge).filter(|(who, _, _)| who == device).map(|(_, what, _)| what.clone())
    }

    /// Tunnel `relay`: `device` passed a step-up for `purpose`, checked by the relay with the
    /// phone's passkey; good here for `step_up` as one checked here would be.
    pub fn confirmed(&mut self, device: &str, purpose: &str, now: DateTime<Utc>) {
        self.passed.insert(device.to_string(), (purpose.to_string(), now));
    }

    /// `device`'s failed confirmations of the last minute.
    fn failed(&self, device: &str) -> usize {
        self.failures.get(device).map_or(0, Vec::len)
    }

    /// `device` passed a step-up for `purpose` within `step_up`.
    pub fn stepped_up(&self, device: &str, purpose: &str, now: DateTime<Utc>) -> bool {
        self.passed.get(device).is_some_and(|(what, at)| what == purpose && now - *at < self.step_up)
    }

    /// The phone's push subscription, or none.
    pub fn set_push(&mut self, device: &str, endpoint: Option<String>) -> Result<()> {
        let d = self.devices.iter_mut().find(|d| d.id == device).context(tr("this phone is not paired (anymore)"))?;
        if let Some(e) = &endpoint {
            crate::push::allowed(e)?;
        }
        d.push = endpoint;
        self.save()
    }

    pub fn logout(&mut self, token: &str) {
        if self.sessions.remove(&key(token)).is_some() {
            self.save_sessions();
        }
    }

    fn expire(&mut self, now: DateTime<Utc>) {
        if self.pairing.as_ref().is_some_and(|p| p.expires <= now) {
            self.pairing = None;
        }
        self.challenges.retain(|_, until| *until > now);
        self.step_ups.retain(|_, (_, _, until)| *until > now);
        let (idle, max) = (self.idle, self.max);
        let devices = &self.devices;
        let before = self.sessions.len();
        self.sessions.retain(|_, s| now - s.last < idle && now - s.created < max && devices.iter().any(|d| d.id == s.device));
        if self.sessions.len() != before {
            self.save_sessions();
        }
        for times in self.failures.values_mut() {
            times.retain(|t| now - *t < Duration::minutes(1));
        }
        self.failures.retain(|_, times| !times.is_empty());
    }
}

/// A name for the folder of a test.
/// A phone's name as kept: trimmed, without control characters, 40 characters at most.
fn device_name(name: &str) -> String {
    name.trim().chars().filter(|c| !c.is_control()).take(40).collect::<String>().trim_end().to_string()
}

#[cfg(test)]
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("prism-remote-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webauthn::tests::Authenticator;

    fn party() -> Party {
        Party::of("https://mac.tail1234.ts.net").unwrap()
    }

    /// A phone paired through the whole ceremony; its session.
    fn pair(auth: &mut Auth, phone: &Authenticator, now: DateTime<Utc>) -> String {
        let (token, _) = auth.pair_start(now);
        let (_, challenge) = auth.pair_claim(&token, "Pixel 9", now).unwrap();
        auth.pair_finish(&token, &phone.create(&challenge, &party()), &party(), now).unwrap();
        assert_eq!(auth.pair_state(&token, &challenge, now), PairState::Waiting);
        auth.pair_confirm(now, None).unwrap();
        assert_eq!(auth.pair_state(&token, "", now), PairState::Gone, "whoever saw the QR code gets nothing");
        match auth.pair_state(&token, &challenge, now) {
            PairState::Paired { session } => session,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn pairing_needs_the_desk_then_the_phone_logs_in() {
        let dir = scratch("pair");
        let now = Utc::now();
        let mut auth = Auth::load(&dir).unwrap();
        let phone = Authenticator::new();
        let session = pair(&mut auth, &phone, now);
        let device = auth.check(&session, now).unwrap();
        assert_eq!(auth.devices()[0].name, "Pixel 9");
        // Kept: another Prism finds it.
        let mut again = Auth::load(&dir).unwrap();
        let (challenge, allowed) = again.login_begin(now).unwrap();
        assert_eq!(allowed, [auth.devices()[0].credential.id.clone()]);
        let (s2, d2) = again.login_finish(&phone.get(&challenge, &party()), &party(), now).unwrap();
        assert_eq!((again.check(&s2, now), d2), (Some(device.clone()), device.clone()));
        // The challenge was single use.
        assert!(again.login_finish(&phone.get(&challenge, &party()), &party(), now).is_err());
        // Its sessions too, owner-only: Prism relaunched, the phone reconnects without its passkey.
        let later = Utc::now() + Duration::minutes(1);
        assert_eq!(Auth::load(&dir).unwrap().check(&session, later), Some(device.clone()), "a session outlives a restart");
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(dir.join(SESSIONS)).unwrap().permissions()) & 0o777, 0o600);
        assert!(!std::fs::read_to_string(dir.join(SESSIONS)).unwrap().contains(&session), "no cookie in the file, its hash");
        assert!(Auth::load(&dir).unwrap().check(&session, now + Duration::hours(3)).is_none(), "Prism quit longer than its idle time: over");
        again.logout(&s2);
        again.end_sessions();
        assert!(Auth::load(&dir).unwrap().check(&session, later).is_none(), "ended, for good");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_pairing_is_single_use_and_expires() {
        let dir = scratch("expire");
        let now = Utc::now();
        let mut auth = Auth::load(&dir).unwrap();
        let (token, _) = auth.pair_start(now);
        assert!(auth.pair_claim("forged", "x", now).is_err());
        auth.pair_claim(&token, "Pixel", now).unwrap();
        assert!(auth.pair_claim(&token, "Other", now).is_err(), "a second phone on the same QR code");
        assert!(auth.pair_confirm(now, None).is_err(), "no passkey yet");
        let (token, _) = auth.pair_start(now);
        assert!(auth.pair_claim(&token, "Late", now + Duration::minutes(3)).is_err());
        assert_eq!(auth.pair_state(&token, "", now + Duration::minutes(3)), PairState::Gone);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sessions_end_when_idle_too_long_or_revoked() {
        let dir = scratch("sessions");
        let now = Utc::now();
        let mut auth = Auth::load(&dir).unwrap();
        auth.idle = Duration::minutes(10);
        auth.max = Duration::hours(1);
        let session = pair(&mut auth, &Authenticator::new(), now);
        assert!(auth.check(&session, now + Duration::minutes(9)).is_some());
        assert!(auth.check(&session, now + Duration::minutes(18)).is_some(), "activity moves the idle limit");
        assert!(auth.alive(&session, now + Duration::minutes(27)), "watching does not count, but it is not over yet");
        assert!(!auth.alive(&session, now + Duration::minutes(29)));
        assert!(auth.check(&session, now + Duration::minutes(29)).is_none());
        let session = pair(&mut auth, &Authenticator { id: b"second".to_vec(), ..Authenticator::new() }, now);
        let device = auth.check(&session, now).unwrap();
        auth.rename(&device, "  Work\n phone  ").unwrap();
        assert_eq!(auth.devices().iter().find(|d| d.id == device).unwrap().name, "Work phone");
        assert!(auth.rename(&device, " \t ").is_err(), "a name is needed");
        assert!(auth.check(&session, now).is_some(), "renaming keeps its session");
        assert_eq!(Auth::load(&dir).unwrap().devices().iter().find(|d| d.id == device).unwrap().name, "Work phone", "saved");
        auth.revoke(&device).unwrap();
        assert!(auth.check(&session, now).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_step_up_confirms_one_purpose_by_the_phones_own_passkey() {
        let dir = scratch("step-up");
        let now = Utc::now();
        let mut auth = Auth::load(&dir).unwrap();
        let phone = Authenticator::new();
        let session = pair(&mut auth, &phone, now);
        let device = auth.check(&session, now).unwrap();
        let (c, allow) = auth.step_up_begin(&device, "merge_mr:abc", now).unwrap();
        assert_eq!(allow.len(), 1);
        // Answered for another purpose: refused, and spent.
        assert!(auth.step_up_finish(&device, "merge_mr:other", &phone.get(&c, &party()), &party(), now).is_err());
        assert!(auth.step_up_finish(&device, "merge_mr:abc", &phone.get(&c, &party()), &party(), now).is_err(), "single use");
        let (c, _) = auth.step_up_begin(&device, "merge_mr:abc", now).unwrap();
        auth.step_up_finish(&device, "merge_mr:abc", &phone.get(&c, &party()), &party(), now).unwrap();
        assert!(auth.stepped_up(&device, "merge_mr:abc", now + Duration::minutes(4)));
        assert!(!auth.stepped_up(&device, "merge_mr:abc", now + Duration::minutes(6)));
        assert!(!auth.stepped_up(&device, "control:a1", now));
        // Another phone's passkey does not answer this phone's confirmation.
        let other = Authenticator { id: b"other".to_vec(), key: p256::ecdsa::SigningKey::from_bytes(&[3u8; 32].into()).unwrap(), ..Authenticator::new() };
        let (c, _) = auth.step_up_begin(&device, "kill", now).unwrap();
        assert!(auth.step_up_finish(&device, "kill", &other.get(&c, &party()), &party(), now).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A stranger's failed logins and piled-up challenges lock no one out; a phone's failed
    /// confirmations lock its own confirmations only, for a minute.
    #[test]
    fn a_stranger_locks_no_one_out() {
        let dir = scratch("throttle");
        let now = Utc::now();
        let mut auth = Auth::load(&dir).unwrap();
        let phone = Authenticator::new();
        pair(&mut auth, &phone, now);
        let stranger = Authenticator { id: b"stranger".to_vec(), ..Authenticator::new() };
        for _ in 0..2 * MAX_CHALLENGES {
            let (c, _) = auth.login_begin(now).unwrap();
            assert!(auth.login_finish(&stranger.get(&c, &party()), &party(), now).is_err());
        }
        let (c, _) = auth.login_begin(now).unwrap();
        let (_, device) = auth.login_finish(&phone.get(&c, &party()), &party(), now).unwrap();
        for _ in 0..MAX_FAILURES {
            let (c, _) = auth.step_up_begin(&device, "kill", now).unwrap();
            assert!(auth.step_up_finish(&device, "kill", &stranger.get(&c, &party()), &party(), now).is_err());
        }
        assert!(auth.step_up_begin(&device, "kill", now).is_err());
        assert!(auth.step_up_begin(&device, "kill", now + Duration::minutes(2)).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }
}
