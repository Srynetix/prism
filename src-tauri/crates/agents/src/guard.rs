//! A command of the agent that would remove the folder it works in (`git worktree remove` of its
//! worktree, `rm -r` or `find -delete` of it or above): refused before it runs (`PreToolUse`),
//! with why. Removed, its CLI goes on in a folder that no longer exists; Prism removes the
//! worktree when the agent is closed. A guard, not a shell: what it does not read (a script, an
//! alias, a variable of its own) still removes it, and the Agents view then says so.

use std::path::{Component, Path, PathBuf};

/// What the agent reads when its command is refused.
pub const REFUSED: &str = "Prism: this removes the worktree you are working in, which would leave you without a folder. Finish your work instead: Prism removes this worktree itself when the agent is closed. Another worktree can still be removed.";

/// Whether `command`, run from `cwd`, removes `cwd` or `project` (the folder its CLI started in),
/// or a folder holding either. Read as a shell would, roughly: its commands one after the other,
/// a `cd` moving the folder the next ones run from (until the end of its `( … )`).
pub fn removes_own_folder(command: &str, cwd: &Path, project: Option<&Path>) -> bool {
    let mut here = cwd.to_path_buf();
    let mut stack = Vec::new();
    for piece in pieces(command) {
        let words = match piece {
            Piece::Open => {
                stack.push(here.clone());
                continue;
            }
            Piece::Close => {
                here = stack.pop().unwrap_or(here);
                continue;
            }
            Piece::Words(words) => words,
        };
        let words: Vec<String> = words.iter().map(|w| expand(w, &here)).collect();
        let words = program(&words);
        let name = words.first().map(|w| w.rsplit('/').next().unwrap_or(w));
        let args = words.get(1..).unwrap_or_default();
        let targets: Vec<PathBuf> = match name {
            Some("cd" | "pushd") => {
                if let Some(dir) = args.iter().find(|a| !a.starts_with('-')) {
                    here = resolve(&here, dir);
                }
                continue;
            }
            Some("git") => match worktree_remove(args) {
                Some((base, paths)) => {
                    let base = base.map_or_else(|| here.clone(), |b| resolve(&here, &b));
                    paths.iter().map(|t| resolve(&base, t)).collect()
                }
                None => continue,
            },
            Some("rm") if args.iter().any(|a| a == "--recursive" || (a.starts_with('-') && !a.starts_with("--") && a.contains(['r', 'R']))) => operands(args).map(|t| resolve(&here, t)).collect(),
            Some("find") => match find_removes(args) {
                Some(paths) => paths.iter().map(|t| resolve(&here, t)).collect(),
                None => continue,
            },
            _ => continue,
        };
        if targets.iter().any(|t| [Some(cwd), project].into_iter().flatten().any(|inside| within(inside, t))) {
            return true;
        }
    }
    false
}

/// The command a line runs, past its assignments (`VAR=1`) and what runs it (`sudo`, `command`,
/// `env`, `exec`, `nice`, `time`… and their options).
fn program(words: &[String]) -> &[String] {
    let mut rest = words;
    loop {
        match rest.first().map(String::as_str) {
            Some(w) if is_assignment(w) => rest = &rest[1..],
            Some("sudo" | "command" | "builtin" | "exec" | "env" | "nice" | "nohup" | "time" | "doas") => {
                rest = &rest[1..];
                while rest.first().is_some_and(|w| w.starts_with('-')) {
                    // `sudo -u bob`, `nice -n 5`: options that take a value.
                    let takes = matches!(rest[0].as_str(), "-u" | "-g" | "-n" | "-C" | "-h" | "-p");
                    rest = &rest[(if takes { 2 } else { 1 }).min(rest.len())..];
                }
            }
            _ => return rest,
        }
    }
}

/// `find <paths> -delete`, or `-exec rm -r`, with nothing choosing what (`-name`, `-type`…):
/// all of its paths go, the folders included. Its paths.
fn find_removes(args: &[String]) -> Option<Vec<String>> {
    let at = args.iter().position(|a| a.starts_with(['-', '(', '!'])).unwrap_or(args.len());
    let (paths, expr) = args.split_at(at);
    let rm_r = |w: &[String]| w[0] == "rm" && w[1].starts_with('-') && w[1].contains(['r', 'R']);
    let deletes = expr.iter().any(|a| a == "-delete") || expr.windows(2).any(rm_r);
    let chosen = expr.iter().take_while(|a| !matches!(a.as_str(), "-exec" | "-execdir" | "-ok")).any(|a| a.starts_with(['-', '(', '!']) && !matches!(a.as_str(), "-delete" | "-depth" | "-mindepth" | "-maxdepth" | "-xdev" | "-mount"));
    (deletes && !chosen).then(|| paths.to_vec())
}

