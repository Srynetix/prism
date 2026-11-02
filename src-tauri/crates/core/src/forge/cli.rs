//! The forges' command-line tools, `glab` and `gh`, through which Prism reaches GitLab and
//! GitHub: they handle authentication (the keychain), the token never passes through Prism.

use prism_base::tr;
use std::ffi::OsStr;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Deserialize};
use tokio::process::Command;

/// The longest a call may take: a tool stuck on a prompt or the network would otherwise hold
/// its poll loop for good. Generous, for a push (`gh repo create --push`) or a release download.
const TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cli {
    Glab,
    Gh,
}

impl Cli {
    fn name(self) -> &'static str {
        match self {
            Cli::Glab => "glab",
            Cli::Gh => "gh",
        }
    }

    /// The tool, without prompts; `glab` pointed at the config's instance (`[gitlab] host`).
    pub fn command(self) -> Command {
        match self {
            Cli::Glab => self.command_on(&crate::conventions::get().gitlab_host),
            Cli::Gh => {
                let mut cmd = crate::platform::command("gh");
                cmd.env("GH_PROMPT_DISABLED", "1").env("GH_NO_UPDATE_NOTIFIER", "1");
                cmd
            }
        }
    }

    /// `glab` on another instance than the config's (a release's, a pack's); `gh` as `command`.
    pub fn command_on(self, host: &str) -> Command {
        match self {
            Cli::Glab => {
                let mut cmd = crate::platform::command("glab");
                if !host.is_empty() {
                    cmd.env("GITLAB_HOST", host);
                }
                cmd
            }
            Cli::Gh => self.command(),
        }
    }

    /// Runs `cmd` (this tool, with its arguments): its standard output. An error names the call
    /// (`glab api`, `gh graphql`); a GraphQL error, which makes the tool fail with the response on
    /// its output, gives its own message, which says more.
    pub async fn output<S: AsRef<OsStr>>(self, mut cmd: Command, args: &[S]) -> Result<Vec<u8>> {
        #[cfg(feature = "testing")]
        if let Some(answer) = fake::answer(self, args, None) {
            return answer;
        }
        let run = cmd.args(args).stdin(std::process::Stdio::null()).kill_on_drop(true).output();
        let out = tokio::time::timeout(TIMEOUT, run).await.with_context(|| tr!("{cli} did not answer within {s} s", cli = self.name(), s = TIMEOUT.as_secs()))?.with_context(|| format!("{} not found in PATH", self.name()))?;
        if !out.status.success() {
            if let Ok(Graphql::<serde_json::Value> { errors: Some(errors), .. }) = serde_json::from_slice(&out.stdout) {
                bail!("GraphQL: {}", messages(errors));
            }
            let call = args.get(1).or(args.first()).map(|a| a.as_ref().to_string_lossy().into_owned()).unwrap_or_default();
            bail!("{} {call}: {}", self.name(), String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(out.stdout)
    }

    /// `output` of the tool on the config's instance.
    pub async fn run<S: AsRef<OsStr>>(self, args: &[S]) -> Result<Vec<u8>> {
        self.output(self.command(), args).await
    }

    /// `run`, read as JSON; an empty answer (a 204) reads as `null`.
    pub async fn json<T: DeserializeOwned, S: AsRef<OsStr>>(self, args: &[S]) -> Result<T> {
        parse(self, &self.run(args).await?)
    }

    /// An `api` call with a JSON body on standard input (`--input -`): the way to send nested
    /// objects and lists (a comment's position on the diff, assignees).
    pub async fn input<T: DeserializeOwned>(self, method: &str, path: &str, body: &serde_json::Value) -> Result<T> {
        #[cfg(feature = "testing")]
        if let Some(answer) = fake::answer(self, &["api", "--method", method, path, "--input", "-"], Some(body)) {
            return parse(self, &answer?);
        }
        self.send(method, path, "application/json", &serde_json::to_vec(body)?).await
    }

    /// An `api` call with `body` as it is on standard input, of type `content_type`: a file sent
    /// as `multipart/form-data` (an upload), which the tool passes on byte for byte.
    pub async fn send<T: DeserializeOwned>(self, method: &str, path: &str, content_type: &str, body: &[u8]) -> Result<T> {
        use tokio::io::AsyncWriteExt;
        #[cfg(feature = "testing")]
        if let Some(answer) = fake::answer(self, &["api", "--method", method, path, "--input", "-"], None) {
            return parse(self, &answer?);
        }
        let mut child = self
            .command()
            .args(["api", "--method", method, path, "--input", "-", "-H", &format!("Content-Type: {content_type}")])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("{} not found in PATH", self.name()))?;
        let mut stdin = child.stdin.take().with_context(|| format!("{} stdin", self.name()))?;
        stdin.write_all(body).await?;
        drop(stdin);
        let out = tokio::time::timeout(TIMEOUT, child.wait_with_output()).await.with_context(|| tr!("{cli} did not answer within {s} s", cli = self.name(), s = TIMEOUT.as_secs()))??;
        if !out.status.success() {
            bail!("{} api: {}", self.name(), String::from_utf8_lossy(&out.stderr).trim());
        }
        parse(self, &out.stdout)
    }
}

