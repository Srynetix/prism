//! Team and repository conventions, taken from the config at launch: ticket prefix,
//! GitLab host, main branch and release branches, ClickUp flow statuses. The
//! collectors read them here rather than each receiving them; the snapshot carries them to the frontend.

use std::sync::{Arc, RwLock};

use serde::Serialize;

use crate::config::Config;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Conventions {
    pub ticket_prefix: String,
    /// Every ticket id prefix, as patterns: `ticket_prefix`'s, and those of the Obsidian boards.
    pub ticket_patterns: Vec<String>,
    /// The forge's provider (`[forge] provider`).
    pub forge: crate::forge::Provider,
    pub gitlab_host: String,
    pub main_branch: String,
    pub release_prefix: String,
    pub workflow_statuses: Vec<String>,
    pub before_dev_statuses: Vec<String>,
    pub in_progress_statuses: Vec<String>,
    pub review_statuses: Vec<String>,
    pub approved_statuses: Vec<String>,
    pub merge_queue_statuses: Vec<String>,
    pub after_merge_statuses: Vec<String>,
    pub done_statuses: Vec<String>,
    pub blocked_statuses: Vec<String>,
    pub time_history_days: u32,
    /// Hours to log per working day (`[clickup] day_hours`).
    pub day_hours: f64,
    pub validation_statuses: Vec<String>,
    /// Statuses of a ticket ready to be picked (`[tracker] pick_statuses`).
    pub pick_statuses: Vec<String>,
    /// Words standing for the ticket prefix in Claude sessions' branches (`[tracker] branch_aliases`).
    pub branch_aliases: Vec<String>,
    /// The tracker's own estimate is entered from Prism (`[tracker] estimate_builtin`).
    pub estimate_builtin: bool,
    /// Custom fields holding an estimate in hours, entered from Prism (`[clickup] estimate_fields`; ClickUp only).
    pub estimate_fields: Vec<String>,
    /// Assignees with their profile picture (`[clickup] avatars`).
    pub avatars: bool,
    /// MR authors and members with their profile picture (`[forge] avatars`).
    pub forge_avatars: bool,
    /// A ticket's actual cost against its estimate (`[tracker] actuals`).
    pub actuals: bool,
    /// The ticket panel's next action card (`[tracker] next_action`).
    pub next_action: bool,
    /// The actual time is the time logged only, without the agents' (`[tracker] actuals_logged_only`).
    pub actuals_logged_only: bool,
    /// Where a ticket's type comes from (`[tracker] ticket_type`): empty, the task type; `tags`; or a field's name.
    pub ticket_type: String,
}

impl Default for Conventions {
    /// Those of an empty config (and of tests).
    fn default() -> Self {
        let cfg: Config = toml::from_str("[clickup]\n").expect("an empty config");
        Self::of(&cfg)
    }
}

impl Conventions {
    pub fn of(cfg: &Config) -> Self {
        let c = &cfg.clickup;
        let t = &cfg.tracker;
        // The primary repository's branches; an empty main branch is the remote's, read from its
        // clone as Prism starts, otherwise `main`.
        let (main_branch, release_prefix) = cfg.primary_branches();
        let main_branch = if main_branch.is_empty() { "main".into() } else { main_branch };
        Self {
            ticket_prefix: t.ticket_prefix.trim().trim_end_matches('-').to_string(),
            ticket_patterns: ticket_patterns(cfg),
            forge: cfg.forge.provider,
            gitlab_host: cfg.gitlab.host.trim().trim_start_matches("https://").trim_end_matches('/').to_string(),
            main_branch,
            release_prefix,
            workflow_statuses: t.workflow_statuses.clone(),
            before_dev_statuses: t.before_dev_statuses.clone(),
            in_progress_statuses: t.in_progress_statuses.clone(),
            review_statuses: t.review_statuses.clone(),
            approved_statuses: t.approved_statuses.clone(),
            merge_queue_statuses: t.merge_queue_statuses.clone(),
            after_merge_statuses: t.after_merge_statuses.clone(),
            done_statuses: t.done_statuses.clone(),
            blocked_statuses: t.blocked_statuses.clone(),
            time_history_days: c.time_history_days,
            day_hours: if c.day_hours > 0.0 { c.day_hours } else { 8.0 },
            validation_statuses: t.validation_statuses.clone(),
            pick_statuses: t.pick_statuses.clone(),
            branch_aliases: t.branch_aliases.iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect(),
            estimate_builtin: t.estimate_builtin && t.provider != crate::tracker::Provider::Obsidian,
            estimate_fields: if t.provider == crate::tracker::Provider::Clickup { c.estimate_fields.iter().map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect() } else { Vec::new() },
            avatars: c.avatars,
            forge_avatars: cfg.forge.avatars,
            actuals: t.actuals,
            next_action: t.next_action,
            actuals_logged_only: t.actuals_logged_only,
            ticket_type: t.ticket_type.trim().to_string(),
        }
    }

