//! A command of the agent that would remove the folder it works in (`git worktree remove` of its
//! worktree, `rm -r` of it or above): refused before it runs (`PreToolUse`), with why. Removed,
//! its CLI goes on in a folder that no longer exists; Prism removes the worktree when the agent
//! is closed.

use std::path::{Component, Path, PathBuf};

/// What the agent reads when its command is refused.
pub const REFUSED: &str = "Prism: this removes the worktree you are working in, which would leave you without a folder. Finish your work instead: Prism removes this worktree itself when the agent is closed. Another worktree can still be removed.";

/// Whether `command`, run from `cwd`, removes `cwd` or `project` (the folder its CLI started in),
/// or a folder holding either. Read as a shell would, roughly: its commands one after the other,
/// a `cd` moving the folder the next ones run from.
pub fn removes_own_folder(command: &str, cwd: &Path, project: Option<&Path>) -> bool {
    let mut here = cwd.to_path_buf();
    for words in commands(command) {
        let words: Vec<&str> = words.iter().map(String::as_str).skip_while(|w| is_assignment(w)).collect();
        let targets: Vec<PathBuf> = match words.as_slice() {
            ["cd", dir] => {
                here = resolve(&here, dir);
                continue;
            }
            ["git", rest @ ..] => {
                let mut base = here.clone();
                let mut rest = rest;
                while let ["-C", dir, more @ ..] = rest {
                    base = resolve(&base, dir);
                    rest = more;
                }
                match rest {
                    ["worktree", "remove", args @ ..] => operands(args).map(|t| resolve(&base, t)).collect(),
                    _ => continue,
                }
            }
            ["rm", args @ ..] if args.iter().any(|a| *a == "--recursive" || (a.starts_with('-') && !a.starts_with("--") && a.contains(['r', 'R']))) => operands(args).map(|t| resolve(&here, t)).collect(),
            _ => continue,
        };
        if targets.iter().any(|t| [Some(cwd), project].into_iter().flatten().any(|inside| within(inside, t))) {
            return true;
        }
    }
    false
}

/// `VAR=value` before a command.
fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// A command's arguments that are not options (all of them after `--`).
fn operands<'a>(args: &'a [&'a str]) -> impl Iterator<Item = &'a str> + 'a {
    let mut ended = false;
    args.iter().copied().filter(move |a| {
        if ended {
            return true;
        }
        if *a == "--" {
            ended = true;
            return false;
        }
        !a.starts_with('-')
    })
}

fn resolve(base: &Path, dir: &str) -> PathBuf {
    let dir = match dir.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => dirs::home_dir().map(|h| h.join(rest.trim_start_matches('/'))).unwrap_or_else(|| PathBuf::from(dir)),
        _ => PathBuf::from(dir),
    };
    normal(&base.join(dir))
}

/// `path` without its `.` and `..`, links followed when it exists (`/tmp` is `/private/tmp`).
fn normal(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

/// `inside` is `folder` or under it.
fn within(inside: &Path, folder: &Path) -> bool {
    normal(inside).starts_with(folder)
}

/// The words of each command of `line`: split at `&&`, `||`, `;`, `|`, `&` and new lines, outside
/// quotes; quotes and backslashes removed as a shell does.
fn commands(line: &str) -> Vec<Vec<String>> {
    let mut all = Vec::new();
    let (mut words, mut word, mut started) = (Vec::new(), String::new(), false);
    let mut chars = line.chars().peekable();
    let end_word = |words: &mut Vec<String>, word: &mut String, started: &mut bool| {
        if *started {
            words.push(std::mem::take(word));
            *started = false;
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                started = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    word.push(c);
                }
            }
            '"' => {
                started = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => word.extend(chars.next()),
                        c => word.push(c),
                    }
                }
            }
            '\\' => {
                started = true;
                word.extend(chars.next());
            }
            ';' | '&' | '|' | '\n' => {
                end_word(&mut words, &mut word, &mut started);
                if !words.is_empty() {
                    all.push(std::mem::take(&mut words));
                }
            }
            c if c.is_whitespace() => end_word(&mut words, &mut word, &mut started),
            c => {
                started = true;
                word.push(c);
            }
        }
    }
    end_word(&mut words, &mut word, &mut started);
    if !words.is_empty() {
        all.push(words);
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("prism-guard-{}-{:?}", std::process::id(), std::thread::current().id()));
        let (repo, wt) = (root.join("repo"), root.join("repo.worktrees/feat"));
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::create_dir_all(root.join("repo.worktrees/other")).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        (root.canonicalize().unwrap(), repo.canonicalize().unwrap(), wt.canonicalize().unwrap())
    }

    /// Its own worktree, however named, refused; another one, or its files, not.
    #[test]
    fn refuses_removing_its_own_worktree_only() {
        let (root, repo, wt) = tree();
        let own = |cmd: &str| removes_own_folder(cmd, &wt, Some(&wt));
        assert!(own(&format!("git -C {} worktree remove {}", repo.display(), wt.display())));
        assert!(own("cd .. && git worktree remove --force feat"));
        assert!(own("git worktree remove ."));
        assert!(own("cd ../../repo; git worktree remove -f ../repo.worktrees/feat"));
        assert!(own(&format!("rm -rf '{}'", wt.display())));
        assert!(own("rm -r -f .."), "a folder holding it");
        assert!(!own("git worktree remove ../other"), "another worktree");
        assert!(!own("rm -rf src target"), "its own files");
        assert!(!own("rm ."), "not recursive: rm refuses it anyway");
        assert!(!own("echo 'git worktree remove .'"));
        assert!(!own("git worktree list"));
        // Its shell moved elsewhere (its repository): its worktree, where its CLI started, still its own.
        assert!(removes_own_folder(&format!("git worktree remove {}", wt.display()), &repo, Some(&wt)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn splits_commands_as_a_shell() {
        assert_eq!(commands("cd 'a b' && X=1 git \"c\\\"d\"|wc -l\nls"), [vec!["cd", "a b"], vec!["X=1", "git", "c\"d"], vec!["wc", "-l"], vec!["ls"]]);
    }
}
