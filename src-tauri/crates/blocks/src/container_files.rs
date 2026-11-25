//! A container's files, from its logs pane (a pod's) or the Docker block (a container's): its folders
//! listed, a text file shown, a file or a folder downloaded, files sent into a folder. Through the
//! `docker` and `kubectl` CLIs, with the rights they have; what Prism lets through is the window's
//! to check (`[docker] files`, `[clusters] files`, a container Prism shows).
//!
//! Listing needs a shell in the container (`sh`, `stat`): a minimal image has none, and says so.
//! Reading a file (shown or downloaded) needs `cat` in a pod; Docker copies it out of a container
//! even stopped or without a shell (`docker cp`, a tar). Editing one needs it running, with `sh`.
//! A folder comes as a `.tar.gz`: `tar` in a pod, `docker cp` in Docker.
//! A file is sent by `docker cp`, or into a pod by `sh -c 'cat > …'` (no `tar` needed there). A
//! file edited is written over in place (`REWRITE`), keeping its owner and mode, once Prism checked
//! it did not change since it was read.

use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use chrono::{DateTime, Utc};
use prism_base::{i18n::tr, platform, tr};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};

/// Entries of a folder listed at most.
const MAX_ENTRIES: usize = 5000;
/// A text file shown up to this many bytes.
pub const TEXT_MAX: usize = 1 << 20;
/// Room for a tar's headers before its file (`docker cp`): its own, a long name's, PAX's.
const TAR_ROOM: usize = 16 << 10;
/// How long a listing or a read waits for the container; a transfer, for its next bytes.
const WAIT: Duration = Duration::from_secs(30);
/// A send is given this many bytes a second, beyond `WAIT`, before it is given up.
const SLOW_LINK: u64 = 256 << 10;

/// A container, as the window names it.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ContainerRef {
    /// A container of the Docker block, by its id.
    Docker { id: String },
    /// A pod's container (its first, when none is named).
    Pod { context: String, namespace: String, pod: String, container: Option<String> },
}

/// A container reached: by `docker` (its context, empty for the current one) or `kubectl`.
#[derive(Debug, Clone, PartialEq)]
pub enum Container {
    Docker { context: String, id: String },
    Pod { context: String, namespace: String, pod: String, container: Option<String> },
}

/// One entry of a folder.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ContainerEntry {
    pub name: String,
    /// A folder, or a link to one.
    pub dir: bool,
    /// A link's target.
    pub link: Option<String>,
    /// In bytes (a link's own); absent without `stat` in the container.
    pub size: Option<u64>,
    pub modified: Option<DateTime<Utc>>,
}

/// A folder of a container: its absolute path, its entries (folders first).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ContainerListing {
    pub path: String,
    pub entries: Vec<ContainerEntry>,
    /// More than `MAX_ENTRIES` entries: the first ones only.
    pub truncated: bool,
}

/// A file shown: its text, unless it is binary; its first `TEXT_MAX` bytes only.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ContainerFileText {
    pub text: Option<String>,
    pub binary: bool,
    pub truncated: bool,
    /// The text's fingerprint (SHA-256), when it is whole: what an edit checks before writing.
    pub hash: Option<String>,
}

/// An edit's outcome: written, or not, the file having changed since it was read (its text now,
/// none when it is no longer text Prism shows whole).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ContainerSave {
    Saved { hash: String },
    Changed { text: Option<String>, hash: Option<String> },
}

/// Lists `$1`: its absolute path, `stat`'s sizes and dates, `//`, then one line per entry by its
/// kind (`d` folder, `f` other, `D`/`L` a link to a folder / to something else, its target after a
/// tab). Exit 3: no such folder. Without `stat`, no size nor date; the kinds are the shell's own.
const LIST: &str = r#"cd -- "$1" 2>/dev/null || exit 3
pwd
set -- .* *
stat -c '%s %Y %n' -- "$@" 2>/dev/null
echo //
for f do
  case $f in .|..) continue ;; esac
  if [ -L "$f" ]; then
    if [ -d "$f" ]; then t=D; else t=L; fi
    printf '%s %s\t%s\n' "$t" "$f" "$(readlink -- "$f" 2>/dev/null)"
  elif [ -d "$f" ]; then printf 'd %s\n' "$f"
  elif [ -e "$f" ]; then printf 'f %s\n' "$f"
  fi
done
exit 0"#;

/// Writes its standard input to `$1`.
const WRITE: &str = r#"cat > "$1""#;

/// Writes its standard input over the file `$1`, in place: it keeps its owner and its mode. Read
/// first into a file beside it when its folder lets (`$1.prism-edit.<pid>`, only its owner's): a
/// send cut short leaves the file as it was. Exit 4: not a file.
const REWRITE: &str = r#"[ -f "$1" ] || exit 4
t="$1.prism-edit.$$"
if (umask 077; : > "$t") 2>/dev/null; then
  cat > "$t" || { rm -f "$t"; exit 1; }
  cat "$t" > "$1"; s=$?; rm -f "$t"; exit $s
fi
exec cat > "$1""#;

/// SHA-256 of `bytes`, in hex.
fn digest(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A Kubernetes object's name (DNS-1123): nothing of an option or a path reaches `kubectl`.
fn k8s_name(what: &str, s: &str) -> Result<()> {
    let ok = !s.is_empty() && s.len() <= 253 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.') && s.as_bytes()[0].is_ascii_alphanumeric();
    ensure!(ok, "not a {what} name: {s}");
    Ok(())
}

/// A path in the container: absolute, on one line.
fn absolute(path: &str) -> Result<&str> {
    ensure!(path.starts_with('/') && !path.contains(['\0', '\n']), tr!("{path}: not an absolute path in the container", path = path));
    Ok(path)
}

/// The last segment of a container's path (`/` for the root).
fn base_name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or("root")
}

