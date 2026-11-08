//! Settings view: `config.toml` read as is and edited key by key with `toml_edit`, which keeps
//! the file's comments, order and formatting. A change goes through a preview
//! (the text that would be written, without touching anything), then through saving: copy of the old
//! file to `.bak`, write, reread by `config::load`; a file the app could not
//! read is immediately replaced by the old one and the error is reported.

use prism_base::{i18n::tr, tr};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

/// A key to set (`value`) or to remove (`null`: the default value takes over).
#[derive(Debug, Clone, Deserialize)]
pub struct Edit {
    /// Path of the key: `["console", "key"]`.
    pub path: Vec<String>,
    pub value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FileView {
    pub path: String,
    pub text: String,
    /// The values present in the file (not the defaults), as JSON.
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub values: serde_json::Value,
    /// The config as the app reads it: the file completed with the defaults (empty if invalid).
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown> | null"))]
    pub effective: serde_json::Value,
    /// The default value of each key (empty for required keys).
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub defaults: serde_json::Value,
    /// The file does not load: why.
    pub error: Option<String>,
    /// Tables of the merged config that neither the core nor a block of this build reads
    /// (`[shipping]` in a Prism built without the block reading it, or a typo).
    pub unread: Vec<String>,
    /// Values the packs give (under the file's own), as JSON.
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown> | null"))]
    pub inherited: serde_json::Value,
    /// The packs the file builds on.
    pub packs: Vec<crate::packs::PackInfo>,
    /// The files of those packs, as they are in `packs/` (the missing ones left out).
    pub pack_files: Vec<PackFile>,
    /// The packs with the file on top, as the app merges them, before the defaults (absent if
    /// the file or a pack does not load).
    pub merged: Option<String>,
}

/// A pack's file, shown read-only next to `config.toml`.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PackFile {
    pub name: String,
    pub path: String,
    pub text: String,
}

pub fn read(path: &Path) -> Result<FileView> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    // As the app reads it: a former `[gitlab]` key shows in `[forge]`, where it now belongs.
    let values = text
        .parse::<toml::Table>()
        .map(|mut t| {
            crate::config::migrate(&mut t);
            serde_json::to_value(t).unwrap_or_default()
        })
        .unwrap_or_default();
    let defaults = crate::config::defaults();
    let (effective, error, unread) = match crate::config::load(path) {
        Ok(c) => {
            let mut effective = serde_json::to_value(&c).unwrap_or_default();
            crate::blocks::settings(&mut effective, &c.table);
            (effective, None, unread(&c.table, &defaults))
        }
        Err(e) => (serde_json::Value::Null, Some(format!("{e:#}")), Vec::new()),
    };
    let (inherited, packs) = match crate::packs::layered(path, &text) {
        Ok((_, inherited, packs)) => (serde_json::to_value(inherited).unwrap_or_default(), packs),
        Err(_) => (serde_json::Value::Null, Vec::new()),
    };
    let pack_files = packs
        .iter()
        .filter(|p| !p.missing)
        .filter_map(|p| {
            let file = crate::packs::file(path, &p.name);
            let text = std::fs::read_to_string(&file).ok()?;
            Some(PackFile { name: p.name.clone(), path: file.display().to_string(), text })
        })
        .collect();
    let merged = merged(path, &text).ok();
    Ok(FileView { path: path.display().to_string(), text, values, effective, defaults, error, unread, inherited, packs, pack_files, merged })
}

/// The tables of `table` no default names: no struct of the core, no block of this build reads them.
pub fn unread(table: &toml::Table, defaults: &serde_json::Value) -> Vec<String> {
    table.iter().filter(|(k, v)| v.is_table() && defaults.get(k.as_str()).is_none()).map(|(k, _)| k.clone()).collect()
}

/// `text` (the file, or what saving would write) on top of its packs, as TOML: what the app
/// reads before filling in the defaults.
pub fn merged(path: &Path, text: &str) -> Result<String> {
    let (table, _, _) = crate::packs::layered(path, text)?;
    Ok(toml::to_string_pretty(&table)?)
}

/// The text these changes would produce, without writing anything.
pub fn preview(path: &Path, edits: &[Edit]) -> Result<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    apply(&text, edits)
}

/// Writes the changes; returns the new text.
pub fn save(path: &Path, edits: &[Edit]) -> Result<String> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let new = apply(&old, edits)?;
    write(path, &old, &new)?;
    Ok(new)
}

