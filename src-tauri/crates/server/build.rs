use std::process::Command;

include!("../../build_shared.rs");

fn main() {
    let prism = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let commit = prism_commit(&prism);
    build_label(&commit);
    page(&prism.join("web/dist"));
}

/// The phone's page, carried in the binary (a server has no app bundle): the files of `web/dist`
/// the remote serves (`remote.html`, `favicon.svg`, `assets/`, `remote/`), built before this crate
/// (`pnpm --dir web build`). Without them, none: the server says so at start.
fn page(dist: &std::path::Path) {
    println!("cargo:rerun-if-changed={}", dist.display());
    let mut files = Vec::new();
    for top in ["remote.html", "favicon.svg", "assets", "remote"] {
        collect(dist, &dist.join(top), &mut files);
    }
    files.sort();
    if files.is_empty() {
        println!("cargo:warning=no {}: this prism-server carries no phone page (pnpm --dir web build first)", dist.display());
    }
    let arms: String = files.iter().map(|(name, path)| format!("        {name:?} => Some(include_bytes!({path:?})),\n")).collect();
    let code = format!("/// A file of the phone's page, by its path in `web/dist`.\npub fn asset(path: &str) -> Option<&'static [u8]> {{\n    match path {{\n{arms}        _ => None,\n    }}\n}}\n");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("page.rs");
    std::fs::write(out, code).expect("page.rs");
}

fn collect(dist: &std::path::Path, path: &std::path::Path, files: &mut Vec<(String, String)>) {
    if path.is_dir() {
        println!("cargo:rerun-if-changed={}", path.display());
        for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
            collect(dist, &entry.path(), files);
        }
    } else if path.is_file()
        && let (Ok(name), Ok(abs)) = (path.strip_prefix(dist), path.canonicalize())
    {
        files.push((name.to_string_lossy().replace('\\', "/"), abs.to_string_lossy().into_owned()));
    }
}
