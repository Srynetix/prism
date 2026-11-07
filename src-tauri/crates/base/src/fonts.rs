//! The fonts the user installed, for the webview. macOS's WebKit hides them from web content
//! (only the system's are visible): Prism finds the files of a family in the fonts folders, by
//! the names written in them, and hands them over for the page to load (`FontFace`).

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use serde::Serialize;

/// One face of a family: what `@font-face` needs to tell it from the others.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Face {
    /// Its number in the family's list, to ask for its file (`font_face`).
    pub index: usize,
    pub weight: u16,
    pub italic: bool,
    #[serde(skip)]
    path: PathBuf,
}

/// The faces of `family` (its name, any case) among the installed fonts; none on Windows, whose
/// webview sees them already. Found once per family.
pub fn faces(family: &str) -> Vec<Face> {
    static FOUND: OnceLock<Mutex<HashMap<String, Vec<Face>>>> = OnceLock::new();
    let key = family.trim().to_lowercase();
    let mut found = FOUND.get_or_init(Mutex::default).lock().unwrap();
    if let Some(f) = found.get(&key) {
        return f.clone();
    }
    let mut list: Vec<Face> = Vec::new();
    for path in folders().iter().flat_map(|d| files(d)) {
        if let Some((names, weight, italic)) = read(&path)
            && names.iter().any(|n| n.to_lowercase() == key)
            && !list.iter().any(|f| f.weight == weight && f.italic == italic)
        {
            list.push(Face { index: list.len(), weight, italic, path });
        }
    }
    found.insert(key, list.clone());
    list
}

/// The file of one face, as returned by `faces`.
pub fn file(family: &str, index: usize) -> anyhow::Result<Vec<u8>> {
    let face = faces(family).into_iter().nth(index).ok_or_else(|| anyhow::anyhow!("no face {index} for {family}"))?;
    Ok(std::fs::read(face.path)?)
}

fn folders() -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        out.extend(home.map(|h| h.join("Library/Fonts")));
        out.push("/Library/Fonts".into());
    } else if cfg!(target_os = "linux") {
        out.extend(std::env::var_os("XDG_DATA_HOME").map(|d| PathBuf::from(d).join("fonts")));
        out.extend(home.iter().flat_map(|h| [h.join(".local/share/fonts"), h.join(".fonts")]));
        out.extend(["/usr/local/share/fonts", "/usr/share/fonts"].map(PathBuf::from));
    }
    out
}

/// The font files of a folder and its subfolders (font managers sort them by family).
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for e in entries.flatten() {
        let path = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            out.extend(files(&path));
        } else if path.extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x.to_lowercase().as_str(), "ttf" | "otf")) {
            out.push(path);
        }
    }
    out
}

/// A font file's family names (`name` table: family, typographic family), weight and slant
/// (`OS/2`). Only the tables it needs are read.
fn read(path: &Path) -> Option<(Vec<String>, u16, bool)> {
    let mut f = File::open(path).ok()?;
    let mut head = [0u8; 12];
    f.read_exact(&mut head).ok()?;
    let tables = u16::from_be_bytes([head[4], head[5]]) as usize;
    let mut dir = vec![0u8; tables * 16];
    f.read_exact(&mut dir).ok()?;
    let mut table = |tag: &[u8]| -> Option<Vec<u8>> {
        let rec = dir.chunks(16).find(|r| &r[..4] == tag)?;
        let (offset, len) = (be32(rec, 8)? as u64, be32(rec, 12)? as usize);
        let mut buf = vec![0u8; len.min(1 << 20)];
        f.seek(SeekFrom::Start(offset)).ok()?;
        f.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    let names = family_names(&table(b"name")?);
    let (weight, italic) = table(b"OS/2").and_then(|t| Some((be16(&t, 4)?, be16(&t, 62)? & 1 == 1))).unwrap_or((400, false));
    (!names.is_empty()).then_some((names, weight, italic))
}

/// Name ids 1 (family) and 16 (typographic family), in their Unicode or Mac Roman records.
fn family_names(t: &[u8]) -> Vec<String> {
    let (Some(count), Some(base)) = (be16(t, 2), be16(t, 4)) else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for i in 0..count as usize {
        let r = 6 + i * 12;
        let (Some(platform), Some(id), Some(len), Some(off)) = (be16(t, r), be16(t, r + 6), be16(t, r + 8), be16(t, r + 10)) else { break };
        if id != 1 && id != 16 {
            continue;
        }
        let start = base as usize + off as usize;
        let Some(bytes) = t.get(start..start + len as usize) else { continue };
        let name = match platform {
            0 | 3 => String::from_utf16_lossy(&bytes.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect::<Vec<_>>()),
            1 => bytes.iter().map(|&b| b as char).collect(),
            _ => continue,
        };
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal font file: a `name` table with a family in UTF-16 and Mac Roman, an `OS/2` table.
    fn font(family: &str, weight: u16, italic: bool) -> Vec<u8> {
        let utf16: Vec<u8> = family.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        let roman = family.as_bytes();
        let mut name = Vec::new();
        name.extend([0u16, 2, 6 + 2 * 12].iter().flat_map(|v| v.to_be_bytes()));
        name.extend([3u16, 1, 0x409, 1, utf16.len() as u16, 0].iter().flat_map(|v| v.to_be_bytes()));
        name.extend([1u16, 0, 0, 16, roman.len() as u16, utf16.len() as u16].iter().flat_map(|v| v.to_be_bytes()));
        name.extend(&utf16);
        name.extend(roman);
        let mut os2 = vec![0u8; 64];
        os2[4..6].copy_from_slice(&weight.to_be_bytes());
        os2[62..64].copy_from_slice(&(italic as u16).to_be_bytes());
        let mut out = vec![0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0];
        let start = 12 + 2 * 16;
        for (tag, offset, len) in [(b"OS/2", start, os2.len()), (b"name", start + os2.len(), name.len())] {
            out.extend(tag);
            out.extend([0u8; 4]);
            out.extend((offset as u32).to_be_bytes());
            out.extend((len as u32).to_be_bytes());
        }
        out.extend(os2);
        out.extend(name);
        out
    }

    #[test]
    fn reads_family_weight_and_slant() {
        let dir = std::env::temp_dir().join(format!("prism-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Rec-BoldItalic.ttf");
        std::fs::write(&path, font("RecMonoCasual Nerd Font Mono", 700, true)).unwrap();
        let (names, weight, italic) = read(&path).unwrap();
        assert_eq!((names, weight, italic), (vec!["RecMonoCasual Nerd Font Mono".to_string()], 700, true));
        assert_eq!(files(&dir), [path]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_what_is_not_a_font() {
        let dir = std::env::temp_dir().join(format!("prism-fonts-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("broken.ttf");
        std::fs::write(&path, b"not a font at all").unwrap();
        assert_eq!(read(&path), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