/// Adds a `[[repos]]` entry at the end of the file's own, the others as they are (a phone's new
/// project: the file's repositories are not the merged ones Settings edits). Saved as `save` does.
pub fn add_repo(path: &Path, row: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc: DocumentMut = old.parse().context(tr("config.toml is not valid TOML"))?;
    let mut t = Table::new();
    for (k, v) in row.iter().filter(|(_, v)| !v.is_null()) {
        t.insert(k, to_item(v)?);
    }
    match doc.get_mut("repos") {
        None => {
            doc.insert("repos", Item::ArrayOfTables(ArrayOfTables::new()));
        }
        Some(Item::ArrayOfTables(_)) => {}
        Some(_) => bail!("repos is not a list of [[repos]] tables"),
    }
    doc["repos"].as_array_of_tables_mut().context("repos")?.push(t);
    write(path, &old, &doc.to_string())
}

/// Adds an entry at the end of the file's own `[[table.array]]` (a quick action made from a
/// session: `agents`, `quick_actions`), the rest of the file as it is; never in a pack. The keys
/// set to null are left out. Saved as `save` does.
pub fn add_nested_entry(path: &Path, table: &str, array: &str, row: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc: DocumentMut = old.parse().context(tr("config.toml is not valid TOML"))?;
    let mut t = Table::new();
    for (k, v) in row.iter().filter(|(_, v)| !v.is_null()) {
        t.insert(k, to_item(v)?);
    }
    if doc.get(table).is_none() {
        let mut parent = Table::new();
        // No empty `[table]` header: only its `[[table.array]]`.
        parent.set_implicit(true);
        doc.insert(table, Item::Table(parent));
    }
    let parent = doc.get_mut(table).and_then(Item::as_table_like_mut).with_context(|| format!("{table} is not a table"))?;
    match parent.get(array) {
        None => {
            parent.insert(array, Item::ArrayOfTables(ArrayOfTables::new()));
        }
        Some(Item::ArrayOfTables(_)) => {}
        Some(_) => bail!("{table}.{array} is not a list of [[{table}.{array}]] tables"),
    }
    parent.get_mut(array).and_then(Item::as_array_of_tables_mut).context(array.to_string())?.push(t);
    write(path, &old, &doc.to_string())
}

/// Sets keys of the file's `[[repos]]` entry named `name` (a project published later: its
/// `project`), the rest of the entry and of the file as they are. Saved as `save` does.
pub fn set_repo(path: &Path, name: &str, keys: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc: DocumentMut = old.parse().context(tr("config.toml is not valid TOML"))?;
    let row = doc
        .get_mut("repos")
        .and_then(Item::as_array_of_tables_mut)
        .and_then(|rows| rows.iter_mut().find(|t| t.get("name").and_then(Item::as_str) == Some(name)))
        .with_context(|| tr!("no [[repos]] named {name} in config.toml", name = name))?;
    for (k, v) in keys {
        match v {
            serde_json::Value::Null => {
                row.remove(k);
            }
            v => {
                row.insert(k, to_item(v)?);
            }
        }
    }
    write(path, &old, &doc.to_string())
}

/// Sets `field` of the file's `[[table.array]]` entry whose `key` is `id` (a routine switched on
/// or off: `agents`, `routines`, `name`), the rest of the file as it is. Saved as `save` does.
pub fn set_nested_entry_key(path: &Path, table: &str, array: &str, key: &str, id: &str, field: &str, value: &serde_json::Value) -> Result<()> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc: DocumentMut = old.parse().context(tr("config.toml is not valid TOML"))?;
    let row = doc
        .get_mut(table)
        .and_then(Item::as_table_like_mut)
        .and_then(|t| t.get_mut(array))
        .and_then(Item::as_array_of_tables_mut)
        .and_then(|rows| rows.iter_mut().find(|t| t.get(key).and_then(Item::as_str) == Some(id)))
        .with_context(|| tr!("no [[{table}.{array}]] {id} in config.toml (a pack's?)", table = table, array = array, id = id))?;
    row.insert(field, to_item(value)?);
    write(path, &old, &doc.to_string())
}

