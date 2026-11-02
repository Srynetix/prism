//! The local clones of the config's repositories (`[[repos]] path`): a pack names a repository
//! and its project, each member says where their clone is. Settings finds them in a folder the
//! user picks (never elsewhere: nothing is scanned unasked), checks that a chosen folder is a clone
//! of the right project, or clones it with glab or gh.
//!
//! A clone is recognized by its remotes, read from `.git/config` without running git: its folder's
//! name says nothing (`~/src/helm-charts` can be `example-org/helm`).

use prism_base::{i18n::tr, tr};
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;

/// Folders a search does not enter: dependencies, builds, worktrees, hidden ones.
const SKIP: [&str; 6] = ["node_modules", "target", "vendor", "dist", "build", "Library"];
/// Depth of a search below the chosen folder (`~/dev/team/group/repo`: 3).
const DEPTH: usize = 4;
/// Folders a search visits at most: a home folder picked by mistake stays quick.
const MAX_VISITS: usize = 20_000;

/// A project's path from a remote URL: `git@host:group/sub/app.git`, `ssh://git@host:22/group/app`,
/// `https://host/group/app.git` → `group/sub/app`, `group/app`.
pub fn project_of(url: &str) -> Option<String> {
    let url = url.trim();
    let path = match url.split_once("://") {
        // scheme://[user@]host[:port]/path
        Some((_, rest)) => rest.split_once('/')?.1,
        // scp-like: [user@]host:path (a Windows drive or a local path has no `host:`)
        None => url.split_once(':').filter(|(host, _)| !host.contains('/') && host.len() > 1)?.1,
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    (!path.is_empty()).then(|| path.to_string())
}

/// Same project, whatever the case (GitLab and GitHub paths are case-insensitive).
pub fn same_project(a: &str, b: &str) -> bool {
    a.trim().trim_matches('/').eq_ignore_ascii_case(b.trim().trim_matches('/'))
}

/// The projects of a clone's remotes (`[remote "…"] url`), read from its `.git/config`. A
/// worktree's `.git` is a file: not a clone of its own, none.
pub fn remotes(dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(dir.join(".git").join("config")) else { return Vec::new() };
    let mut out = Vec::new();
    let mut in_remote = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_remote = line.starts_with("[remote ");
        } else if let Some(url) = line.strip_prefix("url").map(str::trim_start).and_then(|l| l.strip_prefix('=')).filter(|_| in_remote) {
            out.extend(project_of(url));
        }
    }
    out
}

/// A clone found for a project.
#[derive(Debug, Serialize, PartialEq)]
pub struct Found {
    pub project: String,
    /// `~/…` under the home folder, as config.toml writes it.
    pub path: String,
}

/// The clones of `projects` under `root` (DEPTH levels at most, MAX_VISITS folders): one per
/// project, the shallowest. A clone is not entered (its submodules, its own folders).
pub fn find(root: &Path, projects: &[String]) -> Result<Vec<Found>> {
    ensure!(root.is_dir(), tr!("{path} is not a folder", path = root.display()));
    let mut found: Vec<Found> = Vec::new();
    let mut level = vec![root.to_path_buf()];
    let mut visits = 0;
    for _ in 0..=DEPTH {
        let mut next = Vec::new();
        for dir in level {
            visits += 1;
            if visits > MAX_VISITS {
                return Ok(found);
            }
            if dir.join(".git").is_dir() {
                for remote in remotes(&dir) {
                    if let Some(p) = projects.iter().find(|p| same_project(p, &remote) && !found.iter().any(|f| same_project(&f.project, p))) {
                        found.push(Found { project: p.clone(), path: tilde(&dir) });
                    }
                }
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                // Symbolic links are not followed: no loop, no tour of another disk.
                let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
                if is_dir && !name.starts_with('.') && !name.ends_with(".worktrees") && !SKIP.contains(&name.as_ref()) {
                    next.push(e.path());
                }
            }
        }
        if found.len() == projects.len() {
            break;
        }
        next.sort();
        level = next;
    }
    Ok(found)
}

