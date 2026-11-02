use prism_base::tr;
use std::{
    sync::{Arc, LazyLock, RwLock},
    time::{Duration, Instant},
};

use anyhow::{bail, Result};
use futures::StreamExt;
use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize};

use crate::{
    http::encode,
    config::ClickupConfig,
    state::{Board, Dependency, Hub, ListRef, OutsideSprint, Person, PickBoard, Pickup, QuarterBoard, Refinement, Sprint, StatusOption, Ticket},
};

mod actions;
pub mod fields;
mod search;

pub use actions::Actions;

const SOURCE: &str = "clickup";
/// The slow queues (refinement, to pick, outside the sprint, next sprint): they answer after the sprint.
pub const QUEUES: &str = "clickup queues";
/// ClickUp calls at once (its limit is per minute: this only spreads the same calls).
const PARALLEL: usize = 6;
const API: &str = "https://api.clickup.com/api/v2";
/// Lists change little (a new sprint every two weeks): we re-resolve them rarely.
const RESOLVE_EVERY: Duration = Duration::from_secs(600);
/// The refinement and "To pick" queues change on the scale of a day: re-read every
/// 3 minutes, not on every pass.
/// A blocker outside the tickets read (another list) is read again after this long.
const DEPENDENCY_EVERY: Duration = Duration::from_secs(300);
/// At most so many of them read per pass: a ticket with a long chain fills in over a few passes.
const DEPENDENCY_READS: usize = 40;
const REFINEMENT_EVERY: Duration = Duration::from_secs(180);
/// System keychain entry read when CLICKUP_TOKEN is not set.
pub const KEYCHAIN_SERVICE: &str = "prism-clickup";

/// The workspace's custom task types (`custom_item_id` → name and Font Awesome icon), re-read with
/// the lists. Id 1 is the built-in "Milestone" type; 0 or nothing, an ordinary task.
static TASK_TYPES: LazyLock<RwLock<std::collections::HashMap<u64, (String, Option<String>)>>> = LazyLock::new(Default::default);

pub fn task_type(id: Option<u64>) -> Option<String> {
    match id? {
        0 => None,
        1 => Some("Milestone".into()),
        id => TASK_TYPES.read().unwrap().get(&id).map(|(name, _)| name.clone()),
    }
}

/// The Font Awesome solid icon ClickUp shows for the type (`bug`), when it was given one.
pub fn task_type_icon(id: Option<u64>) -> Option<String> {
    TASK_TYPES.read().unwrap().get(&id?).and_then(|(_, icon)| icon.clone())
}

/// ClickUp workspace id, shared with time tracking whose APIs
/// hang off it. Published from the first call, without waiting for the list walk.
/// Also carries the forced re-read request ("Refresh ClickUp") and the pass count.
pub struct Workspace {
    id: RwLock<Option<String>>,
    ready: tokio::sync::Notify,
    passes: crate::passes::Passes,
    /// The next requested re-read also walks the lists again (full refresh).
    full: std::sync::atomic::AtomicBool,
    /// The spaces of the sprint and the boards, from the last list walk: where a search looks.
    spaces: RwLock<Vec<String>>,
    /// Their tickets, read for a search and kept a while ([`search`]).
    index: search::Index,
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace {
            id: RwLock::default(),
            ready: tokio::sync::Notify::new(),
            passes: Default::default(),
            full: std::sync::atomic::AtomicBool::new(false),
            spaces: RwLock::default(),
            index: Default::default(),
        }
    }
}

pub type TeamId = Arc<Workspace>;

impl Workspace {
    pub fn get(&self) -> Option<String> {
        self.id.read().unwrap().clone()
    }

    fn set(&self, id: Option<String>) {
        let known = id.is_some();
        *self.id.write().unwrap() = id;
        if known {
            self.ready.notify_waiters();
        }
    }

    /// Immediate, full re-read (lists, sprint, slow queues), without waiting for the interval.
    /// Returns at the end of the pass following the request, with its error if any.
    pub async fn refresh(&self) -> Result<()> {
        self.full.store(true, std::sync::atomic::Ordering::SeqCst);
        self.passes.request(Duration::from_secs(120), "ClickUp").await
    }

    /// Re-read of the slow queues (Refinement, "To pick") on the next round, without waiting or
    /// walking the lists again: after a status change, a ticket may enter a
    /// queue read filtered by status, where no local copy can appear.
    pub fn refresh_queues(&self) {
        self.passes.poke();
    }

    /// Returns as soon as the id is known (immediately if it already is).
    pub async fn ready(&self) {
        loop {
            let notified = self.ready.notified();
            if self.get().is_some() {
                return;
            }
            notified.await;
        }
    }
}