impl Container {
    /// A container of the Docker block: its id as Docker writes them (no option slipped in).
    pub fn docker(context: &str, id: &str) -> Result<Self> {
        ensure!(!id.is_empty() && !id.starts_with('-') && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)), "not a container id: {id}");
        Ok(Self::Docker { context: context.to_string(), id: id.to_string() })
    }

    /// A pod's container: Kubernetes names only, the context as `--context=` whatever its name.
    pub fn pod(context: &str, namespace: &str, pod: &str, container: Option<&str>) -> Result<Self> {
        ensure!(!context.is_empty(), "no kube context");
        k8s_name("namespace", namespace)?;
        k8s_name("pod", pod)?;
        if let Some(c) = container {
            k8s_name("container", c)?;
        }
        Ok(Self::Pod { context: context.to_string(), namespace: namespace.to_string(), pod: pod.to_string(), container: container.map(String::from) })
    }

    fn program(&self) -> &'static str {
        match self {
            Self::Docker { .. } => "docker",
            Self::Pod { .. } => "kubectl",
        }
    }

    fn docker_cli(context: &str) -> tokio::process::Command {
        let mut cmd = platform::command("docker");
        if !context.is_empty() {
            cmd.arg(format!("--context={context}"));
        }
        cmd
    }

    /// `argv` run in the container; `input`: its standard input open.
    fn exec(&self, input: bool, argv: &[&str]) -> tokio::process::Command {
        let mut cmd = match self {
            Self::Docker { context, id } => {
                let mut cmd = Self::docker_cli(context);
                cmd.arg("exec");
                if input {
                    cmd.arg("-i");
                }
                cmd.arg(id);
                cmd
            }
            Self::Pod { context, namespace, pod, container } => {
                let mut cmd = platform::command("kubectl");
                cmd.args([format!("--context={context}"), format!("--namespace={namespace}"), "exec".into()]);
                if input {
                    cmd.arg("-i");
                }
                cmd.arg(pod);
                if let Some(c) = container {
                    cmd.arg(format!("--container={c}"));
                }
                cmd.arg("--");
                cmd
            }
        };
        cmd.args(argv).kill_on_drop(true);
        cmd
    }

    /// Why a command in the container failed, in the user's words when Prism knows them.
    fn failure(&self, stderr: &[u8], code: Option<i32>, path: &str) -> anyhow::Error {
        let err = String::from_utf8_lossy(stderr);
        let lower = err.to_lowercase();
        if code == Some(3) {
            return anyhow!(tr!("{path}: no such folder in the container", path = path));
        }
        if (lower.contains("is not running") || lower.contains("container not found")) && matches!(self, Self::Docker { .. }) {
            return anyhow!(tr("This container is stopped: start it to list its files. A file whose path you know still downloads."));
        }
        if lower.contains("executable file not found") || lower.contains("no such file or directory: unknown") || lower.contains("exec: \"sh\"") {
            return anyhow!(match self {
                Self::Docker { .. } => tr("This container has no shell (a minimal image): its files cannot be listed. A file whose path you know still downloads."),
                Self::Pod { .. } => tr("This container has no shell (a minimal image): its files cannot be listed nor read."),
            });
        }
        let line = err.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("failed");
        anyhow!("{}: {line}", self.program())
    }

    /// The folder `path` (empty: the container's working folder).
    pub async fn list(&self, path: &str) -> Result<ContainerListing> {
        let dir = if path.trim().is_empty() { "." } else { absolute(path)? };
        let run = self.exec(false, &["sh", "-c", LIST, "sh", dir]).stdin(Stdio::null()).output();
        let out = tokio::time::timeout(WAIT, run).await.map_err(|_| late())?.with_context(|| tr!("{program} not found", program = self.program()))?;
        if !out.status.success() {
            return Err(self.failure(&out.stderr, out.status.code(), dir));
        }
        parse_listing(&String::from_utf8_lossy(&out.stdout))
    }

    /// Where `path` leads, its links followed (`readlink -f`): a running container only.
    pub async fn real_path(&self, path: &str) -> Result<String> {
        let path = absolute(path)?;
        let run = self.exec(false, &["readlink", "-f", "--", path]).stdin(Stdio::null()).output();
        let out = tokio::time::timeout(WAIT, run).await.map_err(|_| late())?.with_context(|| tr!("{program} not found", program = self.program()))?;
        if !out.status.success() {
            return Err(self.failure(&out.stderr, out.status.code(), path));
        }
        let real = String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string();
        ensure!(real.starts_with('/'), "the container's answer cannot be read");
        Ok(real)
    }

    /// The first `TEXT_MAX` bytes of a file, as text unless they look binary. Docker: out of a tar
    /// of it (`docker cp`), even from a stopped container or one without a shell; a pod: `cat`.
    pub async fn text(&self, path: &str) -> Result<ContainerFileText> {
        let path = absolute(path)?;
        let (bytes, truncated) = match self {
            Self::Docker { context, id } => {
                let mut cmd = Self::docker_cli(context);
                // `-L`: a link gives what it points to.
                cmd.args(["cp", "-L", &format!("{id}:{path}"), "-"]);
                let (tar, _) = self.head(cmd, path, (TEXT_MAX + 1 + TAR_ROOM) as u64).await?;
                tar_head(&tar)?
            }
            Self::Pod { .. } => {
                let (mut bytes, _) = self.head(self.exec(false, &["cat", "--", path]), path, TEXT_MAX as u64 + 1).await?;
                let truncated = bytes.len() > TEXT_MAX;
                bytes.truncate(TEXT_MAX);
                (bytes, truncated)
            }
        };
        Ok(as_text(bytes, truncated))
    }

    /// The first `cap` bytes `cmd` writes, and whether it ended before them (its failure said then).
    /// Its errors read meanwhile: a full pipe of them would stop it.
    async fn head(&self, mut cmd: tokio::process::Command, path: &str, cap: u64) -> Result<(Vec<u8>, bool)> {
        let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn().with_context(|| tr!("{program} not found", program = self.program()))?;
        let mut stdout = child.stdout.take().context("no output")?;
        let mut stderr = child.stderr.take().context("no errors")?;
        let errors = tokio::spawn(async move {
            let mut e = Vec::new();
            let _ = stderr.read_to_end(&mut e).await;
            e
        });
        let mut bytes = Vec::new();
        tokio::time::timeout(WAIT, (&mut stdout).take(cap).read_to_end(&mut bytes)).await.map_err(|_| late())??;
        if bytes.len() as u64 >= cap {
            let _ = child.start_kill();
            return Ok((bytes, false));
        }
        // Killed on the way out (`kill_on_drop`) if it hangs once its output is read.
        let status = tokio::time::timeout(WAIT, child.wait()).await.map_err(|_| late())??;
        if !status.success() {
            return Err(self.failure(&errors.await.unwrap_or_default(), status.code(), path));
        }
        Ok((bytes, true))
    }

    /// The text file `path` written over with `text`, in place (`REWRITE`). With `expected` (its
    /// fingerprint when read), read again first: changed since, it is left as it is and its text
    /// now returned. Running containers only (`exec`).
    pub async fn rewrite(&self, path: &str, text: &str, expected: Option<&str>) -> Result<ContainerSave> {
        let path = absolute(path)?;
        ensure!(text.len() <= TEXT_MAX, tr!("over {size} MB: too big to edit here", size = (TEXT_MAX >> 20).to_string()));
        if let Some(expected) = expected {
            let now = self.text(path).await?;
            if now.hash.as_deref() != Some(expected) {
                let whole = !now.truncated;
                return Ok(ContainerSave::Changed { text: now.text.filter(|_| whole), hash: now.hash });
            }
        }
        let mut child = self.exec(true, &["sh", "-c", REWRITE, "sh", path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().with_context(|| tr!("{program} not found", program = self.program()))?;
        let mut stdin = child.stdin.take().context("no input")?;
        let bytes = text.as_bytes().to_vec();
        let write = async move {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(&bytes).await?;
            stdin.shutdown().await
        };
        let limit = WAIT + Duration::from_secs(text.len() as u64 / SLOW_LINK);
        let run = async {
            // A refused write (the command ended early) says why on its error output.
            let _ = write.await;
            child.wait_with_output().await
        };
        let out = tokio::time::timeout(limit, run).await.map_err(|_| late())??;
        if out.status.code() == Some(4) {
            bail!(tr!("{path}: not a file in the container", path = path));
        }
        if matches!(self, Self::Docker { .. }) && String::from_utf8_lossy(&out.stderr).contains("is not running") {
            bail!(tr("This container is stopped: start it to save the file."));
        }
        if !out.status.success() {
            return Err(self.failure(&out.stderr, out.status.code(), path));
        }
        tracing::info!("container files: {path} edited");
        Ok(ContainerSave::Saved { hash: digest(text.as_bytes()) })
    }

    /// `path` written to `dest`: a file as it is, a folder (`dir`) as a `.tar.gz`; refused beyond
    /// `max` bytes. Written beside it first, then renamed: a failure leaves a file already at
    /// `dest` (one the save dialog replaces) as it was. `create` makes the files written.
    pub async fn save(&self, path: &str, dir: bool, dest: &Path, max: u64, create: fn(&Path) -> std::io::Result<std::fs::File>) -> Result<()> {
        let path = absolute(path)?;
        let part = beside(dest, "prism-part");
        let _ = std::fs::remove_file(&part);
        let done = match self.save_into(path, dir, &part, max, create).await {
            Ok(()) => std::fs::rename(&part, dest).with_context(|| format!("writing {}", dest.display())),
            Err(e) => Err(e),
        };
        if done.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        done
    }

    async fn save_into(&self, path: &str, dir: bool, dest: &Path, max: u64, create: fn(&Path) -> std::io::Result<std::fs::File>) -> Result<()> {
        let gz = |file| flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let gz_end = |gz: flate2::write::GzEncoder<std::fs::File>| gz.finish().and_then(|f| f.sync_all());
        match (self, dir) {
            // Docker: a tar of it, even from a stopped container or one without a shell.
            (Self::Docker { context, id }, _) => {
                let mut cmd = Self::docker_cli(context);
                // `-L`: a link (`/etc/os-release` on Alpine) gives what it points to.
                cmd.args(["cp", "-L", &format!("{id}:{path}"), "-"]).kill_on_drop(true);
                if dir {
                    self.pipe(cmd, path, max, gz(create(dest)?), gz_end).await
                } else {
                    // The tar beside where the file goes, then the file taken out of it.
                    let tar = beside(dest, "tar");
                    let piped = self.pipe(cmd, path, max.saturating_add(1 << 20), create(&tar)?, |f| f.sync_all()).await;
                    let out = match piped {
                        Ok(()) => {
                            let (tar, dest) = (tar.clone(), dest.to_path_buf());
                            tokio::task::spawn_blocking(move || untar_one(&tar, &dest, max, create)).await?
                        }
                        Err(e) => Err(e),
                    };
                    let _ = std::fs::remove_file(&tar);
                    out
                }
            }
            (Self::Pod { .. }, false) => self.pipe(self.exec(false, &["cat", "--", path]), path, max, create(dest)?, |f| f.sync_all()).await,
            (Self::Pod { .. }, true) => {
                let parent = match path.trim_end_matches('/').rsplit_once('/') {
                    Some(("", _)) | None => "/",
                    Some((p, _)) => p,
                };
                let name = if path.trim_end_matches('/').is_empty() { "." } else { base_name(path) };
                let no_shell = tr("This container has no shell (a minimal image): its files cannot be listed nor read.");
                self.pipe(self.exec(false, &["tar", "cf", "-", "-C", parent, "--", name]), path, max, gz(create(dest)?), gz_end)
                    .await
                    .map_err(|e| if e.to_string().contains(no_shell) || e.to_string().contains("\"tar\"") { anyhow!(tr("This container has no tar: a folder cannot be downloaded from it, its files can, one by one.")) } else { e })
            }
        }
    }

    /// `cmd`'s output written to `out`, then `end` (a gzip's last bytes); its failure said. The
    /// disk and the compression on a thread of their own, not the async runtime's; a container
    /// that sends nothing for `WAIT` is given up (and its command killed).
    async fn pipe<W: Write + Send + 'static>(&self, mut cmd: tokio::process::Command, path: &str, max: u64, mut out: W, end: impl FnOnce(W) -> std::io::Result<()> + Send + 'static) -> Result<()> {
        let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn().with_context(|| tr!("{program} not found", program = self.program()))?;
        let mut stdout = child.stdout.take().context("no output")?;
        let mut stderr = child.stderr.take().context("no errors")?;
        let errors = tokio::spawn(async move {
            let mut e = Vec::new();
            let _ = stderr.read_to_end(&mut e).await;
            e
        });
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
        let writer = tokio::task::spawn_blocking(move || -> std::io::Result<W> {
            while let Some(chunk) = rx.blocking_recv() {
                out.write_all(&chunk)?;
            }
            Ok(out)
        });
        let copied = copy_capped(&mut stdout, &tx, max).await;
        drop(tx);
        let written = writer.await?;
        if let Err(e) = copied {
            let _ = child.start_kill();
            // The disk's failure, when it made the copy stop.
            return Err(written.err().map(anyhow::Error::from).unwrap_or(e));
        }
        let out = written?;
        let status = tokio::time::timeout(WAIT, child.wait()).await.map_err(|_| late())??;
        let err = errors.await.unwrap_or_default();
        if !status.success() {
            return Err(self.failure(&err, status.code(), path));
        }
        tokio::task::spawn_blocking(move || end(out)).await??;
        Ok(())
    }

    /// Local files (`sources`) sent into the folder `dir`, each under its own name, replacing one
    /// there. How many were sent.
    pub async fn upload(&self, dir: &str, sources: &[PathBuf]) -> Result<usize> {
        for src in sources {
            let meta = std::fs::metadata(src).with_context(|| tr!("{path}: not found", path = src.display().to_string()))?;
            ensure!(meta.is_file(), tr!("{path}: only files can be sent, not a folder", path = src.display().to_string()));
            ensure!(src.is_absolute(), "not an absolute path: {}", src.display());
            let name = src.file_name().and_then(|n| n.to_str()).with_context(|| tr!("{path}: a name the container cannot take", path = src.display().to_string()))?;
            self.send(dir, src, name, meta.len()).await?;
        }
        Ok(sources.len())
    }

    /// Bytes (a file dropped on the window) sent into the folder `dir` as `name`: written first to
    /// a folder only its owner reads (on Linux, `/tmp` is everyone's), removed after.
    pub async fn put(&self, dir: &str, name: &str, bytes: &[u8]) -> Result<()> {
        file_name(name)?;
        let tmp = std::env::temp_dir().join(format!("prism-put-{}", uuid::Uuid::new_v4().simple()));
        let mut folder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut folder, 0o700);
        folder.create(&tmp)?;
        let src = tmp.join(name);
        let sent = async {
            prism_agents::files::create_private(&src)?.write_all(bytes)?;
            self.send(dir, &src, name, bytes.len() as u64).await
        }
        .await;
        let _ = std::fs::remove_dir_all(&tmp);
        sent
    }

    /// The local file `src` (`size` bytes) written into the folder `dir` as `name`, replacing one
    /// there. Given up (its command killed) when it takes longer than `WAIT` plus a slow link's
    /// time for its size.
    async fn send(&self, dir: &str, src: &Path, name: &str, size: u64) -> Result<()> {
        let dir = absolute(dir)?.trim_end_matches('/');
        file_name(name)?;
        let to = format!("{dir}/{name}");
        let run = match self {
            Self::Docker { context, id } => Self::docker_cli(context).arg("cp").arg(src).arg(format!("{id}:{to}")).stdin(Stdio::null()).kill_on_drop(true).output(),
            Self::Pod { .. } => self.exec(true, &["sh", "-c", WRITE, "sh", &to]).stdin(Stdio::from(std::fs::File::open(src)?)).output(),
        };
        let out = tokio::time::timeout(WAIT + Duration::from_secs(size / SLOW_LINK), run).await.map_err(|_| late())?.with_context(|| tr!("{program} not found", program = self.program()))?;
        if !out.status.success() {
            return Err(self.failure(&out.stderr, out.status.code(), if dir.is_empty() { "/" } else { dir }));
        }
        tracing::info!("container files: sent {name} into {dir}");
        Ok(())
    }
}

