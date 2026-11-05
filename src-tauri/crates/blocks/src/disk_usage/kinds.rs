//! What a file or a folder is, from its name and its siblings' (never its content): its kind, for
//! the colors, and whether it can be rebuilt (Reclaimable), and how.

use std::ffi::OsStr;

use serde::{Deserialize, Serialize};

/// What takes the space, for the colors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DiskKind"))]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Other,
    /// Build output and dependencies: `node_modules`, `target/`, `.venv`, DerivedData.
    Build,
    Cache,
    /// Virtual machines and disk images: Docker's, the simulators', `.qcow2`.
    Vm,
    Media,
    /// Machine-learning models: `.gguf`, `.safetensors`, Ollama's.
    Models,
    Apps,
    Docs,
    /// Archives and installers: `.zip`, `.dmg`, `.iso`.
    Archives,
    /// Source code and repositories' history.
    Code,
}

impl Kind {
    pub const ALL: [Kind; 10] = [Kind::Other, Kind::Build, Kind::Cache, Kind::Vm, Kind::Media, Kind::Models, Kind::Apps, Kind::Docs, Kind::Archives, Kind::Code];

    pub fn code(self) -> u8 {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0) as u8
    }

    pub fn from_code(c: u8) -> Self {
        Self::ALL.get(c as usize).copied().unwrap_or_default()
    }
}

/// Rebuildable data, and how it is freed (`clean`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DiskReclaim"))]
#[serde(rename_all = "snake_case")]
pub enum Reclaim {
    /// Each folder removed; the package manager installs them again.
    NodeModules,
    /// Cargo's `target/` (beside a `Cargo.toml`), removed: `cargo clean`.
    RustTarget,
    /// `.venv`, removed.
    PythonEnv,
    /// `.next`, `.nuxt`, `.svelte-kit`, `.turbo`, `.parcel-cache`, `.angular`: removed.
    JsBuild,
    /// `.terraform`, removed: `terraform init`.
    Terraform,
    /// CocoaPods' `Pods` (beside a `Podfile`), removed: `pod install`.
    Pods,
    /// Xcode's DerivedData, emptied.
    Xcode,
    /// The applications' and tools' caches (`~/Library/Caches`, `~/.cache`, npm's, Cargo's
    /// registry), emptied.
    Caches,
    /// The iOS simulators no Xcode can run any more: `xcrun simctl delete unavailable`.
    Simulators,
    /// Docker's disk image: `docker system prune -a` (unused images, stopped containers, build
    /// cache; volumes are kept).
    Docker,
    /// The Trash, emptied.
    Trash,
}

impl Reclaim {
    pub const ALL: [Reclaim; 11] = [Reclaim::NodeModules, Reclaim::RustTarget, Reclaim::PythonEnv, Reclaim::JsBuild, Reclaim::Terraform, Reclaim::Pods, Reclaim::Xcode, Reclaim::Caches, Reclaim::Simulators, Reclaim::Docker, Reclaim::Trash];

    /// 0: none.
    pub fn code(this: Option<Self>) -> u8 {
        this.and_then(|r| Self::ALL.iter().position(|x| *x == r)).map_or(0, |i| i as u8 + 1)
    }

    pub fn from_code(c: u8) -> Option<Self> {
        c.checked_sub(1).and_then(|i| Self::ALL.get(i as usize).copied())
    }

    /// The folder itself goes (otherwise: what is in it).
    pub fn removes_folder(self) -> bool {
        matches!(self, Reclaim::NodeModules | Reclaim::RustTarget | Reclaim::PythonEnv | Reclaim::JsBuild | Reclaim::Terraform | Reclaim::Pods)
    }
}

fn ext(name: &str) -> Option<String> {
    name.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()).map(|(_, e)| e.to_ascii_lowercase())
}

/// A file's kind, by its extension.
pub fn file_kind(name: &OsStr) -> Kind {
    let name = name.to_string_lossy();
    if name == "Docker.raw" {
        return Kind::Vm;
    }
    let Some(e) = ext(&name) else { return Kind::Other };
    match e.as_str() {
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "mp3" | "wav" | "flac" | "aac" | "m4a" | "aiff" | "ogg" | "jpg" | "jpeg" | "png" | "heic" | "gif" | "tiff" | "tif" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "psd" | "webp" => Kind::Media,
        "img" | "vmdk" | "qcow2" | "vdi" | "vhd" | "vhdx" | "hdd" | "sparseimage" | "ova" => Kind::Vm,
        "gguf" | "safetensors" | "ckpt" | "pt" | "pth" | "onnx" | "mlmodel" | "tflite" => Kind::Models,
        "zip" | "tar" | "gz" | "tgz" | "xz" | "bz2" | "7z" | "rar" | "zst" | "dmg" | "iso" | "pkg" | "deb" | "rpm" | "appimage" | "xip" => Kind::Archives,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "key" | "pages" | "numbers" | "odt" | "ods" | "odp" | "txt" | "md" | "csv" | "epub" | "json" | "parquet" | "sqlite" | "db" => Kind::Docs,
        "rs" | "ts" | "js" | "py" | "go" | "java" | "c" | "h" | "cpp" | "swift" | "kt" | "rb" | "svelte" | "vue" | "pack" | "idx" => Kind::Code,
        "a" | "o" | "rlib" | "rmeta" | "dylib" | "so" | "class" | "jar" | "wasm" => Kind::Build,
        _ => Kind::Other,
    }
}

