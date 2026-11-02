//! The Docker block's collector: the containers, images, volumes and disk of a Docker daemon (Docker Desktop,
//! OrbStack, colima…), read through the `docker` CLI and its JSON output. Containers and their
//! usage are re-read every `[docker] poll_seconds`; the disk (`docker system df -v`, slower) every
//! few minutes, and right after an action. Actions only reach what Prism displays: a container to
//! start, stop, restart or remove, an image to remove, a kind of unused data to prune.

use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use prism_base::{i18n::tr, tr};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{DockerConfig, DockerContainer, DockerDisk, DockerImage, DockerState, DockerVolume};
use crate::BlockCtx;
/// The disk is measured less often: `system df -v` walks every layer and volume.
const DISK_EVERY: Duration = Duration::from_secs(180);
/// Lines of a container's log shown.
const LOG_LINES: u32 = 1000;

/// The daemon read, shared by the collector and the window's actions.
struct Docker {
    ctx: BlockCtx,
    cfg: DockerConfig,
    /// The state last published: the actions only reach what it shows.
    shown: std::sync::Mutex<DockerState>,
    refresh: tokio::sync::Notify,
    /// The next pass measures the disk again.
    disk_due: AtomicBool,
}

static DAEMON: prism_base::rt::Swap<Docker> = prism_base::rt::Swap::new();

fn docker() -> Result<&'static Docker> {
    DAEMON.get().context(tr("the Docker block is off ([docker] enabled)"))
}

/// Reads the daemon now, then every `poll_seconds`.
pub async fn run(cfg: DockerConfig, ctx: BlockCtx) {
    let every = Duration::from_secs(cfg.poll_seconds.max(5));
    let d = DAEMON.set(Docker { ctx: ctx.clone(), cfg, shown: Default::default(), refresh: tokio::sync::Notify::new(), disk_due: AtomicBool::new(true) });
    let mut measured: Option<Instant> = None;
    let mut sizes: HashMap<String, u64> = HashMap::new();
    let mut disk: Option<Disk> = None;
    let mut first = true;
    loop {
        if !std::mem::take(&mut first) {
            tokio::select! {
                _ = ctx.pause(every) => {}
                _ = d.refresh.notified() => {}
            }
        }
        let due = d.disk_due.swap(false, Ordering::SeqCst) || measured.is_none_or(|at| at.elapsed() > DISK_EVERY);
        match d.pass(due).await {
            Ok(Pass { mut state, measured_disk }) => {
                if let Some(m) = measured_disk {
                    sizes = m.container_sizes.clone();
                    disk = Some(m);
                    measured = Some(Instant::now());
                }
                if let Some(m) = &disk {
                    state.images = m.images.clone();
                    state.volumes = m.volumes.clone();
                    state.disk = m.kinds.clone();
                    state.disk_at = Some(m.at);
                }
                for c in &mut state.containers {
                    c.size_bytes = sizes.get(&c.id).copied();
                }
                ctx.set_error(None);
                d.show(state);
            }
            Err(e) => {
                tracing::warn!("docker: {e:#}");
                ctx.set_error(Some(format!("{e:#}")));
                d.show(DockerState { context: d.cfg.context.trim().to_string(), reclaimable_gb: d.cfg.reclaimable_gb, ..Default::default() });
                // Measured again as soon as the daemon is back.
                measured = None;
            }
        }
    }
}

struct Pass {
    state: DockerState,
    measured_disk: Option<Disk>,
}

#[derive(Clone)]
struct Disk {
    at: DateTime<Utc>,
    images: Vec<DockerImage>,
    volumes: Vec<DockerVolume>,
    kinds: Vec<DockerDisk>,
    container_sizes: HashMap<String, u64>,
}

