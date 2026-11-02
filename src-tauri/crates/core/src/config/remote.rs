//! Prism from a phone (`[remote]`): the remote's server, how a phone reaches it, what it shows,
//! and how long a phone's session lasts. Personal: a pack does not set it.

use serde::{Deserialize, Serialize};

/// `[remote]`. Off by default; on, Prism answers on loopback only (`port`), behind a tunnel.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct RemoteConfig {
    pub enabled: bool,
    /// How a phone reaches the Mac: `tailscale` (Prism runs `tailscale serve` and checks the
    /// tailnet user), `other` (a tunnel of yours brings HTTPS to `port`, at `url`), or `relay`
    /// (another profile's Prism, open beside this one with one of those, brings the phone here).
    pub tunnel: RemoteTunnel,
    /// The loopback port Prism listens on; never another interface. Tunnel `relay`: unused, a free
    /// port is taken and told to the profile's relay only (`remote/endpoint.json`).
    pub port: u16,
    /// Tunnel tailscale through a `tailscaled` of its own, in userspace mode beside a VPN
    /// (`just tailscale`): its socket. Empty: the Tailscale app's.
    pub tailscale_socket: String,
    /// `other`: the HTTPS address the phone opens (`https://prism.example.net`); the passkeys are
    /// bound to its host. `tailscale`: empty, the Mac's tailnet name.
    pub url: String,
    /// The Mail block on the phone; off, its data never leaves the Mac.
    pub mail: bool,
    /// A phone may act (class `act`): retry, rebase, comment, a ticket's status, an agent's
    /// permission or prompt. Off, it only reads.
    pub actions: bool,
    /// A phone may do what is critical (merge, a manual job, a kill, a removal), each time
    /// confirmed by its passkey (biometrics). Off: never.
    pub critical: bool,
    /// A phone may send a text to an agent from its answer box, and take an agent's terminal to type
    /// in it, both confirmed by its passkey (once per agent for `step_up_minutes`). Off: it watches,
    /// and answers with its quick answers and keys only. On by default: the passkey guards it.
    pub typing: bool,
    /// A phone may restart or shut down the Mac (More › The Mac), each time confirmed by its
    /// passkey. Off by default.
    pub power: bool,
    /// A phone reads the settings (More › Settings, secrets aside) and changes the few of
    /// `PHONE_SETTINGS`, each save confirmed by its passkey. On by default: the passkey guards it.
    pub settings: bool,
    /// A phone may create a project (More › New project): a folder with a git repository in one of
    /// the `[[workspaces]]`, never elsewhere, and on request its project on the forge; added to the
    /// repositories. Each time confirmed by its passkey. On by default: nothing without a working
    /// folder, and the passkey guards it.
    pub projects: bool,
    /// Taking a terminal again within this long asks nothing more.
    pub step_up_minutes: u32,
    /// The phone gives a terminal back after this long without a key; 0: only when released or
    /// when the desk types.
    pub control_idle_minutes: u32,
    /// The phone's bottom tabs, in order; the other views are under More.
    pub tabs: Vec<String>,
    /// A paired phone is woken when an agent waits for it (a permission, a question): an empty
    /// push, its text written by the phone; nothing of Prism goes through the push service.
    pub push: bool,
    /// Without a request from the phone for this long, biometrics again.
    pub session_idle_minutes: u32,
    /// Biometrics again after this long, active or not.
    pub session_max_hours: u32,
    /// The Mac does not idle-sleep while the remote is on and on power.
    pub keep_awake: bool,
    /// macOS, while the remote is on: Prism opens at login and again after a crash (a LaunchAgent
    /// runs `prism watch` every minute); quit with ⌘Q, it stays closed until the next login. Off by
    /// default.
    pub keep_running: bool,
    /// A phone opens the dev servers its agents run (Dev servers block) in its browser, through a
    /// second address of the tunnel, its session required. On by default: nothing without the
    /// session, and only the ports of the block's running servers.
    pub preview: bool,
    /// The preview's loopback port; never another interface.
    pub preview_port: u16,
    /// Tunnel `other`: the HTTPS address your tunnel brings to `preview_port`, on the same host as
    /// `url` (another port). Empty: no preview. Tunnel `tailscale`: empty, the Mac's tailnet name
    /// on port 8443.
    pub preview_url: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RemoteTunnel {
    #[default]
    Tailscale,
    Other,
    /// Reached through the profile whose remote has the tunnel (the hub), at `/p/<profile>/`: no
    /// tunnel, no pairing nor passkey of its own; the hub checks the phone, this Prism keeps its
    /// own `[remote]` rights.
    Relay,
}

/// A view as `tabs` names it: lower case, digits and `_`. The phone knows its views (home, alerts, mrs,
/// tickets, refinement, agents, worktrees, notes, today, ci, renovate, infra, argocd, sprint, and every
/// block with a source of its own by its id: system, trail, docker, a pack's…) and drops the others.
fn view_id(t: &str) -> bool {
    !t.is_empty() && t.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tunnel: RemoteTunnel::Tailscale,
            port: 47800,
            url: String::new(),
            tailscale_socket: String::new(),
            mail: false,
            actions: false,
            critical: false,
            typing: true,
            power: false,
            settings: true,
            projects: true,
            step_up_minutes: 5,
            control_idle_minutes: 0,
            tabs: ["home", "mrs", "tickets", "agents"].map(String::from).to_vec(),
            push: false,
            session_idle_minutes: 120,
            session_max_hours: 12,
            keep_awake: true,
            keep_running: false,
            preview: true,
            preview_port: 47801,
            preview_url: String::new(),
        }
    }
}

