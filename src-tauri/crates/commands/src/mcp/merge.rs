//! A TOML fragment an agent proposes, merged into `config.toml` as it is (its comments, order and
//! formatting kept, `toml_edit`): a table's keys set one by one, an entry of an array of tables
//! (`[[repos]]`, `[[views]]`, `[[custom_blocks]]`…) merged into the one of the same `id` or `name`,
//! otherwise added. And what is shown of a config: any value that looks like a secret hidden.

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, Table, Value};

/// The keys an entry of an array of tables is known by, in order.
const IDENTITY: &[&str] = &["id", "name"];

/// `file` with `fragment` merged in, and the keys it sets (`forge.bare_mr_repo`, `repos[acme-web].forge`).
pub fn merge(file: &str, fragment: &str) -> Result<(String, Vec<String>)> {
    let mut doc: DocumentMut = file.parse().context("config.toml is not valid TOML")?;
    let add: DocumentMut = fragment.parse().context("the fragment is not valid TOML")?;
    let mut set = Vec::new();
    merge_table(doc.as_table_mut(), add.as_table(), "", &mut set);
    Ok((doc.to_string(), set))
}

/// `[[views]]` replaced whole by `views` (a fragment of `[[views]]` only), the rest as it is.
pub fn replace_views(file: &str, views: &str) -> Result<String> {
    let mut doc: DocumentMut = file.parse().context("config.toml is not valid TOML")?;
    let add: DocumentMut = views.parse().context("the views are not valid TOML")?;
    let item = add.get("views").cloned().context("no [[views]] in the proposal")?;
    anyhow::ensure!(add.as_table().len() == 1, "only [[views]] in a layout proposal");
    doc.insert("views", item);
    Ok(doc.to_string())
}

fn path(at: &str, key: &str) -> String {
    if at.is_empty() { key.to_string() } else { format!("{at}.{key}") }
}

fn merge_table(into: &mut Table, add: &Table, at: &str, set: &mut Vec<String>) {
    for (key, item) in add.iter() {
        let here = path(at, key);
        match (into.get_mut(key), item) {
            (Some(existing), new) if is_table(existing) && is_table(new) => {
                if let (Some(dst), Some(src)) = (as_table_mut(existing), as_table(new)) {
                    merge_table(dst, &src, &here, set);
                }
            }
            (Some(Item::ArrayOfTables(existing)), Item::ArrayOfTables(new)) => {
                for entry in new.iter() {
                    let id = identity(entry);
                    match id.as_ref().and_then(|(k, v)| existing.iter_mut().find(|e| identity(e).is_some_and(|(ek, ev)| ek == *k && ev == *v))) {
                        Some(found) => merge_table(found, entry, &format!("{here}[{}]", id.as_ref().map_or("", |(_, v)| v.as_str())), set),
                        None => {
                            existing.push(entry.clone());
                            set.push(format!("{here}[{}]", id.map_or_else(|| "+".to_string(), |(_, v)| v)));
                        }
                    }
                }
            }
            // The same value: nothing set (an entry's own `name`, a key repeated as it is).
            (Some(existing), new) if same(existing, new) => {}
            (_, new) => {
                into.insert(key, new.clone());
                set.push(here);
            }
        }
    }
}

fn same(a: &Item, b: &Item) -> bool {
    match (a.as_value(), b.as_value()) {
        (Some(x), Some(y)) => x.to_string().trim() == y.to_string().trim(),
        _ => false,
    }
}

/// `id = "…"` or `name = "…"` of an entry.
fn identity(t: &Table) -> Option<(&'static str, String)> {
    IDENTITY.iter().find_map(|k| t.get(k).and_then(|v| v.as_str()).map(|v| (*k, v.to_string())))
}

fn is_table(item: &Item) -> bool {
    matches!(item, Item::Table(_)) || matches!(item, Item::Value(Value::InlineTable(_)))
}

fn as_table(item: &Item) -> Option<Table> {
    match item {
        Item::Table(t) => Some(t.clone()),
        Item::Value(Value::InlineTable(t)) => Some(t.clone().into_table()),
        _ => None,
    }
}

fn as_table_mut(item: &mut Item) -> Option<&mut Table> {
    if let Item::Value(Value::InlineTable(t)) = item {
        *item = Item::Table(std::mem::take(t).into_table());
    }
    item.as_table_mut()
}

/// A key whose value would be a secret, were one written there (none should: tokens live in the
/// keychain).
pub fn secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["token", "secret", "password", "passwd", "api_key", "apikey", "private_key", "credential"].iter().any(|s| k.contains(s))
}

/// `text` with the values of secret-looking keys (and strings that look like a token) hidden; a
/// `keychain:` reference shown, it is no secret.
pub fn mask(text: &str) -> String {
    let Ok(mut doc) = text.parse::<DocumentMut>() else { return text.to_string() };
    mask_table(doc.as_table_mut());
    doc.to_string()
}

fn mask_table(t: &mut Table) {
    mask_table_in(t, false);
}

/// `secret`: under a secret-looking key (`[secrets]`, `[x.credentials]`): every value hidden.
fn mask_table_in(t: &mut Table, secret: bool) {
    for (key, item) in t.iter_mut() {
        let secret = secret || secret_key(key.get());
        mask_item(item, secret);
    }
}

