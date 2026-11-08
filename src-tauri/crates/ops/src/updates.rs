//! Prism's own updates: the releases of a repository (`[updates] source`), read with glab or gh,
//! so with the access each one already has, and no token of Prism's. A team's pack names its own
//! private repository, whose builds carry its blocks; the open-source Prism, its public one.
//!
//! A release is tagged `prism-v<version>` (scripts/release.sh), next to anything else the
//! repository publishes, and carries one file per system, each with its `.sha256` and its
//! `.minisig`:
//! `Prism_<version>_macos-universal.app.tar.gz` (the app itself: what an update replaces;
//! the `.dmg` is for a first install), `Prism_<version>_<amd64|arm64>.AppImage` and `.deb`,
//! `Prism_<version>_amd64.pkg.tar.zst` (Arch Linux), `Prism_<version>_windows-<x64|arm64>-setup.exe`
//! (the NSIS installer, for a first install and for an update).
//! On GitLab, the files are in the project's generic packages, the release links to them.
//!
//! Installing replaces the app where it runs (a `Prism.app`, an AppImage), then Prism restarts;
//! on Windows, the installer runs as Prism restarts (`take_installer`), without a question, and
//! starts the new Prism;
//! a `.deb` or an Arch package needs root: it is downloaded, for `sudo apt install` or
//! `sudo pacman -U`. Only a file signed with the key built into this Prism (`update-key.pub`) is
//! installed: whoever can publish in the source, or point a pack's `[updates] source` elsewhere,
//! cannot have another app installed.

use prism_base::{i18n::tr, tr};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};

/// Tags of Prism's releases, before their version.
const TAG: &str = "prism-v";

/// The public key Prism's releases are signed with (minisign, scripts/release.sh). A fork that
/// publishes its own releases puts its own key's there.
const UPDATE_KEY: &str = include_str!("../../../update-key.pub");

/// What Settings › General › Updates and the status bar show.
#[derive(Debug, Clone, Serialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UpdateInfo {
    /// This Prism's version.
    pub current: String,
    /// `[updates]` on, with a source.
    pub enabled: bool,
    pub source: String,
    /// The newest release of the source, newer or not.
    pub latest: Option<Release>,
    /// `latest` is newer than this Prism.
    pub available: bool,
    /// The releases newer than this Prism, newest first: their notes are what an update brings.
    pub newer: Vec<Release>,
    /// The release of this Prism's version, if the source has it: its notes say what it brought.
    pub installed: Option<Release>,
    /// The file of `latest` for this system, if any.
    pub asset: Option<String>,
    /// How it installs: `replace` (the app, then a restart), `download` (a .deb, an Arch package), or empty: it cannot.
    pub install: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "PrismRelease"))]
pub struct Release {
    pub version: String,
    pub tag: String,
    pub notes: String,
    /// Its page on the forge.
    pub url: String,
    #[serde(skip)]
    assets: Vec<Asset>,
}

#[derive(Debug, Clone)]
struct Asset {
    name: String,
    /// GitLab: the API path after `/api/v4/` (a generic package); GitHub: `repos/o/r/releases/assets/<id>`.
    api: String,
    host: String,
}

#[derive(Debug, PartialEq)]
enum Source<'a> {
    Gitlab(&'a str),
    Github(&'a str),
}

fn source(text: &str) -> Result<Source<'_>> {
    let text = text.trim();
    let (gitlab, rest) = match (text.strip_prefix("glab:"), text.strip_prefix("gh:")) {
        (Some(rest), _) => (true, rest),
        (_, Some(rest)) => (false, rest),
        _ => bail!(tr!("[updates] source: glab:group/project or gh:owner/repo, not {text}", text = text)),
    };
    ensure!(rest.contains('/') && !rest.starts_with('/') && !rest.ends_with('/'), tr!("[updates] source: glab:group/project or gh:owner/repo, not {text}", text = text));
    Ok(if gitlab { Source::Gitlab(rest) } else { Source::Github(rest) })
}

