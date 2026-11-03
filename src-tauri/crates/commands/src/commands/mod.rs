//! The commands the window invokes, by domain; `table.rs` lists them all, the window's own
//! (`desk`) with them, `tauri_handler()` registers the others with Tauri, `phone_call()` answers
//! a phone's.

pub mod agents;
pub mod system;
pub mod infra;
pub mod tracker;
pub mod config;
pub mod integrations;
pub mod forge;
pub mod remote;
pub mod feedback;

use crate::app::AppState;

/// The state a command reads: Tauri's (`tauri::State`) in the window, the server's otherwise.
pub struct State<'a, T: ?Sized = AppState>(pub(crate) &'a T);

impl<T: ?Sized> Clone for State<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for State<'_, T> {}

impl<'a, T: ?Sized> State<'a, T> {
    pub fn inner(&self) -> &'a T {
        self.0
    }
}

impl<T: ?Sized> std::ops::Deref for State<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0
    }
}

/// The window's app manages the state as an `Arc` (its menu, its tray and the remote's host
/// share it).
#[cfg(feature = "tauri")]
impl<'de, R: tauri::Runtime> tauri::ipc::CommandArg<'de, R> for State<'de, AppState> {
    fn from_command(command: tauri::ipc::CommandItem<'de, R>) -> Result<Self, tauri::ipc::InvokeError> {
        let state = <tauri::State<'de, std::sync::Arc<AppState>> as tauri::ipc::CommandArg<'de, R>>::from_command(command)?;
        Ok(State(&**state.inner()))
    }
}

/// A command's error, as the window reads it: the message, with its causes (`{e:#}`). Lets the
/// commands use `?` on anyhow's errors and on the few others they meet.
#[derive(Debug)]
pub struct CmdError(String);

pub type Cmd<T = ()> = Result<T, CmdError>;

impl serde::Serialize for CmdError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl From<anyhow::Error> for CmdError {
    fn from(e: anyhow::Error) -> Self {
        Self(format!("{e:#}"))
    }
}

impl From<String> for CmdError {
    fn from(e: String) -> Self {
        Self(e)
    }
}

impl From<&str> for CmdError {
    fn from(e: &str) -> Self {
        Self(e.into())
    }
}

macro_rules! cmd_error_from {
    ($($t:ty),*) => {$(
        impl From<$t> for CmdError {
            fn from(e: $t) -> Self {
                Self(e.to_string())
            }
        }
    )*};
}

cmd_error_from!(tokio::task::JoinError, std::io::Error);
#[cfg(feature = "tauri")]
cmd_error_from!(tauri::Error);

/// What only GitLab offers (merge queue label, redeploying a tag).
fn gitlab(state: &AppState) -> Cmd<crate::collectors::gitlab::Actions> {
    state.forge().gitlab().cloned().map_err(Into::into)
}

/// What the demo profile does with a command: refused (it would act on GitLab, ClickUp, an agent,
/// the Mac), or allowed (it reads, or answers the demo itself).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Demo {
    Allowed,
    Refused,
}

/// A command's answer as JSON.
pub(crate) fn reply<T: serde::Serialize>(r: Result<T, CmdError>) -> anyhow::Result<serde_json::Value> {
    let v = r.map_err(|e| anyhow::anyhow!(serde_json::to_value(&e).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()))?;
    Ok(serde_json::to_value(v)?)
}

pub(crate) fn args<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> anyhow::Result<T> {
    Ok(serde_json::from_value(if v.is_null() { serde_json::Value::Object(Default::default()) } else { v })?)
}

/// A `[phone: …]` command answered: its route as any other's (`#[prism_macros::command]`'s `peer`,
/// its arguments read as the window sends them); the list says which, and that its arguments are
/// the command's own (`remote::tests`).
macro_rules! phone_route {
    ($app:ident, $args:ident, $f:path; $($phone:tt)*) => {
        $f($app, $args).await
    };
}

/// `module::command: Class [demo: own | refused],` for each command (`table.rs`), after the
/// window's own (`desk { command: Class, … }`, the app's): `COMMANDS`, `DESK`, the phone's
/// routes, and `tauri_handler()` that registers the others with Tauri.
macro_rules! commands {
    (
        desk { $($desk:ident : $dclass:ident $([demo: $ddemo:ident])?,)* }
        $($module:ident :: $name:ident : $class:ident $([demo: $demo:ident])? $([phone: $($phone:tt)*])?,)*
    ) => {
        /// Command → its class from a phone, and what the demo profile does with it.
        pub const COMMANDS: &[(&str, Class, Demo)] = &[
            $((stringify!($desk), $dclass, demo!($dclass $(, $ddemo)?)),)*
            $((stringify!($name), $class, demo!($class $(, $demo)?))),*
        ];

        /// The window's own commands, which its app answers (Tauri's window, menu, tray, clipboard).
        pub const DESK: &[&str] = &[$(stringify!($desk)),*];

        /// The commands a phone calls as the window does (`[phone: …]`).
        #[cfg_attr(not(test), allow(dead_code))]
        pub const PHONE_ROUTED: &[&str] = &[$($(phone_name!($name; $($phone)*),)?)*];

        /// A phone's call of one of them (`crate::remote`): its answer; another command is refused.
        pub async fn phone_call(app: &AppState, command: &str, a: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            match command {
                $($(stringify!($name) => phone_route!(app, a, $module::$name::peer; $($phone)*),)?)*
                other => anyhow::bail!(prism_base::tr!("{other} is not answered from a phone", other = other)),
            }
        }

        /// Another Prism's window calling any of them but the window's own (`crate::remote::peer`):
        /// its answer; the window's own are its app's.
        pub async fn peer_call(app: &AppState, command: &str, a: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            match command {
                $(stringify!($name) => $module::$name::peer(app, a).await,)*
                other => anyhow::bail!(prism_base::tr!("{other} is not answered from another Prism", other = other)),
            }
        }

        /// The commands but the window's own, registered with Tauri; the demo profile's guard
        /// (`refused_in_demo`) is the app's, in front of them and its own.
        #[cfg(feature = "tauri")]
        pub fn tauri_handler() -> fn(tauri::ipc::Invoke) -> bool {
            tauri::generate_handler![$($module::$name),*]
        }
    };
}

/// A `[phone: …]` command's name.
macro_rules! phone_name {
    ($name:ident; $($phone:tt)*) => {
        stringify!($name)
    };
}

/// A command's demo rule: from its class, unless the table says otherwise.
macro_rules! demo {
    ($class:ident) => {
        match $class {
            Act | Critical => Demo::Refused,
            Read | Never => Demo::Allowed,
        }
    };
    ($class:ident, own) => {
        Demo::Allowed
    };
    ($class:ident, refused) => {
        Demo::Refused
    };
}

mod table;
pub use table::{peer_call, phone_call, COMMANDS, DESK};
#[cfg(feature = "tauri")]
pub use table::tauri_handler;
#[cfg(test)]
#[allow(unused_imports)]
pub use table::PHONE_ROUTED;

/// The demo profile is on and refuses `command` (it writes nothing: its data is a sample).
pub fn refused_in_demo(command: &str) -> bool {
    crate::demo::active() && COMMANDS.iter().any(|(name, _, demo)| *name == command && *demo == Demo::Refused)
}
