//! Sessions Prism holds itself, in place of its tmux server where there is none (Windows, its
//! ConPTY): each one a pseudo-terminal running its program, its screen and history kept by a
//! terminal emulator (`vt100`), the terminals attached to it fed what it prints. They live in
//! Prism's process and stop with it: an agent is resumed at the next launch, not found running.
//!
//! What tmux does for Prism and this does too: a session per agent or shell, named as tmux names
//! it (`Tmux::session`); several clients, the window taking the size of the latest to ask
//! (`window-size latest`); a dead program's last screen kept (`remain-on-exit`) unless the session
//! ends with it; keys, pastes (bracketed when the program asks), the history with its colours.
//! What it does not: copy mode (the window's terminal selects), external terminals.

use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use anyhow::{bail, Context, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};

use super::{key_bytes, pasteable, Pane, ShellSession, PREFIX, SHELL_PREFIX};

/// Lines of history a session keeps, as tmux's `history-limit`.
const HISTORY: usize = 20_000;

/// This process's sessions, by tmux name.
static SERVER: LazyLock<Server> = LazyLock::new(Server::default);

/// The process's sessions.
pub fn server() -> &'static Server {
    &SERVER
}

type Sessions = Arc<Mutex<HashMap<String, Arc<Session>>>>;

#[derive(Default)]
pub struct Server {
    sessions: Sessions,
    next_client: std::sync::atomic::AtomicU64,
}

pub struct Session {
    name: String,
    /// Its server's sessions: one ending with its program leaves them.
    sessions: Sessions,
    state: Mutex<State>,
}

struct State {
    screen: vt100::Parser,
    pty: Option<Pty>,
    /// The program's exit status, once it has exited.
    exited: Option<i32>,
    /// Its last screen kept once its program has exited (an agent's); a shell's session ends.
    remain: bool,
    cwd: PathBuf,
    /// Unix seconds.
    created: i64,
    /// `set_note`'s, as tmux's user options.
    notes: HashMap<String, String>,
    clients: Vec<Client>,
    size: (u16, u16),
    /// Which pty the reading thread reads: a respawned session's former one is no longer heard.
    generation: u64,
}

struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
}

type OnData = Box<dyn Fn(Vec<u8>) + Send>;
type OnEnd = Box<dyn FnOnce() + Send>;

struct Client {
    id: u64,
    /// A phone watching: it never types, and its size counts only when `sized`.
    read_only: bool,
    on_data: OnData,
    on_end: Option<OnEnd>,
}

impl Server {
    fn get(&self, name: &str) -> Result<Arc<Session>> {
        self.sessions.lock().unwrap().get(name).cloned().with_context(|| format!("can't find session: {name}"))
    }

