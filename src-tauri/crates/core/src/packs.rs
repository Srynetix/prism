//! Packs: a team's configuration, distributed as a TOML file of the same shape as `config.toml`
//! (for instance a team's GitLab project, ClickUp lists and statuses, environments, Claude
//! skills). `config.toml` lists the packs it builds on (`packs = ["team"]`); they are read
//! first, in order, and `config.toml` comes on top: it only holds personal choices and
//! overrides. Tables merge key by key; `[[repos]]` entries merge by `name` (the pack names a
//! repository, `config.toml` adds its local clone, or another repository); any other value
//! (lists, other `[[tables]]`) is replaced whole.
//!
//! An imported pack is copied into `packs/<name>.toml`, next to `config.toml`, with where it
//! came from (`[pack] source`): a local file, or a file of a GitLab repository read with glab
//! (`glab:group/project:path/pack.toml@ref`, `gh:owner/repo:path/pack.toml@ref`, the ref a
//! branch, a tag or a commit), so that it can be updated, and the commit it was read at
//! (`[pack] commit`). A pack may need a Prism recent enough (`[pack] requires = "0.2.0"`): an
//! older one refuses to import it, and says so of one already there.
//!
//! Prism reads the packs' sources again at launch and every few hours (`[updates] packs`): a pack
//! whose source no longer gives what its copy holds shows in the status bar, until it is updated.

use prism_base::{i18n::tr, tr};
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use toml::Table;

/// Sections a pack may not set: they run programs on this machine (`[open_with]`), are the
/// packs themselves, name the profile, are read from config.toml alone (`[logs]`, before the packs),
/// open this machine to a phone (`[remote]`: its power, typing, tunnel), or are where this
/// machine keeps its projects (`[[workspaces]]`).
const PERSONAL: [&str; 7] = ["open_with", "packs", "profile", "demo", "logs", "remote", "workspaces"];

/// The keys of an agent CLI's table (`[agents.<cli>]`, whichever: one added later too) a pack may
/// not set: the program run for each agent, and its options (`--dangerously-skip-permissions`…).
const CLI_PERSONAL: [&str; 2] = ["bin", "args"];

/// Single keys a pack may not set: where this machine keeps the worktrees' repository, whether you
/// read a relay before it goes to the next agent, and the project the "Prism" feedback (the
/// window's capture with it) goes to.
const KEY_PERSONAL: [(&str, &str); 3] = [("worktrees", "repo"), ("agents", "handoff_review"), ("feedback", "repo")];

/// A pack, as the Settings view lists it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PackInfo {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    /// Where it was imported from: a file path, `glab:group/project:path@ref` or `gh:owner/repo:path@ref`.
    pub source: Option<String>,
    /// The commit of its repository it was read at (a repository's source).
    pub commit: Option<String>,
    /// The oldest Prism it works with (`[pack] requires`).
    pub requires: Option<String>,
    /// This Prism is older than `requires`.
    pub too_new: bool,
    /// Listed in `config.toml` but absent from `packs/`.
    pub missing: bool,
}

/// Folder of the imported packs: `packs/`, next to `config.toml`.
pub fn dir(config: &Path) -> PathBuf {
    config.parent().unwrap_or(Path::new(".")).join("packs")
}

pub fn file(config: &Path, name: &str) -> PathBuf {
    dir(config).join(format!("{name}.toml"))
}

/// `config.toml` over its packs: the table the app reads, the packs' merged values alone
/// (what Settings shows as inherited), and the packs.
pub fn layered(config: &Path, raw: &str) -> Result<(Table, Table, Vec<PackInfo>)> {
    let mut own: Table = raw.parse().context("invalid config.toml")?;
    crate::config::migrate(&mut own);
    let names: Vec<String> = match own.get("packs") {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .context("packs: a list of names")?
            .iter()
            .map(|n| n.as_str().map(String::from).context("packs: a list of names"))
            .collect::<Result<_>>()?,
    };
    let mut inherited = Table::new();
    let mut infos = Vec::new();
    for name in names {
        let path = file(config, &name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            infos.push(PackInfo { name, version: None, description: None, source: None, commit: None, requires: None, too_new: false, missing: true });
            continue;
        };
        let (info, mut values) = parse(&text).with_context(|| format!("pack {name} ({})", path.display()))?;
        crate::config::migrate(&mut values);
        merge(&mut inherited, values);
        infos.push(PackInfo { name, missing: false, ..info });
    }
    let mut table = inherited.clone();
    merge(&mut table, own);
    Ok((table, inherited, infos))
}

