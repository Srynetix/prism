//! The local connections between Prism's own processes: `prism mcp` to the running Prism,
//! `prism sessions` to its clients, the app's window to a profile served without one, a launch to
//! the running app (Windows). A Unix socket in a folder of this account's only (0700, the socket
//! 0600); on Windows a named pipe this account's only (`windows::pipe_server`). Either end checks
//! the other is this account's: nothing is said to, nor heard from, another's.
//!
//! An endpoint is a path: the socket's, or the pipe's name (`\\.\pipe\prism-…`). `home` is the
//! folder Prism's data sits in (the account's, private already): a socket's folder below it is not
//! asked to be 0700, one elsewhere (`/tmp`, `$XDG_RUNTIME_DIR`) is.

use std::io;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::time::Duration;

/// A connection a `Listener` accepted.
#[cfg(unix)]
pub type Server = tokio::net::UnixStream;
#[cfg(windows)]
pub type Server = tokio::net::windows::named_pipe::NamedPipeServer;

/// A connection `connect` opened.
#[cfg(unix)]
pub type Client = tokio::net::UnixStream;
#[cfg(windows)]
pub type Client = tokio::net::windows::named_pipe::NamedPipeClient;

/// A connection `connect_blocking` opened, for code without a runtime.
#[cfg(unix)]
pub type BlockingClient = std::os::unix::net::UnixStream;
#[cfg(windows)]
pub type BlockingClient = std::fs::File;

/// What `listen` does with an endpoint another process may hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taken {
    /// Another Prism answering there keeps it (`io::ErrorKind::AddrInUse`); a socket left by one
    /// gone is replaced.
    Refuse,
    /// Unix: the socket replaced whatever it is (one Prism per profile: it is a former one's).
    /// Windows' pipe cannot be taken over: as `Refuse`.
    Replace,
}

/// The socket's folder, made if missing (0700), and refused unless it is a real folder of this
/// account's that no one else may enter (outside `home`): another account cannot have put it there
/// first.
#[cfg(unix)]
pub fn private_dir(socket: &Path, home: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let dir = socket.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{}: no folder", socket.display())))?;
    if !dir.exists() {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    let meta = std::fs::symlink_metadata(dir)?;
    // The profile's data folder sits in the home (private already); the fallback in a shared one.
    let shared = !dir.starts_with(home);
    if !(meta.is_dir() && meta.uid() == crate::process::uid() && (!shared || meta.mode() & 0o077 == 0)) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{}: not a private folder of this account", dir.display())));
    }
    Ok(())
}

/// `path` written and read by the owner only (0600).
#[cfg(unix)]
pub fn owner_only(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| io::Error::new(e.kind(), format!("{}: permissions: {e}", path.display())))
}

/// The socket at `path` is this account's, in a private folder: never talked to otherwise.
#[cfg(unix)]
pub fn owned_socket(path: &Path, home: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    private_dir(path, home)?;
    let meta = std::fs::symlink_metadata(path).map_err(|e| io::Error::new(e.kind(), format!("no {}: {e}", path.display())))?;
    if meta.uid() != crate::process::uid() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{}: another account's socket", path.display())));
    }
    Ok(())
}

/// An endpoint listened on: `accept` gives each connection.
pub struct Listener {
    at: PathBuf,
    /// Bound without a runtime (`listen`), made the runtime's at the first `accept`.
    #[cfg(unix)]
    std: Option<std::os::unix::net::UnixListener>,
    #[cfg(unix)]
    tokio: Option<tokio::net::UnixListener>,
    /// The instance the next client opens: there is always one, so the name stays ours.
    #[cfg(windows)]
    next: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
}

/// Listens on `at`. Unix: the socket bound in its private folder (`private_dir`), 0600, needing no
/// runtime yet. Windows: the pipe's first instance, claiming the name (it fails if another process
/// made it first: another account squatting it, or another Prism): call it inside the runtime.
pub fn listen(at: &Path, home: &Path, taken: Taken) -> io::Result<Listener> {
    #[cfg(unix)]
    {
        private_dir(at, home)?;
        if taken == Taken::Refuse && std::os::unix::net::UnixStream::connect(at).is_ok() {
            return Err(io::Error::new(io::ErrorKind::AddrInUse, format!("{}: another Prism listens there", at.display())));
        }
        let _ = std::fs::remove_file(at);
        let std = std::os::unix::net::UnixListener::bind(at).and_then(|l| l.set_nonblocking(true).map(|()| l)).map_err(|e| io::Error::new(e.kind(), format!("binding {}: {e}", at.display())))?;
        if let Err(e) = owner_only(at) {
            let _ = std::fs::remove_file(at);
            return Err(e);
        }
        Ok(Listener { at: at.to_path_buf(), std: Some(std), tokio: None })
    }
    #[cfg(windows)]
    {
        let _ = (home, taken);
        let first = crate::windows::pipe_server(&name(at)?, true).map_err(|e| io::Error::new(if e.kind() == io::ErrorKind::PermissionDenied { io::ErrorKind::AddrInUse } else { e.kind() }, format!("{}: {e} (another Prism listens there?)", at.display())))?;
        Ok(Listener { at: at.to_path_buf(), next: Some(first) })
    }
}

/// A pipe's name as Windows takes it.
#[cfg(windows)]
fn name(at: &Path) -> io::Result<String> {
    at.to_str().map(String::from).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{}: not a pipe's name", at.display())))
}

