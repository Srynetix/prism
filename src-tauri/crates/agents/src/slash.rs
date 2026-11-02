//! The slash commands an agent's CLI takes, for the autocomplete of its Chat box, as the CLI itself
//! tells them: Prism keeps no list of its own. Claude Code answers the Agent SDK's `initialize`
//! request in the agent's folder (`claude -p` in stream-json, no prompt sent, so no API call): its
//! own commands, the project's and the user's, skills and plugins, as it resolves them. Codex says
//! its own nowhere: only the user's prompts (`~/.codex/prompts/*.md`, as `prompts:name`) are read.
//! The other CLIs: none. Only names, descriptions and argument hints are kept.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::provider::Provider;
use prism_base::{i18n::tr, platform};

/// Claude Code asked again for a folder after this long.
const KEEP: Duration = Duration::from_secs(300);
/// Claude Code's answer waited for this long.
const WAIT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "SlashCommand"))]
pub struct SlashCommand {
    /// Without its slash: `compact`, `acme-flow:wip`.
    pub name: String,
    pub description: String,
    /// What it takes after its name, if it says.
    pub hint: Option<String>,
    /// `built-in`, a plugin's name, `user`, or `custom` when the CLI does not say.
    pub source: String,
}

type Cache = HashMap<(Vec<String>, PathBuf), (Instant, Vec<SlashCommand>)>;
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The commands of `provider` run as `argv` (its executable and `[agents.<cli>] args`) in `cwd`.
pub async fn list(provider: Provider, argv: Vec<String>, cwd: PathBuf) -> Result<Vec<SlashCommand>> {
    match provider {
        Provider::Claude => {
            let key = (argv, cwd);
            if let Some((at, list)) = CACHE.lock().unwrap().get_or_insert_default().get(&key)
                && at.elapsed() < KEEP
            {
                return Ok(list.clone());
            }
            let list = tokio::time::timeout(WAIT, claude(&key.0, &key.1)).await.context(tr("Claude Code did not answer"))??;
            CACHE.lock().unwrap().get_or_insert_default().insert(key, (Instant::now(), list.clone()));
            Ok(list)
        }
        Provider::Codex => {
            let Some(home) = std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex"))) else { return Ok(Vec::new()) };
            Ok(tokio::task::spawn_blocking(move || codex_prompts(&home.join("prompts"))).await?)
        }
        Provider::OpenCode | Provider::CommandCode => Ok(Vec::new()),
    }
}

/// Claude Code's commands for `cwd`: the `initialize` control request in stream-json, its answer
/// read, the CLI let go (stdin closed, killed on drop). `-p` asks no trust of the folder (a MR
/// branch's `.claude/settings.json`): its hooks off, no MCP server started (theirs only listed
/// commands). Its settings left out (`--setting-sources user`) would leave out its commands too.
async fn claude(argv: &[String], cwd: &Path) -> Result<Vec<SlashCommand>> {
    let (program, args) = argv.split_first().context("no Claude Code executable")?;
    let mut child = platform::command(program)
        .args(args)
        .args(["--settings", r#"{"disableAllHooks":true}"#, "--strict-mcp-config"])
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("could not run {program}"))?;
    let mut stdin = child.stdin.take().context("no stdin")?;
    stdin.write_all(br#"{"type":"control_request","request_id":"prism-slash","request":{"subtype":"initialize"}}"#).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    let mut lines = BufReader::new(child.stdout.take().context("no stdout")?).lines();
    while let Some(line) = lines.next_line().await? {
        if let Some(list) = initialize_answer(&line)? {
            return Ok(list);
        }
    }
    bail!("Claude Code ended without its commands")
}

/// The commands of a stream-json line, if it is the answer to `initialize`.
fn initialize_answer(line: &str) -> Result<Option<Vec<SlashCommand>>> {
    #[derive(Deserialize)]
    struct Line {
        #[serde(rename = "type")]
        kind: String,
        response: Option<Response>,
    }
    #[derive(Deserialize)]
    struct Response {
        request_id: Option<String>,
        subtype: Option<String>,
        error: Option<String>,
        response: Option<Answer>,
    }
    #[derive(Deserialize)]
    struct Answer {
        #[serde(default)]
        commands: Vec<Command>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Command {
        name: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        argument_hint: String,
        #[serde(default)]
        builtin: bool,
    }
    let Ok(Line { kind, response: Some(r) }) = serde_json::from_str::<Line>(line) else { return Ok(None) };
    if kind != "control_response" || r.request_id.as_deref() != Some("prism-slash") {
        return Ok(None);
    }
    if r.subtype.as_deref() == Some("error") {
        bail!("Claude Code: {}", r.error.unwrap_or_default());
    }
    let commands = r.response.map(|a| a.commands).unwrap_or_default();
    Ok(Some(
        commands
            .into_iter()
            // An MCP server's prompt is named with spaces (`claude.ai Figma:… (MCP)`): no word to type.
            .filter(|c| !c.name.is_empty() && !c.name.contains(char::is_whitespace))
            .map(|c| {
                // A plugin's or the user's says so first: `(acme-flow) Scan the repository…`.
                let tagged = c.description.strip_prefix('(').and_then(|d| d.split_once(") "));
                let (source, description) = match tagged {
                    _ if c.builtin => ("built-in".to_string(), c.description.clone()),
                    Some((source, rest)) => (source.to_string(), rest.to_string()),
                    None => ("custom".to_string(), c.description.clone()),
                };
                SlashCommand { name: c.name, description, hint: Some(c.argument_hint).filter(|h| !h.is_empty()), source }
            })
            .collect(),
    ))
}

/// Codex's prompts (`prompts:name`), their description from their frontmatter or first line.
fn codex_prompts(dir: &Path) -> Vec<SlashCommand> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "md")).collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| {
            let name = p.file_stem()?.to_str()?.to_string();
            let (description, hint) = describe(&std::fs::read_to_string(&p).ok()?);
            Some(SlashCommand { name: format!("prompts:{name}"), description, hint, source: "user".into() })
        })
        .collect()
}

