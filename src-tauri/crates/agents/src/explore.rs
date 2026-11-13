//! The assistant's read-only subagent (`assistant_explore`): the CLI run once, without a terminal,
//! in a repository's clone, on a question; its last answer, a synthesis, is all the assistant gets.
//! Claude Code in plan mode with its read tools only, Codex in its read-only sandbox, OpenCode in
//! its plan agent; Command Code has no such mode.

use std::{path::Path, process::Stdio, time::Duration};

use anyhow::{bail, Context, Result};
use tokio::io::AsyncReadExt;

use super::*;

/// At most this much of its answer comes back.
const MAX_CHARS: usize = 12_000;

/// What the subagent is asked, around the assistant's question.
pub fn prompt(repo: &str, question: &str) -> String {
    format!(
        "You explore the repository {repo} (the current folder) for another agent, READ-ONLY: never edit, create nor run anything. Question: {question}\n\nAnswer in under 300 words, as a synthesis: the files and functions concerned (path:line), how it works today, and the leads for the change. No preamble."
    )
}

/// `provider`'s command line for one read-only run on `prompt`; Codex writes its last answer to
/// `out`.
pub fn command(provider: Provider, cli: &config::CliConfig, prompt: &str, out: &Path) -> Result<Vec<String>> {
    let program = provider.program().to_string();
    let argv: Vec<String> = match provider {
        Provider::Claude => [
            program.as_str(),
            "-p",
            "--permission-mode",
            "plan",
            "--allowedTools=Read,Grep,Glob,LS",
            "--disallowedTools=Bash,Edit,MultiEdit,Write,NotebookEdit,WebFetch,WebSearch,Task,Agent",
            "--strict-mcp-config",
            "--settings",
            r#"{"disableAllHooks":true}"#,
            "--output-format",
            "text",
            "--",
            prompt,
        ]
        .map(String::from)
        .to_vec(),
        // Its model and effort after `exec`, whose options they are.
        Provider::Codex => {
            let mut argv = vec![cli.bin.clone().unwrap_or(program), "exec".into()];
            if let Some(m) = cli.model.as_deref().map(str::trim).filter(|m| prism_core::agents::models::valid(m)) {
                argv.extend(["--model".into(), m.into()]);
            }
            if let Some(e) = cli.effort.as_deref().map(str::trim).filter(|e| prism_core::agents::models::valid_effort(e)) {
                argv.extend(["-c".into(), format!("model_reasoning_effort=\"{e}\"")]);
            }
            argv.extend(["--sandbox", "read-only", "--skip-git-repo-check", "--color", "never", "--output-last-message"].map(String::from));
            argv.extend([out.display().to_string(), "--".into(), prompt.into()]);
            return Ok(argv);
        }
        Provider::OpenCode => vec![program, "run".into(), "--agent".into(), "plan".into(), prompt.into()],
        Provider::CommandCode => bail!(tr("Command Code cannot explore a repository on its own: pick another CLI for the assistant ([agents.assistant] provider)")),
    };
    // Its executable, model and effort, without the user's extra options (they may ask a terminal).
    let mut cli = cli.clone();
    cli.args.clear();
    Ok(provider.with_cli(argv, &cli))
}

impl Agents {
    /// Explores `dir` (repository `repo`) on `question` with `provider` (its `account`), read-only;
    /// gives up after `limit`. Its synthesis.
    pub async fn explore(&self, provider: Provider, account: Option<&str>, repo: &str, dir: &Path, question: &str, limit: Duration) -> Result<String> {
        if !dir.is_dir() {
            bail!(tr!("{path} is not a directory", path = dir.display()));
        }
        let cli = self.inner.settings.lock().unwrap().cli(provider).clone();
        let out = std::env::temp_dir().join(format!("prism-explore-{}.txt", uuid::Uuid::new_v4()));
        let argv = command(provider, &cli, &prompt(repo, question), &out)?;
        let (program, args) = argv.split_first().context("no executable")?;
        let mut cmd = platform::command(program);
        cmd.args(args).current_dir(dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        // Its account: the one asked, else the CLI's default one.
        if let Some(var) = provider.home_var()
            && let Some(a) = self.inner.settings.lock().unwrap().account(provider, account).cloned()
            && let Some(home) = provider.own_home(&a.dir)
        {
            cmd.env(var, home);
        }
        let mut child = cmd.spawn().with_context(|| format!("could not run {program}"))?;
        let (mut stdout, mut stderr) = (child.stdout.take().context("no stdout")?, child.stderr.take().context("no stderr")?);
        let read = async {
            let (mut o, mut e) = (String::new(), String::new());
            let _ = tokio::join!(stdout.read_to_string(&mut o), stderr.read_to_string(&mut e));
            let status = child.wait().await;
            (o, e, status)
        };
        let (stdout, stderr, status) = match tokio::time::timeout(limit, read).await {
            Ok(r) => r,
            Err(_) => bail!(tr!("the exploration took more than {s} s: ask a narrower question", s = limit.as_secs())),
        };
        let answer = match std::fs::read_to_string(&out) {
            Ok(text) => {
                let _ = std::fs::remove_file(&out);
                text
            }
            Err(_) => stdout,
        };
        let answer = answer.trim();
        if !status.is_ok_and(|s| s.success()) || answer.is_empty() {
            let why = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no answer").trim().to_string();
            bail!(tr!("the exploration failed: {why}", why = why));
        }
        Ok(if answer.chars().count() > MAX_CHARS { format!("{}…", answer.chars().take(MAX_CHARS).collect::<String>()) } else { answer.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_cli_explores_read_only() {
        let cli = config::CliConfig { model: Some("sonnet".into()), args: vec!["--dangerously-skip-permissions".into()], ..Default::default() };
        let out = Path::new("/tmp/out.txt");
        let claude = command(Provider::Claude, &cli, "where?", out).unwrap();
        assert_eq!(&claude[..4], ["claude", "--model", "sonnet", "-p"]);
        assert!(claude.contains(&"plan".to_string()) && claude.last().unwrap() == "where?");
        assert!(!claude.iter().any(|a| a.contains("dangerously")), "the user's options left out");
        let codex = command(Provider::Codex, &config::CliConfig::default(), "where?", out).unwrap();
        assert_eq!(&codex[..4], ["codex", "exec", "--sandbox", "read-only"]);
        let codex = command(Provider::Codex, &cli, "where?", out).unwrap();
        assert_eq!(&codex[..4], ["codex", "exec", "--model", "sonnet"]);
        assert!(codex.contains(&"/tmp/out.txt".to_string()));
        assert_eq!(command(Provider::OpenCode, &config::CliConfig::default(), "where?", out).unwrap(), ["opencode", "run", "--agent", "plan", "where?"]);
        assert!(command(Provider::CommandCode, &config::CliConfig::default(), "where?", out).is_err());
    }
}
