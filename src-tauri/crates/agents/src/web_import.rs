//! A web page imported into an agent's Web view (Import…): a file's link (a Notion page's
//! attachment, any HTML file online) downloaded into the agent's folder, `.prism/web/`, which its
//! git ignores, and served from there as `@imported/<name>` — Prism's annotation script in it, as
//! any page it serves (Comment). Its origin is kept beside it, for the prompt to say where it
//! came from and where to put it back; the same link imported again replaces its copy.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{bail, ensure, Context, Result};
use prism_base::{i18n::tr, tr};
use serde::{Deserialize, Serialize};

/// The Web view's address of an imported page: `@imported/<name>`.
pub const IMPORTED: &str = "@imported";
/// Where the imported pages are, in the agent's folder.
const DIR: &str = ".prism/web";
/// How long a download may take.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// The largest page imported: an attachment's bound.
const MAX: usize = crate::attach::MAX_BYTES;

/// Where an imported page came from: its link (without its query: a signed link's keys are not
/// kept, and they expire), and the page it belongs to (a Notion page), when given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "WebOrigin"))]
pub struct Origin {
    pub url: String,
    pub page: Option<String>,
    /// A Notion HTML block's id: its file read from it, and put back there (Update on Notion).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub block: Option<String>,
    /// The block's file's own name, which it keeps when put back (here it may have another).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub file: Option<String>,
}

/// The imported pages' folder in `folder` (the agent's).
pub fn root(folder: &Path) -> PathBuf {
    folder.join(DIR)
}

/// `url` without its query and fragment: what may be said and kept of a signed link.
fn bare(url: &str) -> &str {
    url.split(['?', '#']).next().unwrap_or(url)
}

/// The host of an `https:` link (no credentials in it), as the client will read it (an address
/// written otherwise, `127.1` or `2130706433`, as the address it is); `None` for anything else.
fn https_host(url: &str) -> Option<url::Host<String>> {
    let u = reqwest::Url::parse(url.trim()).ok()?;
    (u.scheme() == "https" && u.username().is_empty() && u.password().is_none()).then(|| u.host().map(|h| h.to_owned())).flatten()
}

/// An address of this computer or its network: loopback, private, link-local, shared, unnamed.
fn private_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, ..] = ip.octets();
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast() || a == 0 || (a == 100 && (64..128).contains(&b))
        }
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified() || (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80 || ip.to_ipv4_mapped().is_some_and(|v4| private_ip(IpAddr::V4(v4))),
    }
}

/// Not this computer nor its network by its host: no such address, no `localhost`, no `.local`
/// (a name is checked again once resolved: `PublicOnly`).
fn public(host: &url::Host<String>) -> bool {
    match host {
        url::Host::Domain(d) => {
            let d = d.trim_end_matches('.').to_ascii_lowercase();
            !(d == "localhost" || d.ends_with(".localhost") || d.ends_with(".local") || d.ends_with(".internal"))
        }
        url::Host::Ipv4(ip) => !private_ip((*ip).into()),
        url::Host::Ipv6(ip) => !private_ip((*ip).into()),
    }
}

/// An `https:` link to a public host: nothing else is fetched, the redirects included.
fn allowed(url: &str) -> bool {
    https_host(url).is_some_and(|h| public(&h))
}

/// Names resolved to public addresses only: a public name pointing at this computer or its
/// network (a rebinding) is refused once resolved.
struct PublicOnly;

impl reqwest::dns::Resolve for PublicOnly {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let found: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.filter(|a| !private_ip(a.ip())).collect();
            if found.is_empty() {
                return Err(format!("{host}: not a public address").into());
            }
            Ok(Box::new(found.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Its own client: redirects followed only to an allowed link (an `https` one to a public host,
/// five at most), names resolved to public addresses only, no cookie, no credentials. Not built:
/// nothing is fetched (never a client without these guards).
static CLIENT: LazyLock<std::result::Result<reqwest::Client, String>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|a| {
            if a.previous().len() >= 5 {
                a.error("too many redirects")
            } else if allowed(a.url().as_str()) {
                a.follow()
            } else {
                a.stop()
            }
        }))
        .dns_resolver(std::sync::Arc::new(PublicOnly))
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
});

