//! What the window shows, as the collectors publish it: the snapshot and its types, by domain
//! in this module's files. The window's `types.ts` mirrors them.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

mod hub;
mod cluster;
mod forge;
mod tracker;
mod agents;
mod integrations;

pub use hub::*;
pub use cluster::*;
pub use forge::*;
pub use tracker::*;
pub use agents::*;
pub use integrations::*;

/// Applications to open a repository in from the Repos menu (`open -a`).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpenWith {
    pub editor: Option<String>,
    pub terminal: Option<String>,
}

/// The blocks that read a source of their own, on or off.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Features {
    pub vulnerabilities: bool,
    pub renovate: bool,
    pub ci_health: bool,
}

/// Another profile open in its own Prism on this Mac (`profile::Presence`): its agents and those
/// waiting, for the status bar.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OtherProfile {
    pub id: String,
    pub name: String,
    pub color: String,
    pub agents: u32,
    pub waiting: u32,
    /// The title of the agent waiting longest there.
    pub first: Option<String>,
}

/// The active profile, for the rail and the window title.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActiveProfile {
    pub id: String,
    pub name: String,
    pub color: String,
    /// Other profiles exist: the window, the menu bar icon and the Dock name this one.
    pub shared: bool,
    /// No `config.toml` yet (first launch): the window offers to set it up (`setup_profile`).
    pub fresh: bool,
}

impl Snapshot {
    /// The MR is one of repository `name` (`[[repos]] name`): its own, or the primary repository's (`repo` absent).
    pub fn mr_in_repo(&self, m: &MergeRequest, name: &str) -> bool {
        match &m.repo {
            Some(r) => r == name,
            None => self.repositories.iter().any(|r| r.primary && r.name == name),
        }
    }