fn parse<T: DeserializeOwned>(cli: Cli, out: &[u8]) -> Result<T> {
    let out = if out.iter().all(u8::is_ascii_whitespace) { b"null".as_slice() } else { out };
    serde_json::from_slice(out).with_context(|| format!("unreadable {} response", cli.name()))
}

/// A GraphQL answer: its data, or its errors.
#[derive(Deserialize)]
pub struct Graphql<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<GraphqlError>>,
}

#[derive(Deserialize)]
pub struct GraphqlError {
    pub message: String,
}

impl<T> Graphql<T> {
    pub fn into_data(self) -> Result<T> {
        if let Some(errors) = self.errors {
            bail!("GraphQL: {}", messages(errors));
        }
        self.data.context("empty GraphQL response")
    }
}

fn messages(errors: Vec<GraphqlError>) -> String {
    errors.into_iter().map(|e| e.message).collect::<Vec<_>>().join("; ")
}

/// The tools answered from memory, for the tests of what calls them: within `fake::Replay::run`,
/// a call gets the answer of the last rule it matches, or an error naming it; calls are kept.
#[cfg(feature = "testing")]
pub mod fake {
    use std::{
        ffi::OsStr,
        future::Future,
        sync::{Arc, Mutex},
    };

    use anyhow::{anyhow, Result};

    use super::Cli;

    /// A call made: the tool, its arguments, its body on standard input.
    #[derive(Debug, Clone)]
    pub struct Call {
        pub cli: Cli,
        pub args: Vec<String>,
        pub body: Option<serde_json::Value>,
    }

    impl Call {
        /// The call's arguments, joined by spaces: what tests look into.
        pub fn line(&self) -> String {
            self.args.join(" ")
        }
    }

    struct Rule {
        cli: Cli,
        /// Each must be found in the arguments (`merge_requests/12/merge`, `--method PUT`…).
        needles: Vec<String>,
        answer: Result<Vec<u8>, String>,
    }

    #[derive(Default)]
    pub struct Replay {
        rules: Mutex<Vec<Rule>>,
        calls: Mutex<Vec<Call>>,
    }

    tokio::task_local! {
        static REPLAY: Arc<Replay>;
    }

    impl Replay {
        /// Answers `answer` (JSON) to the calls of `cli` whose arguments, joined, contain every needle.
        pub fn on(&self, cli: Cli, needles: &[&str], answer: serde_json::Value) -> &Self {
            self.rule(cli, needles, Ok(serde_json::to_vec(&answer).unwrap()))
        }

        /// Fails those calls with `message`, as the tool would on its standard error.
        pub fn fail(&self, cli: Cli, needles: &[&str], message: &str) -> &Self {
            self.rule(cli, needles, Err(message.to_string()))
        }

        fn rule(&self, cli: Cli, needles: &[&str], answer: Result<Vec<u8>, String>) -> &Self {
            self.rules.lock().unwrap().push(Rule { cli, needles: needles.iter().map(|n| n.to_string()).collect(), answer });
            self
        }

        pub fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }

        /// Runs `f` with the tools answered from this replay.
        pub async fn run<F: Future>(self: &Arc<Self>, f: F) -> F::Output {
            REPLAY.scope(self.clone(), f).await
        }
    }

    pub(super) fn answer<S: AsRef<OsStr>>(cli: Cli, args: &[S], body: Option<&serde_json::Value>) -> Option<Result<Vec<u8>>> {
        let args: Vec<String> = args.iter().map(|a| a.as_ref().to_string_lossy().into_owned()).collect();
        REPLAY
            .try_with(|replay| {
                let line = args.join(" ");
                replay.calls.lock().unwrap().push(Call { cli, args, body: body.cloned() });
                let rules = replay.rules.lock().unwrap();
                match rules.iter().rev().find(|r| r.cli == cli && r.needles.iter().all(|n| line.contains(n.as_str()))) {
                    Some(r) => r.answer.clone().map_err(|e| anyhow!("{} {line}: {e}", cli.name())),
                    None => Err(anyhow!("fake {}: no answer for `{line}`", cli.name())),
                }
            })
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphql_errors_come_before_the_data() {
        let ok: Graphql<u32> = serde_json::from_str(r#"{"data": 3}"#).unwrap();
        assert_eq!(ok.into_data().unwrap(), 3);
        let ko: Graphql<u32> = serde_json::from_str(r#"{"data": null, "errors": [{"message": "a"}, {"message": "b"}]}"#).unwrap();
        assert_eq!(ko.into_data().unwrap_err().to_string(), "GraphQL: a; b");
    }

    #[test]
    fn an_empty_answer_reads_as_null() {
        assert_eq!(parse::<Option<u32>>(Cli::Gh, b" \n").unwrap(), None);
        assert_eq!(parse::<u32>(Cli::Glab, b"4").unwrap(), 4);
    }
}