/// The file's name from its link: its last part, decoded and made safe, as `.html`.
fn name_of(url: &str) -> String {
    let last = bare(url).trim_end_matches('/').rsplit('/').next().unwrap_or_default();
    let name = crate::attach::safe_name(&crate::files::percent_decode(last));
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") { name } else { format!("{name}.html") }
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
/// keeps its origin beside it, and returns its Web view address (`@imported/<name>`). The same
/// link imported before: its copy replaced (its remarks kept on it).
pub async fn import(folder: &Path, url: &str, page: Option<&str>) -> Result<String> {
    let url = url.trim();
    let shown = bare(url).to_string();
    ensure!(https_host(url).is_some(), tr!("{url}: only an https link is imported", url = shown));
    ensure!(allowed(url), tr!("{url}: not a page online (this computer or its network)", url = shown));
    // A Notion page's or block's link: its HTML block read by Notion's API, put back there later.
    if crate::notion::is_notion(url) {
        let mut found = crate::notion::embeds(url).await?;
        ensure!(found.len() == 1, tr!("{n} HTML blocks on this Notion page: copy the link of the one to comment (⋯ › Copy link to block)", n = found.len()));
        let (block, file) = found.remove(0);
        let (body, kind) = download(&file, "Notion").await?;
        let name = name_of(&file);
        if !is_html(&body, &kind, &name.to_ascii_lowercase()) {
            bail!(tr("this Notion block is not an HTML block (an embed of an .html file)"));
        }
        let origin = Origin { url: shown.clone(), page: Some(bare(url).to_string()), block: Some(block), file: Some(name.clone()) };
        let folder = folder.to_path_buf();
        return tokio::task::spawn_blocking(move || keep(&folder, &name, &body, &origin)).await?;
    }
    let page = page.map(str::trim).filter(|p| !p.is_empty()).map(String::from);
    if let Some(p) = &page {
        ensure!(https_host(p).is_some(), tr!("{url}: only an https link is imported", url = bare(p)));
    }
    let (body, kind) = download(url, &shown).await?;
    let name = name_of(url);
    if !is_html(&body, &kind, &name.to_ascii_lowercase()) {
        bail!(tr!("{url} is not an HTML page", url = shown));
    }
    let folder = folder.to_path_buf();
    let origin = Origin { url: shown, page: page.map(|p| bare(&p).to_string()), block: None, file: None };
    tokio::task::spawn_blocking(move || keep(&folder, &name, &body, &origin)).await?
}

/// `url` read (an allowed one, its redirects too), as it comes and bounded: its bytes and type.
/// `shown`: how it is named in a message (never its query: a signed link's keys).
async fn download(url: &str, shown: &str) -> Result<(Vec<u8>, String)> {
    ensure!(allowed(url), tr!("{url}: not a page online (this computer or its network)", url = shown));
    let client = CLIENT.as_ref().map_err(|e| anyhow::anyhow!("no client to download with: {e}"))?;
    let mut res = client.get(url).send().await.map_err(|e| anyhow::anyhow!("{}", e.without_url())).with_context(|| tr!("downloading {url}", url = shown))?;
    let status = res.status();
    if !status.is_success() {
        bail!(tr!("{url} answered {status}: an expired link? Copy it again", url = shown, status = status.as_u16()));
    }
    if res.content_length().is_some_and(|n| n > MAX as u64) {
        bail!(tr!("{name} is larger than {mb} MB", name = shown, mb = MAX / 1024 / 1024));
    }
    let kind = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default().to_ascii_lowercase();
    // Read as it comes, stopped past the bound: a link to a huge file is not held whole.
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| anyhow::anyhow!("{}", e.without_url())).with_context(|| tr!("downloading {url}", url = shown))? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX {
            bail!(tr!("{name} is larger than {mb} MB", name = shown, mb = MAX / 1024 / 1024));
        }
    }
    Ok((body, kind))
}

/// Writes the page and its origin (off the async workers: files, and git's exclude file).
fn keep(folder: &Path, name: &str, body: &[u8], origin: &Origin) -> Result<String> {
    let dir = root(folder);
    for d in [folder.join(".prism"), dir.clone()] {
        if d.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("{} is a link: nothing imported there", d.display());
        }
    }
    let path = match imported_from(&dir, origin) {
        Some(path) if !path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) => {
            ensure!(body.len() <= MAX, tr!("{name} is larger than {mb} MB", name = name, mb = MAX / 1024 / 1024));
            std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
            path
        }
        _ => crate::attach::save(&dir, name, body)?,
    };
    crate::attach::exclude(folder);
    std::fs::write(origin_path(&path), serde_json::to_vec_pretty(origin)?).context("keeping its origin")?;
    Ok(format!("{IMPORTED}/{}", path.file_name().unwrap_or_default().to_string_lossy()))
}

/// The page already imported from the same place: the same Notion block, or the same link.
fn imported_from(dir: &Path, from: &Origin) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        let page = name.strip_suffix(".origin.json")?.to_string();
        let origin: Origin = serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
        let same = match (&from.block, &origin.block) {
            (Some(a), Some(b)) => a == b,
            (None, None) => origin.url == from.url,
            _ => false,
        };
        same.then(|| dir.join(page)).filter(|p| p.is_file())
    })
}

