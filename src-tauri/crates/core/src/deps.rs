//! The tools Prism runs, with their versions (Settings › General › Dependencies): tmux and git,
//! the agents' CLIs, the forges' CLIs, kubectl, gcloud, trivy, docker, and on Linux the audio
//! helpers (pactl, playerctl). Each is looked for in the PATH (an agent CLI at its
//! `[agents.<cli>] bin`) and asked its version, all side by side, a few seconds at most; the
//! answer is kept a while, as the feedback composer attaches it too.
//!
//! Only a name, a path and a version leave: no login, nothing of the config but which tools it uses.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::agents::provider::Provider as AgentProvider;
use crate::detect::{tilde, version, which};

/// How long a probe's answer is reused (the composer opening, the page shown again).
const KEEP: Duration = Duration::from_secs(600);

/// A tool Prism runs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "ToolVersion"))]
pub struct Dependency {
    /// The command: `tmux`, `claude`, `glab`.
    pub name: String,
    /// The config uses it (tmux and git always, the chosen agent and forge CLIs, kubectl with
    /// `[clusters]`, trivy with `[vulnerabilities]`); else optional.
    pub needed: bool,
    /// Its executable (`~/…`); absent: not found.
    pub path: Option<String>,
    /// Its version, as a number (`3.5a`, `2.1.3`); absent: not found, or it did not answer.
    pub version: Option<String>,
    /// The first line of what it answered, whole.
    pub raw: Option<String>,
    /// How to install it; empty for an agent CLI (its own site says).
    pub hint: String,
}

struct Probe {
    name: &'static str,
    /// Its executable: the config's `bin`, else `name`.
    program: String,
    args: &'static [&'static str],
    needed: bool,
    /// Its package, for the install hint; empty: none.
    package: &'static str,
}

fn probes(cfg: &crate::config::Config) -> Vec<Probe> {
    let probe = |name: &'static str, args: &'static [&'static str], needed: bool, package: &'static str| Probe { name, program: name.into(), args, needed, package };
    // Prism's own sessions in place of tmux (Windows): none to look for.
    let mut out = if crate::agents::local_sessions() { Vec::new() } else { vec![probe("tmux", &["-V"], cfg.agents.enabled, "tmux")] };
    out.push(probe("git", &["--version"], true, "git"));
    for p in AgentProvider::ALL {
        let used = cfg.agents.enabled && (cfg.agents.provider == p || cfg.repos.iter().any(|r| r.agent == Some(p)));
        let bin = cfg.agents.cli(p).bin.clone().filter(|b| !b.is_empty());
        out.push(Probe { name: p.program(), program: bin.unwrap_or_else(|| p.program().into()), args: &["--version"], needed: used, package: "" });
    }
    let forge = cfg.forge.enabled.then_some(cfg.forge.provider);
    out.extend([
        probe("glab", &["--version"], forge == Some(crate::forge::Provider::Gitlab), "glab"),
        probe("gh", &["--version"], forge == Some(crate::forge::Provider::Github), "gh"),
        probe("kubectl", &["version", "--client"], cfg.clusters.enabled, "kubectl"),
        probe("gcloud", &["--version"], false, "google-cloud-sdk"),
        probe("trivy", &["--version"], cfg.vulnerabilities.enabled, "trivy"),
        probe("docker", &["--version"], false, "docker"),
    ]);
    // Linux's audio helpers: the Sound block reads the devices through pactl, the Audio block's
    // "Now Playing" through playerctl. Optional: without one, its block says so.
    #[cfg(target_os = "linux")]
    out.extend([probe("pactl", &["--version"], false, "pulseaudio-utils"), probe("playerctl", &["--version"], false, "playerctl")]);
    out
}

/// `3.5a` of `tmux 3.5a`, `2.1.3` of `2.1.3 (Claude Code)`, `1.31.0` of `Client Version: v1.31.0`:
/// the first word that starts with a digit.
pub fn number(line: &str) -> Option<String> {
    line.split(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')')
        .map(|w| w.strip_prefix('v').unwrap_or(w))
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()) && w.contains('.'))
        .map(str::to_string)
}

async fn run(home: &Path, p: Probe) -> Dependency {
    let path = if p.program.contains(['/', '\\']) { Some(std::path::PathBuf::from(&p.program)).filter(|b| b.is_file()) } else { which(&p.program) };
    let raw = match &path {
        Some(bin) => version(&bin.display().to_string(), p.args).await,
        None => None,
    };
    Dependency {
        name: p.name.into(),
        needed: p.needed,
        path: path.map(|b| tilde(home, &b)),
        version: raw.as_deref().and_then(number),
        raw,
        hint: if p.package.is_empty() { String::new() } else { crate::platform::install_hint(p.package) },
    }
}

