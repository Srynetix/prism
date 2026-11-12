//! `[wasm]` of a module's `module.toml`: its component, the config tables it is given, its
//! capabilities and its blocks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

/// A module's back end, as its manifest declares it.
#[derive(Debug, Clone)]
pub struct Spec {
    /// Its component, inside the module's folder.
    pub component: PathBuf,
    pub tables: Vec<String>,
    pub capabilities: Capabilities,
    pub blocks: Vec<BlockSpec>,
}

/// What it may reach through Prism (`host`): nothing it does not declare.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Capabilities {
    /// git in its blocks' clones.
    pub git: bool,
    /// GitLab's API, through glab.
    pub gitlab: bool,
    /// The hosts it may GET from.
    pub http: Vec<String>,
    /// The programs it may run (bare names, found in Prism's PATH).
    pub exec: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockSpec {
    pub id: String,
    #[serde(default)]
    pub parts: Vec<String>,
    #[serde(default)]
    pub repos: Repos,
    #[serde(default)]
    pub reloads: bool,
    #[serde(default)]
    pub provides: Vec<String>,
    /// Its commands' class from a phone (`read`, `act`, `critical`); the others: never.
    #[serde(default)]
    pub remote: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Repos {
    #[default]
    None,
    One,
    Many,
}

#[derive(Deserialize)]
struct Manifest {
    wasm: Option<Wasm>,
}

#[derive(Deserialize)]
struct Wasm {
    component: String,
    #[serde(default)]
    tables: Vec<String>,
    #[serde(default)]
    capabilities: Capabilities,
    #[serde(default)]
    blocks: Vec<BlockSpec>,
}

/// An id of a block or of a part: as the window's (`web/src/blocks/runtime.ts`).
fn plain(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && id.len() <= 40 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The `[wasm]` of the module in `folder`.
pub fn read(folder: &Path) -> Result<Spec> {
    let text = std::fs::read_to_string(folder.join("module.toml")).context("module.toml")?;
    parse(folder, &text)
}

pub fn parse(folder: &Path, text: &str) -> Result<Spec> {
    let mut wasm = toml::from_str::<Manifest>(text).context("module.toml")?.wasm.ok_or_else(|| anyhow!("no [wasm]"))?;
    // A part named as its own block (Delivery's `shipping`) is the block's source: not a part apart.
    for b in &mut wasm.blocks {
        let id = b.id.clone();
        b.parts.retain(|p| *p != id);
    }
    let component = prism_core::modules::within(folder, &wasm.component).filter(|c| c.is_file()).ok_or_else(|| anyhow!("[wasm] component: no {} inside the module", wasm.component))?;
    if wasm.blocks.is_empty() {
        bail!("[wasm] declares no block ([[wasm.blocks]])");
    }
    let mut seen = Vec::new();
    for b in &wasm.blocks {
        for id in std::iter::once(&b.id).chain(&b.parts) {
            if !plain(id) {
                bail!("[[wasm.blocks]] {id}: an id is lowercase letters, digits and _");
            }
            if seen.contains(&id) {
                bail!("[[wasm.blocks]] {id}: declared twice");
            }
            seen.push(id);
        }
        if let Some((command, class)) = b.remote.iter().find(|(_, c)| !matches!(c.as_str(), "read" | "act" | "critical" | "never")) {
            bail!("[[wasm.blocks]] {} remote.{command} = {class:?}: read, act, critical or never", b.id);
        }
    }
    if let Some(bad) = wasm.capabilities.exec.iter().find(|p| p.is_empty() || p.contains(['/', '\\'])) {
        bail!("[wasm] capabilities.exec: {bad:?} is not a program's name");
    }
    Ok(Spec { component, tables: wasm.tables, capabilities: wasm.capabilities, blocks: wasm.blocks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_its_blocks_and_refuses_what_it_should_not_declare() {
        let dir = std::env::temp_dir().join(format!("prism-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b.wasm"), b"").unwrap();
        let ok = parse(
            &dir,
            "[wasm]\ncomponent = \"b.wasm\"\ntables = [\"hello\"]\ncapabilities = { git = true, http = [\"example.org\"] }\n\
             [[wasm.blocks]]\nid = \"hello\"\nparts = [\"hello_reads\"]\nrepos = \"one\"\nprovides = [\"stages\"]\nremote = { ping = \"read\" }\n",
        )
        .unwrap();
        assert_eq!(ok.blocks[0].repos, Repos::One);
        assert!(ok.capabilities.git && !ok.capabilities.gitlab);
        let bad = |text: &str| parse(&dir, &format!("[wasm]\ncomponent = \"b.wasm\"\n{text}")).unwrap_err().to_string();
        assert!(bad("").contains("no block"));
        assert!(bad("[[wasm.blocks]]\nid = \"Hello\"\n").contains("lowercase"));
        assert!(bad("[[wasm.blocks]]\nid = \"a\"\nparts = [\"b\"]\n[[wasm.blocks]]\nid = \"b\"\n").contains("twice"));
        // A part named as its own block: Delivery's `shipping`.
        assert_eq!(parse(&dir, "[wasm]\ncomponent = \"b.wasm\"\n[[wasm.blocks]]\nid = \"a\"\nparts = [\"a\", \"c\"]\n").unwrap().blocks[0].parts, ["c"]);
        assert!(bad("[[wasm.blocks]]\nid = \"a\"\nremote = { x = \"always\" }\n").contains("read, act"));
        assert!(bad("capabilities = { exec = [\"/bin/sh\"] }\n[[wasm.blocks]]\nid = \"a\"\n").contains("program"));
        assert!(parse(&dir, "[wasm]\ncomponent = \"../b.wasm\"\n[[wasm.blocks]]\nid = \"a\"\n").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