/// `git [global options] worktree remove [options] <paths>`: the folder `-C` moves it to, if
/// any, and its paths.
fn worktree_remove(args: &[String]) -> Option<(Option<String>, Vec<String>)> {
    let mut base: Option<String> = None;
    let mut i = 0;
    while let Some(a) = args.get(i) {
        match a.as_str() {
            "-C" => {
                let dir = args.get(i + 1)?;
                base = Some(match base {
                    Some(b) => Path::new(&b).join(dir).display().to_string(),
                    None => dir.clone(),
                });
                i += 2;
            }
            // Global options that take a value apart.
            "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--exec-path" | "--config-env" => i += 2,
            a if a.starts_with('-') => i += 1,
            _ => break,
        }
    }
    let rest: Vec<&str> = args.get(i..)?.iter().map(String::as_str).collect();
    match rest.as_slice() {
        ["worktree", "remove", more @ ..] => Some((base, operands(more).map(str::to_string).collect())),
        _ => None,
    }
}

/// `VAR=value` before a command.
fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// A command's arguments that are not options (all of them after `--`).
fn operands<'a, S: AsRef<str>>(args: &'a [S]) -> impl Iterator<Item = &'a str> + 'a {
    let mut ended = false;
    args.iter().map(AsRef::as_ref).filter(move |a| {
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

/// The folder a word names through the shell: `$PWD`, `${PWD}`, `$(pwd)` and `` `pwd` `` are
/// the folder the command runs in, `$HOME` the home folder.
fn expand(word: &str, here: &Path) -> String {
    let here = here.display().to_string();
    let home = dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
    word.replace("$(pwd)", &here).replace("`pwd`", &here).replace("${PWD}", &here).replace("$PWD", &here).replace("${HOME}", &home).replace("$HOME", &home)
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

/// A piece of a command line: a command's words, or a `(` / `)` around commands (a subshell, its
/// `cd` its own).
#[derive(Debug, PartialEq)]
enum Piece {
    Words(Vec<String>),
    Open,
    Close,
}

/// The pieces of `line`, split at `&&`, `||`, `;`, `|`, `&`, new lines and parentheses outside
/// quotes; quotes and backslashes removed as a shell does. A redirection and its target are left
/// out (`> out.txt`, `2>&1`, `&> log`), and a heredoc's lines (`<<EOF` … `EOF`) are text, not
/// commands.
fn pieces(line: &str) -> Vec<Piece> {
    let mut lexer = Lexer::default();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                lexer.started = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    lexer.word.push(c);
                }
            }
            '"' => {
                lexer.started = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => lexer.word.extend(chars.next()),
                        c => lexer.word.push(c),
                    }
                }
            }
            '\\' => {
                lexer.started = true;
                lexer.word.extend(chars.next());
            }
            // `&>` and `&>>`: a redirection, not a separator.
            '&' if chars.peek() == Some(&'>') => {
                lexer.end_word();
                chars.next();
                if chars.peek() == Some(&'>') {
                    chars.next();
                }
                lexer.redirect = true;
            }
            '>' | '<' => {
                // `2>`: the file descriptor before it is no word.
                if lexer.word.chars().all(|c| c.is_ascii_digit()) && !lexer.word.is_empty() {
                    lexer.word.clear();
                    lexer.started = false;
                }
                lexer.end_word();
                let mut op = c.to_string();
                while let Some(&n) = chars.peek().filter(|n| matches!(n, '<' | '>' | '&' | '-' | '|')) {
                    op.push(n);
                    chars.next();
                }
                // `2>&1`, `>&-`: to another descriptor, no file.
                if op.ends_with('&') {
                    while chars.peek().is_some_and(|n| n.is_ascii_digit() || *n == '-') {
                        chars.next();
                    }
                    continue;
                }
                if op.starts_with("<<") && !op.starts_with("<<<") {
                    lexer.heredoc = true;
                } else {
                    lexer.redirect = true;
                }
            }
            '\n' => {
                lexer.end_command();
                // A heredoc's lines, up to its delimiter: skipped.
                for delimiter in std::mem::take(&mut lexer.delimiters) {
                    let mut text = String::new();
                    loop {
                        text.clear();
                        let mut more = false;
                        for c in chars.by_ref() {
                            more = true;
                            if c == '\n' {
                                break;
                            }
                            text.push(c);
                        }
                        if !more || text.trim() == delimiter {
                            break;
                        }
                    }
                }
            }
            ';' | '&' | '|' => lexer.end_command(),
            // `$(…)`: a substitution, part of the word (`$(pwd)`).
            '(' if lexer.word.ends_with('$') => {
                lexer.word.push(c);
                let mut depth = 1;
                for c in chars.by_ref() {
                    lexer.word.push(c);
                    depth += match c {
                        '(' => 1,
                        ')' => -1,
                        _ => 0,
                    };
                    if depth == 0 {
                        break;
                    }
                }
            }
            '(' | ')' => {
                lexer.end_command();
                lexer.pieces.push(if c == '(' { Piece::Open } else { Piece::Close });
            }
            c if c.is_whitespace() => lexer.end_word(),
            c => {
                lexer.started = true;
                lexer.word.push(c);
            }
        }
    }
    lexer.end_command();
    lexer.pieces
}