    /// These conventions with what applies live taken from `read` (a config just saved): the flow
    /// statuses, the estimates, the avatars and the actuals. The rest (prefix, host, branches…) stays as configured at
    /// launch. Every field is named: a new one must say which kind it is.
    pub fn with_live(&self, read: Conventions) -> Conventions {
        let Conventions { ticket_prefix: _, ticket_patterns: _, forge: _, gitlab_host: _, main_branch: _, release_prefix: _, time_history_days: _, day_hours: _, branch_aliases: _, workflow_statuses, before_dev_statuses, in_progress_statuses, review_statuses, approved_statuses, merge_queue_statuses, after_merge_statuses, done_statuses, blocked_statuses, validation_statuses, pick_statuses, estimate_builtin, estimate_fields, avatars, forge_avatars, actuals, next_action, actuals_logged_only, ticket_type } = read;
        Conventions { workflow_statuses, before_dev_statuses, in_progress_statuses, review_statuses, approved_statuses, merge_queue_statuses, after_merge_statuses, done_statuses, blocked_statuses, validation_statuses, pick_statuses, estimate_builtin, estimate_fields, avatars, forge_avatars, actuals, next_action, actuals_logged_only, ticket_type, ..self.clone() }
    }
}

/// The prefix (escaped) then, on Obsidian, its boards' patterns: each once.
fn ticket_patterns(cfg: &Config) -> Vec<String> {
    let prefix = cfg.tracker.ticket_prefix.trim().trim_end_matches('-');
    let boards = if cfg.tracker.provider == crate::tracker::Provider::Obsidian { cfg.obsidian.prefixes() } else { Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for p in std::iter::once(regex::escape(prefix)).filter(|p| !p.is_empty()).chain(boards) {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

static CURRENT: RwLock<Option<Arc<Conventions>>> = RwLock::new(None);

/// At launch, before the collectors; again when a saved config changes what applies live
/// (`commands::config::apply_live`).
pub fn set(c: Conventions) {
    *CURRENT.write().unwrap() = Some(Arc::new(c));
}

/// The current conventions: read them again on each pass, they may change while Prism runs.
pub fn get() -> Arc<Conventions> {
    CURRENT.read().unwrap().clone().unwrap_or_default()
}

/// Most recent release branch of the primary repository (Git Flow), otherwise the main branch.
pub async fn latest_release(repo: &std::path::Path) -> anyhow::Result<String> {
    let c = get();
    latest_release_in(repo, &c.main_branch, &c.release_prefix).await
}

/// Most recent release branch of a repository (`release_prefix`), otherwise `main`; without a
/// release prefix (no Git Flow), `main`.
pub async fn latest_release_in(repo: &std::path::Path, main: &str, release_prefix: &str) -> anyhow::Result<String> {
    if release_prefix.is_empty() {
        return Ok(main.to_string());
    }
    let pattern = format!("refs/remotes/origin/{release_prefix}");
    let refs = crate::git::run(repo, &["for-each-ref", "--sort=-creatordate", "--format=%(refname:short)", &pattern]).await?;
    Ok(refs.lines().next().and_then(|r| r.strip_prefix("origin/")).unwrap_or(main).to_string())
}

/// `search` parameter of the GitLab API listing the release branches: `^release/`, encoded.
pub fn release_search() -> String {
    crate::http::encode(&format!("^{}", get().release_prefix))
}

/// Link to the GitLab instance: `web("/group/p/-/jobs/1")` or `web("group/p")`.
pub fn web(path: &str) -> String {
    format!("https://{}/{}", get().gitlab_host, path.trim_start_matches('/'))
}

/// Image registry of the forge: the GitLab instance's, or GitHub's (`ghcr.io`).
#[allow(dead_code, reason = "for the blocks of another repository (`blocks::sdk`)")]
pub fn registry() -> String {
    match get().forge {
        crate::forge::Provider::Gitlab => format!("registry.{}", get().gitlab_host),
        crate::forge::Provider::Github => "ghcr.io".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A saved config changes the flow statuses right away; the prefix and the branches stay as
    /// launched.
    #[test]
    fn a_saved_config_applies_its_statuses_only() {
        let launched = Conventions::of(&crate::config::parsed("[clickup]\nticket_prefix = \"ACME-\"\n"));
        let saved = Conventions::of(&crate::config::parsed("[forge]\nmain_branch = \"master\"\n\n[clickup]\nticket_prefix = \"NEW-\"\ndone_statuses = [\"shipped\"]\n"));
        let next = launched.with_live(saved);
        assert_eq!((next.ticket_prefix.as_str(), next.main_branch.as_str()), ("ACME", launched.main_branch.as_str()));
        assert_eq!(next.done_statuses, ["shipped"]);
    }

    /// Another team: its prefix, its GitLab instance, its branches, its statuses.
    #[test]
    fn read_from_the_config() {
        let cfg = crate::config::parsed(
            "[gitlab]\nhost = \"https://git.acme.io/\"\n\n[forge]\nmain_branch = \"master\"\nrelease_prefix = \"rel-\"\n\n[clickup]\nticket_prefix = \"ACME-\"\ndone_statuses = [\"shipped\"]\n",
        );
        let c = Conventions::of(&cfg);
        assert_eq!((c.ticket_prefix.as_str(), c.gitlab_host.as_str(), c.main_branch.as_str(), c.release_prefix.as_str()), ("ACME", "git.acme.io", "master", "rel-"));
        assert_eq!(c.done_statuses, ["shipped"]);
        let d = Conventions::default();
        assert_eq!((d.ticket_prefix.as_str(), d.gitlab_host.as_str(), d.release_prefix.as_str()), ("", "gitlab.com", ""));
        assert_eq!(d.after_merge_statuses, ["complete"]);
        assert!(d.approved_statuses.is_empty() && d.validation_statuses.is_empty());
    }
}
