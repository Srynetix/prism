//! Processes: signals, sessions, the user id, and macOS's spawn of a program responsible for itself.

/// The user id Prism runs as.
pub fn uid() -> u32 {
    // SAFETY: getuid never fails and reads no memory.
    unsafe { libc::getuid() }
}

/// The signals Prism sends another Prism: SIGUSR1 brings its window up, SIGUSR2 restarts it.
#[derive(Debug, Clone, Copy)]
pub enum Signal {
    Usr1,
    Usr2,
}

/// Sends `signal` to process `pid`; false if it could not (gone, not ours).
pub fn signal(pid: u32, signal: Signal) -> bool {
    let sig = match signal {
        Signal::Usr1 => libc::SIGUSR1,
        Signal::Usr2 => libc::SIGUSR2,
    };
    // SAFETY: kill reads no memory; the signals sent are handled by Prism, never a group (pid > 0).
    pid > 0 && unsafe { libc::kill(pid as libc::pid_t, sig) } == 0
}

/// Kills process group `pgid` with SIGKILL: a program and every child it started in its group (a
/// custom block's script past its timeout); false if it could not (gone already). Never the
/// caller's own group nor every process (`pgid` 0 or 1 refused).
pub fn kill_group(pgid: u32) -> bool {
    // SAFETY: getpgrp never fails and reads no memory.
    let own = unsafe { libc::getpgrp() } as u32;
    if pgid <= 1 || pgid == own {
        return false;
    }
    // SAFETY: killpg reads no memory; our own group and groups 0 and 1 are refused above.
    unsafe { libc::killpg(pgid as libc::pid_t, libc::SIGKILL) == 0 }
}

/// Whether process `pid` exists (someone else's included).
pub fn exists(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    pid > 0 && (unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

/// The child of `cmd` in a session of its own, detached from the terminal's.
pub fn new_session(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::unix::process::CommandExt;
    // SAFETY: the hook runs in the child between fork and exec, where setsid (async-signal-safe,
    // no allocation) is all it calls.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        })
    }
}

/// macOS: runs `program` (found in the PATH) as a process responsible for itself rather than for
/// Prism, its input and output on `/dev/null` and its error output in the file `err`, and waits
/// for it: its exit code (-1 when killed by a signal). The attribute is a private one of
/// libSystem (the one terminals use); without it, a plain spawn. Blocking.
#[cfg(target_os = "macos")]
pub fn spawn_disclaimed(program: &str, args: &[String], err: &std::path::Path) -> std::io::Result<i32> {
    use std::ffi::CString;
    use std::io::{Error, ErrorKind};
    use std::ptr;

    let cstr = |s: &str| CString::new(s).map_err(|_| Error::new(ErrorKind::InvalidInput, format!("a NUL byte in {s:?}")));
    let prog = cstr(program)?;
    let owned: Vec<CString> = std::iter::once(Ok(prog.clone())).chain(args.iter().map(|a| cstr(a))).collect::<std::io::Result<_>>()?;
    let mut argv: Vec<*mut libc::c_char> = owned.iter().map(|c| c.as_ptr() as *mut _).collect();
    argv.push(ptr::null_mut());
    let err_c = cstr(&err.display().to_string())?;
    let null = cstr("/dev/null")?;
    // Close every other descriptor of Prism in the child (Apple's POSIX_SPAWN_CLOEXEC_DEFAULT).
    const CLOEXEC_DEFAULT: libc::c_short = 0x4000;
    type Disclaim = unsafe extern "C" fn(*mut libc::posix_spawnattr_t, libc::c_int) -> libc::c_int;

    // SAFETY: plain libc calls on locals initialised here, destroyed before returning; the
    // argument and environment arrays are NUL-terminated and outlive the spawn; the symbol looked
    // up has the signature of `Disclaim` (libSystem's `responsibility.h`).
    unsafe {
        let mut actions: libc::posix_spawn_file_actions_t = ptr::null_mut();
        let mut attr: libc::posix_spawnattr_t = ptr::null_mut();
        let code = libc::posix_spawn_file_actions_init(&mut actions);
        if code != 0 {
            return Err(Error::from_raw_os_error(code));
        }
        let code = libc::posix_spawnattr_init(&mut attr);
        if code != 0 {
            libc::posix_spawn_file_actions_destroy(&mut actions);
            return Err(Error::from_raw_os_error(code));
        }
        // The first that fails, if any (each returns 0 or an error number).
        let code = [
            libc::posix_spawn_file_actions_addopen(&mut actions, 0, null.as_ptr(), libc::O_RDONLY, 0),
            libc::posix_spawn_file_actions_addopen(&mut actions, 1, null.as_ptr(), libc::O_WRONLY, 0),
            libc::posix_spawn_file_actions_addopen(&mut actions, 2, err_c.as_ptr(), libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC, 0o600),
            libc::posix_spawnattr_setflags(&mut attr, CLOEXEC_DEFAULT),
        ]
        .into_iter()
        .find(|&c| c != 0);
        if let Some(code) = code {
            libc::posix_spawn_file_actions_destroy(&mut actions);
            libc::posix_spawnattr_destroy(&mut attr);
            return Err(Error::from_raw_os_error(code));
        }
        let sym = libc::dlsym(libc::RTLD_DEFAULT, c"responsibility_spawnattrs_setdisclaim".as_ptr());
        if sym.is_null() {
            tracing::warn!("platform: no responsibility_spawnattrs_setdisclaim, {program} stays Prism's");
        } else {
            let disclaim: Disclaim = std::mem::transmute(sym);
            disclaim(&mut attr, 1);
        }
        let mut pid: libc::pid_t = 0;
        let spawned = libc::posix_spawnp(&mut pid, prog.as_ptr(), &actions, &attr, argv.as_ptr(), *libc::_NSGetEnviron() as *const *mut libc::c_char);
        libc::posix_spawn_file_actions_destroy(&mut actions);
        libc::posix_spawnattr_destroy(&mut attr);
        if spawned != 0 {
            return Err(Error::from_raw_os_error(spawned));
        }
        let mut status: libc::c_int = 0;
        while libc::waitpid(pid, &mut status, 0) == -1 {
            if Error::last_os_error().kind() != ErrorKind::Interrupted {
                break;
            }
        }
        Ok(if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) } else { -1 })
    }
}