    /// A new session running `argv` in `cwd` (120×40, as tmux's), with `env` (`NAME=value`) on
    /// top of Prism's environment.
    pub fn new_session(&self, name: &str, cwd: &Path, env: &[String], argv: &[String]) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.contains_key(name) {
            bail!("duplicate session: {name}");
        }
        let size = (120, 40);
        let session = Arc::new(Session {
            name: name.to_string(),
            sessions: self.sessions.clone(),
            state: Mutex::new(State {
                screen: vt100::Parser::new(nz(size.1), nz(size.0), HISTORY),
                pty: None,
                exited: None,
                remain: true,
                cwd: cwd.to_path_buf(),
                created: chrono::Utc::now().timestamp(),
                notes: HashMap::new(),
                clients: Vec::new(),
                size,
                generation: 0,
            }),
        });
        session.start(cwd, env, argv)?;
        sessions.insert(name.to_string(), session);
        Ok(())
    }

    /// Replaces the session's program (killed if it still runs) with `argv`: its screen, history
    /// and clients stay, they see the new one start.
    pub fn respawn(&self, name: &str, cwd: &Path, env: &[String], argv: &[String]) -> Result<()> {
        let session = self.get(name)?;
        let old = {
            let mut st = session.state.lock().unwrap();
            st.exited = None;
            st.cwd = cwd.to_path_buf();
            st.pty.take()
        };
        if let Some(mut old) = old {
            let _ = old.killer.kill();
        }
        session.start(cwd, env, argv)
    }

    /// Every session killed: Prism quits.
    pub fn stop_all(&self) {
        let all: Vec<Arc<Session>> = self.sessions.lock().unwrap().drain().map(|(_, s)| s).collect();
        for session in all {
            session.close();
        }
    }

    pub fn kill(&self, name: &str) {
        let Some(session) = self.sessions.lock().unwrap().remove(name) else { return };
        session.close();
    }

    /// Every session's state, by tmux name.
    pub fn panes(&self) -> Vec<(String, Pane)> {
        self.sessions.lock().unwrap().values().map(|s| (s.name.clone(), s.pane())).collect()
    }

    /// The state of a session's program and the one in its foreground; `None`: no such session.
    pub fn pane(&self, name: &str) -> Option<(Pane, String)> {
        let session = self.get(name).ok()?;
        let pane = session.pane();
        Some((pane, session.command(&processes())))
    }

    /// The sessions' programs, by tmux name, with their process: what the System and Dev servers
    /// blocks read of tmux's `#{pane_pid}` and `#{pane_current_command}`.
    pub fn processes(&self) -> Vec<(String, Option<u32>, String, bool)> {
        let sessions: Vec<Arc<Session>> = self.sessions.lock().unwrap().values().cloned().collect();
        let sys = processes();
        sessions
            .iter()
            .map(|s| {
                let (pid, dead) = {
                    let st = s.state.lock().unwrap();
                    (st.pty.as_ref().and_then(|p| p.pid), st.exited.is_some())
                };
                (s.name.clone(), pid, s.command(&sys), dead)
            })
            .collect()
    }

    /// The shells' sessions (`prismsh-<owner>-<n>`).
    pub fn shells(&self) -> Vec<ShellSession> {
        let sessions: Vec<Arc<Session>> = self.sessions.lock().unwrap().values().cloned().collect();
        let sys = processes();
        let mut out: Vec<ShellSession> = sessions
            .iter()
            .filter_map(|s| {
                let (owner, n) = s.name.strip_prefix(SHELL_PREFIX)?.rsplit_once('-')?;
                let (path, created, pod) = {
                    let st = s.state.lock().unwrap();
                    (st.cwd.display().to_string(), st.created, st.notes.get("prism-pod").cloned().unwrap_or_default())
                };
                Some(ShellSession { owner: owner.to_string(), n: n.parse().ok()?, path, created, command: s.command(&sys), pod })
            })
            .collect();
        out.sort_by(|a, b| (&a.owner, a.n).cmp(&(&b.owner, b.n)));
        out
    }

    pub fn set_note(&self, name: &str, key: &str, value: &str) -> Result<()> {
        self.get(name)?.state.lock().unwrap().notes.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// A shell's session ends with its program: no last screen kept.
    pub fn end_with_program(&self, name: &str) -> Result<()> {
        let session = self.get(name)?;
        let exited = {
            let mut st = session.state.lock().unwrap();
            st.remain = false;
            st.exited.is_some()
        };
        if exited {
            session.leave();
        }
        Ok(())
    }

    /// Types `keys` as tmux's `send-keys` names them.
    pub fn send_keys(&self, name: &str, keys: &[&str]) -> Result<()> {
        let bytes = key_bytes(keys).with_context(|| format!("keys not known: {}", keys.join(" ")))?;
        self.get(name)?.write(bytes.as_bytes())
    }

    /// Pastes `text` as one block: bracketed when the program asked for it.
    pub fn paste(&self, name: &str, text: &str) -> Result<()> {
        let session = self.get(name)?;
        let text = pasteable(text);
        let bracketed = session.state.lock().unwrap().screen.screen().bracketed_paste();
        let data = if bracketed { format!("\x1b[200~{text}\x1b[201~") } else { text };
        session.write(data.as_bytes())
    }

    /// The last `lines` of the history, and the screen with `screen`, with their colours.
    pub fn history(&self, name: &str, lines: u32, screen: bool) -> Result<String> {
        let session = self.get(name)?;
        let mut st = session.state.lock().unwrap();
        Ok(history(&mut st.screen, lines as usize, screen))
    }

    /// The last `lines` of the history and the screen, as plain text (tmux's `capture-pane -p`).
    pub fn text(&self, name: &str, lines: u32) -> Result<String> {
        let session = self.get(name)?;
        let mut st = session.state.lock().unwrap();
        Ok(text(&mut st.screen, lines as usize))
    }

    /// The visible screen, with its colours.
    pub fn screen(&self, name: &str) -> Result<String> {
        let session = self.get(name)?;
        let st = session.state.lock().unwrap();
        Ok(st.screen.screen().rows_formatted(0, st.size.0).collect::<Vec<_>>().join("\n"))
    }

    pub fn window_size(&self, name: &str) -> Result<(u16, u16)> {
        Ok(self.get(name)?.state.lock().unwrap().size)
    }

    /// A client of the session: `on_data` gets its screen now, then what its program prints;
    /// `on_end` is called when the session ends or the client is dropped. Its size, given, is the
    /// window's (the latest client's).
    pub fn attach(&self, name: &str, size: Option<(u16, u16)>, read_only: bool, on_data: OnData, on_end: OnEnd) -> Result<LocalClient> {
        let session = self.get(name)?;
        let id = self.next_client.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some((cols, rows)) = size {
            session.resize(cols, rows);
        }
        let mut st = session.state.lock().unwrap();
        // The screen as it stands, drawn from a clear one, its modes (bracketed paste, the mouse,
        // the cursor) with it.
        let mut now = b"\x1b[H\x1b[2J".to_vec();
        now.extend(st.screen.screen().state_formatted().into_bytes());
        on_data(now);
        st.clients.push(Client { id, read_only, on_data, on_end: Some(on_end) });
        drop(st);
        Ok(LocalClient { session, id, read_only })
    }
}

