//! What Prism does for the module (`host` of the WIT world), as `blocks::sdk` names it: GitLab, git,
//! programs, HTTP, the conventions and the ticket ids. Each answers at once: an `async fn` here
//! is ready on its first poll, so a collector's loop runs up to its next `pause` in one go.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;

use crate::bindings::prism::module::host;
use crate::types::Conventions;

pub(crate) fn log(level: u8, message: &str) {
    host::log(level, message);
}

/// Git in a clone: its standard output. Its blocks' clones only, read subcommands only.
pub async fn git(dir: impl AsRef<Path>, args: &[&str]) -> Result<String> {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    host::git(&dir.as_ref().to_string_lossy(), &args).map_err(|e| anyhow!(e))
}

/// A clone's default branch (`origin/HEAD`, otherwise the main one).
pub async fn default_branch(dir: &Path) -> String {
    git(dir, &["rev-parse", "--abbrev-ref", "origin/HEAD"])
        .await
        .ok()
        .and_then(|r| r.trim().strip_prefix("origin/").map(String::from))
        .unwrap_or_else(|| conventions::get().main_branch.clone())
}

/// GitLab through Prism (glab): its REST API.
pub mod gitlab {
    use super::*;

    /// GET of `path` (`projects/g%2Fp/pipelines?…`), its JSON.
    pub async fn api<T: DeserializeOwned>(path: &str) -> Result<T> {
        let body = host::gitlab_api("GET", path, None).map_err(|e| anyhow!(e))?;
        serde_json::from_str(&body).context("unreadable glab response")
    }

    /// `glab api` with its arguments (`--method`/`-X`, the path, `-f`/`-F`/`--field`/
    /// `--raw-field key=value`): only `api`, Prism gives no other glab command to a module.
    pub async fn glab<T: DeserializeOwned>(args: &[String]) -> Result<T> {
        let (method, path, body) = parse_glab(args)?;
        let body = (!body.is_empty()).then(|| serde_json::Value::Object(body).to_string());
        let text = host::gitlab_api(&method, &path, body.as_deref()).map_err(|e| anyhow!(e))?;
        serde_json::from_str(&text).context("unreadable glab response")
    }

    /// A raw GET (a file, an artifact), as text.
    pub async fn glab_text(path: &str) -> Result<String> {
        let bytes = host::gitlab_raw(path).map_err(|e| anyhow!(e))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// A project's path for the API: `group/project` → `group%2Fproject`.
    pub fn encode(project: &str) -> String {
        project.replace('/', "%2F")
    }

    /// Another project whose pipelines' job logs the window may read.
    pub fn allow_log_project(project: &str) {
        host::allow_log_project(project);
    }

    pub(crate) fn parse_glab(args: &[String]) -> Result<(String, String, serde_json::Map<String, serde_json::Value>)> {
        let mut it = args.iter();
        if it.next().map(String::as_str) != Some("api") {
            return Err(anyhow!("glab {}: Prism gives a module `glab api` only", args.first().map_or("", String::as_str)));
        }
        let (mut method, mut path, mut body) = ("GET".to_string(), None, serde_json::Map::new());
        while let Some(a) = it.next() {
            match a.as_str() {
                "-X" | "--method" => method = it.next().cloned().unwrap_or_default().to_ascii_uppercase(),
                "-f" | "-F" | "--field" | "--raw-field" => {
                    let raw = a == "-f" || a == "--raw-field";
                    let kv = it.next().ok_or_else(|| anyhow!("glab {a}: no key=value"))?;
                    let (k, v) = kv.split_once('=').ok_or_else(|| anyhow!("glab {a} {kv}: no ="))?;
                    let value = if raw { serde_json::Value::String(v.into()) } else { serde_json::from_str(v).unwrap_or_else(|_| serde_json::Value::String(v.into())) };
                    body.insert(k.into(), value);
                }
                other if other.starts_with('-') => return Err(anyhow!("glab api {other}: not given to a module")),
                other => path = Some(other.to_string()),
            }
        }
        if !body.is_empty() && method == "GET" {
            method = "POST".into();
        }
        Ok((method, path.ok_or_else(|| anyhow!("glab api: no path"))?, body))
    }
}

/// A program run by Prism (`exec`: one the manifest declares), as `tokio::process::Command`.
pub fn command(program: impl AsRef<str>) -> Command {
    Command { program: program.as_ref().to_string(), args: Vec::new() }
}

pub struct Command {
    program: String,
    args: Vec<String>,
}

/// A program's outcome.
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct ExitStatus(bool);

impl ExitStatus {
    pub fn success(&self) -> bool {
        self.0
    }
}

impl Command {
    pub fn arg(&mut self, arg: impl AsRef<str>) -> &mut Self {
        self.args.push(arg.as_ref().to_string());
        self
    }

    pub fn args<I: IntoIterator<Item = S>, S: AsRef<str>>(&mut self, args: I) -> &mut Self {
        self.args.extend(args.into_iter().map(|a| a.as_ref().to_string()));
        self
    }

    pub async fn output(&mut self) -> Result<Output> {
        match host::exec(&self.program, &self.args) {
            Ok(out) => Ok(Output { status: ExitStatus(true), stdout: out.into_bytes(), stderr: Vec::new() }),
            // Not declared: no program at all, as a missing one.
            Err(e) if e.contains("capabilities") => Err(anyhow!(e)),
            Err(e) => Ok(Output { status: ExitStatus(false), stdout: Vec::new(), stderr: e.into_bytes() }),
        }
    }
}

/// Team conventions, as Prism has them (`[forge]`, `[tracker]`, branches): given as each block
/// starts.
pub mod conventions {
    use super::*;
    pub use crate::types::Conventions;

