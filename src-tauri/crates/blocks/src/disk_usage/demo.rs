//! The Disk usage block in the demo profile: a developer's home folder on a disk almost full,
//! scanned two hours ago, Docker, Xcode and Rust builds taking the most.

use std::sync::OnceLock;

use chrono::{DateTime, Duration, Utc};

use super::{
    kinds::{Kind, Reclaim},
    scan::{Node, Tree, DIR, REPO, WORKTREE},
    DiskSpace, DiskUsageState, ScanInfo,
};

const GB: f64 = (1u64 << 30) as f64;

/// A folder or a file of the sample: name, size (GB, files only), kind, reclaim, Δ (GB), flags.
struct S {
    name: &'static str,
    gb: f64,
    kind: Option<Kind>,
    reclaim: Option<Reclaim>,
    delta: f64,
    flags: u8,
    children: Vec<S>,
}

fn f(name: &'static str, gb: f64, kind: Kind) -> S {
    S { name, gb, kind: Some(kind), reclaim: None, delta: 0.0, flags: 0, children: Vec::new() }
}

fn d(name: &'static str, children: Vec<S>) -> S {
    S { name, gb: 0.0, kind: None, reclaim: None, delta: 0.0, flags: DIR, children }
}

impl S {
    fn r(mut self, r: Reclaim) -> Self {
        self.reclaim = Some(r);
        if self.flags & DIR != 0 && self.kind.is_none() {
            self.kind = Some(match r {
                Reclaim::Caches => Kind::Cache,
                Reclaim::Docker | Reclaim::Simulators => Kind::Vm,
                _ => Kind::Build,
            });
        }
        self
    }
    fn up(mut self, gb: f64) -> Self {
        self.delta = gb;
        self
    }
    fn repo(mut self) -> Self {
        self.flags |= REPO;
        self
    }
    fn worktree(mut self) -> Self {
        self.flags |= WORKTREE;
        self
    }
}

fn sample() -> S {
    use Kind::*;
    use Reclaim::*;
    let files = |name, gb, kind| f(name, gb, kind);
    d("~", vec![
        d("Library", vec![
            d("Containers", vec![
                d("com.docker.docker", vec![d("Data", vec![d("vms", vec![d("0", vec![files("Docker.raw", 48.0, Vm).up(6.1)])]), files("log", 0.9, Other)])]).r(Docker),
                d("com.apple.mail", vec![files("Data", 3.1, Docs)]),
            ]),
            d("Developer", vec![
                d("Xcode", vec![d("DerivedData", vec![files("Prism-abc", 14.2, Build).up(3.8), files("Mobile-def", 8.2, Build)]).r(Xcode), files("iOS DeviceSupport", 4.2, Build)]),
                d("CoreSimulator", vec![d("Devices", vec![files("iPhone 17", 9.1, Vm), files("iPad Pro", 7.2, Vm).up(0.4)]).r(Simulators), files("Caches", 1.1, Cache)]),
            ]),
            d("Caches", vec![files("pnpm", 6.2, Cache).up(0.7), files("ms-playwright", 4.1, Cache), files("Homebrew", 3.8, Cache), files("com.spotify.client", 2.4, Cache)]).r(Caches),
            d("Application Support", vec![files("Google", 5.4, Apps), files("Code", 3.2, Apps), files("Slack", 2.1, Apps)]),
        ]),
        d("Development", vec![
            d("Tools", vec![
                d("prism", vec![d("target", vec![files("debug", 19.4, Build).up(3.1), files("release", 7.7, Build).up(1.1)]).r(RustTarget), d("node_modules", vec![files("", 1.6, Build)]).r(NodeModules), files(".git", 0.9, Code)]).repo(),
                d("prism-worktrees", vec![
                    d("quick-actions", vec![d("target", vec![files("debug", 9.8, Build).up(9.8)]).r(RustTarget), d("node_modules", vec![files("", 1.5, Build)]).r(NodeModules)]).worktree(),
                    d("sound-status", vec![d("target", vec![files("debug", 4.4, Build)]).r(RustTarget), d("node_modules", vec![files("", 1.5, Build)]).r(NodeModules)]).worktree(),
                ]),
            ]),
            d("Projects", vec![
                d("mobile", vec![files("ios-build", 11.2, Build), d("node_modules", vec![files("", 6.0, Build)]).r(NodeModules), d("Pods", vec![files("", 4.0, Build)]).r(Pods)]).repo(),
                d("api", vec![d(".venv", vec![files("", 2.1, Build)]).r(PythonEnv), d("node_modules", vec![files("", 3.4, Build)]).r(NodeModules), files("fixtures.sql", 9.6, Docs)]).repo(),
                d("web-app", vec![d("node_modules", vec![files("", 9.2, Build)]).r(NodeModules).up(-1.2), d(".next", vec![files("", 3.1, Build)]).r(JsBuild)]).repo(),
                d("infra", vec![d(".terraform", vec![files("", 4.8, Build)]).r(Terraform)]).repo(),
            ]),
        ]),
        d("Movies", vec![files("Screen recordings", 24.2, Media).up(2.6), files("Exports", 13.8, Media)]),
        d("Downloads", vec![files("ubuntu-26.04.iso", 6.1, Archives).up(1.9), files("Xcode_26.xip", 13.0, Archives), files("datasets.zip", 8.0, Archives), files("", 3.9, Docs)]),
        d("Pictures", vec![d("Photos Library.photoslibrary", vec![files("", 21.5, Media)])]),
        d(".ollama", vec![d("models", vec![files("qwen3-coder-30b.gguf", 18.0, Models).up(4.7)])]),
        d("Documents", vec![files("", 12.0, Docs)]),
        d(".cache", vec![files("huggingface", 7.2, Models), files("pip", 1.8, Cache)]).r(Caches),
        d(".rustup", vec![files("toolchains", 4.6, Build)]),
        d(".Trash", vec![files("old-backup.tar.gz", 5.3, Archives)]).r(Trash),
    ])
}