static KEPT: Mutex<Option<(Instant, Vec<Dependency>)>> = Mutex::new(None);

/// Every tool, found or not; `fresh`: asked again rather than the answer of the last minutes.
pub async fn list(cfg: &crate::config::Config, fresh: bool) -> Vec<Dependency> {
    if !fresh
        && let Some((at, deps)) = KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && at.elapsed() < KEEP
    {
        return deps.clone();
    }
    let home = dirs::home_dir().unwrap_or_default();
    let deps = futures::future::join_all(probes(cfg).into_iter().map(|p| run(&home, p))).await;
    *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), deps.clone()));
    deps
}

/// The demo profile's: a machine with the usual tools, never this one's.
pub fn sample() -> Vec<Dependency> {
    let agent = |name: &str| AgentProvider::ALL.iter().any(|p| p.program() == name);
    let dep = |name: &str, needed: bool, found: Option<(&str, &str)>| Dependency {
        name: name.into(),
        needed,
        path: found.map(|(p, _)| p.into()),
        version: found.and_then(|(_, raw)| number(raw)),
        raw: found.map(|(_, raw)| raw.into()),
        hint: if agent(name) { String::new() } else { crate::platform::install_hint(name) },
    };
    vec![
        dep("tmux", true, Some(("/opt/homebrew/bin/tmux", "tmux 3.5a"))),
        dep("git", true, Some(("/usr/bin/git", "git version 2.47.1"))),
        dep("claude", true, Some(("~/.local/bin/claude", "2.1.3 (Claude Code)"))),
        dep("cmd", false, None),
        dep("opencode", false, None),
        dep("codex", false, Some(("/opt/homebrew/bin/codex", "codex-cli 0.46.0"))),
        dep("glab", true, Some(("/opt/homebrew/bin/glab", "glab 1.48.0 (2024-11-05)"))),
        dep("gh", false, Some(("/opt/homebrew/bin/gh", "gh version 2.62.0 (2024-11-14)"))),
        dep("kubectl", true, Some(("/opt/homebrew/bin/kubectl", "Client Version: v1.31.0"))),
        dep("gcloud", false, None),
        dep("trivy", false, None),
        dep("docker", false, Some(("/usr/local/bin/docker", "Docker version 27.3.1, build ce12230"))),
    ]
}

/// One line for a report: `tmux 3.5a, git 2.47.1, claude 2.1.3, glab missing`. The tools found,
/// and the needed ones that are not.
pub fn summary(deps: &[Dependency]) -> String {
    deps.iter()
        .filter_map(|d| match (&d.version, &d.path) {
            (Some(v), _) => Some(format!("{} {v}", d.name)),
            (None, Some(_)) => Some(format!("{} ?", d.name)),
            (None, None) if d.needed => Some(format!("{} missing", d.name)),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_of_the_usual_answers() {
        let cases = [
            ("tmux 3.5a", "3.5a"),
            ("2.1.3 (Claude Code)", "2.1.3"),
            ("codex-cli 0.46.0", "0.46.0"),
            ("git version 2.47.1", "2.47.1"),
            ("gh version 2.62.0 (2024-11-14)", "2.62.0"),
            ("glab 1.48.0 (abc1234)", "1.48.0"),
            ("Client Version: v1.31.0", "1.31.0"),
            ("Docker version 27.3.1, build ce12230", "27.3.1"),
            ("Google Cloud SDK 520.0.0", "520.0.0"),
            ("Version: 0.58.1", "0.58.1"),
        ];
        for (line, want) in cases {
            assert_eq!(number(line).as_deref(), Some(want), "{line}");
        }
        assert_eq!(number("next-3"), None);
    }

    fn dep(name: &str, needed: bool, path: bool, version: Option<&str>) -> Dependency {
        Dependency { name: name.into(), needed, path: path.then(|| "~/bin".into()), version: version.map(Into::into), raw: None, hint: String::new() }
    }

    #[test]
    fn summary_names_the_found_and_the_needed_missing() {
        let deps = [dep("tmux", true, true, Some("3.5a")), dep("codex", false, false, None), dep("glab", true, false, None), dep("gcloud", false, true, None)];
        assert_eq!(summary(&deps), "tmux 3.5a, glab missing, gcloud ?");
    }
}
