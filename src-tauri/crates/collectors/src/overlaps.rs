//! Overlaps between the agents and worktrees of a repository: each worktree's changes against the
//! merge-base of its base (the commits of its branch, its uncommitted changes and its new files:
//! what it will bring), compared two by two among those of the same base. The same lines: a merge
//! conflict all but certain; the same file: a merge to check. Told before the merge, rather than at
//! the rebase: the forge sees conflicts between pushed MRs only, git at the merge only, and neither
//! sees an agent's uncommitted work.
//!
//! Read again on events, not by polling: a file written in a worktree (watched on macOS), a commit,
//! a pull or a rebase (its HEAD), an agent's turn over (its status), the files it changed counted
//! again by the worktrees collector, the base moved. Only the worktrees whose key moved are read again; the
//! snapshot changes only when an overlap does.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Result;

use crate::{
    config::{OverlapsConfig, WorktreeRepo},
    git::run as git,
    state::{Hub, LineRange, Overlap, OverlapElsewhere, OverlapFile, OverlapLevel, Overlaps, Snapshot, Worktree},
};

const SOURCE: &str = "overlaps";
/// Writes grouped: an agent writes a file in several goes, its tools a few files in a row.
const SETTLE: Duration = Duration::from_secs(3);
/// Whatever happened, everything is read again this often: a change no event told of.
const SAFETY: Duration = Duration::from_secs(300);
/// A diff larger than this is read up to it: its first files are compared, not the rest.
const MAX_DIFF: usize = 4 << 20;

/// A worktree's changes against its base, in the lines of its merge-base.
#[derive(Debug, Clone, Default)]
struct Changes {
    /// `git merge-base HEAD origin/<base>`.
    merge_base: String,
    /// Changed files (their path in the base, a new one's own), their hunks in order.
    files: BTreeMap<String, Vec<Hunk>>,
}

/// Lines of the base a change covers, in half lines: line `n` is `2n`, the gap after it `2n + 1`;
/// a pure insertion after line `n` is the point `2n + 1`, lines `s`..`e` changed `[2s, 2e]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hunk {
    lo: u64,
    hi: u64,
    /// As git wrote it (`@@ -start,len`), for the window.
    range: LineRange,
}

impl Hunk {
    fn new(start: u32, len: u32) -> Self {
        let (s, l) = (u64::from(start), u64::from(len));
        let (lo, hi) = if l == 0 { (2 * s + 1, 2 * s + 1) } else { (2 * s, 2 * (s + l - 1)) };
        Self { lo, hi, range: LineRange { start, len } }
    }

    /// The whole file (a binary one): every line.
    fn whole() -> Self {
        Self { lo: 0, hi: u64::MAX / 4, range: LineRange { start: 0, len: u32::MAX } }
    }

    /// The first line after it, in half lines.
    fn after(&self) -> u64 {
        if self.range.len == 0 { self.hi + 1 } else { self.hi + 2 }
    }

    /// The same lines, or touching: a change on a line and an insertion right before or after it.
    fn meets(&self, other: &Hunk) -> bool {
        self.lo <= other.hi + 1 && other.lo <= self.hi + 1
    }
}

/// A worktree compared: one with changes, on a work branch.
#[derive(Debug, Clone)]
struct Side {
    repo: String,
    path: String,
    branch: Option<String>,
    base: String,
    changes: Changes,
}

