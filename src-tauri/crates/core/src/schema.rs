//! The config's JSON schema, for agents (`prism mcp`): every table of the core (`schemars`, from
//! the config's structs and their doc comments) and of each block (`Block::schema`), each marked
//! with what reads it (`x-prism-read-by`) and its Settings page (`x-prism-settings`). A key's
//! entry (`key`) and a TOML's check against it (`check`) are read from there.

use serde_json::{json, Map, Value};

/// The whole schema: an object of tables, `$defs` holding the types they share.
#[cfg(feature = "schema")]
pub fn config() -> Value {
    let mut root = serde_json::to_value(schemars::schema_for!(crate::config::Config)).unwrap_or_default();
    if !root["properties"].is_object() {
        root["properties"] = json!({});
    }
    for b in crate::blocks::all() {
        for (table, schema) in b.schema() {
            let props = root["properties"].as_object_mut().expect("properties");
            match (props.get_mut(table), schema) {
                // A block's keys in a table of the core (`[gitlab]`'s of its own): added to it.
                (Some(existing), Value::Object(mut add)) if existing.get("$ref").is_none() && existing["properties"].is_object() => {
                    if let Some(Value::Object(keys)) = add.remove("properties") {
                        existing["properties"].as_object_mut().expect("properties").extend(keys);
                    }
                }
                (Some(_), _) => {}
                (None, mut schema) => {
                    schema["x-prism-block"] = json!(b.id());
                    props.insert(table.to_string(), schema);
                }
            }
        }
    }
    let blocks: Vec<(String, String)> = crate::blocks::all().iter().map(|b| (b.id().to_string(), b.id().to_string())).collect();
    if let Some(props) = root["properties"].as_object_mut() {
        for (table, schema) in props.iter_mut() {
            let readers = read_by(table, &blocks);
            if !readers.is_empty() {
                schema["x-prism-read-by"] = json!(readers);
            }
            schema["x-prism-settings"] = json!(settings_page(table, None));
        }
    }
    root["title"] = json!("Prism config.toml");
    root["description"] = json!("Prism's config.toml: its tables, with their types, defaults and descriptions. Packs layer under it (`packs`); a key a pack sets is overridden by setting it here.");
    root
}

/// The schema without the feature: none (the binaries are built with it).
#[cfg(not(feature = "schema"))]
pub fn config() -> Value {
    json!({ "type": "object", "properties": {} })
}

/// The blocks and views a table's keys reach, for an agent: `MRs`, `Pipelines`…
pub fn read_by(table: &str, blocks: &[(String, String)]) -> Vec<String> {
    let known: &[&str] = match table {
        "forge" | "gitlab" | "github" => &["MRs", "Pipelines", "Worktrees", "Alerts", "CI health", "Merge queue"],
        "repos" => &["MRs", "Pipelines", "Worktrees", "Agents", "Repos menu"],
        "workspaces" => &["Settings › Repositories (new project, clone)"],
        "worktrees" => &["Worktrees"],
        "tracker" | "clickup" | "jira" | "obsidian" => &["Tickets", "Sprint", "Time", "Alerts"],
        "calendar" | "google" => &["Agenda", "Meetings"],
        "mail" => &["Mail"],
        "clusters" | "environments" => &["Environments", "Pods"],
        "pods" => &["Pods"],
        "argocd" | "argocd_layout" => &["ArgoCD"],
        "vulnerabilities" => &["Vulnerabilities"],
        "renovate" => &["Renovate"],
        "audio" => &["Audio (status bar)"],
        "alerts" => &["Alerts", "Digest"],
        "agents" => &["Agents", "Launcher", "Chat", "Routines", "Quick actions"],
        "views" => &["Pages (the rail)"],
        "appearance" => &["The whole window"],
        "notes" => &["Notes"],
        "dictation" => &["Chat (microphone)", "Launcher"],
        "palette" => &["Palette"],
        "remote" | "machines" => &["Phone remote", "Other computers"],
        "mcp" => &["prism mcp"],
        _ => &[],
    };
    let mut out: Vec<String> = known.iter().map(|s| s.to_string()).collect();
    if let Some((_, label)) = blocks.iter().find(|(id, _)| id == table) {
        out.push(format!("block {label}"));
    }
    out
}