/// The container stopped answering.
fn late() -> anyhow::Error {
    anyhow!(tr("the container did not answer in time"))
}

/// A file's name, as the container takes it: one segment, on one line.
fn file_name(name: &str) -> Result<()> {
    ensure!(!name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\n', '\0']), tr!("{path}: a name the container cannot take", path = name));
    Ok(())
}

/// A file beside `dest`, its name followed by `.<what>`: where a download is written before it
/// takes `dest`'s place.
fn beside(dest: &Path, what: &str) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{what}"));
    dest.with_file_name(name)
}

/// `from` sent to the writer (`to`) chunk by chunk, refused beyond `max` bytes; given up when
/// nothing comes for `WAIT`, or when the writer stopped (its error said by the caller).
async fn copy_capped<R: AsyncRead + Unpin>(from: &mut R, to: &tokio::sync::mpsc::Sender<Vec<u8>>, max: u64) -> Result<()> {
    let mut buf = vec![0u8; 64 << 10];
    let mut total = 0u64;
    loop {
        let n = tokio::time::timeout(WAIT, from.read(&mut buf)).await.map_err(|_| late())??;
        if n == 0 {
            return Ok(());
        }
        total += n as u64;
        if total > max {
            bail!(tr!("over {size} MB: too big to download", size = (max >> 20).to_string()));
        }
        to.send(buf[..n].to_vec()).await.map_err(|_| anyhow!("the file could not be written"))?;
    }
}