/// Checks the source of the config for a release newer than this Prism.
pub async fn check(cfg: &crate::config::UpdatesConfig) -> Result<UpdateInfo> {
    let mut info = UpdateInfo { current: crate::packs::PRISM.into(), enabled: cfg.enabled && !cfg.source.trim().is_empty(), source: cfg.source.trim().into(), ..Default::default() };
    if info.source.is_empty() {
        return Ok(info);
    }
    let list = releases(source(&info.source)?).await?;
    info.newer = newer(&list, &info.current);
    info.installed = list.iter().find(|r| r.version == info.current).cloned();
    let latest = newest(list);
    if let Some(r) = &latest {
        info.available = !crate::packs::satisfies(&info.current, &r.version);
        if let Some((asset, how)) = pick(&r.version, &r.assets) {
            info.asset = Some(asset.name.clone());
            info.install = how.into();
        }
    }
    info.latest = latest;
    Ok(info)
}

/// The newest of Prism's releases.
fn newest(releases: Vec<Release>) -> Option<Release> {
    releases.into_iter().reduce(|a, b| if crate::packs::satisfies(&a.version, &b.version) { a } else { b })
}

/// The releases newer than `current`, newest first.
fn newer(releases: &[Release], current: &str) -> Vec<Release> {
    let mut out: Vec<Release> = releases.iter().filter(|r| !crate::packs::satisfies(current, &r.version)).cloned().collect();
    out.sort_by(|a, b| match (crate::packs::satisfies(&a.version, &b.version), crate::packs::satisfies(&b.version, &a.version)) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    });
    out
}

/// Prism's releases of a source (`prism-v…` tags), drafts left out.
async fn releases(src: Source<'_>) -> Result<Vec<Release>> {
    let enc = crate::http::encode;
    let version = |tag: &str| tag.strip_prefix(TAG).filter(|v| v.starts_with(|c: char| c.is_ascii_digit())).map(String::from);
    match src {
        Source::Gitlab(project) => {
            #[derive(Deserialize)]
            struct R {
                tag_name: String,
                #[serde(default)]
                description: Option<String>,
                #[serde(default)]
                _links: Links,
                #[serde(default)]
                assets: Assets,
            }
            #[derive(Deserialize, Default)]
            struct Links {
                #[serde(rename = "self", default)]
                this: String,
            }
            #[derive(Deserialize, Default)]
            struct Assets {
                #[serde(default)]
                links: Vec<Link>,
            }
            #[derive(Deserialize)]
            struct Link {
                name: String,
                url: String,
            }
            let host = &crate::conventions::get().gitlab_host;
            let out = glab(&format!("projects/{}/releases?per_page=50", enc(project)), host).await?;
            let list: Vec<R> = serde_json::from_slice(&out).context("GitLab's answer is not a list of releases")?;
            Ok(list
                .into_iter()
                .filter_map(|r| {
                    let assets = r
                        .assets
                        .links
                        .into_iter()
                        .filter_map(|l| {
                            // `https://host/api/v4/projects/…/packages/generic/…`: read through glab, with its access.
                            let (base, api) = l.url.split_once("/api/v4/")?;
                            Some(Asset { name: l.name, api: api.to_string(), host: base.trim_start_matches("https://").trim_start_matches("http://").to_string() })
                        })
                        .collect();
                    Some(Release { version: version(&r.tag_name)?, notes: r.description.unwrap_or_default(), url: r._links.this, tag: r.tag_name, assets })
                })
                .collect())
        }
        Source::Github(repo) => {
            #[derive(Deserialize)]
            struct R {
                tag_name: String,
                #[serde(default)]
                body: Option<String>,
                html_url: String,
                #[serde(default)]
                draft: bool,
                #[serde(default)]
                assets: Vec<A>,
            }
            #[derive(Deserialize)]
            struct A {
                name: String,
                id: u64,
            }
            let out = crate::collectors::github::gh_bytes(&["api".into(), format!("repos/{repo}/releases?per_page=50")]).await?;
            let list: Vec<R> = serde_json::from_slice(&out).context("GitHub's answer is not a list of releases")?;
            Ok(list
                .into_iter()
                .filter(|r| !r.draft)
                .filter_map(|r| {
                    let assets = r.assets.into_iter().map(|a| Asset { name: a.name, api: format!("repos/{repo}/releases/assets/{}", a.id), host: String::new() }).collect();
                    Some(Release { version: version(&r.tag_name)?, notes: r.body.unwrap_or_default(), url: r.html_url, tag: r.tag_name, assets })
                })
                .collect())
        }
    }
}