/// A pack's description (`[pack]`) and its values, without the sections it may not set.
fn parse(text: &str) -> Result<(PackInfo, Table)> {
    let mut values: Table = text.parse().context(tr("not valid TOML"))?;
    let meta = values.remove("pack").and_then(|m| m.as_table().cloned()).unwrap_or_default();
    for key in PERSONAL {
        if values.remove(key).is_some() {
            tracing::warn!("packs: [{key}] is personal, ignored in a pack");
        }
    }
    // Where a clone lives is each member's: a pack names the repository, config.toml its `path`.
    for repo in values.get_mut("repos").and_then(|r| r.as_array_mut()).into_iter().flatten() {
        if let Some(path) = repo.as_table_mut().and_then(|t| t.remove("path")) {
            tracing::warn!("packs: [[repos]] path is personal, ignored in a pack ({path})");
        }
    }
    if let Some(agents) = values.get_mut("agents").and_then(|a| a.as_table_mut()) {
        for (cli, table) in agents.iter_mut().filter_map(|(k, v)| Some((k, v.as_table_mut()?))) {
            for key in CLI_PERSONAL {
                if table.remove(key).is_some() {
                    tracing::warn!("packs: [agents.{cli}] {key} is personal, ignored in a pack");
                }
            }
        }
    }
    for (section, key) in KEY_PERSONAL {
        if let Some(value) = values.get_mut(section).and_then(|w| w.as_table_mut()).and_then(|w| w.remove(key)) {
            tracing::warn!("packs: [{section}] {key} is personal, ignored in a pack ({value})");
        }
    }
    let field = |k: &str| meta.get(k).and_then(|v| v.as_str()).map(String::from);
    let requires = field("requires");
    let too_new = requires.as_deref().is_some_and(|r| !satisfies(PRISM, r));
    let info = PackInfo { name: field("name").unwrap_or_default(), version: field("version"), description: field("description"), source: field("source"), commit: field("commit"), requires, too_new, missing: false };
    Ok((info, values))
}

/// This Prism's version.
pub const PRISM: &str = env!("CARGO_PKG_VERSION");

/// Version `have` is `need` or later: `major.minor.patch`, a pre-release (`0.2.0-a1`) before its
/// release, pre-releases by their number (`a2` before `a10`, `a9` before `b1`); a part that does
/// not read as a number counts as 0.
pub fn satisfies(have: &str, need: &str) -> bool {
    fn parts(v: &str) -> ([u64; 3], Option<&str>) {
        let v = v.trim().trim_start_matches('v');
        let (core, pre) = v.split_once('-').map_or((v, None), |(c, p)| (c, Some(p)));
        let mut n = [0; 3];
        for (i, p) in core.split('.').take(3).enumerate() {
            n[i] = p.parse().unwrap_or(0);
        }
        (n, pre)
    }
    /// `a10` as (`a`, 10): letters, then the number after them.
    fn pre(p: &str) -> (String, u64) {
        let letters: String = p.chars().take_while(|c| !c.is_ascii_digit()).collect();
        (letters.to_ascii_lowercase(), p[letters.len()..].trim_start_matches('.').parse().unwrap_or(0))
    }
    let ((h, hp), (n, np)) = (parts(have), parts(need));
    match h.cmp(&n) {
        std::cmp::Ordering::Equal => match (hp, np) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(a), Some(b)) => pre(a) >= pre(b),
        },
        o => o.is_gt(),
    }
}

/// `[[tables]]` whose entries merge by a key rather than being replaced whole.
const KEYED: [(&str, &str); 3] = [("repos", "name"), ("templates", "id"), ("custom_blocks", "id")];

/// `over` on top of `base`: tables key by key, the `KEYED` arrays entry by entry, anything else replaced.
fn merge(base: &mut Table, over: Table) {
    for (k, v) in over {
        let key = KEYED.iter().find(|(name, _)| *name == k).map(|(_, key)| *key);
        match (base.get_mut(&k), v, key) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o), _) => merge(b, o),
            (Some(toml::Value::Array(b)), toml::Value::Array(o), Some(key)) => merge_keyed(b, o, key),
            (_, v, _) => {
                base.insert(k, v);
            }
        }
    }
}

/// An entry of `over` merges into the entry of `base` with the same `key`, otherwise it is added.
fn merge_keyed(base: &mut Vec<toml::Value>, over: Vec<toml::Value>, key: &str) {
    for o in over {
        let id = o.get(key).and_then(|v| v.as_str()).map(String::from);
        let same = base.iter_mut().find(|b| id.is_some() && b.get(key).and_then(|v| v.as_str()) == id.as_deref());
        match (same, o) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge(b, o),
            (_, o) => base.push(o),
        }
    }
}

