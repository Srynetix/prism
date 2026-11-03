//! A tmux server in memory, for the tests of `Agents`: it answers the commands `Tmux` runs
//! (sessions, panes, keys) and remembers them, without the real binary.

use std::{collections::BTreeMap, sync::Mutex};

use anyhow::{bail, Context, Result};

use super::{Tmux, PREFIX};

#[derive(Debug, Clone)]
struct Session {
    /// Exit status once its program has exited (`remain-on-exit`).
    dead: Option<i32>,
    argv: Vec<String>,
    /// Ctrl-C received: two in a row make the program exit, as the CLIs do.
    interrupts: u8,
}

#[derive(Default)]
pub struct Server {
    sessions: Mutex<BTreeMap<String, Session>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl Server {
    /// The commands run so far, each as its arguments.
    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }

    /// The commands run so far whose first argument is `command`.
    pub fn calls_of(&self, command: &str) -> Vec<Vec<String>> {
        self.calls().into_iter().filter(|c| c.first().is_some_and(|c| c == command)).collect()
    }

    /// The program the session of `key` (an agent, a shell) runs.
    pub fn argv(&self, key: &str) -> Option<Vec<String>> {
        self.sessions.lock().unwrap().get(&Tmux::session(key)).map(|s| s.argv.clone())
    }

    /// The program of `key`'s session exits with `code`; its pane stays.
    pub fn exit(&self, key: &str, code: i32) {
        if let Some(s) = self.sessions.lock().unwrap().get_mut(&Tmux::session(key)) {
            s.dead = Some(code);
        }
    }

    /// The server is gone (a computer restart): no session left.
    pub fn crash(&self) {
        self.sessions.lock().unwrap().clear();
    }

    pub fn run(&self, args: &[&str]) -> Result<String> {
        self.calls.lock().unwrap().push(args.iter().map(|a| a.to_string()).collect());
        let after = |flag: &str| args.iter().position(|a| *a == flag).and_then(|i| args.get(i + 1)).map(|a| a.to_string());
        let program = || args.iter().position(|a| *a == "--").map(|i| args[i + 1..].iter().map(|a| a.to_string()).collect()).unwrap_or_default();
        let mut sessions = self.sessions.lock().unwrap();
        let target = after("-t");
        let missing = |name: &str| anyhow::anyhow!("can't find session: {name}");
        match args.first().copied().unwrap_or_default() {
            "list-panes" => {
                let format = after("-F").context("list-panes without -F")?;
                let all = args.contains(&"-a");
                let lines: Vec<String> = sessions
                    .iter()
                    .filter(|(name, _)| all || target.as_deref() == Some(name.as_str()))
                    .map(|(name, s)| expand(&format, name, s))
                    .collect();
                if !all && lines.is_empty() {
                    return Err(missing(target.as_deref().unwrap_or_default()));
                }
                Ok(lines.iter().map(|l| format!("{l}\n")).collect())
            }
            "new-session" => {
                let name = after("-s").context("new-session without -s")?;
                if sessions.contains_key(&name) {
                    bail!("duplicate session: {name}");
                }
                sessions.insert(name, Session { dead: None, argv: program(), interrupts: 0 });
                Ok(String::new())
            }
            "respawn-pane" => {
                let name = target.context("respawn-pane without -t")?;
                let s = sessions.get_mut(&name).ok_or_else(|| missing(&name))?;
                *s = Session { dead: None, argv: program(), interrupts: 0 };
                Ok(String::new())
            }
            "kill-session" => {
                let name = target.context("kill-session without -t")?;
                sessions.remove(&name).map(|_| String::new()).ok_or_else(|| missing(&name))
            }
            "send-keys" => {
                let name = target.context("send-keys without -t")?;
                let s = sessions.get_mut(&name).ok_or_else(|| missing(&name))?;
                for key in args.iter().skip_while(|a| **a != name.as_str()).skip(1) {
                    if *key == "C-c" {
                        s.interrupts += 1;
                        if s.interrupts >= 2 && s.dead.is_none() {
                            s.dead = Some(130);
                        }
                    } else {
                        s.interrupts = 0;
                    }
                }
                Ok(String::new())
            }
            "display-message" => {
                let name = target.context("display-message without -t")?;
                let s = sessions.get(&name).ok_or_else(|| missing(&name))?;
                Ok(expand(args.last().unwrap_or(&""), &name, s))
            }
            _ => Ok(String::new()),
        }
    }
}

/// A tmux format, for one session's pane.
fn expand(format: &str, name: &str, s: &Session) -> String {
    format
        .replace("#{session_name}", name)
        .replace("#{pane_dead}", if s.dead.is_some() { "1" } else { "0" })
        .replace("#{pane_dead_status}", &s.dead.map(|c| c.to_string()).unwrap_or_default())
        .replace("#{pane_current_command}", s.argv.first().map_or("", String::as_str))
        .replace("#{pane_in_mode}", "0")
        .replace("#{window_width}", "120")
        .replace("#{window_height}", "40")
}

#[test]
fn answers_like_tmux() {
    let server = Server::default();
    let agent = format!("{PREFIX}a1");
    server.run(&["new-session", "-d", "-s", &agent, "--", "claude", "--session-id", "a1"]).unwrap();
    let panes = server.run(&["list-panes", "-a", "-F", "#{session_name}\t#{pane_dead}\t#{pane_dead_status}"]).unwrap();
    assert_eq!(panes, format!("{agent}\t0\t\n"));
    server.run(&["send-keys", "-t", &agent, "C-c"]).unwrap();
    server.run(&["send-keys", "-t", &agent, "C-c"]).unwrap();
    assert_eq!(server.run(&["list-panes", "-t", &agent, "-F", "#{pane_dead}\t#{pane_dead_status}"]).unwrap(), "1\t130\n");
    server.run(&["kill-session", "-t", &agent]).unwrap();
    assert!(server.run(&["kill-session", "-t", &agent]).unwrap_err().to_string().contains("can't find session"));
}