/// Removes the file's `[[table.array]]` entry whose `key` is `id`, the rest as it is.
pub fn remove_nested_entry(path: &Path, table: &str, array: &str, key: &str, id: &str) -> Result<()> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc: DocumentMut = old.parse().context(tr("config.toml is not valid TOML"))?;
    let rows = doc.get_mut(table).and_then(Item::as_table_like_mut).and_then(|t| t.get_mut(array)).and_then(Item::as_array_of_tables_mut);
    let at = rows.as_ref().and_then(|rows| rows.iter().position(|t| t.get(key).and_then(Item::as_str) == Some(id)));
    match (rows, at) {
        (Some(rows), Some(i)) => {
            rows.remove(i);
        }
        _ => bail!(tr!("no [[{table}.{array}]] {id} in config.toml (a pack's?)", table = table, array = array, id = id)),
    }
    write(path, &old, &doc.to_string())
}

/// Sets the file's `[[array]]` entry whose `key` is `id` (a custom block: `custom_blocks`, `id`):
/// `entry` replaces it whole, keeping its place, or is added after the file's own when there is
/// none; `None` removes it. The rest of the file as it is; a text on several lines written as a
/// literal block (`'''`), as one types a script. Saved as `save` does.
pub fn set_entry(path: &Path, array: &str, key: &str, id: &str, entry: Option<&serde_json::Map<String, serde_json::Value>>) -> Result<()> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let new = entry_text(&old, array, key, id, entry)?;
    write(path, &old, &new)
}

/// `set_entry`'s text, without writing it (an agent's proposal shows what it would write).
pub fn entry_text(old: &str, array: &str, key: &str, id: &str, entry: Option<&serde_json::Map<String, serde_json::Value>>) -> Result<String> {
    let mut doc: DocumentMut = old.parse().context(tr("config.toml is not valid TOML"))?;
    match doc.get(array) {
        None if entry.is_none() => return Ok(old.to_string()),
        None => {
            doc.insert(array, Item::ArrayOfTables(ArrayOfTables::new()));
        }
        Some(Item::ArrayOfTables(_)) => {}
        Some(_) => bail!("{array} is not a list of [[{array}]] tables"),
    }
    let rows = doc[array].as_array_of_tables_mut().context("array")?;
    let at = rows.iter().position(|t| t.get(key).and_then(Item::as_str) == Some(id));
    match (entry, at) {
        (None, Some(i)) => {
            rows.remove(i);
        }
        (None, None) => {}
        (Some(map), at) => {
            let t = entry_table(map)?;
            match at.and_then(|i| rows.get_mut(i)) {
                Some(row) => {
                    // Its place and the comment above it kept.
                    let decor = row.decor().clone();
                    *row = t;
                    *row.decor_mut() = decor;
                }
                None => rows.push(t),
            }
        }
    }
    if rows.is_empty() {
        doc.remove(array);
    }
    Ok(doc.to_string())
}

/// The keys of an entry in the order one reads them (JSON objects come sorted): what it is, what
/// it runs, when, the rest by name, its `[[tables]]` last.
const ENTRY_ORDER: [&str; 18] = ["id", "name", "label", "command", "script", "args", "agent", "when", "every_seconds", "on_demand", "timeout_seconds", "cwd", "env", "critical", "remote", "enabled", "show_source", "actions"];

fn entry_table(map: &serde_json::Map<String, serde_json::Value>) -> Result<Table> {
    let rank = |k: &str| ENTRY_ORDER.iter().position(|o| *o == k).unwrap_or(ENTRY_ORDER.len() - 1);
    let mut keys: Vec<_> = map.iter().filter(|(_, v)| !v.is_null()).collect();
    keys.sort_by_key(|(k, v)| (v.as_array().is_some_and(|a| !a.is_empty() && a.iter().all(|i| i.is_object())), rank(k)));
    let mut t = Table::new();
    for (k, v) in keys {
        t.insert(k, entry_item(v)?);
    }
    Ok(t)
}

/// `to_item`, a text on several lines as a literal block (`'''`), or a basic one escaped when it
/// holds `'''` or a control character; its nested `[[tables]]` the same.
fn entry_item(v: &serde_json::Value) -> Result<Item> {
    match v {
        serde_json::Value::String(s) if s.contains('\n') => {
            let odd = |c: char| c.is_control() && c != '\n' && c != '\t';
            let repr = if !s.contains("'''") && !s.chars().any(odd) {
                format!("'''\n{s}'''")
            } else {
                let mut out = String::from("\"\"\"\n");
                for c in s.chars() {
                    match c {
                        '\\' => out.push_str("\\\\"),
                        '"' => out.push_str("\\\""),
                        c if odd(c) => out.push_str(&format!("\\u{:04X}", c as u32)),
                        c => out.push(c),
                    }
                }
                out.push_str("\"\"\"");
                out
            };
            // Parsed back, as written: toml_edit keeps the text of a value it read.
            let value: Value = repr.parse().map_err(|e| anyhow::anyhow!("a text on several lines: {e}"))?;
            anyhow::ensure!(value.as_str() == Some(s.as_str()), "a text on several lines not read back the same");
            Ok(Item::Value(value))
        }
        serde_json::Value::Array(items) if !items.is_empty() && items.iter().all(|i| i.is_object()) => {
            let mut tables = ArrayOfTables::new();
            for i in items {
                tables.push(entry_table(i.as_object().context("a table")?)?);
            }
            Ok(Item::ArrayOfTables(tables))
        }
        v => to_item(v),
    }
}

