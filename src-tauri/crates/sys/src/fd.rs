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
