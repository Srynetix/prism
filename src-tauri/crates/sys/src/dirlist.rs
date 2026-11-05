//! A folder's entries with their size on the disk, read in bulk (`getattrlistbulk`): one call
//! answers many entries with their attributes, where `readdir` then one `lstat` per entry costs a
//! call each. What the Disk usage block's scan reads on macOS.

use std::{ffi::OsString, os::fd::AsRawFd, os::unix::ffi::OsStringExt, os::unix::fs::OpenOptionsExt, path::Path};

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    File,
    Dir,
    Link,
    Other,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: OsString,
    pub kind: Type,
    /// As `st_dev`, `st_ino`, `st_nlink` read them (files only for the links).
    pub dev: u64,
    pub ino: u64,
    pub links: u32,
    /// Bytes on the disk (`st_blocks × 512`); 0 for a folder.
    pub alloc: u64,
    /// A folder another volume is mounted on: its `dev` is still the folder's (the volume under
    /// it), not the mounted one's as `lstat` would say.
    pub mount_point: bool,
    /// Its content is in the cloud (iCloud Drive, OneDrive… evicted): reading it would download it.
    pub dataless: bool,
}

/// `ATTR_CMN_ERROR` (`sys/attr.h`, not in `libc`): an entry whose attributes cannot be read says
/// so in its own record, instead of failing the whole call (and the whole folder with it).
const ATTR_CMN_ERROR: libc::attrgroup_t = 0x2000_0000;

/// `SF_DATALESS` (`sys/stat.h`).
const DATALESS: u32 = 0x4000_0000;

const COMMON: libc::attrgroup_t = libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_NAME | libc::ATTR_CMN_DEVID | libc::ATTR_CMN_OBJTYPE | libc::ATTR_CMN_FLAGS | libc::ATTR_CMN_FILEID | ATTR_CMN_ERROR;
const DIR: libc::attrgroup_t = libc::ATTR_DIR_MOUNTSTATUS;
const FILE: libc::attrgroup_t = libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE;

/// The entries of folder `path` (`.` and `..` never among them).
pub fn read(path: &Path) -> std::io::Result<Vec<Entry>> {
    // As `opendir` opens it: never a FIFO or a device waiting for a writer.
    let dir = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NONBLOCK | libc::O_CLOEXEC).open(path)?;
    let mut list = libc::attrlist { bitmapcount: libc::ATTR_BIT_MAP_COUNT, reserved: 0, commonattr: COMMON, volattr: 0, dirattr: DIR, fileattr: FILE, forkattr: 0 };
    BUF.with_borrow_mut(|buf| read_into(&dir, &mut list, buf))
}

thread_local! {
    /// Each thread's buffer, reused from folder to folder.
    static BUF: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; 128 * 1024]);
}

fn read_into(dir: &std::fs::File, list: &mut libc::attrlist, buf: &mut [u8]) -> std::io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    loop {
        // SAFETY: `dir` is an open folder for the call's length; `list` and `buf` are live, `buf`'s
        // length is the size given; the kernel writes at most that many bytes into it.
        let n = unsafe { libc::getattrlistbulk(dir.as_raw_fd(), std::ptr::from_mut(list).cast(), buf.as_mut_ptr().cast(), buf.len(), 0) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n == 0 {
            return Ok(out);
        }
        let mut at = 0usize;
        for _ in 0..n {
            let len = read_u32(buf, at) as usize;
            if len == 0 || at + len > buf.len() {
                break;
            }
            if let Some(e) = entry(&buf[at..at + len]) {
                out.push(e);
            }
            at += len;
        }
    }
}

fn read_u32(buf: &[u8], at: usize) -> u32 {
    buf.get(at..at + 4).map_or(0, |b| u32::from_ne_bytes(b.try_into().unwrap_or_default()))
}

fn read_i32(buf: &[u8], at: usize) -> i32 {
    read_u32(buf, at) as i32
}

fn read_u64(buf: &[u8], at: usize) -> u64 {
    buf.get(at..at + 8).map_or(0, |b| u64::from_ne_bytes(b.try_into().unwrap_or_default()))
}