/// `new` in place of `old`: the old copied to `.bak`, the new reread by `config::load`, the old
/// put back if it does not load.
pub fn write(path: &Path, old: &str, new: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if !old.is_empty() {
        prism_base::files::write_private(&path.with_extension("toml.bak"), old).context("backup")?;
    }
    prism_base::files::write_private(path, new).context("write")?;
    if let Err(e) = crate::config::load(path) {
        prism_base::files::write_private(path, old).context("restore")?;
        bail!(tr!("not saved, the app could not read it: {err}", err = format!("{e:#}")));
    }
    Ok(())
}

fn apply(text: &str, edits: &[Edit]) -> Result<String> {
    let mut doc: DocumentMut = text.parse().context(tr("config.toml is not valid TOML"))?;
    // A `[forge]` key that was `[gitlab]`'s, a `[tracker]` one that was `[clickup]`'s, a `[clusters]`
    // one that was `[runners]`', `[[pods]]` that was `[runners] namespace`, an `[agents.claude]` one
    // that was `[agents]`', `[sessions]`' or `[usage]`'s: written or removed, its
    // former place goes, so that the file holds it once.
    let former_of = |path: &[String]| -> Option<(&str, String)> {
        match path {
            [t, k] if t == "forge" && crate::config::FORMER_GITLAB_KEYS.contains(&k.as_str()) => Some(("gitlab", k.clone())),
            [t, k] if t == "tracker" && crate::config::FORMER_CLICKUP_KEYS.contains(&k.as_str()) => Some(("clickup", k.clone())),
            [t, k] if t == "clusters" => crate::config::FORMER_RUNNERS_KEYS.iter().find(|(new, _)| new == k).map(|(_, old)| ("runners", old.to_string())),
            [t] if t == "pods" => Some(("runners", "namespace".into())),
            [a, k] if a == "agents" && k == "kinds" => Some(("agents", "categories".into())),
            [a, c, k] if a == "agents" && c == "claude" => crate::config::FORMER_CLAUDE_KEYS.iter().find(|(_, _, new)| new == k).map(|(t, old, _)| (*t, old.to_string())),
            _ => None,
        }
    };
    let former: Vec<Edit> = edits
        .iter()
        .filter_map(|e| {
            let (table, key) = former_of(&e.path)?;
            Some(Edit { path: vec![table.into(), key], value: None })
        })
        .collect();
    'edits: for e in edits.iter().chain(&former) {
        let Some((last, parents)) = e.path.split_last() else { continue };
        let mut table = doc.as_table_mut();
        for p in parents {
            if table.get(p).is_none() {
                // Removing a key whose table is absent: nothing to do (not the parent's same key).
                if e.value.is_none() {
                    continue 'edits;
                }
                table.insert(p, Item::Table(Table::new()));
            }
            table = table.get_mut(p).and_then(Item::as_table_mut).with_context(|| format!("{p} is not a table"))?;
        }
        match &e.value {
            None => {
                table.remove(last);
            }
            Some(v) => {
                let item = to_item(v)?;
                // A value already there keeps its decoration (end-of-line comment).
                match (table.get_mut(last).and_then(Item::as_value_mut), item) {
                    (Some(old), Item::Value(mut new)) => {
                        *new.decor_mut() = old.decor().clone();
                        *old = new;
                    }
                    (_, item) => {
                        table.insert(last, item);
                    }
                }
            }
        }
    }
    // A former table left empty (`[usage]`) goes too.
    for e in &former {
        if doc.get(&e.path[0]).and_then(Item::as_table).is_some_and(|t| t.is_empty()) {
            doc.remove(&e.path[0]);
        }
    }
    Ok(doc.to_string())
}

