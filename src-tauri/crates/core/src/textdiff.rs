//! Two texts compared line by line: a custom block's or a routine's declaration since it was
//! trusted, a config change an agent proposes (`mcp`); and their hash.

use serde::Serialize;
use sha2::Digest;

/// `bytes`' SHA-256 in hex: a declaration's, to know it is the one trusted; a token's, kept in its stead.
pub fn sha256(bytes: impl AsRef<[u8]>) -> String {
    sha2::Sha256::digest(bytes.as_ref()).iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "TextDiffLine"))]
pub struct Line {
    /// `+` added, `-` removed, ` ` the same.
    pub op: String,
    pub text: String,
}

/// The lines from `old` to `new` (longest common subsequence).
pub fn lines(old: &str, new: &str) -> Vec<Line> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    // The lines both start and end with are the same: only what lies between is compared.
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (mid_a, mid_b) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let line = |op: &str, t: &str| Line { op: op.into(), text: t.to_string() };
    let mut out: Vec<Line> = a[..head].iter().map(|t| line(" ", t)).collect();
    // Beyond this, what lies between is shown removed then added rather than compared.
    if mid_a.len() * mid_b.len() > 400_000 {
        out.extend(mid_a.iter().map(|t| line("-", t)));
        out.extend(mid_b.iter().map(|t| line("+", t)));
    } else {
        let mut lcs = vec![vec![0u32; mid_b.len() + 1]; mid_a.len() + 1];
        for i in (0..mid_a.len()).rev() {
            for j in (0..mid_b.len()).rev() {
                lcs[i][j] = if mid_a[i] == mid_b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < mid_a.len() || j < mid_b.len() {
            if i < mid_a.len() && j < mid_b.len() && mid_a[i] == mid_b[j] {
                out.push(line(" ", mid_a[i]));
                (i, j) = (i + 1, j + 1);
            } else if j < mid_b.len() && (i == mid_a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
                out.push(line("+", mid_b[j]));
                j += 1;
            } else {
                out.push(line("-", mid_a[i]));
                i += 1;
            }
        }
    }
    out.extend(a[a.len() - tail..].iter().map(|t| line(" ", t)));
    out
}

/// Only the changed lines and `context` lines around each, a `…` line where some are skipped.
pub fn around(lines: Vec<Line>, context: usize) -> Vec<Line> {
    let changed: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.op != " ").map(|(i, _)| i).collect();
    let near = |i: usize| changed.iter().any(|c| c.abs_diff(i) <= context);
    let mut out = Vec::new();
    let mut skipped = false;
    for (i, l) in lines.into_iter().enumerate() {
        if near(i) {
            if skipped {
                out.push(Line { op: "…".into(), text: String::new() });
                skipped = false;
            }
            out.push(l);
        } else {
            skipped = true;
        }
    }
    if skipped && !out.is_empty() {
        out.push(Line { op: "…".into(), text: String::new() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(d: &[Line]) -> Vec<String> {
        d.iter().map(|l| format!("{}{}", l.op, l.text)).collect()
    }

    #[test]
    fn lines_added_and_removed() {
        assert_eq!(ops(&lines("a\nb\nc\n", "a\nx\nc\nd\n")), [" a", "+x", "-b", " c", "+d"]);
        assert!(lines("", "a\n").iter().all(|l| l.op == "+"));
        assert!(lines("a\nb\n", "a\nb\n").iter().all(|l| l.op == " "));
    }

    #[test]
    fn context_around_the_changes() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n";
        let new = "1\n2\n3\n4\nX\n6\n7\n8\n";
        assert_eq!(ops(&around(lines(old, new), 1)), ["…", " 4", "+X", "-5", " 6", "…"]);
    }
}