/// `glab api` on the release's instance.
async fn glab(api: &str, host: &str) -> Result<Vec<u8>> {
    let cli = crate::forge::cli::Cli::Glab;
    cli.output(cli.command_on(host), &["api", api]).await
}

/// A file of a release, as is.
async fn download(asset: &Asset) -> Result<Vec<u8>> {
    if asset.host.is_empty() {
        let octet = "Accept: application/octet-stream".to_string();
        crate::collectors::github::gh_bytes(&["api".into(), "-H".into(), octet, asset.api.clone()]).await
    } else {
        glab(&asset.api, &asset.host).await
    }
}

/// The file of this system among a release's, and how it installs: named for the release's own
/// version (an older version's file, signed as it is, is not that release's).
fn pick<'a>(version: &str, assets: &'a [Asset]) -> Option<(&'a Asset, &'static str)> {
    let prefix = format!("Prism_{version}_");
    let find = |suffix: &str| assets.iter().find(|a| a.name.starts_with(&prefix) && a.name.ends_with(suffix));
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" if cfg!(target_os = "linux") => "amd64",
        "x86_64" if cfg!(windows) => "x64",
        a => a,
    };
    if cfg!(windows) {
        return find(&format!("_windows-{arch}-setup.exe")).map(|a| (a, "replace"));
    }
    if cfg!(target_os = "macos") {
        return find("_macos-universal.app.tar.gz").or_else(|| find(&format!("_macos-{arch}.app.tar.gz"))).map(|a| (a, "replace"));
    }
    if cfg!(target_os = "linux") {
        if std::env::var_os("APPIMAGE").is_some() {
            return find(&format!("_{arch}.AppImage")).map(|a| (a, "replace"));
        }
        // Arch Linux and its derivatives: their package, else the .deb is of no use there.
        if Path::new("/usr/bin/pacman").exists() {
            return find(&format!("_{arch}.pkg.tar.zst")).map(|a| (a, "download"));
        }
        return find(&format!("_{arch}.deb")).map(|a| (a, "download"));
    }
    None
}