/// Is `dir` a clone of `project`? Its verdict for Settings.
pub fn verify(dir: &Path, project: &str) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(tr("folder not found").into());
    }
    if !dir.join(".git").exists() {
        return Err(tr("not a git repository").into());
    }
    let remotes = remotes(dir);
    if remotes.iter().any(|r| same_project(r, project)) {
        return Ok(());
    }
    // A worktree (`.git` file) has no remotes of its own.
    match remotes.first() {
        None if dir.join(".git").is_file() => Err(tr("a worktree, not the clone itself").into()),
        None => Err(tr("no remote").into()),
        Some(r) => Err(tr!("a clone of {r}, not {project}", r = r, project = project)),
    }
}

/// Clones `project` into `parent/<its last segment>` with the CLI of `forge` (glab, gh; absent, the
/// profile's): their login, their protocol choice (ssh or https). Returns the clone's path, `~/…`.
pub async fn clone(project: &str, parent: &Path, forge: Option<crate::forge::Provider>) -> Result<String> {
    // A path, never a URL or an option (`--…`) the CLI would take for its own.
    let project = valid_project(project)?;
    let name = project.rsplit('/').next().filter(|n| !n.is_empty()).context("no project")?;
    ensure!(parent.is_dir(), tr!("{path} is not a folder", path = parent.display()));
    let target = parent.join(name);
    ensure!(!target.exists(), tr!("{path} exists already", path = target.display()));
    let dest = target.to_string_lossy().into_owned();
    let mut cmd = match forge.unwrap_or(crate::conventions::get().forge) {
        crate::forge::Provider::Gitlab => crate::collectors::gitlab::glab_cmd(),
        crate::forge::Provider::Github => crate::collectors::github::gh_cmd(),
    };
    let out = cmd.args(["repo", "clone", project, &dest]).stdin(std::process::Stdio::null()).kill_on_drop(true).output().await.context(tr("glab or gh not found in PATH"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!(tr!("cloning {project}: {err}", project = project, err = err.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("failed")));
    }
    Ok(tilde(&target))
}

/// A folder the user picked to add as a repository: what Settings prefills.
#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Folder {
    /// `~/…` under the home folder.
    pub path: String,
    /// Its own name, the repository's by default.
    pub name: String,
    /// `repo` (a clone), `worktree` (a linked worktree: its clone is the one to add) or `none`.
    pub git: &'static str,
    /// The project of its first remote, if any.
    pub project: Option<String>,
}

/// What a folder is, read without running git (its `.git`, its remotes).
pub fn inspect(dir: &Path) -> Result<Folder> {
    ensure!(dir.is_dir(), tr!("{path} is not a folder", path = dir.display()));
    let dotgit = dir.join(".git");
    let git = if dotgit.is_dir() { "repo" } else if dotgit.is_file() { "worktree" } else { "none" };
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Folder { path: tilde(dir), name, git, project: remotes(dir).into_iter().next() })
}

/// `git init` in a folder that is not a repository yet, on the main branch (`[forge] main_branch`).
pub async fn init(dir: &Path) -> Result<()> {
    ensure!(dir.is_dir(), tr!("{path} is not a folder", path = dir.display()));
    ensure!(!dir.join(".git").exists(), tr!("{path} is a git repository already", path = dir.display()));
    let branch = crate::conventions::get().main_branch.clone();
    crate::git::run(dir, &["init", "--quiet", "--initial-branch", &branch]).await?;
    Ok(())
}

/// A project to create on the forge with a new local repository.
#[derive(Debug, serde::Deserialize)]
pub struct NewRemote {
    /// Group (GitLab) or owner (GitHub); empty: the signed-in user's.
    #[serde(default)]
    pub namespace: String,
    /// `private`, `internal` or `public`.
    pub visibility: String,
    /// The forge (`gitlab`, `github`): a working folder's; absent, the profile's (`[forge] provider`).
    #[serde(default)]
    pub forge: Option<crate::forge::Provider>,
}

