//! Notion's HTML blocks (an embed backed by an uploaded `.html` file, rendered live in the page),
//! for the Web view's Import… and its Update on Notion: the block read by Notion's API (its file,
//! through the temporary signed `embed.url` it returns), and its file replaced by a new upload
//! (`file_uploads`, then the block's `embed.file_upload`). The integration's token is in the
//! system keychain (`KEYCHAIN_SERVICE`, the profile's entry); the pages must be shared with it.

use anyhow::{bail, ensure, Context, Result};
use prism_base::{i18n::tr, tr};
use serde_json::{json, Value};

/// The keychain entry of the Notion integration's token (`prism token notion`, Settings).
pub const KEYCHAIN_SERVICE: &str = "prism-notion";
const API: &str = "https://api.notion.com/v1";
const VERSION: &str = "2026-03-11";
/// Blocks of a page read at a time, looking for its HTML ones; pages of them read at most.
const PAGE_SIZE: u32 = 100;
const MAX_PAGES: usize = 20;
/// How long a call may take; an upload, longer.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const UPLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// The largest file Notion takes in one part (its file uploads' `single_part`).
pub const MAX_UPLOAD: usize = 20 * 1024 * 1024;

/// A Notion link's target: a block (`#<id>`, a block's link) or a page (its id ending its path,
/// or `?p=<id>` when opened as a peek), as a dashed UUID.
#[derive(Debug, PartialEq)]
pub enum Target {
    Block(String),
    Page(String),
}

/// Whether `url` is a Notion page's or block's link (`notion.so`, `www.notion.so`, a workspace's
/// `*.notion.site`): read through Notion's API. A file's own link (`file.notion.so`) is not: it is
/// downloaded as any page online.
pub fn is_notion(url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url.trim()) else { return false };
    let host = u.host_str().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    u.scheme() == "https" && (host == "notion.so" || host == "www.notion.so" || host.ends_with(".notion.site"))
}

/// 32 hex digits as Notion's dashed id.
fn dashed(hex: &str) -> String {
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// The id at the end of `s` (32 hex digits, dashed or not), if any.
fn trailing_id(s: &str) -> Option<String> {
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    let tail = hex.get(hex.len().checked_sub(32)?..)?;
    tail.chars().all(|c| c.is_ascii_hexdigit()).then(|| dashed(&tail.to_ascii_lowercase()))
}

/// What a Notion link points at: its block, failing that its page.
pub fn target(url: &str) -> Option<Target> {
    let (rest, fragment) = url.split_once('#').unwrap_or((url, ""));
    if let Some(id) = trailing_id(fragment).filter(|_| fragment.len() >= 32) {
        return Some(Target::Block(id));
    }
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    if let Some(p) = query.split('&').find_map(|kv| kv.strip_prefix("p=")).and_then(trailing_id) {
        return Some(Target::Page(p));
    }
    path.trim_end_matches('/').rsplit('/').next().and_then(trailing_id).map(Target::Page)
}

async fn token() -> Result<String> {
    prism_base::platform::secret(&prism_core::profile::service(KEYCHAIN_SERVICE))
        .await?
        .context(tr("no Notion token: add the integration's token (Settings, or prism token notion), and share the page with it"))
}

/// A call to Notion's API; its refusal said with its message (a page not shared, a token refused).
async fn call(req: reqwest::RequestBuilder, token: &str) -> Result<Value> {
    let res = req.timeout(TIMEOUT).bearer_auth(token).header("Notion-Version", VERSION).send().await.map_err(|e| anyhow::anyhow!("{}", e.without_url())).context("Notion")?;
    let status = res.status();
    let body: Value = res.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let msg = body.get("message").and_then(Value::as_str).unwrap_or_default();
        if status.as_u16() == 404 || status.as_u16() == 403 {
            bail!(tr!("Notion: {msg} (is the page shared with the integration?)", msg = msg));
        }
        bail!("Notion {}: {msg}", status.as_u16());
    }
    Ok(body)
}

/// An HTML block's file (`embed.url`, temporary and signed): an embed whose link is an `.html`
/// file's. Any other embed (a site, a video, a design) is not one, and never written over.
fn embed_url(block: &Value) -> Option<String> {
    if block.get("type")?.as_str()? != "embed" {
        return None;
    }
    let url = block.get("embed")?.get("url")?.as_str()?;
    let path = reqwest::Url::parse(url).ok()?.path().to_ascii_lowercase();
    (path.ends_with(".html") || path.ends_with(".htm")).then(|| url.to_string())
}

