//! What a custom block's program prints, read as the contract says: a JSON object with a `kind`
//! (`list`, `gauge`, `stats`, `markdown`, `table`), `alerts` and `status`; anything that is not
//! JSON is shown as it is. The output is data from a program nobody watches: every text is
//! cut, a link is http(s) or nothing, and nothing in it says what to run (the actions come
//! from the config only).

use serde::Serialize;
use serde_json::{Map, Value};

/// A text shown on a line (a title, a badge, a cell).
const LINE: usize = 300;
/// A markdown body.
const BODY: usize = 64 * 1024;
const ITEMS: usize = 500;
const ROWS: usize = 1000;
const COLUMNS: usize = 20;
const STATS: usize = 24;
const ALERTS: usize = 20;
/// Fields kept on a line besides the known ones (for `when` and `PRISM_ITEM`).
const FIELDS: usize = 30;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomLevel"))]
pub enum Level {
    Ok,
    Info,
    Warn,
    Ko,
}

impl Level {
    fn read(v: Option<&Value>) -> Option<Self> {
        match v?.as_str()?.trim().to_ascii_lowercase().as_str() {
            "ok" | "success" | "green" => Some(Self::Ok),
            "info" => Some(Self::Info),
            "warn" | "warning" | "yellow" => Some(Self::Warn),
            "ko" | "error" | "fail" | "failed" | "red" | "critical" => Some(Self::Ko),
            _ => None,
        }
    }
}

