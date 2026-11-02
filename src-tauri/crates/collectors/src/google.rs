//! Signing in to a Google account, shared by the Mail block (Gmail) and the agenda (Google
//! Calendar): one account per profile, the permissions of the sources turned on, and nothing more.
//! The OAuth client is yours: a "Desktop app" client of your own Google Cloud project, its id in
//! `[google] client_id`, its secret in the system keychain (Google requires it, though a desktop
//! app's secret protects nothing). Google refuses its device code flow to Gmail, so signing in
//! goes through the browser: Prism opens Google's page and listens on the loopback, on a port the
//! system picks, for the one redirect carrying the code (PKCE, and a `state` checked). The refresh
//! token goes into the system keychain, the profile's entry, never config.toml.

use prism_base::{i18n::tr, tr};
use std::{
    time::Duration,
};

use anyhow::{bail, Context, Result};
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

use super::oauth::{self, OAuthError, TokenStore, Tokens};
use crate::config::GoogleConfig;

/// The refresh token.
pub const KEYCHAIN_SERVICE: &str = "prism-google";
/// The OAuth client's secret.
pub const SECRET_SERVICE: &str = "prism-google-secret";
/// Gmail: the labels and the messages' headers, never a body nor an attachment.
pub const GMAIL: &str = "https://www.googleapis.com/auth/gmail.metadata";
/// Google Calendar: the list of your calendars, and their events, read-only.
pub const CALENDARS: &str = "https://www.googleapis.com/auth/calendar.calendarlist.readonly";
pub const EVENTS: &str = "https://www.googleapis.com/auth/calendar.events.readonly";
const AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const REVOKE: &str = "https://oauth2.googleapis.com/revoke";
/// Time left to sign in in the browser.
const SIGN_IN: Duration = Duration::from_secs(300);

pub struct Google {
    cfg: GoogleConfig,
    /// The permissions asked for: those of the sources turned on.
    scopes: Vec<&'static str>,
    http: Client,
    tokens: TokenStore,
}

static GOOGLE: prism_base::rt::Swap<Google> = prism_base::rt::Swap::new();

/// At launch, and on each change of the config: the permissions Gmail and Google Calendar need, those turned on.
pub fn init(cfg: GoogleConfig, gmail: bool, calendar: bool) {
    let mut scopes = Vec::new();
    if gmail {
        scopes.push(GMAIL);
    }
    if calendar {
        scopes.extend([CALENDARS, EVENTS]);
    }
    GOOGLE.set(Google { cfg, scopes, http: crate::http::client(), tokens: TokenStore::new(KEYCHAIN_SERVICE) });
}

pub fn get() -> Result<&'static Google> {
    let g = GOOGLE.get().context(tr("Google is not set up"))?;
    anyhow::ensure!(!g.scopes.is_empty(), "{}", tr("nothing reads Google: choose Gmail as the mail provider, or Google Calendar as the calendar (Settings › Work › Mail, Calendar)"));
    Ok(g)
}

impl Google {
    fn client_id(&self) -> Result<&str> {
        let id = self.cfg.client_id.trim();
        anyhow::ensure!(!id.is_empty(), "{}", tr("no Google OAuth client: set [google] client_id (Settings › Work › Mail › Gmail or Calendar › Google Calendar), the id of your Desktop app client"));
        Ok(id)
    }

    async fn client_secret(&self) -> Result<String> {
        crate::platform::secret(&crate::profile::service(SECRET_SERVICE))
            .await?
            .filter(|s| !s.is_empty())
            .context(tr("no Google client secret in the keychain: paste it in Settings › Work › Mail › Gmail or Calendar › Google Calendar"))
    }

