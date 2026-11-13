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
    state::{Hub, RemoteImage, ScannedImage, VulnFinding, VulnProject, Vulnerabilities},
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
    /// The image exactly as scanned.
    scanned: Option<ScannedImage>,
    error: Option<String>,
    vulns: Vec<(RawVuln, bool)>,
}

static WAKE: LazyLock<Notify> = LazyLock::new(Notify::new);
/// What was asked of the scan meanwhile, done in the order asked.
static ASKED: LazyLock<std::sync::Mutex<Vec<Asked>>> = LazyLock::new(Default::default);

#[derive(Debug, Clone, PartialEq)]
enum Asked {
    /// Everything again.
    Scan,
    /// Each scanned image against its registry: its tag's digest now, a newer tag published.
    Check,
    /// One image again (its line's label).
    Image(String),
}

fn ask(what: Asked) {
    let mut asked = ASKED.lock().unwrap();
    if !asked.contains(&what) {
        asked.push(what);
    }
    WAKE.notify_one();
}

/// Requests a scan without waiting for the next day.
pub fn rescan() {
    ask(Asked::Scan);
}

/// Asks the registry about each scanned image: changed since, or a newer tag published.
pub fn check_registry() {
    ask(Asked::Check);
}

/// Scans one image again (`project`: its line, `cart-api · prod`), not the whole lot.
pub fn rescan_image(project: &str) {
    ask(Asked::Image(project.to_string()));
}

pub async fn run(repos: Vec<ScanRepo>, cfg: VulnerabilitiesConfig, images: Option<Images>, hub: Hub) {
    let cache = crate::profile::cache_dir();
    let saved = cache.join("vulnerabilities.json");
    let save = |v: &Vulnerabilities| {
        if let Err(e) = std::fs::create_dir_all(&cache).and_then(|_| std::fs::write(&saved, serde_json::to_vec(v).unwrap_or_default())) {
            tracing::warn!("vulnerabilities: save: {e}");
        }
    };
    let mut last: Option<Vulnerabilities> = std::fs::read(&saved).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if let Some(v) = &last {
        set(&hub, Vulnerabilities { scanning: false, checking: false, ..v.clone() });
    }
    loop {
        // The kept result is valid until the next day.
        let age = last.as_ref().and_then(|v| v.scanned_at).map(|at| (Utc::now() - at).to_std().unwrap_or_default());
        if let Some(age) = age.filter(|a| *a < EVERY) {
            let _ = tokio::time::timeout(EVERY - age, WAKE.notified()).await;
        }
        let asked = std::mem::take(&mut *ASKED.lock().unwrap());
        // Only a check or an image: done on the last result, the day's scan still to come.
        if let Some(v) = last.as_mut().filter(|_| !asked.is_empty() && !asked.contains(&Asked::Scan)) {
            hub.update(|s| s.vulnerabilities.as_mut().is_some_and(|v| !std::mem::replace(&mut v.checking, true)));
            for what in asked {
                match what {
                    Asked::Check => check(v, images.as_ref()).await,
                    Asked::Image(project) => {
                        if let Err(e) = rescan_one(v, &project, images.as_ref(), &cfg, &cache).await {
                            tracing::warn!("vulnerabilities: {project}: {e:#}");
                            if let Some(p) = v.projects.iter_mut().find(|p| p.project == project) {
                                p.error = Some(first_line(&format!("{e:#}")));
                            }
                        }
                    }
                    Asked::Scan => {}
                }
            }
            save(v);
            set(&hub, Vulnerabilities { scanning: false, checking: false, ..v.clone() });
            continue;
        }
        hub.update(|s| match s.vulnerabilities.as_mut() {
            Some(v) => !std::mem::replace(&mut v.scanning, true),
            None => false,
        });
        match scan(&repos, &cfg, images.as_ref(), &cache, last.as_ref()).await {
            Ok(v) => {
                save(&v);
                set(&hub, v.clone());
                hub.set_error(SOURCE, None);
                last = Some(v);
            }
            Err(e) => {
                tracing::warn!("vulnerabilities: {e:#}");
                hub.set_error(SOURCE, Some(format!("{e:#}")));
                hub.update(|s| s.vulnerabilities.as_mut().is_some_and(|v| std::mem::take(&mut v.scanning)));
                // No new attempt for an hour, unless requested.
                let _ = tokio::time::timeout(Duration::from_secs(3600), WAKE.notified()).await;
            }
        }
    }
}

