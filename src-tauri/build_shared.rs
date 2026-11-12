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

/// `PRISM_BUILD`: this build as feedback and the About panel say it, its commit (`5786d86`). A
/// build still given a team's blocks to compile in stops: they are modules now.
fn build_label(commit: &str) {
    println!("cargo:rerun-if-env-changed=PRISM_EXTRA_BLOCKS");
    if std::env::var("PRISM_EXTRA_BLOCKS").is_ok_and(|v| !v.trim().is_empty()) {
        panic!("PRISM_EXTRA_BLOCKS is set: Prism no longer compiles a team's blocks in, they are modules of its pack (Help › Write a module). Remove it from the environment or .env.");
    }
    println!("cargo:rustc-env=PRISM_BUILD={commit}");
}