/// The settings a phone may change (`[remote] settings`): what only tells Prism how to show and
/// when to notify. Nothing that runs a program, lets someone in, or decides what is read: those
/// stay on the Mac. Each is a key of a table, `["alerts", "quiet_hours"]`.
pub const PHONE_SETTINGS: &[(&str, &str)] = &[
    ("alerts", "digest"),
    ("alerts", "digest_times"),
    ("alerts", "digest_urgent"),
    ("alerts", "quiet_hours"),
    ("alerts", "focus_events"),
    ("alerts", "behind_commits"),
    ("alerts", "pending_job_minutes"),
    ("alerts", "review_wait_hours"),
    ("alerts", "doing_idle_days"),
    ("alerts", "waiting_session_hours"),
    ("alerts", "your_turn_minutes"),
    ("alerts", "argo_out_of_sync_minutes"),
    ("alerts", "meeting_lead_minutes"),
    ("alerts", "worktree_in_use_hours"),
    ("alerts", "large_mr_lines"),
    ("alerts", "no_reviewer_minutes"),
    ("alerts", "open_threads"),
    ("alerts", "band_now"),
    ("alerts", "band_later"),
    ("alerts", "band_hidden"),
    ("alerts", "escalate_hours"),
    ("alerts", "group"),
    ("appearance", "theme"),
    ("appearance", "density"),
    ("appearance", "language"),
    ("appearance", "animations"),
    ("appearance", "theme_motion"),
    ("appearance", "tooltips"),
    ("appearance", "screensaver"),
    ("appearance", "screensaver_seconds"),
];

/// Whether a phone may write `value` at `path`: a key of `PHONE_SETTINGS`, and a plain value (a
/// switch, a number, a text, a list of texts) or none (back to the default).
pub fn phone_may_set(path: &[String], value: Option<&serde_json::Value>) -> bool {
    use serde_json::Value;
    let listed = matches!(path, [t, k] if PHONE_SETTINGS.iter().any(|(pt, pk)| pt == t && pk == k));
    let plain = match value {
        None | Some(Value::Bool(_) | Value::Number(_) | Value::String(_)) => true,
        Some(Value::Array(items)) => items.iter().all(Value::is_string),
        Some(_) => false,
    };
    listed && plain
}

impl RemoteConfig {
    /// The tabs as given, trimmed, what cannot be a view and repeats left out.
    pub fn tabs(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for t in self.tabs.iter().map(|t| t.trim()) {
            if view_id(t) && !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_by_default_on_loopback() {
        let c: RemoteConfig = toml::from_str("").unwrap();
        assert!(!c.enabled);
        assert_eq!((c.tunnel, c.port, c.session_idle_minutes, c.session_max_hours), (RemoteTunnel::Tailscale, 47800, 120, 12));
        let c: RemoteConfig = toml::from_str("tunnel = \"other\"\ntabs = [\"pet\", \"No pe\", \"pet\", \"alerts\", \"\"]").unwrap();
        assert_eq!(c.tunnel, RemoteTunnel::Other);
        assert_eq!(c.tabs(), ["pet", "alerts"]);
        assert!(c.settings && c.projects);
        let c: RemoteConfig = toml::from_str("tunnel = \"relay\"").unwrap();
        assert_eq!(c.tunnel, RemoteTunnel::Relay);
    }

    #[test]
    fn a_phone_sets_only_the_listed_keys_to_plain_values() {
        use serde_json::json;
        let p = |t: &str, k: &str| vec![t.to_string(), k.to_string()];
        assert!(phone_may_set(&p("alerts", "quiet_hours"), Some(&json!("19:00-08:00"))));
        assert!(phone_may_set(&p("alerts", "digest_times"), Some(&json!(["12:30", "17:00"]))));
        assert!(phone_may_set(&p("appearance", "theme"), None));
        // What runs a program, lets someone in, or is not a plain value: never.
        assert!(!phone_may_set(&p("agents", "bin"), Some(&json!("sh"))));
        assert!(!phone_may_set(&["agents".into(), "claude".into(), "bin".into()], Some(&json!("sh"))));
        assert!(!phone_may_set(&p("remote", "settings"), Some(&json!(true))));
        assert!(!phone_may_set(&p("appearance", "friday_guard"), Some(&json!(false))));
        assert!(!phone_may_set(&p("alerts", "digest_times"), Some(&json!([{ "x": 1 }]))));
        assert!(!phone_may_set(&p("alerts", "quiet_hours"), Some(&json!({ "a": "b" }))));
        assert!(!phone_may_set(&["alerts".to_string()], Some(&json!({ "digest": true }))));
    }
}