/// The line of an image's platform besides amd64 (`cart-api · prod · arm64`) belongs to it.
fn image_of(project: &str) -> &str {
    project.strip_suffix(" · arm64").unwrap_or(project)
}

/// Each scanned image against its registry, without pulling it: the digest its tag points to
/// now, and the newest tag the provider names that is published, when newer than the scanned one.
async fn check(v: &mut Vulnerabilities, images: Option<&Images>) {
    let named = match images {
        Some(Images { provider, clone, project, local }) => provider.images(clone, project, local).await.unwrap_or_else(|e| {
            tracing::warn!("vulnerabilities: images: {e:#}");
            Vec::new()
        }),
        None => Vec::new(),
    };
    for p in v.projects.iter_mut() {
        let Some(scanned) = p.scanned.clone() else { continue };
        let now = published(&scanned.reference, scanned.gcp).await;
        let mut newer = None;
        if let Some(target) = named.iter().find(|t| t.label == image_of(&p.project)) {
            for tag in target.tags.iter().take_while(|t| Some(&t.shown) != p.image.as_ref()) {
                if let Some(at) = tag.at.first()
                    && published(&at.image, at.gcp).await.is_ok()
                {
                    newer = Some(tag.shown.clone());
                    break;
                }
            }
        }
        let (digest, error) = match now {
            Ok(n) => (Some(n.digest), None),
            Err(e) => (None, Some(first_line(&format!("{e:#}")))),
        };
        p.remote = Some(RemoteImage { at: Utc::now(), digest, newer, error });
    }
}

/// One image scanned again, its lines and their CVEs put in place of the former ones; « new »
/// against those.
async fn rescan_one(v: &mut Vulnerabilities, project: &str, images: Option<&Images>, cfg: &VulnerabilitiesConfig, cache: &Path) -> Result<()> {
    let Images { provider, clone, project: forge_project, local } = images.context("no image provider")?;
    let label = image_of(project);
    let target = provider.images(clone, forge_project, local).await?.into_iter().find(|t| t.label == label).with_context(|| tr!("{name} is no longer named", name = label))?;
    let dir = cache.join("trivy-image");
    std::fs::create_dir_all(&dir)?;
    let targets = scan_image(target, &mut Gcp::default(), &dir, &cfg.skip_dirs).await;
    let _ = std::fs::remove_dir_all(&dir);
    let fresh = summarize(v.branch.clone(), v.sha.clone(), v.trivy.clone(), targets, Some(v));
    merge(v, label, fresh);
    Ok(())
}

