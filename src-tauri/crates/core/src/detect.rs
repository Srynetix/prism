//! What the pages wizard finds on this machine before asking anything: the forges' CLIs and whom
//! they are signed in as, the agent CLIs, the git repositories of the usual folders, the kubectl
//! contexts, the trackers within reach. Each step of the wizard starts from it.
//!
//! Only verdicts leave: "signed in as <login>", a path, a version. No token is read (the trackers'
//! are only known to be set in the environment), and a repository is known by its `.git/config`
//! alone, git never run. Every probe has a short timeout: a CLI waiting on the network leaves its
//! line empty rather than holding the wizard.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::agents::provider::Provider as AgentProvider;
use crate::forge::Provider as ForgeProvider;

/// The longest one probe takes (`gh api user`, `kubectl config`).
const PROBE: Duration = Duration::from_secs(6);
/// Depth of the search below a folder (`~/Development/team/app`: 2).
const DEPTH: usize = 3;
/// Folders visited at most, all roots together: a huge home stays quick.
const MAX_VISITS: usize = 6_000;
/// Repositories reported at most, the most recently touched first.
const MAX_REPOS: usize = 60;
/// Folders a search does not enter: dependencies, builds, worktrees, hidden ones.
const SKIP: [&str; 7] = ["node_modules", "target", "vendor", "dist", "build", "Library", "Applications"];
/// Where developers keep their clones, under the home folder, besides the working folders.
const USUAL: [&str; 12] = ["Development", "Developer", "dev", "Dev", "code", "Code", "src", "Projects", "projects", "workspace", "git", "repos"];

/// Everything found, for the wizard's steps.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SetupDetection"))]
pub struct Detection {
    /// GitLab (glab) then GitHub (gh).
    pub forges: Vec<ForgeFound>,
    /// Every agent CLI Prism runs, found or not, in `AgentProvider::ALL` order.
    pub agents: Vec<AgentFound>,
    /// tmux, which hosts the agents: its version, absent when not installed.
    pub tmux: Option<String>,
    /// The folders searched for repositories (`~/…`), that exist.
    pub folders: Vec<String>,
    pub repos: Vec<RepoFound>,
    /// kubectl is installed.
    pub kubectl: bool,
    /// Its contexts (kubeconfig), none when kubectl is absent.
    pub kube_contexts: Vec<String>,
    pub trackers: TrackersFound,
}

/// A forge's CLI: installed, and whom it is signed in as on `host`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ForgeFound {
    pub provider: ForgeProvider,
    /// `glab` or `gh` is in the PATH.
    pub installed: bool,
    /// The instance asked: `github.com`, or the GitLab host (the config's, or the one most of the
    /// repositories found point to).
    pub host: String,
    /// The account's username; absent: not signed in (or the instance did not answer).
    pub login: Option<String>,
}

/// An agent CLI: where it is, and its version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgentFound {
    pub provider: AgentProvider,
    /// Its executable in the PATH (`~/…`); absent: not installed.
    pub path: Option<String>,
    /// What `--version` says, first line.
    pub version: Option<String>,
}

/// A git repository found in one of the folders.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RepoFound {
    /// Its folder's name.
    pub name: String,
    /// `~/…`, as config.toml writes it.
    pub path: String,
    /// The searched folder it is in (`~/Development`).
    pub root: String,
    /// Its `origin` remote (else the first one): `group/app`.
    pub project: Option<String>,
    /// That remote's host (`github.com`, `gitlab.example.com`).
    pub host: Option<String>,
    /// Its forge, from the host: `github.com` is GitHub, any host with `gitlab` in it GitLab.
    pub forge: Option<ForgeProvider>,
    /// Last change of its `.git` folder (a commit, a fetch), in seconds since the epoch.
    pub touched: i64,
}