pub async fn run(cfg: ClickupConfig, hub: Hub, team_id: TeamId) {
    fields::configure(&cfg.fields, cfg.filter_fields.as_deref(), &cfg.estimate_fields);
    let every = Duration::from_secs(cfg.poll_seconds.max(15));
    let mut first = true;
    let mut lists: Option<(Instant, Lists)> = None;
    let mut me: Option<Person> = None;
    let mut workspace: Option<String> = None;
    let mut refined: Option<Instant> = None;
    // Kept from one pass to the next: the token is read once, and connections are reused.
    let mut client: Option<Api> = None;
    let mut deps = Dependencies::default();
    loop {
        if !std::mem::take(&mut first) {
            tokio::select! {
                _ = hub.pause(every) => {}
                // Forced re-read: everything, lists and slow queues included.
                _ = team_id.passes.requested() => {
                    let full = team_id.full.swap(false, std::sync::atomic::Ordering::SeqCst);
                    tracing::info!("clickup: {} refresh requested", if full { "full" } else { "queues" });
                    if full {
                        lists = None;
                    }
                    refined = None;
                }
            }
        }
        team_id.passes.begin();
        // The sprint and its boards first: they show (and the source answers) without waiting for
        // the slow queues, read right after.
        let result = async {
            let api = match client.take() {
                Some(api) => api,
                None => Api::new().await?,
            };
            if team_id.get().is_none() {
                // A single quick call: time tracking can start during the list walk.
                let teams: Teams = api.get("/team").await?;
                team_id.set(teams.teams.first().map(|t| t.id.clone()));
            }
            if lists.as_ref().is_none_or(|(at, _)| at.elapsed() > RESOLVE_EVERY) {
                let ((resolved, team), user) = tokio::try_join!(resolve(&api, &cfg, &hub), api.get::<Me>("/user"))?;
                if team.is_some() && team != team_id.get() {
                    team_id.set(team.clone());
                }
                workspace = team;
                *team_id.spaces.write().unwrap() = resolved.spaces.clone();
                lists = Some((Instant::now(), resolved));
                me = Some(user.user.into());
                // Quarterly lists may have changed: the queue is re-read right away.
                refined = None;
            }
            let (_, l) = lists.as_ref().unwrap();
            let mut collected = collect(&api, l).await?;
            deps.fill(&api, collected.tickets_mut()).await;
            anyhow::Ok((api, collected))
        }
        .await;

        let api = match result {
            Ok((api, Collected { sprint, time_tasks })) => {
                hub.update(|s| {
                    let changed = s.sprint != sprint || s.time_tasks != time_tasks || s.tracker_me != me || s.clickup_team != workspace;
                    s.sprint = sprint;
                    s.time_tasks = time_tasks;
                    s.tracker_me = me.clone();
                    s.clickup_team = workspace.clone();
                    changed
                });
                // After the data: whoever sees the source as answered also sees what it brought.
                hub.set_error(SOURCE, None);
                Some(api)
            }
            Err(e) => {
                tracing::warn!("clickup: {e:#}");
                hub.set_error(SOURCE, Some(format!("{e:#}")));
                None
            }
        };
        if let (Some(api), Some((_, l))) = (&api, &lists)
            && refined.is_none_or(|at| at.elapsed() > REFINEMENT_EVERY)
        {
            let sprint = hub.subscribe().borrow().sprint.clone();
            match queues(api, l, &cfg, sprint.as_ref()).await {
                Ok(mut q) => {
                    deps.fill(api, q.tickets_mut()).await;
                    let Queues { refinement, pickup, outside, next_sprint } = q;
                    refined = Some(Instant::now());
                    hub.update(|s| {
                        let changed = s.refinement != refinement || s.pickup != pickup || s.outside_sprint != outside || s.next_sprint != next_sprint;
                        s.refinement = refinement;
                        s.pickup = pickup;
                        s.outside_sprint = outside;
                        s.next_sprint = next_sprint;
                        changed
                    });
                    hub.set_error(QUEUES, None);
                }
                Err(e) => {
                    tracing::warn!("clickup: queues: {e:#}");
                    hub.set_error(QUEUES, Some(format!("{e:#}")));
                }
            }
        }
        client = api;
        let error = hub.subscribe().borrow().errors.get(SOURCE).cloned();
        team_id.passes.end(error);
    }
}

/// The token last read from the keychain, and its entry: reading it runs `security` on macOS, once
/// per click otherwise. Forgotten when ClickUp refuses it or a new one is stored.
static KNOWN_TOKEN: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);

pub fn forget_token() {
    *KNOWN_TOKEN.lock().unwrap() = None;
}

pub async fn token() -> Result<String> {
    if let Ok(t) = std::env::var("CLICKUP_TOKEN")
        && !t.trim().is_empty()
    {
        return Ok(t.trim().to_string());
    }
    // The active profile's entry: `prism-clickup`, `prism-clickup-<profile>`.
    let service = crate::profile::service(KEYCHAIN_SERVICE);
    if let Some((of, token)) = KNOWN_TOKEN.lock().unwrap().as_ref()
        && *of == service
    {
        return Ok(token.clone());
    }
    if let Some(token) = crate::platform::secret(&service).await? {
        *KNOWN_TOKEN.lock().unwrap() = Some((service, token.clone()));
        return Ok(token);
    }
    let how = if cfg!(target_os = "macos") { "keychain entry" } else { "keyring entry (prism clickup-token)" };
    bail!("missing token: set CLICKUP_TOKEN or the \"{service}\" {how}")
}

/// Who the token signs in as (its username), read fresh: the Settings' and the wizard's check.
pub async fn whoami() -> Result<String> {
    #[derive(Deserialize)]
    struct Me {
        user: User,
    }
    #[derive(Deserialize)]
    struct User {
        username: String,
    }
    forget_token();
    Ok(Api::new().await?.get::<Me>("/user").await?.user.username)
}

pub struct Api {
    http: Client,
    token: String,
}

impl Api {
    pub async fn new() -> Result<Self> {
        Ok(Api { http: crate::http::client(), token: token().await? })
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(reqwest::Method::GET, path, None).await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(reqwest::Method::DELETE, path, None).await
    }

    pub async fn put<T: DeserializeOwned>(&self, path: &str, body: serde_json::Value) -> Result<T> {
        self.send(reqwest::Method::PUT, path, Some(body)).await
    }

    pub async fn post<T: DeserializeOwned>(&self, path: &str, body: serde_json::Value) -> Result<T> {
        self.send(reqwest::Method::POST, path, Some(body)).await
    }

    async fn send<T: DeserializeOwned>(&self, method: reqwest::Method, path: &str, body: Option<serde_json::Value>) -> Result<T> {
        self.request(method, API, path, body).await
    }

    async fn request<T: DeserializeOwned>(&self, method: reqwest::Method, base: &str, path: &str, body: Option<serde_json::Value>) -> Result<T> {
        let mut req = self
            .http
            .request(method, format!("{base}{path}"))
            .header("Authorization", &self.token)
            .timeout(Duration::from_secs(20));
        if let Some(body) = body {
            req = req.json(&body);
        }
        let resp = req.send().await?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            forget_token();
        }
        crate::http::json("ClickUp", path, &crate::http::body("ClickUp", path, resp).await?)
    }
}

struct Lists {
    sprint: Option<SprintList>,
    /// The sprint after the current one: its tickets, all statuses, for the Sprint block's Next tab.
    next: Option<SprintList>,
    /// The boards of `[[clickup.boards]]` found in the workspace, in the config's order.
    boards: Vec<FoundBoard>,
    /// Statuses of each of these lists, by list id.
    statuses: std::collections::HashMap<String, Vec<StatusOption>>,
    /// Workspace, and its spaces that hold the sprint or the boards: where to look for
    /// in-progress tickets outside the sprint.
    team: Option<String>,
    spaces: Vec<String>,
}

