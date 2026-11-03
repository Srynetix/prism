//! What the window shows: alerts, views, templates, appearance, notes, console, the apps to open a folder in.

use serde::{Deserialize, Serialize};

use super::*;

/// Appearance (`[appearance]`), sent as is to the frontend: the theme, then one switch per
/// Alert thresholds, applied on save.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct AlertsConfig {
    /// Lag of a branch behind its base beyond which a badge flags it.
    pub behind_commits: u32,
    /// Job waiting for a runner for longer: runners are short.
    pub pending_job_minutes: u32,
    /// Review waiting for me for longer.
    pub review_wait_hours: u32,
    /// "In progress" ticket with neither branch nor MR for longer.
    pub doing_idle_days: u32,
    /// A Claude session waiting for me is flagged during this time, then set aside.
    pub waiting_session_hours: u32,
    /// A session whose turn is just over (Your turn: no question, no permission) is flagged this
    /// long only: stopping there is often the point. 0: never flagged.
    pub your_turn_minutes: u32,
    /// ArgoCD application out of sync for longer.
    pub argo_out_of_sync_minutes: u32,
    /// Notification before a meeting.
    pub meeting_lead_minutes: u32,
    /// More recent activity: a worktree is "in use".
    pub worktree_in_use_hours: u32,
    /// MR changing more lines (added + deleted): flagged as large.
    pub large_mr_lines: u32,
    /// My MR, out of draft, still without a reviewer this long after it was opened; 0: never.
    pub no_reviewer_minutes: u32,
    /// My open MR with review threads left unresolved is flagged.
    pub open_threads: bool,
    /// The alert rules shown in the band Now (someone waits on you); `band_later` in When you can,
    /// `band_hidden` not at all. A rule named nowhere goes by its severity: Now when broken, When you can
    /// for information, Today otherwise.
    pub band_now: Vec<String>,
    pub band_later: Vec<String>,
    pub band_hidden: Vec<String>,
    /// A Today alert held longer goes up to Now; 0: never.
    pub escalate_hours: u32,
    /// The alerts of one MR or one ticket on a single line, the action that unblocks the rest first.
    pub group: bool,
    /// Notifications held and delivered as a summary at `digest_times`, rather than one per event;
    /// the `digest_urgent` kinds still go out at once.
    pub digest: bool,
    /// When the digest goes out (`HH:MM`, local time).
    pub digest_times: Vec<String>,
    /// What still notifies at once in digest mode: `infra`, `deploy`, `agent` (a permission
    /// asked), `pipeline`, `review`, `data`, `block` (a block's own: Docker, dev servers…).
    pub digest_urgent: Vec<String>,
    /// No notification at all in this span (`19:00-08:00`), digest or not; empty: none.
    pub quiet_hours: String,
    /// During a calendar event whose title holds this (`Focus`), nothing notifies, urgent included;
    /// the digest waits for its end. Empty: never.
    pub focus_events: String,
}

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            behind_commits: 20,
            pending_job_minutes: 5,
            review_wait_hours: 4,
            doing_idle_days: 3,
            waiting_session_hours: 2,
            your_turn_minutes: 15,
            argo_out_of_sync_minutes: 15,
            meeting_lead_minutes: 5,
            worktree_in_use_hours: 24,
            large_mr_lines: 400,
            no_reviewer_minutes: 10,
            open_threads: true,
            band_now: ["question", "permission", "review", "branch-ci", "infra"].map(String::from).to_vec(),
            band_later: ["your-turn", "need-rebase", "only-ci", "no-ticket", "agent-cost", "worktree"].map(String::from).to_vec(),
            band_hidden: Vec::new(),
            escalate_hours: 24,
            group: true,
            digest: false,
            digest_times: ["09:00", "12:30", "17:30"].map(String::from).to_vec(),
            digest_urgent: ["infra", "deploy", "agent"].map(String::from).to_vec(),
            quiet_hours: String::new(),
            focus_events: "Focus".into(),
        }
    }
}

/// A rail page: its name, icon, color and blocks, one column per string (`"todo sessions notes"`).
/// "Edit layout" writes them here. `id` keeps the proportions of a renamed page; absent, derived
/// from the name (Edit layout writes it).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ViewConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub columns: Vec<String>,
}