impl NewRemote {
    /// The forge it goes on: its own, else the profile's.
    pub fn provider(&self) -> crate::forge::Provider {
        self.forge.unwrap_or(crate::conventions::get().forge)
    }
}

/// What a project on the forge writes in its `[[repos]]` entry: its project; its MRs followed when
/// it is on the profile's forge, otherwise that forge named (the project is then a link).
fn forge_keys(project: &str, forge: crate::forge::Provider, profile: crate::forge::Provider) -> serde_json::Map<String, serde_json::Value> {
    let mut keys = serde_json::Map::new();
    keys.insert("project".into(), project.into());
    if forge == profile {
        keys.insert("merge_requests".into(), true.into());
    } else {
        keys.insert("forge".into(), serde_json::to_value(forge).unwrap_or_default());
    }
    keys
}

/// A project created: its folder, and its forge project when one was created and pushed.
#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Created {
    /// `~/…` under the home folder.
    pub path: String,
    pub project: Option<String>,
    /// The local repository is there, but the forge step failed: why.
    pub warning: Option<String>,
}

/// A new project's folder name: one segment, no hidden folder, nothing a shell or git trips on.
fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    ensure!(!name.is_empty(), tr("no name"));
    ensure!(!name.starts_with('.') && !name.starts_with('-'), tr("a name cannot start with . or -"));
    ensure!(name.chars().all(|c| c.is_alphanumeric() || "-_.".contains(c)), tr("letters, digits, - _ and . only"));
    Ok(name)
}

/// Creates `parent/name`: a git repository on the main branch with a README and a first commit;
/// with `remote`, the project on the forge (glab, gh: their login), set as `origin` and pushed.
/// A failed forge step leaves the local repository and says why (`warning`).
pub async fn create(parent: &Path, name: &str, remote: Option<NewRemote>) -> Result<Created> {
    let name = valid_name(name)?;
    ensure!(parent.is_dir(), tr!("{path} is not a folder", path = parent.display()));
    let dir = parent.join(name);
    ensure!(!dir.exists(), tr!("{path} exists already", path = dir.display()));
    std::fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let local = async {
        init(&dir).await?;
        std::fs::write(dir.join("README.md"), format!("# {name}\n"))?;
        crate::git::run(&dir, &["add", "README.md"]).await?;
        crate::git::run(&dir, &["commit", "--quiet", "-m", "Initial commit"]).await?;
        anyhow::Ok(())
    };
    if let Err(e) = local.await {
        // Nothing of the user's is in it yet: no half-made folder left behind.
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    let path = tilde(&dir);
    let Some(remote) = remote else { return Ok(Created { path, project: None, warning: None }) };
    match publish(&dir, name, &remote).await {
        Ok(project) => Ok(Created { path, project: Some(project), warning: None }),
        Err(e) => Ok(Created { path, project: None, warning: Some(format!("{e:#}")) }),
    }
}

/// A phone's new project: `create` in the working folder `workspace` (by its name; empty, the only
/// one), never a folder the phone names, then its `[[repos]]` entry at the end of `config` (path,
/// project, worktrees on), read at the next launch. A name already among the repositories (a
/// pack's included) is refused before anything is made. The entry not written, the project stays,
/// and `warning` says why.
pub async fn create_in_workspace(config: &Path, workspace: &str, name: &str, remote: Option<NewRemote>) -> Result<Created> {
    let cfg = crate::config::load(config)?;
    let ws = pick_workspace(&cfg, workspace)?;
    let dir = ws.dir.clone();
    // Its forge is the working folder's, whatever the phone says.
    let forge = ws.forge.unwrap_or(cfg.forge.provider);
    let remote = remote.map(|r| NewRemote { forge: Some(forge), ..r });
    let name = valid_name(name)?;
    ensure!(!cfg.repos.iter().any(|r| r.name.eq_ignore_ascii_case(name)), tr!("“{name}” is a repository already", name = name));
    let mut made = create(&dir, name, remote).await?;
    let mut row = serde_json::Map::new();
    row.insert("name".into(), name.into());
    row.insert("path".into(), made.path.clone().into());
    row.insert("worktrees".into(), true.into());
    if let Some(project) = &made.project {
        row.extend(forge_keys(project, forge, cfg.forge.provider));
    }
    if let Err(e) = crate::settings::add_repo(config, &row) {
        let why = tr!("not added to the repositories: {err}", err = format!("{e:#}"));
        made.warning = Some(match made.warning.take() {
            Some(w) => format!("{w}; {why}"),
            None => why,
        });
    }
    Ok(made)
}

/// The working folder a phone names (empty: the only one).
fn pick_workspace<'a>(cfg: &'a crate::config::Config, workspace: &str) -> Result<&'a crate::config::Workspace> {
    ensure!(!cfg.workspaces.is_empty(), tr("no working folder: add one in Settings › Code › Repositories › Working folders on the Mac"));
    Ok(match workspace.trim() {
        "" if cfg.workspaces.len() == 1 => &cfg.workspaces[0],
        "" => bail!(tr("pick a working folder")),
        w => cfg.workspaces.iter().find(|x| x.name.eq_ignore_ascii_case(w)).with_context(|| tr!("no working folder named {w}", w = w))?,
    })
}

