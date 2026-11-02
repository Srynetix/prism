//! Vulnerabilities in the dependencies of the branches of `[vulnerabilities] branches` (the main
//! one by default), scanned locally with Trivy, in each repository it reads (`[vulnerabilities]
//! repos`, otherwise the primary one).
//!
//! A CI that scans only what an MR touches never rescans a project nobody modifies anymore. Here
//! the whole repository is, every day: as one target by default, or, with `components` (an Nx
//! monorepo's `apps`), each `project.json` project of it and of `libs` apart, then the rest as the
//! root. Fixed HIGH and CRITICAL, from the project's folder so that its `.trivyignore` applies;
//! what it dismisses is reported separately (`--show-suppressed`), as accepted risk. Only the
//! branch's manifests and lockfiles are extracted (`git archive`); Trivy only downloads its database.
//!
//! The images a block names (`[vulnerabilities] images`, `Block::images`: a team's delivery
//! block) are scanned too, newest tag first: the first one published is read, from its
//! registry (`--image-src remote`, Docker's login from the keychain) or from GCP with a gcloud
//! token. System packages included, and up to date with CVEs published after the build. A tag
//! whose image was not published gives way to the previous one (flagged). An unreadable image
//! does not stop the scan, its error is shown on its line.
//! The provider may name base images too, built from
//! another repository: their CVEs are fixed there.
//!
//! One scan per day, or on demand (`rescan`). The last result is kept in the app's
//! cache: it is shown at launch, and serves as the baseline for new CVEs.

use prism_base::tr;
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::Deserialize;
use tokio::{process::Command, sync::Notify};

use super::repos::default_branch;
use crate::git::run as git;
use crate::{
    config::VulnerabilitiesConfig,
    state::{Hub, VulnFinding, VulnProject, Vulnerabilities},
};

const SOURCE: &str = "vulnerabilities";
const EVERY: Duration = Duration::from_secs(24 * 3600);
/// Files Trivy needs: manifests, lockfiles and exception rules.
const FILES: &[&str] = &[
    "package.json", "package-lock.json", "yarn.lock", "pnpm-lock.yaml", "bun.lock",
    "pyproject.toml", "uv.lock", "poetry.lock", "Pipfile.lock", "requirements.txt",
    "Cargo.toml", "Cargo.lock", "go.mod", "go.sum", "Gemfile.lock", "composer.lock",
    "pom.xml", "gradle.lockfile", "packages.lock.json", ".trivyignore",
];
/// The root: the whole repository, or what is outside the components and libraries.
pub const ROOT: &str = "(root)";
/// A first image read downloads its layers: a few hundred MB.
const IMAGE_TIMEOUT: Duration = Duration::from_secs(20 * 60);

pub use crate::images::{ImageProvider, ImageRef, ImageTag, ImageTarget, LocalRepo};

/// The provider named by `[vulnerabilities] images` (`none`: no image is scanned).
pub fn image_provider(name: &str) -> Result<Option<&'static dyn ImageProvider>> {
    match name.trim() {
        "" | "none" => Ok(None),
        // A block that names images (`Block::images`).
        other => crate::blocks::find(other).and_then(|b| b.images()).map(Some).ok_or_else(|| anyhow::anyhow!("[vulnerabilities] images: no block {other} naming images in this Prism (or none)")),
    }
}

/// A repository whose dependencies are scanned: its local clone, its project (Dependabot), its
/// branches (an empty main branch: the remote's default one).
#[derive(Debug, Clone)]
pub struct ScanRepo {
    pub name: String,
    pub clone: PathBuf,
    pub project: Option<String>,
    pub main_branch: String,
    pub release_prefix: String,
}

/// The images to scan: the provider a block is (`[vulnerabilities] images`), with the clone and the
/// project of the repository that block reads, and every repository with a local clone.
pub struct Images {
    pub provider: &'static dyn ImageProvider,
    pub clone: PathBuf,
    pub project: String,
    pub local: Vec<LocalRepo>,
}