/// A board of the config and its lists in the workspace: one, or a folder's.
struct FoundBoard {
    cfg: crate::config::BoardConfig,
    lists: Vec<BoardList>,
}

struct BoardList {
    id: String,
    name: String,
    url: String,
    /// A quarterly list's (year, quarter), for their order.
    key: Option<(i32, u32)>,
}

struct Collected {
    sprint: Option<Sprint>,
    time_tasks: Option<Board>,
}

impl Collected {
    fn tickets_mut(&mut self) -> Vec<&mut Ticket> {
        self.sprint.iter_mut().flat_map(|s| s.tickets.iter_mut()).chain(self.time_tasks.iter_mut().flat_map(|b| b.tickets.iter_mut())).collect()
    }
}

impl Queues {
    fn tickets_mut(&mut self) -> Vec<&mut Ticket> {
        let refinement = self.refinement.boards.iter_mut().flat_map(|b| b.tickets.iter_mut());
        let pickup = self.pickup.boards.iter_mut().flat_map(|b| b.tickets.iter_mut());
        let next = self.next_sprint.iter_mut().flat_map(|s| s.tickets.iter_mut());
        refinement.chain(pickup).chain(self.outside.tickets.iter_mut()).chain(next).collect()
    }
}

/// The other ends of the tickets' dependencies, by task id: the tickets read (each pass), and a
/// blocker outside them read from ClickUp, again after [`DEPENDENCY_EVERY`].
#[derive(Default)]
struct Dependencies {
    known: std::collections::HashMap<String, (Instant, Dependency)>,
}

impl Dependencies {
    /// Gives each dependency of `tickets` the id, name and status of its other ticket.
    async fn fill(&mut self, api: &Api, mut tickets: Vec<&mut Ticket>) {
        let now = Instant::now();
        for t in &tickets {
            self.known.insert(t.task_id.clone(), (now, Dependency::of(t)));
        }
        let missing: std::collections::BTreeSet<String> = tickets
            .iter()
            .flat_map(|t| t.waiting_on.iter().chain(&t.blocking))
            .map(|d| d.task_id.clone())
            .filter(|id| self.known.get(id).is_none_or(|(at, _)| at.elapsed() > DEPENDENCY_EVERY))
            .collect();
        let read: Vec<(String, Result<RawTask>)> = futures::stream::iter(missing.into_iter().take(DEPENDENCY_READS))
            .map(|id| async move {
                let task = api.get::<RawTask>(&format!("/task/{id}")).await;
                (id, task)
            })
            .buffer_unordered(PARALLEL)
            .collect()
            .await;
        for (id, task) in read {
            // Unreadable (deleted, no access): left unknown, and not asked again before its time.
            let dep = task.map(|t| Dependency::of(&Ticket::from(t))).unwrap_or_else(|e| {
                tracing::debug!("clickup: dependency {id}: {e:#}");
                Dependency::unknown(&id)
            });
            self.known.insert(id, (Instant::now(), dep));
        }
        for t in tickets.iter_mut() {
            for d in t.waiting_on.iter_mut().chain(t.blocking.iter_mut()) {
                if let Some((_, k)) = self.known.get(&d.task_id) {
                    *d = k.clone();
                }
            }
        }
    }
}

/// The slow queues, re-read every few minutes: refinement, to pick, outside the sprint, next sprint.
struct Queues {
    refinement: Refinement,
    pickup: Pickup,
    outside: OutsideSprint,
    next_sprint: Option<Sprint>,
}

