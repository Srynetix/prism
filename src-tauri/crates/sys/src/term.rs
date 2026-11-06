//! The terminal Prism's subcommands are run from.

use std::io::{BufRead, Write};

/// A line typed on the terminal of standard input, not echoed (a secret), after `prompt` on
/// standard error. Echo comes back once the line is read, or failed to be.
pub fn read_hidden(prompt: &str) -> std::io::Result<String> {
    let mut err = std::io::stderr();
    write!(err, "{prompt}")?;
    err.flush()?;
    // SAFETY: a zeroed termios is a valid value for tcgetattr to fill; fd 0 is standard input,
    // which tcgetattr checks (ENOTTY, EBADF): nothing else is read or written.
    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: as above, `term` a valid, writable termios.
    if unsafe { libc::tcgetattr(0, &mut term) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let saved = term;
    // Echo off, but the newline still echoed: the next line starts where it should.
    term.c_lflag &= !libc::ECHO;
    term.c_lflag |= libc::ECHONL;
    // SAFETY: `term` is the terminal's own settings, echo changed; fd 0 checked by tcgetattr above.
    if unsafe { libc::tcsetattr(0, libc::TCSANOW, &term) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    // SAFETY: the settings read above, put back as they were.
    unsafe { libc::tcsetattr(0, libc::TCSANOW, &saved) };
    read.map(|_| line.trim_end_matches(['\n', '\r']).to_string())
}
