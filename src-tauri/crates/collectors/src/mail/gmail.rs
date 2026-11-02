//! The Mail block's Gmail: the unread messages of the inbox and the starred ones, through the
//! Gmail API with `gmail.metadata` (labels and headers: never a body, a snippet nor an
//! attachment), signed in with the profile's Google account (`google`). With that permission
//! Gmail refuses search queries: the messages are found by their labels. A message's headers do
//! not change: read once, kept while it is listed. It opens in Gmail on the web.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::{StreamExt, TryStreamExt};
use serde::Deserialize;

use crate::google::Google;
use crate::{
    config::MailConfig,
    state::{MailMessage, MailState},
};

const API: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
/// Messages whose headers are read at once: Gmail counts its quota per second.
const PARALLEL: usize = 6;

/// The headers of the messages listed, by id.
static HEADERS: LazyLock<Mutex<HashMap<String, Headers>>> = LazyLock::new(Default::default);

#[derive(Clone)]
struct Headers {
    subject: String,
    from: String,
    address: String,
    received: Option<DateTime<Utc>>,
    thread: String,
}

/// One read of the inbox; signed out, a state saying so.
pub async fn read(g: &Google, cfg: &MailConfig) -> Result<MailState> {
    let Some(token) = g.access_token().await? else {
        return Ok(MailState { signed_in: false, read_at: Some(Utc::now()), ..Default::default() });
    };
    let max = cfg.max.clamp(1, 200);
    let profile: Profile = g.get(&token, &format!("{API}/profile")).await?;
    let inbox: Label = g.get(&token, &format!("{API}/labels/INBOX")).await?;
    let unread = list(g, &token, &["INBOX", "UNREAD"], max).await?;
    let starred = if cfg.flagged { list(g, &token, &["STARRED"], max).await? } else { Vec::new() };
    let mut ids: Vec<String> = Vec::new();
    for id in unread.iter().chain(&starred) {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    let known: Vec<String> = HEADERS.lock().unwrap().keys().cloned().collect();
    let new: Vec<String> = ids.iter().filter(|id| !known.contains(id)).cloned().collect();
    let read: Vec<(String, Headers)> = futures::stream::iter(new).map(|id| headers(g, &token, id)).buffer_unordered(PARALLEL).try_collect().await?;
    let mut cache = HEADERS.lock().unwrap();
    cache.extend(read);
    cache.retain(|id, _| ids.contains(id));
    // A starred message is unread when among the unread ones listed: an older unread one shows read.
    let messages = ids
        .iter()
        .filter_map(|id| {
            let h = cache.get(id)?.clone();
            Some(MailMessage {
                id: id.clone(),
                subject: h.subject,
                from: h.from,
                address: h.address,
                received: h.received,
                unread: unread.contains(id),
                flagged: starred.contains(id),
                link: link(&profile.email_address, &h.thread),
            })
        })
        .collect();
    Ok(MailState { signed_in: true, unread: inbox.messages_unread, messages: super::merged(messages, max as usize), read_at: Some(Utc::now()), ..Default::default() })
}

/// The ids of the messages bearing all of `labels`, newest first.
async fn list(g: &Google, token: &str, labels: &[&str], max: u32) -> Result<Vec<String>> {
    let labels: String = labels.iter().map(|l| format!("&labelIds={l}")).collect();
    let page: Page = g.get(token, &format!("{API}/messages?maxResults={max}{labels}")).await?;
    Ok(page.messages.into_iter().map(|m| m.id).collect())
}

async fn headers(g: &Google, token: &str, id: String) -> Result<(String, Headers)> {
    let m: RawMessage = g.get(token, &format!("{API}/messages/{id}?format=metadata&metadataHeaders=Subject&metadataHeaders=From")).await?;
    Ok((id, m.into()))
}

/// The conversation in Gmail on the web, in the signed-in account (several may be).
fn link(account: &str, thread: &str) -> String {
    let account = crate::http::encode(account);
    format!("https://mail.google.com/mail/?authuser={account}#all/{thread}")
}

/// Gmail on the web, nothing else.
pub fn gmail_link(link: &str) -> bool {
    link.starts_with("https://mail.google.com/")
}

/// `Name <address>`, `"Last, First" <address>` or a bare address: the name, else the address.
fn sender(from: &str) -> (String, String) {
    let from = from.trim();
    match from.rsplit_once('<') {
        Some((name, rest)) => {
            let address = rest.trim_end_matches('>').trim().to_string();
            let name = name.trim().trim_matches('"').trim();
            (if name.is_empty() { address.clone() } else { name.to_string() }, address)
        }
        None => (from.to_string(), from.to_string()),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    email_address: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Label {
    #[serde(default)]
    messages_unread: u32,
}

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    messages: Vec<Ref>,
}

#[derive(Deserialize)]
struct Ref {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMessage {
    thread_id: String,
    /// Milliseconds since the epoch, as a string.
    #[serde(default)]
    internal_date: Option<String>,
    #[serde(default)]
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(default)]
    headers: Vec<Header>,
}

#[derive(Deserialize)]
struct Header {
    name: String,
    value: String,
}

impl From<RawMessage> for Headers {
    fn from(m: RawMessage) -> Self {
        let headers = m.payload.map(|p| p.headers).unwrap_or_default();
        let header = |name: &str| headers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).map(|h| h.value.clone()).unwrap_or_default();
        let (from, address) = sender(&header("From"));
        Headers {
            subject: header("Subject").trim().to_string(),
            from,
            address,
            received: m.internal_date.and_then(|d| d.parse().ok()).and_then(DateTime::from_timestamp_millis),
            thread: m.thread_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_gmail_metadata() {
        let m: RawMessage = serde_json::from_str(r#"{"id":"18f1","threadId":"18f0","labelIds":["INBOX","UNREAD"],"internalDate":"1791793800000",
            "payload":{"headers":[{"name":"From","value":"\"Shop, The\" <shop@example.org>"},{"name":"Subject","value":" Invoice "}]}}"#).unwrap();
        let h = Headers::from(m);
        assert_eq!((h.subject.as_str(), h.from.as_str(), h.address.as_str(), h.thread.as_str()), ("Invoice", "Shop, The", "shop@example.org", "18f0"));
        assert_eq!(h.received.unwrap().timestamp(), 1_791_793_800);
    }

    #[test]
    fn senders_without_a_name_show_their_address() {
        assert_eq!(sender("noreply@example.org"), ("noreply@example.org".into(), "noreply@example.org".into()));
        assert_eq!(sender("<noreply@example.org>"), ("noreply@example.org".into(), "noreply@example.org".into()));
        assert_eq!(sender("Ann <ann@example.org>"), ("Ann".into(), "ann@example.org".into()));
    }

    #[test]
    fn links_open_gmail_only() {
        let l = link("me+perso@gmail.com", "18f0");
        assert_eq!(l, "https://mail.google.com/mail/?authuser=me%2Bperso%40gmail.com#all/18f0");
        assert!(gmail_link(&l));
        assert!(!gmail_link("https://mail.google.com.evil.example/") && !gmail_link("file:///etc/passwd"));
    }
}
