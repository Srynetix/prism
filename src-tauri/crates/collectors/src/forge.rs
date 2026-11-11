//! The active profile's forge, whichever its provider (`[forge] provider`): what the window asks
//! of it goes to the provider's own actions, or to the other forge's for the MRs of the repositories
//! there (`[[repos]] forge`). Its types are the core's (`prism_core::forge`).

use prism_base::i18n::tr;
use std::sync::Arc;

use anyhow::{bail, Result};

pub use prism_core::forge::*;

pub mod branches;
pub mod elsewhere;
pub mod poll;
pub mod shown;

use crate::{
    {github, gitlab},
    state::Hub,
};

/// Methods each forge's actions have, with the same arguments: the active one's.
macro_rules! delegate {
    ($($(#[$doc:meta])* fn $name:ident(&self $(, $arg:ident: $ty:ty)*) -> $ret:ty;)*) => {
        $(
            $(#[$doc])*
            pub async fn $name(&self $(, $arg: $ty)*) -> $ret {
                match self {
                    Self::Gitlab(g) => g.$name($($arg),*).await,
                    Self::Github(g) => g.$name($($arg),*).await,
                }
            }
        )*
    };
}

/// One forge's actions.
#[derive(Clone)]
pub enum Actions {
    Gitlab(gitlab::Actions),
    Github(github::Actions),
}

impl Actions {
    pub fn provider(&self) -> Provider {
        match self {
            Self::Gitlab(_) => Provider::Gitlab,
            Self::Github(_) => Provider::Github,
        }
    }

    /// What only GitLab offers.
    pub fn gitlab(&self) -> Result<&gitlab::Actions> {
        match self {
            Self::Gitlab(g) => Ok(g),
            Self::Github(_) => bail!("{}", tr("only on GitLab")),
        }
    }
}

impl Actions {
    delegate! {
        /* ---------- Merge requests ---------- */
        fn rebase_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<()>;
        fn merge_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<()>;
        fn set_auto_merge(&self, hub: &Hub, repo: Option<&str>, iid: u64, enable: bool) -> Result<()>;
        fn mr_detail(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<MrDetail>;
        fn mr_diff(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<MrDiff>;
        fn mr_commits(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<Vec<MrCommit>>;
        fn mr_file(&self, hub: &Hub, repo: Option<&str>, iid: u64, path: &str) -> Result<String>;
        fn mr_image(&self, hub: &Hub, repo: Option<&str>, iid: u64, path: &str, old: bool) -> Result<String>;
        fn approve_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64, on: bool) -> Result<()>;

        /// Requests changes, with `body` as its comment (needed on GitHub), or takes the request back (GitLab only).
        fn request_changes(&self, hub: &Hub, repo: Option<&str>, iid: u64, on: bool, body: &str) -> Result<()>;
        fn comment_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64, discussion: Option<&str>, body: &str) -> Result<()>;
        fn comment_line(&self, hub: &Hub, repo: Option<&str>, iid: u64, line: LineRef, body: &str) -> Result<()>;
        fn resolve_thread(&self, hub: &Hub, repo: Option<&str>, iid: u64, discussion: &str, resolved: bool) -> Result<()>;
        fn set_draft(&self, hub: &Hub, repo: Option<&str>, iid: u64, draft: bool) -> Result<()>;
        fn members(&self, repo: Option<&str>) -> Result<Vec<Member>>;
        fn set_people(&self, hub: &Hub, repo: Option<&str>, iid: u64, reviewers: bool, usernames: Vec<String>) -> Result<()>;
        fn labels(&self, repo: Option<&str>) -> Result<Vec<ForgeLabel>>;
        /// An open MR's labels, the whole list; only those that move are sent.
        fn set_labels(&self, hub: &Hub, repo: Option<&str>, iid: u64, labels: Vec<String>) -> Result<()>;

        /// An image attached to a text of the forge (description, comment), as a `data:` URL.
        fn upload(&self, project: Option<&str>, path: &str) -> Result<String>;

        /* ---------- Renovate ---------- */
        fn play_renovate(&self, hub: &Hub) -> Result<()>;
        fn check_renovate(&self, hub: &Hub, marker: &str) -> Result<()>;

        /* ---------- Pipelines and jobs ---------- */

        /// Plays a manual job: GitLab's, or, on GitHub, approves the deployment a job waits for.
        fn play_job(&self, hub: &Hub, id: u64) -> Result<()>;
        fn cancel_pipeline(&self, hub: &Hub, id: u64) -> Result<()>;
        fn retry_pipeline(&self, hub: &Hub, id: u64) -> Result<()>;
        fn pipeline_jobs(&self, project: Option<&str>, pipeline: u64) -> Result<Vec<JobInfo>>;
        fn retry_job(&self, project: Option<&str>, job: u64) -> Result<JobInfo>;
        fn job_log(&self, project: Option<&str>, job: u64) -> Result<JobLog>;
    }
}

/// Methods of `Forge`, each sent to the forge `$pick` names from its arguments.
macro_rules! route {
    ($($(#[$doc:meta])* fn $name:ident(&self $(, $arg:ident: $ty:ty)*) -> $ret:ty => $pick:ident($($key:expr),*);)*) => {
        $(
            $(#[$doc])*
            pub async fn $name(&self $(, $arg: $ty)*) -> $ret {
                self.$pick($($key),*).$name($($arg),*).await
            }
        )*
    };
}

/// Methods of `Forge` that change the forge: as `route!`, then its reads come again at once
/// (`Hub::forge_changed`) when the action went through.
macro_rules! route_changing {
    ($($(#[$doc:meta])* fn $name:ident(&self, $hub:ident: &Hub $(, $arg:ident: $ty:ty)*) -> $ret:ty => $pick:ident($($key:expr),*);)*) => {
        $(
            $(#[$doc])*
            pub async fn $name(&self, $hub: &Hub $(, $arg: $ty)*) -> $ret {
                let done = self.$pick($($key),*).$name($hub $(, $arg)*).await;
                if done.is_ok() {
                    $hub.forge_changed();
                }
                done
            }
        )*
    };
}

/// The profile's forge, and the other one when repositories are there (`[[repos]] forge`): the
/// actions on their MRs, and on what those MRs show (pipelines, jobs, images), go to it.
#[derive(Clone)]
pub struct Forge {
    main: Actions,
    elsewhere: Option<Elsewhere>,
}

#[derive(Clone)]
struct Elsewhere {
    actions: Actions,
    /// (repository, project).
    repos: Arc<Vec<(String, String)>>,
}

impl Forge {
    pub fn new(main: Actions) -> Self {
        Self { main, elsewhere: None }
    }

    /// With the other forge's actions, for the repositories there whose MRs are followed.
    pub fn with_elsewhere(self, actions: Actions, repos: Vec<(String, String)>) -> Self {
        Self { elsewhere: (!repos.is_empty()).then(|| Elsewhere { actions, repos: Arc::new(repos) }), ..self }
    }

    /// The profile's provider.
    pub fn provider(&self) -> Provider {
        self.main.provider()
    }

    /// What only GitLab offers, on the profile's forge.
    pub fn gitlab(&self) -> Result<&gitlab::Actions> {
        self.main.gitlab()
    }

    /// The forge of a repository's MRs (`None`: the primary one, always the profile's).
    fn of_repo(&self, repo: Option<&str>) -> &Actions {
        match (&self.elsewhere, repo) {
            (Some(e), Some(r)) if e.repos.iter().any(|(name, _)| name == r) => &e.actions,
            _ => &self.main,
        }
    }

    /// The forge of a project (`None`: the primary repository's).
    fn of_project(&self, project: Option<&str>) -> &Actions {
        match (&self.elsewhere, project) {
            (Some(e), Some(p)) if e.repos.iter().any(|(_, project)| project == p) => &e.actions,
            _ => &self.main,
        }
    }

    /// The forge of a pipeline Prism displays: that of the MR carrying it, if it is elsewhere.
    fn of_pipeline(&self, hub: &Hub, id: u64) -> &Actions {
        let repo = hub.subscribe().borrow().merge_requests.iter().find(|m| m.forge.is_some() && m.head_pipeline.as_ref().is_some_and(|p| p.id == id)).and_then(|m| m.repo.clone());
        self.of_repo(repo.as_deref())
    }

    /// The forge of a manual job Prism displays: that of the MR whose pipeline carries it, if it is elsewhere.
    fn of_job(&self, hub: &Hub, id: u64) -> &Actions {
        let has = |p: &crate::state::Pipeline| p.stages.iter().any(|s| s.manual_jobs.iter().any(|j| j.id == id));
        let repo = hub.subscribe().borrow().merge_requests.iter().find(|m| m.forge.is_some() && m.head_pipeline.as_ref().is_some_and(has)).and_then(|m| m.repo.clone());
        self.of_repo(repo.as_deref())
    }

    fn main(&self) -> &Actions {
        &self.main
    }

    route_changing! {
        /* ---------- Merge requests ---------- */
        fn rebase_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<()> => of_repo(repo);
        fn merge_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<()> => of_repo(repo);
        fn set_auto_merge(&self, hub: &Hub, repo: Option<&str>, iid: u64, enable: bool) -> Result<()> => of_repo(repo);
        fn approve_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64, on: bool) -> Result<()> => of_repo(repo);
        /// Requests changes, with `body` as its comment (needed on GitHub), or takes the request back (GitLab only).
        fn request_changes(&self, hub: &Hub, repo: Option<&str>, iid: u64, on: bool, body: &str) -> Result<()> => of_repo(repo);
        fn comment_mr(&self, hub: &Hub, repo: Option<&str>, iid: u64, discussion: Option<&str>, body: &str) -> Result<()> => of_repo(repo);
        fn comment_line(&self, hub: &Hub, repo: Option<&str>, iid: u64, line: LineRef, body: &str) -> Result<()> => of_repo(repo);
        fn resolve_thread(&self, hub: &Hub, repo: Option<&str>, iid: u64, discussion: &str, resolved: bool) -> Result<()> => of_repo(repo);
        fn set_draft(&self, hub: &Hub, repo: Option<&str>, iid: u64, draft: bool) -> Result<()> => of_repo(repo);
        fn set_people(&self, hub: &Hub, repo: Option<&str>, iid: u64, reviewers: bool, usernames: Vec<String>) -> Result<()> => of_repo(repo);
        /// An open MR's labels, the whole list; only those that move are sent.
        fn set_labels(&self, hub: &Hub, repo: Option<&str>, iid: u64, labels: Vec<String>) -> Result<()> => of_repo(repo);

        /* ---------- Renovate: in its repository (`[renovate] repo`) ---------- */
        fn play_renovate(&self, hub: &Hub) -> Result<()> => main();
        fn check_renovate(&self, hub: &Hub, marker: &str) -> Result<()> => main();

        /* ---------- Pipelines and jobs ---------- */
        /// Plays a manual job: GitLab's, or, on GitHub, approves the deployment a job waits for.
        fn play_job(&self, hub: &Hub, id: u64) -> Result<()> => of_job(hub, id);
        fn cancel_pipeline(&self, hub: &Hub, id: u64) -> Result<()> => of_pipeline(hub, id);
        fn retry_pipeline(&self, hub: &Hub, id: u64) -> Result<()> => of_pipeline(hub, id);
    }

    route! {
        fn mr_detail(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<MrDetail> => of_repo(repo);
        fn mr_diff(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<MrDiff> => of_repo(repo);
        fn mr_commits(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<Vec<MrCommit>> => of_repo(repo);
        fn mr_file(&self, hub: &Hub, repo: Option<&str>, iid: u64, path: &str) -> Result<String> => of_repo(repo);
        fn mr_image(&self, hub: &Hub, repo: Option<&str>, iid: u64, path: &str, old: bool) -> Result<String> => of_repo(repo);
        fn members(&self, repo: Option<&str>) -> Result<Vec<Member>> => of_repo(repo);
        fn labels(&self, repo: Option<&str>) -> Result<Vec<ForgeLabel>> => of_repo(repo);
        /// An image attached to a text of the forge (description, comment), as a `data:` URL.
        fn upload(&self, project: Option<&str>, path: &str) -> Result<String> => of_project(project);
        fn pipeline_jobs(&self, project: Option<&str>, pipeline: u64) -> Result<Vec<JobInfo>> => of_project(project);
        fn job_log(&self, project: Option<&str>, job: u64) -> Result<JobLog> => of_project(project);
    }

    /// Runs a new pipeline for an open MR (GitLab only: a merge request pipeline); the forge's
    /// reads come again at once when it went through.
    pub async fn run_mr_pipeline(&self, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<()> {
        self.of_repo(repo).gitlab()?.run_mr_pipeline(hub, repo, iid).await?;
        hub.forge_changed();
        Ok(())
    }

    /// The custom emoji of a forge text's project (`project`: as `upload`'s): its GitLab groups';
    /// none on GitHub, which has no such thing.
    pub async fn custom_emoji(&self, project: Option<&str>) -> Result<Vec<CustomEmoji>> {
        match self.of_project(project) {
            Actions::Gitlab(g) => g.custom_emoji(project).await,
            Actions::Github(_) => Ok(Vec::new()),
        }
    }

    /// Retries a job; the forge's reads come again at once when it went through.
    pub async fn retry_job(&self, hub: &Hub, project: Option<&str>, job: u64) -> Result<JobInfo> {
        let done = self.of_project(project).retry_job(project, job).await;
        if done.is_ok() {
            hub.forge_changed();
        }
        done
    }
}

/// Hosts a profile picture is read from: GitHub's, Gravatar (GitLab's default), and the GitLab
/// instance (`gitlab_host`, an uploaded picture).
fn avatar_host(host: &str, gitlab_host: &str) -> bool {
    matches!(host, "avatars.githubusercontent.com" | "secure.gravatar.com" | "www.gravatar.com" | "gravatar.com") || (!gitlab_host.is_empty() && host == gitlab_host)
}

/// Pixels a profile picture is asked at: shown at 16 to 32, on a screen of twice that.
const AVATAR_PX: u32 = 64;

/// `url` asking for a picture `px` wide: GitHub and Gravatar by `s`, GitLab by `width` (an uploaded
/// one comes at its own size otherwise, often a few hundred kB).
fn sized(url: &mut reqwest::Url, px: u32) {
    let key = if url.host_str().is_some_and(|h| h.ends_with("githubusercontent.com") || h.ends_with("gravatar.com")) { "s" } else { "width" };
    let kept: Vec<(String, String)> = url.query_pairs().filter(|(k, _)| k != key && k != "size").map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    url.query_pairs_mut().clear().extend_pairs(kept).append_pair(key, &px.to_string());
}

/// A person's profile picture on the forge (an MR's author, a member), as a `data:` URL for the
/// window, whose CSP allows no external image. Read without authentication (the token stays
/// with `glab`/`gh`): an instance that hides them behind its login keeps the initials.
pub async fn avatar(url: &str, gitlab_host: &str) -> Result<String> {
    let allowed = |u: &reqwest::Url| u.scheme() == "https" && u.host_str().is_some_and(|h| avatar_host(h, gitlab_host));
    let mut parsed = reqwest::Url::parse(url)?;
    if !allowed(&parsed) {
        bail!("not a forge avatar: {}", parsed.host_str().unwrap_or("?"));
    }
    sized(&mut parsed, AVATAR_PX);
    // Gravatar and GitHub redirect to a sized picture: followed only among those hosts.
    let host = gitlab_host.to_string();
    let policy = reqwest::redirect::Policy::custom(move |a| if a.previous().len() < 3 && a.url().scheme() == "https" && a.url().host_str().is_some_and(|h| avatar_host(h, &host)) { a.follow() } else { a.stop() });
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).redirect(policy).build()?;
    let resp = http.get(parsed).send().await?;
    if !resp.status().is_success() {
        bail!("avatar: HTTP {}", resp.status());
    }
    image_data_url(&resp.bytes().await?)
}

/// A GitLab custom emoji's picture (`custom_emoji`), as a `data:` URL, read as a profile picture
/// (unauthenticated) and only on the instance: one a group linked from elsewhere stays its name.
pub async fn emoji_image(url: &str, gitlab_host: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(url)?;
    if parsed.scheme() != "https" || gitlab_host.is_empty() || parsed.host_str() != Some(gitlab_host) {
        bail!("not an emoji of the GitLab instance: {}", parsed.host_str().unwrap_or("?"));
    }
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).redirect(reqwest::redirect::Policy::none()).build()?;
    let resp = http.get(parsed).send().await?;
    if !resp.status().is_success() {
        bail!("emoji: HTTP {}", resp.status());
    }
    image_data_url(&resp.bytes().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_forge_avatar_hosts_are_read() {
        assert!(avatar_host("avatars.githubusercontent.com", "gitlab.com"));
        assert!(avatar_host("secure.gravatar.com", ""));
        assert!(avatar_host("git.acme.io", "git.acme.io"));
        assert!(!avatar_host("git.acme.io.evil.io", "git.acme.io"));
        assert!(!avatar_host("", ""));
    }

    #[test]
    fn asks_for_a_small_picture() {
        let at = |u: &str| {
            let mut u = reqwest::Url::parse(u).unwrap();
            sized(&mut u, 64);
            u.to_string()
        };
        assert_eq!(at("https://avatars.githubusercontent.com/u/1?v=4"), "https://avatars.githubusercontent.com/u/1?v=4&s=64");
        assert_eq!(at("https://secure.gravatar.com/avatar/ab?s=80&d=identicon"), "https://secure.gravatar.com/avatar/ab?d=identicon&s=64");
        assert_eq!(at("https://git.acme.io/uploads/-/system/user/avatar/3/a.png"), "https://git.acme.io/uploads/-/system/user/avatar/3/a.png?width=64");
    }
}
