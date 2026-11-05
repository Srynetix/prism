//! `[[custom_blocks]]` and their `[[custom_blocks.actions]]` as the config declares them, checked,
//! and the hash of what each one runs: the trust given to a block is given to that hash.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha2::Digest;

/// A custom block (`[[custom_blocks]]`).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomBlockDef"))]
pub struct Def {
    /// Its id: `custom:<id>` in `[[views]]`.
    pub id: String,
    /// Its name; the id without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub label: Option<String>,
    /// The program and its arguments, run as is: never a shell in between.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub command: Vec<String>,
    /// Or the program itself, written in the config: run by its shebang (`/bin/sh` without one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub script: Option<String>,
    /// Passed to the script (or after the command's own).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub args: Vec<String>,
    /// Read every so many seconds (300 by default), unless `on_demand`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub every_seconds: Option<u64>,
    /// Read only when asked (its ↻), and once at launch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<bool>"))]
    pub on_demand: bool,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    /// Where it runs (`~` allowed); the home folder without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub cwd: Option<String>,
    /// Its environment's additions: `keychain:<service>` reads a secret of the system keychain.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<BTreeMap<String, String>>"))]
    pub env: BTreeMap<String, String>,
    /// Sent to a phone (off: it stays on the computer, and off the relay).
    #[serde(default = "yes")]
    pub remote: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<Action>>"))]
    pub actions: Vec<Action>,
}

/// A button on its lines (`[[custom_blocks.actions]]`): a command, a script, or an agent.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomAction"))]
pub struct Action {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub label: Option<String>,
    /// Only on the lines it holds for: `state != 'ok'`, `kind == 'disk' && state == 'ko'`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub when: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub script: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub args: Vec<String>,
    /// An agent launched with this prompt, the line given as data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub agent: Option<String>,
    /// From a phone, biometrics each time.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<bool>"))]
    pub critical: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub timeout_seconds: Option<u64>,
}

fn default_timeout() -> u64 {
    20
}

fn yes() -> bool {
    true
}

/// The pause between two reads by default.
pub const DEFAULT_EVERY: u64 = 300;
/// The shortest pause between two reads: a block is never a busy loop.
pub const MIN_EVERY: u64 = 5;
const MAX_TIMEOUT: u64 = 600;

impl Def {
    pub fn label(&self) -> &str {
        self.label.as_deref().filter(|l| !l.trim().is_empty()).unwrap_or(&self.id)
    }

    pub fn every(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.every_seconds.unwrap_or(DEFAULT_EVERY).max(MIN_EVERY))
    }

    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.timeout_seconds.clamp(1, MAX_TIMEOUT))
    }

    /// What is wrong with it, before anything runs.
    pub fn check(&self) -> Result<()> {
        if !valid_id(&self.id) {
            bail!(prism_base::tr!("custom block {id}: an id is lowercase letters, digits, - and _, starting with a letter or a digit (40 at most)", id = self.id));
        }
        runs(&self.command, &self.script).map_err(|e| anyhow::anyhow!("custom block {}: {e}", self.id))?;
        for (name, value) in &self.env {
            if secret_name(name) && !value.starts_with("keychain:") && !value.is_empty() {
                bail!(prism_base::tr!("custom block {id}: env {name} looks like a secret written in clear: store it in the keychain and write keychain:<service>", id = self.id, name = name));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for a in &self.actions {
            if !valid_id(&a.id) || !seen.insert(a.id.as_str()) {
                bail!(prism_base::tr!("custom block {id}: action {action}: a missing, repeated or invalid id", id = self.id, action = a.id));
            }
            let kinds = [!a.command.is_empty(), a.script.is_some(), a.agent.is_some()].iter().filter(|k| **k).count();
            if kinds != 1 {
                bail!(prism_base::tr!("custom block {id}: action {action}: one of command, script or agent", id = self.id, action = a.id));
            }
            if let Some(when) = &a.when {
                super::when::parse(when).map_err(|e| anyhow::anyhow!("custom block {}: action {}: when: {e}", self.id, a.id))?;
            }
        }
        Ok(())
    }

    /// The hash of what it runs: its command or script, arguments, folder, environment (names and
    /// references, never a keychain's values) and actions. Its name, pace or `remote` changed, the
    /// trust holds.
    pub fn hash(&self) -> String {
        #[derive(Serialize)]
        struct Runs<'a> {
            command: &'a [String],
            script: &'a Option<String>,
            args: &'a [String],
            cwd: &'a Option<String>,
            env: &'a BTreeMap<String, String>,
            timeout: u64,
            actions: &'a [Action],
        }
        let runs = Runs { command: &self.command, script: &self.script, args: &self.args, cwd: &self.cwd, env: &self.env, timeout: self.timeout_seconds, actions: &self.actions };
        hex(&sha2::Sha256::digest(serde_json::to_vec(&runs).unwrap_or_default()))
    }
}