impl Session {
    fn start(self: &Arc<Self>, cwd: &Path, env: &[String], argv: &[String]) -> Result<()> {
        let shell;
        let argv = if argv.is_empty() {
            shell = default_shell();
            &shell[..]
        } else {
            argv
        };
        let program = &argv[0];
        let (cols, rows) = self.state.lock().unwrap().size;
        let pair = native_pty_system().openpty(PtySize { cols, rows, pixel_width: 0, pixel_height: 0 }).context("opening a pty")?;
        let mut cmd = CommandBuilder::new(program);
        cmd.args(&argv[1..]);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("PRISM_PROFILE", prism_core::profile::active());
        for var in env {
            if let Some((k, v)) = var.split_once('=') {
                cmd.env(k, v);
            }
        }
        let mut child = pair.slave.spawn_command(cmd).with_context(|| format!("starting {program}"))?;
        drop(pair.slave);
        let pid = child.process_id();
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let generation = {
            let mut st = self.state.lock().unwrap();
            st.generation += 1;
            st.pty = Some(Pty { master: pair.master, writer, killer, pid });
            st.generation
        };
        let me = self.clone();
        std::thread::Builder::new().name(format!("session {}", self.name)).spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if !me.output(generation, &buf[..n]) {
                            return;
                        }
                    }
                }
            }
            let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
            me.exited(generation, code);
        })?;
        Ok(())
    }

    /// What the program printed: on the screen, to the clients. False: a former program's.
    fn output(&self, generation: u64, bytes: &[u8]) -> bool {
        let mut st = self.state.lock().unwrap();
        if st.generation != generation {
            return false;
        }
        st.screen.process(bytes);
        for c in &st.clients {
            (c.on_data)(bytes.to_vec());
        }
        // The cursor's position asked (`ESC [ 6 n`; ConPTY asks it as it starts) with no terminal
        // to answer: answered here, or the program waits for it.
        if !st.clients.iter().any(|c| !c.read_only) && contains(bytes, b"\x1b[6n") {
            let (row, col) = st.screen.screen().cursor_position();
            let answer = format!("\x1b[{};{}R", row + 1, col + 1);
            if let Some(pty) = st.pty.as_mut() {
                let _ = pty.writer.write_all(answer.as_bytes());
                let _ = pty.writer.flush();
            }
        }
        true
    }

    fn exited(self: &Arc<Self>, generation: u64, code: i32) {
        let remain = {
            let mut st = self.state.lock().unwrap();
            if st.generation != generation {
                return;
            }
            st.exited = Some(code);
            st.pty = None;
            st.remain
        };
        if !remain {
            self.leave();
        }
    }

    fn pane(&self) -> Pane {
        match self.state.lock().unwrap().exited {
            Some(code) => Pane::Exited(code),
            None => Pane::Alive,
        }
    }

    /// The program in the foreground: the pty's on Unix, the newest of the program's descendants
    /// elsewhere; its name without `.exe`.
    fn command(&self, sys: &sysinfo::System) -> String {
        let (pid, leader) = {
            let st = self.state.lock().unwrap();
            let pty = st.pty.as_ref();
            #[cfg(unix)]
            let leader = pty.and_then(|p| p.master.process_group_leader());
            #[cfg(not(unix))]
            let leader: Option<i32> = None;
            (pty.and_then(|p| p.pid), leader)
        };
        foreground(sys, pid, leader.and_then(|l| u32::try_from(l).ok()))
    }

    fn write(&self, data: &[u8]) -> Result<()> {
        let mut st = self.state.lock().unwrap();
        let pty = st.pty.as_mut().context("the program has exited")?;
        pty.writer.write_all(data)?;
        pty.writer.flush()?;
        Ok(())
    }

    fn resize(&self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(20), rows.max(5));
        let mut st = self.state.lock().unwrap();
        if st.size == (cols, rows) {
            return;
        }
        st.size = (cols, rows);
        st.screen.set_size(nz(rows), nz(cols));
        if let Some(pty) = &st.pty {
            let _ = pty.master.resize(PtySize { cols, rows, pixel_width: 0, pixel_height: 0 });
        }
    }

    /// Out of its server (if still there), then closed.
    fn leave(self: &Arc<Self>) {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.get(&self.name).is_some_and(|s| Arc::ptr_eq(s, self)) {
            sessions.remove(&self.name);
        }
        drop(sessions);
        self.close();
    }

    /// The session gone: its program killed, its clients told.
    fn close(&self) {
        let (pty, ends) = {
            let mut st = self.state.lock().unwrap();
            st.generation += 1;
            let ends: Vec<OnEnd> = st.clients.drain(..).filter_map(|mut c| c.on_end.take()).collect();
            (st.pty.take(), ends)
        };
        if let Some(mut pty) = pty {
            let _ = pty.killer.kill();
        }
        for end in ends {
            end();
        }
    }
}