impl Docker {
    /// `docker [--context …] <args>`: its standard output, or its error.
    async fn cli(&self, args: &[&str]) -> Result<String> {
        let mut cmd = crate::platform::command("docker");
        let context = self.cfg.context.trim();
        if !context.is_empty() {
            cmd.args(["--context", context]);
        }
        let out = tokio::time::timeout(Duration::from_secs(60), cmd.args(args).kill_on_drop(true).output()).await.context(tr("docker did not answer within a minute"))?.context(tr("docker not found: is it installed, in the login shell's PATH?"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("docker {}: {}", args.first().copied().unwrap_or_default(), err.trim().lines().last().unwrap_or("failed"));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    async fn pass(&self, disk: bool) -> Result<Pass> {
        let server = self.cli(&["version", "--format", "{{.Server.Version}}"]).await.context(tr("Docker does not answer"))?.trim().to_string();
        let context = match self.cfg.context.trim() {
            "" => self.cli(&["context", "show"]).await.map(|c| c.trim().to_string()).unwrap_or_default(),
            c => c.to_string(),
        };
        let mut containers: Vec<DockerContainer> = lines::<RawContainer>(&self.cli(&["ps", "-a", "--no-trunc", "--format", "json"]).await?).into_iter().map(DockerContainer::from).collect();
        if containers.iter().any(|c| c.state == "running") {
            let stats: HashMap<String, RawStats> = lines::<RawStats>(&self.cli(&["stats", "--no-stream", "--no-trunc", "--format", "json"]).await?).into_iter().map(|s| (s.id.clone(), s)).collect();
            for c in &mut containers {
                if let Some(s) = stats.get(&c.id) {
                    c.cpu_percent = percent(&s.cpu_perc);
                    c.mem_percent = percent(&s.mem_perc);
                    c.mem_bytes = s.mem_usage.split('/').next().and_then(bytes);
                }
            }
        }
        // Running first, then by Compose project and name.
        containers.sort_by(|a, b| (a.state != "running", &a.project, &a.name).cmp(&(b.state != "running", &b.project, &b.name)));
        let measured_disk = if disk { Some(self.disk().await?) } else { None };
        Ok(Pass { state: DockerState { context, server: Some(server), containers, reclaimable_gb: self.cfg.reclaimable_gb, ..Default::default() }, measured_disk })
    }

    /// Images, volumes, containers' layers and build cache, with their sizes; the totals per kind.
    async fn disk(&self) -> Result<Disk> {
        let verbose: RawDf = serde_json::from_str(&self.cli(&["system", "df", "-v", "--format", "json"]).await?).context("unreadable docker system df -v")?;
        let kinds = lines::<RawDfKind>(&self.cli(&["system", "df", "--format", "json"]).await?).into_iter().filter_map(RawDfKind::disk).collect();
        let mut images: Vec<DockerImage> = verbose.images.into_iter().map(DockerImage::from).collect();
        images.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
        let mut volumes: Vec<DockerVolume> = verbose.volumes.into_iter().map(DockerVolume::from).collect();
        volumes.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
        let container_sizes = verbose.containers.into_iter().filter_map(|c| Some((c.id, c.size.as_deref().and_then(bytes)?))).collect();
        Ok(Disk { at: Utc::now(), images, volumes, kinds, container_sizes })
    }

    /// The displayed container `id`, or an error.
    fn container(&self, id: &str) -> Result<DockerContainer> {
        self.shown.lock().expect("docker state").containers.iter().find(|c| c.id == id).cloned().with_context(|| tr!("container {id} is not one Prism displays", id = id))
    }

    /// Published, when it changed.
    fn show(&self, state: DockerState) {
        let mut shown = self.shown.lock().expect("docker state");
        if *shown != state {
            self.ctx.publish(&state);
            *shown = state;
        }
    }

    /// Re-read now, the disk included.
    fn again(&self) {
        self.disk_due.store(true, Ordering::SeqCst);
        self.refresh.notify_one();
    }
}

/// Starts, stops, restarts or removes (stopped only) a displayed container.
pub async fn container_action(id: &str, action: &str) -> Result<()> {
    let d = docker()?;
    let c = d.container(id)?;
    let args: &[&str] = match action {
        "start" | "stop" | "restart" => &[action, id],
        "remove" if c.state == "running" => bail!(tr!("stop {name} before removing it", name = c.name)),
        "remove" => &["rm", id],
        other => bail!("unknown action {other}"),
    };
    tracing::info!("docker: {action} {}", c.name);
    d.cli(args).await?;
    d.again();
    Ok(())
}

/// The last lines of a displayed container's log, its output and errors interleaved.
pub async fn logs(id: &str) -> Result<String> {
    let d = docker()?;
    d.container(id)?;
    let mut cmd = crate::platform::command("docker");
    let context = d.cfg.context.trim();
    if !context.is_empty() {
        cmd.args(["--context", context]);
    }
    let out = cmd.args(["logs", "--tail", &LOG_LINES.to_string(), id]).output().await.context(tr("docker not found"))?;
    anyhow::ensure!(out.status.success(), "docker logs: {}", String::from_utf8_lossy(&out.stderr).trim());
    // `docker logs` writes the container's stderr on its own: both, the output first.
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        text.push_str(&err);
    }
    Ok(text)
}

/// Removes a displayed image (Docker refuses one a container uses).
pub async fn remove_image(id: &str) -> Result<()> {
    let d = docker()?;
    let name = d.shown.lock().expect("docker state").images.iter().find(|i| i.id == id).map(|i| i.name.clone()).with_context(|| tr!("image {id} is not one Prism displays", id = id))?;
    tracing::info!("docker: remove image {name}");
    d.cli(&["image", "rm", id]).await?;
    d.again();
    Ok(())
}

/// Removes the unused data of a kind: `images` (dangling), `containers` (stopped), `volumes`
/// (every one no container uses, named ones included), `build_cache`. Returns Docker's summary.
pub async fn prune(kind: &str) -> Result<String> {
    let d = docker()?;
    let args: &[&str] = match kind {
        "images" => &["image", "prune", "-f"],
        "containers" => &["container", "prune", "-f"],
        "volumes" => &["volume", "prune", "-a", "-f"],
        "build_cache" => &["builder", "prune", "-f"],
        other => bail!("nothing to prune as {other}"),
    };
    tracing::info!("docker: prune {kind}");
    let out = d.cli(args).await?;
    d.again();
    Ok(out.lines().rev().find(|l| l.contains("reclaimed")).unwrap_or("done").trim().to_string())
}

/// One JSON object per line, as `--format json` writes lists; unreadable lines skipped.
fn lines<T: for<'de> Deserialize<'de>>(text: &str) -> Vec<T> {
    text.lines().filter(|l| !l.trim().is_empty()).filter_map(|l| serde_json::from_str(l).map_err(|e| tracing::debug!("docker: unreadable line: {e}")).ok()).collect()
}

/// Docker's sizes: `4.021GB`, `512kB`, `0B` (decimal), `120MiB` (binary, in `docker stats`),
/// `1.05MB (virtual 300MB)` (the first one counts).
fn bytes(text: &str) -> Option<u64> {
    let t = text.split_whitespace().next()?;
    let at = t.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (n, unit) = t.split_at(at);
    let factor: f64 = match unit {
        "B" => 1.0,
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0_f64.powi(4),
        _ => return None,
    };
    Some((n.parse::<f64>().ok()? * factor).round() as u64)
}

/// `12.5%` → 12.5.
fn percent(text: &str) -> Option<f64> {
    text.trim().trim_end_matches('%').parse().ok()
}

/// Docker's dates: `2026-10-01 09:12:33 +0200 CEST`.
fn created(text: &str) -> Option<DateTime<Utc>> {
    let mut parts = text.split_whitespace();
    let stamp = format!("{} {} {}", parts.next()?, parts.next()?, parts.next()?);
    DateTime::parse_from_str(&stamp, "%Y-%m-%d %H:%M:%S %z").ok().map(|d| d.with_timezone(&Utc))
}

/// `0.0.0.0:8080->80/tcp, :::8080->80/tcp, 5432/tcp` → `8080→80/tcp`, `5432/tcp`, without repeats.
fn ports(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let short = match p.split_once("->") {
            Some((host, inside)) => format!("{}→{inside}", host.rsplit(':').next().unwrap_or(host)),
            None => p.to_string(),
        };
        if !out.contains(&short) {
            out.push(short);
        }
    }
    out
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawContainer {
    #[serde(rename = "ID")]
    id: String,
    names: String,
    image: String,
    state: String,
    status: String,
    #[serde(default)]
    labels: String,
    #[serde(default)]
    ports: String,
    #[serde(default)]
    created_at: String,
}

impl From<RawContainer> for DockerContainer {
    fn from(c: RawContainer) -> Self {
        let health = ["unhealthy", "healthy", "health: starting"].iter().find(|h| c.status.contains(&format!("({h})"))).map(|h| h.trim_start_matches("health: ").to_string());
        let exit_code = c.status.strip_prefix("Exited (").and_then(|r| r.split(')').next()).and_then(|n| n.parse().ok());
        let project = c.labels.split(',').find_map(|l| l.strip_prefix("com.docker.compose.project=")).map(String::from);
        DockerContainer {
            id: c.id,
            name: c.names.split(',').next().unwrap_or_default().to_string(),
            image: c.image,
            state: c.state,
            health,
            exit_code,
            project,
            ports: ports(&c.ports),
            created_at: created(&c.created_at),
            status: c.status,
            size_bytes: None,
            cpu_percent: None,
            mem_bytes: None,
            mem_percent: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawStats {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "CPUPerc", default)]
    cpu_perc: String,
    #[serde(default)]
    mem_perc: String,
    #[serde(default)]
    mem_usage: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawDf {
    #[serde(default)]
    images: Vec<RawImage>,
    #[serde(default)]
    containers: Vec<RawDfContainer>,
    #[serde(default)]
    volumes: Vec<RawVolume>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawImage {
    #[serde(rename = "ID")]
    id: String,
    repository: String,
    tag: String,
    size: String,
    #[serde(default)]
    unique_size: String,
    #[serde(default)]
    containers: String,
    #[serde(default)]
    created_at: String,
}

impl From<RawImage> for DockerImage {
    fn from(i: RawImage) -> Self {
        let name = match (i.repository.as_str(), i.tag.as_str()) {
            ("<none>", _) => "<none>".to_string(),
            (repo, "<none>" | "") => repo.to_string(),
            (repo, tag) => format!("{repo}:{tag}"),
        };
        DockerImage { id: i.id, name, size_bytes: bytes(&i.size).unwrap_or(0), unique_bytes: bytes(&i.unique_size), containers: i.containers.trim().parse().unwrap_or(0), created_at: created(&i.created_at) }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawDfContainer {
    #[serde(rename = "ID")]
    id: String,
    size: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawVolume {
    name: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    links: String,
}

impl From<RawVolume> for DockerVolume {
    fn from(v: RawVolume) -> Self {
        DockerVolume { name: v.name, size_bytes: bytes(&v.size), links: v.links.trim().parse().unwrap_or(0) }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawDfKind {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(default)]
    total_count: String,
    #[serde(default)]
    active: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    reclaimable: String,
}

impl RawDfKind {
    fn disk(self) -> Option<DockerDisk> {
        let kind = match self.kind.as_str() {
            "Images" => "images",
            "Containers" => "containers",
            "Local Volumes" => "volumes",
            "Build Cache" => "build_cache",
            _ => return None,
        };
        Some(DockerDisk {
            kind: kind.into(),
            total: self.total_count.trim().parse().unwrap_or(0),
            active: self.active.trim().parse().unwrap_or(0),
            size_bytes: bytes(&self.size).unwrap_or(0),
            reclaimable_bytes: bytes(&self.reclaimable).unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_in_docker_units() {
        assert_eq!(bytes("4.021GB"), Some(4_021_000_000));
        assert_eq!(bytes("0B"), Some(0));
        assert_eq!(bytes("512kB"), Some(512_000));
        assert_eq!(bytes("120MiB "), Some(120 * 1024 * 1024));
        assert_eq!(bytes("1.566GB (38%)"), Some(1_566_000_000));
        assert_eq!(bytes("1.05MB (virtual 300MB)"), Some(1_050_000));
        assert_eq!(bytes("N/A"), None);
    }

    #[test]
    fn a_container_reads_its_status() {
        let raw = |status: &str, state: &str| -> RawContainer {
            serde_json::from_value(serde_json::json!({
                "ID": "abc", "Names": "shop-db-1", "Image": "postgres:16", "State": state, "Status": status,
                "Labels": "com.docker.compose.service=db,com.docker.compose.project=shop",
                "Ports": "0.0.0.0:5432->5432/tcp, :::5432->5432/tcp", "CreatedAt": "2026-10-01 09:12:33 +0200 CEST"
            }))
            .unwrap()
        };
        let up = DockerContainer::from(raw("Up 3 days (unhealthy)", "running"));
        assert_eq!((up.health.as_deref(), up.project.as_deref(), up.exit_code), (Some("unhealthy"), Some("shop"), None));
        assert_eq!(up.ports, ["5432→5432/tcp"]);
        assert_eq!(up.created_at.unwrap().to_rfc3339(), "2026-10-01T07:12:33+00:00");
        let starting = DockerContainer::from(raw("Up 5 seconds (health: starting)", "running"));
        assert_eq!(starting.health.as_deref(), Some("starting"));
        let exited = DockerContainer::from(raw("Exited (137) 2 hours ago", "exited"));
        assert_eq!((exited.exit_code, exited.health), (Some(137), None));
    }

    #[test]
    fn the_disk_per_kind() {
        let kinds = lines::<RawDfKind>(
            "{\"Active\":\"5\",\"Reclaimable\":\"1.566GB (38%)\",\"Size\":\"4.021GB\",\"TotalCount\":\"7\",\"Type\":\"Images\"}\n{\"Active\":\"4\",\"Reclaimable\":\"18.2GB (91%)\",\"Size\":\"19.8GB\",\"TotalCount\":\"27\",\"Type\":\"Local Volumes\"}\n",
        );
        let disk: Vec<DockerDisk> = kinds.into_iter().filter_map(RawDfKind::disk).collect();
        assert_eq!((disk[0].kind.as_str(), disk[0].total, disk[0].reclaimable_bytes), ("images", 7, 1_566_000_000));
        assert_eq!((disk[1].kind.as_str(), disk[1].active, disk[1].size_bytes), ("volumes", 4, 19_800_000_000));
    }

    #[test]
    fn images_are_named_by_repository_and_tag() {
        let image = |repo: &str, tag: &str| DockerImage::from(RawImage { id: "sha".into(), repository: repo.into(), tag: tag.into(), size: "1GB".into(), unique_size: "300MB".into(), containers: "2".into(), created_at: String::new() });
        assert_eq!(image("postgres", "16").name, "postgres:16");
        assert_eq!(image("<none>", "<none>").name, "<none>");
        assert_eq!((image("app", "<none>").name.as_str(), image("x", "y").unique_bytes, image("x", "y").containers), ("app", Some(300_000_000), 2));
    }
}