/// A forge project as `group/sub/name`: segments a path or a shell does not trip on.
fn valid_project(project: &str) -> Result<&str> {
    let project = project.trim().trim_matches('/');
    let segments: Vec<&str> = project.split('/').collect();
    ensure!(segments.len() >= 2, tr("a project is group/name (owner/name on GitHub)"));
    for seg in &segments {
        valid_name(seg).with_context(|| tr!("{project}: not a project path", project = project))?;
    }
    Ok(project)
}

/// A phone's clone (`[remote] projects`): `project` of the working folder `workspace`'s forge,
/// cloned into that folder, never one the phone names, then its `[[repos]]` entry written as a new
/// project's (read at the next launch). A name already among the repositories is refused first.
pub async fn clone_in_workspace(config: &Path, workspace: &str, project: &str) -> Result<Created> {
    let cfg = crate::config::load(config)?;
    let ws = pick_workspace(&cfg, workspace)?;
    let project = valid_project(project)?;
    let name = project.rsplit('/').next().unwrap_or(project);
    ensure!(!cfg.repos.iter().any(|r| r.name.eq_ignore_ascii_case(name)), tr!("“{name}” is a repository already", name = name));
    let forge = ws.forge.unwrap_or(cfg.forge.provider);
    let path = clone(project, &ws.dir, Some(forge)).await?;
    let mut row = serde_json::Map::new();
    row.insert("name".into(), name.into());
    row.insert("path".into(), path.clone().into());
    row.insert("worktrees".into(), true.into());
    row.extend(forge_keys(project, forge, cfg.forge.provider));
    let warning = crate::settings::add_repo(config, &row).err().map(|e| tr!("not added to the repositories: {err}", err = format!("{e:#}")));
    Ok(Created { path, project: Some(project.to_string()), warning })
}

/// A project of the forge a phone may clone: its path, its description, and whether it is among
/// the repositories already.
#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ForgeProject {
    pub path: String,
    pub description: Option<String>,
    pub added: bool,
}

