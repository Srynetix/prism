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

/// The soft limit of open files before `raise_open_files` (0: not raised), for the children that
/// should start with it again (`set_open_files`), and what the raise did, for the log.
static ORIGINAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static RAISED: std::sync::OnceLock<Result<(u64, u64), String>> = std::sync::OnceLock::new();

fn soft_limit() -> std::io::Result<libc::rlimit> {
    let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: `getrlimit` writes the struct it is given, nothing else.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(lim)
}

/// The open files this process may hold, raised to `want` within the hard limit: a Mac app starts
/// at 256, which Prism's own sockets (a cluster watch per namespace), WebKit's files and a burst of
/// git commands outgrow ("Too many open files"). What it is now, as the system says once set (it
/// may lower it silently, `kern.maxfilesperproc`). Kept for `open_files_note`.
// `rlim_t` is `u64` here, not on every system.
#[allow(clippy::unnecessary_cast)]
pub fn raise_open_files(want: u64) -> std::io::Result<u64> {
    let raised = (|| {
        let lim = soft_limit()?;
        let before = lim.rlim_cur as u64;
        let target = (want as libc::rlim_t).min(lim.rlim_max);
        if lim.rlim_cur >= target {
            return Ok((before, before));
        }
        let new = libc::rlimit { rlim_cur: target, rlim_max: lim.rlim_max };
        // SAFETY: `setrlimit` reads the struct it is given; a value it refuses is an error, nothing more.
        if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &new) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        ORIGINAL.store(before, std::sync::atomic::Ordering::Relaxed);
        Ok((before, soft_limit()?.rlim_cur as u64))
    })();
    let _ = RAISED.set(raised.as_ref().map(|r| *r).map_err(|e| e.to_string()));
    raised.map(|(_, now)| now)
}

/// What `raise_open_files` did, in words for the log (none when it was not called).
pub fn open_files_note() -> Option<Result<String, String>> {
    RAISED.get().map(|r| r.clone().map(|(before, now)| format!("open files: {now} (was {before})")))
}

/// The soft limit the process started with, when `raise_open_files` raised it: what a long-lived
/// child that starts programs of its own (the tmux server) gets back (`set_open_files`).
pub fn original_open_files() -> Option<u64> {
    Some(ORIGINAL.load(std::sync::atomic::Ordering::Relaxed)).filter(|n| *n > 0)
}

/// The soft limit set to `n` (within the hard limit): in a child between fork and exec too, where
/// only async-signal-safe calls may run (`getrlimit` and `setrlimit` are).
#[allow(clippy::unnecessary_cast)]
pub fn set_open_files(n: u64) -> std::io::Result<()> {
    let lim = soft_limit()?;
    let new = libc::rlimit { rlim_cur: (n as libc::rlim_t).min(lim.rlim_max), rlim_max: lim.rlim_max };
    // SAFETY: as in `raise_open_files`.
    if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &new) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// `cmd` starts with the open files limit the process started with, when `raise_open_files`
/// raised it: a long-lived child that starts programs of its own (the tmux server, its agents).
pub fn start_with_original_open_files(cmd: &mut tokio::process::Command) {
    if let Some(n) = original_open_files() {
        // SAFETY: the closure only calls getrlimit/setrlimit, async-signal-safe, between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                let _ = set_open_files(n);
                Ok(())
            });
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn open_files_are_raised_within_the_hard_limit() {
        let now = super::raise_open_files(4096).unwrap();
        assert!(now >= 256);
        // Never lowered: asking for less keeps what it is.
        assert_eq!(super::raise_open_files(64).unwrap(), now);
        assert!(super::open_files_note().unwrap().unwrap().starts_with("open files: "));
    }
}