    /// When the worktree's branch was delivered: its MRs' last merge, none while one is still open.
    /// Its commits from before count as delivered (squash) when removing it.
    pub fn merged_at(&self, w: &Worktree) -> Option<DateTime<Utc>> {
        let mrs: Vec<_> = self.merge_requests.iter().filter(|m| self.mr_in_repo(m, &w.repo) && Some(m.source_branch.as_str()) == w.branch.as_deref()).collect();
        if mrs.iter().any(|m| m.state == MrState::Opened) { None } else { mrs.iter().filter_map(|m| m.merged_at).max() }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Snapshot {
    /// Current user on the profile's forge, to tell apart "my" MRs and the reviews waiting for me.
    pub me: Option<String>,
    /// The same on the other forge, for the MRs of the repositories there (`MergeRequest::forge`).
    pub me_elsewhere: Option<String>,
    /// Merge queue label (`merge-queue`), which Prism adds to or removes from an MR.
    pub merge_queue_label: Option<String>,
    /// Prefix of the labels that order the merge queue (`merge-priority::`), empty for none.
    pub merge_priority_prefix: Option<String>,
    /// The pods of the `[[pods]]` targets, by target, namespace, role and name.
    pub pods: Vec<PodView>,
    /// By pod (`<target>/<namespace>/<name>`); updated separately from the watch, metrics not
    /// being observable.
    pub pod_usage: BTreeMap<String, Usage>,
    /// Nodes hosting the targets' pods, re-read along with the metrics.
    pub pod_nodes: Vec<NodeLoad>,
    pub worktrees: Vec<Worktree>,
    /// `[worktrees] stale_days`, applied as soon as it is saved.
    pub worktree_stale_days: u32,
    /// The repositories with a local clone (`[[repos]]`), the primary one first.
    pub repos: Vec<Repo>,
    /// Every repository of the config, the primary one first: its role and switches.
    pub repositories: Vec<crate::config::RepoInfo>,
    /// The blocks that read repositories (CI health, Renovate, Vulnerabilities, a pack's), and
    /// which ones they read.
    pub repo_blocks: Vec<crate::blocks::RepoBlock>,
    pub open_with: OpenWith,
    /// Console settings (`[console]`).
    pub console: crate::config::ConsoleConfig,
    /// The command palette (`[palette]`).
    pub palette: crate::config::PaletteConfig,
    /// The editor of an agent's Files tab (`[files]`).
    pub files: crate::config::FilesConfig,
    /// Quick note entry (`[notes]`).
    pub notes: crate::config::NotesConfig,
    /// Theme and playful touches (`[appearance]`).
    pub appearance: crate::config::AppearanceConfig,
    /// Rail pages (`[[views]]`); empty: the app's own.
    pub views: Vec<crate::config::ViewConfig>,
    /// The pages wizard's templates added by the packs (`[[templates]]`).
    pub templates: Vec<crate::config::TemplateConfig>,
    /// The active profile (`profile`): its id, name and color.
    pub profile: ActiveProfile,
    /// The other profiles open beside this one, by name.
    pub other_profiles: Vec<OtherProfile>,
    /// Blocks whose source is on (`[vulnerabilities]`, `[renovate] enabled`,
    /// `[gitlab] ci_health`): an off one says so instead of waiting for data.
    pub features: Features,
    /// The stages Environments, ArgoCD and Settings read (`[clusters] stages`, see `stages`).
    pub stages: Vec<crate::stages::Stage>,
    /// Kubernetes collectors started (`[runners] enabled`).
    pub cluster: bool,
    /// A shell in a pod may be opened (`[clusters] shell`, the clusters read).
    pub pod_shell: bool,
    /// A pod's CPU and memory over time may be shown (`[clusters] usage_history`, the clusters read).
    pub pod_history: bool,
    /// The forge and the repository read (`[forge] enabled`).
    pub gitlab: bool,
    /// The forge's provider: the window's vocabulary and links follow it.
    pub forge: crate::forge::Provider,
    /// macOS Calendar read (`[calendar] enabled`).
    pub calendar: bool,
    /// The tracker read (`[clickup] enabled`), and its provider: the window's vocabulary follows it.
    pub tracker: Option<crate::tracker::Provider>,
    /// What the tracker offers.
    pub tracker_features: crate::tracker::Features,
    /// The state of each block with a source of its own (`blocks`), by its id; absent: off.
    pub blocks: BTreeMap<String, serde_json::Value>,
    /// The Mail block's inbox (`[mail] enabled`); absent: not read.
    pub mail: Option<MailState>,
    /// Team and repository conventions (ticket prefix, branches, flow statuses).
    pub conventions: crate::conventions::Conventions,
    /// Alert thresholds (`[alerts]`).
    pub alerts: crate::config::AlertsConfig,
    pub sessions: Vec<ClaudeSession>,
    /// Archived desktop sessions that had a worktree.
    pub archived_sessions: Vec<ArchivedSession>,
    /// Agents launched by Prism, most recent first.
    pub agents: Vec<Agent>,
    /// The last agent closed in each repository, most recent first.
    pub left: Vec<LeftSession>,
    /// The agents' chains, by agent: what Prism runs after them (`prism_agents::chains`).
    pub chains: BTreeMap<String, Chain>,
    /// The prompts waiting for each agent (its menu open), by agent: how many.
    pub inbox: BTreeMap<String, u32>,
    /// Changes each time the book of what every agent cost changes (`agent-costs.json`): the
    /// book itself is read on demand (`agent_costs`), by the Actual sections that show it.
    pub agent_costs_rev: u64,
    /// Agent settings (`[agents]`).
    pub agents_settings: crate::config::AgentsConfig,
    /// The plan usage of `[agents] provider`'s CLI.
    pub usage: Option<ClaudeUsage>,
    /// The plan usage of the other CLIs agents run on, in the order their first agent was launched.
    pub other_usage: Vec<ProviderUsage>,
    pub merge_requests: Vec<MergeRequest>,
    pub pipelines: Vec<Pipeline>,
    pub pending_jobs: Vec<PendingJob>,
    /// Absent until the pipelines have been read.
    pub ci_health: Option<CiHealth>,
    /// Absent before the first Trivy scan.
    pub vulnerabilities: Option<Vulnerabilities>,
    /// Absent until the clusters have been read (or without `[[environments]]`).
    pub environments: Option<Environments>,
    /// ArgoCD applications; absent with no instance found, or before the first read.
    pub argocd: Option<ArgoCd>,
    pub renovate: Option<Renovate>,
    pub sprint: Option<Sprint>,
    /// The next sprint, all its tickets; absent with no sprint dated after the current one.
    pub next_sprint: Option<Sprint>,
    /// Tasks to log time on (`time` boards): recurring time-entry items (meetings, support…).
    pub time_tasks: Option<Board>,
    /// Refinement queue: tickets of the `refinement` boards (a folder's quarterly lists, of the
    /// current quarter and the following ones, or a board's list), in the refinement statuses,
    /// with the open EPICs and their children.
    pub refinement: Refinement,
    /// "To pick" view: next sprint, bugs, hotfixes.
    pub pickup: Pickup,
    /// Tickets in progress outside the sprint, for the Sprint block.
    pub outside_sprint: OutsideSprint,
    /// Absent until the Calendar has been read (or if the agenda is disabled).
    pub agenda: Option<Agenda>,
    pub time: TimeTracking,
    /// The tracker's user, to recognize my tickets.
    pub tracker_me: Option<Person>,
    /// ClickUp workspace, to build a ticket's link from its `ABC-…`.
    pub clickup_team: Option<String>,
    /// Where a ticket opens by its id, followed by it (Jira: `https://team.atlassian.net/browse/`).
    pub ticket_base: Option<String>,
    /// The stores the window and the phones share (`shared_save`: notes, retro points, pet, block filters), by name: the time of
    /// their last write in milliseconds (absent: none since launch). The other pages read them again.
    pub shared_revs: BTreeMap<String, u64>,
    /// Last error per source; absent when the source responds.
    pub errors: BTreeMap<&'static str, String>,
    /// Prism's recent warnings and errors, most recent first (`problems`): the status bar lists them.
    pub problems: Vec<crate::problems::Problem>,
    /// The blocks of a pack whose settings changed since launch while they do not reload: they
    /// apply once Prism restarts. Every other setting applies as it is saved.
    pub waiting_restart: Vec<String>,
    /// Sources that have responded at least once (success or error): the startup screen waits for them.
    pub answered: std::collections::BTreeSet<&'static str>,
    pub updated_at: Option<DateTime<Utc>>,
    /// Prism from a phone (`[remote]`): absent when off. Never sent to a phone.
    pub remote: Option<RemoteStatus>,
}

/// The remote, as the desk shows it (status bar, Settings › Remote › Devices).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RemoteStatus {
    /// Where a phone opens Prism; absent until the server and its tunnel are up.
    pub url: Option<String>,
    /// Why the remote does not answer (port taken, Tailscale missing…).
    pub error: Option<String>,
    /// The paired phones, most recent first.
    pub devices: Vec<RemoteDevice>,
    /// A pairing under way.
    pub pairing: Option<RemotePairing>,
    /// Agents' terminals a phone controls now: the desk can take them back.
    pub controls: Vec<RemoteControl>,
    /// Tunnel `relay`: reached through another profile's Prism (its pairing, its phones); `url`
    /// is this profile's address there, absent while that Prism is closed.
    pub relay: bool,
    /// The other Prisms that open this one in their window (`[[machines]]` there), A to Z.
    pub peers: Vec<RemotePeer>,
    /// A computer's pairing under way.
    pub peer_pairing: Option<PeerPairing>,
}

/// Another Prism that opens this one in its window, with a token made here.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RemotePeer {
    pub name: String,
    pub paired_at: DateTime<Utc>,
    /// A window of it is connected now.
    pub connected: bool,
}

