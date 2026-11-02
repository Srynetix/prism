use std::process::Command;

include!("build_shared.rs");

fn main() {
    let prism = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let commit = prism_commit(&prism);
    let extra = extra_blocks(&prism);
    build_label(&commit, &extra);
    tauri_build::build()
}