/// The trackers within reach: their tokens set in the environment (never read), Obsidian's vaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TrackersFound {
    /// CLICKUP_TOKEN is set.
    pub clickup_env: bool,
    /// JIRA_TOKEN is set.
    pub jira_env: bool,
    /// Obsidian's vaults, as its own settings list them (`~/…`), the most recently opened first.
    pub obsidian_vaults: Vec<String>,
}

/// Everything, the probes side by side. `workspaces`: the config's working folders, searched first;
/// `gitlab_host`: the config's instance.
pub async fn detect(workspaces: &[PathBuf], gitlab_host: &str) -> Detection {
    let home = dirs::home_dir().unwrap_or_default();
    let roots = roots(&home, workspaces);
    let folders = roots.iter().map(|r| tilde(&home, r)).collect();
    let scan = {
        let (roots, home) = (roots.clone(), home.clone());
        tokio::task::spawn_blocking(move || repositories(&home, &roots))
    };
    let (agents, tmux, kube, repos) = tokio::join!(agents(&home), tmux_version(), kube_contexts(), scan);
    let repos = repos.unwrap_or_default();
    // Signed in where the repositories are: the config's instance unless they all point elsewhere.
    let host = gitlab_host_of(&repos, gitlab_host);
    // Another host than the config's, guessed from clones: asked with glab's own login there only,
    // never with a token of the environment (meant for another instance).
    let guessed = host != gitlab_host.trim().trim_start_matches("https://").trim_end_matches('/');
    let (gitlab, github) = tokio::join!(forge_login(ForgeProvider::Gitlab, &host, guessed), forge_login(ForgeProvider::Github, "github.com", false));
    Detection {
        forges: vec![gitlab, github],
        agents,
        tmux,
        folders,
        repos,
        kubectl: kube.is_some(),
        kube_contexts: kube.unwrap_or_default(),
        trackers: TrackersFound { clickup_env: env_set("CLICKUP_TOKEN"), jira_env: env_set("JIRA_TOKEN"), obsidian_vaults: obsidian_vaults(&home) },
    }
}

fn env_set(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.trim().is_empty())
}

/// The folders searched: the working folders, then the usual ones, those that exist, once each.
fn roots(home: &Path, workspaces: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<(PathBuf, PathBuf)> = Vec::new();
    for dir in workspaces.iter().map(|w| crate::config::expand_home(w)).chain(USUAL.iter().map(|u| home.join(u))) {
        // macOS' file system ignores case: `~/dev` and `~/Dev` are the same folder; one inside
        // another already searched is searched with it.
        let Ok(canonical) = dir.canonicalize() else { continue };
        if canonical.is_dir() && !out.iter().any(|(_, c)| canonical.starts_with(c)) {
            out.push((dir, canonical));
        }
    }
    out.into_iter().map(|(dir, _)| dir).collect()
}

/// The repositories under `roots`, breadth first, the most recently touched first. A repository is
/// not entered (its submodules, its own folders).
fn repositories(home: &Path, roots: &[PathBuf]) -> Vec<RepoFound> {
    let mut found = Vec::new();
    let mut visits = 0;
    for root in roots {
        let mut level = vec![root.clone()];
        for _ in 0..=DEPTH {
            let mut next = Vec::new();
            for dir in level {
                visits += 1;
                if visits > MAX_VISITS {
                    break;
                }
                let git = dir.join(".git");
                if git.is_dir() {
                    let (project, host) = origin(&git).unzip();
                    let touched = git.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64);
                    let forge = host.as_deref().and_then(forge_of);
                    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    found.push(RepoFound { name, path: tilde(home, &dir), root: tilde(home, root), project, host, forge, touched });
                    continue;
                }
                let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                for e in entries.flatten() {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    // Symbolic links are not followed: no loop, no tour of another disk.
                    if e.file_type().is_ok_and(|t| t.is_dir()) && !name.starts_with('.') && !name.ends_with(".worktrees") && !SKIP.contains(&name.as_ref()) {
                        next.push(e.path());
                    }
                }
            }
            next.sort();
            level = next;
        }
    }
    found.sort_by(|a, b| b.touched.cmp(&a.touched).then_with(|| a.path.cmp(&b.path)));
    found.truncate(MAX_REPOS);
    found
}

