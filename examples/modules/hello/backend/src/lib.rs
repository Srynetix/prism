//! The back end of the example module `hello`: a WebAssembly component Prism runs in a sandbox
//! (`src-tauri/crates/modules`). It reads its repository's last commit with git (a capability its
//! `module.toml` declares), counts its reads, answers `ping` and, on purpose, panics on `panic`:
//! Prism says so on its block and starts it again. It declares stages and an image too, read by
//! Environments and Vulnerabilities when `[clusters] stages` / `[vulnerabilities] images` name
//! `hello`.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{json, Value};

wit_bindgen::generate!({ world: "block", path: "../../../../src-tauri/crates/modules/wit" });

use prism::module::host;
use prism::module::types::Published;

/// `[hello]` of Prism's config: the only table its manifest asks for.
#[derive(Deserialize)]
#[serde(default)]
struct Settings {
    /// Seconds between two reads.
    every: u32,
    /// The image Vulnerabilities scans for it.
    image: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { every: 30, image: "alpine:3.20".into() }
    }
}

#[derive(Default)]
struct State {
    settings: Option<Settings>,
    lang: String,
    clone: Option<String>,
    project: Option<String>,
    reads: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    f(STATE.lock().unwrap().get_or_insert_with(State::default))
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

struct Hello;

impl Guest for Hello {
    fn configure(config: String, lang: String, demo: bool) -> Result<Configured, String> {
        #[derive(Deserialize, Default)]
        struct Config {
            #[serde(default)]
            hello: Settings,
        }
        let config: Config = toml::from_str(&config).map_err(|e| format!("[hello]: {e}"))?;
        let every = config.hello.every;
        with(|s| {
            s.settings = Some(config.hello);
            s.lang = lang;
        });
        host::log(2, &format!("configured: a read every {every} s"));
        let stages = json!([
            { "role": "staging", "name": "recette", "env": "staging" },
            { "role": "production", "name": "production", "env": "prod" },
        ]);
        let demo = if demo { vec![("hello".to_string(), json!({ "reads": 42, "at": now_ms(), "commit": "Add the checkout page", "project": "example-org/shop" }).to_string())] } else { vec![] };
        Ok(Configured { parts: vec!["hello_reads".into()], stages: Some(stages.to_string()), demo })
    }

    fn start(env: String) -> Result<(), String> {
        let env: Value = serde_json::from_str(&env).map_err(|e| e.to_string())?;
        with(|s| {
            s.clone = env["clone"].as_str().map(str::to_string);
            s.project = env["project"].as_str().map(str::to_string);
        });
        Ok(())
    }

    fn tick(part: String) -> Tick {
        if part != "hello_reads" {
            return Tick { published: vec![], error: Some(format!("no part {part}")), next: 0 };
        }
        let (clone, project, reads, every) = with(|s| {
            s.reads += 1;
            (s.clone.clone(), s.project.clone(), s.reads, s.settings.as_ref().map_or(30, |c| c.every))
        });
        // Its repository's last commit: git, in its own clone only.
        let (commit, error) = match clone {
            Some(clone) => match host::git(&clone, &["log".into(), "-1".into(), "--format=%s".into()]) {
                Ok(out) => (Some(out.trim().to_string()), None),
                Err(e) => (None, Some(e)),
            },
            None => (None, None),
        };
        let state = json!({ "reads": reads, "at": now_ms(), "commit": commit, "project": project });
        Tick { published: vec![Published { block: "hello".into(), state: state.to_string(), alerts: None, changes: None }], error, next: every.max(5) }
    }

    fn call(_block: String, command: String, _args: String, demo: bool) -> Result<String, String> {
        match command.as_str() {
            "ping" => Ok(json!({ "pong": with(|s| s.reads), "demo": demo }).to_string()),
            // On purpose: Prism says so on the block, and starts the component again.
            "panic" if !demo => panic!("hello panicked on purpose"),
            other => Err(format!("no command {other}")),
        }
    }

    fn hints(_repo: String) -> Result<String, String> {
        Ok(json!({ "apps": [], "expected": [] }).to_string())
    }

    fn images(_repo: String, _project: String, _local: String) -> Result<String, String> {
        let image = with(|s| s.settings.as_ref().map_or_else(|| Settings::default().image, |c| c.image.clone()));
        Ok(json!([{ "label": format!("hello · {image}"), "tags": [{ "tag": image, "shown": image, "at": [{ "image": image, "gcp": false }] }] }]).to_string())
    }
}

export!(Hello);