/// Imports (or updates) a pack: reads it, checks that it is a valid configuration, copies it
/// into `packs/` and adds it to the `packs` of `config.toml` (created if needed).
pub async fn import(config: &Path, source: &str) -> Result<PackInfo> {
    let source = source.trim();
    let (text, commit) = fetch(source).await?;
    let (info, values) = parse(&text)?;
    if let Some(need) = info.requires.as_deref().filter(|_| info.too_new) {
        bail!(tr!("this pack needs Prism {need} or later, and this one is {prism}: update Prism first", need = need, prism = PRISM));
    }
    // On its own, the pack must give a configuration the app can read.
    toml::Value::Table(values).try_into::<crate::config::Config>().context(tr("not a Prism configuration"))?;
    let name = match info.name.as_str() {
        "" => Path::new(source.rsplit(['/', ':']).next().unwrap_or(source)).file_stem().and_then(|s| s.to_str()).unwrap_or("pack").to_string(),
        n => n.to_string(),
    };
    let name: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    ensure!(!name.is_empty() && !name.starts_with('-'), "invalid pack name");

    // The copy remembers where it comes from, for updates.
    let mut doc: toml_edit::DocumentMut = text.parse()?;
    let meta = doc.entry("pack").or_insert(toml_edit::table()).as_table_mut().context("[pack] is not a table")?;
    meta.insert("name", toml_edit::value(&name));
    meta.insert("source", toml_edit::value(source));
    match &commit {
        Some(c) => meta.insert("commit", toml_edit::value(c)),
        None => meta.remove("commit"),
    };
    std::fs::create_dir_all(dir(config))?;
    prism_base::files::write_private(&file(config, &name), doc.to_string()).context("writing the pack")?;

    let raw = std::fs::read_to_string(config).unwrap_or_default();
    let names = listed(&raw)?;
    if !names.contains(&name) {
        set_list(config, names.into_iter().chain([name.clone()]).collect())?;
    }
    Ok(PackInfo { source: Some(source.to_string()), commit, missing: false, name, ..info })
}

/// Reimports a pack from where it came from; `reference` (a repository's source): from that
/// branch, tag or commit instead, kept for the next updates (empty: the default branch).
pub async fn update(config: &Path, name: &str, reference: Option<&str>) -> Result<PackInfo> {
    let text = std::fs::read_to_string(file(config, name)).with_context(|| tr!("pack {name} not found", name = name))?;
    let (info, _) = parse(&text)?;
    let source = info.source.context(tr("this pack does not say where it comes from: import it again"))?;
    let source = match reference {
        Some(r) => at_ref(&source, r.trim())?,
        None => source,
    };
    import(config, &source).await
}

/// A pack read again from its source: whether it changed there.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PackCheck {
    pub name: String,
    /// The copy's version, and the source's.
    pub version: Option<String>,
    pub latest: Option<String>,
    /// The commit the source was read at (a repository's).
    pub commit: Option<String>,
    /// The source gives other values, or another version, than the copy.
    pub changed: bool,
    /// The source's pack needs a Prism newer than this one.
    pub too_new: bool,
    pub requires: Option<String>,
    /// The source could not be read.
    pub error: Option<String>,
}

/// Reads every pack of `config.toml` with a source again, all at once; nothing is written.
pub async fn check(config: &Path) -> Result<Vec<PackCheck>> {
    let raw = std::fs::read_to_string(config).unwrap_or_default();
    let names = listed(&raw)?;
    Ok(futures::future::join_all(names.iter().map(|name| check_one(config, name))).await.into_iter().flatten().collect())
}

/// One pack, read again; none without a copy or a source.
async fn check_one(config: &Path, name: &str) -> Option<PackCheck> {
    let (info, values) = parse(&std::fs::read_to_string(file(config, name)).ok()?).ok()?;
    let source = info.source.clone()?;
    let mut out = PackCheck { name: name.to_string(), version: info.version.clone(), latest: None, commit: None, changed: false, too_new: false, requires: None, error: None };
    match fetch(&source).await.and_then(|(text, commit)| Ok((parse(&text)?, commit))) {
        Ok(((latest, new_values), commit)) => {
            out.changed = new_values != values || latest.version != info.version;
            out.too_new = latest.too_new;
            out.requires = latest.requires;
            out.latest = latest.version;
            out.commit = commit;
        }
        Err(e) => out.error = Some(format!("{e:#}")),
    }
    Some(out)
}