pub async fn run(cfg: OverlapsConfig, repos: Vec<WorktreeRepo>, hub: Hub) {
    if !cfg.enabled || repos.is_empty() {
        hub.update(|s| std::mem::take(&mut s.overlaps) != Overlaps::default());
        return;
    }
    let (tx, mut written) = tokio::sync::mpsc::unbounded_channel::<PathBuf>();
    let mut watched: Vec<String> = Vec::new();
    let mut _watcher: Option<notify::RecommendedWatcher> = None;
    let mut snapshot = hub.subscribe();
    let mut keys: HashMap<String, u64> = HashMap::new();
    let mut read: HashMap<String, Changes> = HashMap::new();
    let mut dirty: HashSet<String> = HashSet::new();
    let mut state = Memory::default();
    let mut first = true;
    // A deadline, not a sleep started again at each change: the snapshot changes every few seconds.
    let mut full = tokio::time::Instant::now() + SAFETY;
    loop {
        if !std::mem::take(&mut first) {
            tokio::select! {
                changed = snapshot.changed() => if changed.is_err() { return },
                Some(path) = written.recv() => mark_written(&path, &watched, &mut dirty),
                _ = tokio::time::sleep_until(full) => {
                    dirty.extend(keys.keys().cloned());
                    full = tokio::time::Instant::now() + SAFETY;
                }
            }
            // A burst of writes settles before anything is read.
            let settle = tokio::time::sleep(SETTLE);
            tokio::pin!(settle);
            loop {
                tokio::select! {
                    _ = &mut settle => break,
                    Some(path) = written.recv() => mark_written(&path, &watched, &mut dirty),
                }
            }
        }
        // What each worktree is (a key moved, it is read again), the snapshot borrowed, not copied.
        let (now, worktrees) = {
            let s = snapshot.borrow_and_update();
            let now = worktree_keys(&s);
            let worktrees: Vec<Worktree> = s.worktrees.iter().filter(|w| now.contains_key(&w.path)).cloned().collect();
            (now, worktrees)
        };
        // A worktree removed: its pairs go, or turn into "has to rebase" if it was merged.
        let removed = keys.keys().any(|p| !now.contains_key(p));
        for (path, key) in &now {
            if keys.get(path) != Some(key) {
                dirty.insert(path.clone());
            }
        }
        keys = now;
        read.retain(|p, _| keys.contains_key(p));
        // The watch follows the worktrees (macOS: one FSEvents stream, whatever their size).
        let mut paths: Vec<String> = keys.keys().cloned().collect();
        paths.sort();
        if paths != watched {
            _watcher = watch(&paths, tx.clone());
            watched = paths;
        }
        if dirty.is_empty() && !removed {
            continue;
        }
        let worktrees: Vec<&Worktree> = worktrees.iter().collect();
        for w in worktrees.iter().filter(|w| dirty.contains(&w.path)) {
            match changes(Path::new(&w.path), &w.base).await {
                Ok(c) => drop(read.insert(w.path.clone(), c)),
                Err(e) => {
                    tracing::debug!("overlaps: {}: {e:#}", w.path);
                    read.remove(&w.path);
                }
            }
        }
        dirty.clear();
        let found = compare(&cfg, &repos, &worktrees, &read, &mut state).await;
        hub.update(|s| {
            let changed = s.overlaps != found;
            s.overlaps = found;
            changed
        });
        hub.set_error(SOURCE, None);
    }
}

/// A file written under a worktree: that worktree (the deepest holding it) is read again. Its git
/// folder, dependencies and build outputs say nothing of its changes.
fn mark_written(path: &Path, watched: &[String], dirty: &mut HashSet<String>) {
    let p = path.to_string_lossy();
    if ["/.git/", "/node_modules/", "/target/", "/.claude/", "/.svelte-kit/", "/dist/"].iter().any(|d| p.contains(d)) {
        return;
    }
    if let Some(w) = watched.iter().filter(|w| p.starts_with(w.as_str()) && p[w.len()..].starts_with('/')).max_by_key(|w| w.len()) {
        dirty.insert(w.clone());
    }
}

/// Each worktree that can be compared and what it is now: its HEAD, base, branch, counts of
/// changes, and the status of the agents in it (a turn over is read again).
fn worktree_keys(s: &Snapshot) -> HashMap<String, u64> {
    s.worktrees
        .iter()
        .filter(|w| !w.prunable)
        .map(|w| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            // `behind`: the base moved (a merge landed), what has to rebase is checked again.
            (&w.head, &w.base, &w.branch, w.changes, w.untracked, w.ahead, w.behind, w.upstream_gone).hash(&mut h);
            for a in s.agents.iter().filter(|a| a.worktree.as_deref() == Some(w.path.as_str()) || a.cwd == w.path || a.also.contains(&w.path)) {
                (&a.id, format!("{:?}", a.status)).hash(&mut h);
            }
            (w.path.clone(), h.finish())
        })
        .collect()
}

fn watch(paths: &[String], tx: tokio::sync::mpsc::UnboundedSender<PathBuf>) -> Option<notify::RecommendedWatcher> {
    // Elsewhere than macOS, a recursive watch is one per folder (inotify), dependencies and build
    // outputs included: the events of the worktrees collector and the agents do instead.
    if !cfg!(target_os = "macos") || paths.is_empty() {
        return None;
    }
    use notify::{RecursiveMode, Watcher};
    let handler = move |event: notify::Result<notify::Event>| {
        if let Ok(e) = event
            && !e.kind.is_access()
        {
            for p in e.paths {
                let _ = tx.send(p);
            }
        }
    };
    let mut watcher = notify::recommended_watcher(handler).inspect_err(|e| tracing::warn!("overlaps: no file watch: {e}")).ok()?;
    for p in paths {
        if let Err(e) = watcher.watch(Path::new(p), RecursiveMode::Recursive) {
            tracing::debug!("overlaps: cannot watch {p}: {e}");
        }
    }
    Some(watcher)
}

