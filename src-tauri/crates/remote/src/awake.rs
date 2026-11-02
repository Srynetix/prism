//! `[remote] keep_awake`: no idle sleep while the remote is on. macOS: `caffeinate -s`, valid on
//! power only (on battery, the Mac sleeps as usual); Linux: `systemd-inhibit`. Dropped, the
//! assertion goes with its process.

pub struct Awake(#[allow(dead_code)] Option<tokio::process::Child>);

impl Awake {
    pub fn hold() -> Self {
        let pid = std::process::id().to_string();
        let mut cmd = if cfg!(target_os = "macos") {
            let mut c = prism_base::platform::command("caffeinate");
            c.args(["-s", "-w", &pid]);
            c
        } else if cfg!(target_os = "linux") {
            let mut c = prism_base::platform::command("systemd-inhibit");
            c.args(["--what=idle:sleep", "--who=Prism", "--why=Prism's remote is on", "--mode=block", "sleep", "infinity"]);
            c
        } else {
            return Self(None);
        };
        cmd.kill_on_drop(true).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        match cmd.spawn() {
            Ok(child) => Self(Some(child)),
            Err(e) => {
                tracing::warn!("remote: keep_awake: {e}");
                Self(None)
            }
        }
    }
}