/// Downloads the newest release for this system and installs it: true when Prism must restart,
/// false when it was only downloaded (a .deb, an Arch package, revealed in the file manager).
pub async fn install(cfg: &crate::config::UpdatesConfig, tag: &str) -> Result<(bool, String)> {
    let src = source(&cfg.source)?;
    let release = releases(src).await?.into_iter().find(|r| r.tag == tag).with_context(|| tr!("no release {tag} in {source}", tag = tag, source = cfg.source.trim()))?;
    // Never back: an older Prism lacks the fixes since, its files signed all the same.
    ensure!(!crate::packs::satisfies(crate::packs::PRISM, &release.version), tr!("Prism {version} is not newer than this one ({current}): not installed", version = release.version, current = crate::packs::PRISM));
    let (asset, how) = pick(&release.version, &release.assets).context(tr("this release has no file for this system"))?;
    let bytes = download(asset).await.with_context(|| tr!("downloading {name}", name = asset.name))?;
    // Its signature, by the key built into this Prism: the file is the one published as this name.
    let sig = release.assets.iter().find(|a| a.name == format!("{}.minisig", asset.name)).with_context(|| tr!("{name}: this release is not signed (no .minisig): not installed", name = asset.name))?;
    let sig = String::from_utf8(download(sig).await?).context("the signature is not text")?;
    verify(UPDATE_KEY, &asset.name, &bytes, &sig)?;
    match how {
        "replace" if cfg!(target_os = "macos") => {
            replace_app(&bytes).await?;
            Ok((true, format!("Prism {} installed", release.version)))
        }
        "replace" if cfg!(windows) => {
            stage_installer(&asset.name, &bytes)?;
            Ok((true, format!("Prism {} downloaded: it installs as Prism restarts", release.version)))
        }
        "replace" => {
            replace_appimage(&bytes)?;
            Ok((true, format!("Prism {} installed", release.version)))
        }
        _ => {
            let dir = dirs::download_dir().or_else(dirs::home_dir).context(tr("no Downloads folder"))?;
            let file = dir.join(Path::new(&asset.name).file_name().context("the file's name")?);
            std::fs::write(&file, &bytes).with_context(|| format!("writing {}", file.display()))?;
            let _ = crate::platform::reveal(&file);
            let installer = if asset.name.ends_with(".pkg.tar.zst") { "sudo pacman -U" } else { "sudo apt install" };
            Ok((false, format!("{} downloaded: {installer} {}", asset.name, file.display())))
        }
    }
}

/// `bytes` were signed with `key` (a minisign public key file) as the file `name`: the trusted
/// comment release.sh signs along (`file:<name>`) keeps a signed file from passing for another,
/// an older version's for instance.
fn verify(key: &str, name: &str, bytes: &[u8], sig: &str) -> Result<()> {
    let line = key.lines().map(str::trim).rfind(|l| !l.is_empty() && !l.starts_with("untrusted comment:")).context(tr("this Prism has no update key: install from the release page"))?;
    let key = minisign_verify::PublicKey::from_base64(line).map_err(|e| anyhow!("this Prism's update key: {e}"))?;
    let sig = minisign_verify::Signature::decode(sig).map_err(|e| anyhow!("{name}: unreadable signature: {e}"))?;
    key.verify(bytes, &sig, false).map_err(|_| anyhow!(tr!("{name}: not signed with the key this Prism trusts: not installed", name = name)))?;
    ensure!(sig.trusted_comment() == format!("file:{name}"), "{name}: its signature is another file's ({}): not installed", sig.trusted_comment());
    Ok(())
}

/// The `Prism.app` this Prism runs from.
fn app_bundle() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("where Prism runs")?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf).context(tr("Prism does not run from an installed Prism.app (a development build?): install it from the release page"))
}