    /// A valid access token, renewed from the keychain's refresh token; `None` when signed out.
    pub async fn access_token(&self) -> Result<Option<String>> {
        if let Some(token) = self.tokens.cached() {
            return Ok(Some(token));
        }
        let Some(refresh) = self.tokens.refresh_token().await? else {
            return Ok(None);
        };
        let secret = self.client_secret().await?;
        let form = [("client_id", self.client_id()?), ("client_secret", secret.as_str()), ("grant_type", "refresh_token"), ("refresh_token", refresh.as_str())];
        match self.token(&form).await? {
            Ok(t) => {
                // A source turned on since signing in: its permission was never granted.
                let missing = missing(&self.scopes, t.scope.as_deref());
                anyhow::ensure!(missing.is_empty(), "{}", tr!("Google did not grant {what}: sign in again in Settings › Work › Mail › Gmail or Calendar › Google Calendar", what = describe(&missing)));
                Ok(Some(self.tokens.keep(t, Some(&refresh)).await?))
            }
            // Revoked, or expired (a client left in "Testing" loses its tokens after 7 days).
            Err(e) if e.error == "invalid_grant" => {
                tracing::info!("google: refresh token refused ({}), signed out", e.describe());
                bail!("{}", tr!("signed out of Google ({err}): sign in again in Settings › Work › Mail › Gmail or Calendar › Google Calendar", err = e.describe()))
            }
            Err(e) => bail!("{}", tr!("Google sign-in: {err}", err = e.describe())),
        }
    }

    /// A GET on a Google API, with the access token.
    pub async fn get<T: for<'de> Deserialize<'de>>(&self, token: &str, url: &str) -> Result<T> {
        self.tokens.get(&self.http, "Google", token, url).await
    }

    async fn token(&self, form: &[(&str, &str)]) -> Result<std::result::Result<Tokens, OAuthError>> {
        oauth::request(&self.http, TOKEN, form, "Google sign-in").await
    }
}

/// Whether a refresh token is in the keychain, and the permissions Prism asks for.
#[derive(Debug, Serialize)]
pub struct State {
    pub signed_in: bool,
    pub scopes: Vec<&'static str>,
}

pub async fn state() -> Result<State> {
    let scopes = GOOGLE.get().map(|g| g.scopes.clone()).unwrap_or_default();
    let signed_in = crate::platform::secret(&crate::profile::service(KEYCHAIN_SERVICE)).await?.is_some();
    Ok(State { signed_in, scopes })
}

/// Signs in through the browser: Google's page opens, Prism waits for its redirect on the loopback,
/// then the sources reading Google read at once.
pub async fn sign_in() -> Result<()> {
    let g = get()?;
    let client_id = g.client_id()?.to_string();
    let secret = g.client_secret().await?;
    let listener = TcpListener::bind("127.0.0.1:0").await.context("listening on the loopback for Google's answer")?;
    let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    // PKCE: the code is worth nothing without this verifier, which never leaves Prism.
    let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()));
    let state = uuid::Uuid::new_v4().simple().to_string();
    let scope = g.scopes.join(" ");
    let url = reqwest::Url::parse_with_params(
        AUTH,
        [
            ("client_id", client_id.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("response_type", "code"),
            ("scope", scope.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state.as_str()),
            // A refresh token, every time: Google gives one only at the first consent otherwise.
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
    )?;
    crate::platform::open_url(url.as_str())?;
    let code = tokio::time::timeout(SIGN_IN, redirected(&listener, &state)).await.map_err(|_| anyhow::anyhow!(tr("no answer from Google within 5 minutes: sign in again")))??;
    let form = [
        ("client_id", client_id.as_str()),
        ("client_secret", secret.as_str()),
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("code_verifier", verifier.as_str()),
        ("redirect_uri", redirect.as_str()),
    ];
    let t = match g.token(&form).await? {
        Ok(t) => t,
        Err(e) => bail!("{}", tr!("Google sign-in: {err}", err = e.describe())),
    };
    anyhow::ensure!(t.refresh_token.is_some(), "{}", tr("Google gave no refresh token: sign in again"));
    // Google lets each permission be unchecked on its consent page.
    let missing = missing(&g.scopes, t.scope.as_deref());
    anyhow::ensure!(missing.is_empty(), "{}", tr!("Google did not grant {what}: sign in again and check every box on Google's page", what = describe(&missing)));
    g.tokens.keep(t, None).await?;
    tracing::info!("google: signed in");
    wake();
    Ok(())
}

/// Signs out: the refresh token is revoked at Google and leaves the keychain.
pub async fn sign_out() -> Result<()> {
    let service = crate::profile::service(KEYCHAIN_SERVICE);
    if let (Some(g), Ok(Some(refresh))) = (GOOGLE.get(), crate::platform::secret(&service).await) {
        // Best effort: the keychain entry goes in any case.
        if let Err(e) = g.http.post(REVOKE).header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(oauth::form(&[("token", refresh.as_str())])).timeout(Duration::from_secs(15)).send().await {
            tracing::warn!("google: revoking the token: {e}");
        }
        g.tokens.forget();
    }
    tokio::task::spawn_blocking(move || crate::platform::delete_secret(&service)).await??;
    wake();
    Ok(())
}

/// The sources reading Google read again, now.
fn wake() {
    super::mail::wake();
    super::calendar::wake();
}

/// The code of Google's redirect to the loopback, its `state` checked. A request without either
/// (a favicon, another page) is answered and waited past.
async fn redirected(listener: &TcpListener, state: &str) -> Result<String> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let mut len = 0;
        while len < buf.len() {
            let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf[len..])).await.unwrap_or(Ok(0))?;
            if n == 0 {
                break;
            }
            len += n;
            if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let head = String::from_utf8_lossy(&buf[..len]).to_string();
        let answer = answer_of(&head, state);
        let page = match &answer {
            Some(Ok(_)) => "Prism is signed in to Google. You can close this tab.",
            Some(Err(_)) => "Prism could not sign in to Google: see Prism's Settings.",
            None => "",
        };
        let status = if answer.is_some() { "200 OK" } else { "404 Not Found" };
        let body = format!("<!doctype html><meta charset=\"utf-8\"><title>Prism</title><p style=\"font:16px system-ui;margin:40px\">{page}</p>");
        let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}", body.len());
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        if let Some(answer) = answer {
            return answer;
        }
    }
}

