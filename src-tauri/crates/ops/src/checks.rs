//! Live checks of the Settings view: is a config.toml value usable?
//! Project found on the forge (glab, gh), kube context reachable and namespace present (kubectl), folder
//! that exists or is a git repository, gcloud login; the logins and tokens of the forge and the
//! tracker, asked of the service itself ("who am I"). Each check runs locally with
//! the machine's tools, 8 s at most, and keeps nothing: only the verdict and a short message
//! are reported.

use prism_base::{i18n::tr, tr};
use std::{path::Path, time::Duration};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Check {
    GitlabProject { project: String },
    KubeContext { context: String },
    KubeNamespace { context: String, namespace: String },
    /// With the row's `project`: the clone must be one of it (its remotes).
    GitRepo {
        path: String,
        #[serde(default)]
        project: Option<String>,
    },
    Directory { path: String },
    Gcloud,
    /// The ClickUp token (CLICKUP_TOKEN, or this profile's keychain entry): who it signs in as.
    ClickupToken,
    /// The Jira token, with the site and the account of the form (saved or not).
    JiraToken { site: String, email: String },
    /// glab's or gh's login (`gitlab`, `github`), on the profile's instance.
    ForgeLogin { forge: crate::forge::Provider },
    /// tmux and the agents' CLI (`[agents] provider`).
    AgentTools {
        #[serde(default)]
        provider: crate::agents::provider::Provider,
        /// Its `[agents.<cli>] bin`; absent: its usual name.
        #[serde(default)]
        bin: Option<String>,
    },
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Outcome {
    pub ok: bool,
    pub message: String,
}

const TIMEOUT: Duration = Duration::from_secs(8);

fn ok(message: impl Into<String>) -> Outcome {
    Outcome { ok: true, message: message.into() }
}

fn ko(message: impl Into<String>) -> Outcome {
    Outcome { ok: false, message: message.into() }
}

pub async fn run(check: Check) -> Outcome {
    match tokio::time::timeout(TIMEOUT, check_inner(check)).await {
        Ok(o) => o,
        Err(_) => ko(tr("no answer within 8 s")),
    }
}

async fn check_inner(check: Check) -> Outcome {
    match check {
        Check::GitlabProject { project } => {
            if project.trim().is_empty() {
                return ko(tr("required"));
            }
            #[derive(Deserialize)]
            struct Project {
                path_with_namespace: String,
            }
            #[derive(Deserialize)]
            struct Repository {
                full_name: String,
            }
            // The forge of the profile, as read at launch.
            match crate::conventions::get().forge {
                crate::forge::Provider::Gitlab => match crate::collectors::gitlab::api::<Project>(&format!("projects/{}", project.trim().replace('/', "%2F"))).await {
                    Ok(p) => ok(tr!("found: {name}", name = p.path_with_namespace)),
                    Err(_) => ko(tr("not found, or no access with glab")),
                },
                crate::forge::Provider::Github => match crate::collectors::github::api::<Repository>(&format!("repos/{}", project.trim())).await {
                    Ok(r) => ok(tr!("found: {name}", name = r.full_name)),
                    Err(_) => ko(tr("not found (owner/repo), or no access with gh")),
                },
            }
        }
        Check::KubeContext { context } => match output("kubectl", &["--context", &context, "get", "--raw", "/version", "--request-timeout=6s"]).await {
            Ok(_) => ok(tr("reachable")),
            Err(e) if e.contains("does not exist") || e.contains("context was not found") => ko(tr("context not in kubeconfig")),
            Err(e) => ko(tr!("unreachable: {err}", err = short(&e))),
        },
        Check::KubeNamespace { context, namespace } => {
            if namespace.trim().is_empty() {
                return ko(tr("required"));
            }
            match output("kubectl", &["--context", &context, "get", "namespace", namespace.trim(), "-o", "name", "--request-timeout=6s"]).await {
                Ok(_) => ok(tr("namespace found")),
                Err(e) if e.contains("NotFound") || e.contains("not found") => ko(tr("no such namespace")),
                Err(e) if e.contains("Forbidden") || e.contains("forbidden") => ok(tr("exists? not allowed to list namespaces")),
                Err(e) => ko(tr!("unreachable: {err}", err = short(&e))),
            }
        }
        Check::GitRepo { path, project } => {
            let dir = crate::config::expand_home(Path::new(path.trim()));
            if !dir.is_dir() {
                return ko(tr("folder not found"));
            }
            if let Some(Err(e)) = project.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(|p| crate::clones::verify(&dir, p)) {
                return ko(e);
            }
            match output("git", &["-C", &dir.to_string_lossy(), "rev-parse", "--abbrev-ref", "HEAD"]).await {
                Ok(branch) => ok(tr!("git repository · {branch}", branch = branch.trim())),
                Err(_) => ko(tr("not a git repository")),
            }
        }
        Check::Directory { path } => {
            let dir = crate::config::expand_home(Path::new(path.trim()));
            if dir.is_dir() { ok(tr("folder found")) } else { ko(tr("folder not found")) }
        }
        Check::AgentTools { provider, bin } => {
            let bin = bin.as_deref().map(str::trim).filter(|b| !b.is_empty());
            let program = bin.map_or_else(|| provider.program().to_string(), |b| crate::config::expand_home(Path::new(b)).display().to_string());
            // Prism's own sessions in place of tmux (Windows): only the CLI to find.
            let tmux = if prism_core::agents::local_sessions() { Ok(String::new()) } else { output("tmux", &["-V"]).await };
            let cli = output(&program, &["--version"]).await;
            match (tmux, cli) {
                (Ok(t), Ok(c)) if t.trim().is_empty() => ok(format!("{} {}", bin.unwrap_or(provider.program()), c.trim())),
                (Ok(t), Ok(c)) => ok(format!("{} · {} {}", t.trim(), bin.unwrap_or(provider.program()), c.trim())),
                (Err(_), _) => ko(tr!("tmux not found ({hint})", hint = crate::platform::install_hint("tmux"))),
                (_, Err(_)) if bin.is_some() => ko(tr!("{program} not found", program = program)),
                (_, Err(_)) => ko(tr!("{program} not found in the PATH", program = provider.program())),
            }
        }
        Check::ClickupToken => match crate::collectors::clickup::whoami().await {
            Ok(who) => ok(tr!("signed in as {who}", who = who)),
            Err(e) => ko(token_error("ClickUp", &format!("{e:#}"))),
        },
        Check::JiraToken { site, email } => {
            if site.trim().is_empty() || email.trim().is_empty() {
                return ko(tr("set the site and the account first"));
            }
            match crate::collectors::jira::whoami(&site, &email).await {
                Ok(who) => ok(tr!("signed in as {who}", who = who)),
                Err(e) => ko(token_error("Jira", &format!("{e:#}"))),
            }
        }
        Check::ForgeLogin { forge } => {
            #[derive(Deserialize)]
            struct User {
                #[serde(alias = "login")]
                username: String,
            }
            let (cli, login) = match forge {
                crate::forge::Provider::Gitlab => (crate::forge::cli::Cli::Glab, "glab auth login"),
                crate::forge::Provider::Github => (crate::forge::cli::Cli::Gh, "gh auth login"),
            };
            match cli.json::<User, _>(&["api", "user"]).await {
                Ok(u) => ok(tr!("signed in as {who}", who = u.username)),
                Err(e) => {
                    let e = format!("{e:#}");
                    let tool = login.split(' ').next().unwrap_or(login);
                    if e.contains("not found in PATH") {
                        ko(tr!("{tool} is not installed ({hint})", tool = tool, hint = crate::platform::install_hint(tool)))
                    } else if unreachable(&e) {
                        ko(tr!("the forge is unreachable (network, VPN?): {err}", err = short(&e)))
                    } else {
                        ko(tr!("not signed in: run the {tool} login command in a terminal, then Test ({err})", tool = tool, err = short(&e)))
                    }
                }
            }
        }
        Check::Gcloud => match output("gcloud", &["auth", "print-access-token", "--quiet"]).await {
            // The token is neither kept nor returned: its mere presence answers.
            Ok(_) => ok(tr("gcloud logged in")),
            Err(_) => ko(tr("gcloud not logged in (gcloud auth login)")),
        },
    }
}

/// A network failure rather than a refusal: the host not found, not answering.
fn unreachable(e: &str) -> bool {
    ["dial tcp", "no such host", "lookup ", "connection refused", "timed out", "timeout", "network is unreachable", "error sending request"].iter().any(|m| e.contains(m))
}

/// A tracker's token refused or missing, in words: where to fix it rather than the raw answer.
fn token_error(service: &str, e: &str) -> String {
    if e.contains("missing token") {
        tr("no token yet: paste it above, then Save to keychain").into()
    } else if e.contains("HTTP 401") || e.contains("HTTP 403") {
        tr!("{service} refused this token: mistyped, revoked, or regenerated since? Create a new one and paste it above", service = service)
    } else if unreachable(e) {
        tr!("{service} unreachable (network, VPN?): {err}", service = service, err = short(e))
    } else if e.contains("HTTP 404") {
        tr!("{service} does not know this site", service = service)
    } else {
        tr!("{service} unreachable: {err}", service = service, err = short(e))
    }
}

/// Standard output of a command, or its error (stderr).
async fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let out = crate::platform::command(program).args(args).stdin(std::process::Stdio::null()).kill_on_drop(true).output().await.map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Last useful line of an error, shortened.
fn short(e: &str) -> String {
    let line = e.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or(e);
    line.chars().take(90).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A token refused or missing says where to fix it, not the raw answer.
    #[test]
    fn token_errors_in_words() {
        assert!(token_error("ClickUp", "missing token: set CLICKUP_TOKEN or the \"prism-clickup\" keychain entry").starts_with("no token yet"));
        assert!(token_error("Jira", "Jira /rest/api/3/myself: HTTP 401 Unauthorized").starts_with("Jira refused this token"));
        assert!(token_error("Jira", "Jira /rest/api/3/myself: HTTP 404 Not Found").contains("does not know this site"));
        assert!(token_error("ClickUp", "error sending request").starts_with("ClickUp unreachable (network"));
        assert!(unreachable("Get \"https://g.example/api/v4/user\": dial tcp: lookup g.example: no such host") && !unreachable("401 Unauthorized"));
    }

    #[tokio::test]
    async fn local_checks() {
        let repo = concat!(env!("CARGO_MANIFEST_DIR"), "/..").to_string();
        assert!(run(Check::GitRepo { path: repo.clone(), project: None }).await.ok);
        assert_eq!(run(Check::GitRepo { path: "/nonexistent/prism".into(), project: None }).await, ko(tr("folder not found")));
        assert_eq!(run(Check::GitRepo { path: "/".into(), project: None }).await, ko(tr("not a git repository")));
        assert_eq!(run(Check::Directory { path: repo }).await, ok(tr("folder found")));
        assert_eq!(run(Check::GitlabProject { project: " ".into() }).await, ko(tr("required")));
    }
}