/// The sample as a tree: each folder's children pushed together, then their own.
fn build(root: S) -> Tree {
    let mut nodes = vec![node(&root, 0)];
    let mut queue = vec![(0u32, root)];
    while let Some((id, s)) = queue.pop() {
        let first = nodes.len() as u32;
        nodes[id as usize].first = first;
        nodes[id as usize].count = s.children.len() as u32;
        for c in &s.children {
            nodes.push(node(c, id));
        }
        for (i, c) in s.children.into_iter().enumerate() {
            queue.push((first + i as u32, c));
        }
    }
    let mut tree = Tree { path: dirs::home_dir().unwrap_or_default(), nodes, info: ScanInfo::default() };
    tree.finish();
    // Δ summed up the folders, from the files' and folders' own.
    for i in (1..tree.nodes.len()).rev() {
        if let Some(d) = tree.nodes[i].delta {
            let p = tree.nodes[i].parent as usize;
            tree.nodes[p].delta = Some(tree.nodes[p].delta.unwrap_or(0) + d);
        }
    }
    tree
}

fn node(s: &S, parent: u32) -> Node {
    let bytes = (s.gb * GB) as u64;
    let dir = s.flags & DIR != 0;
    let files = if dir { 0 } else if matches!(s.kind, Some(Kind::Build | Kind::Cache | Kind::Code | Kind::Docs)) { (s.gb * 9000.0) as u64 + 1 } else { (s.gb * 40.0) as u64 + 1 };
    let kind = s.kind.unwrap_or_default();
    Node {
        name: std::ffi::OsStr::new(s.name).into(),
        parent,
        own: bytes,
        own_files: files,
        own_kind: kind,
        kind,
        classed: s.kind.is_some(),
        reclaim: s.reclaim,
        flags: if s.name.is_empty() { s.flags & !DIR } else { s.flags },
        delta: (s.delta != 0.0).then_some((s.delta * GB) as i64),
        ..Default::default()
    }
}

/// The demo's tree, its nodes summed into the folders that stand for many files.
pub fn tree() -> &'static Tree {
    static TREE: OnceLock<Tree> = OnceLock::new();
    TREE.get_or_init(|| {
        let mut t = build(sample());
        // A file without a name stands for its folder's many small files: summed into it.
        for c in 1..t.nodes.len() {
            if t.nodes[c].name.is_empty() {
                t.nodes[c].flags |= super::scan::REMOVED;
                let (bytes, files, p) = (t.nodes[c].bytes, t.nodes[c].files, t.nodes[c].parent as usize);
                t.nodes[p].own += bytes;
                t.nodes[p].own_files += files;
            }
        }
        t.info.reclaim = t.reclaim_groups();
        t
    })
}

pub fn state(now: DateTime<Utc>) -> DiskUsageState {
    let t = tree();
    let at = now - Duration::hours(2);
    let total = 494_000_000_000u64;
    DiskUsageState {
        roots: vec!["~".into(), "/".into(), "~/Development".into()],
        cleanup: true,
        delta: true,
        scan: Some(ScanInfo {
            id: 1,
            root: "~".into(),
            at,
            duration_ms: 41_200,
            bytes: t.nodes[0].bytes,
            files: t.nodes[0].files,
            dirs: 212_480,
            denied: 3,
            denied_paths: vec!["~/Library/Mail".into(), "~/Library/Safari".into(), "~/Library/Messages".into()],
            previous_at: Some(now - Duration::days(7)),
            saved: false,
            reclaim: t.info.reclaim.clone(),
        }),
        running: None,
        disk: Some(DiskSpace { name: "Macintosh HD".into(), mount: "/".into(), total, available: 41_000_000_000 }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_adds_up() {
        let t = tree();
        let v = t.view(0, 3);
        assert_eq!(v.name, "~");
        assert!(v.bytes > 350 * (1 << 30));
        let groups: Vec<Reclaim> = t.info.reclaim.iter().map(|g| g.kind).collect();
        assert_eq!(groups[0], Reclaim::Docker);
        assert!(groups.contains(&Reclaim::NodeModules) && groups.contains(&Reclaim::Trash));
        // The folders standing for many files show no child of their own.
        let docs = v.children.as_ref().unwrap().iter().find(|c| c.name == "Documents").unwrap();
        assert!(docs.children.as_ref().unwrap().is_empty() && docs.rest_bytes > 0);
    }
}