/// What a worktree brings against `origin/<base>`: `git diff -U0` from the merge-base, and the
/// untracked files as new ones.
async fn changes(dir: &Path, base: &str) -> Result<Changes> {
    let merge_base = git(dir, &["merge-base", "HEAD", &format!("origin/{base}")]).await?.trim().to_string();
    let raw = git(dir, &["-c", "core.quotePath=false", "diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", "-M", &merge_base]).await?;
    let mut files = parse(cut(&raw, MAX_DIFF));
    let untracked = git(dir, &["ls-files", "-z", "--others", "--exclude-standard"]).await?;
    for p in untracked.split('\0').filter(|p| !p.is_empty() && !crate::worktrees::is_claude_litter(p)) {
        files.entry(p.to_string()).or_default().push(Hunk::new(0, 0));
    }
    Ok(Changes { merge_base, files })
}

fn cut(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// `git diff -U0`: each file (its path in the base; a new file's own) and the old side of its hunks.
fn parse(raw: &str) -> BTreeMap<String, Vec<Hunk>> {
    let mut out: BTreeMap<String, Vec<Hunk>> = BTreeMap::new();
    let mut current: Option<String> = None;
    let (mut old, mut new) = (None::<String>, None::<String>);
    for line in raw.lines() {
        if let Some(names) = line.strip_prefix("diff --git ") {
            (current, old, new) = (None, None, None);
            // Kept until `---`/`+++` say better: a mode change or a pure rename has neither.
            // `a/x b/x`, or quoted: `"a/x y" "b/x y"`.
            let split = match names.strip_prefix('"') {
                Some(_) => names.split_once("\" \"").map(|(a, b)| (format!("{a}\""), format!("\"{b}"))),
                None => names.split_once(" b/").map(|(a, b)| (a.to_string(), format!("b/{b}"))),
            };
            if let Some((a, b)) = split {
                (old, new) = (Some(name(&a, "a/")), Some(name(&b, "b/")));
            }
        } else if let Some(p) = line.strip_prefix("rename from ") {
            old = Some(name(p, ""));
        } else if let Some(p) = line.strip_prefix("rename to ") {
            new = Some(name(p, ""));
        } else if let Some(p) = line.strip_prefix("--- ") {
            old = (p != "/dev/null").then(|| name(p, "a/"));
        } else if let Some(p) = line.strip_prefix("+++ ") {
            if p != "/dev/null" {
                new = Some(name(p, "b/"));
            }
            current = old.clone().or_else(|| new.clone());
            if let Some(c) = &current {
                out.entry(c.clone()).or_default();
            }
        } else if line.starts_with("Binary files ") {
            if let Some(path) = old.clone().or_else(|| new.clone()) {
                out.entry(path).or_default().push(Hunk::whole());
            }
        } else if let Some(h) = line.strip_prefix("@@ -") {
            let Some(c) = &current else { continue };
            let range = h.split(' ').next().unwrap_or("");
            let (start, len) = match range.split_once(',') {
                Some((s, l)) => (s.parse().unwrap_or(0), l.parse().unwrap_or(0)),
                None => (range.parse().unwrap_or(0), 1),
            };
            out.entry(c.clone()).or_default().push(Hunk::new(start, len));
        } else if line.starts_with("similarity index") || line.starts_with("old mode") {
            // A pure rename or mode change: the file is touched, no line is.
            if let Some(path) = old.clone().or_else(|| new.clone()) {
                out.entry(path).or_default();
            }
        }
    }
    // A pure rename: its name changes, no line; the file counts as touched.
    out
}

/// A file name as git writes it in a diff: the tab after it (a name with a space) dropped, quoted
/// ones unescaped (`"a/x\"y"`, `\303\251` for `é`), then its `a/`/`b/` prefix.
fn name(raw: &str, prefix: &str) -> String {
    let raw = raw.strip_suffix('\t').unwrap_or(raw);
    let plain = match raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        Some(q) => {
            let mut bytes = Vec::with_capacity(q.len());
            let mut it = q.bytes().peekable();
            while let Some(c) = it.next() {
                if c != b'\\' {
                    bytes.push(c);
                    continue;
                }
                match it.next() {
                    Some(d @ b'0'..=b'7') => {
                        let mut v = u32::from(d - b'0');
                        for _ in 0..2 {
                            match it.peek() {
                                Some(&e @ b'0'..=b'7') => {
                                    v = v * 8 + u32::from(e - b'0');
                                    it.next();
                                }
                                _ => break,
                            }
                        }
                        bytes.push(v as u8);
                    }
                    Some(b'n') => bytes.push(b'\n'),
                    Some(b't') => bytes.push(b'\t'),
                    Some(b'r') => bytes.push(b'\r'),
                    Some(b'a') => bytes.push(7),
                    Some(b'b') => bytes.push(8),
                    Some(b'f') => bytes.push(12),
                    Some(b'v') => bytes.push(11),
                    Some(e) => bytes.push(e),
                    None => {}
                }
            }
            String::from_utf8_lossy(&bytes).into_owned()
        }
        None => raw.to_string(),
    };
    plain.strip_prefix(prefix).map(str::to_string).unwrap_or(plain)
}