#[derive(Default)]
struct Lexer {
    pieces: Vec<Piece>,
    words: Vec<String>,
    word: String,
    started: bool,
    /// The next word is a redirection's target.
    redirect: bool,
    /// The next word is a heredoc's delimiter.
    heredoc: bool,
    /// The delimiters of the heredocs of the line being read.
    delimiters: Vec<String>,
}

impl Lexer {
    fn end_word(&mut self) {
        if !self.started {
            return;
        }
        self.started = false;
        let word = std::mem::take(&mut self.word);
        if std::mem::take(&mut self.redirect) {
            return;
        }
        if std::mem::take(&mut self.heredoc) {
            self.delimiters.push(word.trim_start_matches('-').to_string());
            return;
        }
        self.words.push(word);
    }

    fn end_command(&mut self) {
        self.end_word();
        if !self.words.is_empty() {
            self.pieces.push(Piece::Words(std::mem::take(&mut self.words)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("prism-guard-{}-{}", std::process::id(), uuid::Uuid::new_v4()));
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

    /// The ways of writing it a shell reads the same.
    #[test]
    fn reads_it_however_written() {
        let (root, _, wt) = tree();
        let own = |cmd: &str| removes_own_folder(cmd, &wt, Some(&wt));
        assert!(own("git -c core.x=y --no-pager worktree remove ."));
        assert!(own("git --git-dir=../../repo/.git worktree remove ."));
        assert!(own("sudo -u me rm -rf ."));
        assert!(own("command /bin/rm -rf \"$PWD\""));
        assert!(own("rm -rf $(pwd)"));
        assert!(own("(cd .. && rm -rf feat)"));
        assert!(own("cd .. >/dev/null 2>&1 && rm -rf feat"));
        assert!(own("find . -delete"));
        assert!(own("find .. -exec rm -rf {} +"));
        assert!(!own("find . -name '*.tmp' -delete"), "only some of its files");
        assert!(!own("(cd .. && ls) && rm -rf feat"), "the subshell's cd is its own");
        assert!(!own("cat <<EOF > notes.md\nrm -r ..\nEOF\necho done"), "a heredoc's lines are text");
        assert!(own("cat <<'EOF' > notes.md\nhi\nEOF\nrm -rf ."), "what follows it runs");
        assert!(!own("find src -name '*.tmp' -delete"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn splits_commands_as_a_shell() {
        let words = |p: &[&[&str]]| p.iter().map(|w| Piece::Words(w.iter().map(|s| s.to_string()).collect())).collect::<Vec<_>>();
        assert_eq!(pieces("cd 'a b' && X=1 git \"c\\\"d\"|wc -l\nls"), words(&[&["cd", "a b"], &["X=1", "git", "c\"d"], &["wc", "-l"], &["ls"]]));
        assert_eq!(pieces("ls > out 2>&1 &> log"), words(&[&["ls"]]));
    }
}