/// The Settings page showing a table (or one of its keys): `Code › Forge › GitLab`.
pub fn settings_page(table: &str, key: Option<&str>) -> String {
    let page = match (table, key) {
        ("agents", Some("quick_actions")) => "Agents › Quick actions",
        ("agents", Some("routines")) => "Agents › Routines",
        ("agents", Some("overlaps")) => "Agents › Overlaps",
        ("agents", Some("aside")) => "Agents › In passing",
        ("agents", Some("quick_answers")) => "Agents › Quick answers",
        ("agents", Some("claude")) => "Agents › Agent › Claude Code",
        ("agents", Some("codex")) => "Agents › Agent › Codex",
        ("agents", Some("opencode")) => "Agents › Agent › OpenCode",
        ("agents", Some("command-code")) => "Agents › Agent › Command Code",
        ("agents", Some("prism_mcp")) => "Agents › Prism MCP",
        ("profile" | "demo", _) => "General › Profiles",
        ("packs", _) => "General › Packs",
        ("modules", _) => "General › Packs (Modules)",
        ("logs", _) => "General › Logs",
        ("updates", _) => "General › Updates",
        ("feedback", _) => "General › Feedback",
        ("appearance", _) => "Interface › Appearance",
        ("palette", _) => "Interface › Palette",
        ("views" | "templates", _) => "Interface › Pages",
        ("custom_blocks", _) => "Interface › Custom blocks",
        ("alerts" | "audio", _) => "Interface › Alerts & sound",
        ("open_with", _) => "Interface › Open with",
        ("tracker", _) => "Work › Tracker",
        ("clickup", _) => "Work › Tracker › ClickUp",
        ("jira", _) => "Work › Tracker › Jira",
        ("obsidian", _) => "Work › Tracker › Obsidian",
        ("calendar", _) => "Work › Calendar",
        ("google", _) => "Work › Calendar › Google Calendar (and Work › Mail › Gmail)",
        ("mail", _) => "Work › Mail",
        ("notes", _) => "Work › Notes",
        ("forge", _) => "Code › Forge",
        ("gitlab", _) => "Code › Forge › GitLab",
        ("github", _) => "Code › Forge › GitHub",
        ("repos" | "workspaces" | "worktrees", _) => "Code › Repositories",
        ("renovate", _) => "CI › Renovate",
        ("vulnerabilities", _) => "Security › Vulnerabilities",
        ("agents", _) => "Agents › Agent",
        ("files", _) => "Agents › Files",
        ("console", _) => "Agents › Console",
        ("dictation", _) => "Agents › Dictation",
        ("mcp", _) => "Agents › Prism MCP",
        ("clusters" | "environments", _) => "Infra › Kubernetes",
        ("pods", _) => "Infra › Kubernetes › Pods",
        ("argocd" | "argocd_layout", _) => "Infra › Kubernetes › ArgoCD",
        ("remote", _) => "Remote › Access",
        ("machines", _) => "Remote › Machines",
        ("cleanup", _) => "Cleanup",
        _ => return format!("its block's page ({table}), under its domain"),
    };
    page.to_string()
}

/// A key's entry in `schema`: `forge.bare_mr_repo`, `repos[].forge`, `agents.claude.model`
/// (`[]` or `[0]` for a list's items), its `$ref`s followed.
pub fn key<'a>(schema: &'a Value, path: &str) -> Option<&'a Value> {
    // The last one as written (its own description beside its `$ref`), the others followed.
    let mut at = schema;
    for part in segments(path) {
        let here = resolve(schema, at);
        at = match part {
            Seg::Items => items_of(schema, here)?,
            Seg::Key(k) => property(schema, here, k)?,
        };
    }
    Some(at)
}

