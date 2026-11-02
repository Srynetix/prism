//! Atlassian Document Format (descriptions and comments of the v3 API) as Markdown, for the
//! ticket preview: paragraphs, headings, lists, code, quotes, links, mentions and tables; media
//! are left out (Prism shows no Jira attachment yet).

use serde_json::Value;

/// The Markdown of an ADF document (or of any of its nodes).
pub fn markdown(doc: &Value) -> String {
    let mut out = String::new();
    block(doc, "", &mut out);
    out.trim().to_string()
}

fn children(node: &Value) -> &[Value] {
    node.get("content").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
}

fn attr<'a>(node: &'a Value, key: &str) -> Option<&'a str> {
    node.get("attrs")?.get(key)?.as_str()
}

/// A block node, each of its lines after `indent` (a list item's or a quote's).
fn block(node: &Value, indent: &str, out: &mut String) {
    let kind = node.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "doc" | "panel" | "expand" | "nestedExpand" | "layoutSection" | "layoutColumn" => children(node).iter().for_each(|c| block(c, indent, out)),
        "paragraph" => {
            let text = inline(children(node));
            lines(&text, indent, indent, out);
            out.push_str(indent.trim_end());
            out.push('\n');
        }
        "heading" => {
            let level = node.get("attrs").and_then(|a| a.get("level")).and_then(Value::as_u64).unwrap_or(1).clamp(1, 6) as usize;
            out.push_str(&format!("{indent}{} {}\n\n", "#".repeat(level), inline(children(node))));
        }
        "bulletList" | "orderedList" => {
            let start = node.get("attrs").and_then(|a| a.get("order")).and_then(Value::as_u64).unwrap_or(1);
            for (n, item) in children(node).iter().enumerate() {
                let marker = if kind == "bulletList" { "- ".to_string() } else { format!("{}. ", start + n as u64) };
                let mut inner = String::new();
                children(item).iter().for_each(|c| block(c, "", &mut inner));
                let pad = format!("{indent}{}", " ".repeat(marker.len()));
                lines(inner.trim_end(), &format!("{indent}{marker}"), &pad, out);
            }
            out.push('\n');
        }
        "taskList" => {
            for item in children(node) {
                let done = attr(item, "state") == Some("DONE");
                out.push_str(&format!("{indent}- [{}] {}\n", if done { "x" } else { " " }, inline(children(item))));
            }
            out.push('\n');
        }
        "codeBlock" => {
            let lang = attr(node, "language").unwrap_or("");
            let code: String = children(node).iter().filter_map(|t| t.get("text").and_then(Value::as_str)).collect();
            lines(&format!("```{lang}\n{code}\n```"), indent, indent, out);
            out.push('\n');
        }
        "blockquote" => {
            let mut inner = String::new();
            children(node).iter().for_each(|c| block(c, "", &mut inner));
            let quote = format!("{indent}> ");
            lines(inner.trim_end(), &quote, &quote, out);
            out.push('\n');
        }
        "rule" => out.push_str(&format!("{indent}---\n\n")),
        "table" => {
            for (n, row) in children(node).iter().enumerate() {
                let cells: Vec<String> = children(row).iter().map(|cell| markdown(cell).replace('\n', " ")).collect();
                out.push_str(&format!("{indent}| {} |\n", cells.join(" | ")));
                if n == 0 {
                    out.push_str(&format!("{indent}|{}\n", " --- |".repeat(cells.len())));
                }
            }
            out.push('\n');
        }
        "mediaSingle" | "mediaGroup" | "media" => {}
        // An inline node met where a block is expected.
        _ => {
            let text = inline(std::slice::from_ref(node));
            if !text.is_empty() {
                out.push_str(&format!("{indent}{text}\n\n"));
            }
        }
    }
}

/// `text`'s lines, the first after `first`, the others after `rest`.
fn lines(text: &str, first: &str, rest: &str, out: &mut String) {
    for (n, line) in text.split('\n').enumerate() {
        let lead = if n == 0 { first } else { rest };
        if line.is_empty() {
            out.push_str(lead.trim_end());
        } else {
            out.push_str(lead);
            out.push_str(line);
        }
        out.push('\n');
    }
}