/// The hunks between two merge-bases in a file: each one's old side and new side.
type Moves = Vec<(Hunk, Hunk)>;

/// What is kept from one comparison to the next.
#[derive(Default)]
struct Memory {
    /// The pairs found last time, by (repo, a, b): when one side is merged, the other is checked
    /// against what it brought to the base.
    pairs: HashMap<(String, String, String), Overlap>,
    /// A merge-base's lines in another's, by (from, to, path): the hunks between them (or none:
    /// the one is no ancestor of the other).
    moves: HashMap<(String, String, String), Option<Moves>>,
    /// Each side's changes last seen before it was merged: what it brought, checked against the
    /// other once it is in the base (its own diff is then empty, or its worktree gone).
    last: HashMap<String, Changes>,
}

/// Every pair of the same repository and base, then the merged sides still met.
async fn compare(cfg: &OverlapsConfig, repos: &[WorktreeRepo], worktrees: &[&Worktree], read: &HashMap<String, Changes>, memory: &mut Memory) -> Overlaps {
    let mut out = Overlaps::default();
    let mut sides: Vec<Side> = Vec::new();
    // The merged ones: their branch gone from the remote (the MR merged, the branch deleted).
    let mut merged: HashSet<String> = HashSet::new();
    for w in worktrees {
        let Some(repo) = repos.iter().find(|r| r.name == w.repo) else { continue };
        if w.is_main && !work_branch(repo, w.branch.as_deref()).await {
            continue;
        }
        // Before its changes: merged by a merge commit, its diff against the base is empty.
        if w.upstream_gone {
            merged.insert(w.path.clone());
            continue;
        }
        let Some(c) = read.get(&w.path) else { continue };
        let files: BTreeMap<String, Vec<Hunk>> = c.files.iter().filter(|(p, _)| !cfg.ignored(p)).map(|(p, h)| (p.clone(), h.clone())).collect();
        if files.is_empty() {
            continue;
        }
        sides.push(Side { repo: w.repo.clone(), path: w.path.clone(), branch: w.branch.clone(), base: w.base.clone(), changes: Changes { merge_base: c.merge_base.clone(), files } });
    }
    // The bases of the repositories: a worktree on another is shown as not compared with them.
    let mut bases: HashMap<String, String> = HashMap::new();
    for r in repos {
        if let Ok(b) = crate::worktrees::resolve_base(r).await {
            bases.insert(r.name.clone(), b);
        }
    }
    for s in &sides {
        if bases.get(&s.repo).is_some_and(|b| *b != s.base) {
            out.other_base.push(OverlapElsewhere { repo: s.repo.clone(), path: s.path.clone(), branch: s.branch.clone(), base: s.base.clone() });
        }
    }
    let mut pairs = HashMap::new();
    for (i, a) in sides.iter().enumerate() {
        for b in sides.iter().skip(i + 1).filter(|b| b.repo == a.repo && b.base == a.base) {
            let repo = repos.iter().find(|r| r.name == a.repo).map(|r| r.path.clone()).unwrap_or_else(|| PathBuf::from(&a.path));
            if let Some(o) = pair(&repo, a, b, memory).await {
                pairs.insert((o.repo.clone(), o.a.clone(), o.b.clone()), o);
            }
        }
    }
    // A side merged (or its worktree gone): the other has to rebase on it while its changes meet
    // what the base received since its own merge-base.
    let present: HashSet<&str> = sides.iter().map(|s| s.path.as_str()).collect();
    for s in &sides {
        memory.last.insert(s.path.clone(), s.changes.clone());
    }
    let before = std::mem::take(&mut memory.pairs);
    for ((repo, a, b), was) in before {
        if pairs.contains_key(&(repo.clone(), a.clone(), b.clone())) {
            continue;
        }
        let (gone, stays) = match (present.contains(a.as_str()), present.contains(b.as_str())) {
            (false, true) => (&a, &b),
            (true, false) => (&b, &a),
            _ => continue,
        };
        // Gone without being merged (its changes dropped, its worktree removed unmerged): nothing.
        if !merged.contains(gone) && worktrees.iter().any(|w| w.path == *gone) {
            continue;
        }
        let Some(side) = sides.iter().find(|s| s.path == *stays) else { continue };
        let Some(theirs) = memory.last.get(gone).cloned() else { continue };
        // By its path: a merged overlap has its sides swapped (the merged one first).
        let branch = if was.a == *gone { was.a_branch.clone() } else { was.b_branch.clone() };
        if let Some(o) = rebase_on(side, gone, branch, &theirs, memory).await {
            pairs.insert((repo, o.a.clone(), o.b.clone()), o);
        }
    }
    // The changes of those neither present nor in a pair are of no use any more.
    let kept: HashSet<&String> = pairs.keys().flat_map(|(_, a, b)| [a, b]).collect();
    memory.last.retain(|p, _| present.contains(p.as_str()) || kept.contains(p));
    memory.pairs = pairs.clone();
    out.pairs = pairs.into_values().collect();
    out.pairs.sort_by(|x, y| y.level.cmp(&x.level).then_with(|| (&x.repo, &x.a, &x.b).cmp(&(&y.repo, &y.a, &y.b))));
    out
}

