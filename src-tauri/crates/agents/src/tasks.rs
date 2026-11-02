//! The Tasks panel of an agent: the commands its folder declares — `just` recipes, `task`
//! tasks, `package.json` scripts — in the folder itself and in its subfolders down to
//! `[agents] tasks_depth` (a monorepo's `web/`, `apps/api/`). Each comes with the shell line that
//! runs it from the agent's folder.

use std::path::Path;

use anyhow::Result;
use serde::{de, Deserialize, Serialize};

/// One file declaring tasks.
#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "TaskSource"))]
pub struct Source {
    /// `just`, `task` or `npm` (whatever the package manager).
    #[cfg_attr(feature = "ts", ts(type = "'just' | 'task' | 'npm'"))]
    pub kind: &'static str,
    /// Its path, relative to the agent's folder.
    pub file: String,
    /// The program that runs its tasks: `just`, `task`, `pnpm`, `npm`, `yarn`, `bun`.
    pub runner: String,
    pub tasks: Vec<Task>,
    /// Why the list may be incomplete: the runner missing or refusing the file (then read by Prism).
    pub note: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentTask"))]
pub struct Task {
    pub name: String,
    /// Its doc comment, `desc`, or the script itself.
    pub desc: Option<String>,
    /// The shell line, from the agent's folder.
    pub command: String,
    /// It needs arguments: the line is typed into the shell, left for you to finish.
    pub args: bool,
}

/// Folders never searched: dependencies, builds, caches.
const SKIPPED: [&str; 6] = ["node_modules", "target", "vendor", "dist", "build", "out"];
/// Folders searched at most: a deep tree is cut there rather than read for seconds.
const MAX_DIRS: usize = 2000;

/// The task files of `root` and of its subfolders down to `depth` levels (0: the root alone), the
/// root's first, then each folder followed by its own subfolders, by name.
pub async fn detect(root: &Path, depth: u8) -> Result<Vec<Source>> {
    // A folder gone (worktree removed) is an error, not an empty list.
    std::fs::read_dir(root)?;
    let dirs = folders(root, depth);
    let mut out = Vec::new();
    for rel in &dirs {
        let rel = rel.as_str();
        // The workspace's package manager: the closest lockfile above it, otherwise any found.
        let nearby = Path::new(rel).ancestors().find_map(|a| locked(&root.join(a))).or_else(|| dirs.iter().find_map(|d| locked(&root.join(d))));
        let dir = root.join(rel);
        let names: Vec<String> = std::fs::read_dir(&dir).map(|d| d.flatten().filter_map(|e| e.file_name().to_str().map(String::from)).collect()).unwrap_or_default();
        let at = |name: &str| if rel.is_empty() { name.to_string() } else { format!("{rel}/{name}") };
        if let Some(name) = names.iter().find(|n| matches!(n.to_lowercase().as_str(), "justfile" | ".justfile")) {
            out.push(just(&dir, &at(name), rel).await);
        }
        if let Some(name) = TASKFILES.iter().find(|n| names.iter().any(|x| x == *n)) {
            out.push(taskfile(&dir, &at(name), rel).await);
        }
        if names.iter().any(|n| n == "package.json")
            && let Some(s) = package(&dir, &at("package.json"), rel, nearby)
        {
            out.push(s);
        }
    }
    Ok(out.into_iter().filter(|s| !s.tasks.is_empty() || s.note.is_some()).collect())
}

/// `root`'s folders down to `depth` levels, relative (the root: empty), depth first and by name;
/// hidden folders, dependencies and builds left out, symbolic links not followed.
fn folders(root: &Path, depth: u8) -> Vec<String> {
    fn walk(root: &Path, rel: &str, left: u8, out: &mut Vec<String>) {
        out.push(rel.to_string());
        if left == 0 || out.len() >= MAX_DIRS {
            return;
        }
        let Ok(entries) = std::fs::read_dir(root.join(rel)) else { return };
        let mut subs: Vec<String> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .filter(|n| !n.starts_with('.') && !SKIPPED.contains(&n.as_str()))
            .collect();
        subs.sort();
        for name in subs {
            if out.len() >= MAX_DIRS {
                return;
            }
            walk(root, &if rel.is_empty() { name } else { format!("{rel}/{name}") }, left - 1, out);
        }
    }
    let mut out = Vec::new();
    walk(root, "", depth, &mut out);
    out
}

/// Shell word: as is when it is plain, otherwise single-quoted.
fn quote(word: &str) -> String {
    if !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || "_-.:/@+=,".contains(c)) {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// Runs a runner's listing in `dir`; its output, or why there is none.
async fn listing(program: &str, args: &[&str], dir: &Path) -> std::result::Result<String, String> {
    let out = crate::platform::command(program).args(args).current_dir(dir).output().await.map_err(|_| format!("{program} not found ({})", crate::platform::install_hint(program)))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{program}: {}", err.lines().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/* ---------- just ---------- */

#[derive(Deserialize)]
struct JustDump {
    recipes: std::collections::BTreeMap<String, JustRecipe>,
}
#[derive(Deserialize)]
struct JustRecipe {
    doc: Option<String>,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    parameters: Vec<JustParam>,
}
#[derive(Deserialize)]
struct JustParam {
    default: Option<serde_json::Value>,
    #[serde(default)]
    kind: String,
}

async fn just(dir: &Path, file: &str, rel: &str) -> Source {
    let name = Path::new(file).file_name().and_then(|n| n.to_str()).unwrap_or("justfile").to_string();
    let base = if rel.is_empty() { "just".to_string() } else { format!("just --justfile {}", quote(file)) };
    let line = |recipe: &str| format!("{base} {}", quote(recipe));
    let (tasks, note) = match listing("just", &["--justfile", &name, "--dump", "--dump-format", "json"], dir).await {
        Ok(json) => match serde_json::from_str::<JustDump>(&json) {
            Ok(dump) => {
                let tasks = dump
                    .recipes
                    .into_iter()
                    .filter(|(_, r)| !r.private)
                    .map(|(n, r)| Task { command: line(&n), args: r.parameters.iter().any(|p| p.default.is_none() && p.kind != "star"), name: n, desc: r.doc })
                    .collect();
                (tasks, None)
            }
            Err(e) => (Vec::new(), Some(format!("just: unexpected listing ({e})"))),
        },
        Err(e) => (just_text(&std::fs::read_to_string(dir.join(&name)).unwrap_or_default(), line), Some(e)),
    };
    Source { kind: "just", file: file.to_string(), runner: "just".into(), tasks, note }
}

/// A justfile read by Prism, without `just`: its recipe lines, and the comment just above each.
fn just_text(text: &str, line: impl Fn(&str) -> String) -> Vec<Task> {
    let recipe = regex::Regex::new(r"^@?([A-Za-z0-9][A-Za-z0-9_-]*)([^:]*):([^=]|$)").unwrap();
    let mut doc: Option<String> = None;
    let mut private = false;
    let mut out = Vec::new();
    for l in text.lines() {
        if let Some(c) = l.strip_prefix('#') {
            doc = (!c.starts_with('!')).then(|| c.trim().to_string());
            continue;
        }
        // Attributes, between the comment and the recipe: `[private]` hides it.
        if l.starts_with('[') {
            private |= l.contains("private");
            continue;
        }
        if let Some(m) = recipe.captures(l) {
            let params = m[2].split_whitespace().filter(|p| !p.starts_with('*'));
            if !std::mem::take(&mut private) {
                out.push(Task { name: m[1].to_string(), desc: doc.take(), command: line(&m[1]), args: params.into_iter().any(|p| !p.contains('=')) });
            }
        }
        if !l.starts_with([' ', '\t']) {
            doc = None;
            private = false;
        }
    }
    out
}

/* ---------- task ---------- */

const TASKFILES: [&str; 8] = ["Taskfile.yml", "taskfile.yml", "Taskfile.yaml", "taskfile.yaml", "Taskfile.dist.yml", "taskfile.dist.yml", "Taskfile.dist.yaml", "taskfile.dist.yaml"];

#[derive(Deserialize)]
struct TaskList {
    tasks: Vec<TaskEntry>,
}
#[derive(Deserialize)]
struct TaskEntry {
    name: String,
    #[serde(default)]
    desc: String,
    location: Option<TaskLocation>,
}
#[derive(Deserialize)]
struct TaskLocation {
    #[serde(default)]
    line: u32,
}

async fn taskfile(dir: &Path, file: &str, rel: &str) -> Source {
    let base = if rel.is_empty() { "task".to_string() } else { format!("task -d {}", quote(rel)) };
    let line = |name: &str| format!("{base} {}", quote(name));
    let (tasks, note) = match listing("task", &["--list-all", "--json"], dir).await {
        Ok(json) => match serde_json::from_str::<TaskList>(&json) {
            Ok(mut list) => {
                // In the file's order rather than alphabetical.
                list.tasks.sort_by_key(|t| t.location.as_ref().map_or(u32::MAX, |l| l.line));
                (list.tasks.into_iter().map(|t| Task { command: line(&t.name), desc: Some(t.desc).filter(|d| !d.is_empty()), name: t.name, args: false }).collect(), None)
            }
            Err(e) => (Vec::new(), Some(format!("task: unexpected listing ({e})"))),
        },
        Err(e) => (task_text(&std::fs::read_to_string(dir.join(Path::new(file).file_name().unwrap_or_default())).unwrap_or_default(), line), Some(e)),
    };
    Source { kind: "task", file: file.to_string(), runner: "task".into(), tasks, note }
}

/// A Taskfile read by Prism, without `task`: the keys of its `tasks:` map and their `desc:`.
fn task_text(text: &str, line: impl Fn(&str) -> String) -> Vec<Task> {
    let mut out: Vec<Task> = Vec::new();
    let mut inside = false;
    let mut indent = None;
    for l in text.lines() {
        if l.trim().is_empty() || l.trim_start().starts_with('#') {
            continue;
        }
        let depth = l.len() - l.trim_start().len();
        if depth == 0 {
            inside = l.trim_end() == "tasks:";
            continue;
        }
        if !inside {
            continue;
        }
        let key = l.trim().strip_suffix(':').or_else(|| l.trim().split_once(": ").map(|(k, _)| k));
        if *indent.get_or_insert(depth) == depth {
            if let Some(name) = key {
                let name = name.trim_matches(['"', '\'']);
                out.push(Task { name: name.to_string(), desc: None, command: line(name), args: false });
            }
        } else if let (Some(desc), Some(last)) = (l.trim().strip_prefix("desc:"), out.last_mut()) {
            last.desc.get_or_insert_with(|| desc.trim().trim_matches(['"', '\'']).to_string());
        }
    }
    out
}

/* ---------- package.json ---------- */

#[derive(Deserialize)]
struct Package {
    #[serde(default)]
    scripts: Scripts,
    #[serde(default, rename = "packageManager")]
    package_manager: Option<String>,
}

/// The scripts in the file's order (a JSON map would sort them).
#[derive(Default)]
struct Scripts(Vec<(String, String)>);

impl<'de> Deserialize<'de> for Scripts {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Scripts;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map of scripts")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> std::result::Result<Scripts, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = map.next_entry::<String, serde_json::Value>()? {
                    if let Some(v) = v.as_str() {
                        out.push((k, v.to_string()));
                    }
                }
                Ok(Scripts(out))
            }
        }
        d.deserialize_map(Visitor)
    }
}

