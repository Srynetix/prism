//! Every call of Prism into the C library (and WebKitGTK's) that Rust cannot check, behind safe
//! functions: the other crates forbid `unsafe` (`[workspace.lints]`), Apple's frameworks apart
//! (`prism-macos`). Each block says why it holds; a function whose soundness rests on its caller
//! says so in its own documentation.

pub mod env;
#[cfg(unix)]
pub mod fd;
#[cfg(unix)]
pub mod process;
#[cfg(target_os = "macos")]
pub mod sysctl;
#[cfg(target_os = "linux")]
pub mod webkit;