struct SprintList {
    id: String,
    url: String,
    name: String,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

/// Source of the setup warnings: names of `[clickup]` not found in the workspace.
pub const SETUP: &str = "clickup-setup";

/// Walks the workspace to find the sprint lists and the boards, by name.
async fn resolve(api: &Api, cfg: &ClickupConfig, hub: &Hub) -> Result<(Lists, Option<String>)> {
    let teams: Teams = api.get("/team").await?;
    let mut workspace = teams.teams.first().map(|t| t.id.clone());
    // Types are used to recognize EPICs: their absence does not deprive us of the tickets.
    let types = futures::future::join_all(teams.teams.iter().map(|team| async move { api.get::<CustomItems>(&format!("/team/{}/custom_item", team.id)).await }));
    for items in types.await {
        match items {
            Ok(items) => TASK_TYPES.write().unwrap().extend(items.custom_items.into_iter().map(|i| (i.id, (i.name, i.avatar.and_then(CustomItemAvatar::icon))))),
            Err(e) => tracing::warn!("clickup: task types: {e:#}"),
        }
    }
    let mut sprint_lists = Vec::new();
    let configured = cfg.boards.clone();
    let mut boards: Vec<FoundBoard> = configured.iter().map(|c| FoundBoard { cfg: c.clone(), lists: Vec::new() }).collect();
    let mut team_of = std::collections::HashMap::new();
    let mut spaces_used = Vec::new();
    let current = quarter_of(Utc::now().date_naive());

    for team in teams.teams {
        let spaces: Spaces = api.get(&format!("/team/{}/space?archived=false", team.id)).await?;
        // Every space's folders and lists at once, kept in the spaces' order.
        let contents = futures::future::try_join_all(spaces.spaces.iter().map(|space| async move {
            let (folders, lists) = (format!("/space/{}/folder?archived=false", space.id), format!("/space/{}/list?archived=false", space.id));
            tokio::try_join!(api.get::<Folders>(&folders), api.get::<ListsResp>(&lists))
        }))
        .await?;
        for (space, (folders, folderless)) in spaces.spaces.into_iter().zip(contents) {
            let mut used = false;
            for folder in &folders.folders {
                if folder.name.eq_ignore_ascii_case(&cfg.sprint_folder) {
                    used = true;
                    workspace = Some(team.id.clone());
                    for l in &folder.lists {
                        team_of.insert(l.id.clone(), team.id.clone());
                    }
                    sprint_lists.extend(folder.lists.iter().cloned());
                }
            }
            for board in boards.iter_mut().filter(|b| b.lists.is_empty()) {
                let found: Vec<BoardList> = if board.cfg.folder.is_empty() {
                    let all = folders.folders.iter().flat_map(|f| f.lists.iter()).chain(folderless.lists.iter());
                    all.filter(|l| l.name.eq_ignore_ascii_case(board.cfg.list_name())).take(1).map(|l| BoardList { id: l.id.clone(), name: l.name.clone(), url: board_url(&team.id, &l.id), key: None }).collect()
                } else {
                    let lists = folders.folders.iter().filter(|f| f.name.eq_ignore_ascii_case(&board.cfg.folder)).flat_map(|f| f.lists.iter());
                    let mut found: Vec<BoardList> = lists
                        .filter_map(|l| {
                            let key = quarter_key(&l.name);
                            // A quarterly board: the current and coming quarters only.
                            (!board.cfg.quarters || key.is_some_and(|k| k >= current)).then(|| BoardList { id: l.id.clone(), name: l.name.clone(), url: board_url(&team.id, &l.id), key })
                        })
                        .collect();
                    found.sort_by_key(|l| l.key);
                    found
                };
                used |= !found.is_empty();
                board.lists = found;
            }
            if used {
                spaces_used.push(space.id.clone());
            }
        }
    }

    // Names of the config found nowhere: a block would stay empty without saying why.
    let missing: Vec<String> = (sprint_lists.is_empty() && !cfg.sprint_folder.is_empty())
        .then(|| format!("folder \"{}\" (sprint_folder)", cfg.sprint_folder))
        .into_iter()
        .chain(boards.iter().filter(|b| b.lists.is_empty()).map(|b| match (b.cfg.folder.is_empty(), b.cfg.quarters) {
            (true, _) => format!("list \"{}\" (board {})", b.cfg.list_name(), b.cfg.name),
            (false, true) => format!("quarterly lists of folder \"{}\" (board {})", b.cfg.folder, b.cfg.name),
            (false, false) => format!("folder \"{}\" (board {})", b.cfg.folder, b.cfg.name),
        }))
        .collect();
    hub.set_error(SETUP, (!missing.is_empty()).then(|| tr!("not found in the workspace: {names}", names = missing.join(", "))));
    let found = sprint_lists.len();
    let (current, next) = current_sprint(sprint_lists);
    let sprint = current.map(|mut s| {
        s.url = board_url(team_of.get(&s.id).map(String::as_str).unwrap_or_default(), &s.id);
        s
    });
    let next_list = next.map(|mut n| {
        n.url = board_url(team_of.get(&n.id).map(String::as_str).unwrap_or_default(), &n.id);
        n
    });
    tracing::info!(
        "clickup: {found} sprint lists, current sprint {:?}, {} of {} boards found",
        sprint.as_ref().map(|s| &s.name),
        boards.iter().filter(|b| !b.lists.is_empty()).count(),
        boards.len(),
    );
    let ids: Vec<String> = sprint
        .iter()
        .chain(next_list.iter())
        .map(|s| s.id.clone())
        .chain(boards.iter().flat_map(|b| b.lists.iter().map(|l| l.id.clone())))
        .collect();
    // Without statuses, the tables stay read-only: no reason to deprive us of the tickets.
    let statuses = futures::stream::iter(ids)
        .map(|id| async move {
            let read = list_statuses(api, &id).await;
            (id, read)
        })
        .buffer_unordered(PARALLEL)
        .filter_map(|(id, read)| async move { read.map_err(|e| tracing::warn!("clickup: statuses of list {id}: {e:#}")).ok().map(|list| (id, list)) })
        .collect::<std::collections::HashMap<_, _>>();
    let statuses = statuses.await;
    let team = workspace.clone();
    Ok((Lists { sprint, next: next_list, boards, statuses, team, spaces: spaces_used }, workspace))
}

/// Statuses of a list, in board order.
pub async fn list_statuses(api: &Api, list_id: &str) -> Result<Vec<StatusOption>> {
    let list: ListStatuses = api.get(&format!("/list/{list_id}")).await?;
    Ok(list.statuses.into_iter().map(StatusOption::from).collect())
}

#[derive(Deserialize)]
struct ListStatuses {
    #[serde(default)]
    statuses: Vec<RawStatus>,
}

impl From<RawStatus> for StatusOption {
    fn from(s: RawStatus) -> Self {
        StatusOption { closed: s.is_closed(), status: s.status.to_lowercase(), color: s.color }
    }
}

impl RawStatus {
    pub fn is_closed(&self) -> bool {
        matches!(self.kind.as_str(), "closed" | "done")
    }
}

/// The sprint whose dates bracket today (failing that, the last one started), and the next one.
fn current_sprint(lists: Vec<RawList>) -> (Option<SprintList>, Option<SprintList>) {
    let now = Utc::now();
    let mut dated: Vec<SprintList> = lists
        .into_iter()
        .map(|l| {
            // Date fields are often copied from one sprint to the next; the name is authoritative.
            let (start, end) = dates_from_name(&l.name)
                .map(|(start, end)| (Some(start), Some(end)))
                .unwrap_or((ms(l.start_date.as_deref()), ms(l.due_date.as_deref())));
            SprintList { start, end, id: l.id, name: l.name, url: String::new() }
        })
        .collect();
    dated.sort_by_key(|l| l.start);
    let running = dated.iter().position(|l| matches!((l.start, l.end), (Some(s), Some(e)) if s <= now && now <= e));
    let index = running.or_else(|| dated.iter().rposition(|l| l.start.is_some_and(|s| s <= now)));
    // The next one: the first that starts after the current one (or after now, without a current one).
    let after = index.and_then(|i| dated[i].start).unwrap_or(now);
    let next = dated.iter().position(|l| l.start.is_some_and(|s| s > after));
    let mut take = |i: Option<usize>| i.map(|i| std::mem::replace(&mut dated[i], SprintList::empty()));
    (take(index), take(next))
}

impl SprintList {
    fn empty() -> Self {
        SprintList { id: String::new(), url: String::new(), name: String::new(), start: None, end: None }
    }
}

/// `slow`: the config when the slow queues (refinement, "To pick") need re-reading, otherwise `None`.
async fn collect(api: &Api, lists: &Lists) -> Result<Collected> {
    let sprint = async {
        let Some(l) = &lists.sprint else { return Ok(None) };
        Ok::<_, anyhow::Error>(Some(Sprint {
            name: l.name.clone(),
            list_id: l.id.clone(),
            start: l.start,
            end: l.end,
            url: l.url.clone(),
            tickets: tasks(api, &l.id, true, true).await?,
            statuses: lists.statuses.get(&l.id).cloned().unwrap_or_default(),
            boards: Vec::new(),
        }))
    };
    // Tasks to log time on (`time` boards), with their subtasks: time-tracking entries in their own
    // right, whose description helps choose the right one (on hover).
    let time_tasks = async {
        let found: Vec<(&FoundBoard, &BoardList)> = lists.boards.iter().filter(|b| b.cfg.uses("time")).flat_map(|b| b.lists.iter().map(move |l| (b, l))).collect();
        let Some(&(first, list)) = found.first() else { return Ok(None) };
        let read = futures::future::try_join_all(found.iter().map(|(_, l)| tasks_with(api, &l.id, false, true, true, ""))).await?;
        Ok::<_, anyhow::Error>(Some(Board {
            name: first.cfg.name.clone(),
            url: list.url.clone(),
            tickets: read.into_iter().flatten().collect(),
            statuses: lists.statuses.get(&list.id).cloned().unwrap_or_default(),
        }))
    };
    let (sprint, time_tasks) = tokio::try_join!(sprint, time_tasks)?;
    Ok(Collected { sprint, time_tasks })
}

/// The slow queues, read together.
async fn queues(api: &Api, lists: &Lists, cfg: &ClickupConfig, sprint: Option<&Sprint>) -> Result<Queues> {
    let conventions = crate::conventions::get();
    let (refinement, pickup, outside, next_sprint) = tokio::try_join!(
        refinement_queue(api, lists, &cfg.refinement_statuses),
        pickup(api, lists, &conventions.pick_statuses),
        outside_sprint(api, lists, &cfg.ongoing_statuses, sprint),
        next_sprint(api, lists),
    )?;
    Ok(Queues { refinement, pickup, outside, next_sprint })
}

/// The next sprint, all its tickets (closed included, like the current one): it has not
/// started. It changes little: re-read with the slow queues.
async fn next_sprint(api: &Api, lists: &Lists) -> Result<Option<Sprint>> {
    let Some(l) = &lists.next else { return Ok(None) };
    Ok(Some(Sprint {
        name: l.name.clone(),
        list_id: l.id.clone(),
        start: l.start,
        end: l.end,
        url: l.url.clone(),
        tickets: tasks(api, &l.id, true, true).await?,
        statuses: lists.statuses.get(&l.id).cloned().unwrap_or_default(),
        boards: Vec::new(),
    }))
}

/// The boards read for To pick and Sync, and the next sprint, subtasks included. A `sync` board
/// is read whole (its open tickets), To pick keeping those in the "to pick" statuses; a board
/// only to pick, filtered by ClickUp. The red and orange boards come before the next sprint.
async fn pickup(api: &Api, lists: &Lists, statuses: &[String]) -> Result<Pickup> {
    let filter: String = statuses.iter().map(|s| format!("&statuses[]={}", encode(s))).collect();
    let filter = &filter;
    let read = lists.boards.iter().filter(|b| (b.cfg.uses("pick") || b.cfg.uses("sync")) && !b.lists.is_empty()).map(|b| async move {
        let sync = b.cfg.uses("sync");
        let parts = futures::future::try_join_all(b.lists.iter().map(|l| tasks_with(api, &l.id, false, true, false, if sync { "" } else { filter }))).await?;
        let first = &b.lists[0];
        Ok::<_, anyhow::Error>(PickBoard {
            name: b.cfg.name.clone(),
            kind: "board".into(),
            tone: if b.cfg.tone.is_empty() { "info".into() } else { b.cfg.tone.clone() },
            pick: b.cfg.uses("pick"),
            sync,
            url: first.url.clone(),
            tickets: parts.into_iter().flatten().collect(),
            statuses: lists.statuses.get(&first.id).cloned().unwrap_or_default(),
        })
    });
    let mut boards = futures::future::try_join_all(read).await?;
    if let Some(n) = &lists.next {
        let next = PickBoard {
            name: n.name.clone(),
            kind: "next_sprint".into(),
            tone: "info".into(),
            pick: true,
            sync: false,
            url: n.url.clone(),
            tickets: tasks_with(api, &n.id, false, true, false, filter).await?,
            statuses: lists.statuses.get(&n.id).cloned().unwrap_or_default(),
        };
        let at = boards.iter().take_while(|b| b.tone == "ko" || b.tone == "warn").count();
        boards.insert(at, next);
    }
    Ok(Pickup { statuses: statuses.iter().map(|s| s.to_lowercase()).collect(), boards })
}

/// In-progress (or further) tickets of the whole workspace, outside the current sprint: a
/// read filtered by ClickUp (statuses, followed spaces), subtasks included.
async fn outside_sprint(api: &Api, lists: &Lists, statuses: &[String], sprint: Option<&Sprint>) -> Result<OutsideSprint> {
    let statuses: Vec<String> = statuses.iter().map(|s| s.to_lowercase()).collect();
    let Some(team) = &lists.team else { return Ok(OutsideSprint { statuses, ..Default::default() }) };
    if lists.spaces.is_empty() || statuses.is_empty() {
        return Ok(OutsideSprint { statuses, ..Default::default() });
    }
    let in_sprint: std::collections::HashSet<&str> = sprint.iter().flat_map(|s| s.tickets.iter().map(|t| t.task_id.as_str())).collect();
    let filter: String = statuses
        .iter()
        .map(|s| format!("&statuses[]={}", encode(s)))
        .chain(lists.spaces.iter().map(|id| format!("&space_ids[]={id}")))
        .collect();
    let mut tickets = Vec::new();
    for page in 0..10 {
        let resp: TasksResp = api.get(&format!("/team/{team}/task?page={page}&subtasks=true&include_closed=false{filter}")).await?;
        tickets.extend(resp.tasks.into_iter().map(Ticket::from).filter(|t| !in_sprint.contains(t.task_id.as_str())));
        if resp.last_page.unwrap_or(true) {
            break;
        }
    }
    // Statuses of each list: already known for the followed boards, read otherwise.
    let ids: std::collections::BTreeSet<String> = tickets.iter().filter_map(|t| t.list.as_ref().map(|l| l.id.clone())).collect();
    let list_statuses: std::collections::BTreeMap<String, Vec<StatusOption>> = futures::stream::iter(ids)
        .map(|id| async move {
            let options = match lists.statuses.get(&id) {
                Some(k) => k.clone(),
                None => list_statuses_or_empty(api, &id).await,
            };
            (id, options)
        })
        .buffer_unordered(PARALLEL)
        .collect()
        .await;
    Ok(OutsideSprint { statuses, tickets, list_statuses })
}

/// Without statuses, the picker stays read-only: no reason to deprive us of the tickets.
async fn list_statuses_or_empty(api: &Api, id: &str) -> Vec<StatusOption> {
    list_statuses(api, id).await.unwrap_or_else(|e| {
        tracing::warn!("clickup: statuses of list {id}: {e:#}");
        Vec::new()
    })
}

/// Refinement queue of the `refinement` boards' lists (a folder's quarterly lists, or a board's
/// own): read in full (closed tickets included), so that the tree view shows each EPIC's
/// progress, then pruned by [`refinement_tree`].
async fn refinement_queue(api: &Api, lists: &Lists, statuses: &[String]) -> Result<Refinement> {
    let statuses: Vec<String> = statuses.iter().map(|s| s.to_lowercase()).collect();
    let kept = &statuses;
    let queued = lists.boards.iter().filter(|b| b.cfg.uses("refinement")).flat_map(|b| b.lists.iter().map(move |l| (b, l)));
    let boards = futures::future::try_join_all(queued.map(|(b, l)| async move {
        Ok::<_, anyhow::Error>(QuarterBoard {
            folder: b.cfg.name.clone(),
            quarter: if l.key.is_some() { l.name.clone() } else { String::new() },
            url: l.url.clone(),
            tickets: refinement_tree(tasks(api, &l.id, true, true).await?, kept),
            statuses: lists.statuses.get(&l.id).cloned().unwrap_or_default(),
        })
    }))
    .await?;
    Ok(Refinement { statuses, boards })
}

/// Keeps from a list the tickets in refinement, the open EPICs and all their direct children,
/// even delivered: enough to place each EPIC without carrying the whole quarter.
pub fn refinement_tree(tickets: Vec<Ticket>, statuses: &[String]) -> Vec<Ticket> {
    let epics: std::collections::HashSet<String> = tickets
        .iter()
        .filter(|t| !t.closed && t.task_type.as_deref().is_some_and(|k| k.eq_ignore_ascii_case("epic")))
        .map(|t| t.task_id.clone())
        .collect();
    tickets
        .into_iter()
        .filter(|t| statuses.contains(&t.status) || epics.contains(&t.task_id) || t.parent.as_ref().is_some_and(|p| epics.contains(p)))
        .collect()
}


static QUARTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*Q([1-4])\s+(20\d{2})\s*$").unwrap());

/// `Q4 2026` → (2026, 4); the other lists (sprints, idea boxes, `_old`) are not quarterly.
fn quarter_key(name: &str) -> Option<(i32, u32)> {
    let c = QUARTER.captures(name)?;
    Some((c[2].parse().ok()?, c[1].parse().ok()?))
}

fn quarter_of(day: NaiveDate) -> (i32, u32) {
    use chrono::Datelike;
    (day.year(), (day.month() - 1) / 3 + 1)
}

fn truncate(text: &str) -> String {
    match text.char_indices().nth(DESCRIPTION_CHARS) {
        Some((i, _)) => format!("{}…", text[..i].trim_end()),
        None => text.to_string(),
    }
}

/// Link to a board's list view in the ClickUp app.
fn board_url(team: &str, list: &str) -> String {
    format!("https://app.clickup.com/{team}/v/li/{list}")
}

/// With `subtasks`, children (of an EPIC notably) come with their parents: they are
/// often the ones carrying the MRs and needing specification. `include_timl` brings in tickets
/// added to the sprint from their board.
async fn tasks(api: &Api, list_id: &str, include_closed: bool, subtasks: bool) -> Result<Vec<Ticket>> {
    tasks_with(api, list_id, include_closed, subtasks, false, "").await
}

/// Kept length of a description: enough to recognize the task in a tooltip.
const DESCRIPTION_CHARS: usize = 500;

/// `filter`: parameters appended as is to the request (`&statuses[]=…`).
async fn tasks_with(api: &Api, list_id: &str, include_closed: bool, subtasks: bool, descriptions: bool, filter: &str) -> Result<Vec<Ticket>> {
    let mut out = Vec::new();
    for page in 0..10 {
        let resp: TasksResp = api
            .get(&format!("/list/{list_id}/task?page={page}&subtasks={subtasks}&include_timl=true&include_closed={include_closed}{filter}"))
            .await?;
        out.extend(resp.tasks.into_iter().map(|t| {
            let description = descriptions
                .then(|| t.text_content.as_deref().map(str::trim).filter(|d| !d.is_empty()).map(truncate))
                .flatten();
            Ticket { description, ..Ticket::from(t) }
        }));
        if resp.last_page.unwrap_or(true) {
            break;
        }
    }
    Ok(out)
}

static SPRINT_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?P<year>20\d{2}).*\((?P<d1>\d{1,2})/(?P<m1>\d{1,2})\s*-\s*(?P<d2>\d{1,2})/(?P<m2>\d{1,2})\)").unwrap()
});