/// What a folder is known to be, from its name, its parent's and its siblings' names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirClass {
    pub kind: Option<Kind>,
    pub reclaim: Option<Reclaim>,
}

/// `name` in folder `parent` (its name), beside `siblings`; `home`: `parent` is the home folder.
pub fn dir_class(name: &OsStr, parent: &OsStr, home: bool, has_sibling: impl Fn(&str) -> bool) -> DirClass {
    let name = name.to_string_lossy();
    let parent = parent.to_string_lossy();
    let of = |kind: Kind, reclaim: Option<Reclaim>| DirClass { kind: Some(kind), reclaim };
    match (name.as_ref(), parent.as_ref()) {
        ("node_modules", _) => of(Kind::Build, Some(Reclaim::NodeModules)),
        ("target", _) if has_sibling("Cargo.toml") => of(Kind::Build, Some(Reclaim::RustTarget)),
        (".venv", _) => of(Kind::Build, Some(Reclaim::PythonEnv)),
        ("venv", _) if has_sibling("pyproject.toml") || has_sibling("requirements.txt") || has_sibling("setup.py") => of(Kind::Build, Some(Reclaim::PythonEnv)),
        (".next" | ".nuxt" | ".svelte-kit" | ".turbo" | ".parcel-cache" | ".angular", _) => of(Kind::Build, Some(Reclaim::JsBuild)),
        (".terraform", _) => of(Kind::Build, Some(Reclaim::Terraform)),
        ("Pods", _) if has_sibling("Podfile") => of(Kind::Build, Some(Reclaim::Pods)),
        ("DerivedData", "Xcode") => of(Kind::Build, Some(Reclaim::Xcode)),
        ("Caches", "Library") | ("_cacache", ".npm") | ("registry", ".cargo") => of(Kind::Cache, Some(Reclaim::Caches)),
        (".cache", _) if home => of(Kind::Cache, Some(Reclaim::Caches)),
        ("Devices", "CoreSimulator") => of(Kind::Vm, Some(Reclaim::Simulators)),
        // Docker Desktop's disk image: macOS's container, Linux's VM folder.
        ("com.docker.docker", "Containers") | ("vms", "desktop") => of(Kind::Vm, Some(Reclaim::Docker)),
        (".Trash", _) if home => of(Kind::Other, Some(Reclaim::Trash)),
        ("Trash", "share") => of(Kind::Other, Some(Reclaim::Trash)),
        (".git", _) => of(Kind::Code, None),
        ("models", ".ollama") | ("huggingface", _) => of(Kind::Models, None),
        _ => match ext(&name).as_deref() {
            Some("app") => of(Kind::Apps, None),
            Some("photoslibrary") => of(Kind::Media, None),
            Some("sparsebundle" | "vmwarevm" | "pvm" | "utm") => of(Kind::Vm, None),
            _ => DirClass { kind: None, reclaim: None },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_by_extension() {
        assert_eq!(file_kind(OsStr::new("Docker.raw")), Kind::Vm);
        assert_eq!(file_kind(OsStr::new("clip.MOV")), Kind::Media);
        assert_eq!(file_kind(OsStr::new("llama.gguf")), Kind::Models);
        assert_eq!(file_kind(OsStr::new("x.dmg")), Kind::Archives);
        assert_eq!(file_kind(OsStr::new(".zshrc")), Kind::Other);
        assert_eq!(file_kind(OsStr::new("README")), Kind::Other);
    }

    #[test]
    fn folders_by_name_and_siblings() {
        let none = |_: &str| false;
        let cargo = |s: &str| s == "Cargo.toml";
        assert_eq!(dir_class(OsStr::new("node_modules"), OsStr::new("web"), false, none).reclaim, Some(Reclaim::NodeModules));
        assert_eq!(dir_class(OsStr::new("target"), OsStr::new("prism"), false, cargo).reclaim, Some(Reclaim::RustTarget));
        // A `target` without a Cargo.toml beside it is anyone's.
        assert_eq!(dir_class(OsStr::new("target"), OsStr::new("x"), false, none).reclaim, None);
        assert_eq!(dir_class(OsStr::new(".cache"), OsStr::new("me"), true, none).reclaim, Some(Reclaim::Caches));
        assert_eq!(dir_class(OsStr::new(".cache"), OsStr::new("project"), false, none).reclaim, None);
        assert_eq!(dir_class(OsStr::new("Safari.app"), OsStr::new("Applications"), false, none).kind, Some(Kind::Apps));
    }

    #[test]
    fn codes_round_trip() {
        for k in Kind::ALL {
            assert_eq!(Kind::from_code(k.code()), k);
        }
        assert_eq!(Reclaim::from_code(Reclaim::code(None)), None);
        for r in Reclaim::ALL {
            assert_eq!(Reclaim::from_code(Reclaim::code(Some(r))), Some(r));
        }
    }
}
