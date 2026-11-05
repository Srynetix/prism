//! An action's `when`: on which lines its button shows. Comparisons of a line's field with a value,
//! joined by `&&` or `||` (`&&` first), no parentheses: `state != 'ok'`,
//! `kind == 'disk' && size > 10`, `stale` (a field that is true, a text or a number not empty).

use anyhow::{bail, Result};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
enum Lit {
    Text(String),
    Num(f64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
enum Test {
    Truthy(String),
    Cmp(String, Op, Lit),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Alternatives (`||`) of conjunctions (`&&`).
#[derive(Debug, Clone, PartialEq)]
pub struct When(Vec<Vec<Test>>);

pub fn parse(text: &str) -> Result<When> {
    let mut any = Vec::new();
    for alt in split(text, "||") {
        let mut all = Vec::new();
        for part in split(&alt, "&&") {
            all.push(test(part.trim())?);
        }
        any.push(all);
    }
    Ok(When(any))
}

/// `text` cut at `sep` outside quotes.
fn split(text: &str, sep: &str) -> Vec<String> {
    let (mut out, mut cur, mut quote) = (Vec::new(), String::new(), None::<char>);
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if quote.is_none() && rest.starts_with(sep) {
            out.push(std::mem::take(&mut cur));
            rest = &rest[sep.len()..];
            continue;
        }
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            _ => {}
        }
        cur.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out.push(cur);
    out
}

fn test(text: &str) -> Result<Test> {
    for (sym, op) in [("==", Op::Eq), ("!=", Op::Ne), ("<=", Op::Le), (">=", Op::Ge), ("<", Op::Lt), (">", Op::Gt)] {
        if let Some((field, lit)) = text.split_once(sym) {
            return Ok(Test::Cmp(field_name(field)?, op, literal(lit.trim())?));
        }
    }
    Ok(Test::Truthy(field_name(text)?))
}

fn field_name(text: &str) -> Result<String> {
    let f = text.trim().trim_start_matches("item.");
    if f.is_empty() || !f.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        bail!("{text:?} is not a field");
    }
    Ok(f.to_string())
}

fn literal(text: &str) -> Result<Lit> {
    let quoted = |q: char| text.len() >= 2 && text.starts_with(q) && text.ends_with(q);
    Ok(if quoted('\'') || quoted('"') {
        Lit::Text(text[1..text.len() - 1].to_string())
    } else if text == "true" || text == "false" {
        Lit::Bool(text == "true")
    } else if let Ok(n) = text.parse::<f64>() {
        Lit::Num(n)
    } else {
        bail!("{text:?}: a 'text', a number, true or false")
    })
}

impl When {
    /// Whether it holds for `item` (a line's fields).
    pub fn holds(&self, item: &serde_json::Map<String, Value>) -> bool {
        self.0.iter().any(|all| all.iter().all(|t| t.holds(item)))
    }
}

impl Test {
    fn holds(&self, item: &serde_json::Map<String, Value>) -> bool {
        match self {
            Test::Truthy(f) => match item.get(f) {
                Some(Value::Bool(b)) => *b,
                Some(Value::String(s)) => !s.is_empty(),
                Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
                _ => false,
            },
            Test::Cmp(f, op, lit) => {
                let v = item.get(f).unwrap_or(&Value::Null);
                let ord = match (v, lit) {
                    (Value::String(s), Lit::Text(t)) => Some(s.as_str().cmp(t.as_str())),
                    (Value::Number(n), Lit::Num(m)) => n.as_f64().and_then(|n| n.partial_cmp(m)),
                    (Value::String(s), Lit::Num(m)) => s.trim().parse::<f64>().ok().and_then(|n| n.partial_cmp(m)),
                    (Value::Bool(b), Lit::Bool(c)) => Some(b.cmp(c)),
                    _ => None,
                };
                match (op, ord) {
                    (Op::Eq, o) => o == Some(std::cmp::Ordering::Equal),
                    // Absent or of another type: not equal.
                    (Op::Ne, o) => o != Some(std::cmp::Ordering::Equal),
                    (_, None) => false,
                    (Op::Lt, Some(o)) => o.is_lt(),
                    (Op::Le, Some(o)) => o.is_le(),
                    (Op::Gt, Some(o)) => o.is_gt(),
                    (Op::Ge, Some(o)) => o.is_ge(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn holds(when: &str, item: Value) -> bool {
        parse(when).unwrap().holds(item.as_object().unwrap())
    }

    #[test]
    fn compares_fields() {
        assert!(holds("state != 'ok'", json!({ "state": "ko" })));
        assert!(!holds("state != 'ok'", json!({ "state": "ok" })));
        assert!(holds("state != 'ok'", json!({})));
        assert!(holds("item.kind == \"disk\" && size > 10", json!({ "kind": "disk", "size": 12 })));
        assert!(!holds("kind == 'disk' && size > 10", json!({ "kind": "disk", "size": 2 })));
        assert!(holds("state == 'ko' || stale", json!({ "state": "ok", "stale": true })));
        assert!(holds("title == 'a && b'", json!({ "title": "a && b" })));
        assert!(!holds("size > 1", json!({ "size": "big" })));
    }

    #[test]
    fn refuses_what_is_not_a_test() {
        assert!(parse("state = 'ok'").is_err());
        assert!(parse("state == ok").is_err());
        assert!(parse("").is_err());
    }
}
