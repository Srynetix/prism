//! The trackers: ClickUp, Jira, Obsidian, and the conventions of their statuses.

use prism_base::tr;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClickupConfig {
    /// Off: no ClickUp at all (no token needed): sprint, refinement, tickets, time entries.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Folder of the sprint lists (ClickUp Sprints), found by name in every space of the
    /// workspace. The current sprint is the one whose dates (in its name, `… (21/09 - 04/10)`, or
    /// its list's) hold today.
    #[serde(default = "default_sprint_folder")]
    pub sprint_folder: String,
    /// The team's boards beyond the sprint (`[[clickup.boards]]`): bugs to pick, a refinement
    /// queue, time-tracking tasks…
    #[serde(default)]
    pub boards: Vec<BoardConfig>,
    #[serde(default = "default_clickup_poll")]
    pub poll_seconds: u64,
    /// Statuses of the refinement queue, in flow order.
    #[serde(default = "default_refinement_statuses")]
    pub refinement_statuses: Vec<String>,
    /// "In progress or later" statuses: a ticket in one of them outside the sprint shows in the Sprint block
    /// ("Outside sprint" switch).
    #[serde(default = "default_ongoing_statuses")]
    pub ongoing_statuses: Vec<String>,
    /// Time entries on tickets (Time block, Log time). Off: none read nor offered.
    #[serde(default = "default_true")]
    pub time_tracking: bool,
    /// Days of logged time reread (Time block, date picker).
    #[serde(default = "default_time_history_days")]
    pub time_history_days: u32,
    /// Hours to log per working day (Time block); the week counts five.
    #[serde(default = "default_day_hours")]
    pub day_hours: f64,
    /// Custom fields shown on the tickets (rows and preview), by name, in this order: `Type`…
    #[serde(default)]
    pub fields: Vec<String>,
    /// Those of `fields` also offered as filters and groupings of the ticket lists (a dropdown, a
    /// label), the others only shown (an estimate in hours). Absent or empty: all of them.
    #[serde(default)]
    pub filter_fields: Option<Vec<String>>,
    /// Number custom fields holding an estimate in hours, by name (`Dev estimate (h)`…): entered
    /// from the ticket rows and preview, even when empty. Shown like `fields`, after them.
    #[serde(default)]
    pub estimate_fields: Vec<String>,
    /// Assignees shown with their ClickUp profile picture; off, or without one, their initials.
    #[serde(default = "default_true")]
    pub avatars: bool,
}

