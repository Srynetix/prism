//! The last scan of each root, saved in the profile's data folder (`disk-usage/<root>.tsv`): the
//! folders of 1 MB or more and the files of 32 MB or more, one per line with its size, its file
//! count, its kind and its Δ. The next scan of that root compares with it (Δ), and it is shown again
//! after a restart, a coarser tree (smaller things summed into their folder).

use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use super::{
    kinds::{Kind, Reclaim},
    scan::{Node, Tree, DIR, REMOVED},
    ScanInfo,
};

const MAGIC: &str = "#prism-disk-usage\t1";
const KEEP_DIR: u64 = 1 << 20;
const KEEP_FILE: u64 = 32 << 20;

fn kept(n: &Node) -> bool {
    n.flags & REMOVED == 0 && n.bytes >= if n.flags & DIR != 0 { KEEP_DIR } else { KEEP_FILE }
}

/// The file of `root`'s scans: its path, readable, then a hash of it (two roots never share one).
fn file_for(dir: &Path, root: &Path) -> PathBuf {
    let text = root.to_string_lossy();
    let slug: String = text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
    // FNV-1a: stable from one run of Prism to the next, unlike std's hasher.
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3));
    dir.join(format!("{slug}-{:08x}.tsv", hash as u32))
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (c, c == '\\') {
            (_, true) => match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            },
            (c, false) => out.push(c),
        }
    }
    out
}

/// Fills `tree`'s Δ from the scan saved for its root (when `delta`), then saves it in its place.
pub fn compare_and_save(dir: &Path, tree: &mut Tree, delta: bool) -> Result<()> {
    let file = file_for(dir, &tree.path);
    let rels = tree.rel_paths();
    if delta
        && let Ok(Some((at, sizes))) = sizes(&file)
    {
        for (n, rel) in tree.nodes.iter_mut().zip(&rels) {
            n.delta = match sizes.get(rel) {
                Some(before) => Some(n.bytes as i64 - *before as i64),
                // Not there before, while big enough to have been saved: new.
                None if kept(n) => Some(n.bytes as i64),
                None => None,
            };
        }
        tree.info.previous_at = Some(at);
    }
    fs::create_dir_all(dir)?;
    let tmp = file.with_extension("tsv.tmp");
    let mut w = BufWriter::new(fs::File::create(&tmp)?);
    let i = &tree.info;
    let previous = i.previous_at.map_or("-".to_string(), |p| p.to_rfc3339());
    writeln!(w, "{MAGIC}\t{}\t{}\t{}\t{}\t{}\t{}\t{previous}", escape(&tree.path.to_string_lossy()), i.at.to_rfc3339(), i.duration_ms, i.dirs, i.denied, escape(&i.denied_paths.join("\n")))?;
    for (n, rel) in tree.nodes.iter().zip(&rels) {
        if !kept(n) && !rel.is_empty() {
            continue;
        }
        let delta = n.delta.map_or("-".to_string(), |d| d.to_string());
        writeln!(w, "{}\t{}\t{}\t{}\t{}\t{delta}\t{}", n.bytes, n.files, n.kind.code(), Reclaim::code(n.reclaim), n.flags, escape(rel))?;
    }
    w.flush()?;
    drop(w);
    fs::rename(&tmp, &file)?;
    Ok(())
}

/// A saved scan's time, and its sizes by path.
type Saved = (DateTime<Utc>, HashMap<String, u64>);

/// The saved scan's time and sizes by path.
fn sizes(file: &Path) -> Result<Option<Saved>> {
    let Ok(f) = fs::File::open(file) else { return Ok(None) };
    let mut lines = BufReader::new(f).lines();
    let Some(head) = lines.next().transpose()? else { return Ok(None) };
    let Some(at) = head.split('\t').nth(3).and_then(|a| DateTime::parse_from_rfc3339(a).ok()) else { return Ok(None) };
    let mut out = HashMap::new();
    for line in lines {
        let line = line?;
        let mut cols = line.splitn(7, '\t');
        if let (Some(bytes), Some(rel)) = (cols.next().and_then(|b| b.parse().ok()), cols.nth(5)) {
            out.insert(unescape(rel), bytes);
        }
    }
    Ok(Some((at.with_timezone(&Utc), out)))
}

/// The most recent scan saved in `dir`, as a tree.
pub fn load_latest(dir: &Path) -> Result<Option<Tree>> {
    let Ok(entries) = fs::read_dir(dir) else { return Ok(None) };
    let latest = entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "tsv")).filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).max_by_key(|(t, _)| *t);
    match latest {
        Some((_, path)) => load(&path).map(Some),
        None => Ok(None),
    }
}

