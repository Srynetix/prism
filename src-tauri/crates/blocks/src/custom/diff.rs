//! What changed in a custom block's declaration since it was trusted, line by line, shown before
//! trusting it again.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CustomDiffLine"))]
pub struct Line {
    /// `+` added, `-` removed, ` ` the same.
    pub op: String,
    pub text: String,
}

/// A declaration as TOML, as one would read it in config.toml.
pub fn text(def: &super::Def) -> String {
    toml::to_string(def).unwrap_or_default()
}

/// The lines from `old` to `new` (longest common subsequence; declarations are short).
pub fn lines(old: &str, new: &str) -> Vec<Line> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    // A declaration beyond this is shown whole as new rather than compared.
    if a.len() * b.len() > 400_000 {
        return b.iter().map(|t| Line { op: "+".into(), text: t.to_string() }).collect();
    }
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    let line = |op: &str, t: &str| Line { op: op.into(), text: t.to_string() };
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.push(line(" ", a[i]));
            (i, j) = (i + 1, j + 1);
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out.push(line("+", b[j]));
            j += 1;
        } else {
            out.push(line("-", a[i]));
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_added_and_removed() {
        let d = lines("a\nb\nc\n", "a\nx\nc\nd\n");
        let ops: Vec<String> = d.iter().map(|l| format!("{}{}", l.op, l.text)).collect();
        assert_eq!(ops, [" a", "+x", "-b", " c", "+d"]);
        assert!(lines("", "a\n").iter().all(|l| l.op == "+"));
    }
}