/// A prompt file's `description` and `argument-hint` (frontmatter), failing that its first line.
fn describe(text: &str) -> (String, Option<String>) {
    let (mut description, mut hint, mut body) = (None, None, text);
    if let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n"))
        && let Some(end) = rest.find("\n---")
    {
        for line in rest[..end].lines() {
            let Some((k, v)) = line.split_once(':') else { continue };
            let v = v.trim().trim_matches(['"', '\'']).to_string();
            match k.trim() {
                "description" if !v.is_empty() => description = Some(v),
                "argument-hint" if !v.is_empty() => hint = Some(v),
                _ => {}
            }
        }
        body = rest[end + 4..].split_once('\n').map_or("", |(_, b)| b);
    }
    let first = || body.lines().map(|l| l.trim().trim_start_matches('#').trim()).find(|l| !l.is_empty()).unwrap_or_default().to_string();
    (description.unwrap_or_else(first), hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_answer_to_initialize() {
        let line = r#"{"type":"control_response","response":{"subtype":"success","request_id":"prism-slash","response":{"commands":[
            {"name":"compact","description":"Clear the conversation history but keep a summary","argumentHint":"<optional custom summarization instructions>","builtin":true},
            {"name":"acme-flow:wip","description":"(acme-flow) Dashboard of the work in progress","argumentHint":""},
            {"name":"ship","description":"Ship it","argumentHint":""},
            {"name":"claude.ai Figma:rules (MCP)","description":"x","argumentHint":""}
        ],"models":[]}}}"#;
        let list = initialize_answer(line).unwrap().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].source, "built-in");
        assert_eq!(list[0].hint.as_deref(), Some("<optional custom summarization instructions>"));
        assert_eq!((list[1].source.as_str(), list[1].description.as_str(), list[1].hint.as_deref()), ("acme-flow", "Dashboard of the work in progress", None));
        assert_eq!(list[2].source, "custom");
        assert!(initialize_answer(r#"{"type":"system","subtype":"commands_changed","commands":[]}"#).unwrap().is_none());
        assert!(initialize_answer(r#"{"type":"control_response","response":{"subtype":"error","request_id":"prism-slash","error":"nope"}}"#).is_err());
    }

    #[test]
    fn describes_a_prompt_file() {
        assert_eq!(describe("---\ndescription: \"Review\"\nargument-hint: <file>\n---\nbody"), ("Review".into(), Some("<file>".into())));
        assert_eq!(describe("\n# Fix the tests\nmore"), ("Fix the tests".into(), None));
    }

    /// Against the installed Claude Code: `cargo test -p prism-agents slash -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn asks_the_installed_claude_code() {
        let list = list(Provider::Claude, vec!["claude".into()], std::env::temp_dir()).await.unwrap();
        assert!(list.iter().any(|c| c.name == "compact" && c.source == "built-in"));
    }
}