/// The output read.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomOutput"))]
pub struct Output {
    pub body: Body,
    pub alerts: Vec<Alert>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomBody"))]
pub enum Body {
    List { items: Vec<Item> },
    /// `gauge` (one number) is a `stats` of one.
    Stats { stats: Vec<Stat> },
    Markdown { text: String },
    Table { columns: Vec<Column>, rows: Vec<Item> },
    /// Not JSON: shown as it is, in monospace.
    Raw { text: String },
}

/// A line of a list or a table: its fields (a table's cells by column key), the known ones
/// checked; `actions`, the actions of the config that hold for it (set by the block, never read
/// from the output).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomItem"))]
pub struct Item {
    /// Its key: `id`, else its title, else its place.
    pub key: String,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, string | number | boolean | null>"))]
    pub fields: Map<String, Value>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<Level>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomStat"))]
pub struct Stat {
    /// Its history's key: `key`, else its label.
    pub key: String,
    pub label: String,
    pub value: f64,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn: Option<f64>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ko: Option<f64>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<Level>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomColumn"))]
pub struct Column {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomAlert"))]
pub struct Alert {
    pub severity: Level,
    pub text: String,
    /// What it is about, the same from one read to the next: one alert per key.
    pub key: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomStatus"))]
pub struct Status {
    pub text: String,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<Level>,
}

/// `stdout` read: what it shows, and the reason it is not the contract (JSON that does not follow
/// it: its raw text is shown meanwhile).
pub fn read(stdout: &str) -> (Output, Option<String>) {
    match parse(stdout) {
        Ok(o) => (o, None),
        Err(why) => (raw(stdout), Some(why)),
    }
}

fn raw(stdout: &str) -> Output {
    Output { body: Body::Raw { text: cut(stdout.trim_end(), BODY) }, alerts: Vec::new(), status: None }
}

fn parse(stdout: &str) -> Result<Output, String> {
    let trimmed = stdout.trim();
    let Ok(json) = serde_json::from_str::<Value>(trimmed) else { return Ok(raw(stdout)) };
    let Value::Object(obj) = json else {
        // A plain number or string printed: as it is.
        return if trimmed.starts_with(['{', '[']) { Err(prism_base::i18n::tr("the output is JSON but not an object with a kind").to_string()) } else { Ok(raw(stdout)) };
    };
    let mut body = body(&obj)?;
    unique_keys(&mut body);
    Ok(Output { body, alerts: alerts(obj.get("alerts")), status: status(obj.get("status")) })
}

fn body(obj: &Map<String, Value>) -> Result<Body, String> {
    let kind = obj.get("kind").and_then(Value::as_str).unwrap_or_default();
    Ok(match kind {
        "list" => Body::List { items: array(obj, "items")?.iter().take(ITEMS).enumerate().map(|(i, v)| item(v, i)).collect::<Result<_, _>>()? },
        "gauge" => Body::Stats { stats: vec![stat(&Value::Object(obj.clone()), 0)?] },
        "stats" => Body::Stats { stats: array(obj, "stats")?.iter().take(STATS).enumerate().map(|(i, v)| stat(v, i)).collect::<Result<_, _>>()? },
        "markdown" => Body::Markdown { text: cut(obj.get("text").and_then(Value::as_str).ok_or_else(|| prism_base::i18n::tr("markdown: a text").to_string())?, BODY) },
        "table" => table(obj)?,
        "" => return Err(prism_base::i18n::tr("no kind: list, gauge, stats, markdown or table").to_string()),
        other => return Err(prism_base::tr!("unknown kind {kind}: list, gauge, stats, markdown or table", kind = other)),
    })
}

/// Each line's key (and each stat's, its history's) once in the block: a repeated one gets `#2`,
/// `#3`… — an action finds its line by it, the window keys its rows with it.
fn unique_keys(body: &mut Body) {
    fn dedupe<'a>(keys: impl Iterator<Item = &'a mut String>) {
        let mut seen = std::collections::HashMap::<String, usize>::new();
        for k in keys {
            let n = seen.entry(k.clone()).or_insert(0);
            *n += 1;
            if *n > 1 {
                *k = format!("{k}#{n}");
            }
        }
    }
    match body {
        Body::List { items } => dedupe(items.iter_mut().map(|i| &mut i.key)),
        Body::Table { columns, rows } => {
            let mut seen = std::collections::HashSet::new();
            columns.retain(|c| seen.insert(c.key.clone()));
            dedupe(rows.iter_mut().map(|i| &mut i.key));
        }
        Body::Stats { stats } => dedupe(stats.iter_mut().map(|s| &mut s.key)),
        _ => {}
    }
}

fn array<'a>(obj: &'a Map<String, Value>, key: &str) -> Result<&'a Vec<Value>, String> {
    obj.get(key).and_then(Value::as_array).ok_or_else(|| prism_base::tr!("{key}: a list", key = key))
}

/// A text cut to `max` bytes, at a character's edge.
pub fn cut(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// A link: http(s) only, never `javascript:`, `file:` or the app's own.
fn link(v: Option<&Value>) -> Option<String> {
    let s = v?.as_str()?.trim();
    let lower = s.to_ascii_lowercase();
    ((lower.starts_with("https://") || lower.starts_with("http://")) && s.len() <= 2048 && !s.chars().any(char::is_control)).then(|| s.to_string())
}

/// A field shown: a text cut, a number, a switch; anything else dropped.
fn scalar(v: &Value) -> Option<Value> {
    match v {
        Value::String(s) => Some(Value::String(cut(s, LINE))),
        Value::Number(_) | Value::Bool(_) | Value::Null => Some(v.clone()),
        _ => None,
    }
}

fn item(v: &Value, i: usize) -> Result<Item, String> {
    let obj = v.as_object().ok_or_else(|| prism_base::i18n::tr("items: each one an object").to_string())?;
    let mut fields = Map::new();
    for (k, v) in obj.iter().filter(|(k, _)| !matches!(k.as_str(), "url" | "actions" | "state")).take(FIELDS) {
        if let Some(v) = scalar(v) {
            fields.insert(cut(k, 40), v);
        }
    }
    let text = |k: &str| fields.get(k).and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_f64().map(|n| n.to_string())));
    let key = text("id").or_else(|| text("title")).unwrap_or_else(|| i.to_string());
    let state = Level::read(obj.get("state"));
    if let Some(s) = state {
        // `when` reads it as the others.
        fields.insert("state".into(), serde_json::to_value(s).unwrap_or_default());
    }
    Ok(Item { key, fields, state, url: link(obj.get("url")), actions: Vec::new() })
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())).filter(|n: &f64| n.is_finite())
}

