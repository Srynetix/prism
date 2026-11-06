//! Prism's MCP server for agents: `prism mcp` (stdio, `stdio`) and its other end in the running
//! Prism (`host`), the docs it carries (`docs`), the changes it proposes (`merge`), and its
//! installation into the agent CLIs outside Prism (`install`). The skill, the socket's path and
//! what the window shows of it are in `prism_core::mcp`.

pub mod docs;
pub mod host;
pub mod install;
pub mod merge;
pub mod stdio;

/// The config's schema, with the blocks of this build: computed once.
pub fn schema() -> &'static serde_json::Value {
    static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(prism_core::schema::config)
}

/// `prism mcp [install <cli> | uninstall <cli> | status]`: its exit code.
pub fn cli(args: &[std::ffi::OsString]) -> i32 {
    let arg = |i: usize| args.get(i).and_then(|a| a.to_str()).unwrap_or_default().to_string();
    let done: anyhow::Result<Option<String>> = match arg(2).as_str() {
        "" | "aside" => {
            let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("a runtime");
            rt.block_on(stdio::run(arg(2) == "aside")).map(|()| None)
        }
        "install" => install::install(&arg(3)).map(Some),
        "uninstall" => install::uninstall(&arg(3)).map(Some),
        "status" => Ok(Some(install::status().iter().map(|i| format!("{}\tserver {}\tskill {}{}", i.cli, yes(i.server), yes(i.skill), if i.present { "" } else { "\t(not installed)" })).collect::<Vec<_>>().join("\n"))),
        _ => Err(anyhow::anyhow!("usage: prism mcp [aside] | install <claude|codex|opencode> | uninstall <cli> | status")),
    };
    match done {
        Ok(Some(msg)) => {
            println!("{msg}");
            0
        }
        Ok(None) => 0,
        Err(e) => {
            eprintln!("prism: {e:#}");
            1
        }
    }
}

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}