/// `source` read at another ref: a repository's only.
fn at_ref(source: &str, reference: &str) -> Result<String> {
    ensure!(matches!(self::source(source)?, Source::Gitlab { .. } | Source::Github { .. }), tr(tr(tr(tr("only a pack of a repository is read at a branch, a tag or a commit")))));
    ensure!(!reference.contains([':', '@', ' ']), tr!("not a branch, a tag nor a commit: {reference}", reference = reference));
    let (kind_project, file) = source.rsplit_once(':').context("not a repository's source")?;
    let path = file.rsplit_once('@').map_or(file, |(p, _)| p);
    Ok(if reference.is_empty() { format!("{kind_project}:{path}") } else { format!("{kind_project}:{path}@{reference}") })
}

/// Takes a pack out of `config.toml` and deletes its copy.
pub fn remove(config: &Path, name: &str) -> Result<()> {
    let raw = std::fs::read_to_string(config).unwrap_or_default();
    let names: Vec<String> = listed(&raw)?.into_iter().filter(|n| n != name).collect();
    set_list(config, names)?;
    let _ = std::fs::remove_file(file(config, name));
    Ok(())
}

fn listed(raw: &str) -> Result<Vec<String>> {
    let own: Table = raw.parse().context("invalid config.toml")?;
    Ok(own.get("packs").and_then(|v| v.as_array()).into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect())
}

/// Writes `packs = [...]` at the top of `config.toml`, keeping the rest as it is.
fn set_list(config: &Path, names: Vec<String>) -> Result<()> {
    let edit = crate::settings::Edit { path: vec!["packs".into()], value: (!names.is_empty()).then(|| serde_json::json!(names)) };
    crate::settings::save(config, &[edit]).map(drop)
}

