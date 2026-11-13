//! What `prism mcp` gives an agent without the running Prism, carried in the binary: the help pages
//! (English and French), the commented reference, the packs' example, the build guide, the blocks'
//! SDK and a whole block as a model.

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let root = root.canonicalize().unwrap_or(root);
    let mut help = Vec::new();
    for (lang, dir) in [("en", root.join("web/src/help")), ("fr", root.join("web/src/help/fr"))] {
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut pages: Vec<_> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "md")).collect();
        pages.sort();
        for p in pages {
            let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            help.push(format!("    ({lang:?}, {name:?}, include_str!({:?})),\n", p.display().to_string()));
        }
    }
    let file = |rel: &str| {
        let p = root.join(rel);
        println!("cargo:rerun-if-changed={}", p.display());
        format!("include_str!({:?})", p.display().to_string())
    };
    let example_block: String = [
        "examples/modules/hello/module.toml",
        "examples/modules/hello/backend/src/lib.rs",
        "examples/modules/hello/backend/src/hello.rs",
        "examples/modules/hello/src/defs.ts",
        "examples/modules/hello/src/components.ts",
        "examples/modules/hello/src/Hello.svelte",
        "examples/modules/hello/src/HelloItem.svelte",
    ]
    .iter()
    .map(|rel| format!("    ({rel:?}, {}),\n", file(rel)))
    .collect();
    let code = format!(
        "/// The help pages: language, name, text.\npub static HELP: &[(&str, &str, &str)] = &[\n{}];\npub static EXAMPLE: &str = {};\npub static PACK_EXAMPLE: &str = {};\npub static GUIDE: &str = {};\npub static SDK: &str = {};\npub static WEB_SDK: &str = {};\n/// The example module, whole: its manifest, its back end, its window side.\npub static EXAMPLE_BLOCK: &[(&str, &str)] = &[\n{}];\n",
        help.concat(),
        file("config.example.toml"),
        file("packs/example.toml"),
        file("docs/guide.md"),
        file("src-tauri/crates/module/src/sdk.rs"),
        file("sdk/web/index.d.ts"),
        example_block,
    );
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("mcp_docs.rs");
    std::fs::write(out, code).expect("mcp_docs.rs");
}