/// A terminal attached to a session: it types, it resizes the window. Dropped, it detaches.
pub struct LocalClient {
    session: Arc<Session>,
    id: u64,
    read_only: bool,
}

impl LocalClient {
    pub fn write(&self, data: &[u8]) -> Result<()> {
        if self.read_only {
            bail!("client is read-only");
        }
        self.session.write(data)
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        self.session.resize(cols, rows);
    }
}

impl Drop for LocalClient {
    fn drop(&mut self) {
        let end = {
            let mut st = self.session.state.lock().unwrap();
            let at = st.clients.iter().position(|c| c.id == self.id);
            at.and_then(|i| st.clients.remove(i).on_end.take())
        };
        if let Some(end) = end {
            end();
        }
    }
}

/// The last `lines` lines of `parser`'s history (what scrolled off its screen), and its screen
/// after with `screen`, formatted: read a screenful at a time, scrolled back.
fn history(parser: &mut vt100::Parser, lines: usize, screen: bool) -> String {
    let (rows, cols) = parser.screen().size();
    let (rows, cols) = (usize::from(rows.get()), cols.get());
    parser.screen_mut().set_scrollback(usize::MAX);
    let total = parser.screen().scrollback();
    let mut out: Vec<String> = Vec::new();
    let mut offset = total;
    while offset > 0 {
        parser.screen_mut().set_scrollback(offset);
        // At `offset`, the top `offset` rows shown are history.
        out.extend(parser.screen().rows_formatted(0, cols).take(offset.min(rows)));
        offset = offset.saturating_sub(rows);
    }
    parser.screen_mut().set_scrollback(0);
    if screen {
        let mut shown: Vec<String> = parser.screen().rows_formatted(0, cols).collect();
        // Not the blank rows under the last one written, as tmux's capture.
        while shown.last().is_some_and(|r| r.trim().is_empty()) {
            shown.pop();
        }
        out.extend(shown);
    }
    let skip = out.len().saturating_sub(lines);
    out.drain(..skip);
    out.join("\n")
}