/// The projects of the working folder `workspace`'s forge you are a member of, most recently
/// active first, matching `query` (empty: all), at most 20.
pub async fn search_projects(config: &Path, workspace: &str, query: &str) -> Result<Vec<ForgeProject>> {
    let cfg = crate::config::load(config)?;
    let ws = pick_workspace(&cfg, workspace)?;
    // Letters, digits and a few signs: nothing that changes the request.
    let query: String = query.trim().chars().filter(|c| c.is_alphanumeric() || " -_.".contains(*c)).collect();
    #[derive(serde::Deserialize)]
    struct Row {
        #[serde(alias = "path_with_namespace", alias = "full_name")]
        path: String,
        description: Option<String>,
    }
    let rows: Vec<Row> = match ws.forge.unwrap_or(cfg.forge.provider) {
        crate::forge::Provider::Gitlab => {
            let search = if query.is_empty() { String::new() } else { format!("&search={}", query.replace(' ', "%20")) };
            crate::collectors::gitlab::api(&format!("projects?membership=true&simple=true&archived=false&per_page=20&order_by=last_activity_at{search}")).await?
        }
        crate::forge::Provider::Github => {
            let all: Vec<Row> = crate::collectors::github::api("user/repos?per_page=100&sort=pushed&affiliation=owner,collaborator,organization_member").await?;
            let q = query.to_lowercase();
            all.into_iter().filter(|r| q.is_empty() || r.path.to_lowercase().contains(&q) || r.description.as_deref().is_some_and(|d| d.to_lowercase().contains(&q))).take(20).collect()
        }
    };
    let added = |p: &str| cfg.repos.iter().any(|r| r.project.as_deref().is_some_and(|x| same_project(x, p)));
    Ok(rows.into_iter().map(|r| ForgeProject { added: added(&r.path), path: r.path, description: r.description.filter(|d| !d.trim().is_empty()) }).collect())
}

/// A local project published later (created without its forge project, or added as a folder
/// without a remote): its project created on the forge, named after its folder, set as `origin`,
/// its current branch pushed. Refused when it has a remote already. Its path on the forge.
pub async fn publish_local(dir: &Path, remote: &NewRemote) -> Result<String> {
    ensure!(dir.join(".git").is_dir(), tr!("{path} is not a git repository (a worktree: publish its main clone)", path = dir.display()));
    let remotes = crate::git::run(dir, &["remote"]).await?;
    ensure!(remotes.trim().is_empty(), tr!("{path} has a remote already ({remotes}): give its Project in Settings instead", path = dir.display(), remotes = remotes.split_whitespace().collect::<Vec<_>>().join(", ")));
    crate::git::run(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]).await.map_err(|_| anyhow::anyhow!(tr!("{path} has no commit yet: commit first", path = dir.display())))?;
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = valid_name(&name).with_context(|| tr!("its folder's name cannot name a project ({name})", name = name))?;
    publish(dir, name, remote).await
}

/// A phone's publish (`[remote] projects`): the repository `name` of `config` (its clone, without a
/// project), published as `publish_local` does, then its `project` and `merge_requests` written in
/// its `[[repos]]` entry, read at the next launch. The entry not written, `warning` says why.
pub async fn publish_in_config(config: &Path, name: &str, remote: NewRemote) -> Result<Created> {
    let cfg = crate::config::load(config)?;
    let repo = cfg.repos.iter().find(|r| r.name == name).with_context(|| tr!("no repository named {name}", name = name))?;
    ensure!(repo.project.is_none(), tr!("{name} is on the forge already ({project})", name = name, project = repo.project.as_deref().unwrap_or_default()));
    let dir = repo.path.clone().with_context(|| tr!("{name} has no local clone", name = name))?;
    // Its forge: the one asked, else its working folder's, else the profile's.
    let forge = remote.forge.or_else(|| cfg.workspaces.iter().find(|w| dir.starts_with(&w.dir)).and_then(|w| w.forge)).unwrap_or(cfg.forge.provider);
    let project = publish_local(&dir, &NewRemote { forge: Some(forge), ..remote }).await?;
    let keys = forge_keys(&project, forge, cfg.forge.provider);
    let warning = crate::settings::set_repo(config, &repo.name, &keys).err().map(|e| format!("{project} created, but not written in the repositories: {e:#}"));
    Ok(Created { path: tilde(&dir), project: Some(project), warning })
}