/// `fresh` (one image's lines) in place of that image's lines in `v`.
fn merge(v: &mut Vulnerabilities, label: &str, fresh: Vulnerabilities) {
    let mine = |p: &str| image_of(p) == label;
    let at = v.projects.iter().position(|p| mine(&p.project)).unwrap_or(v.projects.len());
    v.projects.retain(|p| !mine(&p.project));
    for (i, p) in fresh.projects.into_iter().enumerate() {
        v.projects.insert((at + i).min(v.projects.len()), p);
    }
    for f in &mut v.findings {
        f.projects.retain(|p| !mine(p));
    }
    v.findings.retain(|f| !f.projects.is_empty());
    for f in fresh.findings {
        match v.findings.iter_mut().find(|x| (&x.id, &x.package, &x.installed, x.accepted) == (&f.id, &f.package, &f.installed, f.accepted)) {
            Some(x) => {
                x.projects.extend(f.projects);
                x.new |= f.new;
            }
            None => v.findings.push(f),
        }
    }
    sort_findings(&mut v.findings);
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
            targets.extend(scan_image(image, &mut gcp, &dir, &cfg.skip_dirs).await);
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
        targets.push(Target { label: name(p, &components), repo: None, base: false, image: None, note: None, error: None, scanned: None, vulns: findings(&out).with_context(|| format!("trivy report of {p}"))?.0 });
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
            return Some(Target { label, repo: None, base: false, image: None, note: None, error: Some(format!("Dependabot: {e}")), scanned: None, vulns: Vec::new() });
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
    Some(Target { label, repo: None, base: false, image: None, note: None, error: None, scanned: None, vulns })
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

/// The platforms scanned when the image is published for them: amd64 first, which decides
/// whether a tag is published; arm64 besides, on a line of its own.
const PLATFORMS: [&str; 2] = ["linux/amd64", "linux/arm64"];

/// An image: its most recent tag that has one, from the first place that has it; for each
/// platform it is published for (`PLATFORMS`), one target.
async fn scan_image(target: ImageTarget, gcp: &mut Gcp, dir: &Path, skip_dirs: &[String]) -> Vec<Target> {
    let (label, repo, base) = (target.label, target.repo, target.base);
    if let Some(error) = target.error {
        return vec![Target { label, repo: repo.clone(), base, image: None, note: None, error: Some(error), scanned: None, vulns: Vec::new() }];
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
            let token = if at.gcp {
                match gcp.token().await {
                    Ok(token) => Some(token),
                    Err(e) => {
                        result = Err(anyhow::anyhow!("{e}"));
                        continue;
                    }
                }
            } else {
                None
            };
            result = trivy_image(&at.image, &ignore, token.as_deref(), skip_dirs, PLATFORMS[0]).await.map(|r| (r, at, token));
            if result.is_ok() {
                place = Some(at);
                break;
            }
        }
        let image = Some(tag.shown.clone());
        match result {
            Ok(((vulns, meta), at, token)) => {
                let skipped = (!unpublished.is_empty()).then(|| tr!("{tags} not published", tags = unpublished.join(", ")));
                let note = [place.filter(|p| p.gcp).map(|_| String::from("on GCP")), skipped].into_iter().flatten().collect::<Vec<_>>().join(" · ");
                let note = (!note.is_empty()).then_some(note);
                // What the registry publishes: the tag's digest (an index's), its platforms.
                let published = published(&at.image, at.gcp).await.ok();
                let digest = published.as_ref().map(|p| p.digest.clone()).or(meta.digest);
                let scanned = |platform: &str, created| ScannedImage { reference: at.image.clone(), digest: digest.clone(), created, scanned_at: Utc::now(), platform: platform.into(), gcp: at.gcp };
                let mut out = vec![Target { label: label.clone(), repo: repo.clone(), base, image: image.clone(), note: note.clone(), error: None, scanned: Some(scanned(PLATFORMS[0], meta.created)), vulns }];
                if published.as_ref().is_some_and(|p| p.platforms.iter().any(|x| x == PLATFORMS[1])) {
                    let label = format!("{label} · arm64");
                    out.push(match trivy_image(&at.image, &ignore, token.as_deref(), skip_dirs, PLATFORMS[1]).await {
                        Ok((vulns, meta)) => Target { label, repo: repo.clone(), base, image, note, error: None, scanned: Some(scanned(PLATFORMS[1], meta.created)), vulns },
                        Err(e) => Target { label, repo: repo.clone(), base, image, note, error: Some(first_line(&format!("{e:#}"))), scanned: None, vulns: Vec::new() },
                    });
                }
                return out;
            }
            Err(e) if missing(&format!("{e:#}")) => {
                unpublished.push(tag.tag.clone());
                last_error = Some(e);
            }
            Err(e) => {
                tracing::warn!("vulnerabilities: {label}: {e:#}");
                return vec![Target { label, repo: repo.clone(), base, image, note: None, error: Some(first_line(&format!("{e:#}"))), scanned: None, vulns: Vec::new() }];
            }
        }
    }
    let error = format!("{}{}", tr!("no published image for {tags}", tags = unpublished.join(", ")), last_error.map(|e| format!(" ({})", first_line(&format!("{e:#}")))).unwrap_or_default());
    vec![Target { label, repo: repo.clone(), base, image: None, note: None, error: Some(error), scanned: None, vulns: Vec::new() }]
}

/// What Trivy says of the image it read: the digest it was pulled by, when it was built.
#[derive(Debug, Default)]
struct ImageMeta {
    digest: Option<String>,
    created: Option<chrono::DateTime<Utc>>,
}

