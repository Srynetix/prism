//! The docs `prism mcp` carries (`build.rs`): the help pages, the reference, the guide, the SDK
//! and the example block, as resources, and searched by passage (`search`).

include!(concat!(env!("OUT_DIR"), "/mcp_docs.rs"));

/// A help page in `lang` (`en`, `fr`), English when it has none in it.
pub fn help(lang: &str, name: &str) -> Option<&'static str> {
    let find = |l: &str| HELP.iter().find(|(pl, n, _)| *pl == l && *n == name.trim_end_matches(".md")).map(|(_, _, t)| *t);
    find(lang).or_else(|| find("en"))
}

/// The help pages' names, in order.
pub fn help_pages() -> Vec<&'static str> {
    HELP.iter().filter(|(l, _, _)| *l == "en").map(|(_, n, _)| *n).collect()
}

/// The guide's section about modules: from its heading to the next one
/// of the same level.
pub fn guide_modules() -> String {
    section(GUIDE, "## Modules").unwrap_or_default()
}

/// The text of `heading` (a whole line) up to the next heading of its level or above.
pub fn section(text: &str, heading: &str) -> Option<String> {
    let level = heading.chars().take_while(|c| *c == '#').count();
    let start = text.lines().position(|l| l.trim_end() == heading)?;
    let lines: Vec<&str> = text.lines().collect();
    let mut fenced = false;
    let mut end = lines.len();
    for (i, l) in lines.iter().enumerate().skip(start + 1) {
        if l.starts_with("```") {
            fenced = !fenced;
        }
        let hashes = l.chars().take_while(|c| *c == '#').count();
        if !fenced && hashes > 0 && hashes <= level && l.chars().nth(hashes) == Some(' ') {
            end = i;
            break;
        }
    }
    Some(lines[start..end].join("\n"))
}

/// A passage found: where it is, and its text.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Passage {
    pub source: String,
    pub text: String,
    #[serde(skip)]
    pub score: f32,
}

/// The passages of the help (in `lang`, English too), the reference and `schema` (a key per
/// passage, its description) best matching `query`, at most `max`.
pub fn search(query: &str, lang: &str, schema: &serde_json::Value, max: usize) -> Vec<Passage> {
    let terms = words(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut all = Vec::new();
    for (l, name, text) in HELP.iter().filter(|(l, _, _)| *l == lang || *l == "en") {
        let prefix = if *l == "en" { format!("help/{name}.md") } else { format!("help/{l}/{name}.md") };
        all.extend(markdown_passages(&prefix, text));
    }
    all.extend(example_passages(EXAMPLE));
    all.extend(markdown_passages("docs/guide.md", &guide_modules()));
    all.extend(schema_passages(schema));
    for p in &mut all {
        p.score = score(&terms, &p.source, &p.text);
    }
    all.retain(|p| p.score > 0.0);
    all.sort_by(|a, b| b.score.total_cmp(&a.score));
    all.truncate(max);
    for p in &mut all {
        p.text = cut(&p.text, 1200);
    }
    all
}

/// Lowercase words of three letters or more, accents dropped.
fn words(text: &str) -> Vec<String> {
    let plain: String = text.to_lowercase().chars().map(fold).collect();
    plain.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| w.chars().count() >= 3).map(String::from).collect()
}

fn fold(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'î' | 'ï' => 'i',
        'ô' | 'ö' => 'o',
        'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        c => c,
    }
}

/// How well `text` matches: each term found counts (its source's, twice), rarer the more it counts.
fn score(terms: &[String], source: &str, text: &str) -> f32 {
    let body = words(text);
    let head = words(source);
    let mut s = 0.0;
    let mut found = 0;
    for t in terms {
        let n = body.iter().filter(|w| w.starts_with(t.as_str())).count() + 2 * head.iter().filter(|w| w.starts_with(t.as_str())).count();
        if n > 0 {
            found += 1;
            s += 1.0 + (n as f32).ln();
        }
    }
    // Every term found weighs more than many of one.
    s * found as f32 / terms.len() as f32 / (1.0 + (body.len() as f32 / 400.0))
}

