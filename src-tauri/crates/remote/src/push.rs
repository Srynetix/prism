//! `[remote] push`: a phone woken when an agent waits for it. The push carries nothing (no
//! payload, so nothing of Prism goes through Google's or Apple's push service): the phone's
//! service worker shows a plain "an agent is waiting" and opens Prism. Signed with this Prism's
//! VAPID key (ES256), made once and kept beside the paired phones.

use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use p256::ecdsa::{signature::Signer, Signature, SigningKey};

/// The push services a subscription may name: Prism posts to these only, never to an address a
/// page would choose.
const SERVICES: &[&str] = &["fcm.googleapis.com", "updates.push.services.mozilla.com", "push.services.mozilla.com", "web.push.apple.com", "notify.windows.com"];

pub struct Vapid {
    key: SigningKey,
}

impl Vapid {
    /// This Prism's key, from `dir` (`vapid.key`), made on first use.
    pub fn load(dir: &Path) -> Result<Self> {
        let file: PathBuf = dir.join("vapid.key");
        if let Ok(raw) = std::fs::read_to_string(&file) {
            let bytes = B64.decode(raw.trim()).context("vapid.key")?;
            return Ok(Self { key: SigningKey::from_slice(&bytes).context("vapid.key")? });
        }
        let key = loop {
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes).expect("the system's random source");
            if let Ok(k) = SigningKey::from_slice(&bytes) {
                break k;
            }
        };
        std::fs::create_dir_all(dir)?;
        // Created the owner's only: never readable by others, even a moment.
        let mut open = std::fs::OpenOptions::new();
        open.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            open.mode(0o600);
            // A file already there keeps its mode: set it too.
            if file.exists() {
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
            }
        }
        std::io::Write::write_all(&mut open.open(&file)?, B64.encode(key.to_bytes()).as_bytes())?;
        Ok(Self { key })
    }

    /// The public key a page subscribes with (`applicationServerKey`).
    pub fn public(&self) -> String {
        B64.encode(self.key.verifying_key().to_sec1_point(false).as_bytes())
    }

    /// The `Authorization` header for `endpoint`: a JWT for its origin, valid 12 hours.
    fn authorization(&self, endpoint: &url::Url) -> String {
        let aud = endpoint.origin().ascii_serialization();
        let exp = chrono::Utc::now().timestamp() + 12 * 3600;
        let header = B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = B64.encode(serde_json::json!({ "aud": aud, "exp": exp, "sub": "mailto:prism@localhost" }).to_string());
        let signing = format!("{header}.{claims}");
        let sig: Signature = self.key.sign(signing.as_bytes());
        format!("vapid t={signing}.{}, k={}", B64.encode(sig.to_bytes()), self.public())
    }

    /// Wakes the phone subscribed at `endpoint`: an empty push.
    pub async fn send(&self, endpoint: &str) -> Result<()> {
        let url = allowed(endpoint)?;
        let res = reqwest::Client::new()
            .post(url.clone())
            .header("Authorization", self.authorization(&url))
            .header("TTL", "3600")
            .header("Urgency", "high")
            .header("Content-Length", "0")
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .context("push")?;
        if !res.status().is_success() {
            bail!("push: {} answered {}", url.host_str().unwrap_or_default(), res.status());
        }
        Ok(())
    }
}

/// A subscription's endpoint, if it is one of the push services'.
pub fn allowed(endpoint: &str) -> Result<url::Url> {
    let url = url::Url::parse(endpoint).context("not an address")?;
    ensure!(url.scheme() == "https", "a push service is reached over HTTPS");
    let host = url.host_str().unwrap_or_default();
    ensure!(SERVICES.iter().any(|s| host == *s || host.ends_with(&format!(".{s}"))), "not a known push service: {host}");
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushes_go_to_the_push_services_only() {
        assert!(allowed("https://fcm.googleapis.com/fcm/send/abc").is_ok());
        assert!(allowed("https://web.push.apple.com/QK…").is_ok());
        assert!(allowed("https://wns2-am3p.notify.windows.com/w/?token=x").is_ok());
        assert!(allowed("http://fcm.googleapis.com/fcm/send/abc").is_err());
        assert!(allowed("https://fcm.googleapis.com.evil.example/x").is_err());
        assert!(allowed("https://127.0.0.1:47800/api/ws").is_err());
    }

    #[test]
    fn the_key_is_kept_and_signs_for_the_service() {
        let dir = std::env::temp_dir().join(format!("prism-vapid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = Vapid::load(&dir).unwrap();
        let b = Vapid::load(&dir).unwrap();
        assert_eq!(a.public(), b.public());
        assert_eq!(B64.decode(a.public()).unwrap().len(), 65);
        let auth = a.authorization(&url::Url::parse("https://fcm.googleapis.com/fcm/send/abc").unwrap());
        let jwt = auth.strip_prefix("vapid t=").unwrap().split(", k=").next().unwrap();
        let claims: serde_json::Value = serde_json::from_slice(&B64.decode(jwt.split('.').nth(1).unwrap()).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        let _ = std::fs::remove_dir_all(dir);
    }
}