/// The file of a one-file tar (`docker cp`'s) written to `dest`.
fn untar_one(tar: &Path, dest: &Path, max: u64, create: fn(&Path) -> std::io::Result<std::fs::File>) -> Result<()> {
    let mut archive = tar::Archive::new(std::fs::File::open(tar)?);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            bail!(tr("this is a folder: download it as a folder"));
        }
        if !kind.is_file() {
            continue;
        }
        ensure!(entry.size() <= max, tr!("over {size} MB: too big to download", size = (max >> 20).to_string()));
        std::io::copy(&mut entry, &mut create(dest)?)?;
        return Ok(());
    }
    bail!(tr("nothing to download there (a link or a special file)"))
}

/// The file at the head of a tar (`docker cp`'s, cut after `TEXT_MAX` bytes of it): its first
/// `TEXT_MAX` bytes, and whether it has more.
fn tar_head(tar: &[u8]) -> Result<(Vec<u8>, bool)> {
    let mut at = 0;
    while at + 512 <= tar.len() {
        let header = tar::Header::from_byte_slice(&tar[at..at + 512]);
        let size = header.size()? as usize;
        let kind = header.entry_type();
        let data = at + 512;
        if kind.is_dir() {
            bail!(tr("this is a folder: download it as a folder"));
        }
        if kind.is_file() && !header.as_bytes().iter().all(|&b| b == 0) {
            let shown = size.min(TEXT_MAX);
            ensure!(data + shown <= tar.len(), "the container's answer cannot be read");
            return Ok((tar[data..data + shown].to_vec(), size > TEXT_MAX));
        }
        // What comes before the file (a long name, PAX's attributes): skipped.
        at = data + size.div_ceil(512) * 512;
    }
    bail!(tr("nothing to show there (a link or a special file)"))
}