enum Seg<'a> {
    Key(&'a str),
    Items,
}

fn segments(path: &str) -> Vec<Seg<'_>> {
    let mut out = Vec::new();
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let (name, rest) = part.split_once('[').map_or((part, ""), |(n, r)| (n, r));
        if !name.is_empty() {
            out.push(Seg::Key(name));
        }
        for _ in rest.matches(']') {
            out.push(Seg::Items);
        }
    }
    out
}

/// `$ref: #/$defs/X` followed (and an `allOf`/`anyOf` of one, an `Option`'s `null` left out).
fn resolve<'a>(root: &'a Value, mut at: &'a Value) -> &'a Value {
    for _ in 0..16 {
        if let Some(r) = at.get("$ref").and_then(Value::as_str) {
            match r.strip_prefix("#/$defs/").and_then(|name| root.get("$defs")?.get(name)) {
                Some(def) => at = def,
                None => return at,
            }
            continue;
        }
        let one_of = ["allOf", "anyOf", "oneOf"].iter().find_map(|k| at.get(*k).and_then(Value::as_array));
        if let Some(list) = one_of {
            let real: Vec<&Value> = list.iter().filter(|v| v.get("type").and_then(Value::as_str) != Some("null")).collect();
            if real.len() == 1 && at.get("properties").is_none() {
                at = real[0];
                continue;
            }
        }
        return at;
    }
    at
}

fn property<'a>(root: &'a Value, at: &'a Value, key: &str) -> Option<&'a Value> {
    if let Some(p) = at.get("properties").and_then(|p| p.get(key)) {
        return Some(p);
    }
    // A map (`env`, a table of tables): its values' schema.
    if let Some(extra) = at.get("additionalProperties").filter(|v| v.is_object()) {
        return Some(extra);
    }
    // An enum of objects (`anyOf` of tables): the first that has it.
    ["anyOf", "oneOf"].iter().filter_map(|k| at.get(*k).and_then(Value::as_array)).flatten().find_map(|v| resolve(root, v).get("properties").and_then(|p| p.get(key)))
}

fn items_of<'a>(root: &'a Value, at: &'a Value) -> Option<&'a Value> {
    at.get("items").or_else(|| ["anyOf", "oneOf"].iter().filter_map(|k| at.get(*k).and_then(Value::as_array)).flatten().find_map(|v| resolve(root, v).get("items")))
}

/// What an agent is told of a key's schema: its type(s), allowed values, default, description.
pub fn summary(root: &Value, entry: &Value) -> Value {
    let raw = entry;
    let entry = resolve(root, entry);
    let mut out = Map::new();
    for k in ["type", "enum", "default", "description", "format", "minimum", "maximum", "x-prism-block", "x-prism-read-by", "x-prism-settings"] {
        if let Some(v) = raw.get(k).or_else(|| entry.get(k)) {
            out.insert(k.to_string(), v.clone());
        }
    }
    // The type's own description, when the field has one of its own too.
    if let (Some(own), Some(of_type)) = (raw.get("description"), entry.get("description"))
        && own != of_type
    {
        out.insert("type_description".into(), of_type.clone());
    }
    if let Some(alts) = ["anyOf", "oneOf"].iter().find_map(|k| entry.get(*k).and_then(Value::as_array)) {
        let kinds: Vec<Value> = alts.iter().map(|v| resolve(root, v)).map(|v| v.get("const").or_else(|| v.get("type")).or_else(|| v.get("enum")).cloned().unwrap_or(json!("object"))).collect();
        out.insert("one_of".into(), Value::Array(kinds));
    }
    if let Some(props) = entry.get("properties").and_then(Value::as_object) {
        out.insert("keys".into(), json!(props.keys().collect::<Vec<_>>()));
    }
    if let Some(items) = entry.get("items") {
        let items = resolve(root, items);
        out.insert("items".into(), items.get("type").cloned().unwrap_or(json!("object")));
        if let Some(props) = items.get("properties").and_then(Value::as_object) {
            out.insert("item_keys".into(), json!(props.keys().collect::<Vec<_>>()));
        }
    }
    Value::Object(out)
}