/// The forge step of `create` and `publish_local`: the project, `origin`, the push of the current
/// branch (the main branch for a new project). Its path on the forge.
async fn publish(dir: &Path, name: &str, remote: &NewRemote) -> Result<String> {
    let visibility = match remote.visibility.as_str() {
        v @ ("private" | "internal" | "public") => v,
        other => bail!("unknown visibility {other}"),
    };
    let namespace = remote.namespace.trim().trim_matches('/');
    if !namespace.is_empty() {
        for seg in namespace.split('/') {
            valid_name(seg).with_context(|| tr!("{namespace}: not a group or owner", namespace = namespace))?;
        }
    }
    let wanted = if namespace.is_empty() { name.to_string() } else { format!("{namespace}/{name}") };
    let branch = match crate::git::run(dir, &["symbolic-ref", "--short", "HEAD"]).await {
        Ok(b) if !b.trim().is_empty() => b.trim().to_string(),
        _ => crate::conventions::get().main_branch.clone(),
    };
    match remote.provider() {
        crate::forge::Provider::Github => {
            let dest = dir.to_string_lossy().into_owned();
            let args = ["repo", "create", &wanted, &format!("--{visibility}"), "--source", &dest, "--remote", "origin", "--push"];
            crate::forge::cli::Cli::Gh.run(&args).await?;
            remotes(dir).into_iter().next().context(tr("created on GitHub, but no origin was set"))
        }
        crate::forge::Provider::Gitlab => {
            // The local repository is ours already: glab only creates the project.
            let args = ["repo", "create", &wanted, &format!("--{visibility}"), "--defaultBranch", &branch, "--skipGitInit"];
            crate::forge::cli::Cli::Glab.run(&args).await?;
            let full = if namespace.is_empty() {
                #[derive(serde::Deserialize)]
                struct User {
                    username: String,
                }
                let me: User = crate::collectors::gitlab::api("user").await?;
                format!("{}/{name}", me.username)
            } else {
                wanted
            };
            #[derive(serde::Deserialize)]
            struct Project {
                path_with_namespace: String,
                ssh_url_to_repo: String,
                http_url_to_repo: String,
            }
            let p: Project = crate::collectors::gitlab::api(&format!("projects/{}", crate::collectors::gitlab::encode(&full))).await?;
            // glab's own choice of protocol (`glab config set git_protocol`), ssh by default.
            let https = crate::forge::cli::Cli::Glab.run(&["config", "get", "git_protocol"]).await.is_ok_and(|o| String::from_utf8_lossy(&o).trim() == "https");
            let url = if https { &p.http_url_to_repo } else { &p.ssh_url_to_repo };
            crate::git::run(dir, &["remote", "add", "origin", url]).await?;
            crate::git::run(dir, &["push", "--quiet", "--set-upstream", "origin", &branch]).await.with_context(|| tr!("{project} created, but the push failed", project = p.path_with_namespace))?;
            Ok(p.path_with_namespace)
        }
    }
}

