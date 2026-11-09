use std::{
    io::{Read, Write},
    sync::OnceLock,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};

/// A terminal attached to an agent's tmux session (or Prism's own, `tmux::local`): what the Agents
/// view displays. Closing it only detaches this client; the agent carries on.
pub struct Attachment {
    client: Client,
    /// Tells apart two successive attachments of the same agent (see `Agents::attach`).
    pub token: u64,
    label: String,
}

enum Client {
    /// A tmux client in a pseudo-terminal of its own.
    Tmux {
        master: Box<dyn MasterPty + Send>,
        writer: Box<dyn Write + Send>,
        killer: Box<dyn ChildKiller + Send + Sync>,
    },
    /// A client of one of Prism's own sessions.
    Local(Box<dyn crate::tmux::local::Attached>),
}

/// Diagnostics, with `PRISM_TRACE_AGENTS=1` only: everything the terminal sends to an agent
/// (keystrokes included), and what tmux sends back in the seconds following an attachment.
fn tracing_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PRISM_TRACE_AGENTS").is_some_and(|v| v != "0"))
}

/// Readable bytes: control characters as `\x..`, `\r`, `\n`, `\e`.
fn escape(bytes: &[u8]) -> String {
    let mut out = String::new();
    for c in String::from_utf8_lossy(bytes).chars() {
        match c {
            '\x1b' => out.push_str("\\e"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// What tmux sends right after attaching: enough to see its queries to the terminal.
const TRACE_OUTPUT: Duration = Duration::from_secs(3);

impl Attachment {
    /// Runs `argv` in a new pseudo-terminal; `on_data` receives its raw output from a
    /// dedicated thread, and `on_end` is called when it dries up (client detached or killed).
    pub fn spawn(
        label: &str,
        argv: &[String],
        cols: u16,
        rows: u16,
        token: u64,
        on_data: impl Fn(Vec<u8>) + Send + 'static,
        on_end: impl FnOnce() + Send + 'static,
    ) -> Result<Self> {
        let pair = native_pty_system().openpty(size(cols, rows)).context("opening a pty")?;
        // On Windows, an npm CLI's shim started as what it starts: cmd.exe never reads its arguments.
        let argv = prism_base::platform::terminal_argv(argv.to_vec())?;
        let mut cmd = CommandBuilder::new(&argv[0]);
        cmd.args(&argv[1..]);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        // Prism started from inside tmux (or its tests run in an agent's terminal): tmux refuses to
        // attach a client in a client ("sessions should be nested with care").
        cmd.env_remove("TMUX");
        let child = pair.slave.spawn_command(cmd).context("starting tmux attach")?;
        // The slave side now belongs to the tmux client.
        drop(pair.slave);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader()?;
        let writer = writer(pair.master.as_ref())?;

        let label = format!("{label}#{token}");
        if tracing_on() {
            tracing::info!("agent {label}: attach {cols}x{rows}");
        }
        let trace_label = label.clone();
        std::thread::spawn(move || {
            let mut child = child;
            let mut buf = vec![0u8; 64 * 1024];
            let start = Instant::now();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tracing_on() && start.elapsed() < TRACE_OUTPUT {
                            let shown: String = escape(&buf[..n]).chars().take(600).collect();
                            tracing::info!("agent {trace_label} → {:.2}s {n} bytes: {shown}", start.elapsed().as_secs_f32());
                        }
                        on_data(buf[..n].to_vec())
                    }
                }
            }
            let _ = child.wait();
            if tracing_on() {
                tracing::info!("agent {trace_label}: detached");
            }
            on_end();
        });
        Ok(Self { client: Client::Tmux { master: pair.master, writer, killer }, token, label })
    }

    /// A client of one of Prism's own sessions (`tmux::local`).
    pub fn local(label: &str, client: Box<dyn crate::tmux::local::Attached>, token: u64) -> Self {
        Self { client: Client::Local(client), token, label: format!("{label}#{token}") }
    }

    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        if tracing_on() {
            tracing::info!("agent {} ← {}", self.label, escape(data));
        }
        match &mut self.client {
            Client::Tmux { writer, .. } => {
                writer.write_all(data)?;
                writer.flush()?;
                Ok(())
            }
            Client::Local(c) => c.write(data),
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        match &self.client {
            Client::Tmux { master, .. } => master.resize(size(cols, rows)),
            Client::Local(c) => {
                c.resize(cols, rows);
                Ok(())
            }
        }
    }
}

impl Drop for Attachment {
    fn drop(&mut self) {
        if let Client::Tmux { killer, .. } = &mut self.client {
            let _ = killer.kill();
        }
    }
}

/// What the terminal types into the pty. Not portable-pty's writer on Unix: dropped, it sends `\n`
/// and Ctrl-D (an end of input) to the tmux client, which passes them on to the program before
/// dying — a dev server in a shell quits. A copy of the descriptor closes without a word.
#[cfg(unix)]
fn writer(master: &dyn MasterPty) -> Result<Box<dyn Write + Send>> {
    let fd = master.as_raw_fd().context("the pty has no descriptor")?;
    let owned = prism_sys::fd::dup(fd)?;
    Ok(Box::new(std::fs::File::from(owned)))
}

#[cfg(not(unix))]
fn writer(master: &dyn MasterPty) -> Result<Box<dyn Write + Send>> {
    master.take_writer()
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { cols: cols.max(20), rows: rows.max(5), pixel_width: 0, pixel_height: 0 }
}
