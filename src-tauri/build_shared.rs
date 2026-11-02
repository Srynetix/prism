// What the app's build script and the server's (`crates/server`) share: `include!`d by both.

/// The commit Prism (its repository at `prism`) is built from, followed by `+` when tracked files
/// have uncommitted changes (untracked ones are not part of the build): `PRISM_COMMIT`, which the
/// About panel shows, as the splash screen does (web/vite.config.ts). Returned too.
fn prism_commit(prism: &std::path::Path) -> String {
    let git = |args: &[&str]| Command::new("git").arg("-C").arg(prism).args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let dirty = !commit.is_empty() && git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    println!("cargo:rustc-env=PRISM_COMMIT={commit}{}", if dirty { "+" } else { "" });
    println!("cargo:rerun-if-changed={}", prism.join(".git/HEAD").display());
    println!("cargo:rerun-if-changed={}", prism.join(".git/index").display());
    format!("{commit}{}", if dirty { "+" } else { "" })
}

/// `PRISM_BUILD`: this build as feedback and the About panel say it, its commit and its extra
/// blocks' (`5786d86 + acme b2717e7`).
fn build_label(commit: &str, extra: &str) {
    let label = [commit, extra].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" + ");
    println!("cargo:rustc-env=PRISM_BUILD={label}");
}

/// What a build with a team's blocks says after its version: `PRISM_EXTRA_LABEL` as is (a build
/// in a container, without the blocks' git), otherwise their name (`PRISM_EXTRA_NAME`, else their
/// repository's folder) and their commit, `+` with uncommitted changes: `acme 5786d86`. Same
/// as web/vite.config.ts.
fn extra_label(dir: &std::path::Path) -> String {
    println!("cargo:rerun-if-env-changed=PRISM_EXTRA_LABEL");
    println!("cargo:rerun-if-env-changed=PRISM_EXTRA_NAME");
    if let Some(label) = std::env::var("PRISM_EXTRA_LABEL").ok().filter(|l| !l.trim().is_empty()) {
        return label.trim().to_string();
    }
    let git = |args: &[&str]| Command::new("git").arg("-C").arg(dir).args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let top = git(&["rev-parse", "--show-toplevel"]).map(std::path::PathBuf::from);
    if let Some(top) = &top {
        println!("cargo:rerun-if-changed={}", top.join(".git/HEAD").display());
        println!("cargo:rerun-if-changed={}", top.join(".git/index").display());
    }
    let folder = |p: &std::path::Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = std::env::var("PRISM_EXTRA_NAME").ok().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| folder(top.as_deref().unwrap_or(dir)));
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let dirty = !commit.is_empty() && git(&["status", "--porcelain", "--untracked-files=no", "."]).is_some_and(|s| !s.is_empty());
    [name, format!("{commit}{}", if dirty { "+" } else { "" })].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
}

/// The blocks of another repository (`PRISM_EXTRA_BLOCKS`: its folder, absolute or relative to
/// Prism's, `prism`): its `src-tauri/mod.rs` becomes the module `blocks::extra`, which lists them in
/// `BLOCKS`. Without it, none. The label it says after the version (`extra_label`) is returned.
fn extra_blocks(prism: &std::path::Path) -> String {
    println!("cargo:rerun-if-env-changed=PRISM_EXTRA_BLOCKS");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("extra_blocks.rs");
    let (label, code) = match std::env::var("PRISM_EXTRA_BLOCKS").ok().filter(|p| !p.trim().is_empty()) {
        None => (String::new(), "pub fn extra_blocks() -> &'static [&'static dyn Block] {\n    &[]\n}\n".to_string()),
        Some(dir) => {
            let root = prism.join(dir.trim());
            let module = root.join("src-tauri").join("mod.rs");
            let module = module.canonicalize().unwrap_or_else(|e| panic!("PRISM_EXTRA_BLOCKS: no {} ({e})", module.display()));
            println!("cargo:rerun-if-changed={}", module.parent().expect("src-tauri").display());
            (extra_label(&root), format!("#[path = {:?}]\npub mod extra;\n\npub fn extra_blocks() -> &'static [&'static dyn Block] {{\n    &extra::BLOCKS\n}}\n", module.display().to_string()))
        }
    };
    std::fs::write(out, code).expect("extra_blocks.rs");
    label
}
