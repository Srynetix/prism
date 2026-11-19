//! A web page imported into an agent's Web view (Import…): a file's link (a Notion page's
//! attachment, any HTML file online) downloaded into the agent's folder, `.prism/web/`, which its
//! git ignores, and served from there as `@imported/<name>` — Prism's annotation script in it, as
//! any page it serves (Comment). Its origin is kept beside it, for the prompt to say where it
//! came from and where to put it back.

use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use prism_base::tr;
use serde::{Deserialize, Serialize};

/// The Web view's address of an imported page: `@imported/<name>`.
pub const IMPORTED: &str = "@imported";
/// Where the imported pages are, in the agent's folder.
const DIR: &str = ".prism/web";
/// How long a download may take.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Where an imported page came from: its link (without its query: a signed link's keys are not
/// kept, and they expire), and the page it belongs to (a Notion page), when given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "WebOrigin"))]
pub struct Origin {
    pub url: String,
    pub page: Option<String>,
}

/// The imported pages' folder in `folder` (the agent's).
pub fn root(folder: &Path) -> PathBuf {
    folder.join(DIR)
}

/// `url` without its query and fragment.
fn bare(url: &str) -> &str {
    url.split(['?', '#']).next().unwrap_or(url)
}

/// An `https:` link, its host named: nothing else is fetched.
fn https(url: &str) -> Result<()> {
    let rest = url.strip_prefix("https://").with_context(|| tr!("{url}: only an https link is imported", url = url))?;
    ensure!(!rest.split(['/', '?', '#']).next().unwrap_or_default().is_empty(), tr!("{url}: not a link", url = url));
    Ok(())
}

/// The file's name from its link: its last part, decoded, as `.html`.
fn name_of(url: &str) -> String {
    let last = bare(url).trim_end_matches('/').rsplit('/').next().unwrap_or_default();
    let name = crate::files::percent_decode(last);
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") { name } else { format!("{}.html", if name.is_empty() { "page" } else { &name }) }
}

/// HTML: said so by its type or its name, or starting as a page does; never binary.
fn is_html(body: &[u8], kind: &str, name: &str) -> bool {
    if body.contains(&0) {
        return false;
    }
    let head = String::from_utf8_lossy(&body[..body.len().min(512)]).trim_start_matches('\u{feff}').trim_start().to_ascii_lowercase();
    kind.contains("text/html") || head.starts_with("<!doctype html") || head.starts_with("<html") || (kind.is_empty() || kind.starts_with("application/octet-stream") || kind.starts_with("text/")) && (name.ends_with(".html") || name.ends_with(".htm")) && head.starts_with('<')
}

/// Downloads `url` into `folder`'s `.prism/web/` (no credentials sent: a public or signed link),
/// keeps its origin beside it, and returns its Web view address (`@imported/<name>`).
pub async fn import(folder: &Path, url: &str, page: Option<&str>) -> Result<String> {
    let url = url.trim();
    https(url)?;
    let page = page.map(str::trim).filter(|p| !p.is_empty());
    if let Some(p) = page {
        https(p)?;
    }
    let res = prism_base::http::client().get(url).timeout(TIMEOUT).send().await.with_context(|| tr!("downloading {url}", url = bare(url)))?;
    let status = res.status();
    if !status.is_success() {
        bail!(tr!("{url} answered {status}: an expired link? Copy it again", url = bare(url), status = status.as_u16()));
    }
    let kind = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default().to_ascii_lowercase();
    let body = res.bytes().await.with_context(|| tr!("downloading {url}", url = bare(url)))?;
    let name = name_of(url);
    if !is_html(&body, &kind, &name.to_ascii_lowercase()) {
        bail!(tr!("{url} is not an HTML page", url = bare(url)));
    }
    let dir = root(folder);
    for d in [folder.join(".prism"), dir.clone()] {
        if d.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("{} is a link: nothing imported there", d.display());
        }
    }
    let path = crate::attach::save(&dir, &name, &body)?;
    crate::attach::exclude(folder);
    let origin = Origin { url: bare(url).to_string(), page: page.map(String::from) };
    std::fs::write(origin_path(&path), serde_json::to_vec_pretty(&origin)?).context("keeping its origin")?;
    Ok(format!("{IMPORTED}/{}", path.file_name().unwrap_or_default().to_string_lossy()))
}

/// `<name>.origin.json`, beside the page.
fn origin_path(page: &Path) -> PathBuf {
    let mut name = page.file_name().unwrap_or_default().to_os_string();
    name.push(".origin.json");
    page.with_file_name(name)
}

/// Where an imported page (`rel`, under `@imported/`) came from, when said.
pub fn origin(folder: &Path, rel: &str) -> Option<Origin> {
    let page = root(folder).join(rel);
    std::fs::read(origin_path(&page)).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

/// The imported pages, as the Web view's picker lists them (`@imported/<name>`).
pub fn pages(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root(folder)) else { return Vec::new() };
    let mut out: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            let n = n.to_ascii_lowercase();
            n.ends_with(".html") || n.ends_with(".htm")
        })
        .map(|n| format!("{IMPORTED}/{n}"))
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_checks_what_it_imports() {
        assert_eq!(name_of("https://file.notion.so/f/f/abc/Maquette%20v2.html?table=block&expirationTimestamp=1&signature=xyz"), "Maquette v2.html");
        assert_eq!(name_of("https://example.com/report"), "report.html");
        assert_eq!(name_of("https://example.com/"), "example.com.html");
        assert_eq!(bare("https://x.so/a.html?signature=s#top"), "https://x.so/a.html");
        assert!(https("https://x.so/a.html").is_ok());
        for bad in ["http://x.so/a.html", "file:///etc/passwd", "https:///a.html", "javascript:alert(1)"] {
            assert!(https(bad).is_err(), "{bad}");
        }
        assert!(is_html(b"<!DOCTYPE html><p>", "application/octet-stream", "a.html"));
        assert!(is_html(b"\xef\xbb\xbf  <html>", "", "a"));
        assert!(is_html(b"<div>x</div>", "text/html; charset=utf-8", "a"));
        assert!(!is_html(b"%PDF-1.7", "application/pdf", "a.pdf"));
        assert!(!is_html(b"<html>\0\0", "text/html", "a.html"), "binary");
        assert!(!is_html(b"{\"a\":1}", "application/json", "a.html"));
    }

    #[test]
    fn lists_its_pages_and_their_origin() {
        let folder = std::env::temp_dir().join(format!("prism-web-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let page = crate::attach::save(&root(&folder), "Maquette.html", b"<html></html>").unwrap();
        std::fs::write(origin_path(&page), serde_json::to_vec(&Origin { url: "https://file.notion.so/a.html".into(), page: Some("https://notion.so/p".into()) }).unwrap()).unwrap();
        assert_eq!(pages(&folder), ["@imported/Maquette.html"]);
        assert_eq!(origin(&folder, "Maquette.html").unwrap().page.as_deref(), Some("https://notion.so/p"));
        assert_eq!(origin(&folder, "other.html"), None);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