/// The main checkout counts on a work branch only: not the main branch, nor a release one.
async fn work_branch(repo: &WorktreeRepo, branch: Option<&str>) -> bool {
    let Some(branch) = branch else { return false };
    let main = crate::worktrees::main_branch(repo).await;
    branch != main && (repo.release_prefix.is_empty() || !branch.starts_with(&repo.release_prefix)) && !matches!(branch, "main" | "master" | "develop")
}

/// The files `a` and `b` both change, at the lines level where their hunks meet in the base. Two
/// merge-bases apart, the older one's lines are carried into the newer one's first.
async fn pair(repo: &Path, a: &Side, b: &Side, memory: &mut Memory) -> Option<Overlap> {
    let shared: Vec<&String> = a.changes.files.keys().filter(|p| b.changes.files.contains_key(*p)).collect();
    if shared.is_empty() {
        return None;
    }
    let (ma, mb) = (&a.changes.merge_base, &b.changes.merge_base);
    // Which is older, once for the pair: `a`'s (1), `b`'s (2), the same or neither (0).
    let older = if ma == mb {
        0
    } else if is_ancestor(repo, ma, mb).await {
        1
    } else if is_ancestor(repo, mb, ma).await {
        2
    } else {
        0
    };
    let mut files = Vec::new();
    for path in shared {
        let (ha, hb) = (&a.changes.files[path], &b.changes.files[path]);
        // Both in the lines of the newer merge-base.
        let (ca, cb) = match older {
            1 => (carried(repo, ma, mb, path, ha, memory).await, hb.clone()),
            2 => (ha.clone(), carried(repo, mb, ma, path, hb, memory).await),
            _ => (ha.clone(), hb.clone()),
        };
        files.push(file(path, ha, hb, &ca, &cb));
    }
    Some(overlap(a.repo.clone(), a.base.clone(), (&a.path, &a.branch), (&b.path, &b.branch), files, false))
}

/// One shared file: its level, and each side's hunks (in its own base's lines) that meet.
fn file(path: &str, ha: &[Hunk], hb: &[Hunk], ca: &[Hunk], cb: &[Hunk]) -> OverlapFile {
    let a: Vec<LineRange> = ha.iter().zip(ca).filter(|(_, x)| cb.iter().any(|y| x.meets(y))).map(|(h, _)| h.range).collect();
    let b: Vec<LineRange> = hb.iter().zip(cb).filter(|(_, y)| ca.iter().any(|x| x.meets(y))).map(|(h, _)| h.range).collect();
    if a.is_empty() {
        let ranges = |h: &[Hunk]| h.iter().map(|h| h.range).collect();
        OverlapFile { path: path.to_string(), level: OverlapLevel::File, a: ranges(ha), b: ranges(hb) }
    } else {
        OverlapFile { path: path.to_string(), level: OverlapLevel::Lines, a, b }
    }
}

