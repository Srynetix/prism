//! The Mail block: the unread messages of your Microsoft 365 inbox (and the flagged ones), read
//! every minute through Microsoft Graph. The permission is delegated and the narrowest there is,
//! `Mail.ReadBasic`: your own mailbox only, and never a body, a preview nor an attachment. The
//! application is the one your organization registered in Entra ID (`[mail] client_id`); you sign
//! in once from Settings (OAuth device code flow) and the refresh token goes into the system
//! keychain, the profile's entry, never config.toml. A message opens in Outlook on the web.
//! With `[mail] provider = "gmail"`, the inbox is Gmail's instead (`gmail`).

use prism_base::{i18n::tr, tr};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};

mod gmail;

use super::oauth::{self, OAuthError, TokenStore, Tokens};
use crate::{
    config::{MailConfig, MailProvider},
    state::{Hub, MailMessage, MailState},
};

const SOURCE: &str = "mail";
pub const KEYCHAIN_SERVICE: &str = "prism-mail";
/// What Prism asks for: the messages' metadata, and a refresh token to stay signed in.
const SCOPE: &str = "https://graph.microsoft.com/Mail.ReadBasic offline_access";
const GRAPH: &str = "https://graph.microsoft.com/v1.0";
/// The inbox changes little from one minute to the next: read every minute, and right away on demand.
const POLL: Duration = Duration::from_secs(60);
/// The properties read: those `Mail.ReadBasic` allows, and the block shows.
const SELECT: &str = "id,subject,from,receivedDateTime,isRead,flag,webLink";

struct Mail {
    hub: Hub,
    cfg: MailConfig,
    http: Client,
    refresh: tokio::sync::Notify,
    tokens: TokenStore,
    /// The sign-in in progress: its device code.
    pending: Mutex<Option<Pending>>,
}

struct Pending {
    device_code: String,
    interval: Duration,
    until: Instant,
}

static MAIL: prism_base::rt::Swap<Mail> = prism_base::rt::Swap::new();

fn mail() -> Result<&'static Mail> {
    MAIL.get().context(tr("the Mail block is off: turn on Settings › Work › Mail"))
}

/// Reads the inbox now, then every minute or when asked.
pub async fn run(cfg: MailConfig, hub: Hub) {
    let m = Mail { hub: hub.clone(), cfg, http: crate::http::client(), refresh: tokio::sync::Notify::new(), tokens: TokenStore::new(KEYCHAIN_SERVICE), pending: Mutex::new(None) };
    let m = MAIL.set(m);
    let mut first = true;
    loop {
        if !std::mem::take(&mut first) {
            tokio::select! {
                _ = hub.pause(POLL) => {}
                _ = m.refresh.notified() => {}
            }
        }
        let read = match m.cfg.provider {
            MailProvider::Microsoft => m.read().await,
            MailProvider::Gmail => match crate::google::get() {
                Ok(g) => gmail::read(g, &m.cfg).await,
                Err(e) => Err(e),
            },
        };
        let read = read.map(|state| MailState { provider: m.cfg.provider, ..state });
        hub.publish(SOURCE, read, |s, state| crate::state::replace(&mut s.mail, Some(state)));
    }
}

/// Reads the inbox now (a sign-in or out just happened).
pub fn wake() {
    if let Some(m) = MAIL.get() {
        m.refresh.notify_one();
    }
}

impl Mail {
    fn authority(&self) -> String {
        let tenant = self.cfg.tenant.trim();
        format!("https://login.microsoftonline.com/{}/oauth2/v2.0", if tenant.is_empty() { "organizations" } else { tenant })
    }

    fn client_id(&self) -> Result<&str> {
        let id = self.cfg.client_id.trim();
        anyhow::ensure!(!id.is_empty(), "no application to sign in with: set [mail] client_id (Settings › Work › Mail › Microsoft 365), the one registered in Entra ID with the delegated Mail.ReadBasic permission");
        Ok(id)
    }

    /// One read of the inbox; signed out, a state saying so.
    async fn read(&self) -> Result<MailState> {
        let Some(token) = self.access_token().await? else {
            return Ok(MailState { signed_in: false, read_at: Some(Utc::now()), ..Default::default() });
        };
        let max = self.cfg.max.clamp(1, 200);
        let folder: Folder = self.get(&token, &format!("{GRAPH}/me/mailFolders/inbox?$select=unreadItemCount")).await?;
        let mut messages: Vec<MailMessage> = self.get::<Page>(&token, &messages_url("isRead eq false", max)).await?.value.into_iter().map(MailMessage::from).collect();
        if self.cfg.flagged {
            let flagged = self.get::<Page>(&token, &messages_url("flag/flagStatus eq 'flagged'", max)).await?.value;
            messages.extend(flagged.into_iter().map(MailMessage::from));
        }
        Ok(MailState { signed_in: true, unread: folder.unread_item_count, messages: merged(messages, max as usize), read_at: Some(Utc::now()), ..Default::default() })
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, token: &str, url: &str) -> Result<T> {
        self.tokens.get(&self.http, "Microsoft Graph", token, url).await
    }