/// The tracker (`[tracker]`): where the tickets live, and the ticket conventions every provider
/// shares — identifiers, and the flow its statuses follow. Several keys were `[clickup]`'s: see
/// [`FORMER_CLICKUP_KEYS`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrackerConfig {
    /// `clickup` (the default), `obsidian` or `jira`; each is read only when its own section is on.
    #[serde(default)]
    pub provider: crate::tracker::Provider,
    /// Statuses of a ticket ready to be picked: To pick's (ClickUp's pick boards and next sprint,
    /// Obsidian's cards, which then leave the sprint).
    #[serde(default = "default_pick_statuses")]
    pub pick_statuses: Vec<String>,
    /// Ticket identifier prefix (`ABC` for `ABC-1234`), searched for in branches,
    /// MR titles, notes, the filter. Empty (default): no ticket identifiers.
    #[serde(default = "default_ticket_prefix")]
    pub ticket_prefix: String,
    /// The ticket flow, in order: it sorts statuses in Sprint and the lists.
    #[serde(default = "default_workflow_statuses")]
    pub workflow_statuses: Vec<String>,
    /// Statuses before development: an open MR on such a ticket is flagged (Sync).
    #[serde(default = "default_before_dev_statuses")]
    pub before_dev_statuses: Vec<String>,
    /// "In progress" statuses: such a ticket with neither branch nor MR for long is flagged.
    #[serde(default = "default_in_progress_statuses")]
    pub in_progress_statuses: Vec<String>,
    /// Review statuses: the ticket is waiting for its MR; the first one is offered when an MR opens
    /// on a ticket from before development.
    #[serde(default = "default_review_statuses")]
    pub review_statuses: Vec<String>,
    /// Statuses of a ticket whose open MRs are approved: a ticket still in review
    /// (`review_statuses`, not among these) is then flagged, and the first one is offered.
    #[serde(default = "default_approved_statuses")]
    pub approved_statuses: Vec<String>,
    /// Statuses of a ticket whose open MRs go into the merge queue: one not there (in a repository
    /// with a merge queue, `[gitlab] merge_queue_label`) is flagged. Empty: no such rule.
    #[serde(default)]
    pub merge_queue_statuses: Vec<String>,
    /// Statuses after the merge: a merged MR on a ticket not in them is flagged.
    #[serde(default = "default_after_merge_statuses")]
    pub after_merge_statuses: Vec<String>,
    /// Statuses of a finished ticket (a closed ticket always is).
    #[serde(default = "default_done_statuses")]
    pub done_statuses: Vec<String>,
    /// Statuses of a blocked ticket: no Sync rule flags it (MR, branch, status).
    #[serde(default = "default_blocked_statuses")]
    pub blocked_statuses: Vec<String>,
    /// Validation statuses: on a ticket in one of them, "Validate locally"; launched from the
    /// first, the ticket moves to the last and is assigned to me. Empty: no such action.
    #[serde(default = "default_validation_statuses")]
    pub validation_statuses: Vec<String>,
    /// Words that stand for the ticket prefix in the branches of Claude sessions, besides
    /// `ticket` and the prefix itself: `claude/<word>-512-…` is ticket 512 (a skill plugin
    /// naming its branches after itself).
    #[serde(default)]
    pub branch_aliases: Vec<String>,
    /// The tracker's own estimate (ClickUp's time estimate, Jira's original estimate) entered from
    /// Prism, in hours (the default). Off: only shown, for a team estimating in custom fields
    /// (`[clickup] estimate_fields`).
    #[serde(default = "default_true")]
    pub estimate_builtin: bool,
    /// A ticket's actual cost against its estimate: the Actual section of the ticket panel, and
    /// the Actuals of My sprint (the sprint's done tickets, their gap, the trend by type and estimate).
    #[serde(default = "default_true")]
    pub actuals: bool,
    /// The card at the top of the ticket panel: where the ticket stands and the one action that moves it on.
    #[serde(default = "default_true")]
    pub next_action: bool,
    /// The actual time is the time logged only; off (the default), the agents' active time (working,
    /// not waiting for you) is added to it.
    #[serde(default)]
    pub actuals_logged_only: bool,
    /// Where a ticket's type comes from, for the trends of the Actuals: empty (the default), the
    /// tracker's task type (ClickUp's custom task type, Jira's issue type); `tags`, its first tag
    /// (Jira: label); otherwise the name of a custom field of `[clickup] fields`.
    #[serde(default)]
    pub ticket_type: String,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        toml::from_str("").expect("an empty [tracker]")
    }
}

/// The keys of `[tracker]` that were `[clickup]`'s.
pub const FORMER_CLICKUP_KEYS: [&str; 12] = [
    "ticket_prefix",
    "pick_statuses",
    "workflow_statuses",
    "before_dev_statuses",
    "in_progress_statuses",
    "review_statuses",
    "approved_statuses",
    "after_merge_statuses",
    "done_statuses",
    "blocked_statuses",
    "validation_statuses",
    "branch_aliases",
];

/// Obsidian Kanban as the tracker (`[obsidian]`): boards of a vault, each a repository's.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ObsidianConfig {
    /// The vault's folder; its name opens the links (`obsidian://open?vault=…`).
    #[serde(default)]
    pub vault: String,
    /// The boards read (`[[obsidian.boards]]`).
    #[serde(default)]
    pub boards: Vec<ObsidianBoard>,
}

