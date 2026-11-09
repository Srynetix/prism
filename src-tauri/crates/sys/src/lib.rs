//! Every call of Prism into the C library (and WebKitGTK's) that Rust cannot check, behind safe
//! functions: the other crates forbid `unsafe` (`[workspace.lints]`), Apple's frameworks apart
//! (`prism-macos`). Each block says why it holds; a function whose soundness rests on its caller
//! says so in its own documentation.

#[cfg(windows)]
pub mod audio;
#[cfg(target_os = "macos")]
pub mod dirlist;
pub mod env;
#[cfg(any(unix, windows))]
pub mod ipc;
#[cfg(unix)]
pub mod fd;
#[cfg(unix)]
pub mod process;
#[cfg(target_os = "macos")]
pub mod sysctl;
#[cfg(unix)]
pub mod term;
#[cfg(target_os = "linux")]
pub mod webkit;
#[cfg(windows)]
pub mod windows;
