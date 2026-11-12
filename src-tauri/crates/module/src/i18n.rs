//! The module's messages in the window's language (`configure` gives it): `tr!` as Prism's, the
//! translations those of its blocks' `messages`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The languages Prism speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
}

impl Lang {
    pub(crate) fn of(code: &str) -> Self {
        if code.starts_with("fr") { Self::Fr } else { Self::En }
    }
}

fn table() -> &'static Mutex<HashMap<&'static str, &'static str>> {
    static TABLE: OnceLock<Mutex<HashMap<&'static str, &'static str>>> = OnceLock::new();
    TABLE.get_or_init(Default::default)
}

/// The translations of `lang`, its blocks' own.
pub(crate) fn set(pairs: impl IntoIterator<Item = (&'static str, &'static str)>) {
    let mut t = table().lock().unwrap();
    t.clear();
    t.extend(pairs);
}

/// `text` in the messages' language, otherwise as written.
#[doc(hidden)]
pub fn tr(text: &'static str) -> &'static str {
    table().lock().unwrap().get(text).copied().unwrap_or(text)
}

/// `text`'s `{name}`s filled.
#[doc(hidden)]
pub fn fill(text: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = text.to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

/// A message in the messages' language, its `{name}`s filled: `tr!("Could not read {path}",
/// path = p.display())`.
#[macro_export]
macro_rules! tr {
    ($text:literal) => {
        $crate::__i18n::tr($text).to_string()
    };
    ($text:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::__i18n::fill($crate::__i18n::tr($text), &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+])
    };
}