/// `~/…` under the home folder, as config.toml writes paths.
pub fn tilde(p: &Path) -> String {
    let home: PathBuf = dirs::home_dir().unwrap_or_default();
    match p.strip_prefix(&home) {
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => p.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_from_remote_urls() {
        assert_eq!(project_of("git@gitlab.com:group/sub/app.git").as_deref(), Some("group/sub/app"));
        assert_eq!(project_of("ssh://git@gitlab.example.org:2222/group/app.git").as_deref(), Some("group/app"));
        assert_eq!(project_of("https://github.com/owner/repo").as_deref(), Some("owner/repo"));
        assert_eq!(project_of("https://oauth2:x@gitlab.com/group/app.git/").as_deref(), Some("group/app"));
        assert_eq!(project_of("/srv/git/app.git"), None);
        assert_eq!(project_of("C:/repos/app"), None);
    }

    fn clone_at(dir: &Path, url: &str) {
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git/config"), format!("[core]\n\tbare = false\n[remote \"origin\"]\n\turl = {url}\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n")).unwrap();
    }

    #[test]
    fn finds_clones_by_their_remote_not_their_name() {
        let root = std::env::temp_dir().join(format!("prism-clones-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        clone_at(&root.join("ops/helm-charts"), "git@gitlab.com:Example-Org/Ops/helm.git");
        clone_at(&root.join("deep/a/b/c/d/e/far"), "git@gitlab.com:g/far.git");
        clone_at(&root.join("node_modules/dep"), "git@gitlab.com:g/dep.git");
        clone_at(&root.join("app"), "https://gitlab.com/g/app.git");
        // A submodule inside a clone is not looked at.
        clone_at(&root.join("app/sub"), "https://gitlab.com/g/sub.git");
        let projects = ["example-org/ops/helm", "g/app", "g/far", "g/dep", "g/sub"].map(String::from);
        let mut found = find(&root, &projects).unwrap();
        found.sort_by(|a, b| a.project.cmp(&b.project));
        let names: Vec<_> = found.iter().map(|f| f.project.as_str()).collect();
        assert_eq!(names, ["example-org/ops/helm", "g/app"]);
        assert!(found[0].path.ends_with("ops/helm-charts"));
        assert!(verify(&root.join("app"), "G/App").is_ok());
        assert_eq!(verify(&root.join("app"), "g/other"), Err("a clone of g/app, not g/other".into()));
        assert_eq!(verify(&root.join("ops"), "g/app"), Err("not a git repository".into()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn inspects_a_folder_without_running_git() {
        let root = std::env::temp_dir().join(format!("prism-inspect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        clone_at(&root.join("app"), "git@gitlab.com:g/app.git");
        std::fs::create_dir_all(root.join("plain")).unwrap();
        let app = inspect(&root.join("app")).unwrap();
        assert_eq!((app.name.as_str(), app.git, app.project.as_deref()), ("app", "repo", Some("g/app")));
        let plain = inspect(&root.join("plain")).unwrap();
        assert_eq!((plain.git, plain.project), ("none", None));
        assert!(inspect(&root.join("gone")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn new_project_names() {
        assert_eq!(valid_name(" my-app ").unwrap(), "my-app");
        for bad in ["", ".hidden", "-x", "a/b", "a b", "../up"] {
            assert!(valid_name(bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn creates_a_local_project_with_a_first_commit() {
        let root = std::env::temp_dir().join(format!("prism-create-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // The test's own identity: the machine's git config may have none.
        for (k, v) in [("GIT_AUTHOR_NAME", "A"), ("GIT_AUTHOR_EMAIL", "a@x"), ("GIT_COMMITTER_NAME", "A"), ("GIT_COMMITTER_EMAIL", "a@x")] {
            // Identity variables only, read by the git processes these tests start.
            prism_sys::env::set_var(k, v);
        }
        let made = create(&root, "fresh", None).await.unwrap();
        assert_eq!((made.project, made.warning), (None, None));
        let dir = root.join("fresh");
        assert!(dir.join(".git").is_dir() && dir.join("README.md").is_file());
        let log = crate::git::run(&dir, &["log", "--oneline"]).await.unwrap();
        assert_eq!(log.lines().count(), 1);
        assert!(create(&root, "fresh", None).await.unwrap_err().to_string().contains("exists already"));
        assert!(init(&dir).await.is_err(), "a repository already");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_project_on_another_forge_is_a_link() {
        use crate::forge::Provider::{Github, Gitlab};
        let keys = |f, p| serde_json::Value::Object(forge_keys("me/app", f, p));
        assert_eq!(keys(Gitlab, Gitlab), serde_json::json!({ "project": "me/app", "merge_requests": true }));
        assert_eq!(keys(Github, Gitlab), serde_json::json!({ "project": "me/app", "forge": "github" }));
    }

    #[tokio::test]
    async fn publishes_only_a_local_repository_with_a_commit() {
        let root = std::env::temp_dir().join(format!("prism-publish-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("empty")).unwrap();
        let remote = || NewRemote { namespace: String::new(), visibility: "private".into(), forge: None };
        assert!(publish_local(&root.join("empty"), &remote()).await.unwrap_err().to_string().contains("not a git repository"));
        init(&root.join("empty")).await.unwrap();
        assert!(publish_local(&root.join("empty"), &remote()).await.unwrap_err().to_string().contains("no commit yet"));
        std::fs::create_dir_all(root.join("cloned")).unwrap();
        init(&root.join("cloned")).await.unwrap();
        crate::git::run(root.join("cloned"), &["remote", "add", "origin", "git@gitlab.com:g/cloned.git"]).await.unwrap();
        assert!(publish_local(&root.join("cloned"), &remote()).await.unwrap_err().to_string().contains("has a remote already"));
        let config = root.join("config.toml");
        std::fs::write(&config, format!("[[repos]]\nname = \"cloned\"\npath = \"{}\"\nproject = \"g/cloned\"\n", root.join("cloned").display())).unwrap();
        assert!(publish_in_config(&config, "cloned", remote()).await.unwrap_err().to_string().contains("on the forge already"));
        assert!(publish_in_config(&config, "other", remote()).await.is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn a_phone_creates_in_the_working_folder_only() {
        let root = std::env::temp_dir().join(format!("prism-workspace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("work")).unwrap();
        for (k, v) in [("GIT_AUTHOR_NAME", "A"), ("GIT_AUTHOR_EMAIL", "a@x"), ("GIT_COMMITTER_NAME", "A"), ("GIT_COMMITTER_EMAIL", "a@x")] {
            // Identity variables only, read by the git processes these tests start.
            prism_sys::env::set_var(k, v);
        }
        let config = root.join("config.toml");
        std::fs::write(&config, "[[repos]]\nname = \"app\"\n").unwrap();
        assert!(create_in_workspace(&config, "", "fresh", None).await.unwrap_err().to_string().contains("no working folder"));
        std::fs::write(&config, format!("[[workspaces]]\nname = \"Work\"\ndir = \"{}\"\n\n[[workspaces]]\ndir = \"{}\"\n\n[[repos]]\nname = \"app\"\n", root.join("work").display(), root.join("perso").display())).unwrap();
        assert!(create_in_workspace(&config, "", "fresh", None).await.unwrap_err().to_string().contains("pick a working folder"), "two: one is picked");
        assert!(create_in_workspace(&config, "Elsewhere", "fresh", None).await.unwrap_err().to_string().contains("no working folder named"));
        std::fs::create_dir_all(root.join("perso")).unwrap();
        assert!(create_in_workspace(&config, "perso", "mine", None).await.unwrap().warning.is_none(), "named after its folder");
        assert!(root.join("perso/mine/.git").is_dir());
        assert!(create_in_workspace(&config, "work", "App", None).await.unwrap_err().to_string().contains("a repository already"));
        assert!(create_in_workspace(&config, "work", "../up", None).await.is_err(), "one folder of the working folder");
        assert!(!root.join("work/App").exists());
        let made = create_in_workspace(&config, "Work", "fresh", None).await.unwrap();
        assert_eq!(made.warning, None);
        assert!(root.join("work/fresh/.git").is_dir());
        let cfg = crate::config::load(&config).unwrap();
        let fresh = cfg.repos.iter().find(|r| r.name == "fresh").unwrap();
        assert_eq!(fresh.path.as_deref(), Some(root.join("work/fresh").as_path()));
        assert!(fresh.worktrees && fresh.project.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
