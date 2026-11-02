//! Prism's calls into Apple's frameworks, behind safe types and functions: the other crates
//! forbid `unsafe` (`[workspace.lints]`), the C library apart (`prism-sys`). Each block says why
//! it holds. Empty on other systems.

#[cfg(target_os = "macos")]
pub mod appkit;
#[cfg(target_os = "macos")]
pub mod coreaudio;
#[cfg(target_os = "macos")]
pub mod corewlan;
#[cfg(target_os = "macos")]
pub mod eventkit;