/// A TOML value checked against its schema: what does not fit, each with its path (`repos[1].forge:
/// "gitlub" is not one of "gitlab", "github"`). Lenient where the schema is: an unknown key of a
/// table is said only when the schema closes it (`additionalProperties: false`) or names it
/// nowhere (a typo), never refused for a block this build lacks.
pub fn check(root: &Value, value: &Value) -> Vec<Issue> {
    let mut out = Vec::new();
    walk(root, resolve(root, root), value, "", false, &mut out);
    out
}

/// What does not fit the schema: an error (a type, a value), or a warning (a name it does not know,
/// which may be a block of another build).
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct Issue {
    pub message: String,
    pub warning: bool,
}

trait Push {
    fn err(&mut self, message: String);
    fn warn(&mut self, message: String);
}

impl Push for Vec<Issue> {
    fn err(&mut self, message: String) {
        Vec::push(self, Issue { message, warning: false });
    }
    fn warn(&mut self, message: String) {
        Vec::push(self, Issue { message, warning: true });
    }
}

/// `lenient`: under a schema inferred from a block's defaults, which may not name every key.
fn walk(root: &Value, schema: &Value, value: &Value, at: &str, lenient: bool, out: &mut Vec<Issue>) {
    let schema = resolve(root, schema);
    let lenient = lenient || schema.get("x-prism-inferred").is_some();
    let name = if at.is_empty() { "the file".to_string() } else { at.to_string() };
    if let Some(alts) = ["anyOf", "oneOf"].iter().find_map(|k| schema.get(*k).and_then(Value::as_array)) {
        // One of them must fit.
        let fits = alts.iter().any(|alt| {
            let mut errs = Vec::new();
            walk(root, alt, value, at, lenient, &mut errs);
            errs.iter().all(|e| e.warning)
        });
        if !fits {
            let consts: Vec<String> = alts.iter().map(|a| resolve(root, a)).filter_map(|a| a.get("const").or_else(|| a.get("enum")).map(|v| v.to_string())).collect();
            if consts.is_empty() {
                out.err(format!("{name}: {} does not fit its type", short(value)));
            } else {
                out.err(format!("{name}: {} is not one of {}", short(value), consts.join(", ")));
            }
        }
        return;
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        let list: Vec<String> = allowed.iter().map(Value::to_string).collect();
        out.err(format!("{name}: {} is not one of {}", short(value), list.join(", ")));
        return;
    }
    if let Some(c) = schema.get("const")
        && c != value
    {
        out.err(format!("{name}: {} is not {c}", short(value)));
        return;
    }
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if !types.is_empty() && !types.iter().any(|t| fits_type(t, value)) {
        out.err(format!("{name}: expected {}, found {}", types.join(" or "), short(value)));
        return;
    }
    match value {
        Value::Object(keys) => {
            let props = schema.get("properties").and_then(Value::as_object);
            let extra = schema.get("additionalProperties");
            for (k, v) in keys {
                let path = if at.is_empty() { k.clone() } else { format!("{at}.{k}") };
                match props.and_then(|p| p.get(k)) {
                    Some(s) => walk(root, s, v, &path, lenient, out),
                    None => match extra {
                        Some(Value::Bool(false)) => out.err(format!("{path}: unknown key")),
                        Some(s @ Value::Object(_)) => walk(root, s, v, &path, lenient, out),
                        // The file's top: a table no struct nor block of this build reads.
                        _ if at.is_empty() => out.warn(format!("{k}: no table nor key of Prism or of a block of this build is named so (a typo, or a block of another build)")),
                        _ if props.is_some() && !lenient => out.warn(format!("{path}: not a key Prism reads in [{}] (a typo?)", at.split(['.', '[']).next().unwrap_or(at))),
                        _ => {}
                    },
                }
            }
        }
        Value::Array(items) => {
            if let Some(item) = schema.get("items") {
                for (i, v) in items.iter().enumerate() {
                    walk(root, item, v, &format!("{at}[{i}]"), lenient, out);
                }
            }
        }
        _ => {}
    }
}