fn overlap(repo: String, base: String, a: (&String, &Option<String>), b: (&String, &Option<String>), files: Vec<OverlapFile>, merged: bool) -> Overlap {
    let level = files.iter().map(|f| f.level).max().unwrap_or(OverlapLevel::File);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in &files {
        f.path.hash(&mut h);
        for r in f.a.iter().chain(&f.b) {
            (r.start, r.len).hash(&mut h);
        }
    }
    merged.hash(&mut h);
    Overlap { repo, base, a: a.0.clone(), b: b.0.clone(), a_branch: a.1.clone(), b_branch: b.1.clone(), level, files, merged, key: format!("{:016x}", h.finish()) }
}

async fn is_ancestor(repo: &Path, older: &str, newer: &str) -> bool {
    git(repo, &["merge-base", "--is-ancestor", older, newer]).await.is_ok()
}

/// `hunks`, in the lines of merge-base `from`, carried into those of `to` (a descendant): shifted
/// by what was inserted or removed above them, widened to what changed around them.
async fn carried(repo: &Path, from: &str, to: &str, path: &str, hunks: &[Hunk], memory: &mut Memory) -> Vec<Hunk> {
    let key = (from.to_string(), to.to_string(), path.to_string());
    if !memory.moves.contains_key(&key) {
        let raw = git(repo, &["-c", "core.quotePath=false", "diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames", from, to, "--", path]).await.ok();
        let moves = raw.map(|raw| moves(&raw));
        // Merge-bases change slowly; a few hundred files' moves at most.
        if memory.moves.len() > 2000 {
            memory.moves.clear();
        }
        memory.moves.insert(key.clone(), moves);
    }
    match &memory.moves[&key] {
        Some(moves) => hunks.iter().map(|h| carry(h, moves)).collect(),
        None => hunks.to_vec(),
    }
}

/// The hunks of `git diff -U0 from to -- path`: each one's old side and new side.
fn moves(raw: &str) -> Moves {
    let side = |r: &str| match r.split_once(',') {
        Some((s, l)) => Hunk::new(s.parse().unwrap_or(0), l.parse().unwrap_or(0)),
        None => Hunk::new(r.parse().unwrap_or(0), 1),
    };
    raw.lines()
        .filter_map(|l| l.strip_prefix("@@ -"))
        .filter_map(|h| {
            let mut it = h.split(' ');
            let old = it.next()?;
            let new = it.next()?.strip_prefix('+')?;
            Some((side(old), side(new)))
        })
        .collect()
}

fn carry(h: &Hunk, moves: &[(Hunk, Hunk)]) -> Hunk {
    let point = |p: u64| -> (u64, u64) {
        let mut delta: i64 = 0;
        for (old, new) in moves {
            if p < old.lo {
                break;
            }
            if p <= old.hi {
                return (new.lo, new.hi);
            }
            // Where the line after each side begins: absolute, not added up.
            delta = new.after() as i64 - old.after() as i64;
        }
        let q = (p as i64 + delta).max(0) as u64;
        (q, q)
    };
    let (lo, _) = point(h.lo);
    let (_, hi) = point(h.hi);
    Hunk { lo, hi: hi.max(lo), range: h.range }
}

