//! What a phone may call through the remote (`[remote]`): every command of the window, and every
//! command of a block (`Block::remote`), has a class. The remote routes a call only when its class
//! is allowed, whatever the page asks; a command without a class is never routed.

/// A command's risk, from a phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Reads, changes nothing: the session is enough.
    Read,
    /// Changes something that is undone or harmless (a status, a comment, a retry): the session
    /// is enough, with `[remote] actions`.
    Act,
    /// Cannot be undone from Prism, or types into a terminal (a merge, a deploy, a kill, the TTY):
    /// biometrics each time, with `[remote] critical`.
    Critical,
    /// Never routed: the config and its secrets, or what acts on the computer itself.
    Never,
}
