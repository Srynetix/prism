//! The terminal Prism's subcommands are run from.

use std::io::{Read, Write};

/// A line typed on the terminal of standard input, not echoed (a secret), after `prompt` on
/// standard error. The terminal reads key by key meanwhile, its signals off: Ctrl+C is read as a
/// key (`Interrupted`), so the settings are always put back, echo with them.
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
    term.c_lflag &= !(libc::ECHO | libc::ICANON | libc::ISIG);
    term.c_cc[libc::VMIN] = 1;
    term.c_cc[libc::VTIME] = 0;
    // SAFETY: `term` is the terminal's own settings, changed; fd 0 checked by tcgetattr above.
    if unsafe { libc::tcsetattr(0, libc::TCSAFLUSH, &term) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let read = keys(&mut std::io::stdin().lock());
    // SAFETY: the settings read above, put back as they were.
    unsafe { libc::tcsetattr(0, libc::TCSAFLUSH, &saved) };
    let _ = writeln!(err);
    read
}

/// The line typed, key by key: Enter ends it, Backspace and Ctrl+U erase, Ctrl+C cancels, Ctrl+D
/// on an empty line is the end of input.
fn keys(input: &mut impl Read) -> std::io::Result<String> {
    let mut line: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if input.read(&mut byte)? == 0 {
            break;
        }
        match byte[0] {
            b'\n' | b'\r' => break,
            3 => return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled")),
            4 if line.is_empty() => break,
            8 | 127 => {
                // One character back, whatever its UTF-8 length.
                while let Some(b) = line.pop() {
                    if b & 0xC0 != 0x80 {
                        break;
                    }
                }
            }
            21 => line.clear(),
            b => line.push(b),
        }
    }
    String::from_utf8(line).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::keys;

    #[test]
    fn keys_make_the_line() {
        assert_eq!(keys(&mut &b"abc\n"[..]).unwrap(), "abc");
        assert_eq!(keys(&mut &b"abx\x7fc\r"[..]).unwrap(), "abc");
        assert_eq!(keys(&mut "é\x7fe\n".as_bytes()).unwrap(), "e");
        assert_eq!(keys(&mut &b"nope\x15ok\n"[..]).unwrap(), "ok");
        assert_eq!(keys(&mut &b"\x04"[..]).unwrap(), "");
        assert_eq!(keys(&mut &b"ab\x03"[..]).unwrap_err().kind(), std::io::ErrorKind::Interrupted);
    }
}
