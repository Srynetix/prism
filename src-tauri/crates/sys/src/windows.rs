//! Windows' calls, behind safe functions: the processes Prism looks at, the messages between two
//! Prisms (Unix's SIGUSR1 / SIGUSR2), the Recycle Bin, sleep held off, folders made the account's
//! only (an ACL, Unix's 0700), and the person at the computer confirmed by Windows Hello.

use std::ffi::{c_void, OsStr, OsString};
use std::io::{self, Write};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, STILL_ACTIVE};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW, SetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{SECURITY_ATTRIBUTES, GetSecurityDescriptorDacl, GetTokenInformation, TokenUser, ACL, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER};
use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetExitCodeProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHFileOperationW, SHQueryRecycleBinW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING, FO_DELETE, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHFILEOPSTRUCTW,
    SHQUERYRBINFO,
};

/// `s` as Windows reads it: UTF-16, NUL-terminated.
fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// A handle of ours, closed when dropped.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle this module opened, closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Memory Windows allocated for us with LocalAlloc, freed when dropped.
struct Local(*mut c_void);

impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: memory a call of this module received, to free with LocalFree, freed once.
            unsafe { LocalFree(self.0) };
        }
    }
}

/// A string Windows wrote (NUL-terminated UTF-16), read up to its NUL.
///
/// # Safety
/// `p` points to a NUL-terminated UTF-16 string, alive while it is read.
unsafe fn read_wide(p: *const u16) -> String {
    let mut len = 0;
    // SAFETY: the caller's: a NUL ends the string.
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: the `len` units read above.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
}