/// The HTML blocks a Notion link names: the block it points at, or every embed of its page (its
/// first level). Each one's id and its file's temporary link.
pub async fn embeds(url: &str) -> Result<Vec<(String, String)>> {
    let token = token().await?;
    let client = prism_base::http::client();
    match target(url).with_context(|| tr("not a Notion page's or block's link"))? {
        Target::Block(id) => {
            let block = call(client.get(format!("{API}/blocks/{id}")), &token).await?;
            let file = embed_url(&block).with_context(|| tr("this Notion block is not an HTML block (an embed of an .html file)"))?;
            Ok(vec![(id, file)])
        }
        Target::Page(id) => {
            // Its first level, every page of it (`has_more`, `next_cursor`).
            let (mut found, mut cursor) = (Vec::new(), None::<String>);
            for _ in 0..MAX_PAGES {
                let mut req = client.get(format!("{API}/blocks/{id}/children")).query(&[("page_size", PAGE_SIZE.to_string())]);
                if let Some(c) = &cursor {
                    req = req.query(&[("start_cursor", c)]);
                }
                let list = call(req, &token).await?;
                found.extend(list.get("results").and_then(Value::as_array).into_iter().flatten().filter_map(|b| Some((b.get("id")?.as_str()?.to_string(), embed_url(b)?))));
                cursor = list.get("has_more").and_then(Value::as_bool).unwrap_or(false).then(|| list.get("next_cursor").and_then(Value::as_str).map(String::from)).flatten();
                if cursor.is_none() {
                    break;
                }
            }
            ensure!(!found.is_empty(), tr("no HTML block on this Notion page (at its first level): copy the block's link (⋯ › Copy link to block)"));
            Ok(found)
        }
    }
}

/// Replaces the file of the HTML block `block` with `html` (named `name`): uploaded, then the
/// block pointed at it. Notion shows the new version at once.
pub async fn publish(block: &str, name: &str, html: Vec<u8>) -> Result<()> {
    ensure!(html.len() <= MAX_UPLOAD, tr!("{name} is larger than {mb} MB", name = name, mb = MAX_UPLOAD / 1024 / 1024));
    let token = token().await?;
    let client = prism_base::http::client();
    let upload = call(client.post(format!("{API}/file_uploads")).json(&json!({ "mode": "single_part", "filename": name, "content_type": "text/html" })), &token).await?;
    let id = upload.get("id").and_then(Value::as_str).context("Notion: no upload id")?.to_string();
    // multipart/form-data by hand: one part, `file`.
    let boundary = format!("prism-{}", uuid::Uuid::new_v4().simple());
    let safe = name.replace(['"', '\r', '\n'], "_");
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{safe}\"\r\nContent-Type: text/html\r\n\r\n").into_bytes();
    body.extend_from_slice(&html);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let sent = call(client.post(format!("{API}/file_uploads/{id}/send")).timeout(UPLOAD_TIMEOUT).header("Content-Type", format!("multipart/form-data; boundary={boundary}")).body(body), &token).await?;
    ensure!(sent.get("status").and_then(Value::as_str) == Some("uploaded"), "Notion: the file was not taken ({})", sent.get("status").and_then(Value::as_str).unwrap_or("?"));
    call(client.patch(format!("{API}/blocks/{block}")).json(&json!({ "embed": { "file_upload": { "id": id } } })), &token).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_a_notion_link_points_at() {
        let page = "https://www.notion.so/acme/Maquette-pipeline-1a2b3c4d5e6f47a8b9c0d1e2f3a4b5c6";
        assert!(is_notion(page) && is_notion("https://acme.notion.site/x") && is_notion("https://notion.so/x"));
        assert!(!is_notion("https://notion.so.evil.com/x") && !is_notion("https://file.notion.so/f/f/a/b/Maquette.html?sig=1") && !is_notion("http://www.notion.so/x"));
        assert_eq!(target(page), Some(Target::Page("1a2b3c4d-5e6f-47a8-b9c0-d1e2f3a4b5c6".into())));
        assert_eq!(target(&format!("{page}?pvs=4#0f1e2d3c4b5a49687766554433221100")), Some(Target::Block("0f1e2d3c-4b5a-4968-7766-554433221100".into())));
        assert_eq!(target("https://www.notion.so/acme/Home?p=1a2b3c4d5e6f47a8b9c0d1e2f3a4b5c6&pm=s"), Some(Target::Page("1a2b3c4d-5e6f-47a8-b9c0-d1e2f3a4b5c6".into())));
        assert_eq!(target("https://www.notion.so/1a2b3c4d-5e6f-47a8-b9c0-d1e2f3a4b5c6"), Some(Target::Page("1a2b3c4d-5e6f-47a8-b9c0-d1e2f3a4b5c6".into())));
        assert_eq!(target("https://www.notion.so/acme/Home"), None);
        assert_eq!(target(&format!("{page}#heading")), Some(Target::Page("1a2b3c4d-5e6f-47a8-b9c0-d1e2f3a4b5c6".into())), "a short fragment is no block");
    }

    #[test]
    fn finds_an_html_blocks_file() {
        let block = json!({ "object": "block", "id": "b", "type": "embed", "embed": { "url": "https://file.notion.so/f/x.html?sig=1", "caption": [] } });
        assert_eq!(embed_url(&block).as_deref(), Some("https://file.notion.so/f/x.html?sig=1"));
        assert_eq!(embed_url(&json!({ "type": "paragraph", "paragraph": {} })), None);
        for site in ["https://www.figma.com/embed?node=1", "https://www.youtube.com/embed/abc", "https://example.com/"] {
            assert_eq!(embed_url(&json!({ "type": "embed", "embed": { "url": site } })), None, "{site}: an embed, not an HTML block");
        }
    }
}