/// `history`, as plain text: the history and the screen.
fn text(parser: &mut vt100::Parser, lines: usize) -> String {
    let (rows, cols) = parser.screen().size();
    let (rows, cols) = (usize::from(rows.get()), cols.get());
    parser.screen_mut().set_scrollback(usize::MAX);
    let total = parser.screen().scrollback();
    let mut out: Vec<String> = Vec::new();
    let mut offset = total;
    while offset > 0 {
        parser.screen_mut().set_scrollback(offset);
        out.extend(parser.screen().rows(0, cols).take(offset.min(rows)));
        offset = offset.saturating_sub(rows);
    }
    parser.screen_mut().set_scrollback(0);
    out.extend(parser.screen().rows(0, cols));
    let skip = out.len().saturating_sub(lines);
    out.drain(..skip);
    out.join("\n")
}

/// The program in a session's foreground, by name: the pty's process group leader where there is
/// one (Unix), else the newest process descending from the session's, else that one.
fn foreground(sys: &sysinfo::System, pid: Option<u32>, leader: Option<u32>) -> String {
    use sysinfo::Pid;
    let Some(pid) = pid else { return String::new() };
    let name = |p: u32| sys.process(Pid::from_u32(p)).map(|p| p.name().to_string_lossy().trim_end_matches(".exe").to_string());
    if let Some(n) = leader.and_then(name) {
        return n;
    }
    let mut newest = Pid::from_u32(pid);
    let mut start = 0;
    let mut frontier = vec![Pid::from_u32(pid)];
    while let Some(parent) = frontier.pop() {
        for (child, p) in sys.processes().iter().filter(|(_, p)| p.parent() == Some(parent)) {
            if p.start_time() >= start {
                (newest, start) = (*child, p.start_time());
            }
            frontier.push(*child);
        }
    }
    name(newest.as_u32()).or_else(|| name(pid)).unwrap_or_default()
}

/// The shell a session without a program runs, as tmux's default: the user's, as a login shell
/// (`$SHELL -l`, else `/bin/sh`); on Windows, PowerShell 7 if installed, else Windows PowerShell,
/// else `%COMSPEC%`.
fn default_shell() -> Vec<String> {
    if cfg!(windows) {
        if let Some(p) = ["pwsh", "powershell"].into_iter().find(|p| prism_core::detect::which(p).is_some()) {
            return vec![p.into(), "-NoLogo".into()];
        }
        return vec![std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())];
    }
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/sh".into());
    vec![shell, "-l".into()]
}

