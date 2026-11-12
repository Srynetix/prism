//! The example module's component (`examples/modules/hello/backend`), run as Prism runs it:
//! built first (`just module-example`), otherwise these tests say so and pass.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use prism_core::blocks::{BlockCtx, BlockEnv, BlockRepo};
use prism_core::state::Hub;
use serde_json::json;

use crate::worker::Module;

fn example() -> Option<PathBuf> {
    let wasm = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../examples/modules/hello/backend/target/wasm32-wasip2/release/prism_module_hello.wasm");
    if wasm.is_file() {
        return Some(wasm);
    }
    eprintln!("no {}: `just module-example` builds it", wasm.display());
    None
}

/// A folder of its own: the tests run side by side.
fn temp(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("prism-modules-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The example module, its manifest as `module.toml` declares it.
fn module(wasm: &PathBuf, capabilities: &str) -> Arc<Module> {
    let folder = temp("module");
    std::fs::copy(wasm, folder.join("backend.wasm")).unwrap();
    let spec = crate::manifest::parse(&folder, &format!("[wasm]\ncomponent = \"backend.wasm\"\ntables = [\"hello\"]\ncapabilities = {capabilities}\n[[wasm.blocks]]\nid = \"hello\"\nparts = [\"hello_reads\"]\nrepos = \"one\"\nreloads = true\nprovides = [\"stages\", \"images\"]\nremote = {{ ping = \"read\" }}\n")).unwrap();
    let module = Arc::new(Module::new("hello".into(), spec));
    module.warm();
    module
}

fn env(clone: &std::path::Path) -> BlockEnv {
    BlockEnv {
        forge: Default::default(),
        code: true,
        repos: vec![BlockRepo { name: "app".into(), project: Some("g/app".into()), clone: Some(clone.to_path_buf()) }],
        project: Some("g/app".into()),
        clone: Some(clone.to_path_buf()),
        gitlab: Default::default(),
        clones: vec![],
    }
}

fn wait_for(hub: &Hub, what: &str, f: impl Fn(&prism_core::state::Snapshot) -> bool) {
    let t0 = Instant::now();
    while !f(&hub.subscribe().borrow()) {
        assert!(t0.elapsed() < Duration::from_secs(60), "{what}: not within 60 s");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(dir).args(args).env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t").output().unwrap();
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_example_fills_its_block_and_survives_its_panic() {
    let Some(wasm) = example() else { return };
    let clone = temp("clone");
    git(&clone, &["init", "-q"]);
    git(&clone, &["commit", "-q", "--allow-empty", "-m", "First of all"]);
    let module = module(&wasm, "{ git = true }");
    let hub = Hub::new();
    let m = module.clone();
    let (h, c) = (hub.clone(), clone.clone());
    tokio::task::spawn_blocking(move || {
        let config: toml::Table = toml::from_str("[hello]\nevery = 5\n[secrets]\ntoken = \"not for the module\"\n").unwrap();
        assert!(!m.config_text(&config).contains("not for the module"), "only its tables");
        m.configure(&config).unwrap();
        // The stages it declares, read by the core before anything starts.
        assert_eq!(m.shared.lock().unwrap().stages.as_ref().map(|s| s.production().to_string()).as_deref(), Some("prod"));
        let ctx = BlockCtx::new("hello", h.clone(), Arc::new(env(&c)));
        m.start("hello", ctx, crate::block::env_json("hello", &env(&c))).unwrap();
    })
    .await
    .unwrap();
    // Its first read: its repository's last commit, through git in its clone.
    wait_for(&hub, "its state", |s| s.blocks.get("hello").is_some_and(|v| v["commit"] == "First of all"));
    assert_eq!(hub.subscribe().borrow().errors.get("hello_reads"), None);
    // A call from the window, then the images Vulnerabilities reads.
    assert_eq!(module.call("hello", "ping".into(), json!({}), false).await.unwrap()["pong"], 1);
    let images = module.images(clone.clone(), "g/app".into(), vec![]).await.unwrap();
    assert_eq!(images[0].tags[0].tag, "alpine:3.20");
    // A panic: the block's error says it, Prism goes on; the component starts again later.
    let e = module.call("hello", "panic".into(), json!({}), false).await.unwrap_err().to_string();
    assert!(e.contains("panicked on purpose"), "{e}");
    wait_for(&hub, "the block's error", |s| s.errors.get("hello").is_some_and(|e| e.contains("panicked on purpose")));
    assert!(module.call("hello", "ping".into(), json!({}), false).await.unwrap_err().to_string().contains("starting again"));
    // Started again: configured and started as it was, its error gone, reading anew.
    wait_for(&hub, "its restart", |s| !s.errors.contains_key("hello") && s.blocks.get("hello").is_some_and(|v| v["reads"] == 1));
    assert_eq!(module.call("hello", "ping".into(), json!({}), false).await.unwrap()["pong"], 1);
    std::fs::remove_dir_all(clone).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn without_git_its_read_says_so() {
    let Some(wasm) = example() else { return };
    let clone = temp("nogit");
    let module = module(&wasm, "{}");
    let hub = Hub::new();
    let (m, h, c) = (module.clone(), hub.clone(), clone.clone());
    tokio::task::spawn_blocking(move || {
        m.configure(&toml::Table::new()).unwrap();
        m.start("hello", BlockCtx::new("hello", h, Arc::new(env(&c))), crate::block::env_json("hello", &env(&c))).unwrap();
    })
    .await
    .unwrap();
    wait_for(&hub, "the part's error", |s| s.errors.get("hello_reads").is_some_and(|e| e.contains("not among this module's capabilities")));
    std::fs::remove_dir_all(clone).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn its_demo_comes_from_the_component() {
    let Some(wasm) = example() else { return };
    let module = module(&wasm, "{}");
    let m = module.clone();
    let state = tokio::task::spawn_blocking(move || m.demo("hello")).await.unwrap().unwrap();
    assert_eq!(state["reads"], 42);
    assert_eq!(module.call("hello", "ping".into(), json!({}), true).await.unwrap()["demo"], true);
    // Changing nothing in the demo: no panic either.
    assert!(module.call("hello", "panic".into(), json!({}), true).await.is_err());
}

#[test]
fn the_second_load_comes_from_the_cache() {
    let Some(wasm) = example() else { return };
    let cache = temp("cache");
    crate::cache::component_in(&cache, &wasm).unwrap();
    let t0 = Instant::now();
    crate::cache::component_in(&cache, &wasm).unwrap();
    let took = t0.elapsed();
    assert!(took < Duration::from_millis(50), "from the cache in {took:?}");
    std::fs::remove_dir_all(cache).unwrap();
}
