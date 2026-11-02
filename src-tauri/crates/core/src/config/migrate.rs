//! Former layouts of the config, moved to their current tables when read.


use super::*;

/// The keys of `[forge]` that were `[gitlab]`'s.
pub const FORMER_GITLAB_KEYS: [&str; 7] = ["enabled", "main_branch", "release_prefix", "merged_days", "poll_seconds", "ci_health", "ci_health_days"];

/// The keys of `[agents.claude]` that were elsewhere: (table, former key, key).
pub const FORMER_CLAUDE_KEYS: [(&str, &str, &str); 7] = [
    ("agents", "cost_alert", "cost_alert"),
    ("agents", "show_desktop", "show_desktop"),
    ("agents", "count_desktop", "count_desktop"),
    ("sessions", "dir", "sessions_dir"),
    ("sessions", "transcripts", "transcripts"),
    ("sessions", "poll_seconds", "sessions_poll"),
    ("usage", "refresh_token", "refresh_token"),
];

/// One layer of the config (a pack, or the file itself), before they merge: its former `[gitlab]`
/// keys move to `[forge]`, its former `[clickup]` ones to `[tracker]`, Claude Code's to
/// `[agents.claude]`; the new table wins when it sets the same one. In memory only: Settings shows
/// the file as it is.
pub fn migrate(layer: &mut toml::Table) {
    move_keys(layer, "gitlab", "forge", &FORMER_GITLAB_KEYS);
    move_keys(layer, "clickup", "tracker", &FORMER_CLICKUP_KEYS);
    move_runners(layer);
    move_claude(layer);
    move_kinds(layer);
}

/// `[agents] categories` (names) became `[[agents.kinds]]` (tables, a name and the commands that
/// make one), a name alone read as such; `[agents] stages` went (an agent's progress is its status
/// and its MR's).
fn move_kinds(layer: &mut toml::Table) {
    let Some(toml::Value::Table(agents)) = layer.get_mut("agents") else { return };
    agents.remove("stages");
    if let Some(former) = agents.remove("categories") {
        agents.entry("kinds").or_insert(former);
    }
    if let Some(toml::Value::Array(kinds)) = agents.get_mut("kinds") {
        for k in kinds.iter_mut() {
            if let toml::Value::String(name) = k {
                *k = toml::Value::Table(toml::Table::from_iter([("name".to_string(), toml::Value::String(std::mem::take(name)))]));
            }
        }
    }
}

/// Claude Code's keys, once in `[agents]`, `[sessions]` and `[usage]`, to `[agents.claude]`; the
/// last two tables go once empty.
fn move_claude(layer: &mut toml::Table) {
    let mut moved = Vec::new();
    for (table, former, key) in FORMER_CLAUDE_KEYS {
        if let Some(toml::Value::Table(old)) = layer.get_mut(table)
            && let Some(v) = old.remove(former)
        {
            moved.push((key, v));
        }
    }
    for table in ["sessions", "usage"] {
        if layer.get(table).and_then(toml::Value::as_table).is_some_and(toml::Table::is_empty) {
            layer.remove(table);
        }
    }
    if moved.is_empty() {
        return;
    }
    let toml::Value::Table(agents) = layer.entry("agents").or_insert_with(|| toml::Value::Table(toml::Table::new())) else { return };
    if let toml::Value::Table(claude) = agents.entry("claude").or_insert_with(|| toml::Value::Table(toml::Table::new())) {
        for (k, v) in moved {
            claude.entry(k).or_insert(v);
        }
    }
}

/// `[runners]` was the clusters' switch, their context and the GitLab runners' namespace: the first
/// two move to `[clusters]`, the namespace becomes a `[[pods]]` target on the default context
/// (unless the layer already has its own targets).
fn move_runners(layer: &mut toml::Table) {
    let Some(toml::Value::Table(old)) = layer.get_mut("runners") else { return };
    let enabled = old.remove("enabled");
    let context = old.remove("kube_context");
    let namespace = old.remove("namespace").and_then(|n| n.as_str().map(str::trim).filter(|n| !n.is_empty()).map(String::from));
    if old.is_empty() {
        layer.remove("runners");
    }
    if (enabled.is_some() || context.is_some())
        && let toml::Value::Table(clusters) = layer.entry("clusters").or_insert_with(|| toml::Value::Table(toml::Table::new()))
    {
        if let Some(v) = enabled {
            clusters.entry("enabled").or_insert(v);
        }
        if let Some(v) = context {
            clusters.entry("default_context").or_insert(v);
        }
    }
    if let Some(ns) = namespace
        && !layer.contains_key("pods")
    {
        let mut target = toml::Table::new();
        target.insert("name".into(), "Runners".into());
        target.insert("namespaces".into(), toml::Value::Array(vec![ns.into()]));
        layer.insert("pods".into(), toml::Value::Array(vec![toml::Value::Table(target)]));
    }
}

/// The keys of `[clusters]` that were `[runners]`', by their former name.
pub const FORMER_RUNNERS_KEYS: [(&str, &str); 2] = [("enabled", "enabled"), ("default_context", "kube_context")];

fn move_keys(layer: &mut toml::Table, from: &str, to: &str, keys: &[&str]) {
    let Some(toml::Value::Table(old)) = layer.get_mut(from) else { return };
    let moved: Vec<(String, toml::Value)> = keys.iter().filter_map(|k| Some((k.to_string(), old.remove(*k)?))).collect();
    if moved.is_empty() {
        return;
    }
    if let toml::Value::Table(new) = layer.entry(to).or_insert_with(|| toml::Value::Table(toml::Table::new())) {
        for (k, v) in moved {
            new.entry(k).or_insert(v);
        }
    }
}