/// What Trivy scanned: a project's dependencies, or a deployed image.
struct Target {
    /// Name carried by the CVEs: `cart-api`, or `cart-api · prod` for its image.
    label: String,
    /// Repository of the target (base images, a repository besides the first); absent: the first one.
    repo: Option<String>,
    /// A base image (the Base group).
    base: bool,
    /// `image:tag` in the registry.
    image: Option<String>,
    /// Image read from the GCP registry, or newer tags without an image.
    note: Option<String>,
    error: Option<String>,
    vulns: Vec<(RawVuln, bool)>,
}

static RESCAN: LazyLock<Notify> = LazyLock::new(Notify::new);

/// Requests a scan without waiting for the next day.
pub fn rescan() {
    RESCAN.notify_one();
}

pub async fn run(repos: Vec<ScanRepo>, cfg: VulnerabilitiesConfig, images: Option<Images>, hub: Hub) {
    let cache = crate::profile::cache_dir();
    let saved = cache.join("vulnerabilities.json");
    let mut last: Option<Vulnerabilities> = std::fs::read(&saved).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if let Some(v) = &last {
        set(&hub, Vulnerabilities { scanning: false, ..v.clone() });
    }
    loop {
        // The kept result is valid until the next day.
        let age = last.as_ref().and_then(|v| v.scanned_at).map(|at| (Utc::now() - at).to_std().unwrap_or_default());
        if let Some(age) = age.filter(|a| *a < EVERY) {
            let _ = tokio::time::timeout(EVERY - age, RESCAN.notified()).await;
        }
        hub.update(|s| match s.vulnerabilities.as_mut() {
            Some(v) => !std::mem::replace(&mut v.scanning, true),
            None => false,
        });
        match scan(&repos, &cfg, images.as_ref(), &cache, last.as_ref()).await {
            Ok(v) => {
                if let Err(e) = std::fs::create_dir_all(&cache).and_then(|_| std::fs::write(&saved, serde_json::to_vec(&v).unwrap_or_default())) {
                    tracing::warn!("vulnerabilities: save: {e}");
                }
                set(&hub, v.clone());
                hub.set_error(SOURCE, None);
                last = Some(v);
            }
            Err(e) => {
                tracing::warn!("vulnerabilities: {e:#}");
                hub.set_error(SOURCE, Some(format!("{e:#}")));
                hub.update(|s| s.vulnerabilities.as_mut().is_some_and(|v| std::mem::take(&mut v.scanning)));
                // No new attempt for an hour, unless requested.
                let _ = tokio::time::timeout(Duration::from_secs(3600), RESCAN.notified()).await;
            }
        }
    }
}

fn set(hub: &Hub, v: Vulnerabilities) {
    hub.update(|s| {
        let changed = s.vulnerabilities.as_ref() != Some(&v);
        s.vulnerabilities = Some(v);
        changed
    });
}