/// A template of the pages wizard (`[[templates]]`): its pages and the access it turns on.
/// Applying it writes them into config.toml; the template itself is never a setting.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TemplateConfig {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub summary: String,
    /// What it requires, stated on its card ("Requires GitLab…").
    #[serde(default)]
    pub requires: String,
    /// `[forge] enabled`: MRs, CI, worktrees.
    #[serde(default)]
    pub gitlab: bool,
    /// `[runners] enabled`: runners, environments, ArgoCD.
    #[serde(default)]
    pub cluster: bool,
    /// `[data] enabled`: data pipelines on GCP.
    #[serde(default)]
    pub data: bool,
    /// `[agents] enabled`: the Agents view.
    #[serde(default = "default_true")]
    pub agents: bool,
    #[serde(default)]
    pub views: Vec<ViewConfig>,
}

/// `[appearance]`: the theme, the fonts and the playful touches, one switch per
/// feature. macOS "Reduce motion" and "Reduce transparency" turn off movement and a theme's
/// glass and overlays, unless `ignore_reduce`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct AppearanceConfig {
    /// `system` (Paper or Graphite, like macOS) or a theme: `graphite`, `midnight`…
    pub theme: String,
    /// Hovering a theme in the rail's Theme menu shows it on the whole window, until the menu is
    /// left or closed.
    pub theme_preview: bool,
    /// Room around the content, whatever the theme: `compact` (as drawn) or `comfortable`.
    pub density: String,
    /// Animations, a theme's glass and overlays even when macOS asks to reduce motion or
    /// transparency.
    pub ignore_reduce: bool,
    /// Splash screen on every launch.
    pub boot: bool,
    /// Small animations: refresh, Claude waiting or working, current step, counters,
    /// block being moved.
    pub animations: bool,
    /// A theme's own motion (Synthwave's sky, grid and neon; Hacker's cursor, CRT sweep,
    /// phosphor and rain), and its way of showing a row that arrives.
    pub theme_motion: bool,
    /// Confetti (my MR merged, my pipeline green), fireworks (sprint finished), shield
    /// (nothing to fix), sprout (nothing to do).
    pub celebrations: bool,
    /// Logo (7 clicks) and X-ray of the blocks (⌥ held).
    pub easter_eggs: bool,
    /// Prod tag on Friday after 4 pm: one more confirmation.
    pub friday_guard: bool,
    /// Text font (an installed family); empty: the system one.
    pub font: String,
    /// Code and terminal font; empty: Recursive Mono, bundled.
    pub font_mono: String,
    /// Agents' terminals font; empty: RecMonoCasual Nerd Font Mono, built in (a Nerd Font, for the prompt's icons).
    pub font_terminal: String,
    /// The language of Prism's own words: `en`, `fr`; empty: the system's (English when Prism
    /// does not speak it).
    pub language: String,
    /// The format of dates, times and numbers: a language tag (`fr-FR`, `en-GB`, `en-US`…);
    /// empty: the system's.
    pub locale: String,
    /// Screensaver started on its own after `screensaver_seconds` without keyboard or mouse (window
    /// visible); otherwise only on demand (`screensaver` command).
    pub screensaver: bool,
    pub screensaver_seconds: u32,
    /// The one that starts (on its own, or `screensaver` without an argument): `spectrum`, `bounce`,
    /// `matrix`, `stars`, `life`, or `random`.
    pub screensaver_mode: String,
    /// Prism's own tooltips (the app's look, keys for shortcuts) in place of the system's.
    pub tooltips: bool,
    /// Before a tooltip shows, in milliseconds (moving to the next one: at once).
    pub tooltip_delay: u32,
    /// A ticket row shows its thread folded into small squares (worktree, agent, MR, pipeline,
    /// environments). Off (the default): the drawer (`t`, ⋯ Thread) only.
    pub ticket_threads: bool,
    /// A two-finger vertical swipe on the trackpad, over the rail, goes to the next or previous
    /// page of the rail.
    pub page_swipe: bool,
    /// A pin in the side panes' header (MR, ticket, logs, shells…): pinned, a click outside no
    /// longer closes the pane. Off (the default): no pin, a click outside closes.
    pub pin_drawers: bool,
    /// The status bar's order: item ids, `|` parting the left side from the right; an item left
    /// out takes its default place. Empty: by zone (what waits for you, the work, the computer).
    pub statusbar: Vec<String>,
    /// The status bar's items not shown.
    pub statusbar_hidden: Vec<String>,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            theme_preview: true,
            density: "compact".into(),
            ignore_reduce: false,
            boot: true,
            animations: true,
            theme_motion: true,
            celebrations: true,
            easter_eggs: true,
            friday_guard: true,
            font: String::new(),
            font_mono: String::new(),
            font_terminal: String::new(),
            language: String::new(),
            locale: String::new(),
            screensaver: true,
            screensaver_seconds: 600,
            screensaver_mode: "spectrum".into(),
            tooltips: true,
            tooltip_delay: 400,
            ticket_threads: false,
            page_swipe: true,
            pin_drawers: false,
            statusbar: Vec::new(),
            statusbar_hidden: Vec::new(),
        }
    }
}