/// One of command and script, the command not empty.
fn runs(command: &[String], script: &Option<String>) -> Result<()> {
    match (command.first(), script) {
        (Some(_), Some(_)) => bail!("{}", prism_base::i18n::tr("command and script exclude each other")),
        (None, None) => bail!("{}", prism_base::i18n::tr("a command or a script")),
        (Some(p), None) if p.trim().is_empty() => bail!("{}", prism_base::i18n::tr("an empty command")),
        (None, Some(s)) if s.trim().is_empty() => bail!("{}", prism_base::i18n::tr("an empty script")),
        _ => Ok(()),
    }
}

pub fn valid_id(id: &str) -> bool {
    // As the window reads `custom:<id>` (views.ts `isCustom`): a letter or a digit first.
    (1..=40).contains(&id.len()) && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// A name that holds a secret: `*TOKEN*`, `*KEY*`, `*PASSWORD*`, `*SECRET*`.
pub fn secret_name(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["TOKEN", "KEY", "PASSWORD", "PASSWD", "SECRET"].iter().any(|s| n.contains(s))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The blocks of the merged config (`[[custom_blocks]]`), each read alone: one that does not read
/// is an error of its own, the others run. A repeated id: the first one.
pub fn read(config: &toml::Table) -> (Vec<Def>, Vec<String>) {
    let mut defs: Vec<Def> = Vec::new();
    let mut errors = Vec::new();
    let Some(list) = config.get("custom_blocks") else { return (defs, errors) };
    let Some(list) = list.as_array() else {
        return (defs, vec![prism_base::i18n::tr("custom_blocks: a list of [[custom_blocks]] tables").to_string()]);
    };
    for (i, entry) in list.iter().enumerate() {
        let def: Def = match entry.clone().try_into() {
            Ok(d) => d,
            Err(e) => {
                let id = entry.get("id").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_else(|| format!("#{}", i + 1));
                errors.push(format!("custom block {id}: {e}"));
                continue;
            }
        };
        if defs.iter().any(|d| d.id == def.id) {
            errors.push(prism_base::tr!("custom block {id}: declared twice, the first one kept", id = def.id));
            continue;
        }
        match def.check() {
            Ok(()) => defs.push(def),
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    (defs, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs(text: &str) -> (Vec<Def>, Vec<String>) {
        read(&text.parse().unwrap())
    }

    #[test]
    fn reads_and_checks_the_blocks() {
        let (d, e) = defs("[[custom_blocks]]\nid = \"backups\"\ncommand = [\"~/bin/x\", \"--json\"]\n\n[[custom_blocks]]\nid = \"q\"\nscript = \"echo 1\"\non_demand = true\n\n[[custom_blocks.actions]]\nid = \"fix\"\nagent = \"Fix {item.title}\"\nwhen = \"state != 'ok'\"\n");
        assert_eq!(e, Vec::<String>::new());
        assert_eq!(d.len(), 2);
        assert_eq!((d[0].label(), d[0].every().as_secs(), d[0].timeout().as_secs(), d[0].remote), ("backups", 300, 20, true));
        assert_eq!(d[1].actions[0].agent.as_deref(), Some("Fix {item.title}"));
    }

    #[test]
    fn refuses_what_is_wrong() {
        let (d, e) = defs("[[custom_blocks]]\nid = \"a\"\ncommand = [\"x\"]\nscript = \"y\"\n\n[[custom_blocks]]\nid = \"B!\"\ncommand = [\"x\"]\n\n[[custom_blocks]]\nid = \"c\"\ncommand = [\"x\"]\nenv = { API_TOKEN = \"abc\" }\n\n[[custom_blocks]]\nid = \"d\"\ncommand = [\"x\"]\nnope = 1\n\n[[custom_blocks]]\nid = \"e\"\ncommand = [\"x\"]\nenv = { API_TOKEN = \"keychain:svc\", MODE = \"prod\" }\n\n[[custom_blocks]]\nid = \"e\"\ncommand = [\"y\"]\n");
        assert_eq!(d.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["e"]);
        assert_eq!(e.len(), 5, "{e:?}");
        assert!(!valid_id("_scratch") && !valid_id("-x") && valid_id("0-a_b"));
        assert!(e[2].contains("API_TOKEN"), "{e:?}");
    }

    #[test]
    fn the_hash_follows_what_runs_only() {
        let (d, _) = defs("[[custom_blocks]]\nid = \"a\"\ncommand = [\"x\"]\n");
        let mut other = d[0].clone();
        other.label = Some("A".into());
        other.every_seconds = Some(60);
        other.remote = false;
        assert_eq!(d[0].hash(), other.hash());
        other.args = vec!["-v".into()];
        assert_ne!(d[0].hash(), other.hash());
        let mut acted = d[0].clone();
        acted.actions.push(Action { id: "x".into(), label: None, when: None, command: vec!["rm".into()], script: None, args: vec![], agent: None, critical: false, timeout_seconds: None });
        assert_ne!(d[0].hash(), acted.hash());
    }
}