fn mask_item(item: &mut Item, secret: bool) {
    match item {
        Item::Table(t) => mask_table_in(t, secret),
        Item::ArrayOfTables(a) => a.iter_mut().for_each(|t| mask_table_in(t, secret)),
        Item::Value(v) => mask_value(v, secret),
        Item::None => {}
    }
}

fn mask_value(v: &mut Value, secret: bool) {
    match v {
        Value::String(s) => {
            let text = s.value();
            if !text.starts_with("keychain:") && !text.is_empty() && (secret || looks_like_token(text)) {
                let decor = s.decor().clone();
                *s = toml_edit::Formatted::new("•••".to_string());
                *s.decor_mut() = decor;
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|v| mask_value(v, secret)),
        Value::InlineTable(t) => {
            for (k, v) in t.iter_mut() {
                let secret = secret || secret_key(k.get());
                mask_value(v, secret);
            }
        }
        _ => {}
    }
}

/// A JSON value (a key explained) hidden as `mask` hides TOML: `secret`, it sits under a
/// secret-looking key.
pub fn mask_json(v: serde_json::Value, secret: bool) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        J::String(s) if !s.is_empty() && !s.starts_with("keychain:") && (secret || looks_like_token(&s)) => J::String("•••".into()),
        J::Array(a) => J::Array(a.into_iter().map(|v| mask_json(v, secret)).collect()),
        J::Object(o) => J::Object(o.into_iter().map(|(k, v)| {
            let secret = secret || secret_key(&k);
            (k, mask_json(v, secret))
        }).collect()),
        v => v,
    }
}

/// The known shapes of a forge's or a cloud's token: `glpat-…`, `ghp_…`, `github_pat_…`, `xox…`, `sk-…`.
fn looks_like_token(s: &str) -> bool {
    ["glpat-", "ghp_", "gho_", "ghs_", "github_pat_", "xoxb-", "xoxp-", "sk-", "pk_", "AKIA"].iter().any(|p| s.starts_with(p) && s.len() >= 16 && !s.contains(' '))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fragment_sets_keys_and_adds_entries() {
        let file = "# mine\n[forge]\nprovider = \"gitlab\" # here\n\n[[repos]]\nname = \"api\"\npath = \"~/code/api\"\n";
        let (out, set) = merge(file, "[forge]\nbare_mr_repo = \"api\"\n\n[[repos]]\nname = \"acme-web\"\nforge = \"github\"\n\n[[repos]]\nname = \"api\"\nproject = \"g/api\"\n").unwrap();
        assert!(out.starts_with("# mine\n[forge]\nprovider = \"gitlab\" # here\nbare_mr_repo = \"api\"\n"), "{out}");
        assert!(out.contains("name = \"api\"\npath = \"~/code/api\"\nproject = \"g/api\"\n"), "{out}");
        assert!(out.contains("[[repos]]\nname = \"acme-web\"\nforge = \"github\"\n"), "{out}");
        assert_eq!(set, ["forge.bare_mr_repo", "repos[acme-web]", "repos[api].project"]);
    }

    #[test]
    fn an_inline_table_merges_as_a_table() {
        let (out, set) = merge("[agents]\nenabled = true\n", "agents = { prism_mcp = true }\n").unwrap();
        assert!(out.contains("enabled = true") && out.contains("prism_mcp = true"), "{out}");
        assert_eq!(set, ["agents.prism_mcp"]);
    }

    #[test]
    fn the_views_are_replaced_whole() {
        let out = replace_views("[forge]\nenabled = true\n\n[[views]]\nname = \"A\"\n", "[[views]]\nname = \"B\"\ncolumns = [\"mrs\"]\n").unwrap();
        assert!(out.contains("name = \"B\"") && !out.contains("name = \"A\"") && out.contains("[forge]"), "{out}");
        assert!(replace_views("", "[forge]\nx = 1\n").is_err());
    }

    #[test]
    fn secrets_are_hidden() {
        let out = mask("[x]\napi_token = \"abc\" # mine\nother = \"glpat-0123456789abcdef\"\nplain = \"hello\"\nenv = { TOKEN = \"keychain:svc\", PASSWORD = \"p\" }\n");
        assert!(out.contains("api_token = \"•••\" # mine") && out.contains("other = \"•••\"") && out.contains("plain = \"hello\""), "{out}");
        assert!(out.contains("TOKEN = \"keychain:svc\"") && out.contains("PASSWORD = \"•••\""), "{out}");
    }

    #[test]
    fn a_secret_table_hides_all_its_values() {
        let out = mask("[secrets]\ngitlab = \"abc\"\n\n[x.credentials]\nuser = \"p\"\n\n[y]\nplain = \"ok\"\n");
        assert!(out.contains("gitlab = \"•••\"") && out.contains("user = \"•••\"") && out.contains("plain = \"ok\""), "{out}");
        let j = mask_json(serde_json::json!({ "env": { "API_KEY": "k", "HOST": "h" }, "t": "glpat-0123456789abcdef" }), false);
        assert_eq!(j, serde_json::json!({ "env": { "API_KEY": "•••", "HOST": "h" }, "t": "•••" }));
        assert_eq!(mask_json(serde_json::json!("abc"), true), serde_json::json!("•••"));
    }
}