/// Google's redirect in a request's head: `Some(code)`, `Some(error)`, or `None` (not it).
fn answer_of(head: &str, state: &str) -> Option<Result<String>> {
    let mut parts = head.lines().next()?.split_whitespace();
    let (method, target) = (parts.next()?, parts.next()?);
    let query = target.strip_prefix("/?")?;
    if method != "GET" {
        return None;
    }
    let url = reqwest::Url::parse(&format!("http://127.0.0.1/?{query}")).ok()?;
    let param = |k: &str| url.query_pairs().find(|(key, _)| key == k).map(|(_, v)| v.into_owned());
    if param("state").as_deref() != Some(state) {
        return None;
    }
    Some(match (param("code"), param("error")) {
        (Some(code), _) => Ok(code),
        (None, Some(e)) if e == "access_denied" => Err(anyhow::anyhow!(tr("access refused on Google's page"))),
        (None, e) => Err(anyhow::anyhow!(tr!("Google sign-in: {err}", err = e.unwrap_or_else(|| tr("no code").into())))),
    })
}

/// The permissions asked for that `granted` (space-separated, as Google gives them) lacks. Google
/// leaves the scope out of a renewal's answer at times: nothing is then known to lack.
fn missing(asked: &[&'static str], granted: Option<&str>) -> Vec<&'static str> {
    let Some(granted) = granted else { return Vec::new() };
    let granted: Vec<&str> = granted.split_whitespace().collect();
    asked.iter().copied().filter(|s| !granted.contains(s)).collect()
}

fn describe(scopes: &[&str]) -> String {
    let mut names: Vec<&str> = scopes.iter().map(|s| if *s == GMAIL { "Gmail" } else { "Google Calendar" }).collect();
    names.dedup();
    format!("access to {}", names.join(" and "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_redirect_with_its_state_answers() {
        let req = |target: &str| format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:4567\r\n\r\n");
        assert_eq!(answer_of(&req("/?state=abc&code=4%2F0Ab&scope=x"), "abc").unwrap().unwrap(), "4/0Ab");
        assert!(answer_of(&req("/?state=abc&error=access_denied"), "abc").unwrap().is_err());
        assert!(answer_of(&req("/?state=other&code=x"), "abc").is_none(), "another sign-in's");
        assert!(answer_of(&req("/favicon.ico"), "abc").is_none());
        assert!(answer_of("POST /?state=abc&code=x HTTP/1.1\r\n\r\n", "abc").is_none());
    }

    #[test]
    fn unchecked_permissions_are_found() {
        let asked = [GMAIL, CALENDARS, EVENTS];
        assert!(missing(&asked, Some(&format!("{EVENTS} {GMAIL} {CALENDARS}"))).is_empty());
        assert_eq!(missing(&asked, Some(GMAIL)), [CALENDARS, EVENTS]);
        assert_eq!(describe(&missing(&asked, Some(GMAIL))), "access to Google Calendar");
        assert!(missing(&asked, None).is_empty());
    }
}