/// The project and host of a clone's `origin` remote (else its first), from its `.git/config`.
fn origin(git: &Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(git.join("config")).ok()?;
    let mut remote = String::new();
    let mut urls: Vec<(String, String)> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            remote = line.strip_prefix("[remote \"").and_then(|r| r.strip_suffix("\"]")).unwrap_or_default().to_string();
        } else if let Some(url) = line.strip_prefix("url").map(str::trim_start).and_then(|l| l.strip_prefix('=')).filter(|_| !remote.is_empty()) {
            urls.push((remote.clone(), url.trim().to_string()));
        }
    }
    let (_, url) = urls.iter().find(|(r, _)| r == "origin").or(urls.first())?;
    remote_parts(url)
}

/// `git@host:group/app.git`, `ssh://git@host:22/group/app`, `https://host/group/app.git` →
/// (`group/app`, `host`).
pub fn remote_parts(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    let (host, path) = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/')?,
        // scp-like: [user@]host:path (a local path has no `host:`)
        None => url.split_once(':').filter(|(host, _)| !host.contains('/') && host.len() > 1)?,
    };
    let host = host.rsplit('@').next()?.split(':').next()?.to_lowercase();
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    (!host.is_empty() && path.contains('/')).then(|| (path.to_string(), host))
}

/// The variables glab takes a token from, before its own login of the host.
const GITLAB_TOKEN_ENV: [&str; 4] = ["GITLAB_TOKEN", "GITLAB_ACCESS_TOKEN", "OAUTH_TOKEN", "GITLAB_PRIVATE_TOKEN"];

/// GitHub for `github.com`, GitLab for a host that says so in one of its labels (`gitlab.com`,
/// `gitlab.example.org`, not `mygitlabs.example`).
pub fn forge_of(host: &str) -> Option<ForgeProvider> {
    if host == "github.com" || host.ends_with(".github.com") {
        Some(ForgeProvider::Github)
    } else if host.split('.').any(|label| label == "gitlab") {
        Some(ForgeProvider::Gitlab)
    } else {
        None
    }
}

/// The GitLab instance to ask: the config's, unless no repository points there and most of the
/// GitLab ones point to another (a self-hosted instance, at the first launch).
pub fn gitlab_host_of(repos: &[RepoFound], configured: &str) -> String {
    let configured = configured.trim().trim_start_matches("https://").trim_end_matches('/');
    let hosts: Vec<&str> = repos.iter().filter(|r| r.forge == Some(ForgeProvider::Gitlab)).filter_map(|r| r.host.as_deref()).collect();
    if hosts.is_empty() || hosts.contains(&configured) {
        return configured.to_string();
    }
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for h in hosts {
        match counts.iter_mut().find(|(x, _)| *x == h) {
            Some((_, n)) => *n += 1,
            None => counts.push((h, 1)),
        }
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(h, _)| h.to_string()).unwrap_or_else(|| configured.to_string())
}

/// glab's or gh's login on `host`: its username only. `guessed`: a host the config does not name,
/// asked without the tokens of the environment.
async fn forge_login(provider: ForgeProvider, host: &str, guessed: bool) -> ForgeFound {
    #[derive(Deserialize)]
    struct User {
        #[serde(alias = "login")]
        username: String,
    }
    let cli = match provider {
        ForgeProvider::Gitlab => crate::forge::cli::Cli::Glab,
        ForgeProvider::Github => crate::forge::cli::Cli::Gh,
    };
    let program = match provider {
        ForgeProvider::Gitlab => "glab",
        ForgeProvider::Github => "gh",
    };
    let installed = which(program).is_some();
    let login = if installed {
        let mut cmd = cli.command_on(host);
        if guessed {
            for name in GITLAB_TOKEN_ENV {
                cmd.env_remove(name);
            }
        }
        let answer = tokio::time::timeout(PROBE, cli.output(cmd, &["api", "user"])).await;
        answer.ok().and_then(Result::ok).and_then(|out| serde_json::from_slice::<User>(&out).ok()).map(|u| u.username)
    } else {
        None
    };
    ForgeFound { provider, installed, host: host.to_string(), login }
}