/// The imported page `rel` put back on its Notion HTML block (its file replaced there).
pub async fn publish(folder: &Path, rel: &str) -> Result<()> {
    let from = origin(folder, rel).filter(|o| o.block.is_some()).with_context(|| tr("this page did not come from a Notion HTML block"))?;
    let path = root(folder).join(own_name(rel)?);
    let html = tokio::fs::read(&path).await.with_context(|| format!("reading {}", path.display()))?;
    crate::notion::publish(from.block.as_deref().unwrap_or_default(), from.file.as_deref().unwrap_or(rel), html).await
}

/// `<name>.origin.json`, beside the page.
fn origin_path(page: &Path) -> PathBuf {
    let mut name = page.file_name().unwrap_or_default().to_os_string();
    name.push(".origin.json");
    page.with_file_name(name)
}

/// One name of `.prism/web/`, never a path.
fn own_name(rel: &str) -> Result<&str> {
    ensure!(!rel.is_empty() && !rel.contains(['/', '\\']) && !rel.starts_with('.'), tr!("{name}: not an imported page", name = rel));
    Ok(rel)
}

/// Where an imported page (`rel`, under `@imported/`) came from, when said.
pub fn origin(folder: &Path, rel: &str) -> Option<Origin> {
    let page = root(folder).join(own_name(rel).ok()?);
    std::fs::read(origin_path(&page)).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

/// An imported page removed, its origin with it.
pub fn remove(folder: &Path, rel: &str) -> Result<()> {
    let page = root(folder).join(own_name(rel)?);
    match std::fs::remove_file(&page) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("removing {}", page.display())),
    }
    let _ = std::fs::remove_file(origin_path(&page));
    Ok(())
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
        assert_eq!(name_of("https://file.notion.so/f/f/abc/Maquette%20v2.html?table=block&expirationTimestamp=1&signature=xyz"), "Maquette-v2.html");
        assert_eq!(name_of("https://example.com/report"), "report.html");
        assert!(name_of("https://file.notion.so/f/%E6%97%A5%E6%9C%AC.html").ends_with(".html"), "nothing safe left of its name: still a page");
        assert!(name_of("https://example.com/%2e%2e").ends_with(".html"));
        assert_eq!(bare("https://x.so/a.html?signature=s#top"), "https://x.so/a.html");
        assert!(allowed("https://file.notion.so/a.html") && allowed("https://8.8.8.8/a"));
        for bad in [
            "http://x.so/a.html",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://user:pw@x.so/a",
            "https://localhost:8080/a",
            "https://127.0.0.1/a",
            "https://10.0.0.2/a",
            "https://192.168.1.1/a",
            "https://169.254.169.254/latest",
            "https://[::1]/a",
            "https://[fd00::1]/a",
            "https://printer.local/a",
            "https://127.1/a",
            "https://2130706433/a",
            "https://0x7f.0.0.1/a",
            "https://localhost./a",
            "https://metadata.google.internal/a",
        ] {
            assert!(!allowed(bad), "{bad}");
        }
        assert!(is_html(b"<!DOCTYPE html><p>", "application/octet-stream", "a.html"));
        assert!(is_html(b"\xef\xbb\xbf  <html>", "", "a"));
        assert!(is_html(b"<div>x</div>", "text/html; charset=utf-8", "a"));
        assert!(!is_html(b"%PDF-1.7", "application/pdf", "a.pdf"));
        assert!(!is_html(b"<html>\0\0", "text/html", "a.html"), "binary");
        assert!(!is_html(b"{\"a\":1}", "application/json", "a.html"));
    }

    #[test]
    fn keeps_replaces_lists_and_removes_its_pages() {
        let folder = std::env::temp_dir().join(format!("prism-web-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let notion = Origin { url: "https://file.notion.so/f/Maquette.html".into(), page: Some("https://notion.so/p".into()), block: None, file: None };
        assert_eq!(keep(&folder, "Maquette.html", b"<html>v1</html>", &notion).unwrap(), "@imported/Maquette.html");
        // The same link again: its copy replaced, not a second one.
        assert_eq!(keep(&folder, "Maquette.html", b"<html>v2</html>", &notion).unwrap(), "@imported/Maquette.html");
        assert_eq!(std::fs::read_to_string(root(&folder).join("Maquette.html")).unwrap(), "<html>v2</html>");
        let other = Origin { url: "https://example.com/Maquette.html".into(), page: None, block: None, file: None };
        assert_eq!(keep(&folder, "Maquette.html", b"<html></html>", &other).unwrap(), "@imported/Maquette-2.html");
        assert_eq!(pages(&folder), ["@imported/Maquette-2.html", "@imported/Maquette.html"]);
        assert_eq!(origin(&folder, "Maquette.html").unwrap().page.as_deref(), Some("https://notion.so/p"));
        assert_eq!(origin(&folder, "../x"), None);
        assert!(remove(&folder, "../../etc").is_err());
        remove(&folder, "Maquette.html").unwrap();
        assert_eq!(pages(&folder), ["@imported/Maquette-2.html"]);
        assert_eq!(origin(&folder, "Maquette.html"), None);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
