//! `[remote] keep_awake`: no idle sleep while the remote is on. macOS: `caffeinate -s`, valid on
//! power only (on battery, the Mac sleeps as usual); Linux: `systemd-inhibit`; Windows:
//! `SetThreadExecutionState`, held by a thread of its own. Dropped, the assertion goes with its
//! process (its thread).

pub struct Awake(#[allow(dead_code)] Hold);

enum Hold {
    None,
    #[cfg(not(windows))]
    Process(#[allow(dead_code)] tokio::process::Child),
    /// Windows: dropped, the thread lets go and ends.
    #[cfg(windows)]
    Thread(#[allow(dead_code)] std::sync::mpsc::Sender<()>),
}

impl Awake {
    pub fn hold() -> Self {
        #[cfg(windows)]
        return Self::thread();
        #[cfg(not(windows))]
        Self::process()
    }

    #[cfg(windows)]
    fn thread() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let spawned = std::thread::Builder::new().name("keep-awake".into()).spawn(move || {
            if !prism_sys::windows::stay_awake(true) {
                tracing::warn!("remote: keep_awake: SetThreadExecutionState refused");
            }
            // Until the sender drops.
            let _ = rx.recv();
            prism_sys::windows::stay_awake(false);
        });
        match spawned {
            Ok(_) => Self(Hold::Thread(tx)),
            Err(e) => {
                tracing::warn!("remote: keep_awake: {e}");
                Self(Hold::None)
            }
        }
    }

    #[cfg(not(windows))]
    fn process() -> Self {
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
            return Self(Hold::None);
        };
        cmd.kill_on_drop(true).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        match cmd.spawn() {
            Ok(child) => Self(Hold::Process(child)),
            Err(e) => {
                tracing::warn!("remote: keep_awake: {e}");
                Self(Hold::None)
            }
        }
    }
}
