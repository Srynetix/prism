//! What Prism's OAuth sign-ins share, Microsoft 365's (`mail`) and Google's (`google`): the token
//! endpoint's answers, and the tokens kept, the access token in memory and the refresh token in
//! the system keychain (the profile's entry, never config.toml).

use prism_base::{i18n::tr, tr};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Deserialize};

/// An `application/x-www-form-urlencoded` body.
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{}={}", crate::http::encode(k), crate::http::encode(v))).collect::<Vec<_>>().join("&")
}

#[derive(Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    #[serde(default = "an_hour")]
    pub expires_in: u64,
    /// The permissions granted (Google's answer says them).
    pub scope: Option<String>,
}

fn an_hour() -> u64 {
    3600
}

#[derive(Debug, Default, Deserialize)]
pub struct OAuthError {
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub error_description: String,
}

impl OAuthError {
    /// The description's first line (Microsoft's carry a trace and a timestamp after it), and the
    /// error's code.
    pub fn describe(&self) -> String {
        let first = self.error_description.lines().next().unwrap_or_default().trim();
        match (self.error.as_str(), first) {
            ("", "") => "refused".into(),
            (e, "") => e.into(),
            ("", d) => d.into(),
            (e, d) => format!("{d} ({e})"),
        }
    }
}

/// A POST of `form` to a token endpoint: its tokens, or its OAuth error. `who` names it in the
/// errors of the exchange itself ("Google sign-in").
pub async fn request(http: &reqwest::Client, url: &str, form: &[(&str, &str)], who: &str) -> Result<std::result::Result<Tokens, OAuthError>> {
    let resp = http
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(self::form(form))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .with_context(|| tr!("{who} did not answer", who = who))?;
    if resp.status().is_success() {
        return Ok(Ok(resp.json().await.with_context(|| tr!("{who}'s answer is not understood", who = who))?));
    }
    Ok(Err(resp.json().await.unwrap_or_default()))
}

/// The tokens of one sign-in: the access token and when it expires, in memory; the refresh token
/// in the keychain, under `service` (the profile's entry).
pub struct TokenStore {
    service: &'static str,
    access: Mutex<Option<(String, Instant)>>,
}

impl TokenStore {
    pub const fn new(service: &'static str) -> Self {
        Self { service, access: Mutex::new(None) }
    }

    /// The access token, while it is valid.
    pub fn cached(&self) -> Option<String> {
        self.access.lock().unwrap().clone().filter(|(_, until)| Instant::now() < *until).map(|(token, _)| token)
    }

    /// The refresh token of the keychain; `None`: signed out.
    pub async fn refresh_token(&self) -> Result<Option<String>> {
        crate::platform::secret(&crate::profile::service(self.service)).await
    }

    /// Keeps the access token in memory and the refresh token in the keychain, when new (both
    /// providers may rotate it). Returns the access token.
    pub async fn keep(&self, t: Tokens, previous: Option<&str>) -> Result<String> {
        if let Some(refresh) = t.refresh_token.filter(|r| Some(r.as_str()) != previous) {
            let service = crate::profile::service(self.service);
            tokio::task::spawn_blocking(move || crate::platform::store_secret(&service, &refresh)).await??;
        }
        // A minute of margin, so that a read never starts with a token about to expire.
        let until = Instant::now() + Duration::from_secs(t.expires_in.saturating_sub(60));
        *self.access.lock().unwrap() = Some((t.access_token.clone(), until));
        Ok(t.access_token)
    }

    /// The access token is dropped (refused by the API, signed out): renewed at the next read.
    pub fn forget(&self) {
        *self.access.lock().unwrap() = None;
    }

    /// A GET on `service`'s API with the access token; refused (401), the token is dropped and
    /// renewed at the next read. Google's and Microsoft Graph's errors say why in `error.message`.
    pub async fn get<T: DeserializeOwned>(&self, http: &reqwest::Client, service: &str, token: &str, url: &str) -> Result<T> {
        #[derive(Default, Deserialize)]
        struct Refusal {
            #[serde(default)]
            error: Why,
        }
        #[derive(Default, Deserialize)]
        struct Why {
            message: Option<String>,
        }
        let resp = http.get(url).bearer_auth(token).timeout(Duration::from_secs(30)).send().await.with_context(|| tr!("{who} did not answer", who = service))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            self.forget();
        }
        if !status.is_success() {
            let body: Refusal = resp.json().await.unwrap_or_default();
            bail!("{service}: {} ({status})", body.error.message.unwrap_or_else(|| tr("request refused").into()));
        }
        resp.json().await.with_context(|| tr!("{who}'s answer is not understood", who = service))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_read_their_first_line_and_code() {
        let e: OAuthError = serde_json::from_str(r#"{"error":"invalid_grant","error_description":"AADSTS70000: The grant is expired.\r\nTrace ID: x"}"#).unwrap();
        assert_eq!(e.describe(), "AADSTS70000: The grant is expired. (invalid_grant)");
        assert_eq!(OAuthError { error: "expired_token".into(), error_description: String::new() }.describe(), "expired_token");
        assert_eq!(OAuthError::default().describe(), "refused");
    }

    #[test]
    fn forms_are_encoded() {
        assert_eq!(form(&[("scope", "https://graph.microsoft.com/Mail.ReadBasic offline_access"), ("a", "x y&z")]), "scope=https%3A%2F%2Fgraph.microsoft.com%2FMail.ReadBasic%20offline_access&a=x%20y%26z");
    }

    #[test]
    fn an_expired_access_token_is_not_given() {
        let store = TokenStore::new("prism-test");
        *store.access.lock().unwrap() = Some(("old".into(), Instant::now()));
        assert_eq!(store.cached(), None);
        *store.access.lock().unwrap() = Some(("new".into(), Instant::now() + Duration::from_secs(60)));
        assert_eq!(store.cached().as_deref(), Some("new"));
        store.forget();
        assert_eq!(store.cached(), None);
    }
}