fn fits_type(t: &str, v: &Value) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "integer" => v.as_i64().is_some() || v.as_u64().is_some(),
        "number" => v.is_number(),
        "null" => v.is_null(),
        _ => true,
    }
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 60 { format!("{}…", s.chars().take(60).collect::<String>()) } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each table of config.toml says where Settings shows it (`prism_explain_key`): a table added
    /// without its page here is caught.
    #[test]
    fn every_table_has_its_settings_page() {
        let tables = serde_json::to_value(crate::config::parsed("")).unwrap();
        let lost: Vec<&String> = tables.as_object().unwrap().keys().filter(|t| settings_page(t, None).starts_with("its block's page")).collect();
        assert!(lost.is_empty(), "tables with no page in schema::settings_page: {lost:?}");
    }

    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "forge": { "$ref": "#/$defs/Forge" },
                "repos": { "type": "array", "items": { "$ref": "#/$defs/Repo" } },
            },
            "$defs": {
                "Forge": { "type": "object", "properties": { "provider": { "$ref": "#/$defs/Provider" }, "enabled": { "type": "boolean", "default": true, "description": "On." } } },
                "Provider": { "type": "string", "enum": ["gitlab", "github"] },
                "Repo": { "type": "object", "properties": { "name": { "type": "string" }, "forge": { "anyOf": [{ "$ref": "#/$defs/Provider" }, { "type": "null" }] } } },
            },
        })
    }

    #[test]
    fn a_key_is_found_through_refs_and_lists() {
        let s = schema();
        assert_eq!(key(&s, "forge.enabled").unwrap()["default"], true);
        assert_eq!(summary(&s, key(&s, "repos[].forge").unwrap())["enum"][1], "github");
        assert_eq!(key(&s, "repos[0].name").unwrap()["type"], "string");
        assert!(key(&s, "forge.nope").is_none());
        assert_eq!(summary(&s, key(&s, "forge").unwrap())["keys"], json!(["enabled", "provider"]));
    }

    #[test]
    fn checks_types_values_and_names() {
        let s = schema();
        let ok = json!({ "forge": { "provider": "github" }, "repos": [{ "name": "web", "forge": "github" }] });
        assert!(check(&s, &ok).is_empty(), "{:?}", check(&s, &ok));
        let bad = json!({ "forge": { "enabled": "yes", "provder": "x" }, "repos": [{ "name": "web", "forge": "gitlub" }], "shiping": {} });
        let errs: Vec<String> = check(&s, &bad).into_iter().map(|e| format!("{}{}", if e.warning { "w " } else { "" }, e.message)).collect();
        assert!(errs.iter().any(|e| e.starts_with("forge.enabled: expected boolean")), "{errs:?}");
        assert!(errs.iter().any(|e| e.starts_with("w forge.provder: not a key Prism reads in [forge]")), "{errs:?}");
        assert!(errs.iter().any(|e| e.starts_with("repos[0].forge: \"gitlub\"")), "{errs:?}");
        assert!(errs.iter().any(|e| e.starts_with("w shiping:")), "{errs:?}");
    }

    #[test]
    fn a_block_schema_is_inferred_from_its_defaults() {
        let s = crate::blocks::infer_schema(&json!({ "enabled": false, "every": 30, "hosts": ["a"] }));
        assert_eq!(s["properties"]["enabled"]["type"], "boolean");
        assert_eq!(s["properties"]["every"]["default"], 30);
        assert_eq!(s["properties"]["hosts"]["items"]["type"], "string");
    }
}