fn inline(nodes: &[Value]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node.get("type").and_then(Value::as_str).unwrap_or("") {
            "text" => out.push_str(&marked(node)),
            "hardBreak" => out.push('\n'),
            "mention" => out.push_str(attr(node, "text").unwrap_or("@someone")),
            "emoji" => out.push_str(attr(node, "text").or_else(|| attr(node, "shortName")).unwrap_or("")),
            "inlineCard" | "blockCard" | "embedCard" => out.push_str(attr(node, "url").unwrap_or("")),
            "date" => {
                let day = attr(node, "timestamp").and_then(|t| t.parse::<i64>().ok()).and_then(chrono::DateTime::from_timestamp_millis);
                out.push_str(&day.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default());
            }
            "status" => out.push_str(&format!("[{}]", attr(node, "text").unwrap_or(""))),
            _ => out.push_str(&inline(children(node))),
        }
    }
    out
}

/// A text node with its marks: bold, italic, strikethrough, code, link.
fn marked(node: &Value) -> String {
    let mut text = node.get("text").and_then(Value::as_str).unwrap_or("").to_string();
    let marks = node.get("marks").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let has = |k: &str| marks.iter().any(|m| m.get("type").and_then(Value::as_str) == Some(k));
    if has("code") {
        text = format!("`{text}`");
    }
    if has("strong") {
        text = format!("**{text}**");
    }
    if has("em") {
        text = format!("*{text}*");
    }
    if has("strike") {
        text = format!("~~{text}~~");
    }
    if let Some(href) = marks.iter().find(|m| m.get("type").and_then(Value::as_str) == Some("link")).and_then(|m| attr(m, "href")) {
        text = format!("[{text}]({href})");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::markdown;
    use serde_json::json;

    fn text(t: &str) -> serde_json::Value {
        json!({ "type": "text", "text": t })
    }

    #[test]
    fn paragraphs_marks_and_headings() {
        let doc = json!({ "type": "doc", "version": 1, "content": [
            { "type": "heading", "attrs": { "level": 2 }, "content": [text("Context")] },
            { "type": "paragraph", "content": [
                text("Run "),
                { "type": "text", "text": "task deploy", "marks": [{ "type": "code" }] },
                text(", see "),
                { "type": "text", "text": "the doc", "marks": [{ "type": "link", "attrs": { "href": "https://example.com" } }] },
                { "type": "hardBreak" },
                { "type": "mention", "attrs": { "id": "1", "text": "@Ada" } }
            ] }
        ] });
        assert_eq!(markdown(&doc), "## Context\n\nRun `task deploy`, see [the doc](https://example.com)\n@Ada");
    }

    #[test]
    fn nested_lists() {
        let item = |t: &str, nested: Option<serde_json::Value>| {
            let mut content = vec![json!({ "type": "paragraph", "content": [text(t)] })];
            content.extend(nested);
            json!({ "type": "listItem", "content": content })
        };
        let doc = json!({ "type": "doc", "content": [
            { "type": "orderedList", "content": [
                item("one", Some(json!({ "type": "bulletList", "content": [item("a", None)] }))),
                item("two", None)
            ] }
        ] });
        assert_eq!(markdown(&doc), "1. one\n\n   - a\n2. two");
    }

    #[test]
    fn code_quotes_and_tables() {
        let cell = |t: &str| json!({ "type": "tableCell", "content": [{ "type": "paragraph", "content": [text(t)] }] });
        let doc = json!({ "type": "doc", "content": [
            { "type": "codeBlock", "attrs": { "language": "sh" }, "content": [text("ls\npwd")] },
            { "type": "blockquote", "content": [{ "type": "paragraph", "content": [text("quoted")] }] },
            { "type": "table", "content": [
                { "type": "tableRow", "content": [cell("k"), cell("v")] },
                { "type": "tableRow", "content": [cell("a"), cell("1")] }
            ] }
        ] });
        assert_eq!(markdown(&doc), "```sh\nls\npwd\n```\n\n> quoted\n\n| k | v |\n| --- | --- |\n| a | 1 |");
    }
}
