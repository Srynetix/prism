//! ClickUp custom fields shown on tickets (`[clickup] fields`, by name): their value put in
//! words, with the color of a dropdown or label option. Only the listed fields are kept, and the
//! estimate fields (`[clickup] estimate_fields`), entered from Prism: those even without a value.

use std::sync::{LazyLock, RwLock};

use chrono::{DateTime, Local};
use serde::Deserialize;

use crate::state::{FieldValue, TicketField};

/// The fields shown, by name, as in the config, and whether each is also a filter and a grouping
/// of the ticket lists (`[clickup] filter_fields`); replaced on every Settings save.
static SHOWN: LazyLock<RwLock<Vec<(String, bool)>>> = LazyLock::new(Default::default);
/// The estimate fields, by name: shown after the others when not among them.
static ESTIMATES: LazyLock<RwLock<Vec<String>>> = LazyLock::new(Default::default);

/// `filters`: the fields also offered as filters; `None` or empty: all of them. `estimates`: the
/// number fields entered from Prism.
pub fn configure(names: &[String], filters: Option<&[String]>, estimates: &[String]) {
    let filters = filters.filter(|f| f.iter().any(|x| !x.trim().is_empty()));
    let filter = |n: &str| filters.is_none_or(|f| f.iter().any(|x| x.trim().eq_ignore_ascii_case(n)));
    *SHOWN.write().unwrap() = names.iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).map(|n| (n.clone(), filter(&n))).collect();
    *ESTIMATES.write().unwrap() = estimates.iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
}

/// Whether `name` is an estimate field (`[clickup] estimate_fields`).
pub fn is_estimate(name: &str) -> bool {
    ESTIMATES.read().unwrap().iter().any(|e| e.eq_ignore_ascii_case(name.trim()))
}

/// The types an estimate in hours can be written to.
pub fn holds_hours(kind: &str) -> bool {
    matches!(kind, "number" | "currency")
}

