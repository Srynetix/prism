//! Prism's own messages in the user's language (`[appearance] language`), above all the errors the
//! window shows: a source's read (`Snapshot.errors`), a command's failure. The English text is the
//! key, as in the window (`web/src/i18n.svelte.ts`): `tr("Not a git repository")`, or with values,
//! `tr!("Could not rebase {mr}: {err}", mr = iid, err = format!("{e:#}"))`. A language's table
//! (`fr/<crate>.rs`, one per crate) gives the translation, failing that the English stays.
//!
//! What another program says (glab, git, kubectl, an API) stays as it said it. A block of another
//! repository brings its own table (`Block::messages`, added with `add`); Prism's win over it.

mod fr;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::RwLock;

/// The languages Prism speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
}

static LANG: AtomicU8 = AtomicU8::new(0);

/// `[appearance] language` resolved: empty, the system's (`LANG`, which `shell_env` set from
/// macOS's settings); English when Prism does not speak it.
pub fn resolve(setting: &str) -> Lang {
    let system = std::env::var("LANG").unwrap_or_default();
    let tag = if setting.trim().is_empty() { system.as_str() } else { setting.trim() };
    if tag.to_ascii_lowercase().starts_with("fr") {
        Lang::Fr
    } else {
        Lang::En
    }
}

/// The messages' language from now on; `true` when it changed.
pub fn set(lang: Lang) -> bool {
    LANG.swap(lang as u8, Ordering::Relaxed) != lang as u8
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::Fr as u8 {
        Lang::Fr
    } else {
        Lang::En
    }
}

type Table = &'static [(&'static str, &'static str)];

/// The tables added by blocks, by language.
static ADDED: RwLock<Vec<(Lang, Table)>> = RwLock::new(Vec::new());

/// A table of `lang` besides Prism's (a block's, `Block::messages`); the same one twice counts once.
pub fn add(lang: Lang, table: Table) {
    let mut added = ADDED.write().unwrap_or_else(|e| e.into_inner());
    if !table.is_empty() && !added.iter().any(|(l, t)| *l == lang && *t == table) {
        added.push((lang, table));
    }
}

fn added(lang: Lang, text: &str) -> Option<&'static str> {
    let added = ADDED.read().unwrap_or_else(|e| e.into_inner());
    added.iter().filter(|(l, _)| *l == lang).find_map(|(_, t)| t.iter().find(|(en, _)| *en == text).map(|(_, tr)| *tr))
}

/// `text` in the messages' language.
pub fn tr(text: &'static str) -> &'static str {
    match lang() {
        Lang::En => text,
        Lang::Fr => fr::get(text).or_else(|| added(Lang::Fr, text)).unwrap_or(text),
    }
}

/// The `{name}`s of each translation of `table` that are not its English text's: a table's test.
pub fn placeholders_differ(table: Table) -> Vec<(&'static str, &'static str)> {
    let names = |s: &str| {
        let mut v: Vec<String> = s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(n, _)| n.to_string())).collect();
        v.sort();
        v
    };
    table.iter().copied().filter(|(en, tr)| names(en) != names(tr)).collect()
}

/// `one` or `other` by `n`, in the messages' language: French says the singular for 0 and 1 as well,
/// as the window's `tn` (`web/src/i18n.svelte.ts`).
pub fn tn(n: f64, one: &'static str, other: &'static str) -> &'static str {
    let single = match lang() {
        Lang::Fr => n.abs() < 2.0,
        Lang::En => n == 1.0,
    };
    tr(if single { one } else { other })
}

/// `text` with each `{name}` replaced by its value.
pub fn fill(text: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = text.to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

/// A message in the messages' language, its `{name}`s filled: `tr!("Could not read {path}", path = p.display())`.
#[macro_export]
macro_rules! tr {
    ($text:literal) => {
        $crate::i18n::tr($text).to_string()
    };
    ($text:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::fill($crate::i18n::tr($text), &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+])
    };
}

/// A count's message, `{n}` and the other `{name}`s filled: `tn!(n, "{n} file", "{n} files")`.
#[macro_export]
macro_rules! tn {
    ($n:expr, $one:literal, $other:literal $(, $name:ident = $value:expr)* $(,)?) => {{
        let n = $n;
        $crate::i18n::fill($crate::i18n::tn(n as f64, $one, $other), &[("n", &n as &dyn std::fmt::Display) $(, (stringify!($name), &$value as &dyn std::fmt::Display))*])
    }};
}