/// A saved scan as a tree; its lines come parents first, a folder's children together.
pub fn load(file: &Path) -> Result<Tree> {
    let f = fs::File::open(file).with_context(|| file.display().to_string())?;
    let mut lines = BufReader::new(f).lines();
    let head = lines.next().transpose()?.context("empty saved scan")?;
    anyhow::ensure!(head.starts_with(MAGIC), "not a saved scan: {}", file.display());
    let h: Vec<&str> = head.split('\t').collect();
    let col = |i: usize| h.get(i).copied().unwrap_or_default();
    let root = PathBuf::from(unescape(col(2)));
    let at = DateTime::parse_from_rfc3339(col(3)).context("saved scan: its time")?.with_timezone(&Utc);
    let mut nodes: Vec<Node> = Vec::new();
    let mut ids: HashMap<String, u32> = HashMap::new();
    for line in lines {
        let line = line?;
        let c: Vec<&str> = line.splitn(7, '\t').collect();
        anyhow::ensure!(c.len() == 7, "saved scan: a short line");
        let rel = unescape(c[6]);
        let (parent, name) = match rel.rsplit_once('/') {
            _ if rel.is_empty() => (0, root.as_os_str().to_os_string()),
            Some((p, n)) => (*ids.get(p).context("saved scan: a child before its folder")?, n.into()),
            None => (0, rel.clone().into()),
        };
        let id = nodes.len() as u32;
        let kind = Kind::from_code(c[2].parse().unwrap_or(0));
        nodes.push(Node {
            name: name.as_os_str().into(),
            parent,
            bytes: c[0].parse().unwrap_or(0),
            files: c[1].parse().unwrap_or(0),
            kind,
            own_kind: kind,
            classed: true,
            reclaim: Reclaim::from_code(c[3].parse().unwrap_or(0)),
            flags: c[4].parse().unwrap_or(0),
            delta: c[5].parse().ok(),
            ..Default::default()
        });
        if id > 0 {
            let p = &mut nodes[parent as usize];
            if p.count == 0 {
                p.first = id;
            }
            p.count += 1;
        }
        ids.insert(rel, id);
    }
    anyhow::ensure!(!nodes.is_empty(), "saved scan without its root");
    // What each folder holds besides its children saved: its own.
    for i in 0..nodes.len() {
        let n = &nodes[i];
        let kids: (u64, u64) = (n.first..n.first + n.count).fold((0, 0), |(b, f), c| (b + nodes[c as usize].bytes, f + nodes[c as usize].files));
        let n = &mut nodes[i];
        n.own = n.bytes.saturating_sub(kids.0);
        n.own_files = n.files.saturating_sub(kids.1);
    }
    let mut tree = Tree { path: root, nodes, info: ScanInfo::default() };
    tree.info = ScanInfo {
        id: at.timestamp_millis() as u64,
        root: super::shorten(&tree.path),
        at,
        duration_ms: col(4).parse().unwrap_or(0),
        bytes: tree.nodes[0].bytes,
        files: tree.nodes[0].files,
        dirs: col(5).parse().unwrap_or(0),
        denied: col(6).parse().unwrap_or(0),
        denied_paths: unescape(col(7)).lines().map(str::to_string).filter(|l| !l.is_empty()).collect(),
        previous_at: DateTime::parse_from_rfc3339(col(8)).ok().map(|p| p.with_timezone(&Utc)),
        saved: true,
        reclaim: Vec::new(),
    };
    tree.info.reclaim = tree.reclaim_groups();
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::AtomicBool, Arc};

    use super::super::scan::{walk, Counters, WalkOptions};
    use super::*;

    #[test]
    fn saved_compared_and_read_back() {
        let base = std::env::temp_dir().join(format!("prism-du-store-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (root, store) = (base.join("root"), base.join("store"));
        fs::create_dir_all(root.join("a\tb/node_modules")).unwrap();
        fs::write(root.join("a\tb/node_modules/x.bin"), vec![1u8; 2 << 20]).unwrap();
        let opts = || WalkOptions { skip: Vec::new(), devices: Vec::new(), stop: Arc::new(AtomicBool::new(false)), counters: Arc::new(Counters::default()) };
        let mut first = walk(&root, 2, &opts());
        first.info.at = Utc::now();
        compare_and_save(&store, &mut first, true).unwrap();
        assert_eq!(first.info.previous_at, None);

        fs::write(root.join("a\tb/node_modules/y.bin"), vec![1u8; 3 << 20]).unwrap();
        let mut second = walk(&root, 2, &opts());
        second.info.at = Utc::now();
        compare_and_save(&store, &mut second, true).unwrap();
        assert!(second.info.previous_at.is_some());
        let grown = second.nodes[0].delta.unwrap();
        assert!(grown >= 3 << 20, "{grown}");

        let back = load_latest(&store).unwrap().unwrap();
        assert!(back.info.saved);
        assert_eq!(back.path, root);
        assert_eq!(back.nodes[0].bytes, second.nodes[0].bytes);
        let nm = back.of_reclaim(Reclaim::NodeModules);
        assert_eq!(nm.len(), 1);
        assert_eq!(back.path(nm[0]), root.join("a\tb/node_modules"));
        assert_eq!(back.info.reclaim[0].kind, Reclaim::NodeModules);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn roots_alike_keep_their_own_file() {
        let d = Path::new("/x");
        assert_ne!(file_for(d, Path::new("/Volumes/My Disk")), file_for(d, Path::new("/Volumes/My_Disk")));
        assert_eq!(file_for(d, Path::new("/a")), file_for(d, Path::new("/a")));
    }
}