async fn scan(repos: &[ScanRepo], cfg: &VulnerabilitiesConfig, images: Option<&Images>, cache: &Path, previous: Option<&Vulnerabilities>) -> Result<Vulnerabilities> {
    let version = run_ok(crate::platform::command("trivy").arg("--version")).await.with_context(|| tr!("trivy not found in PATH ({hint})", hint = crate::platform::install_hint("trivy")))?;
    let dir = cache.join("trivy-src");
    let mut targets = Vec::new();
    // The first repository's branches and commits head the block.
    let mut heads: Vec<(String, String)> = Vec::new();
    let several = cfg.branches.len() > 1;
    for (i, r) in repos.iter().enumerate() {
        let mut scanned = HashSet::new();
        for which in &cfg.branches {
            let branch = resolve(r, which).await.with_context(|| r.name.clone())?;
            // `release` falls back to the main branch when there is none: scanned once.
            if !scanned.insert(branch.clone()) {
                continue;
            }
            let (sha, found) = dependencies(r, &branch, cfg, &dir).await.with_context(|| format!("{} {branch}", r.name))?;
            // Besides the first repository, its projects carry its name; with several branches, theirs.
            let repo = (i > 0).then(|| r.name.clone());
            let label = |l: String| {
                let l = if repo.is_some() { format!("{}/{l}", r.name) } else { l };
                if several { format!("{l} · {which}") } else { l }
            };
            targets.extend(found.into_iter().map(|t| Target { label: label(t.label), repo: repo.clone(), ..t }));
            if i == 0 {
                heads.push((branch, sha));
            }
        }
    }

    // The images the provider names (`[vulnerabilities] images`).
    if let Some(Images { provider, clone, project, local }) = images {
        std::fs::create_dir_all(&dir)?;
        let mut gcp = Gcp::default();
        for image in provider.images(clone, project, local).await? {
            targets.push(scan_image(image, &mut gcp, &dir, &cfg.skip_dirs).await);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    // On GitHub, Dependabot's alerts too, when the repository has them on.
    if crate::conventions::get().forge == crate::forge::Provider::Github {
        for (i, r) in repos.iter().enumerate() {
            if let Some(project) = &r.project {
                targets.extend(dependabot(project).await.map(|t| if i > 0 { Target { label: format!("{}/{}", r.name, t.label), repo: Some(r.name.clone()), ..t } } else { t }));
            }
        }
    }
    let mut v = summarize(String::new(), String::new(), version.lines().next().unwrap_or_default().trim().to_string(), targets, previous);
    let mut heads = heads.into_iter();
    (v.branch, v.sha) = heads.next().unwrap_or_default();
    v.other_branches = heads.collect();
    Ok(v)
}

/// A branch of `[vulnerabilities] branches` in a repository: `release`, its latest release branch
/// (otherwise its main one); `main`, its main branch; any other, that branch.
async fn resolve(r: &ScanRepo, which: &str) -> Result<String> {
    let main = || async { if r.main_branch.is_empty() { default_branch(&r.clone).await } else { r.main_branch.clone() } };
    Ok(match which.trim() {
        "release" => crate::conventions::latest_release_in(&r.clone, &main().await, &r.release_prefix).await?,
        "main" => main().await,
        other => other.to_string(),
    })
}

/// A repository's dependencies on a branch: its commit, and one target per project, each scanned
/// from its folder.
async fn dependencies(r: &ScanRepo, branch: &str, cfg: &VulnerabilitiesConfig, dir: &Path) -> Result<(String, Vec<Target>)> {
    let head = format!("origin/{branch}");
    let sha = git(&r.clone, &["rev-parse", "--short", &head]).await?.trim().to_string();
    let tree = git(&r.clone, &["ls-tree", "-r", "--name-only", &head]).await?;
    let paths: Vec<&str> = tree.lines().collect();
    let components = cfg.components.trim().trim_end_matches('/').to_string();
    let projects = projects(&paths, &components, &cfg.libs);
    let files: Vec<&str> = paths.iter().copied().filter(|p| FILES.contains(&p.rsplit('/').next().unwrap_or(p))).collect();

    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir)?;
    let archive = dir.join("src.tar");
    let mut args = vec!["archive", "--format=tar", "-o", archive.to_str().context("cache path")?, &head, "--"];
    args.extend(&files);
    git(&r.clone, &args).await?;
    run_ok(crate::platform::command("tar").arg("-xf").arg(&archive).arg("-C").arg(dir)).await?;

    let mut targets = Vec::new();
    for p in &projects {
        let root_skip = [components.as_str(), cfg.libs.trim().trim_end_matches('/'), "node_modules,.yarn,.venv"].iter().filter(|d| !d.is_empty()).copied().collect::<Vec<_>>().join(",");
        let (cwd, skip) = if p == ROOT { (dir.to_path_buf(), root_skip.as_str()) } else { (dir.join(p), ".venv,venv,.cache") };
        let out = run_ok(
            crate::platform::command("trivy")
                .current_dir(&cwd)
                .args(["fs", "--quiet", "--format", "json", "--scanners", "vuln", "--severity", "HIGH,CRITICAL", "--ignore-unfixed", "--show-suppressed"])
                .args(["--skip-dirs", skip, "--skip-files", ".env", "."]),
        )
        .await
        .with_context(|| tr!("trivy on {p}", p = p))?;
        targets.push(Target { label: name(p, &components), repo: None, base: false, image: None, note: None, error: None, vulns: findings(&out).with_context(|| format!("trivy report of {p}"))? });
    }
    let _ = std::fs::remove_dir_all(dir);
    Ok((sha, targets))
}

/// Dependabot's open alerts on a GitHub repository, high and critical as Trivy reads them: a target
/// of their own, `Dependabot`. None when the repository has them off; an unreadable list says why
/// on its line.
async fn dependabot(project: &str) -> Option<Target> {
    #[derive(Deserialize)]
    struct Alert {
        security_advisory: Advisory,
        security_vulnerability: Affected,
        html_url: String,
    }
    #[derive(Deserialize)]
    struct Advisory {
        ghsa_id: String,
        cve_id: Option<String>,
        summary: String,
        severity: String,
    }
    #[derive(Deserialize)]
    struct Affected {
        package: Package,
        vulnerable_version_range: String,
        first_patched_version: Option<Patched>,
    }
    #[derive(Deserialize)]
    struct Package {
        name: String,
    }
    #[derive(Deserialize)]
    struct Patched {
        identifier: String,
    }
    let label = "Dependabot".to_string();
    // This list pages by cursor (`after`), not by `page`: gh follows its links.
    let args = ["api".to_string(), "--paginate".into(), "--slurp".into(), format!("repos/{project}/dependabot/alerts?state=open&per_page=100")];
    let alerts: Vec<Alert> = match super::github::gh_bytes(&args).await.and_then(|b| serde_json::from_slice::<Vec<Vec<Alert>>>(&b).context("unreadable gh response")) {
        Ok(pages) => pages.into_iter().flatten().collect(),
        Err(e) => {
            let e = first_line(&format!("{e:#}"));
            if e.contains("disabled") || e.contains("404") {
                tracing::info!("vulnerabilities: no Dependabot alerts ({e})");
                return None;
            }
            return Some(Target { label, repo: None, base: false, image: None, note: None, error: Some(format!("Dependabot: {e}")), vulns: Vec::new() });
        }
    };
    let vulns = alerts
        .into_iter()
        .filter(|a| matches!(a.security_advisory.severity.as_str(), "high" | "critical"))
        .map(|a| {
            let v = RawVuln {
                id: a.security_advisory.cve_id.unwrap_or(a.security_advisory.ghsa_id),
                pkg: a.security_vulnerability.package.name,
                installed: a.security_vulnerability.vulnerable_version_range,
                fixed: a.security_vulnerability.first_patched_version.map(|p| p.identifier),
                severity: a.security_advisory.severity.to_uppercase(),
                title: Some(a.security_advisory.summary),
                url: Some(a.html_url),
            };
            (v, false)
        })
        .collect();
    Some(Target { label, repo: None, base: false, image: None, note: None, error: None, vulns })
}

/// GCP registry access token, requested from gcloud when the scan first needs it.
#[derive(Default)]
struct Gcp {
    token: Option<std::result::Result<String, String>>,
}

impl Gcp {
    async fn token(&mut self) -> std::result::Result<String, String> {
        if self.token.is_none() {
            let got = run_ok(crate::platform::command("gcloud").args(["auth", "print-access-token"])).await;
            self.token = Some(got.map(|t| t.trim().to_string()).map_err(|e| format!("gcloud: {}", first_line(&format!("{e:#}")))));
        }
        self.token.clone().unwrap_or_else(|| Err("gcloud".into()))
    }
}

/// An image: its most recent tag that has one, from the first place that has it.
async fn scan_image(target: ImageTarget, gcp: &mut Gcp, dir: &Path, skip_dirs: &[String]) -> Target {
    let (label, repo, base) = (target.label, target.repo, target.base);
    if let Some(error) = target.error {
        return Target { label, repo: repo.clone(), base, image: None, note: None, error: Some(error), vulns: Vec::new() };
    }
    // Its `.trivyignore` rules, in a file of their own for Trivy.
    let ignore = match target.ignore {
        Some(rules) => {
            let file = dir.join(format!("trivyignore-{}", label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect::<String>()));
            if let Err(e) = std::fs::write(&file, rules) {
                tracing::warn!("vulnerabilities: {label}: .trivyignore: {e}");
            }
            file
        }
        None => PathBuf::new(),
    };
    let mut unpublished = Vec::new();
    let mut last_error = None;
    for tag in &target.tags {
        let mut result = Err(anyhow::anyhow!(tr!("nowhere to read {tag}", tag = tag.shown)));
        let mut place = None;
        for at in &tag.at {
            result = if at.gcp {
                match gcp.token().await {
                    Ok(token) => trivy_image(&at.image, &ignore, Some(&token), skip_dirs).await,
                    Err(e) => Err(anyhow::anyhow!("{e}")),
                }
            } else {
                trivy_image(&at.image, &ignore, None, skip_dirs).await
            };
            if result.is_ok() {
                place = at.gcp.then_some("on GCP");
                break;
            }
        }
        let image = Some(tag.shown.clone());
        match result {
            Ok(vulns) => {
                let skipped = (!unpublished.is_empty()).then(|| tr!("{tags} not published", tags = unpublished.join(", ")));
                let note = [place.map(String::from), skipped].into_iter().flatten().collect::<Vec<_>>().join(" · ");
                return Target { label, repo: repo.clone(), base, image, note: (!note.is_empty()).then_some(note), error: None, vulns };
            }
            Err(e) if missing(&format!("{e:#}")) => {
                unpublished.push(tag.tag.clone());
                last_error = Some(e);
            }
            Err(e) => {
                tracing::warn!("vulnerabilities: {label}: {e:#}");
                return Target { label, repo: repo.clone(), base, image, note: None, error: Some(first_line(&format!("{e:#}"))), vulns: Vec::new() };
            }
        }
    }
    let error = format!("{}{}", tr!("no published image for {tags}", tags = unpublished.join(", ")), last_error.map(|e| format!(" ({})", first_line(&format!("{e:#}")))).unwrap_or_default());
    Target { label, repo: repo.clone(), base, image: None, note: None, error: Some(error), vulns: Vec::new() }
}

async fn trivy_image(image: &str, ignore: &Path, token: Option<&str>, skip_dirs: &[String]) -> Result<Vec<(RawVuln, bool)>> {
    let mut cmd = crate::platform::command("trivy");
    cmd.args(["image", "--quiet", "--format", "json", "--scanners", "vuln", "--severity", "HIGH,CRITICAL", "--ignore-unfixed", "--show-suppressed"])
        .args(["--image-src", "remote", "--platform", "linux/amd64"]);
    if !skip_dirs.is_empty() {
        cmd.arg("--skip-dirs").arg(skip_dirs.join(","));
    }
    if ignore.exists() {
        cmd.arg("--ignorefile").arg(ignore);
    }
    // Through the environment, so that the token does not show up in the process list.
    if let Some(t) = token {
        cmd.env("TRIVY_REGISTRY_TOKEN", t);
    }
    cmd.arg(image);
    match tokio::time::timeout(IMAGE_TIMEOUT, run_ok(&mut cmd)).await {
        Ok(out) => findings(&out?),
        Err(_) => bail!("{}", tr!("timed out after {n} min", n = IMAGE_TIMEOUT.as_secs() / 60)),
    }
}

/// The registry does not know this image or this tag.
fn missing(error: &str) -> bool {
    let e = error.to_lowercase();
    ["manifest_unknown", "manifest unknown", "name_unknown", "not found", "404"].iter().any(|m| e.contains(m))
}

/// Vulnerabilities of a Trivy report, and those a `.trivyignore` dismisses (accepted).
fn findings(report: &str) -> Result<Vec<(RawVuln, bool)>> {
    let report: Report = serde_json::from_str(report)?;
    Ok(report
        .results
        .into_iter()
        .flat_map(|r| {
            let accepted = r.suppressed.into_iter().filter(|m| m.status == "ignored").map(|m| (m.finding, true));
            r.vulnerabilities.into_iter().map(|v| (v, false)).chain(accepted)
        })
        .collect())
}

/// Last useful line of a Trivy error (the cause), shortened.
fn first_line(e: &str) -> String {
    let line = e.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or(e);
    line.chars().take(200).collect()
}

/// The Nx projects (a `project.json`) under the components folder or the libraries' (`libs`),
/// none of either when empty, and the root.
fn projects(paths: &[&str], components: &str, libs: &str) -> Vec<String> {
    let dir = if components.is_empty() { String::from("/") } else { format!("{components}/") };
    let libs = format!("{}/", libs.trim().trim_end_matches('/'));
    let mut out: Vec<String> = paths
        .iter()
        .filter_map(|p| p.strip_suffix("/project.json"))
        .filter(|d| d.starts_with(&dir) || (libs != "/" && d.starts_with(&libs)))
        .map(String::from)
        .collect();
    out.sort();
    out.push(ROOT.into());
    out
}

/// `apps/cart-api` → `cart-api`; libs keep their prefix to tell them apart.
fn name(dir: &str, components: &str) -> String {
    dir.strip_prefix(components).and_then(|d| d.strip_prefix('/')).unwrap_or(dir).to_string()
}

/// One line per project, one entry per (CVE, package, version, accepted or not), with the affected
/// projects. A CVE to fix is new if the previous scan did not see it in this project.
fn summarize(branch: String, sha: String, trivy: String, targets: Vec<Target>, previous: Option<&Vulnerabilities>) -> Vulnerabilities {
    let known: Option<HashSet<(String, String)>> = previous
        .map(|p| p.findings.iter().filter(|f| !f.accepted).flat_map(|f| f.projects.iter().map(move |pr| (f.id.clone(), pr.clone()))).collect());
    let is_new = |id: &str, project: &str| known.as_ref().is_some_and(|k| !k.contains(&(id.to_string(), project.to_string())));
    let mut findings: BTreeMap<(String, String, String, bool), VulnFinding> = BTreeMap::new();
    let mut projects = Vec::new();
    for Target { label: project, repo, base, image, note, error, vulns } in &targets {
        let mut seen = HashSet::new();
        let (mut critical, mut high, mut new, mut accepted_count) = (0, 0, 0, 0);
        for (v, accepted) in vulns {
            let accepted = *accepted;
            // The same package can appear in several of the project's lockfiles.
            if !seen.insert((&v.id, &v.pkg, &v.installed, accepted)) {
                continue;
            }
            let fresh = !accepted && is_new(&v.id, project);
            match (accepted, v.severity.as_str()) {
                (true, _) => accepted_count += 1,
                (false, "CRITICAL") => critical += 1,
                (false, _) => high += 1,
            }
            if fresh {
                new += 1;
            }
            let f = findings.entry((v.id.clone(), v.pkg.clone(), v.installed.clone(), accepted)).or_insert_with(|| VulnFinding {
                id: v.id.clone(),
                severity: v.severity.to_lowercase(),
                package: v.pkg.clone(),
                installed: v.installed.clone(),
                fixed: v.fixed.clone().unwrap_or_default(),
                title: v.title.clone().unwrap_or_default(),
                url: v.url.clone(),
                projects: Vec::new(),
                new: false,
                accepted,
            });
            f.projects.push(project.clone());
            f.new |= fresh;
        }
        projects.push(VulnProject { project: project.clone(), repo: repo.clone(), base: *base, image: image.clone(), note: note.clone(), error: error.clone(), critical, high, new, accepted: accepted_count });
    }
    let mut findings: Vec<VulnFinding> = findings.into_values().collect();
    findings.sort_by(|a, b| {
        a.accepted.cmp(&b.accepted).then((a.severity != "critical").cmp(&(b.severity != "critical"))).then(b.new.cmp(&a.new)).then(a.package.cmp(&b.package))
    });
    Vulnerabilities { branch, sha, trivy, scanned_at: Some(Utc::now()), scanning: false, projects, other_branches: Vec::new(), findings }
}

/// Output of a command, or its error.
async fn run_ok(cmd: &mut Command) -> Result<String> {
    // A scan stopped (a reload, its timeout) stops trivy too, rather than leaving it running.
    let out = cmd.kill_on_drop(true).output().await?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[derive(Deserialize)]
struct Report {
    #[serde(rename = "Results", default)]
    results: Vec<RawResult>,
}

#[derive(Deserialize)]
struct RawResult {
    #[serde(rename = "Vulnerabilities", default)]
    vulnerabilities: Vec<RawVuln>,
    /// Dismissed by a rule (`.trivyignore`), with `--show-suppressed`.
    #[serde(rename = "ExperimentalModifiedFindings", default)]
    suppressed: Vec<Suppressed>,
}

#[derive(Deserialize)]
struct Suppressed {
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "Finding")]
    finding: RawVuln,
}