fn stat(v: &Value, i: usize) -> Result<Stat, String> {
    let obj = v.as_object().ok_or_else(|| prism_base::i18n::tr("stats: each one an object").to_string())?;
    let value = num(obj.get("value")).ok_or_else(|| prism_base::i18n::tr("a gauge or a stat: a number value").to_string())?;
    let label = obj.get("label").and_then(Value::as_str).map(|s| cut(s, LINE)).unwrap_or_default();
    let key = obj.get("key").and_then(Value::as_str).map(|s| cut(s, 80)).filter(|k| !k.is_empty()).unwrap_or_else(|| if label.is_empty() { i.to_string() } else { label.clone() });
    let (warn, ko) = (num(obj.get("warn")), num(obj.get("ko")));
    let state = Level::read(obj.get("state")).or_else(|| threshold(value, warn, ko));
    Ok(Stat { key, label, value, min: num(obj.get("min")), max: num(obj.get("max")), unit: obj.get("unit").and_then(Value::as_str).map(|s| cut(s, 20)), warn, ko, state })
}

/// Its state by its thresholds: higher is worse, unless `ko` is below `warn`.
fn threshold(value: f64, warn: Option<f64>, ko: Option<f64>) -> Option<Level> {
    let lower_worse = matches!((warn, ko), (Some(w), Some(k)) if k < w);
    let past = |t: f64| if lower_worse { value <= t } else { value >= t };
    match (warn, ko) {
        (None, None) => None,
        _ if ko.is_some_and(past) => Some(Level::Ko),
        _ if warn.is_some_and(past) => Some(Level::Warn),
        _ => Some(Level::Ok),
    }
}

fn table(obj: &Map<String, Value>) -> Result<Body, String> {
    let columns: Vec<Column> = array(obj, "columns")?
        .iter()
        .take(COLUMNS)
        .map(|c| match c {
            Value::String(s) => Ok(Column { key: cut(s, 40), label: cut(s, LINE) }),
            Value::Object(o) => {
                let key = o.get("key").and_then(Value::as_str).ok_or_else(|| prism_base::i18n::tr("columns: a key for each").to_string())?;
                Ok(Column { key: cut(key, 40), label: cut(o.get("label").and_then(Value::as_str).unwrap_or(key), LINE) })
            }
            _ => Err(prism_base::i18n::tr("columns: texts or objects with a key").to_string()),
        })
        .collect::<Result<_, String>>()?;
    let rows = array(obj, "rows")?
        .iter()
        .take(ROWS)
        .enumerate()
        .map(|(i, r)| match r {
            // By position: the cells in the columns' order.
            Value::Array(cells) => item(&Value::Object(columns.iter().zip(cells).map(|(c, v)| (c.key.clone(), v.clone())).collect()), i),
            Value::Object(_) => item(r, i),
            _ => Err(prism_base::i18n::tr("rows: lists of cells or objects").to_string()),
        })
        .collect::<Result<_, String>>()?;
    Ok(Body::Table { columns, rows })
}

fn alerts(v: Option<&Value>) -> Vec<Alert> {
    let mut out: Vec<Alert> = Vec::new();
    for a in v.and_then(Value::as_array).into_iter().flatten().take(ALERTS) {
        let Some(text) = a.get("text").and_then(Value::as_str).filter(|t| !t.trim().is_empty()) else { continue };
        let key = a.get("key").and_then(Value::as_str).map(|k| cut(k, 80)).unwrap_or_else(|| cut(text, 80));
        // One per key: the later one of a read is dropped.
        if out.iter().any(|o| o.key == key) {
            continue;
        }
        out.push(Alert { severity: Level::read(a.get("severity")).unwrap_or(Level::Warn), text: cut(text, LINE), key });
    }
    out
}

