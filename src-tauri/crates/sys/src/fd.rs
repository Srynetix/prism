//! File descriptors known only by their number.

use std::os::fd::{FromRawFd, OwnedFd, RawFd};

/// A copy of descriptor `fd` (close-on-exec), owned: closing it leaves `fd` open. A descriptor
/// already closed is an error (`EBADF`).
pub fn dup(fd: RawFd) -> std::io::Result<OwnedFd> {
    // SAFETY: `fcntl(F_DUPFD_CLOEXEC)` reads no memory; on any descriptor it answers a new one or -1.
    let new = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if new < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `new` was just opened by fcntl and is owned by no one else.
    Ok(unsafe { OwnedFd::from_raw_fd(new) })
}

/// The open files this process (and what it starts) may hold, raised to `want` within the hard
/// limit: a Mac app starts at 256, which Prism's own sockets (a cluster watch per namespace),
/// WebKit's files and a burst of git commands outgrow ("Too many open files"). What it is now.
// `rlim_t` is `u64` here, not on every system.
#[allow(clippy::unnecessary_cast)]
pub fn raise_open_files(want: u64) -> std::io::Result<u64> {
    let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: `getrlimit` writes the struct it is given, nothing else.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let target = (want as libc::rlim_t).min(lim.rlim_max);
    if lim.rlim_cur >= target {
        return Ok(lim.rlim_cur as u64);
    }
    let new = libc::rlimit { rlim_cur: target, rlim_max: lim.rlim_max };
    // SAFETY: `setrlimit` reads the struct it is given; a value it refuses is an error, nothing more.
    if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &new) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(target as u64)
}

#[cfg(test)]
mod tests {
    #[test]
    fn open_files_are_raised_within_the_hard_limit() {
        let now = super::raise_open_files(4096).unwrap();
        assert!(now >= 256);
        // Never lowered: asking for less keeps what it is.
        assert_eq!(super::raise_open_files(64).unwrap(), now);
    }
}