async fn trivy_image(image: &str, ignore: &Path, token: Option<&str>, skip_dirs: &[String], platform: &str) -> Result<(Vec<(RawVuln, bool)>, ImageMeta)> {
    let mut cmd = crate::platform::command("trivy");
    cmd.args(["image", "--quiet", "--format", "json", "--scanners", "vuln", "--severity", "HIGH,CRITICAL", "--ignore-unfixed", "--show-suppressed"])
        .args(["--image-src", "remote", "--platform", platform]);
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

/// What the registry publishes under a tag now: the digest it points to (a multi-platform
/// image's index) and the platforms it is published for (none known: a single image, or GCP).
#[derive(Debug, PartialEq)]
struct Published {
    digest: String,
    platforms: Vec<String>,
}

/// Asked without pulling anything: Docker's `buildx imagetools` (its logins), or gcloud for an
/// image on GCP's registry.
async fn published(image: &str, gcp: bool) -> Result<Published> {
    let ask = async {
        if gcp {
            let out = run_ok(crate::platform::command("gcloud").args(["artifacts", "docker", "images", "describe", image, "--format=json"])).await?;
            let v: serde_json::Value = serde_json::from_str(&out)?;
            let digest = v.pointer("/image_summary/digest").and_then(|d| d.as_str()).context("gcloud: no digest")?;
            Ok(Published { digest: digest.to_string(), platforms: Vec::new() })
        } else {
            let out = run_ok(crate::platform::command("docker").args(["buildx", "imagetools", "inspect", image, "--format", "{{json .Manifest}}"])).await?;
            manifest(&out)
        }
    };
    match tokio::time::timeout(Duration::from_secs(60), ask).await {
        Ok(r) => r,
        Err(_) => bail!("{}", tr!("timed out after {n} min", n = 1)),
    }
}

/// A manifest as `docker buildx imagetools inspect --format '{{json .Manifest}}'` writes it.
fn manifest(json: &str) -> Result<Published> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let digest = v.get("digest").and_then(|d| d.as_str()).context("no digest")?.to_string();
    let platforms = v
        .get("manifests")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let p = m.get("platform")?;
            let (os, arch) = (p.get("os")?.as_str()?, p.get("architecture")?.as_str()?);
            // Attestations come as `unknown/unknown`.
            (os != "unknown").then(|| match p.get("variant").and_then(|v| v.as_str()) {
                Some(variant) => format!("{os}/{arch}/{variant}"),
                None => format!("{os}/{arch}"),
            })
        })
        .collect();
    Ok(Published { digest, platforms })
}

/// The registry does not know this image or this tag.
fn missing(error: &str) -> bool {
    let e = error.to_lowercase();
    ["manifest_unknown", "manifest unknown", "name_unknown", "not found", "404"].iter().any(|m| e.contains(m))
}

/// Vulnerabilities of a Trivy report, and those a `.trivyignore` dismisses (accepted); for an
/// image, what it says of it.
fn findings(report: &str) -> Result<(Vec<(RawVuln, bool)>, ImageMeta)> {
    let report: Report = serde_json::from_str(report)?;
    let meta = ImageMeta {
        digest: report.metadata.repo_digests.first().and_then(|d| d.split_once('@')).map(|(_, d)| d.to_string()),
        created: report.metadata.image_config.created,
    };
    let vulns = report
        .results
        .into_iter()
        .flat_map(|r| {
            let accepted = r.suppressed.into_iter().filter(|m| m.status == "ignored").map(|m| (m.finding, true));
            r.vulnerabilities.into_iter().map(|v| (v, false)).chain(accepted)
        })
        .collect();
    Ok((vulns, meta))
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
    for Target { label: project, repo, base, image, note, error, scanned, vulns } in &targets {
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
        projects.push(VulnProject { project: project.clone(), repo: repo.clone(), base: *base, image: image.clone(), note: note.clone(), error: error.clone(), critical, high, new, accepted: accepted_count, scanned: scanned.clone(), remote: None });
    }
    let mut findings: Vec<VulnFinding> = findings.into_values().collect();
    sort_findings(&mut findings);
    Vulnerabilities { branch, sha, trivy, scanned_at: Some(Utc::now()), scanning: false, checking: false, projects, other_branches: Vec::new(), findings }
}