#[derive(Deserialize)]
struct RawVuln {
    #[serde(rename = "VulnerabilityID")]
    id: String,
    #[serde(rename = "PkgName")]
    pkg: String,
    #[serde(rename = "InstalledVersion")]
    installed: String,
    #[serde(rename = "FixedVersion")]
    fixed: Option<String>,
    #[serde(rename = "Severity")]
    severity: String,
    #[serde(rename = "Title")]
    title: Option<String>,
    #[serde(rename = "PrimaryURL")]
    url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;


    fn vuln(id: &str, pkg: &str, severity: &str) -> RawVuln {
        RawVuln { id: id.into(), pkg: pkg.into(), installed: "1.0".into(), fixed: Some("1.1".into()), severity: severity.into(), title: None, url: None }
    }

    fn deps(label: &str, vulns: Vec<(RawVuln, bool)>) -> Target {
        Target { label: label.into(), repo: None, base: false, image: None, note: None, error: None, vulns }
    }

    #[test]
    fn a_missing_image_is_told_apart_from_other_errors() {
        assert!(missing("GET https://registry/v2/x/manifests/1: MANIFEST_UNKNOWN: manifest unknown"));
        assert!(missing("unexpected status code 404 Not Found"));
        assert!(!missing("unexpected status code 401 Unauthorized"));
    }