/// The system's processes, their names and parents: read once for every session asked about.
fn processes() -> sysinfo::System {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn nz(n: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(n.max(1)).unwrap()
}

/// The agents' sessions of `panes`, by agent id.
pub fn agent_panes(panes: Vec<(String, Pane)>) -> HashMap<String, Pane> {
    panes.into_iter().filter_map(|(name, pane)| Some((name.strip_prefix(PREFIX)?.to_string(), pane))).collect()
}

#[cfg(all(test, unix))]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn wait(what: &str, mut ok: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ok() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn sh(script: &str) -> Vec<String> {
        vec!["/bin/sh".into(), "-c".into(), script.into()]
    }

    /// A session's whole path: started, typed into, attached, seen, exited with its screen kept,
    /// killed.
    #[test]
    fn a_session_runs_keeps_its_screen_and_ends() {
        let s = Server::default();
        let dir = std::env::temp_dir();
        s.new_session("prism-local-1", &dir, &["GREETING=hello".into()], &sh("echo $GREETING; read line; echo got $line; exit 3")).unwrap();
        wait("the greeting", || s.screen("prism-local-1").unwrap().contains("hello"));
        let (tx, rx) = std::sync::mpsc::channel();
        let client = s.attach("prism-local-1", Some((100, 30)), false, Box::new(move |b| { let _ = tx.send(b); }), Box::new(|| {})).unwrap();
        let first = String::from_utf8_lossy(&rx.recv_timeout(Duration::from_secs(5)).unwrap()).into_owned();
        assert!(first.contains("hello"), "the screen as it stands: {first}");
        assert_eq!(s.window_size("prism-local-1").unwrap(), (100, 30));
        client.write(b"there").unwrap();
        s.send_keys("prism-local-1", &["Enter"]).unwrap();
        wait("its exit", || s.pane("prism-local-1").map(|p| p.0) == Some(Pane::Exited(3)));
        assert!(s.history("prism-local-1", 100, true).unwrap().contains("got there"));
        assert_eq!(agent_panes(s.panes()).get("local-1"), Some(&Pane::Exited(3)));
        drop(client);
        s.kill("prism-local-1");
        assert!(s.pane("prism-local-1").is_none());
    }

    /// A shell's session ends with its program, and its clients are told.
    #[test]
    fn a_shell_session_ends_with_its_program() {
        let s = server();
        let dir = std::env::temp_dir();
        s.new_session("prismsh-own-1", &dir, &[], &sh("read line")).unwrap();
        s.end_with_program("prismsh-own-1").unwrap();
        s.set_note("prismsh-own-1", "prism-pod", "{}").unwrap();
        let shells = s.shells();
        let mine = shells.iter().find(|x| x.owner == "own").unwrap();
        assert_eq!((mine.n, mine.pod.as_str()), (1, "{}"));
        let (tx, rx) = std::sync::mpsc::channel();
        let _client = s.attach("prismsh-own-1", None, false, Box::new(|_| {}), Box::new(move || { let _ = tx.send(()); })).unwrap();
        s.send_keys("prismsh-own-1", &["-l", "x"]).unwrap();
        s.send_keys("prismsh-own-1", &["Enter"]).unwrap();
        rx.recv_timeout(Duration::from_secs(10)).expect("told the session ended");
        assert!(s.pane("prismsh-own-1").is_none());
    }

    /// The history: what scrolled off the screen, then the screen; the last lines only.
    #[test]
    fn the_history_is_read_back() {
        let mut p = vt100::Parser::new(nz(5), nz(20), 100);
        for i in 1..=12 {
            p.process(format!("line {i}\r\n").as_bytes());
        }
        let all = history(&mut p, 100, true);
        let plain: Vec<String> = all.lines().map(|l| String::from_utf8(strip(l.as_bytes())).unwrap()).collect();
        assert_eq!(plain.first().map(|l| l.trim()), Some("line 1"));
        assert_eq!(plain.last().map(|l| l.trim()), Some("line 12"));
        assert_eq!(plain.len(), 12);
        let past = history(&mut p, 3, false);
        assert_eq!(past.lines().count(), 3);
        assert_eq!(p.screen().scrollback(), 0);
    }

    /// Pasted text arrives bracketed once the program asks for it.
    #[test]
    fn a_paste_is_bracketed_when_asked() {
        let s = Server::default();
        s.new_session("prism-paste", &std::env::temp_dir(), &[], &sh("printf '\\033[?2004h'; cat")).unwrap();
        wait("bracketed paste on", || s.get("prism-paste").unwrap().state.lock().unwrap().screen.screen().bracketed_paste());
        s.paste("prism-paste", "a\nb").unwrap();
        wait("the paste echoed", || s.screen("prism-paste").unwrap().contains('b'));
        s.kill("prism-paste");
    }

    fn strip(b: &[u8]) -> Vec<u8> {
        let mut p = vt100::Parser::new(nz(1), nz(200), 0);
        p.process(b);
        p.screen().contents().into_bytes()
    }
}
