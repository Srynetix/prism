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

/// The example module, its manifest as `module.toml` declares it, trusted.
fn module(wasm: &PathBuf, capabilities: &str) -> Arc<Module> {
    module_trusted(wasm, capabilities, true)
}

fn module_trusted(wasm: &PathBuf, capabilities: &str, trusted: bool) -> Arc<Module> {
    let folder = temp("module");
    std::fs::copy(wasm, folder.join("backend.wasm")).unwrap();
    let spec = crate::manifest::parse(&folder, &format!("[wasm]\ncomponent = \"backend.wasm\"\ntables = [\"hello\"]\ncapabilities = {capabilities}\n[[wasm.blocks]]\nid = \"hello\"\nparts = [\"hello_reads\"]\nrepos = \"one\"\nreloads = true\nprovides = [\"stages\", \"images\"]\nremote = {{ ping = \"read\" }}\n")).unwrap();
    let module = Arc::new(Module::new("hello".into(), spec, prism_core::modules::stopped_mark(&folder, "hello"), trusted.then(|| std::fs::read(wasm).unwrap())));
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
    let before = hub.subscribe().borrow().blocks["hello"]["at"].clone();
    let e = module.call("hello", "panic".into(), json!({}), false).await.unwrap_err().to_string();
    assert!(e.contains("panicked on purpose"), "{e}");
    wait_for(&hub, "the block's error", |s| s.errors.get("hello").is_some_and(|e| e.contains("panicked on purpose")));
    assert!(module.call("hello", "ping".into(), json!({}), false).await.unwrap_err().to_string().contains("starting again"));
    // Started again: configured and started as it was, its error gone, reading anew.
    // (Its state read anew: the one before the panic said 1 too.)
    wait_for(&hub, "its restart", |s| !s.errors.contains_key("hello") && s.blocks.get("hello").is_some_and(|v| v["reads"] == 1 && v["at"] != before));
    assert_eq!(module.call("hello", "ping".into(), json!({}), false).await.unwrap()["pong"], 1);
    std::fs::remove_dir_all(clone).unwrap();
}

/// A call's deadline (3 s in tests) counts its own computing, not what Prism does for it.
#[tokio::test(flavor = "multi_thread")]
async fn its_deadline_counts_its_own_time() {
    let Some(wasm) = example() else { return };
    let clone = temp("deadline");
    git(&clone, &["init", "-q"]);
    git(&clone, &["commit", "-q", "--allow-empty", "-m", "First of all"]);
    let module = module(&wasm, "{ git = true }");
    let (m, h, c) = (module.clone(), Hub::new(), clone.clone());
    tokio::task::spawn_blocking(move || {
        m.configure(&toml::Table::new()).unwrap();
        m.start("hello", BlockCtx::new("hello", h, Arc::new(env(&c))), crate::block::env_json("hello", &env(&c))).unwrap();
    })
    .await
    .unwrap();
    // Waiting 5 s for git: past the deadline in time, not in its own.
    let waited = module.call("hello", "wait".into(), json!({ "repo": clone, "secs": 5 }), false).await.unwrap();
    assert!(waited["reads"].as_u64().unwrap() > 1, "{waited}");
    // Computing 5 s: stopped.
    let e = module.call("hello", "spin".into(), json!({ "secs": 5 }), false).await.unwrap_err().to_string();
    assert!(e.contains("computing longer than 3 s"), "{e}");
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
    let t0 = Instant::now();
    crate::cache::component_in(&cache, &wasm).unwrap();
    let compiled = t0.elapsed();
    let t0 = Instant::now();
    crate::cache::component_in(&cache, &wasm).unwrap();
    let took = t0.elapsed();
    // Against its compiling, not a clock: the machine may be busy.
    assert!(took * 4 < compiled, "from the cache in {took:?}, compiled in {compiled:?}");
    std::fs::remove_dir_all(cache).unwrap();
}

/// The example's block, the same file as in its component, compiled into Prism: the module SDK
/// gives a block the names Prism's `blocks::sdk` does.
#[test]
fn the_example_block_compiles_inside_prism_too() {
    use prism_core::blocks::Block;
    let hello = &crate::hello_inside_prism::HELLO;
    assert_eq!((hello.id(), hello.parts()), ("hello", &["hello_reads"][..]));
    assert_eq!(hello.stages().unwrap().stages().production(), "prod");
    assert_eq!(hello.demo(chrono::Utc::now()).unwrap()["reads"], 42);
}