/// JSON to TOML: an object becomes a table, a list of objects `[[tables]]`, the rest a
/// value.
fn to_item(v: &serde_json::Value) -> Result<Item> {
    Ok(match v {
        serde_json::Value::Array(items) if !items.is_empty() && items.iter().all(|i| i.is_object()) => {
            let mut tables = ArrayOfTables::new();
            for i in items {
                let mut t = Table::new();
                for (k, v) in i.as_object().into_iter().flatten() {
                    if !v.is_null() {
                        t.insert(k, to_item(v)?);
                    }
                }
                tables.push(t);
            }
            Item::ArrayOfTables(tables)
        }
        serde_json::Value::Object(map) => {
            let mut t = Table::new();
            for (k, v) in map {
                t.insert(k, to_item(v)?);
            }
            Item::Table(t)
        }
        v => Item::Value(to_value(v)?),
    })
}

fn to_value(v: &serde_json::Value) -> Result<Value> {
    Ok(match v {
        serde_json::Value::String(s) => s.as_str().into(),
        serde_json::Value::Bool(b) => (*b).into(),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n.as_f64().unwrap_or_default().into(),
        },
        serde_json::Value::Array(items) => {
            let mut a = Array::new();
            for i in items {
                a.push(to_value(i)?);
            }
            Value::Array(a)
        }
        serde_json::Value::Object(map) => {
            let mut t = toml_edit::InlineTable::new();
            for (k, v) in map {
                t.insert(k, to_value(v)?);
            }
            Value::InlineTable(t)
        }
        serde_json::Value::Null => bail!("null is not a TOML value"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn edit(path: &[&str], value: Option<serde_json::Value>) -> Edit {
        Edit { path: path.iter().map(|s| s.to_string()).collect(), value }
    }

    /// A routine switched on or off, then removed: its `[[agents.routines]]` entry alone, the
    /// comments around it kept.
    #[test]
    fn a_nested_entry_key_set() {
        let dir = std::env::temp_dir().join(format!("prism-nested-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "# mine\n[[agents.routines]]\nname = \"A\"\ncron = \"0 3 * * *\" # night\n\n[[agents.routines]]\nname = \"B\"\n").unwrap();
        set_nested_entry_key(&path, "agents", "routines", "name", "B", "enabled", &serde_json::json!(false)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# night") && text.contains("name = \"B\"\nenabled = false"), "{text}");
        assert!(set_nested_entry_key(&path, "agents", "routines", "name", "C", "enabled", &serde_json::json!(true)).is_err());
        remove_nested_entry(&path, "agents", "routines", "name", "A").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("name = \"A\"") && text.contains("name = \"B\""), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A custom block saved from Settings: its script as a literal block, its actions as
    /// `[[custom_blocks.actions]]`, replaced in place, then removed.
    #[test]
    fn sets_an_entry() {
        let text = "[gitlab]\nproject = \"a\"\n\n# backups\n[[custom_blocks]]\nid = \"b\"\ncommand = [\"x\"]\n";
        let entry = json!({ "id": "b", "script": "#!/bin/sh\necho '{}'\n", "every_seconds": 60, "actions": [{ "id": "go", "command": ["y"] }] });
        let out = entry_text(text, "custom_blocks", "id", "b", entry.as_object()).unwrap();
        let back: toml::Table = out.parse().unwrap();
        assert_eq!(back["custom_blocks"][0]["script"].as_str(), Some("#!/bin/sh\necho '{}'\n"));
        assert_eq!(back["custom_blocks"][0]["actions"][0]["id"].as_str(), Some("go"));
        assert!(out.contains("# backups\n[[custom_blocks]]\nid = \"b\"\n"), "{out}");
        assert!(out.contains("script = '''\n#!/bin/sh\necho '{}'\n'''\n"), "{out}");
        assert!(out.contains("[[custom_blocks.actions]]"), "{out}");
        // A script holding ''' itself, quotes and a control character: escaped, read back the same.
        let odd = "a'''b\\\"\"\"\u{7}\n";
        let tricky = json!({ "id": "c", "script": odd });
        let out2 = entry_text(&out, "custom_blocks", "id", "c", tricky.as_object()).unwrap();
        let back: toml::Table = out2.parse().unwrap();
        assert_eq!(back["custom_blocks"][1]["script"].as_str(), Some(odd));
        let gone = entry_text(&out2, "custom_blocks", "id", "b", None).unwrap();
        let gone = entry_text(&gone, "custom_blocks", "id", "c", None).unwrap();
        assert_eq!(gone.trim_end(), "[gitlab]\nproject = \"a\"");
    }

    #[test]
    fn keeps_comments_and_order() {
        let text = "# top\n[gitlab]\nproject = \"a/b\" # the repo\npoll_seconds = 15\n\n[console]\nkey = \"`\"\n";
        let out = apply(text, &[edit(&["console", "key"], Some(json!("Cmd+K"))), edit(&["gitlab", "project"], Some(json!("c/d")))]).unwrap();
        assert_eq!(out, "# top\n[gitlab]\nproject = \"c/d\" # the repo\npoll_seconds = 15\n\n[console]\nkey = \"Cmd+K\"\n");
    }

    #[test]
    fn creates_and_removes() {
        let out = apply("[gitlab]\nproject = \"a\"\n", &[edit(&["console", "history"], Some(json!(80))), edit(&["gitlab", "project"], None)]).unwrap();
        assert_eq!(out, "[gitlab]\n\n[console]\nhistory = 80\n");
        // Removing a key from a missing table does not create it.
        assert_eq!(apply("", &[edit(&["console", "key"], None)]).unwrap(), "");
    }

    /// A quick action added after the file's own, in `[agents]` as it is; in a file without
    /// `[agents]`, no empty header written.
    #[test]
    fn adds_a_quick_action() {
        let dir = std::env::temp_dir().join(format!("prism-settings-qa-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let row = |n: &str| json!({ "name": n, "prompt": "Update {base} of {repo}\nthen scan it.", "repos": ["app"], "worktree": false, "mode": "acceptEdits", "kind": null }).as_object().unwrap().clone();
        std::fs::write(&path, "[agents]
provider = \"claude\" # mine\n\n[[agents.quick_actions]]\nname = \"Old\"\nprompt = \"x\"\n").unwrap();
        add_nested_entry(&path, "agents", "quick_actions", &row("New")).unwrap();
        let cfg = crate::config::load(&path).unwrap();
        assert_eq!(cfg.agents.quick_actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Old", "New"]);
        assert_eq!((cfg.agents.quick_actions[1].mode.as_deref(), cfg.agents.quick_actions[1].kind.as_deref()), (Some("acceptEdits"), None));
        assert!(cfg.agents.quick_actions[1].prompt.contains("\nthen scan it."));
        assert!(std::fs::read_to_string(&path).unwrap().contains("provider = \"claude\" # mine"));
        std::fs::write(&path, "[console]\nkey = \"`\"\n").unwrap();
        add_nested_entry(&path, "agents", "quick_actions", &row("First")).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("[agents]\n") && text.contains("[[agents.quick_actions]]"), "{text}");
        assert_eq!(crate::config::load(&path).unwrap().agents.quick_actions[0].name, "First");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A repository added after the file's own, comments kept; in a file that has none, the first.
    #[test]
    fn adds_a_repository() {
        let dir = std::env::temp_dir().join(format!("prism-settings-repo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let row = |n: &str| json!({ "name": n, "path": format!("~/dev/{n}"), "worktrees": true, "project": null }).as_object().unwrap().clone();
        std::fs::write(&path, "# mine\n[[repos]]\nname = \"app\" # the app\npath = \"~/dev/app\"\n\n[console]\nkey = \"`\"\n").unwrap();
        add_repo(&path, &row("fresh")).unwrap();
        let cfg = crate::config::load(&path).unwrap();
        assert_eq!(cfg.repos.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["app", "fresh"]);
        assert!(cfg.repos[1].worktrees && cfg.repos[1].project.is_none());
        assert!(std::fs::read_to_string(&path).unwrap().contains("name = \"app\" # the app"));
        std::fs::write(&path, "[console]\nkey = \"`\"\n").unwrap();
        add_repo(&path, &row("first")).unwrap();
        assert_eq!(crate::config::load(&path).unwrap().repos[0].name, "first");
        let keys = json!({ "project": "me/first", "merge_requests": true }).as_object().unwrap().clone();
        set_repo(&path, "first", &keys).unwrap();
        let first = crate::config::load(&path).unwrap().repos.remove(0);
        assert_eq!((first.project.as_deref(), first.merge_requests, first.worktrees), (Some("me/first"), true, true));
        assert!(set_repo(&path, "nope", &keys).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// On a copy of config.example.toml: the key changes, the rest of the file does not, the
    /// .bak copy keeps the old one; a file the app could no longer read is not kept.
    #[test]
    fn saves_the_example_config() {
        let dir = std::env::temp_dir().join(format!("prism-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let example = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../config.example.toml")).unwrap();
        std::fs::write(&path, &example).unwrap();

        let new = save(&path, &[edit(&["console", "key"], Some(json!("Cmd+K"))), edit(&["console", "history"], Some(json!(80)))]).unwrap();
        assert_eq!(crate::config::load(&path).unwrap().console.key, "Cmd+K");
        assert_eq!(new.lines().count(), example.lines().count() + 1, "only one line added");
        assert!(new.contains("# Prism console"), "comments kept");
        assert_eq!(std::fs::read_to_string(path.with_extension("toml.bak")).unwrap(), example);

        // Rewritten [[tables]] and a removed key: the app rereads the result.
        save(&path, &[
            edit(&["environments"], Some(json!([{ "env": "prod", "context": "ctx", "namespaces": ["apps"] }]))),
            edit(&["audio", "stations"], Some(json!([{ "label": "FIP", "url": "https://x" }]))),
            edit(&["gitlab", "helm_project"], None),
        ])
        .unwrap();
        let cfg = crate::config::load(&path).unwrap();
        assert_eq!(cfg.environments.len(), 1);
        assert_eq!(cfg.audio.stations.len(), 1);
        let new = std::fs::read_to_string(&path).unwrap();
        assert!(!new.lines().any(|l| l.starts_with("helm_project")), "a key no struct of the core reads is removed too");
        assert!(new.contains("[[environments]]") && new.contains("[[audio.stations]]"));

        // A value of the wrong type: refused, the file stays as it was.
        assert!(save(&path, &[edit(&["forge", "poll_seconds"], Some(json!("often")))]).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), new);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// What the view receives: the file's values, the effective config and the defaults.
    #[test]
    fn reads_effective_values_and_defaults() {
        let view = read(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../config.example.toml"))).unwrap();
        assert!(view.error.is_none(), "{:?}", view.error);
        assert_eq!(view.values["repos"][0]["project"], "example-org/app");
        assert_eq!(view.effective["gitlab"]["project"], "", "the primary repository is not copied into [gitlab]");
        assert_eq!(view.effective["forge"]["poll_seconds"], 15);
        assert_eq!(view.defaults["clickup"]["poll_seconds"], 45);
        assert_eq!(view.defaults["vulnerabilities"]["branches"], json!(["main"]));
        assert_eq!(view.defaults["gitlab"]["helm_project"], serde_json::Value::Null);
        assert!(view.defaults["audio"]["stations"].as_array().is_some_and(|a| !a.is_empty()));
    }

    /// A former `[clickup]` key shows in `[tracker]`, and moves there for good when it is changed.
    #[test]
    fn former_clickup_keys_move_to_the_tracker() {
        let text = "[clickup]\nsprint_folder = \"Mine\"\nticket_prefix = \"ACME\"\n";
        let out = apply(text, &[edit(&["tracker", "ticket_prefix"], Some(json!("NEW")))]).unwrap();
        assert_eq!(out, "[clickup]\nsprint_folder = \"Mine\"\n\n[tracker]\nticket_prefix = \"NEW\"\n");
    }

    /// A former `[agents]`, `[sessions]` or `[usage]` key shows in `[agents.claude]`, and moves
    /// there for good when it is changed.
    #[test]
    fn former_claude_keys_move_to_agents_claude() {
        let dir = std::env::temp_dir().join(format!("prism-settings-claude-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[agents]\nfont_size = 13.0\nshow_desktop = false\n\n[usage]\nrefresh_token = false\n").unwrap();
        let view = read(&path).unwrap();
        assert_eq!((view.values["agents"]["claude"]["show_desktop"].as_bool(), view.values.get("usage")), (Some(false), None));
        assert_eq!(view.effective["agents"]["claude"]["refresh_token"], false);
        let out = apply(&view.text, &[edit(&["agents", "claude", "show_desktop"], Some(json!(true))), edit(&["agents", "claude", "refresh_token"], None)]).unwrap();
        assert_eq!(out, "[agents]\nfont_size = 13.0\n\n[agents.claude]\nshow_desktop = true\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A former `[gitlab]` key shows in `[forge]`, and moves there for good when it is changed.
    #[test]
    fn former_gitlab_keys_move_to_the_forge() {
        let dir = std::env::temp_dir().join(format!("prism-settings-forge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[gitlab]\nhost = \"git.acme.io\"\nmain_branch = \"develop\" # ours\nenabled = false\n").unwrap();
        let view = read(&path).unwrap();
        assert_eq!((view.values["forge"]["main_branch"].as_str(), view.values["gitlab"].get("main_branch")), (Some("develop"), None));
        assert_eq!(view.effective["forge"]["enabled"], false);
        let out = apply(&view.text, &[edit(&["forge", "main_branch"], Some(json!("trunk"))), edit(&["forge", "enabled"], None)]).unwrap();
        assert_eq!(out, "[gitlab]\nhost = \"git.acme.io\"\n\n[forge]\nmain_branch = \"trunk\"\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Former `[runners]` keys go when their `[clusters]` and `[[pods]]` successors are written.
    #[test]
    fn former_runners_keys_move_to_the_clusters() {
        let text = "[runners]\nenabled = true\nkube_context = \"ci\"\nnamespace = \"gitlab-runner\"\n";
        let out = apply(text, &[edit(&["clusters", "default_context"], Some(json!("prod"))), edit(&["pods"], Some(json!([{ "namespaces": ["app-*"] }])))]).unwrap();
        assert!(!out.contains("kube_context") && !out.contains("namespace ="), "{out}");
        assert!(out.contains("enabled = true") && out.contains("default_context = \"prod\"") && out.contains("[[pods]]"), "{out}");
    }

    /// Removing a key of an absent table leaves the parent's key of the same name alone.
    #[test]
    fn removing_from_an_absent_table() {
        let out = apply("[deploy]\ntag = \"x\"\n", &[edit(&["deploy", "preprod", "tag"], None)]).unwrap();
        assert_eq!(out, "[deploy]\ntag = \"x\"\n");
    }

    #[test]
    fn arrays_of_tables() {
        let text = "[gitlab]\nproject = \"a\"\n\n[[repos]]\nname = \"old\"\npath = \"/x\"\n";
        let out = apply(text, &[edit(&["repos"], Some(json!([{ "name": "helm", "path": "~/helm", "project": null }, { "name": "infra", "path": "~/infra" }])))]).unwrap();
        assert_eq!(out, "[gitlab]\nproject = \"a\"\n\n[[repos]]\nname = \"helm\"\npath = \"~/helm\"\n\n[[repos]]\nname = \"infra\"\npath = \"~/infra\"\n");
    }

    /// Settings › Interface › Pages writes `[[views]]`, with the list of columns; the config rereads it.
    #[test]
    fn views_round_trip() {
        let rows = json!([{ "name": "Mon focus", "icon": "star", "columns": ["sessions notes", "mrs"] }, { "name": "Infra", "id": "infra" }]);
        let out = apply("[gitlab]\nproject = \"a\"\n", &[edit(&["views"], Some(rows))]).unwrap();
        assert!(out.contains("[[views]]\ncolumns = [\"sessions notes\", \"mrs\"]\nicon = \"star\"\nname = \"Mon focus\"\n"), "{out}");
        #[derive(serde::Deserialize)]
        struct Only {
            views: Vec<crate::config::ViewConfig>,
        }
        let read: Only = toml::from_str(&out).unwrap();
        assert_eq!(read.views.len(), 2);
        assert_eq!(read.views[0].columns, ["sessions notes", "mrs"]);
        assert_eq!(read.views[1].id.as_deref(), Some("infra"));
        assert!(read.views[1].columns.is_empty());
    }

    /// Settings › Work › Tracker › ClickUp writes `[[clickup.boards]]` under its table; the config rereads it.
    #[test]
    fn clickup_boards_round_trip() {
        let rows = json!([{ "name": "Hotfix", "list": "Urgent bugs", "uses": ["pick"], "tone": "ko", "quarters": false }, { "name": "Product", "folder": "Product", "quarters": true, "uses": ["refinement"], "tone": null }]);
        let out = apply("[clickup]\nsprint_folder = \"Mine\"\n", &[edit(&["clickup", "boards"], Some(rows))]).unwrap();
        assert!(out.contains("[[clickup.boards]]"), "{out}");
        let cfg: crate::config::Config = toml::from_str(&out).unwrap();
        let boards = cfg.clickup.boards.clone();
        assert_eq!(boards.iter().map(|b| (b.name.as_str(), b.list_name(), b.quarters)).collect::<Vec<_>>(), [("Hotfix", "Urgent bugs", false), ("Product", "Product", true)]);
        assert_eq!(cfg.clickup.sprint_folder, "Mine");
    }

    #[test]
    fn tables_from_objects() {
        let out = apply("", &[edit(&["console", "aliases"], Some(json!({ "r": "claude review" })))]).unwrap();
        assert_eq!(out, "[console]\n\n[console.aliases]\nr = \"claude review\"\n");
    }
}