/// The executable of process `pid` while it runs; `None`: gone, or not this account's to look at.
pub fn executable(pid: u32) -> Option<PathBuf> {
    if pid == 0 {
        return None;
    }
    // SAFETY: no pointer; a null handle (refused, gone) is checked.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let handle = Handle(handle);
    let mut code = 0u32;
    // SAFETY: a handle opened above, `code` ours to write.
    if unsafe { GetExitCodeProcess(handle.0, &mut code) } == 0 || code != STILL_ACTIVE as u32 {
        return None;
    }
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` holds `len` units; Windows writes at most that many and sets `len` to what it wrote.
    if unsafe { QueryFullProcessImageNameW(handle.0, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) } == 0 {
        return None;
    }
    Some(PathBuf::from(OsString::from_wide(&buf[..len as usize])))
}

/// The pipe on which Prism's process `pid` hears another launch: what SIGUSR1 / SIGUSR2 are on
/// Unix. Windows' default security on a pipe lets only its creator's account write to it.
pub fn signal_pipe(pid: u32) -> String {
    format!(r"\\.\pipe\prism-signal-{pid}")
}

/// Sends `byte` to process `pid`'s pipe (`signal_pipe`); false if nothing listens there. The pipe
/// busy with another launch's message is tried again for half a second.
pub fn signal(pid: u32, byte: u8) -> bool {
    for _ in 0..10 {
        match std::fs::OpenOptions::new().write(true).open(signal_pipe(pid)) {
            // Another account's pipe under that name: told nothing.
            Ok(pipe) if !pipe_server_is_mine(&pipe) => return false,
            Ok(mut pipe) => return pipe.write_all(&[byte]).is_ok(),
            // ERROR_PIPE_BUSY: its one instance is taken, the next comes in a moment.
            Err(e) if e.raw_os_error() == Some(231) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(_) => return false,
        }
    }
    false
}

/// This process's `signal_pipe`, heard until the end: each byte another launch sends (`signal`)
/// given to `on`. An instance per message (`ipc::Listener`).
pub async fn listen_signals(mut on: impl FnMut(u8)) {
    use tokio::io::AsyncReadExt;
    let name = signal_pipe(std::process::id());
    let mut listener = match crate::ipc::listen(Path::new(&name), Path::new(""), crate::ipc::Taken::Refuse) {
        Ok(l) => l,
        Err(e) => return tracing::warn!("no pipe {name}: {e}"),
    };
    loop {
        let mut connected = match listener.accept().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("{name}: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };
        let mut byte = [0u8; 1];
        if tokio::time::timeout(std::time::Duration::from_secs(2), connected.read_exact(&mut byte)).await.is_ok_and(|r| r.is_ok()) {
            on(byte[0]);
        }
    }
}

/// The calling thread keeps the computer from sleeping (`on`), or no longer. Windows holds it as
/// long as the thread lives, or until it says otherwise: call it from a thread kept for it.
pub fn stay_awake(on: bool) -> bool {
    let flags = if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS };
    // SAFETY: flags only.
    unsafe { SetThreadExecutionState(flags) != 0 }
}

/// This account's SID, `S-1-5-21-…`.
fn user_sid() -> io::Result<String> {
    // SAFETY: the process's own pseudo handle, never closed.
    process_sid(unsafe { GetCurrentProcess() })
}

/// The SID of the account process `process` runs as.
fn process_sid(process: HANDLE) -> io::Result<String> {
    let mut token: HANDLE = null_mut();
    // SAFETY: a process handle the caller holds; `token` ours to write, closed by `Handle`.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Handle(token);
    let mut len = 0u32;
    // SAFETY: a size query: no buffer, `len` set to what one needs (the call "fails" so).
    unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut len) };
    // u64s: a buffer aligned for TOKEN_USER's pointer.
    let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
    // SAFETY: `buf` holds at least `len` bytes.
    if unsafe { GetTokenInformation(token.0, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: Windows wrote a TOKEN_USER at the start of `buf`, aligned for it; its SID lies in `buf` too.
    let user = unsafe { &*buf.as_ptr().cast::<TOKEN_USER>() };
    let mut text: *mut u16 = null_mut();
    // SAFETY: a SID inside `buf`, alive; `text` freed by `Local`.
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let text = Local(text.cast());
    // SAFETY: a NUL-terminated string Windows wrote, alive until `text` drops.
    Ok(unsafe { read_wide(text.0.cast()) })
}

/// The access list of `dir` as SDDL (`D:PAI(A;OICI;FA;;;S-1-5-…)…`).
fn dacl_of(dir: &[u16]) -> io::Result<String> {
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: a NUL-terminated path; only the descriptor asked for, freed by `Local`.
    let err = unsafe { GetNamedSecurityInfoW(dir.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), null_mut(), null_mut(), &mut sd) };
    if err != 0 {
        return Err(io::Error::from_raw_os_error(err as i32));
    }
    let sd = Local(sd);
    let mut text: *mut u16 = null_mut();
    // SAFETY: a descriptor alive in `sd`; `text` freed by `Local`.
    if unsafe { ConvertSecurityDescriptorToStringSecurityDescriptorW(sd.0, SDDL_REVISION_1, DACL_SECURITY_INFORMATION, &mut text, null_mut()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let text = Local(text.cast());
    // SAFETY: a NUL-terminated string Windows wrote, alive until `text` drops.
    Ok(unsafe { read_wide(text.0.cast()) })
}

/// Whether an access list (SDDL) is already `private_dir`'s: protected from its parents, and every
/// entry this account's or the system's.
fn is_private(sddl: &str, sid: &str) -> bool {
    let Some(rest) = sddl.strip_prefix("D:") else { return false };
    let (flags, aces) = rest.split_at(rest.find('(').unwrap_or(rest.len()));
    flags.contains('P') && !aces.is_empty() && aces.trim_matches(|c| c == '(' || c == ')').split(")(").all(|ace| ace.starts_with("A;") && ace.rsplit(';').next().is_some_and(|who| who == sid || who == "SY"))
}

/// `dir` (and what it holds, by inheritance) this account's only, with the system's: Unix's 0700.
/// Its parents' entries (Administrators, other accounts) no longer apply. Left as it is when
/// already so: Windows walks every file below to apply a new list.
pub fn private_dir(dir: &Path) -> io::Result<()> {
    let sid = user_sid()?;
    let path = wide(dir.as_os_str());
    if dacl_of(&path).is_ok_and(|d| is_private(&d, &sid)) {
        return Ok(());
    }
    let sddl = wide(OsStr::new(&format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)")));
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: a NUL-terminated string; the descriptor freed by `Local`.
    if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let sd = Local(sd);
    let (mut present, mut defaulted) = (0, 0);
    let mut dacl: *mut ACL = null_mut();
    // SAFETY: a descriptor alive in `sd`; `dacl` points into it, used while it lives.
    if unsafe { GetSecurityDescriptorDacl(sd.0, &mut present, &mut dacl, &mut defaulted) } == 0 || present == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a NUL-terminated path; `dacl` alive in `sd`; no owner, group nor SACL changed.
    let err = unsafe { SetNamedSecurityInfoW(path.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, null_mut(), null_mut(), dacl, null()) };
    if err != 0 {
        return Err(io::Error::from_raw_os_error(err as i32));
    }
    Ok(())
}

/// A named pipe's first or next instance, this account's only (and the system's): no other account
/// of this computer opens it, nor any of another computer. `first`: the name claimed — it fails if
/// some process made it already (another account squatting it).
pub fn pipe_server(name: &str, first: bool) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    let sddl = wide(OsStr::new(&format!("D:P(A;;GA;;;{})(A;;GA;;;SY)", user_sid()?)));
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: a NUL-terminated string; the descriptor freed by `Local`.
    if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let sd = Local(sd);
    let mut attrs = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd.0, bInheritHandle: 0 };
    let mut options = tokio::net::windows::named_pipe::ServerOptions::new();
    options.first_pipe_instance(first).reject_remote_clients(true);
    // SAFETY: `attrs` a SECURITY_ATTRIBUTES whose descriptor lives in `sd`, both alive through the call.
    unsafe { options.create_with_security_attributes_raw(name, (&mut attrs as *mut SECURITY_ATTRIBUTES).cast()) }
}

/// Whether the server end of pipe `client` (a handle of ours) runs as this account: a pipe another
/// account made first under our name is never spoken to. A server of this account started as
/// administrator (elevated) is refused too: its token cannot be read from a normal process, and
/// Prism is not meant to run elevated (`prism mcp`, `prism sessions`, `prism restart` then fail).
pub fn pipe_server_is_mine(client: &impl std::os::windows::io::AsRawHandle) -> bool {
    let mut pid = 0u32;
    // SAFETY: a pipe handle the caller holds, alive through the call; `pid` ours to write.
    if unsafe { GetNamedPipeServerProcessId(client.as_raw_handle() as HANDLE, &mut pid) } == 0 {
        return false;
    }
    // SAFETY: an id; the handle closed by `Handle`.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let process = Handle(process);
    matches!((process_sid(process.0), user_sid()), (Ok(a), Ok(b)) if a == b)
}

/// `C:\` of `C:\Users\me\x`.
fn root_of(path: &Path) -> PathBuf {
    let mut root = PathBuf::new();
    for c in path.components() {
        root.push(c);
        if matches!(c, Component::RootDir) {
            break;
        }
    }
    root
}

/// `path` (absolute) to the Recycle Bin, as Explorer's Delete would: it can be put back. Refused
/// on a disk without one (a network share, a USB stick), where Windows would delete it for good;
/// too large for the Bin, Windows asks first.
pub fn recycle(path: &Path) -> io::Result<()> {
    let root = wide(root_of(path).as_os_str());
    // DRIVE_FIXED: an internal disk, which has a Bin.
    // SAFETY: a NUL-terminated path.
    if unsafe { GetDriveTypeW(root.as_ptr()) } != 3 {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "no Recycle Bin on this disk"));
    }
    // A list of paths, each NUL-terminated, the list ended by another NUL.
    let mut from = wide(path.as_os_str());
    from.push(0);
    let mut op = SHFILEOPSTRUCTW {
        hwnd: null_mut(),
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        pTo: null(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_WANTNUKEWARNING | FOF_NOERRORUI | FOF_SILENT) as u16,
        fAnyOperationsAborted: 0,
        hNameMappings: null_mut(),
        lpszProgressTitle: null(),
    };
    // SAFETY: `from` (double NUL-terminated) outlives the call; no other pointer given.
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 {
        return Err(io::Error::other(format!("SHFileOperation: error {code:#x}")));
    }
    if op.fAnyOperationsAborted != 0 {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    Ok(())
}

/// What the Recycle Bins of every disk hold: bytes and items.
pub fn recycle_bin() -> io::Result<(u64, u64)> {
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
    // SAFETY: no root (every disk); `info` ours, its size given.
    let hr = unsafe { SHQueryRecycleBinW(null(), &mut info) };
    if hr < 0 {
        return Err(io::Error::other(format!("SHQueryRecycleBin: {hr:#x}")));
    }
    Ok((info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64))
}

/// Every disk's Recycle Bin emptied, without Explorer's question nor sound.
pub fn empty_recycle_bin() -> io::Result<()> {
    // SAFETY: no window, no root (every disk).
    let hr = unsafe { SHEmptyRecycleBinW(null_mut(), null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND) };
    // E_UNEXPECTED: already empty.
    if hr < 0 && hr as u32 != 0x8000_FFFF {
        return Err(io::Error::other(format!("SHEmptyRecycleBin: {hr:#x}")));
    }
    Ok(())
}

/// The person at this computer confirms `reason` with Windows Hello: a face, a finger or the PIN.
/// Its dialog sits on window `hwnd` (one of this process's, the app's main thread running);
/// without one, on its own. Blocking, until answered. False: refused, cancelled, or no Windows
/// Hello set up (said in the log).
pub fn confirm(hwnd: Option<isize>, reason: &str) -> bool {
    match hello(hwnd, reason) {
        Ok(yes) => yes,
        Err(e) => {
            tracing::warn!("Windows Hello: {e}");
            false
        }
    }
}

fn hello(hwnd: Option<isize>, reason: &str) -> ::windows::core::Result<bool> {
    use ::windows::core::{factory, HSTRING};
    use ::windows::Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability};
    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    use ::windows_future::IAsyncOperation;

    let available = UserConsentVerifier::CheckAvailabilityAsync()?.join()?;
    if available != UserConsentVerifierAvailability::Available {
        tracing::warn!("Windows Hello is not available ({}): set it up in Settings › Accounts › Sign-in options", available.0);
        return Ok(false);
    }
    let message = HSTRING::from(reason);
    let asked: IAsyncOperation<UserConsentVerificationResult> = match hwnd {
        Some(h) => {
            let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()?;
            // SAFETY: `h` is a window of this process, alive while its dialog shows (the caller's).
            unsafe { interop.RequestVerificationForWindowAsync(HWND(h as *mut c_void), &message)? }
        }
        None => UserConsentVerifier::RequestVerificationAsync(&message)?,
    };
    Ok(asked.join()? == UserConsentVerificationResult::Verified)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_a_private_folder() {
        let me = "S-1-5-21-1-2-3-1001";
        assert!(is_private(&format!("D:PAI(A;OICI;FA;;;{me})(A;OICI;FA;;;SY)"), me));
        assert!(!is_private(&format!("D:AI(A;OICI;FA;;;{me})(A;OICI;FA;;;SY)"), me), "inherits its parents'");
        assert!(!is_private(&format!("D:PAI(A;OICI;FA;;;{me})(A;OICI;FA;;;BA)"), me), "Administrators");
        assert!(!is_private("D:P", me), "nobody");
    }

    #[test]
    fn a_path_s_disk() {
        assert_eq!(root_of(Path::new(r"C:\Users\me\x")), PathBuf::from(r"C:\"));
    }
}