/// To fix first (critical, then new), then the accepted ones.
fn sort_findings(findings: &mut [VulnFinding]) {
    findings.sort_by(|a, b| {
        a.accepted.cmp(&b.accepted).then((a.severity != "critical").cmp(&(b.severity != "critical"))).then(b.new.cmp(&a.new)).then(a.package.cmp(&b.package))
    });
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
    #[serde(rename = "Metadata", default)]
    metadata: RawMetadata,
}

/// An image's: the digests it is known by (`repo@sha256:…`), its build date.
#[derive(Deserialize, Default)]
struct RawMetadata {
    #[serde(rename = "RepoDigests", default)]
    repo_digests: Vec<String>,
    #[serde(rename = "ImageConfig", default)]
    image_config: RawImageConfig,
}

#[derive(Deserialize, Default)]
struct RawImageConfig {
    #[serde(default)]
    created: Option<chrono::DateTime<Utc>>,
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
        Target { label: label.into(), repo: None, base: false, image: None, note: None, error: None, scanned: None, vulns }
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

    #[test]
    fn a_registry_manifest_its_digest_and_platforms() {
        let index = r#"{"schemaVersion":2,"mediaType":"application/vnd.oci.image.index.v1+json","digest":"sha256:aa","manifests":[
            {"digest":"sha256:1","platform":{"architecture":"amd64","os":"linux"}},
            {"digest":"sha256:2","platform":{"architecture":"arm64","os":"linux","variant":"v8"}},
            {"digest":"sha256:3","platform":{"architecture":"unknown","os":"unknown"}}]}"#;
        assert_eq!(manifest(index).unwrap(), Published { digest: "sha256:aa".into(), platforms: vec!["linux/amd64".into(), "linux/arm64/v8".into()] });
        let single = r#"{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"sha256:bb"}"#;
        assert_eq!(manifest(single).unwrap(), Published { digest: "sha256:bb".into(), platforms: Vec::new() });
        assert!(manifest("{}").is_err());
    }

    #[test]
    fn trivy_says_how_the_image_was_read() {
        let report = r#"{"ArtifactName":"r/x:1","Metadata":{"RepoDigests":["r/x@sha256:cc"],"ImageConfig":{"created":"2026-11-12T08:00:00Z"}},"Results":[]}"#;
        let (vulns, meta) = findings(report).unwrap();
        assert!(vulns.is_empty());
        assert_eq!(meta.digest.as_deref(), Some("sha256:cc"));
        assert_eq!(meta.created.map(|c| c.to_rfc3339()), Some("2026-11-12T08:00:00+00:00".into()));
        assert!(findings(r#"{"Results":[]}"#).unwrap().1.digest.is_none(), "a project's report has none");
    }

    #[test]
    fn one_image_rescanned_takes_its_lines_place() {
        let by = vec![
            deps("cart-api", vec![(vuln("CVE-1", "pyjwt", "HIGH"), false)]),
            deps("api · prod", vec![(vuln("CVE-1", "pyjwt", "HIGH"), false), (vuln("CVE-5", "openssl", "CRITICAL"), false)]),
            deps("api · prod · arm64", vec![(vuln("CVE-5", "openssl", "CRITICAL"), false)]),
            deps("worker", Vec::new()),
        ];
        let mut v = summarize("main".into(), "abc".into(), "0.74".into(), by, None);
        // The new image: openssl fixed, a new CVE, no arm64 any more.
        let fresh = summarize(v.branch.clone(), v.sha.clone(), v.trivy.clone(), vec![deps("api · prod", vec![(vuln("CVE-1", "pyjwt", "HIGH"), false), (vuln("CVE-7", "zlib", "HIGH"), false)])], Some(&v));
        merge(&mut v, "api · prod", fresh);
        assert_eq!(v.projects.iter().map(|p| p.project.as_str()).collect::<Vec<_>>(), ["cart-api", "api · prod", "worker"]);
        let ids: Vec<(&str, Vec<&str>, bool)> = v.findings.iter().map(|f| (f.id.as_str(), f.projects.iter().map(String::as_str).collect(), f.new)).collect();
        assert_eq!(ids, [("CVE-7", vec!["api · prod"], true), ("CVE-1", vec!["cart-api", "api · prod"], false)], "new first");
    }
}