/// The data a block shares with the core reads the same on both sides (`types.rs` of the SDK,
/// generated by scripts/module-sdk-types.py).
#[test]
fn the_sdk_types_read_the_cores_json() {
    use prism_core::alerts::{Alert, AlertRule, Fix, OpenTarget, Severity};
    use prism_core::changes::{ChangeEvent, ChangeKind, ChangeLink, ChangeMr, ChangeSeverity};
    fn same<A: serde::Serialize, B: serde::Serialize + serde::de::DeserializeOwned>(a: &A) {
        let core = serde_json::to_value(a).unwrap();
        let sdk: B = serde_json::from_value(core.clone()).unwrap_or_else(|e| panic!("{e}: {core}"));
        assert_eq!(serde_json::to_value(&sdk).unwrap(), core);
    }
    let alert = Alert {
        kind: "Deploy".into(),
        block: Some("hello".into()),
        item: None,
        notification: Some(prism_core::alerts::Notification { key: "k".into(), title: "t".into() }),
        severity: Severity::Ko,
        text: "failed".into(),
        url: Some("https://example.org".into()),
        open: Some(OpenTarget::Pipeline { pipeline: 3, label: "p".into(), project: Some("g/p".into()) }),
        notify: Some(true),
        fixes: Some(vec![Fix::Retry { pipeline: 3, label: "Retry".into(), project: None }, Fix::Block { block: "hello".into(), command: "ping".into(), args: json!({ "a": 1 }), label: "Ping".into(), title: "t".into() }]),
        answer: None,
        rule: Some(AlertRule::Block),
        since: Some(chrono::Utc::now()),
        short: Some("s".into()),
    };
    same::<_, prism_module::sdk::Alert>(&alert);
    let mut change = ChangeEvent::new("id", chrono::Utc::now(), "hello", ChangeKind::Rollout, ChangeSeverity::Warn, "Rolled out");
    change.link = Some(ChangeLink::Mr { repo: Some("app".into()), iid: 4 });
    change.mr = Some(ChangeMr { repo: None, iid: 4, title: "MR".into(), url: None });
    change.workloads = vec!["api".into()];
    same::<_, prism_module::sdk::ChangeEvent>(&change);
    same::<_, prism_module::sdk::Stage>(&prism_core::stages::Stage { role: "production".into(), name: "prod".into(), env: "prod".into() });
    same::<_, prism_module::sdk::conventions::Conventions>(&prism_core::conventions::Conventions::default());
}

/// Not trusted: its block says it waits, nothing of it runs; trusted, it starts as it waited.
#[tokio::test(flavor = "multi_thread")]
async fn an_untrusted_module_waits_then_runs_once_trusted() {
    let Some(wasm) = example() else { return };
    let clone = temp("waits");
    git(&clone, &["init", "-q"]);
    git(&clone, &["commit", "-q", "--allow-empty", "-m", "Waited"]);
    let module = module_trusted(&wasm, "{ git = true }", false);
    let hub = Hub::new();
    let (m, h, c) = (module.clone(), hub.clone(), clone.clone());
    tokio::task::spawn_blocking(move || {
        m.configure(&toml::from_str("[hello]\nevery = 5\n").unwrap()).unwrap();
        m.start("hello", BlockCtx::new("hello", h, Arc::new(env(&c))), crate::block::env_json("hello", &env(&c))).unwrap();
    })
    .await
    .unwrap();
    wait_for(&hub, "its waiting", |s| s.errors.get("hello").is_some_and(|e| e.contains("waits for your trust")));
    assert!(!hub.subscribe().borrow().blocks.contains_key("hello"), "nothing of it ran");
    assert!(module.call("hello", "ping".into(), json!({}), false).await.unwrap_err().to_string().contains("trust"));
    let m = module.clone();
    let w = wasm.clone();
    tokio::task::spawn_blocking(move || assert!(m.trust_as(m.spec.clone(), std::fs::read(w).unwrap()))).await.unwrap();
    wait_for(&hub, "its first read, trusted", |s| s.blocks.get("hello").is_some_and(|v| v["commit"] == "Waited") && !s.errors.contains_key("hello"));
    std::fs::remove_dir_all(clone).unwrap();
}

/// Trusted, it runs as the version trusted: what that one declares it may reach, not what the
/// manifest read at launch said; a folder that is not that version is not run.
#[tokio::test(flavor = "multi_thread")]
async fn it_runs_as_the_version_trusted() {
    let Some(wasm) = example() else { return };
    let clone = temp("as-trusted");
    git(&clone, &["init", "-q"]);
    git(&clone, &["commit", "-q", "--allow-empty", "-m", "x"]);
    // At launch, a version that declared git; trusted, the one shown declares nothing.
    let module = module_trusted(&wasm, "{ git = true }", false);
    let mut shown = module.spec.clone();
    shown.capabilities = Default::default();
    let hub = Hub::new();
    let (m, h, c, w) = (module.clone(), hub.clone(), clone.clone(), wasm.clone());
    tokio::task::spawn_blocking(move || {
        m.configure(&toml::Table::new()).unwrap();
        m.start("hello", BlockCtx::new("hello", h, Arc::new(env(&c))), crate::block::env_json("hello", &env(&c))).unwrap();
        assert!(m.trust_as(shown, std::fs::read(w).unwrap()));
    })
    .await
    .unwrap();
    wait_for(&hub, "git refused: not the trusted version's", |s| s.errors.get("hello_reads").is_some_and(|e| e.contains("not among this module's capabilities")));
    // Another folder than the version trusted: not read.
    let folder = temp("other");
    std::fs::write(folder.join("module.toml"), "[wasm]\ncomponent = \"b.wasm\"\n[[wasm.blocks]]\nid = \"hello\"\n").unwrap();
    std::fs::write(folder.join("b.wasm"), b"x").unwrap();
    assert!(crate::worker::load_trusted(&folder, "not its fingerprint").unwrap_err().to_string().contains("trust it again"));
    let f = prism_core::modules::fingerprint_of(&folder);
    assert!(crate::worker::load_trusted(&folder, &f).is_ok());
    for d in [clone, folder] {
        std::fs::remove_dir_all(d).unwrap();
    }
}