/// Every key of the French tables, for the checks below.
#[cfg(test)]
pub(crate) fn fr_keys() -> Vec<(&'static str, &'static str)> {
    fr::all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_in_english_and_french() {
        assert_eq!(tn(1.0, "{n} file", "{n} files"), "{n} file");
        assert_eq!(tn(0.0, "{n} file", "{n} files"), "{n} files");
        assert_eq!(crate::tn!(2, "{n} file", "{n} files"), "2 files");
        assert_eq!(crate::tn!(1, "{n} file of {who}", "{n} files of {who}", who = "me"), "1 file of me");
    }

    #[test]
    fn fills_and_falls_back_to_english() {
        assert_eq!(fill("{a} and {b}", &[("a", &1), ("b", &"two")]), "1 and two");
        assert_eq!(tr("a sentence no table has"), "a sentence no table has");
        assert_eq!(resolve("fr"), Lang::Fr);
        assert_eq!(resolve("en"), Lang::En);
    }

    /// A translation keeps every `{name}` of its English text.
    #[test]
    fn translations_keep_their_placeholders() {
        let all: Table = Box::leak(fr_keys().into_boxed_slice());
        let wrong = placeholders_differ(all);
        assert!(wrong.is_empty(), "{wrong:?}");
    }

    /// Every text Prism's Rust says through `tr`/`tr!` has its French: the library crates and the
    /// app (its menus aside, which have their own table, `menu::tr`). Comments and tests left out.
    #[test]
    fn every_text_said_has_its_french() {
        fn files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    files(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        /// A string literal at the start of `rest` (its opening quote), unescaped, and what follows it.
        fn literal(rest: &str) -> Option<(String, &str)> {
            let rest = rest.trim_start().strip_prefix('"')?;
            let mut text = String::new();
            let mut chars = rest.char_indices();
            while let Some((i, c)) = chars.next() {
                match c {
                    '"' => return Some((text, &rest[i + 1..])),
                    '\\' => match chars.next() {
                        Some((_, 'n')) => text.push('\n'),
                        Some((_, c)) => text.push(c),
                        None => break,
                    },
                    c => text.push(c),
                }
            }
            Some((text, ""))
        }
        /// The string literals right after `tr(` or `tr!(`, and the two of `tn(n, …)` or `tn!(n, …)`, unescaped.
        fn said(code: &str) -> Vec<String> {
            let mut out = Vec::new();
            for (at, _) in code.match_indices("tr").chain(code.match_indices("tn")) {
                let before = code[..at].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let plural = &code[at..at + 2] == "tn";
                let rest = &code[at + 2..];
                let rest = rest.strip_prefix('!').unwrap_or(rest);
                let Some(rest) = rest.strip_prefix('(') else { continue };
                if !plural {
                    if let Some((text, _)) = literal(rest) {
                        out.push(text);
                    }
                    continue;
                }
                // The count first, up to its comma (no comma nor quote in it), then the two forms.
                let Some(comma) = rest.find(',') else { continue };
                if rest[..comma].contains('"') {
                    continue;
                }
                let Some((one, rest)) = literal(&rest[comma + 1..]) else { continue };
                let Some((other, _)) = rest.trim_start().strip_prefix(',').and_then(literal) else { continue };
                out.extend([one, other]);
            }
            out
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut all = Vec::new();
        files(&root.join("crates"), &mut all);
        files(&root.join("src"), &mut all);
        let keys: std::collections::HashSet<&str> = fr_keys().into_iter().map(|(k, _)| k).collect();
        let mut missing = Vec::new();
        for path in all.iter().filter(|p| !p.components().any(|c| c.as_os_str() == "i18n" || c.as_os_str() == "target")) {
            let code = std::fs::read_to_string(path).unwrap_or_default();
            if code.contains("menu::tr") || path.ends_with("menu.rs") {
                continue;
            }
            let code = code.split("#[cfg(test)]\nmod tests").next().unwrap_or_default();
            let code: String = code.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
            missing.extend(said(&code).into_iter().filter(|t| !keys.contains(t.as_str())).map(|t| format!("{}: {t}", path.strip_prefix(&root).unwrap_or(path).display())));
        }
        assert!(missing.is_empty(), "no French for:\n{}", missing.join("\n"));
    }

    /// A block's table answers after Prism's, added once however often.
    #[test]
    fn added_tables_answer_after_prisms() {
        const TABLE: Table = &[("a sentence only a block says", "une phrase que seul un bloc dit")];
        add(Lang::Fr, TABLE);
        add(Lang::Fr, TABLE);
        assert_eq!(ADDED.read().unwrap().iter().filter(|(_, t)| *t == TABLE).count(), 1);
        assert_eq!(added(Lang::Fr, "a sentence only a block says"), Some("une phrase que seul un bloc dit"));
        assert_eq!(added(Lang::En, "a sentence only a block says"), None);
    }
}