/// A computer's pairing: the code typed on it, then its name, to accept here.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PeerPairing {
    pub code: String,
    pub expires_at: DateTime<Utc>,
    /// Once a computer has given the code: the name it gives.
    pub name: Option<String>,
}

/// Settings › Remote › Away: one thing that decides whether the Mac stays within reach for days
/// without anyone at it (`prism_remote::away`).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AwayCheck {
    /// Fine as it is; otherwise a risk to weigh, `text` saying what to do.
    pub ok: bool,
    pub text: String,
}

/// An agent's terminal a phone types in.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RemoteControl {
    pub agent: String,
    /// The phone's name.
    pub device: String,
    pub since: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RemoteDevice {
    pub id: String,
    pub name: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
    /// A session of it is open and its page connected now.
    pub connected: bool,
}

/// A pairing: its one-time address (shown as a QR code), then the code the phone shows, to
/// confirm on the desk.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RemotePairing {
    /// The address the phone opens, its one-time token in the fragment.
    pub url: String,
    /// That address as an SVG QR code.
    pub qr_svg: String,
    pub expires_at: DateTime<Utc>,
    /// Once a phone has opened it: the code it shows, and the name it gives.
    pub code: Option<String>,
    pub device: Option<String>,
    /// The phone has created its passkey: the desk may confirm.
    pub ready: bool,
}

/// `*slot = value`; whether it changed: what a `Hub::update` returns.
pub fn replace<T: PartialEq>(slot: &mut T, value: T) -> bool {
    let changed = *slot != value;
    *slot = value;
    changed
}