    /// A valid access token, renewed from the keychain's refresh token; `None` when signed out.
    async fn access_token(&self) -> Result<Option<String>> {
        if let Some(token) = self.tokens.cached() {
            return Ok(Some(token));
        }
        let Some(refresh) = self.tokens.refresh_token().await? else {
            return Ok(None);
        };
        let form = [("client_id", self.client_id()?), ("grant_type", "refresh_token"), ("refresh_token", refresh.as_str()), ("scope", SCOPE)];
        match self.token(&form).await? {
            Ok(t) => Ok(Some(self.tokens.keep(t, Some(&refresh)).await?)),
            // The refresh token no longer works (revoked, expired, password changed): signed out.
            Err(e) if e.error == "invalid_grant" || e.error == "interaction_required" => {
                tracing::info!("mail: refresh token refused ({}), signed out", e.error);
                bail!("{}", tr!("signed out of Microsoft 365 ({err}): sign in again in Settings › Work › Mail › Microsoft 365", err = e.error))
            }
            Err(e) => bail!("{}", tr!("Microsoft sign-in: {err}", err = e.describe())),
        }
    }

    /// The token endpoint: its tokens, or its OAuth error.
    async fn token(&self, form: &[(&str, &str)]) -> Result<std::result::Result<Tokens, OAuthError>> {
        oauth::request(&self.http, &format!("{}/token", self.authority()), form, "Microsoft sign-in").await
    }
}

/// The inbox's messages matching `filter`, newest first. Graph refuses `$orderby` on a property
/// absent from `$filter`: the date comes first in both.
fn messages_url(filter: &str, top: u32) -> String {
    let q = crate::http::encode;
    format!(
        "{GRAPH}/me/mailFolders/inbox/messages?$filter={}&$orderby={}&$select={SELECT}&$top={top}",
        q(&format!("receivedDateTime ge 1900-01-01T00:00:00Z and {filter}")),
        q("receivedDateTime desc"),
    )
}

/// Unread then flagged, without duplicates, newest first, at most `max`.
pub fn merged(messages: Vec<MailMessage>, max: usize) -> Vec<MailMessage> {
    let mut out: Vec<MailMessage> = Vec::new();
    for m in messages {
        if !out.iter().any(|x| x.id == m.id) {
            out.push(m);
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.received));
    out.truncate(max);
    out
}

/// The code to type at Microsoft's address, to sign in.
#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    /// Seconds left to type it.
    pub expires_in: u64,
    #[serde(default, skip_serializing)]
    device_code: String,
    #[serde(default = "five", skip_serializing)]
    interval: u64,
}

fn five() -> u64 {
    5
}

/// Starts signing in: the code to type at Microsoft's address.
pub async fn sign_in_start() -> Result<DeviceCode> {
    let m = mail()?;
    anyhow::ensure!(m.cfg.provider == MailProvider::Microsoft, "{}", tr("Gmail signs in with your Google account: Settings › Work › Mail › Gmail"));
    let form = [("client_id", m.client_id()?), ("scope", SCOPE)];
    let resp = m
        .http
        .post(format!("{}/devicecode", m.authority()))
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(oauth::form(&form))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context(tr("Microsoft sign-in did not answer"))?;
    if !resp.status().is_success() {
        let e: OAuthError = resp.json().await.unwrap_or_default();
        bail!("{}", tr!("Microsoft sign-in: {err}", err = e.describe()));
    }
    let code: DeviceCode = resp.json().await.context("Microsoft sign-in's answer is not understood")?;
    *m.pending.lock().unwrap() = Some(Pending { device_code: code.device_code.clone(), interval: Duration::from_secs(code.interval.max(1)), until: Instant::now() + Duration::from_secs(code.expires_in) });
    Ok(code)
}

/// Waits for the code to be typed and the permission accepted, then reads the inbox.
pub async fn sign_in_wait() -> Result<()> {
    let m = mail()?;
    let (device_code, mut interval, until) = {
        let p = m.pending.lock().unwrap();
        let p = p.as_ref().context(tr("no sign-in in progress"))?;
        (p.device_code.clone(), p.interval, p.until)
    };
    loop {
        tokio::time::sleep(interval).await;
        anyhow::ensure!(Instant::now() < until, "{}", tr("the code expired: sign in again"));
        // Another sign-in started meanwhile: this one stops.
        if m.pending.lock().unwrap().as_ref().is_none_or(|p| p.device_code != device_code) {
            bail!("{}", tr("sign-in replaced by another"));
        }
        let form = [("client_id", m.client_id()?), ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("device_code", device_code.as_str())];
        match m.token(&form).await? {
            Ok(t) => {
                anyhow::ensure!(t.refresh_token.is_some(), "{}", tr("Microsoft gave no refresh token: the application must allow offline_access"));
                m.tokens.keep(t, None).await?;
                *m.pending.lock().unwrap() = None;
                m.refresh.notify_one();
                return Ok(());
            }
            Err(e) if e.error == "authorization_pending" => {}
            Err(e) if e.error == "slow_down" => interval += Duration::from_secs(5),
            Err(e) => {
                *m.pending.lock().unwrap() = None;
                bail!("{}", tr!("Microsoft sign-in: {err}", err = e.describe()));
            }
        }
    }
}