/// `side` against what the merged side brought to the base: the hunks that landed on
/// `origin/<base>` since `side`'s merge-base and meet the merged side's last changes (not every
/// change landed since, others' included). Still met, it has to rebase on it; rebased, what it
/// brought is behind its merge-base and nothing is left.
async fn rebase_on(side: &Side, merged_path: &str, merged_branch: Option<String>, theirs: &Changes, memory: &mut Memory) -> Option<Overlap> {
    let dir = Path::new(&side.path);
    let tip = format!("origin/{}", side.base);
    let paths: Vec<&str> = theirs.files.keys().map(String::as_str).filter(|p| side.changes.files.contains_key(*p)).collect();
    if paths.is_empty() {
        return None;
    }
    let mut args = vec!["-c", "core.quotePath=false", "diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames", side.changes.merge_base.as_str(), tip.as_str(), "--"];
    args.extend(&paths);
    let landed = parse(&git(dir, &args).await.ok()?);
    let (from, to) = (&theirs.merge_base, &side.changes.merge_base);
    // Their lines in those of `side`'s merge-base, when it is the newer one.
    let carry_theirs = from != to && is_ancestor(dir, from, to).await;
    let mut files = Vec::new();
    for path in paths {
        let Some(base) = landed.get(path) else { continue };
        let mine = &theirs.files[path];
        let mine = if carry_theirs { carried(dir, from, to, path, mine, memory).await } else { mine.clone() };
        let brought: Vec<Hunk> = base.iter().filter(|l| mine.iter().any(|m| m.meets(l))).copied().collect();
        if brought.is_empty() {
            continue;
        }
        let ours = &side.changes.files[path];
        files.push(file(path, &brought, ours, &brought, ours));
    }
    if files.is_empty() {
        return None;
    }
    let merged_path = merged_path.to_string();
    Some(overlap(side.repo.clone(), side.base.clone(), (&merged_path, &merged_branch), (&side.path, &side.branch), files, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(start: u32, len: u32) -> Hunk {
        Hunk::new(start, len)
    }

    #[test]
    fn hunks_meet_on_the_same_lines_or_touching() {
        // Lines 92–97 against 95–100: the same lines.
        assert!(h(92, 6).meets(&h(95, 6)));
        // An insertion after line 5 and a change of line 6: touching, a conflict for git.
        assert!(h(5, 0).meets(&h(6, 1)) && h(5, 1).meets(&h(5, 0)));
        // Lines 5 and 7, line 6 between them untouched: apart.
        assert!(!h(5, 1).meets(&h(7, 1)));
        // Insertions after lines 5 and 6: apart, line 6 between them.
        assert!(!h(5, 0).meets(&h(6, 0)));
        // Two new files: both insert at the top.
        assert!(h(0, 0).meets(&h(0, 0)));
    }

    #[test]
    fn parses_a_diff_without_context() {
        let raw = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -10,3 +10,4 @@ fn x()\n-a\n+b\n@@ -40 +41,0 @@\n-c\ndiff --git a/new.rs b/new.rs\nnew file mode 100644\n--- /dev/null\n+++ b/new.rs\n@@ -0,0 +1,2 @@\n+x\n+y\ndiff --git a/old.rs b/moved.rs\nsimilarity index 100%\nrename from old.rs\nrename to moved.rs\ndiff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\n";
        let files = parse(raw);
        assert_eq!(files["src/a.rs"].iter().map(|h| h.range).collect::<Vec<_>>(), vec![LineRange { start: 10, len: 3 }, LineRange { start: 40, len: 1 }]);
        assert_eq!(files["new.rs"], vec![h(0, 0)]);
        assert!(files["old.rs"].is_empty());
        assert_eq!(files["img.png"], vec![Hunk::whole()]);
    }

    #[test]
    fn reads_the_names_git_quotes() {
        assert_eq!(name("a/docs/my notes.md\t", "a/"), "docs/my notes.md");
        assert_eq!(name("\"b/src/\\303\\251.rs\"", "b/"), "src/é.rs");
        assert_eq!(name("\"a/x\\\"y\\\\z\"", "a/"), "x\"y\\z");
        let raw = "diff --git \"a/n\\303\\251.bin\" \"b/n\\303\\251.bin\"\nBinary files \"a/n\\303\\251.bin\" and \"b/n\\303\\251.bin\" differ\n";
        assert_eq!(parse(raw)["né.bin"], vec![Hunk::whole()]);
    }

    #[test]
    fn carries_lines_into_a_newer_merge_base() {
        // Between the merge-bases: 3 lines inserted after line 10, line 50 changed into 2.
        let moves = moves("@@ -10,0 +11,3 @@\n@@ -50 +54,2 @@\n");
        // Line 30 is now 33; lines 49–51 are 52–56 (line 50 became 54–55).
        assert_eq!(carry(&h(30, 1), &moves), Hunk { lo: 66, hi: 66, range: h(30, 1).range });
        let c = carry(&h(49, 3), &moves);
        assert_eq!((c.lo, c.hi), (2 * 52, 2 * 56));
        // Above every move: unchanged.
        assert_eq!(carry(&h(3, 2), &moves), h(3, 2));
    }

    /// Two worktrees off the same base: the same lines, then only the same file, then one ignored.
    #[tokio::test]
    async fn finds_the_overlaps_of_two_worktrees() {
        let root = std::env::temp_dir().join(format!("prism-overlaps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let main = root.join("main");
        std::fs::create_dir_all(&main).unwrap();
        let run = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).current_dir(dir).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        let body: String = (1..=100).map(|n| format!("line {n}\n")).collect();
        std::fs::write(main.join("bar.rs"), &body).unwrap();
        std::fs::write(main.join("Cargo.lock"), "a\n").unwrap();
        run(&main, &["init", "-q", "-b", "main"]);
        run(&main, &["add", "."]);
        run(&main, &["commit", "-q", "-m", "one"]);
        run(&main, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let (wa, wb) = (root.join("a"), root.join("b"));
        run(&main, &["worktree", "add", "-q", "-b", "agent/a", wa.to_str().unwrap()]);
        run(&main, &["worktree", "add", "-q", "-b", "agent/b", wb.to_str().unwrap()]);
        // A commits lines 92–97; B changes 95 without committing; both touch the lockfile.
        let edit = |dir: &Path, lines: &[u32]| {
            let text: String = (1..=100).map(|n| if lines.contains(&n) { format!("{n} changed\n") } else { format!("line {n}\n") }).collect();
            std::fs::write(dir.join("bar.rs"), text).unwrap();
            std::fs::write(dir.join("Cargo.lock"), format!("{lines:?}\n")).unwrap();
        };
        edit(&wa, &[92, 93, 94, 95, 96, 97]);
        run(&wa, &["commit", "-qam", "a"]);
        edit(&wb, &[95]);
        let repo = WorktreeRepo { name: "r".into(), path: main.clone(), main_branch: "main".into(), release_prefix: String::new(), base_branch: "main".into() };
        let wt = |p: &Path, branch: &str| Worktree {
            repo: "r".into(), path: p.to_string_lossy().into(), branch: Some(branch.into()), ticket: None, base: "main".into(), behind: 0, ahead: 0, dirty: true, changes: 1,
            untracked: None, size_bytes: None, unpushed: 0, upstream: None, upstream_gone: false, unpulled: 0, head: None, last_commit: None, is_main: false, prunable: false, locked: false,
        };
        let (ta, tb) = (wt(&wa, "agent/a"), wt(&wb, "agent/b"));
        let mut read = HashMap::new();
        for w in [&ta, &tb] {
            read.insert(w.path.clone(), changes(Path::new(&w.path), "main").await.unwrap());
        }
        let cfg = OverlapsConfig::default();
        let mut memory = Memory::default();
        let found = compare(&cfg, std::slice::from_ref(&repo), &[&ta, &tb], &read, &mut memory).await;
        assert_eq!(found.pairs.len(), 1, "{found:?}");
        let o = &found.pairs[0];
        assert_eq!(o.level, OverlapLevel::Lines);
        assert_eq!(o.files.len(), 1, "the lockfile is ignored");
        assert_eq!(o.files[0].a, vec![LineRange { start: 92, len: 6 }]);
        assert_eq!(o.files[0].b, vec![LineRange { start: 95, len: 1 }]);
        // B moves to line 10: the same file, other lines.
        edit(&wb, &[10]);
        read.insert(tb.path.clone(), changes(&wb, "main").await.unwrap());
        let found = compare(&cfg, std::slice::from_ref(&repo), &[&ta, &tb], &read, &mut memory).await;
        assert_eq!(found.pairs[0].level, OverlapLevel::File);
        // A's branch merged (gone from the remote) and its commit in the base: B, still on line 10,
        // no longer meets it; back on line 95, it has to rebase on A.
        run(&main, &["update-ref", "refs/remotes/origin/main", "agent/a"]);
        let merged = Worktree { upstream_gone: true, ..ta.clone() };
        edit(&wb, &[95]);
        read.insert(tb.path.clone(), changes(&wb, "main").await.unwrap());
        // A read again: its commit now in the base, its diff is empty (a merge, not a squash).
        read.insert(ta.path.clone(), changes(&wa, "main").await.unwrap());
        assert!(read[&ta.path].files.is_empty());
        let found = compare(&cfg, std::slice::from_ref(&repo), &[&merged, &tb], &read, &mut memory).await;
        assert_eq!(found.pairs.len(), 1, "{found:?}");
        assert!(found.pairs[0].merged && found.pairs[0].level == OverlapLevel::Lines && found.pairs[0].a == ta.path);
        // A second round: the same overlap, A still named after its own branch.
        let again = compare(&cfg, std::slice::from_ref(&repo), &[&merged, &tb], &read, &mut memory).await;
        assert_eq!(again, found);
        assert_eq!(again.pairs[0].a_branch.as_deref(), Some("agent/a"));
        assert_eq!(again.pairs[0].b_branch.as_deref(), Some("agent/b"));
        // Its worktree removed: still to rebase on it.
        let gone = compare(&cfg, std::slice::from_ref(&repo), &[&tb], &read, &mut memory).await;
        assert_eq!(gone, found);
        // B rebased on the base: nothing left.
        run(&wb, &["stash", "-q"]);
        run(&wb, &["rebase", "-q", "origin/main"]);
        read.insert(tb.path.clone(), changes(&wb, "main").await.unwrap());
        let found = compare(&cfg, std::slice::from_ref(&repo), &[&merged, &tb], &read, &mut memory).await;
        assert!(found.pairs.is_empty(), "{found:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