const LOCKFILES: [(&str, &str); 5] = [("pnpm-lock.yaml", "pnpm"), ("yarn.lock", "yarn"), ("bun.lock", "bun"), ("bun.lockb", "bun"), ("package-lock.json", "npm")];

/// The package manager whose lockfile is in `dir`.
fn locked(dir: &Path) -> Option<&'static str> {
    LOCKFILES.iter().find(|(f, _)| dir.join(f).is_file()).map(|(_, r)| *r)
}

/// `nearby`: the package manager of the workspace (the closest lockfile above, or any in the agent's folder).
fn package(dir: &Path, file: &str, rel: &str, nearby: Option<&'static str>) -> Option<Source> {
    let pkg: Package = match serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).ok()?) {
        Ok(p) => p,
        Err(e) => return Some(Source { kind: "npm", file: file.into(), runner: "npm".into(), tasks: Vec::new(), note: Some(format!("package.json unreadable: {e}")) }),
    };
    // `packageManager`, otherwise the lockfile of its folder, otherwise the agent folder's.
    let declared = pkg.package_manager.as_deref().and_then(|p| p.split('@').next()).filter(|p| ["pnpm", "yarn", "bun", "npm"].contains(p));
    let runner = declared.or_else(|| locked(dir)).or(nearby).unwrap_or("npm");
    let base = match (runner, rel) {
        (r, "") => format!("{r} run"),
        ("npm", d) => format!("npm --prefix {} run", quote(d)),
        ("pnpm", d) => format!("pnpm --dir {} run", quote(d)),
        (r, d) => format!("{r} --cwd {} run", quote(d)),
    };
    let names: Vec<&str> = pkg.scripts.0.iter().map(|(n, _)| n.as_str()).collect();
    // `prebuild` / `postbuild` run with `build`: not tasks of their own.
    let hook = |n: &str| ["pre", "post"].iter().any(|p| n.strip_prefix(p).is_some_and(|rest| names.contains(&rest)));
    let tasks = pkg.scripts.0.iter().filter(|(n, _)| !hook(n)).map(|(n, s)| Task { name: n.clone(), desc: Some(s.clone()), command: format!("{base} {}", quote(n)), args: false }).collect();
    Some(Source { kind: "npm", file: file.into(), runner: runner.into(), tasks, note: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("prism-tasks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn quotes_only_what_needs_it() {
        assert_eq!(quote("build:web"), "build:web");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote("a b"), "'a b'");
    }

    #[test]
    fn reads_a_justfile_without_just() {
        let text = "# Builds it\n[group('ci')]\nbuild target=\"x\":\n  echo\n\n[private]\nsecret:\n  echo\n\ndeploy env:\n  echo\n\nset shell := [\"zsh\"]\nalias b := build\nlint *files:\n  echo\n";
        let tasks = just_text(text, |n| format!("just {n}"));
        let got: Vec<_> = tasks.iter().map(|t| (t.name.as_str(), t.desc.as_deref(), t.args)).collect();
        assert_eq!(got, [("build", Some("Builds it"), false), ("deploy", None, true), ("lint", None, false)]);
    }

    #[test]
    fn reads_a_taskfile_without_task() {
        let text = "version: '3'\nvars:\n  APP: x\ntasks:\n  dev:\n    desc: Runs it\n    cmds:\n      - pnpm dev\n  'build:web':\n    cmds: [x]\n  lint: pnpm lint\n";
        let tasks = task_text(text, |n| format!("task {n}"));
        let got: Vec<_> = tasks.iter().map(|t| (t.name.as_str(), t.desc.as_deref())).collect();
        assert_eq!(got, [("dev", Some("Runs it")), ("build:web", None), ("lint", None)]);
    }

    #[test]
    fn package_scripts_keep_their_order_and_runner() {
        let root = scratch("pkg");
        std::fs::create_dir_all(root.join("web")).unwrap();
        std::fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(root.join("web/package.json"), r#"{"scripts":{"dev":"vite","prebuild":"x","build":"vite build","check":"tsc"}}"#).unwrap();
        let s = package(&root.join("web"), "web/package.json", "web", locked(&root)).unwrap();
        assert_eq!(s.runner, "pnpm");
        let got: Vec<_> = s.tasks.iter().map(|t| t.command.as_str()).collect();
        assert_eq!(got, ["pnpm --dir web run dev", "pnpm --dir web run build", "pnpm --dir web run check"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn detects_the_root_and_its_subfolders() {
        let root = scratch("detect");
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
        std::fs::write(root.join("package.json"), r#"{"packageManager":"yarn@4.1.0","scripts":{"test":"jest"}}"#).unwrap();
        std::fs::write(root.join("app/package.json"), r#"{"scripts":{"start":"node ."}}"#).unwrap();
        std::fs::write(root.join("app/package-lock.json"), "{}").unwrap();
        std::fs::write(root.join("node_modules/x/package.json"), r#"{"scripts":{"no":"x"}}"#).unwrap();
        let sources = detect(&root, 1).await.unwrap();
        let got: Vec<_> = sources.iter().map(|s| (s.file.as_str(), s.tasks[0].command.as_str())).collect();
        assert_eq!(got, [("package.json", "yarn run test"), ("app/package.json", "npm --prefix app run start")]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Down to `depth` levels and no further; each package takes the closest lockfile above it.
    #[tokio::test]
    async fn detects_down_to_the_depth() {
        let root = scratch("depth");
        for d in ["apps/api/src/deep", "apps/web", "libs/ui/node_modules/x"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(root.join("yarn.lock"), "").unwrap();
        std::fs::write(root.join("apps/pnpm-lock.yaml"), "").unwrap();
        for (dir, script) in [("apps/api", "api"), ("apps/api/src", "src"), ("apps/api/src/deep", "deep"), ("apps/web", "web"), ("libs/ui/node_modules/x", "no")] {
            std::fs::write(root.join(dir).join("package.json"), format!(r#"{{"scripts":{{"{script}":"x"}}}}"#)).unwrap();
        }
        let files = |sources: Vec<Source>| sources.into_iter().map(|s| (s.file, s.runner)).collect::<Vec<_>>();
        let three = files(detect(&root, 3).await.unwrap());
        let pnpm = |f: &str| (f.to_string(), "pnpm".to_string());
        assert_eq!(three, [pnpm("apps/api/package.json"), pnpm("apps/api/src/package.json"), pnpm("apps/web/package.json")]);
        assert_eq!(files(detect(&root, 2).await.unwrap()).len(), 2, "apps/api/src is level 3");
        assert!(detect(&root, 0).await.unwrap().is_empty(), "the root alone");
        assert_eq!(files(detect(&root, 4).await.unwrap()).len(), 4);
        let _ = std::fs::remove_dir_all(&root);
    }
}