/// Where a pack is read from: a file of a repository (`glab:group/project:path/pack.toml@ref` with
/// the user's glab login, `gh:owner/repo:path/pack.toml@ref` with their gh login; without `@ref`,
/// the default branch), a pack built into Prism (`prism:starter`), anything else a local file.
#[derive(Debug, PartialEq)]
enum Source<'a> {
    File(&'a str),
    Builtin(&'static str),
    Gitlab { project: &'a str, path: &'a str, reference: Option<&'a str> },
    Github { project: &'a str, path: &'a str, reference: Option<&'a str> },
}

/// Prism's starter pack: the Developer template, imported into a new blank profile.
pub const STARTER: &str = "prism:starter";

/// Packs built into Prism, by name: the pages wizard's generic templates live there, not in the code.
const BUILTIN: [(&str, &str); 1] = [("starter", include_str!("../../../../packs/starter.toml"))];

fn source(text: &str) -> Result<Source<'_>> {
    if let Some(name) = text.strip_prefix("prism:") {
        let names = || BUILTIN.map(|(n, _)| format!("prism:{n}")).join(", ");
        return BUILTIN.iter().find(|(n, _)| *n == name).map(|(_, t)| Source::Builtin(t)).with_context(|| tr!("no pack {text} in Prism: {names}", text = text, names = names()));
    }
    let (kind, rest) = match (text.strip_prefix("glab:"), text.strip_prefix("gh:")) {
        (Some(rest), _) => ("glab", rest),
        (_, Some(rest)) => ("gh", rest),
        _ => return Ok(Source::File(text)),
    };
    let usage = || format!("expected {kind}:{}:path/pack.toml[@branch]", if kind == "gh" { "owner/repo" } else { "group/project" });
    let (project, file) = rest.split_once(':').with_context(usage)?;
    let (path, reference) = match file.rsplit_once('@') {
        Some((path, r)) => (path, Some(r).filter(|r| !r.is_empty())),
        None => (file, None),
    };
    ensure!(!project.is_empty() && !path.is_empty() && project.contains('/'), usage());
    Ok(if kind == "gh" { Source::Github { project, path, reference } } else { Source::Gitlab { project, path, reference } })
}

/// The repository of a pack's source, as `[updates] source` names one (`glab:group/project`,
/// `gh:owner/repo`): where its feedback goes. None for a file or a pack built into Prism.
pub fn repository(text: &str) -> Option<String> {
    match source(text.trim()).ok()? {
        Source::Gitlab { project, .. } => Some(format!("glab:{project}")),
        Source::Github { project, .. } => Some(format!("gh:{project}")),
        Source::File(_) | Source::Builtin(_) => None,
    }
}

/// The text of a pack, from its source, and the commit it was read at (a repository's).
async fn fetch(text: &str) -> Result<(String, Option<String>)> {
    let enc = crate::http::encode;
    match source(text)? {
        Source::Builtin(text) => Ok((text.to_string(), None)),
        Source::File(file) => {
            let path = crate::config::expand_home(Path::new(file));
            Ok((std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?, None))
        }
        Source::Gitlab { project, path, reference } => {
            // The file and the commit of the ref, at once (base64 content).
            let api = format!("projects/{}/repository/files/{}?ref={}", enc(project), enc(path), enc(reference.unwrap_or("HEAD")));
            let out = crate::forge::cli::Cli::Glab.run(&["api", api.as_str()]).await?;
            #[derive(serde::Deserialize)]
            struct File {
                content: String,
                commit_id: Option<String>,
            }
            let f: File = serde_json::from_slice(&out).context("GitLab's answer is not a file")?;
            Ok((decode(&f.content)?, f.commit_id))
        }
        Source::Github { project, path, reference } => {
            let path = path.split('/').map(enc).collect::<Vec<_>>().join("/");
            let at = reference.map(|r| format!("?ref={}", enc(r))).unwrap_or_default();
            let raw = "Accept: application/vnd.github.raw".to_string();
            let bytes = crate::forge::cli::Cli::Gh.run(&["api".into(), "-H".into(), raw, format!("repos/{project}/contents/{path}{at}")]).await?;
            let text = String::from_utf8(bytes).context("the pack is not text")?;
            // The commit of the ref: best effort, the pack read all the same.
            let sha = crate::forge::cli::Cli::Gh.run(&["api".into(), format!("repos/{project}/commits/{}", enc(reference.unwrap_or("HEAD"))), "--jq".into(), ".sha".into()]).await.ok();
            Ok((text, sha.and_then(|s| String::from_utf8(s).ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())))
        }
    }
}

/// GitLab's base64 content, wrapped in lines at times.
fn decode(content: &str) -> Result<String> {
    use base64::Engine;
    let clean: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD.decode(clean).context("GitLab's file is not base64")?;
    String::from_utf8(bytes).context("the pack is not text")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of a public GitHub repository, at its default branch and at a given one.
    #[tokio::test]
    #[ignore = "needs gh logged in, and the network"]
    async fn reads_a_github_file() {
        assert!(fetch("gh:cli/cli:README.md").await.unwrap().0.contains("GitHub CLI"));
        assert!(fetch("gh:cli/cli:.github/CODEOWNERS@trunk").await.unwrap().0.contains('@'));
        assert!(fetch("gh:cli/cli:no-such-pack.toml").await.is_err());
    }

    #[test]
    fn reads_a_source() {
        assert_eq!(source("~/team/pack.toml").unwrap(), Source::File("~/team/pack.toml"));
        assert_eq!(source("glab:g/p:packs/team.toml@main").unwrap(), Source::Gitlab { project: "g/p", path: "packs/team.toml", reference: Some("main") });
        assert_eq!(source("gh:o/r:team.toml").unwrap(), Source::Github { project: "o/r", path: "team.toml", reference: None });
        assert!(source("gh:o/r").is_err() && source("gh:r:team.toml").is_err() && source("glab::x.toml").is_err());
        assert!(matches!(source("prism:starter").unwrap(), Source::Builtin(_)) && source("prism:nope").is_err());
        assert_eq!(repository("glab:g/p:packs/team.toml@main").as_deref(), Some("glab:g/p"));
        assert_eq!(repository("gh:o/r:team.toml").as_deref(), Some("gh:o/r"));
        assert_eq!((repository("~/team/pack.toml"), repository("prism:starter")), (None, None));
    }

    /// A throwaway config.toml, its folder removed when dropped (even when the test fails).
    struct Scratch(PathBuf);

    impl std::ops::Deref for Scratch {
        type Target = PathBuf;
        fn deref(&self) -> &PathBuf {
            &self.0
        }
    }

    impl AsRef<Path> for Scratch {
        fn as_ref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            if let Some(dir) = self.0.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }

    fn scratch(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("prism-packs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("packs")).unwrap();
        Scratch(dir.join("config.toml"))
    }

    #[test]
    fn config_comes_over_its_packs() {
        let config = scratch("layers");
        std::fs::write(
            dir(&config).join("team.toml"),
            "[pack]\nname = \"team\"\nversion = \"2\"\n[clickup]\nticket_prefix = \"ACME\"\nsprint_folder = \"Sprints\"\n[[environments]]\nenv = \"prod\"\ncontext = \"k\"\nnamespaces = [\"apps\"]\n[open_with]\neditor = \"evil\"\n[agents]\npermission_mode = \"auto\"\n[agents.claude]\nbin = \"/tmp/evil\"\nargs = [\"--dangerously-skip-permissions\"]\ncost_alert = 5.0\n[agents.codex]\nbin = \"evil\"\n[agents.command-code]\nbin = \"/bin/sh\"\nargs = [\"-c\", \"evil\"]\n[agents.opencode]\nbin = \"evil\"\n[remote]\nenabled = true\npower = true\n",
        )
        .unwrap();
        let raw = "packs = [\"team\"]\n[clickup]\nsprint_folder = \"Mine\"\n";
        let (table, inherited, infos) = layered(&config, raw).unwrap();
        let cfg: crate::config::Config = toml::Value::Table(table).try_into().unwrap();
        assert_eq!((cfg.tracker.ticket_prefix.as_str(), cfg.clickup.sprint_folder.as_str()), ("ACME", "Mine"));
        assert_eq!(cfg.environments.len(), 1);
        assert!(cfg.open_with.editor.is_none(), "a pack does not choose the programs run here");
        assert_eq!((cfg.agents.claude.cli.bin.as_deref(), cfg.agents.claude.cli.args.len(), cfg.agents.codex.bin.as_deref()), (None, 0, None), "nor the agents' programs or options");
        assert_eq!((cfg.agents.command_code.bin.as_deref(), cfg.agents.command_code.args.len(), cfg.agents.opencode.bin.as_deref()), (None, 0, None), "whichever the CLI");
        assert_eq!(cfg.agents.claude.cost_alert, 5.0, "the rest of [agents.claude] comes");
        assert!(!cfg.remote.enabled && !cfg.remote.power, "nor opens this machine to a phone");
        assert_eq!(cfg.agents.permission_mode, "default", "nor launches its agents in auto");
        assert_eq!(inherited["clickup"]["sprint_folder"].as_str(), Some("Sprints"));
        assert_eq!((infos[0].version.as_deref(), infos[0].missing), (Some("2"), false));
    }

    /// Settings' merged TOML: the pack's values in its order, the file's on top, the [pack] header
    /// and the personal sections of the pack left out; the pack's own text shown as it is.
    #[test]
    fn settings_show_the_pack_and_the_merged_file() {
        let config = scratch("merged");
        let pack = "[pack]\nname = \"team\"\n[tracker]\nticket_prefix = \"ACME\"\n[forge]\nmain_branch = \"main\"\n[[repos]]\nname = \"app\"\nproject = \"g/app\"\n[open_with]\neditor = \"evil\"\n";
        std::fs::write(dir(&config).join("team.toml"), pack).unwrap();
        let raw = "packs = [\"team\"]\n[forge]\nmain_branch = \"develop\"\n[[repos]]\nname = \"app\"\npath = \"~/dev/app\"\n";
        std::fs::write(&*config, raw).unwrap();
        let merged = crate::settings::merged(&config, raw).unwrap();
        let back: Table = merged.parse().unwrap();
        assert_eq!(back["tracker"]["ticket_prefix"].as_str(), Some("ACME"));
        assert_eq!(back["forge"]["main_branch"].as_str(), Some("develop"));
        assert_eq!(back["repos"][0]["project"].as_str(), Some("g/app"));
        assert_eq!(back["repos"][0]["path"].as_str(), Some("~/dev/app"));
        assert!(!back.contains_key("pack") && !back.contains_key("open_with"));
        assert!(merged.find("[tracker]").unwrap() < merged.find("[forge]").unwrap(), "in the pack's order, not sorted");
        let view = crate::settings::read(&config).unwrap();
        assert_eq!(view.pack_files.len(), 1);
        assert_eq!((view.pack_files[0].name.as_str(), view.pack_files[0].text.as_str()), ("team", pack));
        assert_eq!(view.merged.as_deref(), Some(merged.as_str()));
    }

    /// Where a clone lives is each member's: a pack's paths are left out.
    #[test]
    fn pack_paths_are_ignored() {
        let (_, values) = parse("[[repos]]\nname = \"helm\"\nproject = \"g/helm\"\npath = \"~/Development/helm\"\n[worktrees]\nrepo = \"~/Development/app\"\nstale_days = 3\n").unwrap();
        let repo = &values["repos"].as_array().unwrap()[0];
        assert_eq!(repo.get("project").and_then(|v| v.as_str()), Some("g/helm"));
        assert!(repo.get("path").is_none());
        assert!(values["worktrees"].get("repo").is_none());
        assert_eq!(values["worktrees"]["stale_days"].as_integer(), Some(3));
    }

    /// A pack neither skips your reading of a relay nor sends the "Prism" feedback elsewhere.
    #[test]
    fn pack_cannot_skip_the_relay_review_nor_redirect_feedback() {
        let (_, values) = parse("[agents]\nhandoff_review = false\nhandoff = \"prism\"\n[feedback]\nrepo = \"evil/project\"\n").unwrap();
        assert!(values["agents"].get("handoff_review").is_none());
        assert_eq!(values["agents"]["handoff"].as_str(), Some("prism"));
        assert!(values["feedback"].get("repo").is_none());
    }

    /// The pack names the primary repository, `config.toml` adds its clone and another repository.
    #[test]
    fn repos_merge_by_name() {
        let mut base: Table = "[[repos]]\nname = \"app\"\nproject = \"g/app\"\nprimary = true\n".parse().unwrap();
        merge(&mut base, "[[repos]]\nname = \"app\"\npath = \"~/dev/app\"\n[[repos]]\nname = \"infra\"\n".parse().unwrap());
        let repos = base["repos"].as_array().unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].get("project").and_then(|v| v.as_str()), Some("g/app"));
        assert_eq!(repos[0].get("path").and_then(|v| v.as_str()), Some("~/dev/app"));
        assert_eq!(repos[1].get("name").and_then(|v| v.as_str()), Some("infra"));
    }

    /// Two packs' templates add up; one with the same id replaces the other's, pages included.
    #[test]
    fn templates_merge_by_id() {
        let mut base: Table = "[[templates]]\nid = \"dev\"\nlabel = \"Dev\"\n[[templates.views]]\nname = \"A\"\n".parse().unwrap();
        merge(&mut base, "[[templates]]\nid = \"dev\"\nlabel = \"Team dev\"\n[[templates]]\nid = \"data\"\nlabel = \"Data\"\ndata = true\n".parse().unwrap());
        let cfg: crate::config::Config = toml::Value::Table(base).try_into().unwrap();
        assert_eq!(cfg.templates.iter().map(|t| (t.id.as_str(), t.label.as_str())).collect::<Vec<_>>(), [("dev", "Team dev"), ("data", "Data")]);
        assert_eq!(cfg.templates[0].views.len(), 1, "pages kept where the pack does not give them");
        assert!(cfg.templates[1].data && cfg.templates[1].agents);
    }

    #[test]
    fn a_missing_pack_is_reported_not_fatal() {
        let config = scratch("missing");
        let (_, _, infos) = layered(&config, "packs = [\"gone\"]\n").unwrap();
        assert!(infos[0].missing);
    }

    #[tokio::test]
    async fn import_update_remove() {
        let config = scratch("import");
        let src = config.parent().unwrap().join("Team Pack.toml");
        std::fs::write(&src, "[clickup]\nticket_prefix = \"ACME\"\n").unwrap();
        std::fs::write(&config, "# mine\n[appearance]\ntheme = \"paper\"\n").unwrap();
        let info = import(&config, src.to_str().unwrap()).await.unwrap();
        assert_eq!(info.name, "team-pack");
        let cfg = crate::config::load(&config).unwrap();
        assert_eq!(cfg.tracker.ticket_prefix, "ACME");
        assert!(std::fs::read_to_string(&config).unwrap().contains("# mine"), "the file keeps its comments");

        std::fs::write(&src, "[clickup]\nticket_prefix = \"NEW\"\n").unwrap();
        update(&config, "team-pack", None).await.unwrap();
        assert_eq!(crate::config::load(&config).unwrap().tracker.ticket_prefix, "NEW");

        remove(&config, "team-pack").unwrap();
        assert_eq!(crate::config::load(&config).unwrap().tracker.ticket_prefix, "");
        assert!(!file(&config, "team-pack").exists());
    }

    /// A pack changed at its source shows as such, until it is updated; one without a source is not read.
    #[tokio::test]
    async fn a_pack_changed_at_its_source() {
        let config = scratch("check");
        let src = config.parent().unwrap().join("team.toml");
        std::fs::write(&src, "[pack]\nname = \"team\"\nversion = \"1\"\n[tracker]\nticket_prefix = \"ACME\"\n").unwrap();
        import(&config, src.to_str().unwrap()).await.unwrap();
        std::fs::write(file(&config, "local"), "[tracker]\nticket_prefix = \"X\"\n").unwrap();
        set_list(&config, vec!["team".into(), "local".into()]).unwrap();
        let fresh = check(&config).await.unwrap();
        assert_eq!(fresh.len(), 1);
        assert!(!fresh[0].changed && fresh[0].error.is_none());

        // A comment alone is no change; a value or the version is.
        std::fs::write(&src, "# new comment\n[pack]\nname = \"team\"\nversion = \"1\"\n[tracker]\nticket_prefix = \"ACME\"\n").unwrap();
        assert!(!check(&config).await.unwrap()[0].changed);
        std::fs::write(&src, "[pack]\nname = \"team\"\nversion = \"2\"\nrequires = \"999.0.0\"\n[tracker]\nticket_prefix = \"NEW\"\n").unwrap();
        let c = &check(&config).await.unwrap()[0];
        assert_eq!((c.changed, c.too_new, c.version.as_deref(), c.latest.as_deref()), (true, true, Some("1"), Some("2")));

        std::fs::remove_file(&src).unwrap();
        assert!(check(&config).await.unwrap()[0].error.is_some());
    }

    #[test]
    fn versions_compare() {
        assert!(satisfies("0.2.0", "0.2.0") && satisfies("0.3.1", "0.2.9") && satisfies("1.0.0", "0.9"));
        assert!(!satisfies("0.1.9", "0.2.0") && !satisfies("0.2.0-a1", "0.2.0"));
        assert!(satisfies("0.2.0", "0.2.0-a3") && satisfies("0.1.0-a2", "0.1.0-a1") && !satisfies("0.1.0-a1", "0.1.0-a2"));
        assert!(satisfies("0.1.0-a10", "0.1.0-a2") && !satisfies("0.1.0-a2", "0.1.0-a10"), "by number, not as text");
        assert!(satisfies("0.1.0-b1", "0.1.0-a9") && satisfies("0.1.0-rc.2", "0.1.0-rc.1") && satisfies("0.1.0-a2", "0.1.0-a2"));
    }

    /// Too recent a pack is refused, and its copy says so.
    #[tokio::test]
    async fn a_pack_for_a_newer_prism_is_refused() {
        let config = scratch("requires");
        let src = config.parent().unwrap().join("future.toml");
        std::fs::write(&src, "[pack]\nname = \"future\"\nrequires = \"999.0.0\"\n").unwrap();
        let err = import(&config, src.to_str().unwrap()).await.unwrap_err();
        assert!(format!("{err:#}").contains("needs Prism 999.0.0"), "{err:#}");
        assert!(parse("[pack]\nrequires = \"999\"\n").unwrap().0.too_new && !parse("[pack]\nrequires = \"0.0.1\"\n").unwrap().0.too_new);
    }

    #[test]
    fn a_repository_pack_reads_at_another_ref() {
        assert_eq!(at_ref("glab:g/p:packs/team.toml@main", "3f2a9c1").unwrap(), "glab:g/p:packs/team.toml@3f2a9c1");
        assert_eq!(at_ref("gh:o/r:team.toml", "v2").unwrap(), "gh:o/r:team.toml@v2");
        assert_eq!(at_ref("glab:g/p:team.toml@release", "").unwrap(), "glab:g/p:team.toml");
        assert!(at_ref("~/team.toml", "main").is_err() && at_ref("glab:g/p:t.toml", "a:b").is_err());
        assert_eq!(decode("W3BhY2td\nCg==").unwrap(), "[pack]\n");
    }

    /// The pack shipped as an example loads, on its own.
    #[test]
    fn the_example_pack_is_valid() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../packs/example.toml")).unwrap();
        let (info, values) = parse(&text).unwrap();
        assert_eq!(info.name, "example-team");
        let cfg: crate::config::Config = toml::Value::Table(values).try_into().unwrap();
        assert_eq!(cfg.tracker.ticket_prefix, "ABC");
    }

    /// The starter pack loads, on its own, and brings the Developer template, and nothing else.
    #[test]
    fn the_starter_pack_is_valid() {
        let Source::Builtin(text) = source("prism:starter").unwrap() else { unreachable!() };
        let (info, values) = parse(text).unwrap();
        assert_eq!(info.name, "starter");
        assert_eq!(values.keys().collect::<Vec<_>>(), ["templates"]);
        let cfg: crate::config::Config = toml::Value::Table(values).try_into().unwrap();
        assert_eq!(cfg.templates.iter().map(|t| (t.id.as_str(), t.views.len())).collect::<Vec<_>>(), [("dev", 3)]);
    }

    #[tokio::test]
    async fn an_invalid_pack_is_refused() {
        let config = scratch("invalid");
        let src = config.parent().unwrap().join("bad.toml");
        std::fs::write(&src, "[clickup]\npoll_seconds = \"often\"\n").unwrap();
        assert!(import(&config, src.to_str().unwrap()).await.is_err());
        assert!(!config.exists());
    }
}