/// Swaps the running `Prism.app` for the release's: extracted next to it, its signature checked,
/// then renamed into place; the former one is put back if that fails.
async fn replace_app(archive: &[u8]) -> Result<()> {
    let app = app_bundle()?;
    let parent = app.parent().context("the app's folder")?;
    let work = parent.join(format!(".prism-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).with_context(|| tr!("{folder} is not writable: move Prism.app to a folder you can write, or install the .dmg", folder = parent.display()))?;
    let result = async {
        let tgz = work.join("Prism.app.tar.gz");
        std::fs::write(&tgz, archive)?;
        let run = |program: &str, args: &[&std::ffi::OsStr]| {
            let mut cmd = crate::platform::command(program);
            cmd.args(args).stdin(std::process::Stdio::null());
            cmd
        };
        let out = run("tar", &["-xzf".as_ref(), tgz.as_os_str(), "-C".as_ref(), work.as_os_str()]).output().await?;
        ensure!(out.status.success(), "tar: {}", String::from_utf8_lossy(&out.stderr).trim());
        let new = work.join("Prism.app");
        ensure!(new.join("Contents/MacOS").is_dir(), tr("the archive holds no Prism.app"));
        let out = run("codesign", &["--verify".as_ref(), "--deep".as_ref(), "--strict".as_ref(), new.as_os_str()]).output().await?;
        ensure!(out.status.success(), tr!("the new app's signature does not verify: {err}", err = String::from_utf8_lossy(&out.stderr).trim()));
        let old = work.join("Prism-previous.app");
        std::fs::rename(&app, &old).with_context(|| format!("moving {} aside", app.display()))?;
        if let Err(e) = std::fs::rename(&new, &app) {
            let _ = std::fs::rename(&old, &app);
            return Err(e).with_context(|| format!("putting the new app in {}", app.display()));
        }
        Ok(())
    }
    .await;
    crate::platform::remove_dirs_detached(&[work]);
    result
}

/// The installer of an update, verified, waiting for Prism to restart (`take_installer`).
static INSTALLER: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Writes the verified installer in a folder of Prism's for `take_installer`.
fn stage_installer(name: &str, bytes: &[u8]) -> Result<()> {
    let dir = std::env::temp_dir().join("prism-update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    let file = dir.join(Path::new(name).file_name().context("the file's name")?);
    std::fs::write(&file, bytes).with_context(|| format!("writing {}", file.display()))?;
    *INSTALLER.lock().unwrap() = Some(file);
    Ok(())
}

/// The installer `install` downloaded, to run in place of a plain restart: once.
pub fn take_installer() -> Option<PathBuf> {
    INSTALLER.lock().unwrap().take()
}

/// Runs an update's installer as Prism quits: without a question (`/P`, a progress bar), as an
/// update (`/UPDATE`: the shortcuts left as they are), the new Prism started once it is done
/// (`/R`). The installer closes what still runs from `prism.exe`; `prism sessions` runs from a
/// copy of its own, and the agents go on.
pub fn run_installer(setup: &Path) -> Result<()> {
    let mut cmd = std::process::Command::new(setup);
    cmd.args(["/P", "/UPDATE", "/R"]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd.spawn().with_context(|| format!("starting {}", setup.display()))?;
    Ok(())
}

/// Replaces the AppImage Prism runs from (`APPIMAGE`), through a file next to it.
fn replace_appimage(bytes: &[u8]) -> Result<()> {
    let path = PathBuf::from(std::env::var_os("APPIMAGE").context(tr("Prism does not run from an AppImage"))?);
    let next = path.with_extension("AppImage.new");
    std::fs::write(&next, bytes).with_context(|| format!("writing {}", next.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&next, &path).with_context(|| format!("replacing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_source() {
        assert_eq!(source("glab:acme/prism-pack").unwrap(), Source::Gitlab("acme/prism-pack"));
        assert_eq!(source(" gh:owner/prism ").unwrap(), Source::Github("owner/prism"));
        assert!(source("owner/prism").is_err() && source("gh:prism").is_err() && source("glab:g/").is_err());
    }

    fn release(version: &str) -> Release {
        Release { version: version.into(), tag: format!("{TAG}{version}"), notes: String::new(), url: String::new(), assets: Vec::new() }
    }

    #[test]
    fn the_newest_release_wins() {
        let r = newest(vec![release("0.1.0-a2"), release("0.2.0"), release("0.1.0"), release("0.2.0-b1")]).unwrap();
        assert_eq!(r.version, "0.2.0");
        assert!(newest(Vec::new()).is_none());
    }

    #[test]
    fn lists_what_an_update_brings() {
        let list = [release("0.1.0-a2"), release("0.2.0"), release("0.1.0"), release("0.1.0-a3")];
        let versions: Vec<String> = newer(&list, "0.1.0-a2").into_iter().map(|r| r.version).collect();
        assert_eq!(versions, ["0.2.0", "0.1.0", "0.1.0-a3"]);
        assert!(newer(&list, "0.2.0").is_empty());
    }

    /// A throwaway key (`minisign -G -W`), and its signatures of `prism` as two files.
    const KEY: &str = "untrusted comment: minisign public key 72D3D5E6BBBDAE1C\nRWQcrr275tXTchhvk0NV/w1Y4FDtYKLMHmM6TD/MyT9EARhHpu2ZtlT8\n";
    const NAME: &str = "Prism_9.9.9_macos-universal.app.tar.gz";
    const SIG: &str = "untrusted comment: signature from minisign secret key\nRUQcrr275tXTchrJaJceNqrTYv+KNW2ckAshvdrK5+K5S03UNe/j/pZ51Qb4ynOC7z+HmWaD1sOXhGPz/qqkty5ubjHwWzL6XQE=\ntrusted comment: file:Prism_9.9.9_macos-universal.app.tar.gz\najAFAdVu9HpCi2QIHE6GRNhoGhadiwKMa912j1FbXdP2y5T+/4/hjY+0Jln8/o/kUL+pt1wrQWLVIdhEAK2MDA==\n";
    const OLD: &str = "untrusted comment: signature from minisign secret key\nRUQcrr275tXTchrJaJceNqrTYv+KNW2ckAshvdrK5+K5S03UNe/j/pZ51Qb4ynOC7z+HmWaD1sOXhGPz/qqkty5ubjHwWzL6XQE=\ntrusted comment: file:Prism_0.0.1_macos-universal.app.tar.gz\n0nNb5lDQztdL/z0kXWauRdpeSXMD6TjnUKZpSRRX++1G0HuZrL/8P8LSyooOXSggozLQ1KMuq9gv9mftGqZ7Aw==\n";

    #[test]
    fn installs_only_what_its_key_signed() {
        verify(KEY, NAME, b"prism", SIG).unwrap();
        assert!(verify(KEY, NAME, b"prism!", SIG).unwrap_err().to_string().contains("not signed with the key"), "another file");
        assert!(verify(KEY, NAME, b"prism", OLD).unwrap_err().to_string().contains("another file's"), "a signed file passed for another");
        let other = "untrusted comment: minisign public key 0000\nRWQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n";
        assert!(verify(other, NAME, b"prism", SIG).is_err(), "another key");
        assert!(verify("", NAME, b"prism", SIG).unwrap_err().to_string().contains("no update key"));
        verify(UPDATE_KEY, NAME, b"", "").unwrap_err();
    }

    #[test]
    fn picks_the_file_of_this_system() {
        let asset = |name: &str| Asset { name: name.into(), api: String::new(), host: String::new() };
        let assets = [
            asset("Prism_0.2.0_macos-universal.dmg"),
            asset("Prism_0.2.0_macos-universal.app.tar.gz"),
            asset("Prism_0.2.0_macos-universal.app.tar.gz.sha256"),
            asset("Prism_0.2.0_amd64.deb"),
            asset("Prism_0.2.0_windows-x64-setup.exe"),
            asset("Prism_0.2.0_windows-x64-setup.exe.minisig"),
            asset("Prism_0.2.0_windows-arm64-setup.exe"),
        ];
        let picked = pick("0.2.0", &assets).map(|(a, how)| (a.name.as_str(), how));
        if cfg!(target_os = "macos") {
            assert_eq!(picked, Some(("Prism_0.2.0_macos-universal.app.tar.gz", "replace")));
        }
        if cfg!(all(windows, target_arch = "x86_64")) {
            assert_eq!(picked, Some(("Prism_0.2.0_windows-x64-setup.exe", "replace")));
        }
        if cfg!(all(windows, target_arch = "aarch64")) {
            assert_eq!(picked, Some(("Prism_0.2.0_windows-arm64-setup.exe", "replace")));
        }
        assert!(pick("9.0.0", &assets).is_none(), "an older version's files in a newer release");
    }
}