    #[test]
    fn last_line_of_an_error() {
        assert_eq!(first_line("scan error:\n  unable to fetch\n  MANIFEST_UNKNOWN: manifest unknown\n"), "MANIFEST_UNKNOWN: manifest unknown");
    }

    #[test]
    fn nx_projects_and_root() {
        let paths = ["apps/cart-api/project.json", "apps/cart-api/uv.lock", "libs/design-system/project.json", "tools/x/project.json", "yarn.lock"];
        assert_eq!(projects(&paths, "apps", "libs"), ["apps/cart-api", "libs/design-system", ROOT]);
        assert_eq!(projects(&paths, "apps", ""), ["apps/cart-api", ROOT], "no library folder by default");
        assert_eq!(projects(&paths, "", ""), [ROOT], "without a components folder, the repository as a whole");
        assert_eq!(name("apps/cart-api", "apps"), "cart-api");
        assert_eq!(name("libs/design-system", "apps"), "libs/design-system");
    }

    #[test]
    fn findings_are_merged_across_projects_and_new_ones_flagged() {
        let by = vec![
            deps("cart-api", vec![(vuln("CVE-1", "pyjwt", "CRITICAL"), false), (vuln("CVE-2", "requests", "HIGH"), false), (vuln("CVE-2", "requests", "HIGH"), false)]),
            deps("worker", vec![(vuln("CVE-2", "requests", "HIGH"), false), (vuln("CVE-9", "urllib3", "HIGH"), true)]),
        ];
        let first = summarize("release/x".into(), "abc".into(), "0.74".into(), by, None);
        assert_eq!(first.findings.len(), 3);
        assert_eq!(first.findings[0].id, "CVE-1");
        assert_eq!(first.findings[1].projects, ["cart-api", "worker"]);
        assert!(first.findings[2].accepted, "accepted findings come last");
        assert_eq!((first.projects[1].high, first.projects[1].accepted), (1, 1));
        assert!(first.findings.iter().all(|f| !f.new), "nothing is new without a previous scan");
        assert_eq!((first.projects[0].critical, first.projects[0].high), (1, 1));

        let by = vec![deps("cart-api", vec![(vuln("CVE-2", "requests", "HIGH"), false), (vuln("CVE-3", "axios", "HIGH"), false), (vuln("CVE-4", "lodash", "HIGH"), true)])];
        let second = summarize("release/x".into(), "def".into(), "0.74".into(), by, Some(&first));
        let new: Vec<_> = second.findings.iter().filter(|f| f.new).map(|f| f.id.as_str()).collect();
        assert_eq!(new, ["CVE-3"]);
        assert_eq!(second.projects[0].new, 1);
    }
}
