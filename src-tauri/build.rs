use std::process::Command;

include!("build_shared.rs");

fn main() {
    let prism = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let commit = prism_commit(&prism);
    build_label(&commit);
    tauri_build::build()
}
