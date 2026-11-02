//! Changes to Prism's own environment. The C library's `getenv` is not synchronised with
//! `setenv`: a thread reading the environment while another changes it can read freed memory,
//! hence `unsafe` since Rust 2024.
//!
//! These stay safe functions so that the crates above, which forbid `unsafe`, can call them;
//! their contract is the callers' to keep, and each call says why it holds.

use std::ffi::OsStr;

/// Sets `name` to `value`. Only at the very start of `main`, before any other thread starts
/// (`shell_env`, `profile::export`, before `rt::init` builds the runtime), or in a test for
/// variables only the programs it starts read: Rust's own readers (`std::env::var`, `Command`)
/// take std's environment lock, so the race left is with C code calling `getenv` meanwhile.
pub fn set_var(name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) {
    // SAFETY: by this function's contract, no other thread reads the environment meanwhile
    // outside std's lock.
    unsafe { std::env::set_var(name, value) };
}

/// Removes `name`, under `set_var`'s contract.
pub fn remove_var(name: impl AsRef<OsStr>) {
    // SAFETY: as `set_var`.
    unsafe { std::env::remove_var(name) };
}