/// One entry of the buffer: its length, the attributes returned, then each of them in the order
/// of their bits (common, folder, then file), those not returned left out.
fn entry(e: &[u8]) -> Option<Entry> {
    // length (4), then attribute_set_t (5 × 4).
    let common = read_u32(e, 4);
    let dir = read_u32(e, 12);
    let file = read_u32(e, 16);
    let mut at = 24;
    let mut name = None;
    if common & libc::ATTR_CMN_NAME != 0 {
        // attrreference_t: offset from itself, length with the NUL.
        let off = read_i32(e, at) as usize;
        let len = read_u32(e, at + 4) as usize;
        let start = at + off;
        let bytes = e.get(start..start + len.saturating_sub(1))?;
        name = Some(OsString::from_vec(bytes.to_vec()));
        at += 8;
    }
    let mut dev = 0;
    if common & libc::ATTR_CMN_DEVID != 0 {
        // dev_t, signed, as `MetadataExt::dev` widens it.
        dev = read_i32(e, at) as u64;
        at += 4;
    }
    let mut kind = Type::Other;
    if common & libc::ATTR_CMN_OBJTYPE != 0 {
        kind = match read_u32(e, at) {
            1 => Type::File,
            2 => Type::Dir,
            5 => Type::Link,
            _ => Type::Other,
        };
        at += 4;
    }
    let mut dataless = false;
    if common & libc::ATTR_CMN_FLAGS != 0 {
        dataless = read_u32(e, at) & DATALESS != 0;
        at += 4;
    }
    let mut ino = 0;
    if common & libc::ATTR_CMN_FILEID != 0 {
        ino = read_u64(e, at);
        at += 8;
    }
    // Its attributes could not be read: left out, as `readdir` + `lstat` would.
    if common & ATTR_CMN_ERROR != 0 {
        if read_u32(e, at) != 0 {
            return None;
        }
        at += 4;
    }
    let mut mount_point = false;
    if dir & libc::ATTR_DIR_MOUNTSTATUS != 0 {
        mount_point = read_u32(e, at) & libc::DIR_MNTSTATUS_MNTPOINT != 0;
        at += 4;
    }
    let mut links = 1;
    if file & libc::ATTR_FILE_LINKCOUNT != 0 {
        links = read_u32(e, at);
        at += 4;
    }
    let mut alloc = 0;
    if file & libc::ATTR_FILE_ALLOCSIZE != 0 {
        alloc = read_u64(e, at);
    }
    Some(Entry { name: name?, kind, dev, ino, links, alloc, mount_point, dataless })
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;

    use super::*;

    #[test]
    fn agrees_with_lstat() {
        let dir = std::env::temp_dir().join(format!("prism-dirlist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("é file"), vec![3u8; 100_000]).unwrap();
        std::fs::hard_link(dir.join("é file"), dir.join("link")).unwrap();
        std::os::unix::fs::symlink("sub", dir.join("sym")).unwrap();
        // A FIFO is listed, and reading a FIFO as a folder fails at once.
        assert_eq!(std::process::Command::new("mkfifo").arg(dir.join("sub/fifo")).status().unwrap().code(), Some(0));
        assert_eq!(read(&dir.join("sub")).unwrap()[0].kind, Type::Other);
        assert!(read(&dir.join("sub/fifo")).is_err());
        let mut entries = read(&dir).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<_> = entries.iter().map(|e| e.name.to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["link", "sub", "sym", "é file"]);
        for e in &entries {
            let m = std::fs::symlink_metadata(dir.join(&e.name)).unwrap();
            assert_eq!((e.dev, e.ino), (m.dev(), m.ino()), "{:?}", e.name);
            if e.kind == Type::File {
                assert_eq!((e.alloc, e.links as u64), (m.blocks() * 512, m.nlink()), "{:?}", e.name);
            }
        }
        assert_eq!(entries.iter().map(|e| e.kind).collect::<Vec<_>>(), [Type::File, Type::Dir, Type::Link, Type::File]);
        assert!(entries.iter().all(|e| !e.mount_point && !e.dataless));
        // `/Volumes`' entries are the volumes mounted (the system's own at least, through its link).
        if let Ok(volumes) = read(Path::new("/System/Volumes")) {
            assert!(volumes.iter().any(|e| e.mount_point), "{volumes:?}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