/// Signs out: the refresh token leaves the keychain, the block empties.
pub async fn sign_out() -> Result<()> {
    let service = crate::profile::service(KEYCHAIN_SERVICE);
    tokio::task::spawn_blocking(move || crate::platform::delete_secret(&service)).await??;
    if let Some(m) = MAIL.get() {
        m.tokens.forget();
        m.refresh.notify_one();
    }
    Ok(())
}

/// Opens a message the block displays, in Outlook or Gmail on the web.
pub fn open(id: &str) -> Result<()> {
    let m = mail()?;
    let link = m.hub.subscribe().borrow().mail.as_ref().and_then(|s| s.messages.iter().find(|x| x.id == id).map(|x| x.link.clone())).context("not a message Prism displays")?;
    match m.cfg.provider {
        MailProvider::Microsoft => anyhow::ensure!(outlook_link(&link), "not an Outlook link"),
        MailProvider::Gmail => anyhow::ensure!(gmail::gmail_link(&link), "not a Gmail link"),
    }
    crate::platform::open_url(&link)
}

/// Graph's `webLink`: Outlook on the web, nothing else.
fn outlook_link(link: &str) -> bool {
    ["https://outlook.office.com/", "https://outlook.office365.com/"].iter().any(|p| link.starts_with(p))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Folder {
    #[serde(default)]
    unread_item_count: u32,
}

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    value: Vec<RawMessage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMessage {
    id: String,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    from: Option<Recipient>,
    received_date_time: Option<DateTime<Utc>>,
    #[serde(default)]
    is_read: bool,
    #[serde(default)]
    flag: Option<Flag>,
    #[serde(default)]
    web_link: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Recipient {
    email_address: Address,
}

#[derive(Deserialize)]
struct Address {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    address: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Flag {
    #[serde(default)]
    flag_status: String,
}

impl From<RawMessage> for MailMessage {
    fn from(m: RawMessage) -> Self {
        let (name, address) = m.from.map(|f| (f.email_address.name.unwrap_or_default(), f.email_address.address.unwrap_or_default())).unwrap_or_default();
        let from = if name.trim().is_empty() { address.clone() } else { name.trim().to_string() };
        MailMessage {
            id: m.id,
            subject: m.subject.unwrap_or_default().trim().to_string(),
            from,
            address,
            received: m.received_date_time,
            unread: !m.is_read,
            flagged: m.flag.is_some_and(|f| f.flag_status == "flagged"),
            link: m.web_link.unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"{"value":[
        {"id":"AAMk1","subject":" Invoice ","from":{"emailAddress":{"name":"Shop","address":"shop@example.org"}},"receivedDateTime":"2026-10-12T08:30:00Z","isRead":false,"flag":{"flagStatus":"notFlagged"},"webLink":"https://outlook.office365.com/owa/?ItemID=AAMk1"},
        {"id":"AAMk2","subject":null,"from":{"emailAddress":{"name":"","address":"noreply@example.org"}},"receivedDateTime":"2026-10-12T09:00:00Z","isRead":true,"flag":{"flagStatus":"flagged"}}
    ]}"#;

    #[test]
    fn reads_graph_messages() {
        let page: Page = serde_json::from_str(PAGE).unwrap();
        let m: Vec<MailMessage> = page.value.into_iter().map(MailMessage::from).collect();
        assert_eq!((m[0].subject.as_str(), m[0].from.as_str(), m[0].unread, m[0].flagged), ("Invoice", "Shop", true, false));
        assert_eq!((m[1].subject.as_str(), m[1].from.as_str(), m[1].unread, m[1].flagged, m[1].link.as_str()), ("", "noreply@example.org", false, true, ""));
    }

    #[test]
    fn unread_and_flagged_merge_newest_first() {
        let page: Page = serde_json::from_str(PAGE).unwrap();
        let m: Vec<MailMessage> = page.value.into_iter().map(MailMessage::from).collect();
        let twice = [m.clone(), m].concat();
        let out = merged(twice, 10);
        assert_eq!(out.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["AAMk2", "AAMk1"]);
        assert_eq!(merged(out, 1).len(), 1);
    }

    #[test]
    fn the_filter_comes_with_the_date_ordered() {
        let url = messages_url("isRead eq false", 30);
        assert!(url.starts_with("https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages?$filter=receivedDateTime%20ge%201900-01-01T00%3A00%3A00Z%20and%20isRead%20eq%20false&$orderby=receivedDateTime%20desc&"), "{url}");
        assert!(url.ends_with("&$select=id,subject,from,receivedDateTime,isRead,flag,webLink&$top=30"), "{url}");
    }

    #[test]
    fn only_outlook_links_open() {
        assert!(outlook_link("https://outlook.office365.com/owa/?ItemID=AAMk1"));
        assert!(outlook_link("https://outlook.office.com/mail/inbox/id/AAMk1"));
        assert!(!outlook_link("https://example.org/owa") && !outlook_link("file:///etc/passwd"));
    }
}