fn markdown_passages(prefix: &str, text: &str) -> Vec<Passage> {
    // A help page's front matter (`---` … `---`) is no passage.
    let text = text.strip_prefix("---\n").and_then(|rest| rest.split_once("\n---\n")).map_or(text, |(_, body)| body);
    let mut out = Vec::new();
    let mut heading = String::new();
    let mut buf = String::new();
    let mut fenced = false;
    let flush = |heading: &str, buf: &mut String, out: &mut Vec<Passage>| {
        if !buf.trim().is_empty() {
            let source = if heading.is_empty() { prefix.to_string() } else { format!("{prefix} › {heading}") };
            out.push(Passage { source, text: buf.trim().to_string(), score: 0.0 });
        }
        buf.clear();
    };
    for line in text.lines() {
        if line.starts_with("```") {
            fenced = !fenced;
        }
        if !fenced && line.starts_with('#') {
            flush(&heading, &mut buf, &mut out);
            heading = line.trim_start_matches('#').trim().to_string();
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
    }
    flush(&heading, &mut buf, &mut out);
    out
}

/// The reference cut at its blank lines: a table and the comments above it, one passage.
fn example_passages(text: &str) -> Vec<Passage> {
    let mut out = Vec::new();
    let mut start = 1;
    let mut buf = String::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            if !buf.trim().is_empty() {
                out.push(Passage { source: format!("config.example.toml:{start}"), text: buf.trim_end().to_string(), score: 0.0 });
            }
            buf.clear();
            start = i + 2;
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
    }
    if !buf.trim().is_empty() {
        out.push(Passage { source: format!("config.example.toml:{start}"), text: buf, score: 0.0 });
    }
    out
}

/// One passage per key of the schema with a description: `forge.bare_mr_repo — …`.
fn schema_passages(schema: &serde_json::Value) -> Vec<Passage> {
    let mut out = Vec::new();
    let Some(props) = schema.get("properties").and_then(|p| p.as_object()) else { return out };
    for (table, entry) in props {
        let entry = deref(schema, entry);
        if let Some(d) = entry.get("description").and_then(|d| d.as_str()) {
            out.push(Passage { source: format!("schema › {table}"), text: d.to_string(), score: 0.0 });
        }
        let items = entry.get("items").map(|i| deref(schema, i)).unwrap_or(entry);
        let suffix = if entry.get("items").is_some() { "[]" } else { "" };
        for (key, sub) in items.get("properties").and_then(|p| p.as_object()).into_iter().flatten() {
            let sub = deref(schema, sub);
            if let Some(d) = sub.get("description").and_then(|d| d.as_str()) {
                out.push(Passage { source: format!("schema › {table}{suffix}.{key}"), text: d.to_string(), score: 0.0 });
            }
        }
    }
    out
}

fn deref<'a>(root: &'a serde_json::Value, v: &'a serde_json::Value) -> &'a serde_json::Value {
    let mut at = v;
    for _ in 0..8 {
        match at.get("$ref").and_then(|r| r.as_str()).and_then(|r| r.strip_prefix("#/$defs/")).and_then(|n| root.get("$defs")?.get(n)) {
            Some(d) => at = d,
            None => break,
        }
    }
    at
}

fn cut(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_docs_are_carried() {
        assert!(help_pages().contains(&"05-mrs"));
        assert!(help("fr", "05-mrs").is_some());
        assert!(EXAMPLE.contains("[agents]"));
        assert!(guide_modules().starts_with("## Modules") && guide_modules().contains("### The module SDK") && !guide_modules().contains("## Command line"));
        assert!(EXAMPLE_BLOCK.iter().any(|(p, t)| p.ends_with("hello/backend/src/hello.rs") && t.contains("impl Block")));
    }

    #[test]
    fn a_search_finds_the_reference() {
        let found = search("custom_blocks script shebang", "en", &serde_json::json!({}), 5);
        assert!(found.iter().any(|p| p.source.starts_with("config.example.toml:") && p.text.contains("Custom blocks")), "{found:?}");
        let fr = search("dépôts autre forge github", "fr", &serde_json::json!({}), 5);
        assert!(!fr.is_empty());
    }
}