impl RawField {
    pub fn named(&self, name: &str) -> bool {
        self.name.trim().eq_ignore_ascii_case(name.trim())
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
}

/// A custom field of a task, as the API gives it (`custom_fields`).
#[derive(Debug, Deserialize)]
pub struct RawField {
    #[serde(default)]
    pub id: String,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    type_config: TypeConfig,
    #[serde(default)]
    value: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
struct TypeConfig {
    #[serde(default)]
    options: Vec<RawOption>,
}

#[derive(Debug, Deserialize)]
struct RawOption {
    #[serde(default)]
    id: serde_json::Value,
    /// `label` for a labels field.
    #[serde(alias = "label")]
    name: Option<String>,
    color: Option<String>,
    orderindex: Option<serde_json::Value>,
}

/// The listed fields that have a value, in the config's order, then the estimate fields not
/// listed; an estimate field (a number) is kept without a value, to be entered.
pub fn shown(raw: &[RawField]) -> Vec<TicketField> {
    let names = SHOWN.read().unwrap();
    let estimates = ESTIMATES.read().unwrap();
    let extra = estimates.iter().filter(|e| !names.iter().any(|(n, _)| n.eq_ignore_ascii_case(e))).map(|e| (e.clone(), false));
    names
        .iter()
        .cloned()
        .chain(extra)
        .filter_map(|(name, filter)| raw.iter().find(|f| f.named(&name)).map(|f| (f, filter)))
        .filter_map(|(f, filter)| {
            let values = values(f);
            let estimate = holds_hours(&f.kind) && estimates.iter().any(|e| f.named(e));
            (estimate || !values.is_empty()).then(|| TicketField { name: f.name.trim().to_string(), values, filter: filter && !estimate, estimate })
        })
        .collect()
}

fn values(f: &RawField) -> Vec<FieldValue> {
    let v = &f.value;
    let plain = |label: String| vec![FieldValue { label, color: None, order: None }];
    let text = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    };
    // Option ids are strings, orderindexes numbers or numeric strings: compared as text.
    let option = |key: &serde_json::Value| {
        let key = text(key)?;
        f.type_config.options.iter().find(|o| text(&o.id).as_deref() == Some(&key) || o.orderindex.as_ref().and_then(text).as_deref() == Some(&key))
    };
    let of = |o: &RawOption| {
        let order = o.orderindex.as_ref().and_then(text).and_then(|i| i.parse().ok());
        o.name.clone().filter(|n| !n.trim().is_empty()).map(|label| FieldValue { label, color: o.color.clone(), order })
    };
    match f.kind.as_str() {
        "drop_down" => option(v).and_then(of).into_iter().collect(),
        "labels" => v.as_array().map(|ids| ids.iter().filter_map(option).filter_map(of).collect()).unwrap_or_default(),
        "checkbox" => match v {
            serde_json::Value::Bool(true) => plain("yes".into()),
            serde_json::Value::String(s) if s == "true" => plain("yes".into()),
            _ => Vec::new(),
        },
        "date" => text(v)
            .and_then(|ms| ms.parse::<i64>().ok())
            .and_then(DateTime::from_timestamp_millis)
            .map(|d| plain(d.with_timezone(&Local).format("%Y-%m-%d").to_string()))
            .unwrap_or_default(),
        "emoji" => text(v).filter(|n| n != "0").map(|n| plain(format!("★ {n}"))).unwrap_or_default(),
        "users" | "tasks" => v
            .as_array()
            .map(|items| items.iter().filter_map(|i| i.get("username").or_else(|| i.get("name")).and_then(text)).map(|l| FieldValue { label: l, color: None, order: None }).collect())
            .unwrap_or_default(),
        "location" => v.get("formatted_address").and_then(text).map(plain).unwrap_or_default(),
        "progress" => v.get("percent_complete").and_then(text).map(|p| plain(format!("{p} %"))).unwrap_or_default(),
        // Text, number, money, email, phone, link…: their value as is.
        _ => text(v).map(plain).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fields shown are global: the tests that configure them take turns.
    static CONFIGURED: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn field(json: serde_json::Value) -> RawField {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn puts_values_in_words() {
        let options = serde_json::json!({ "options": [
            { "id": "a1", "name": "Bug", "color": "#e50000", "orderindex": 0 },
            { "id": "b2", "name": "Feature", "color": "#04a9f4", "orderindex": 1 },
        ]});
        let drop = |value| field(serde_json::json!({ "name": "Type", "type": "drop_down", "type_config": options, "value": value }));
        let feature = vec![FieldValue { label: "Feature".into(), color: Some("#04a9f4".into()), order: Some(1) }];
        assert_eq!(values(&drop(serde_json::json!(1))), feature, "by orderindex");
        assert_eq!(values(&drop(serde_json::json!("b2"))), feature, "by option id");
        assert!(values(&drop(serde_json::Value::Null)).is_empty());
        let labels = field(serde_json::json!({ "name": "Zones", "type": "labels", "type_config": { "options": [
            { "id": "x", "label": "API", "color": null }, { "id": "y", "label": "Web", "color": "#0f0" } ]}, "value": ["y", "x"] }));
        assert_eq!(values(&labels).iter().map(|v| v.label.as_str()).collect::<Vec<_>>(), ["Web", "API"]);
        assert_eq!(values(&field(serde_json::json!({ "name": "Points", "type": "number", "value": "8" })))[0].label, "8");
        assert!(values(&field(serde_json::json!({ "name": "Done", "type": "checkbox", "value": false }))).is_empty());
        assert_eq!(values(&field(serde_json::json!({ "name": "Owner", "type": "users", "value": [{ "username": "Ada" }] })))[0].label, "Ada");
    }

    #[test]
    fn keeps_the_listed_fields_in_their_order() {
        let raw = vec![
            field(serde_json::json!({ "name": "Points", "type": "number", "value": 3 })),
            field(serde_json::json!({ "name": "Secret", "type": "short_text", "value": "x" })),
            field(serde_json::json!({ "name": "Type", "type": "short_text", "value": "Bug" })),
            field(serde_json::json!({ "name": "Empty", "type": "short_text" })),
        ];
        let _turn = CONFIGURED.lock().unwrap_or_else(|e| e.into_inner());
        configure(&["type".into(), "Points".into(), "Empty".into(), " ".into()], None, &[]);
        let got: Vec<_> = shown(&raw).into_iter().map(|f| (f.name, f.values[0].label.clone(), f.filter)).collect();
        assert_eq!(got, [("Type".to_string(), "Bug".to_string(), true), ("Points".into(), "3".into(), true)]);
        // Only Type filters and groups; Points is only shown.
        configure(&["type".into(), "Points".into()], Some(&["Type".into()]), &[]);
        assert_eq!(shown(&raw).iter().map(|f| f.filter).collect::<Vec<_>>(), [true, false]);
        configure(&[], None, &[]);
        assert!(shown(&raw).is_empty());
    }

    #[test]
    fn keeps_the_estimate_fields_even_empty() {
        let raw = vec![
            field(serde_json::json!({ "name": "Type", "type": "short_text", "value": "Bug" })),
            field(serde_json::json!({ "name": "Dev estimate (h)", "type": "number" })),
            field(serde_json::json!({ "name": "QA estimate (h)", "type": "number", "value": "7" })),
            field(serde_json::json!({ "name": "Total (h)", "type": "formula", "value": 7 })),
        ];
        let _turn = CONFIGURED.lock().unwrap_or_else(|e| e.into_inner());
        configure(&["Type".into(), "QA estimate (h)".into()], None, &["dev estimate (h)".into(), "QA estimate (h)".into(), "Total (h)".into()]);
        let got: Vec<_> = shown(&raw).into_iter().map(|f| (f.name, f.values.len(), f.estimate, f.filter)).collect();
        assert_eq!(got, [
            ("Type".to_string(), 1, false, true),
            // Listed: in its place, an estimate, never a filter.
            ("QA estimate (h)".into(), 1, true, false),
            // Not listed: after, kept empty.
            ("Dev estimate (h)".into(), 0, true, false),
            // A formula is not written to: only shown, with its value.
            ("Total (h)".into(), 1, false, false),
        ]);
        configure(&[], None, &[]);
    }
}
