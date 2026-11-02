//! The snapshot as a phone (and the window) gets it: filtered on the Mac for a phone (what it may
//! not see is never sent), then as changes to the last one sent.

use serde_json::{Map, Value};

/// The snapshot's JSON without what stays on the Mac: the remote's own state (its pairing token),
/// Prism's own warnings and errors (paths, hosts, a mail's error), and the mail unless `[remote]
/// mail` lets it out.
pub fn filter(mut snap: Value, mail: bool) -> Value {
    if let Some(obj) = snap.as_object_mut() {
        obj.remove("remote");
        obj.remove("problems");
        if !mail {
            obj.insert("mail".into(), Value::Null);
            if let Some(errors) = obj.get_mut("errors").and_then(Value::as_object_mut) {
                errors.remove("mail");
            }
        }
    }
    snap
}

/// One change: the value now at `path`, or the key removed (`delete`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Change {
    pub path: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub delete: bool,
}

/// What changed from `old` to `new`: objects compared key by key, arrays of the same length item
/// by item (the index as the path's key: an agent's status changed is that agent, not all of
/// them, a day of Tokens is that day, not the year), anything else replaced whole.
pub fn diff(old: &Value, new: &Value) -> Vec<Change> {
    let mut out = Vec::new();
    walk(&mut Vec::new(), old, new, &mut out);
    out
}

fn walk(path: &mut Vec<String>, old: &Value, new: &Value, out: &mut Vec<Change>) {
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => objects(path, a, b, out),
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() && !a.is_empty() => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                path.push(i.to_string());
                walk(path, x, y, out);
                path.pop();
            }
        }
        _ if old == new => {}
        _ => out.push(Change { path: path.clone(), value: Some(new.clone()), delete: false }),
    }
}

fn objects(path: &mut Vec<String>, a: &Map<String, Value>, b: &Map<String, Value>, out: &mut Vec<Change>) {
    for key in a.keys().filter(|k| !b.contains_key(*k)) {
        let mut p = path.clone();
        p.push(key.clone());
        out.push(Change { path: p, value: None, delete: true });
    }
    for (key, value) in b {
        path.push(key.clone());
        match a.get(key) {
            Some(old) => walk(path, old, value, out),
            None => out.push(Change { path: path.clone(), value: Some(value.clone()), delete: false }),
        }
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// What the page does with the changes (`remote/live.ts`), to check the round trip.
    fn apply(mut v: Value, changes: &[Change]) -> Value {
        for c in changes {
            let Some((last, parents)) = c.path.split_last() else {
                v = c.value.clone().unwrap();
                continue;
            };
            let mut at = &mut v;
            for p in parents {
                at = match at {
                    Value::Array(a) => &mut a[p.parse::<usize>().unwrap()],
                    _ => at.as_object_mut().unwrap().entry(p.clone()).or_insert(json!({})),
                };
            }
            if let Value::Array(a) = at {
                a[last.parse::<usize>().unwrap()] = c.value.clone().unwrap();
                continue;
            }
            let obj = at.as_object_mut().unwrap();
            if c.delete {
                obj.remove(last);
            } else {
                obj.insert(last.clone(), c.value.clone().unwrap());
            }
        }
        v
    }

    #[test]
    fn changes_rebuild_the_new_snapshot_nulls_included() {
        let old = json!({ "usage": { "pct": 10 }, "agents": [1, 2], "errors": { "gitlab": "x" }, "tracker": "clickup", "same": { "a": 1 } });
        let new = json!({ "usage": null, "agents": [1, 2, 3], "errors": {}, "tracker": "clickup", "same": { "a": 1 }, "sprint": { "n": 4 } });
        let changes = diff(&old, &new);
        assert_eq!(apply(old.clone(), &changes), new);
        assert!(!changes.iter().any(|c| c.path == ["same"] || c.path == ["tracker"]));
        assert!(diff(&new, &new).is_empty());
    }

    #[test]
    fn an_array_of_the_same_length_changes_item_by_item() {
        let old = json!({ "agents": [{ "id": "a", "status": "running" }, { "id": "b", "status": "idle" }], "days": [1, 2, 3] });
        let new = json!({ "agents": [{ "id": "a", "status": "running" }, { "id": "b", "status": "needs_you" }], "days": [1, 2, 4] });
        let changes = diff(&old, &new);
        assert_eq!(changes.iter().map(|c| c.path.join(".")).collect::<Vec<_>>(), ["agents.1.status", "days.2"]);
        assert_eq!(apply(old, &changes), new);
        // Grown or shrunk: replaced whole.
        let longer = json!({ "days": [1, 2, 3, 4] });
        assert_eq!(diff(&json!({ "days": [1, 2, 3] }), &longer)[0].path, ["days"]);
    }

    #[test]
    fn the_mail_and_the_pairing_stay_on_the_mac() {
        let snap = json!({ "mail": { "unread": [1] }, "errors": { "mail": "x", "gitlab": "y" }, "remote": { "pairing": { "url": "secret" } }, "problems": [{ "message": "x" }] });
        let out = filter(snap.clone(), false);
        assert_eq!(out, json!({ "mail": null, "errors": { "gitlab": "y" } }));
        assert_eq!(filter(snap, true)["mail"], json!({ "unread": [1] }));
    }
}