/// Bytes read from a file, as text unless they look binary (a NUL, or not UTF-8 beyond a cut
/// character at the end).
fn as_text(bytes: Vec<u8>, truncated: bool) -> ContainerFileText {
    let binary = bytes.iter().take(8192).any(|&b| b == 0);
    let text = (!binary).then(|| match String::from_utf8(bytes) {
        Ok(s) => Some(s),
        Err(e) => {
            let valid = e.utf8_error().valid_up_to();
            let bytes = e.into_bytes();
            // Cut in the middle of a character by the limit: what comes before.
            (truncated && bytes.len() - valid < 4).then(|| String::from_utf8_lossy(&bytes[..valid]).into_owned())
        }
    });
    match text.flatten() {
        Some(text) => ContainerFileText { hash: (!truncated).then(|| digest(text.as_bytes())), text: Some(text), binary: false, truncated },
        None => ContainerFileText { text: None, binary: true, truncated, hash: None },
    }
}

/// `LIST`'s answer read.
fn parse_listing(text: &str) -> Result<ContainerListing> {
    let mut lines = text.lines();
    let path = lines.next().filter(|p| p.starts_with('/')).context("the container's answer cannot be read")?.to_string();
    let mut stats: HashMap<&str, (Option<u64>, Option<DateTime<Utc>>)> = HashMap::new();
    for line in lines.by_ref() {
        if line == "//" {
            break;
        }
        let mut parts = line.splitn(3, ' ');
        if let (Some(size), Some(at), Some(name)) = (parts.next(), parts.next(), parts.next()) {
            stats.insert(name, (size.parse().ok(), at.parse().ok().and_then(|s| DateTime::from_timestamp(s, 0))));
        }
    }
    let mut entries = Vec::new();
    let mut truncated = false;
    for line in lines {
        let Some((kind, rest)) = line.split_once(' ') else { continue };
        let (name, link) = match kind {
            "D" | "L" => match rest.split_once('\t') {
                Some((n, l)) => (n, Some(l.to_string())),
                None => (rest, None),
            },
            "d" | "f" => (rest, None),
            _ => continue,
        };
        if name.is_empty() {
            continue;
        }
        if entries.len() == MAX_ENTRIES {
            truncated = true;
            break;
        }
        let (size, modified) = stats.get(name).copied().unwrap_or_default();
        entries.push(ContainerEntry { name: name.to_string(), dir: kind == "d" || kind == "D", link, size, modified });
    }
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(ContainerListing { path, entries, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_is_read() {
        let text = "/app\n4096 1700000000 src\n120 1700000100 main.py\n7 1700000200 cur\n//\nd src\nf main.py\nD cur\t/app/src\nL broken\t/nowhere\n";
        let l = parse_listing(text).unwrap();
        assert_eq!(l.path, "/app");
        let names: Vec<_> = l.entries.iter().map(|e| (e.name.as_str(), e.dir, e.link.as_deref(), e.size)).collect();
        assert_eq!(names, [("cur", true, Some("/app/src"), Some(7)), ("src", true, None, Some(4096)), ("broken", false, Some("/nowhere"), None), ("main.py", false, None, Some(120))]);
        assert_eq!(l.entries[3].modified, DateTime::from_timestamp(1_700_000_100, 0));
        assert!(!l.truncated);
    }

    #[test]
    fn a_listing_without_stat_has_names_and_kinds() {
        let l = parse_listing("/\n//\nd etc\nf .env\n").unwrap();
        assert_eq!(l.entries.iter().map(|e| (e.name.as_str(), e.dir, e.size)).collect::<Vec<_>>(), [("etc", true, None), (".env", false, None)]);
        assert!(parse_listing("sh: not found\n").is_err());
    }

    #[test]
    fn names_reaching_the_clis_are_checked() {
        assert!(Container::pod("gke_x", "apps", "api-7f9c-abcde", Some("api")).is_ok());
        assert!(Container::pod("gke_x", "apps", "--all", None).is_err());
        assert!(Container::pod("gke_x", "Apps", "api", None).is_err());
        assert!(Container::pod("", "apps", "api", None).is_err());
        assert!(Container::docker("", "3f9c1e0a7b").is_ok());
        assert!(Container::docker("", "-v").is_err());
        assert!(Container::docker("", "a:b").is_err());
        assert!(absolute("relative/path").is_err());
        assert!(absolute("/etc/hosts\n--x").is_err());
        assert_eq!(base_name("/var/log/"), "log");
        assert_eq!(base_name("/"), "root");
        assert!(file_name("notes.txt").is_ok());
        assert!(file_name("../x").is_err() && file_name("..").is_err() && file_name("a\nb").is_err());
    }

    #[test]
    fn the_commands_run() {
        let pod = Container::pod("ctx a", "apps", "api-1", Some("web")).unwrap();
        let cmd = pod.exec(true, &["cat", "--", "/x"]);
        let args: Vec<_> = cmd.as_std().get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["--context=ctx a", "--namespace=apps", "exec", "-i", "api-1", "--container=web", "--", "cat", "--", "/x"]);
        let docker = Container::docker("orbstack", "c1").unwrap();
        let args: Vec<_> = docker.exec(false, &["sh"]).as_std().get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["--context=orbstack", "exec", "c1", "sh"]);
    }

    #[test]
    fn text_or_binary() {
        assert_eq!(as_text(b"hello".to_vec(), false), ContainerFileText { text: Some("hello".into()), binary: false, truncated: false, hash: Some(digest(b"hello")) });
        assert_eq!(digest(b"hello"), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        assert_eq!(as_text(b"hello".to_vec(), true).hash, None, "a part is never fingerprinted");
        assert!(as_text(vec![b'a', 0, b'b'], false).binary);
        // A character cut by the limit: the text before it.
        let mut cut = "été".as_bytes().to_vec();
        cut.pop();
        assert_eq!(as_text(cut, true).text.as_deref(), Some("ét"));
    }

    #[test]
    fn failures_in_words() {
        let docker = Container::docker("", "c1").unwrap();
        let said = |stderr: &str, code| docker.failure(stderr.as_bytes(), code, "/x").to_string();
        assert!(said("", Some(3)).contains("no such folder"));
        assert!(said("Error response from daemon: container c1 is not running", Some(1)).contains("stopped"));
        assert!(said("OCI runtime exec failed: exec failed: unable to start container process: exec: \"sh\": executable file not found in $PATH: unknown", Some(126)).contains("no shell"));
        assert_eq!(said("permission denied\n", Some(1)), "docker: permission denied");
        // A pod not running: kubectl's words, not Docker's promise of a download.
        let pod = Container::pod("ctx", "apps", "api-1", None).unwrap();
        assert_eq!(pod.failure(b"error: cannot exec into a container in a completed pod; current phase is Succeeded: pod api-1 is not running", Some(1), "/x").to_string(), "kubectl: error: cannot exec into a container in a completed pod; current phase is Succeeded: pod api-1 is not running");
    }

    #[test]
    fn a_tar_gives_its_file() {
        let dir = std::env::temp_dir().join(format!("prism-cf-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let tar_path = dir.join("t.tar");
        let mut b = tar::Builder::new(std::fs::File::create(&tar_path).unwrap());
        let mut h = tar::Header::new_gnu();
        h.set_size(5);
        h.set_cksum();
        b.append_data(&mut h, "hosts", &b"hello"[..]).unwrap();
        b.finish().unwrap();
        drop(b);
        let dest = dir.join("hosts");
        untar_one(&tar_path, &dest, 100, |p| std::fs::File::create(p)).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello");
        assert!(untar_one(&tar_path, &dir.join("small"), 2, |p| std::fs::File::create(p)).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The listing script, run by this computer's shell on a folder of its own.
    #[cfg(unix)]
    #[test]
    fn the_listing_script_runs() {
        let dir = std::env::temp_dir().join(format!("prism-ls-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(dir.join("sub dir")).unwrap();
        std::fs::write(dir.join("a file.txt"), "abc").unwrap();
        std::fs::write(dir.join(".hidden"), "").unwrap();
        std::os::unix::fs::symlink("sub dir", dir.join("to-sub")).unwrap();
        let out = std::process::Command::new("sh").args(["-c", LIST, "sh"]).arg(&dir).output().unwrap();
        assert!(out.status.success());
        let l = parse_listing(&String::from_utf8_lossy(&out.stdout)).unwrap();
        let got: Vec<_> = l.entries.iter().map(|e| (e.name.as_str(), e.dir, e.link.as_deref())).collect();
        assert_eq!(got, [("sub dir", true, None), ("to-sub", true, Some("sub dir")), (".hidden", false, None), ("a file.txt", false, None)]);
        // macOS' stat has no `-c`: sizes there only where GNU or BusyBox stat answers (a container's).
        if l.entries[3].size.is_some() {
            assert_eq!(l.entries[3].size, Some(3));
        }
        let missing = std::process::Command::new("sh").args(["-c", LIST, "sh", "/nowhere/at/all"]).output().unwrap();
        assert_eq!(missing.status.code(), Some(3));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_tar_head_gives_its_file_even_cut() {
        let tar_of = |name: &str, body: &[u8]| {
            let mut b = tar::Builder::new(Vec::new());
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            b.append_data(&mut h, name, body).unwrap();
            b.into_inner().unwrap()
        };
        assert_eq!(tar_head(&tar_of("a.txt", b"hello")).unwrap(), (b"hello".to_vec(), false));
        // A name past 100 characters: a long name's header first, skipped.
        let long = format!("{}.conf", "n".repeat(150));
        assert_eq!(tar_head(&tar_of(&long, b"x = 1\n")).unwrap(), (b"x = 1\n".to_vec(), false));
        // A file over the limit, its tar cut as `text` reads it.
        let big = vec![b'a'; TEXT_MAX + 2 * TAR_ROOM];
        let tar = tar_of("big.log", &big);
        let (head, more) = tar_head(&tar[..TEXT_MAX + 1 + TAR_ROOM]).unwrap();
        assert_eq!((head.len(), more), (TEXT_MAX, true));
        let mut dir = tar::Builder::new(Vec::new());
        dir.append_dir("etc", std::env::temp_dir()).unwrap();
        assert!(tar_head(&dir.into_inner().unwrap()).unwrap_err().to_string().contains("folder"));
        assert!(tar_head(&[0u8; 1024]).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn the_rewrite_script_keeps_the_file_mode() {
        use std::{io::Write as _, os::unix::fs::PermissionsExt};
        let dir = std::env::temp_dir().join(format!("prism-rw-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("run.sh");
        std::fs::write(&file, "old\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o751)).unwrap();
        let rewrite = |path: &Path, text: &str| {
            let mut child = std::process::Command::new("sh").args(["-c", REWRITE, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
            child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
            child.wait().unwrap().code()
        };
        assert_eq!(rewrite(&file, "new text\n"), Some(0));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new text\n");
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o751);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "nothing left beside it");
        // In a folder it cannot write to, the file itself is still written over.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(rewrite(&file, "again\n"), Some(0));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "again\n");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(rewrite(&dir, "x"), Some(4), "a folder");
        assert_eq!(rewrite(&dir.join("absent"), "x"), Some(4), "no such file");
        assert!(!dir.join("absent").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// Against a real container: `PRISM_CF_DOCKER=<id> cargo test -p prism-blocks real_container -- --ignored`.
#[cfg(test)]
mod real {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn real_container() {
        let id = std::env::var("PRISM_CF_DOCKER").expect("PRISM_CF_DOCKER");
        let c = Container::docker("", &id).unwrap();
        let tmp = std::env::temp_dir().join(format!("prism-cf-real-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&tmp).unwrap();
        let create = |p: &Path| std::fs::File::create(p);

        let etc = c.list("/etc").await.unwrap();
        assert_eq!(etc.path, "/etc");
        let hostname = etc.entries.iter().find(|e| e.name == "hostname").expect("hostname");
        assert!(hostname.size.is_some() && hostname.modified.is_some(), "{hostname:?}");
        assert!(etc.entries.first().unwrap().dir);
        let work = c.list("").await.unwrap();
        assert!(work.path.starts_with('/'));
        assert!(c.list("/nowhere").await.unwrap_err().to_string().contains("no such folder"));

        let text = c.text("/etc/hostname").await.unwrap();
        assert!(!text.binary && text.text.as_deref().is_some_and(|t| !t.trim().is_empty()));
        assert!(c.text("/nowhere/at/all").await.is_err());
        assert!(c.text("/etc").await.unwrap_err().to_string().contains("folder"));
        // Where a link leads (a phone's read checked by it).
        assert_eq!(c.real_path("/etc/../etc/hostname").await.unwrap(), "/etc/hostname");
        let out = c.exec(false, &["ln", "-sf", "/proc/1/environ", "/tmp/innocent.txt"]).output().await.unwrap();
        assert!(out.status.success());
        assert_eq!(c.real_path("/tmp/innocent.txt").await.unwrap(), "/proc/1/environ");

        c.save("/etc/hostname", false, &tmp.join("hostname"), 1 << 20, create).await.unwrap();
        assert_eq!(std::fs::read_to_string(tmp.join("hostname")).unwrap(), text.text.clone().unwrap());
        c.save("/etc", true, &tmp.join("etc.tar.gz"), 64 << 20, create).await.unwrap();
        let gz = std::fs::read(tmp.join("etc.tar.gz")).unwrap();
        assert_eq!(&gz[..2], &[0x1f, 0x8b]);
        assert!(c.save("/etc", false, &tmp.join("etc-as-file"), 64 << 20, create).await.is_err());
        assert!(!tmp.join("etc-as-file").exists());
        assert!(c.save("/etc", true, &tmp.join("small.tar.gz"), 10, create).await.unwrap_err().to_string().contains("too big"));
        assert!(!tmp.join("small.tar.gz").exists());
        // A failed download leaves the file it was to replace as it was, and nothing beside it.
        std::fs::write(tmp.join("kept"), "mine").unwrap();
        assert!(c.save("/nowhere/at/all", false, &tmp.join("kept"), 1 << 20, create).await.is_err());
        assert_eq!(std::fs::read_to_string(tmp.join("kept")).unwrap(), "mine");
        assert_eq!(std::fs::read_dir(&tmp).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains(".prism-part") || e.file_name().to_string_lossy().ends_with(".tar")).count(), 0);

        std::fs::write(tmp.join("sent file.txt"), "from prism\n").unwrap();
        c.upload("/tmp", &[tmp.join("sent file.txt")]).await.unwrap();
        c.put("/tmp", "dropped.txt", b"dropped\n").await.unwrap();
        let t = c.list("/tmp").await.unwrap();
        assert!(t.entries.iter().any(|e| e.name == "sent file.txt" && e.size == Some(11)), "{t:?}");
        assert_eq!(c.text("/tmp/dropped.txt").await.unwrap().text.as_deref(), Some("dropped\n"));

        // An edit: written over when unchanged since read, left as it is otherwise.
        let read = c.text("/tmp/dropped.txt").await.unwrap();
        let saved = c.rewrite("/tmp/dropped.txt", "edited\n", read.hash.as_deref()).await.unwrap();
        assert_eq!(saved, ContainerSave::Saved { hash: digest(b"edited\n") });
        assert_eq!(c.text("/tmp/dropped.txt").await.unwrap().text.as_deref(), Some("edited\n"));
        let stale = c.rewrite("/tmp/dropped.txt", "mine\n", read.hash.as_deref()).await.unwrap();
        assert_eq!(stale, ContainerSave::Changed { text: Some("edited\n".into()), hash: Some(digest(b"edited\n")) });
        assert!(matches!(c.rewrite("/tmp/dropped.txt", "forced\n", None).await.unwrap(), ContainerSave::Saved { .. }));
        assert_eq!(c.text("/tmp/dropped.txt").await.unwrap().text.as_deref(), Some("forced\n"));
        assert!(c.rewrite("/tmp", "x", None).await.unwrap_err().to_string().contains("not a file"));
        std::fs::remove_dir_all(tmp).unwrap();
    }

    /// A stopped container: nothing listed (it says why), a file still downloaded by its path.
    #[tokio::test]
    #[ignore]
    async fn real_stopped_container() {
        let id = std::env::var("PRISM_CF_STOPPED").expect("PRISM_CF_STOPPED");
        let c = Container::docker("", &id).unwrap();
        assert!(c.list("/etc").await.unwrap_err().to_string().contains("stopped"));
        let dest = std::env::temp_dir().join(format!("prism-cf-os-release-{}", uuid::Uuid::new_v4().simple()));
        c.save("/etc/os-release", false, &dest, 1 << 20, |p| std::fs::File::create(p)).await.unwrap();
        assert!(std::fs::read_to_string(&dest).unwrap().contains("NAME="));
        std::fs::remove_file(dest).unwrap();
        // Shown too (`docker cp`, a link followed); not saved.
        let text = c.text("/etc/os-release").await.unwrap();
        assert!(text.text.as_deref().is_some_and(|t| t.contains("NAME=")) && text.hash.is_some(), "{text:?}");
        assert!(c.rewrite("/etc/os-release", "x", None).await.unwrap_err().to_string().contains("stopped"));
    }
    /// `PRISM_CF_DOCKER=<id>`, with `/tmp/run.sh` mode 751 owned by nobody: an edit keeps both.
    #[tokio::test]
    #[ignore]
    async fn real_edit_keeps_mode() {
        let c = Container::docker("", &std::env::var("PRISM_CF_DOCKER").unwrap()).unwrap();
        let read = c.text("/tmp/run.sh").await.unwrap();
        assert!(matches!(c.rewrite("/tmp/run.sh", "echo new\n", read.hash.as_deref()).await.unwrap(), ContainerSave::Saved { .. }));
        let out = c.exec(false, &["sh", "-c", "stat -c '%a %U' /tmp/run.sh; /tmp/run.sh; ls /tmp"]).output().await.unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).lines().take(2).collect::<Vec<_>>(), ["751 nobody", "new"]);
        assert!(!String::from_utf8_lossy(&out.stdout).contains("prism-edit"));
    }
}
