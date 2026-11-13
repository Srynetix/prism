//! Prism's MCP server for agents: `prism mcp` (stdio, `stdio`) and its other end in the running
//! Prism (`host`), the docs it carries (`docs`), the changes it proposes (`merge`), and its
//! installation into the agent CLIs outside Prism (`install`). The skill, the socket's path and
//! what the window shows of it are in `prism_core::mcp`.

pub mod assistant;
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

/// `prism mcp [aside]`: the server on standard input and output, until the agent closes them.
pub fn serve_stdio(aside: bool) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("a runtime");
    rt.block_on(stdio::run(aside))
}

/// `prism mcp status`: one line per agent CLI, tab-separated.
pub fn status_text() -> String {
    install::status().iter().map(|i| format!("{}\tserver {}\tskill {}{}", i.cli, yes(i.server), yes(i.skill), if i.present { "" } else { "\t(not installed)" })).collect::<Vec<_>>().join("\n")
}

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}