/// `Q4 2026 - Sprint 30 (21/12 - 1/1)` → from 21/12/2026 00:00 to 1/1/2027 inclusive.
fn dates_from_name(name: &str) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let c = SPRINT_NAME.captures(name)?;
    let n = |k: &str| c[k].parse::<u32>().ok();
    let year: i32 = c["year"].parse().ok()?;
    let (d1, m1, d2, m2) = (n("d1")?, n("m1")?, n("d2")?, n("m2")?);
    let start = NaiveDate::from_ymd_opt(year, m1, d1)?;
    let end_year = if m2 < m1 { year + 1 } else { year };
    let end = NaiveDate::from_ymd_opt(end_year, m2, d2)?;
    Some((start.and_hms_opt(0, 0, 0)?.and_utc(), end.and_hms_opt(23, 59, 59)?.and_utc()))
}

/// ClickUp encodes dates in milliseconds, inside strings.
pub fn ms(v: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(v?.parse().ok()?)
}

#[derive(Deserialize)]
struct Teams {
    teams: Vec<Id>,
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
struct Spaces {
    spaces: Vec<Id>,
}

#[derive(Deserialize)]
struct Folders {
    folders: Vec<Folder>,
}

#[derive(Deserialize)]
struct Folder {
    name: String,
    #[serde(default)]
    lists: Vec<RawList>,
}

#[derive(Deserialize)]
struct ListsResp {
    lists: Vec<RawList>,
}

#[derive(Deserialize, Clone)]
struct RawList {
    id: String,
    name: String,
    start_date: Option<String>,
    due_date: Option<String>,
}

#[derive(Deserialize)]
struct CustomItems {
    #[serde(default)]
    custom_items: Vec<CustomItem>,
}

#[derive(Deserialize)]
struct CustomItem {
    id: u64,
    name: String,
    #[serde(default)]
    avatar: Option<CustomItemAvatar>,
}

/// The icon of a task type: `source` "fas" for a Font Awesome solid icon, named by `value`.
#[derive(Deserialize)]
struct CustomItemAvatar {
    source: Option<String>,
    value: Option<String>,
}

impl CustomItemAvatar {
    fn icon(self) -> Option<String> {
        (self.source.as_deref() == Some("fas")).then_some(self.value).flatten().filter(|v| !v.is_empty())
    }
}

#[derive(Deserialize)]
struct TasksResp {
    tasks: Vec<RawTask>,
    last_page: Option<bool>,
}

#[derive(Deserialize)]
struct RawTask {
    id: String,
    custom_id: Option<String>,
    name: String,
    status: RawStatus,
    #[serde(default)]
    assignees: Vec<RawUser>,
    url: String,
    date_created: Option<String>,
    date_updated: Option<String>,
    parent: Option<String>,
    /// Custom task type (see [`task_type`]).
    custom_item_id: Option<u64>,
    /// Plain-text description (ClickUp also provides a heavier Markdown version).
    text_content: Option<String>,
    /// `null` when no priority is set.
    priority: Option<RawPriority>,
    /// In milliseconds.
    time_estimate: Option<u64>,
    /// Everyone's time logged on it, in milliseconds (with time tracking on).
    #[serde(default)]
    time_spent: Option<u64>,
    #[serde(default)]
    tags: Vec<RawTagName>,
    list: Option<RawListRef>,
    #[serde(default)]
    custom_fields: Vec<fields::RawField>,
    /// Both directions: `task_id` waits on `depends_on`. Read loosely (ClickUp documents them as
    /// strings for a list of tasks, objects for one task): anything else is skipped, never the task.
    #[serde(default)]
    dependencies: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawTagName {
    name: String,
}

#[derive(Deserialize)]
struct RawListRef {
    id: String,
    name: Option<String>,
}

#[derive(Deserialize)]
struct RawPriority {
    priority: String,
}

#[derive(Deserialize)]
pub struct RawStatus {
    pub status: String,
    pub color: Option<String>,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
pub struct RawUser {
    id: u64,
    username: Option<String>,
    initials: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default, rename = "profilePicture")]
    profile_picture: Option<String>,
}

impl From<RawUser> for Person {
    fn from(u: RawUser) -> Self {
        let name = u.username.unwrap_or_default();
        let initials = u.initials.unwrap_or_else(|| name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect());
        let color = u.color.map(|c| c.trim().to_string()).filter(|c| c.starts_with('#'));
        let avatar = u.profile_picture.filter(|p| p.starts_with("https://"));
        Person { id: u.id, name, initials: initials.to_uppercase(), color, avatar }
    }
}

#[derive(Deserialize)]
struct Me {
    user: RawUser,
}

impl From<RawTask> for Ticket {
    fn from(t: RawTask) -> Self {
        // This task waits on `depends_on` where it is `task_id`, and blocks `task_id` where it is `depends_on`.
        let mut waiting_on: Vec<Dependency> = Vec::new();
        let mut blocking: Vec<Dependency> = Vec::new();
        for d in &t.dependencies {
            let (Some(task_id), Some(depends_on)) = (d.get("task_id").and_then(|v| v.as_str()), d.get("depends_on").and_then(|v| v.as_str())) else { continue };
            let (list, other) = if task_id == t.id { (&mut waiting_on, depends_on) } else if depends_on == t.id { (&mut blocking, task_id) } else { continue };
            if !list.iter().any(|x| x.task_id == other) {
                list.push(Dependency::unknown(other));
            }
        }
        Ticket {
            waiting_on,
            blocking,
            task_id: t.id.clone(),
            id: t.custom_id.unwrap_or(t.id),
            name: t.name,
            closed: t.status.is_closed(),
            parent: t.parent,
            priority: t.priority.and_then(|p| crate::state::Priority::of(&p.priority)),
            task_type: task_type(t.custom_item_id),
            task_type_icon: task_type_icon(t.custom_item_id),
            description: None,
            list: t.list.map(|l| ListRef { name: l.name.unwrap_or_default(), id: l.id }),
            repo: None,
            time_estimate: t.time_estimate,
            time_spent: t.time_spent.filter(|ms| *ms > 0),
            tags: t.tags.into_iter().map(|t| t.name).collect(),
            status: t.status.status.to_lowercase(),
            assignees: t.assignees.into_iter().map(Person::from).collect(),
            url: t.url,
            created_at: ms(t.date_created.as_deref()),
            updated_at: ms(t.date_updated.as_deref()),
            fields: fields::shown(&t.custom_fields),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_assignee_keeps_their_https_profile_picture() {
        let user = |picture: &str| -> Person { serde_json::from_value::<RawUser>(serde_json::json!({ "id": 7, "username": "Jo Wu", "color": "#7b68ee", "profilePicture": picture })).unwrap().into() };
        assert_eq!(user("https://attachments.clickup.com/profilePictures/7_a.jpg").avatar.as_deref(), Some("https://attachments.clickup.com/profilePictures/7_a.jpg"));
        assert_eq!(user("http://example.com/7.jpg").avatar, None);
        let none: Person = serde_json::from_value::<RawUser>(serde_json::json!({ "id": 7, "username": "Jo Wu", "profilePicture": null })).unwrap().into();
        assert_eq!((none.avatar, none.initials.as_str()), (None, "JW"));
    }

    fn ticket(id: &str, status: &str, parent: Option<&str>, epic: bool, closed: bool) -> Ticket {
        Ticket {
            fields: Vec::new(),
            waiting_on: Vec::new(),
            blocking: Vec::new(),
            id: id.into(),
            task_id: id.into(),
            parent: parent.map(Into::into),
            priority: None,
            task_type: epic.then(|| "EPIC".into()),
            task_type_icon: None,
            description: None,
            list: None,
            repo: None,
            time_estimate: None,
            time_spent: None,
            tags: Vec::new(),
            name: id.into(),
            status: status.into(),
            closed,
            assignees: Vec::new(),
            url: String::new(),
            created_at: None,
            updated_at: None,
        }
    }

    /// Both directions of ClickUp's dependencies, once each; an unexpected shape is skipped.
    #[test]
    fn reads_dependencies_both_ways() {
        let raw: RawTask = serde_json::from_value(serde_json::json!({
            "id": "t1", "name": "Export", "status": { "status": "to do", "color": null, "type": "open" }, "url": "u",
            "date_created": null, "date_updated": null, "parent": null, "custom_item_id": null, "text_content": null,
            "priority": null, "time_estimate": null, "list": null,
            "dependencies": [
                { "task_id": "t1", "depends_on": "b1", "type": 1 },
                { "task_id": "t1", "depends_on": "b1", "type": 1 },
                { "task_id": "w1", "depends_on": "t1", "type": 0 },
                "b9"
            ]
        }))
        .unwrap();
        let t = Ticket::from(raw);
        assert_eq!(t.waiting_on.iter().map(|d| d.task_id.as_str()).collect::<Vec<_>>(), ["b1"]);
        assert_eq!(t.blocking.iter().map(|d| d.task_id.as_str()).collect::<Vec<_>>(), ["w1"]);
        assert!(t.waiting_on[0].id.is_empty(), "unknown until filled");
    }

    #[test]
    fn refinement_keeps_open_epics_and_all_their_children() {
        let tickets = vec![
            ticket("epic", "doing", None, true, false),
            ticket("done-child", "done", Some("epic"), false, true),
            ticket("queued", "to refine", None, false, false),
            ticket("old-epic", "done", None, true, true),
            ticket("old-child", "done", Some("old-epic"), false, true),
            ticket("other", "doing", None, false, false),
        ];
        let kept: Vec<_> = refinement_tree(tickets, &["to refine".into()]).into_iter().map(|t| t.id).collect();
        assert_eq!(kept, ["epic", "done-child", "queued"]);
    }

    #[test]
    fn sprint_dates_come_from_the_name() {
        let (start, end) = dates_from_name("Q3 2026 - Sprint 24 (28/9 - 9/10)").unwrap();
        assert_eq!(start.date_naive(), NaiveDate::from_ymd_opt(2026, 9, 28).unwrap());
        assert_eq!(end.date_naive(), NaiveDate::from_ymd_opt(2026, 10, 9).unwrap());
    }

    #[test]
    fn sprint_spanning_new_year() {
        let (_, end) = dates_from_name("Q4 2026 - Sprint 30 (21/12 - 1/1)").unwrap();
        assert_eq!(end.date_naive(), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
    }

    #[test]
    fn statuses_keep_their_closed_type() {
        let raw: Vec<RawStatus> = serde_json::from_value(serde_json::json!([
            { "status": "To Do", "color": "#ddd", "type": "open" }, { "status": "Done", "type": "closed" }
        ]))
        .unwrap();
        let options: Vec<StatusOption> = raw.into_iter().map(StatusOption::from).collect();
        assert_eq!(options.iter().map(|s| (s.status.as_str(), s.closed)).collect::<Vec<_>>(), [("to do", false), ("done", true)]);
    }

    #[test]
    fn custom_task_types_are_named() {
        TASK_TYPES.write().unwrap().insert(1001, ("EPIC".into(), Some("book".into())));
        assert_eq!(task_type(Some(1001)).as_deref(), Some("EPIC"));
        assert_eq!(task_type_icon(Some(1001)).as_deref(), Some("book"));
        assert_eq!(task_type_icon(Some(1)), None);
        assert_eq!(task_type(Some(1)).as_deref(), Some("Milestone"));
        assert_eq!(task_type(Some(0)), None);
        assert_eq!(task_type(None), None);
    }

    #[test]
    fn current_and_next_sprints() {
        use chrono::Datelike;
        let day = |offset: i64| Utc::now().date_naive() + chrono::Duration::days(offset);
        let list = |id: &str, from: i64, to: i64| {
            let (a, b) = (day(from), day(to));
            RawList { id: id.into(), name: format!("Q4 {} - Sprint ({}/{} - {}/{})", a.year(), a.day(), a.month(), b.day(), b.month()), start_date: None, due_date: None }
        };
        let (current, next) = current_sprint(vec![list("next", 5, 15), list("past", -20, -10), list("now", -5, 4), list("later", 19, 29)]);
        assert_eq!(current.map(|s| s.id).as_deref(), Some("now"));
        assert_eq!(next.map(|s| s.id).as_deref(), Some("next"));
    }

    #[test]
    fn quarter_lists_are_recognised() {
        assert_eq!(quarter_key("Q4 2026"), Some((2026, 4)));
        assert_eq!(quarter_key("Q3 2026 - Sprint 24 (28/9 - 9/10)"), None);
        assert_eq!(quarter_key("Boîte à idée"), None);
        assert_eq!(quarter_of(NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()), (2026, 4));
        assert!((2027, 1) > (2026, 4));
    }

    #[test]
    fn statuses_are_encoded_for_the_query() {
        assert_eq!(encode("to specify"), "to%20specify");
        assert_eq!(encode("à faire"), "%C3%A0%20faire");
    }

    #[test]
    fn name_without_dates_falls_back() {
        assert!(dates_from_name("Backlog").is_none());
    }
}