fn status(v: Option<&Value>) -> Option<Status> {
    match v? {
        Value::String(s) if !s.trim().is_empty() => Some(Status { text: cut(s, 40), state: None }),
        Value::Object(o) => Some(Status { text: cut(o.get("text").and_then(Value::as_str)?, 40), state: Level::read(o.get("state")) }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_list_with_alerts_and_status() {
        let out = parse(r#"{"kind":"list","items":[{"title":"NAS","sub":"2 h ago","state":"ok","url":"https://nas"},{"title":"Laptop","state":"ko","badge":"3 d","url":"javascript:alert(1)","actions":["rm"]}],"alerts":[{"severity":"ko","text":"Laptop not backed up","key":"laptop"},{"text":"again","key":"laptop"}],"status":{"text":"1 KO","state":"ko"}}"#).unwrap();
        let Body::List { items } = &out.body else { panic!("{out:?}") };
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].key.as_str(), items[0].url.as_deref(), items[0].state), ("NAS", Some("https://nas"), Some(Level::Ok)));
        // A javascript: link dropped; an `actions` field of the output never read.
        assert_eq!((items[1].url.as_deref(), items[1].fields.get("actions")), (None, None));
        assert_eq!(out.alerts.len(), 1);
        assert_eq!(out.status, Some(Status { text: "1 KO".into(), state: Some(Level::Ko) }));
    }

    #[test]
    fn reads_a_gauge_stats_and_a_table() {
        let out = parse(r#"{"kind":"gauge","value":42,"max":100}"#).unwrap();
        assert_eq!(out.body, Body::Stats { stats: vec![Stat { key: "0".into(), label: String::new(), value: 42.0, min: None, max: Some(100.0), unit: None, warn: None, ko: None, state: None }] });
        let out = parse(r#"{"kind":"stats","stats":[{"label":"Disk","value":91,"warn":80,"ko":90,"unit":"%"},{"label":"Battery","value":30,"warn":40,"ko":10}]}"#).unwrap();
        let Body::Stats { stats } = &out.body else { panic!() };
        assert_eq!((stats[0].state, stats[1].state), (Some(Level::Ko), Some(Level::Warn)));
        let out = parse(r#"{"kind":"table","columns":["host",{"key":"ms","label":"Latency"}],"rows":[["a",12],{"host":"b","ms":40,"state":"warn"}]}"#).unwrap();
        let Body::Table { columns, rows } = &out.body else { panic!() };
        assert_eq!(columns[1].label, "Latency");
        assert_eq!((rows[0].fields["ms"].as_i64(), rows[1].state), (Some(12), Some(Level::Warn)));
    }

    #[test]
    fn what_is_not_the_contract() {
        assert_eq!(parse("hello\nworld\n").unwrap().body, Body::Raw { text: "hello\nworld".into() });
        assert_eq!(parse("42").unwrap().body, Body::Raw { text: "42".into() });
        let (shown, err) = read(r#"{"kind":"pie"}"#);
        assert!(err.unwrap().contains("pie"));
        assert_eq!(shown.body, Body::Raw { text: r#"{"kind":"pie"}"#.into() });
        assert!(parse(r#"{"kind":"list"}"#).is_err());
        assert!(parse("[1, 2]").is_err());
    }

    #[test]
    fn keys_are_unique() {
        let out = parse(r#"{"kind":"list","items":[{"title":"disk"},{"title":"disk"},{"title":"disk"}]}"#).unwrap();
        let Body::List { items } = &out.body else { panic!() };
        assert_eq!(items.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["disk", "disk#2", "disk#3"]);
        let out = parse(r#"{"kind":"table","columns":["a","a","b"],"rows":[["x",1]]}"#).unwrap();
        let Body::Table { columns, .. } = &out.body else { panic!() };
        assert_eq!(columns.len(), 2);
    }

    #[test]
    fn texts_are_cut() {
        let long = "é".repeat(400);
        let out = parse(&format!(r#"{{"kind":"list","items":[{{"title":"{long}"}}]}}"#)).unwrap();
        let Body::List { items } = &out.body else { panic!() };
        assert!(items[0].fields["title"].as_str().unwrap().len() <= LINE + 3);
    }
}