/// Notes (`[notes]`): quick capture over any view, sent as is to the frontend.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct NotesConfig {
    /// Opens quick capture. Same format as `[console] key`.
    pub key: String,
}

impl Default for NotesConfig {
    fn default() -> Self {
        Self { key: "Cmd+Shift+N".into() }
    }
}

/// The command palette (`[palette]`), sent as is to the frontend: one field to go to anything Prism
/// shows and to run its commands.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct PaletteConfig {
    pub enabled: bool,
    /// Its shortcut (`Cmd+K`, `Cmd+P`, `Ctrl+Space`).
    pub key: String,
    /// What is typed is also searched in the tracker, for the tickets Prism does not show (closed,
    /// another board): any ticket opens from the palette.
    pub tracker_search: bool,
}

impl Default for PaletteConfig {
    fn default() -> Self {
        Self { enabled: true, key: "Cmd+K".into(), tracker_search: true }
    }
}

/// Settings › Cleanup (`[cleanup]`): what each feature is used for, counted on this machine only
/// (the profile's usage.json, never sent), read by the window alone.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct CleanupConfig {
    /// The features turned off leave the Settings tree (a search still finds them).
    pub hide_off: bool,
}

/// The editor of an agent's Files tab (`[files]`), sent as is to the frontend.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(default)]
pub struct FilesConfig {
    /// Vim's keys in the editor (`:w` saves).
    pub vim: bool,
    /// Long lines wrapped instead of scrolled sideways.
    pub wrap: bool,
    /// Its text's size, in pixels.
    pub font_size: f32,
    /// A file open in the editor that the agent changes is read again (kept if you edited it: you choose).
    pub live_reload: bool,
    /// Lines found at most by the search in files.
    pub search_max: u32,
}

impl Default for FilesConfig {
    fn default() -> Self {
        Self { vim: false, wrap: false, font_size: 11.5, live_reload: true, search_max: 2000 }
    }
}

/// Prism console (`[console]`), sent as is to the frontend.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ConsoleConfig {
    /// Shortcut that drops it down: a character (`` ` ``, `²`, `@`), a key name (`F1`,
    /// `Backquote`, `IntlBackslash`) or a combination (`Cmd+K`, `Ctrl+Space`, `Alt+T`).
    #[serde(default = "default_console_key")]
    pub key: String,
    /// Commands kept from one launch to the next.
    #[serde(default = "default_console_history")]
    pub history: usize,
    /// Aliases: `r = "claude review"`, expanded before running.
    #[serde(default)]
    pub aliases: std::collections::BTreeMap<String, String>,
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self { key: default_console_key(), history: default_console_history(), aliases: Default::default() }
    }
}

fn default_console_history() -> usize {
    50
}

fn default_console_key() -> String {
    "`".into()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OpenWithConfig {
    /// macOS: an application name (`Zed`, `Visual Studio Code`); Linux and Windows: a command
    /// (`zed`, `code`). Absent: no button.
    #[serde(default)]
    pub editor: Option<String>,
    /// Same; by default `Terminal` (macOS), `x-terminal-emulator` (Linux), `wt` (Windows).
    #[serde(default)]
    pub terminal: Option<String>,
}