/// Every agent CLI: found in the PATH or not.
async fn agents(home: &Path) -> Vec<AgentFound> {
    let probes = AgentProvider::ALL.map(|p| async move {
        let path = which(p.program());
        let version = match &path {
            Some(bin) => version(&bin.display().to_string(), &["--version"]).await,
            None => None,
        };
        AgentFound { provider: p, path: path.map(|b| tilde(home, &b)), version }
    });
    futures::future::join_all(probes).await
}

/// The first line of `program args`, when it runs.
pub async fn version(program: &str, args: &[&str]) -> Option<String> {
    let run = crate::platform::command(program).args(args).stdin(std::process::Stdio::null()).kill_on_drop(true).output();
    let out = tokio::time::timeout(PROBE, run).await.ok()?.ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    out.status.success().then(|| text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().chars().take(60).collect())
}

/// kubectl's contexts; `None` when kubectl is not installed.
async fn kube_contexts() -> Option<Vec<String>> {
    which("kubectl")?;
    let run = crate::platform::command("kubectl").args(["config", "get-contexts", "-o", "name"]).stdin(std::process::Stdio::null()).kill_on_drop(true).output();
    let out = tokio::time::timeout(PROBE, run).await.ok().and_then(Result::ok);
    Some(out.filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()).unwrap_or_default())
}

/// tmux's version; Prism's own sessions in its place (`agents::local_sessions`): "Prism".
async fn tmux_version() -> Option<String> {
    if crate::agents::local_sessions() {
        return Some("Prism".into());
    }
    version("tmux", &["-V"]).await
}

/// A program in the PATH (the login shell's, as Prism reads it at launch). On Windows, with the
/// extensions of `PATHEXT` (`git` is `git.exe`).
pub fn which(program: &str) -> Option<PathBuf> {
    crate::platform::which(program)
}

/// Obsidian's vaults, from its own settings (`obsidian.json`), the most recently opened first.
fn obsidian_vaults(home: &Path) -> Vec<String> {
    let file = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/obsidian/obsidian.json")
    } else if cfg!(windows) {
        // `%APPDATA%\obsidian`.
        dirs::config_dir().unwrap_or_default().join("obsidian/obsidian.json")
    } else {
        home.join(".config/obsidian/obsidian.json")
    };
    std::fs::read_to_string(file).map(|text| vaults_of(&text, home)).unwrap_or_default()
}

/// The vaults of `obsidian.json` (`{"vaults": {"<id>": {"path": …, "ts": …}}}`) that still exist.
pub fn vaults_of(text: &str, home: &Path) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let mut vaults: Vec<(i64, PathBuf)> = v["vaults"].as_object().into_iter().flatten().filter_map(|(_, vault)| Some((vault["ts"].as_i64().unwrap_or(0), PathBuf::from(vault["path"].as_str()?)))).filter(|(_, p)| p.is_dir()).collect();
    vaults.sort_by_key(|v| std::cmp::Reverse(v.0));
    vaults.into_iter().map(|(_, p)| tilde(home, &p)).collect()
}