impl Listener {
    /// Where it listens.
    pub fn at(&self) -> &Path {
        &self.at
    }

    /// The next connection. An error leaves the listener usable: the caller waits a moment and
    /// calls again.
    #[cfg(unix)]
    pub async fn accept(&mut self) -> io::Result<Server> {
        if self.tokio.is_none() {
            let std = self.std.take().ok_or_else(|| io::Error::other("no listener"))?;
            self.tokio = Some(tokio::net::UnixListener::from_std(std)?);
        }
        let listener = self.tokio.as_ref().ok_or_else(|| io::Error::other("no listener"))?;
        Ok(listener.accept().await?.0)
    }

    /// The next connection: an instance per client, the next one made before the connected one is
    /// handed over (no moment without one, when another process could claim the name).
    #[cfg(windows)]
    pub async fn accept(&mut self) -> io::Result<Server> {
        let name = name(&self.at)?;
        let server = match self.next.take() {
            Some(s) => s,
            None => crate::windows::pipe_server(&name, false)?,
        };
        let connected = server.connect().await;
        // Made before the connected one (or the failed one) goes.
        self.next = crate::windows::pipe_server(&name, false).ok();
        match connected {
            Ok(()) => Ok(server),
            Err(e) => {
                // Kept for the next client only when no new one could be made: reset first.
                if self.next.is_none() && server.disconnect().is_ok() {
                    self.next = Some(server);
                }
                Err(e)
            }
        }
    }
}

/// `ERROR_PIPE_BUSY`: all a pipe's instances taken an instant.
#[cfg(windows)]
const BUSY: i32 = 231;

/// Busy, tried again this often, this many times (2.5 s).
#[cfg(windows)]
const BUSY_WAIT: Duration = Duration::from_millis(50);
#[cfg(windows)]
const BUSY_TRIES: u32 = 50;

/// Connects to `at`, this account's only (`owned_socket`; Windows: the pipe's server runs as this
/// account, `windows::pipe_server_is_mine`), else `PermissionDenied`. Nothing there: `NotFound`
/// or `ConnectionRefused`, as the system says it.
pub async fn connect(at: &Path, home: &Path) -> io::Result<Client> {
    #[cfg(unix)]
    {
        if at.exists() {
            owned_socket(at, home)?;
        }
        tokio::net::UnixStream::connect(at).await
    }
    #[cfg(windows)]
    {
        let _ = home;
        let name = name(at)?;
        let mut tries = 0;
        loop {
            match tokio::net::windows::named_pipe::ClientOptions::new().open(&name) {
                Err(e) if e.raw_os_error() == Some(BUSY) && tries < BUSY_TRIES => tries += 1,
                Ok(pipe) => return mine(pipe, &name),
                Err(e) => return Err(e),
            }
            tokio::time::sleep(BUSY_WAIT).await;
        }
    }
}

/// `connect`, without a runtime.
pub fn connect_blocking(at: &Path, home: &Path) -> io::Result<BlockingClient> {
    #[cfg(unix)]
    {
        if at.exists() {
            owned_socket(at, home)?;
        }
        std::os::unix::net::UnixStream::connect(at)
    }
    #[cfg(windows)]
    {
        let _ = home;
        let mut tries = 0;
        loop {
            match std::fs::OpenOptions::new().read(true).write(true).open(at) {
                Err(e) if e.raw_os_error() == Some(BUSY) && tries < BUSY_TRIES => tries += 1,
                Ok(pipe) => return mine(pipe, &at.display().to_string()),
                Err(e) => return Err(e),
            }
            std::thread::sleep(BUSY_WAIT);
        }
    }
}

/// `pipe`, unless another account made it first under our name.
#[cfg(windows)]
fn mine<P: std::os::windows::io::AsRawHandle>(pipe: P, name: &str) -> io::Result<P> {
    if crate::windows::pipe_server_is_mine(&pipe) {
        Ok(pipe)
    } else {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{name}: not this account's")))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A socket served in its private folder (0700, itself 0600), reached by `connect`; a second
    /// listener refused while the first answers, `Replace` taking it over.
    #[tokio::test]
    async fn a_socket_is_served_reached_and_kept() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("prism-ipc-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let at = dir.join("x.sock");
        let home = Path::new("/nonexistent-home");
        let mut listener = listen(&at, home, Taken::Refuse).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(std::fs::metadata(&at).unwrap().permissions().mode() & 0o777, 0o600);
        let served = tokio::spawn(async move {
            let mut s = listener.accept().await.unwrap();
            let mut b = [0u8; 2];
            s.read_exact(&mut b).await.unwrap();
            s.write_all(&b).await.unwrap();
            listener
        });
        let mut c = connect(&at, home).await.unwrap();
        c.write_all(b"hi").await.unwrap();
        let mut b = [0u8; 2];
        c.read_exact(&mut b).await.unwrap();
        assert_eq!(&b, b"hi");
        let listener = served.await.unwrap();
        assert_eq!(listen(&at, home, Taken::Refuse).err().map(|e| e.kind()), Some(io::ErrorKind::AddrInUse));
        assert!(listen(&at, home, Taken::Replace).is_ok());
        drop(listener);
        // A folder others may enter: refused.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(connect_blocking(&at, home).err().map(|e| e.kind()), Some(io::ErrorKind::PermissionDenied));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
