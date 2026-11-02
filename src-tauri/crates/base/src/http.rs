//! What the HTTP calls share: one client, and the encoding of a URL's parts.

use std::sync::LazyLock;
use crate::tr;

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

/// The process's client, without a default timeout (each request sets its own): its connections
/// are reused from one call to the next. Cheap to clone.
pub fn client() -> reqwest::Client {
    CLIENT.clone()
}

/// Percent-encoding of a URL's part (a path segment, a query value): all but the unreserved
/// characters of RFC 3986.
pub fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A service's refusal of a call: its status and the start of what it said (its error messages
/// tell what is wrong: an unknown ticket, permissions…). `status` tells a refused token (401,
/// 403) from the rest.
#[derive(Debug)]
pub struct HttpError {
    pub service: &'static str,
    /// The path called, without its query.
    pub path: String,
    pub status: reqwest::StatusCode,
    pub detail: String,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}: HTTP {} {}", self.service, self.path, self.status, self.detail)
    }
}

impl std::error::Error for HttpError {}

/// The body of `service`'s answer to `path`; refused, an `HttpError`.
pub async fn body(service: &'static str, path: &str, resp: reqwest::Response) -> anyhow::Result<String> {
    let status = resp.status();
    if !status.is_success() {
        let detail = resp.text().await.unwrap_or_default().chars().take(300).collect::<String>().trim().to_string();
        return Err(HttpError { service, path: path.split('?').next().unwrap_or(path).to_string(), status, detail }.into());
    }
    use anyhow::Context;
    resp.text().await.with_context(|| tr!("{service}: unreadable response for {path}", service = service, path = path))
}

/// A body read as JSON; an empty one (a write answered without a body, a 204) is `null`.
pub fn json<T: serde::de::DeserializeOwned>(service: &str, path: &str, body: &str) -> anyhow::Result<T> {
    use anyhow::Context;
    serde_json::from_str(if body.trim().is_empty() { "null" } else { body }).with_context(|| tr!("{service}: unreadable response for {path}", service = service, path = path.split('?').next().unwrap_or(path)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_all_but_the_unreserved() {
        assert_eq!(super::encode("a-Z_0.~"), "a-Z_0.~");
        assert_eq!(super::encode("sprint 12/é&x=1"), "sprint%2012%2F%C3%A9%26x%3D1");
    }

    #[test]
    fn an_empty_body_reads_as_null() {
        assert_eq!(super::json::<Option<u32>>("ClickUp", "/task/1", " ").unwrap(), None);
        assert!(super::json::<u32>("Jira", "/issue?x=1", "<html>").unwrap_err().to_string().ends_with("for /issue"));
    }
}