/// `~/…` under the home folder, as config.toml writes it.
pub fn tilde(home: &Path, p: &Path) -> String {
    match p.strip_prefix(home) {
        // `/` on Windows too: config.toml is the same file everywhere.
        Ok(rest) if !home.as_os_str().is_empty() && cfg!(windows) => format!("~/{}", rest.display()).replace('\\', "/"),
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => p.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_read_as_project_and_host() {
        assert_eq!(remote_parts("git@github.com:me/blog.git"), Some(("me/blog".into(), "github.com".into())));
        assert_eq!(remote_parts("ssh://git@GitLab.example.org:2222/team/sub/app"), Some(("team/sub/app".into(), "gitlab.example.org".into())));
        assert_eq!(remote_parts("https://gitlab.com/group/app.git/"), Some(("group/app".into(), "gitlab.com".into())));
        assert_eq!(remote_parts("/srv/git/app.git"), None);
        assert_eq!((forge_of("github.com"), forge_of("gitlab.example.org"), forge_of("git.example.org")), (Some(ForgeProvider::Github), Some(ForgeProvider::Gitlab), None));
    }

    fn repo(host: &str) -> RepoFound {
        RepoFound { name: "app".into(), path: "~/dev/app".into(), root: "~/dev".into(), project: Some("g/app".into()), host: Some(host.into()), forge: forge_of(host), touched: 0 }
    }

    /// The config's instance, unless every GitLab repository found points to another one.
    #[test]
    fn gitlab_host_follows_the_repositories() {
        assert_eq!(gitlab_host_of(&[], "gitlab.com"), "gitlab.com");
        assert_eq!(gitlab_host_of(&[repo("github.com")], "gitlab.com"), "gitlab.com");
        assert_eq!(gitlab_host_of(&[repo("gitlab.team.io"), repo("gitlab.team.io"), repo("gitlab.com")], "gitlab.com"), "gitlab.com");
        assert_eq!(gitlab_host_of(&[repo("gitlab.team.io"), repo("gitlab.team.io"), repo("gitlab.other.io")], "https://gitlab.com/"), "gitlab.team.io");
    }

    /// A clone is found with its origin, a worktree folder and dependencies are skipped, and the
    /// most recently touched comes first.
    #[test]
    fn finds_the_repositories_of_a_folder() {
        let home = std::env::temp_dir().join(format!("prism-detect-{}", std::process::id()));
        let root = home.join("Development");
        for (dir, url) in [("team/app", "git@gitlab.team.io:team/app.git"), ("blog", "https://github.com/me/blog")] {
            std::fs::create_dir_all(root.join(dir).join(".git")).unwrap();
            std::fs::write(root.join(dir).join(".git/config"), format!("[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = git@x.io:y/z\n[remote \"origin\"]\n\turl = {url}\n")).unwrap();
        }
        std::fs::create_dir_all(root.join("node_modules/dep/.git")).unwrap();
        std::fs::create_dir_all(root.join("app.worktrees/feat/.git")).unwrap();
        let found = repositories(&home, &roots(&home, &[]));
        let mut paths: Vec<_> = found.iter().map(|r| (r.path.as_str(), r.project.as_deref(), r.forge)).collect();
        paths.sort_by_key(|(p, _, _)| *p);
        assert_eq!(paths, vec![("~/Development/blog", Some("me/blog"), Some(ForgeProvider::Github)), ("~/Development/team/app", Some("team/app"), Some(ForgeProvider::Gitlab))]);
        assert!(found.iter().all(|r| r.root == "~/Development"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn obsidian_vaults_that_exist_newest_first() {
        let home = std::env::temp_dir().join(format!("prism-vaults-{}", std::process::id()));
        std::fs::create_dir_all(home.join("Notes")).unwrap();
        std::fs::create_dir_all(home.join("Work")).unwrap();
        let text = format!(r#"{{"vaults":{{"a":{{"path":"{0}/Notes","ts":1}},"b":{{"path":"{0}/Work","ts":5}},"c":{{"path":"{0}/Gone","ts":9}}}}}}"#, home.display());
        assert_eq!(vaults_of(&text, &home), vec!["~/Work".to_string(), "~/Notes".to_string()]);
        assert!(vaults_of("not json", &home).is_empty());
        let _ = std::fs::remove_dir_all(home);
    }
}