/// Jira Cloud as the tracker (`[jira]`): the active sprint of a Scrum board, or the open issues of
/// a Kanban one. The token comes from JIRA_TOKEN or the system keychain, never this file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JiraConfig {
    /// The Jira Cloud site: `team.atlassian.net`.
    #[serde(default)]
    pub site: String,
    /// The Atlassian account the API token belongs to.
    #[serde(default)]
    pub email: String,
    /// The board whose active sprint is read (its id, in the board's URL: `…/boards/42`).
    #[serde(default)]
    pub board: u64,
    /// The issues followed beyond the sprint (`[[jira.queues]]`): To pick, Refinement, Sync.
    #[serde(default)]
    pub queues: Vec<JiraQueue>,
    /// The Refinement block's statuses, one column each, in order. Empty: every open status its
    /// queues' issues are in.
    #[serde(default)]
    pub refinement_statuses: Vec<String>,
    /// In progress or further: the board's issues in these statuses outside the active sprint show
    /// in Sprint. Empty: Jira's "In Progress" category.
    #[serde(default)]
    pub ongoing_statuses: Vec<String>,
    /// The type of an issue created from Prism (Sprint's +), as the board's project names it.
    #[serde(default = "default_jira_issue_type")]
    pub issue_type: String,
    /// Worklogs: my time logged on issues (Time block, Log time). Off: none read nor offered.
    #[serde(default)]
    pub time_tracking: bool,
    #[serde(default = "default_jira_poll")]
    pub poll_seconds: u64,
}

fn default_jira_issue_type() -> String {
    "Task".into()
}

/// Issues followed beyond the sprint (`[[jira.queues]]`): those a JQL query finds.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JiraQueue {
    /// Its name in Prism.
    pub name: String,
    /// The query: `project = ABC AND type = Bug AND resolution = Unresolved`.
    pub jql: String,
    /// `pick`: To pick (its issues in `[tracker] pick_statuses`); `refinement`: the Refinement
    /// queue; `sync`: read whole, its issues checked against their MRs like the sprint's.
    #[serde(default)]
    pub uses: Vec<String>,
    /// To pick's colour: `ko`, `warn` (before the next sprint), `info`, `violet`, `ok`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tone: String,
}

impl JiraQueue {
    pub fn uses(&self, what: &str) -> bool {
        self.uses.iter().any(|u| u == what)
    }
}

impl Default for JiraConfig {
    fn default() -> Self {
        toml::from_str("").expect("JiraConfig defaults")
    }
}

impl JiraConfig {
    /// `https://team.atlassian.net`, whatever form the site was written in.
    pub fn base_url(&self) -> String {
        let site = self.site.trim().trim_end_matches('/');
        let site = site.strip_prefix("https://").or_else(|| site.strip_prefix("http://")).unwrap_or(site);
        format!("https://{site}")
    }

    /// The site is an Atlassian Cloud one (`team.atlassian.net`, or a former `team.jira.com`):
    /// the token goes there, so a site written elsewhere — by a pack, `team.atlassian.net@other.host`
    /// — sends it nowhere.
    pub fn check_site(&self) -> anyhow::Result<()> {
        let url = self.base_url();
        let host = url.trim_start_matches("https://");
        let bare = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        let cloud = [".atlassian.net", ".jira.com"].iter().any(|d| host.to_ascii_lowercase().ends_with(d));
        anyhow::ensure!(bare && cloud, tr!("[jira] site: an Atlassian Cloud site (team.atlassian.net), not {host}", host = format!("{host:?}")));
        Ok(())
    }
}

fn default_jira_poll() -> u64 {
    60
}

/// An Obsidian Kanban board: a Markdown file of the vault, and the repository its tickets belong to.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObsidianBoard {
    /// Path of the board's file, relative to the vault (`Projets/Things/Board.md`).
    pub path: String,
    /// The repository (`[[repos]] name`) its tickets are worked on in: their identifiers are
    /// resolved in it, and it names the board in Prism. Absent: the file's folder names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The prefix of its ticket ids, as a pattern (`T`, `[ABCLM]`): Prism adds `-<number>`. Several
    /// letters, classes and alternatives; no capturing group. Absent: `[tracker] ticket_prefix`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