    pub(crate) static CURRENT: Mutex<Option<Arc<Conventions>>> = Mutex::new(None);

    /// The current conventions.
    pub fn get() -> Arc<Conventions> {
        CURRENT.lock().unwrap().clone().unwrap_or_else(|| Arc::new(Conventions { main_branch: "main".into(), release_prefix: "release/".into(), gitlab_host: "gitlab.com".into(), ..Default::default() }))
    }

    /// Most recent release branch of a clone, otherwise the main branch.
    pub async fn latest_release(repo: &Path) -> Result<String> {
        let c = get();
        latest_release_in(repo, &c.main_branch, &c.release_prefix).await
    }

    pub async fn latest_release_in(repo: &Path, main: &str, release_prefix: &str) -> Result<String> {
        if release_prefix.is_empty() {
            return Ok(main.to_string());
        }
        let pattern = format!("refs/remotes/origin/{release_prefix}");
        let refs = super::git(repo, &["for-each-ref", "--sort=-creatordate", "--format=%(refname:short)", &pattern]).await?;
        Ok(refs.lines().next().and_then(|r| r.strip_prefix("origin/")).unwrap_or(main).to_string())
    }

    /// `search` parameter of the GitLab API listing the release branches, encoded.
    pub fn release_search() -> String {
        let raw = format!("^{}", get().release_prefix);
        raw.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
    }

    /// Link to the GitLab instance.
    pub fn web(path: &str) -> String {
        format!("https://{}/{}", get().gitlab_host, path.trim_start_matches('/'))
    }

    /// Image registry of the forge.
    pub fn registry() -> String {
        match get().forge {
            crate::types::Provider::Gitlab => format!("registry.{}", get().gitlab_host),
            crate::types::Provider::Github => "ghcr.io".into(),
        }
    }
}

/// Ticket ids in texts (`[tracker] ticket_prefix`), as Prism finds them.
pub mod ticket {
    use regex::Regex;

    fn patterns() -> (String, Option<Regex>, Option<Regex>) {
        let c = super::conventions::get();
        let prefix = c.ticket_prefix.to_uppercase();
        let ticket = (!c.ticket_patterns.is_empty()).then(|| Regex::new(&format!(r"(?i)\b(?:{})-\d+", c.ticket_patterns.join("|"))).ok()).flatten();
        let session = (!prefix.is_empty()).then(|| {
            let names = std::iter::once("ticket".to_string()).chain(std::iter::once(regex::escape(&prefix))).chain(c.branch_aliases.iter().map(|a| regex::escape(a))).collect::<Vec<_>>().join("|");
            Regex::new(&format!(r"(?i)\b(?:{names})-(\d{{3,5}})\b")).unwrap()
        });
        (prefix, ticket, session)
    }

    /// The ticket prefix (`SHOP`), uppercase.
    pub fn prefix() -> &'static str {
        static LAST: std::sync::Mutex<Option<&'static str>> = std::sync::Mutex::new(None);
        let now = patterns().0;
        let mut last = LAST.lock().unwrap();
        match *last {
            Some(p) if p == now => p,
            _ => last.insert(Box::leak(now.into_boxed_str())),
        }
    }

    pub fn find<'a>(texts: impl IntoIterator<Item = &'a str>) -> Option<String> {
        let re = patterns().1?;
        texts.into_iter().find_map(|t| re.find(t)).map(|m| m.as_str().to_uppercase())
    }

    pub fn find_all<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let Some(re) = patterns().1 else { return Vec::new() };
        let mut out: Vec<String> = Vec::new();
        for id in texts.into_iter().flat_map(|t| re.find_iter(t).map(|m| m.as_str().to_uppercase()).collect::<Vec<_>>()) {
            if !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }

    pub fn find_in_session(title: &str, branch: Option<&str>) -> Option<String> {
        let (prefix, _, session) = patterns();
        find([title, branch.unwrap_or_default()]).or_else(|| session?.captures(branch?).map(|c| format!("{prefix}-{}", &c[1])))
    }
}

/// Sets the conventions a block starts with.
pub(crate) fn set_conventions(c: Conventions) {
    *conventions::CURRENT.lock().unwrap() = Some(Arc::new(c));
}

#[cfg(test)]
mod tests {
    use super::gitlab::parse_glab;

    #[test]
    fn glab_api_arguments_become_a_request() {
        let s = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let (m, p, b) = parse_glab(&s(&["api", "projects/1/pipeline", "-f", "ref=main", "-F", "n=2"])).unwrap();
        assert_eq!((m.as_str(), p.as_str()), ("POST", "projects/1/pipeline"));
        assert_eq!(serde_json::Value::Object(b), serde_json::json!({ "ref": "main", "n": 2 }));
        assert_eq!(parse_glab(&s(&["api", "-X", "delete", "projects/1"])).unwrap().0, "DELETE");
        assert!(parse_glab(&s(&["ci", "status"])).unwrap_err().to_string().contains("only"));
        assert!(parse_glab(&s(&["api", "--paginate", "x"])).is_err());
    }
}