impl ObsidianConfig {
    /// The id prefixes its boards add to `[tracker] ticket_prefix`, as patterns.
    pub fn prefixes(&self) -> Vec<String> {
        self.boards.iter().filter_map(|b| b.prefix.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(String::from)).collect()
    }
}

/// A board's id prefix: a pattern that compiles, without a capturing group (the number is one).
pub fn check_prefix(pattern: &str) -> Result<()> {
    let re = regex::Regex::new(&format!("^(?:{pattern})$")).with_context(|| tr!("[[obsidian.boards]] prefix “{pattern}” is not a valid pattern", pattern = pattern))?;
    anyhow::ensure!(re.captures_len() == 1, tr!("[[obsidian.boards]] prefix “{pattern}”: no capturing group, (?:…) instead", pattern = pattern));
    anyhow::ensure!(!re.is_match(""), tr!("[[obsidian.boards]] prefix “{pattern}” matches nothing at all", pattern = pattern));
    Ok(())
}

/// A ClickUp board followed beyond the sprint (`[[clickup.boards]]`): a list, or the lists of a
/// folder, found by name, and what Prism uses it for.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct BoardConfig {
    /// Its name in Prism; also the list looked for, without `list` nor `folder`.
    pub name: String,
    /// The ClickUp list, by name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub list: String,
    /// Or every list of this ClickUp folder.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    /// With `folder`: only its quarterly lists (`Q4 2026`), of the current and coming quarters.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub quarters: bool,
    /// `pick`: To pick (its tickets in `pick_statuses`); `refinement`: the Refinement queue;
    /// `sync`: read whole, its tickets checked against their MRs like the sprint's (Alerts);
    /// `time`: tasks to log time on (Time block).
    #[serde(default)]
    pub uses: Vec<String>,
    /// To pick's colour: `ko`, `warn` (before the next sprint), `info`, `violet`, `ok`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tone: String,
}

impl BoardConfig {
    pub fn uses(&self, what: &str) -> bool {
        self.uses.iter().any(|u| u == what)
    }

    /// The list looked for, when it is not a folder.
    pub fn list_name(&self) -> &str {
        if self.list.is_empty() { &self.name } else { &self.list }
    }
}

impl Default for ClickupConfig {
    /// Every key has its default: an absent `[clickup]` section.
    fn default() -> Self {
        toml::from_str("").expect("ClickupConfig defaults")
    }
}

fn default_ticket_prefix() -> String {
    String::new()
}

fn statuses(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn default_workflow_statuses() -> Vec<String> {
    statuses(&["to do", "in progress", "review", "complete"])
}

fn default_before_dev_statuses() -> Vec<String> {
    statuses(&["to do"])
}

fn default_in_progress_statuses() -> Vec<String> {
    statuses(&["in progress"])
}

fn default_review_statuses() -> Vec<String> {
    statuses(&["review"])
}

fn default_approved_statuses() -> Vec<String> {
    Vec::new()
}

fn default_after_merge_statuses() -> Vec<String> {
    statuses(&["complete"])
}

fn default_done_statuses() -> Vec<String> {
    statuses(&["complete"])
}

fn default_blocked_statuses() -> Vec<String> {
    statuses(&["blocked"])
}

fn default_validation_statuses() -> Vec<String> {
    Vec::new()
}

fn default_day_hours() -> f64 {
    8.0
}

fn default_time_history_days() -> u32 {
    30
}

fn default_ongoing_statuses() -> Vec<String> {
    ["in progress", "review"].map(String::from).to_vec()
}

fn default_pick_statuses() -> Vec<String> {
    ["to do"].map(String::from).to_vec()
}

fn default_refinement_statuses() -> Vec<String> {
    ["to do"].map(String::from).to_vec()
}

fn default_sprint_folder() -> String {
    "Sprints".into()
}

fn default_clickup_poll() -> u64 {
    45
}
