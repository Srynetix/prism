//! Obsidian Kanban boards as the tracker: Markdown files of a vault, read from disk, nothing to
//! sign in to. A board is `## Lane` headings over `- [ ] card` lines, a card's indented lines its
//! description; a card linking a note (`[[T-12]]`) takes the note's frontmatter (`id`, `title`,
//! `priority`, `created`) and body, and one titled `**C-68 — …**` takes that id. The open cards
//! of every board make the sprint, but for those ready to be picked (`[tracker] pick_statuses`),
//! which make To pick. Prism also moves a card to another lane (its note's `status`
//! following), sets its priority and comments on it, writing the files as Obsidian would.

mod edit;

use prism_base::{i18n::tr, tr};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, RwLock},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;

use crate::{
    http::encode,
    task_view::{Comment, TaskView},
    config::ObsidianConfig,
    state::{Hub, ListRef, PickBoard, Pickup, Sprint, SprintBoard, StatusOption, Ticket},
};

const SOURCE: &str = "obsidian";
/// The files are watched: they are read again as soon as they change, and also at this interval,
/// in case a change went unseen (a sync service writing behind the system's back).
const FALLBACK: Duration = Duration::from_secs(60);
/// Without a watch, they are read again this often.
const POLL: Duration = Duration::from_secs(5);
/// A save often comes as several events: read once they have settled.
const SETTLE: Duration = Duration::from_millis(200);

/// A board of the config, its paths resolved.
#[derive(Debug, Clone)]
struct BoardRef {
    /// The board's file.
    file: PathBuf,
    /// Its name in Prism: its repository's, or its folder's.
    name: String,
    repo: Option<String>,
}

/// A card read on the last pass, with what its preview and its changes need.
#[derive(Debug, Clone)]
struct Entry {
    ticket: Ticket,
    /// Its board's file, its first line there, and its rank among its lane's cards reading the
    /// same: where to find it again.
    board: PathBuf,
    card: String,
    nth: usize,
    /// The note it links, if found.
    note: Option<PathBuf>,
    /// Its own description: the card's indented lines.
    body: String,
    tags: Vec<String>,
    statuses: Vec<StatusOption>,
}

#[derive(Default)]
struct Shared {
    vault: PathBuf,
    boards: Vec<BoardRef>,
    cards: RwLock<Vec<Entry>>,
    passes: super::passes::Passes,
}

/// The Obsidian tracker, shared by its collector and the window's actions.
#[derive(Clone, Default)]
pub struct Actions {
    shared: Arc<Shared>,
}

impl Actions {
    pub fn new(cfg: &ObsidianConfig) -> Self {
        let vault = crate::config::expand_home(Path::new(cfg.vault.trim()));
        let boards = cfg
            .boards
            .iter()
            .filter(|b| !b.path.trim().is_empty())
            .map(|b| {
                let file = vault.join(b.path.trim());
                let repo = b.repo.as_deref().map(str::trim).filter(|r| !r.is_empty()).map(String::from);
                let folder = file.parent().and_then(Path::file_name).map(|f| f.to_string_lossy().into_owned());
                let name = repo.clone().or(folder).unwrap_or_else(|| b.path.clone());
                BoardRef { file, name, repo }
            })
            .collect();
        Self { shared: Arc::new(Shared { vault, boards, ..Default::default() }) }
    }

    /// Reads the boards now, then whenever their files change, into the sprint.
    pub async fn run(self, hub: Hub) {
        let (tx, mut changed) = tokio::sync::mpsc::unbounded_channel();
        // Kept for as long as the loop runs: dropping it stops the watch.
        let watcher = self.watch(tx);
        let every = if watcher.is_some() { FALLBACK } else { POLL };
        let mut first = true;
        loop {
            if !std::mem::take(&mut first) {
                tokio::select! {
                    _ = hub.pause(every) => {}
                    _ = self.shared.passes.requested() => {}
                    Some(()) = changed.recv() => {
                        tokio::time::sleep(SETTLE).await;
                        while changed.try_recv().is_ok() {}
                    }
                }
            }
            self.shared.passes.begin();
            let error = self.publish(&hub).err().map(|e| format!("{e:#}"));
            self.shared.passes.end(error);
        }
    }

    /// Watches the boards' folders (their notes included): a change to a Markdown file there is sent on `tx`. None when the system refuses the watch.
    fn watch(&self, tx: tokio::sync::mpsc::UnboundedSender<()>) -> Option<notify::RecommendedWatcher> {
        use notify::{RecursiveMode, Watcher};
        let handler = move |event: notify::Result<notify::Event>| {
            if let Ok(e) = event
                && !e.kind.is_access()
                && e.paths.iter().any(|p| p.extension().is_some_and(|x| x == "md"))
            {
                let _ = tx.send(());
            }
        };
        let mut watcher = match notify::recommended_watcher(handler) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("obsidian: no file watch, reading every {}s: {e}", POLL.as_secs());
                return None;
            }
        };
        let mut folders: Vec<(PathBuf, RecursiveMode)> = self.shared.boards.iter().filter_map(|b| b.file.parent()).map(|d| (d.to_path_buf(), RecursiveMode::Recursive)).collect();
        folders.sort();
        folders.dedup_by(|a, b| a.0 == b.0);
        for (dir, mode) in &folders {
            if let Err(e) = watcher.watch(dir, *mode) {
                tracing::warn!("obsidian: cannot watch {}: {e}", dir.display());
            }
        }
        Some(watcher)
    }

    /// Reads the boards into the snapshot.
    fn publish(&self, hub: &Hub) -> Result<()> {
        match self.read() {
            Ok(Read { sprint, pickup, cards }) => {
                *self.shared.cards.write().unwrap() = cards;
                hub.update(|s| {
                    let changed = s.sprint.as_ref() != Some(&sprint) || s.pickup != pickup || s.tracker_me.is_some();
                    s.sprint = Some(sprint);
                    s.pickup = pickup;
                    s.tracker_me = None;
                    changed
                });
                hub.set_error(SOURCE, None);
                Ok(())
            }
            Err(e) => {
                tracing::warn!("obsidian: {e:#}");
                hub.set_error(SOURCE, Some(format!("{e:#}")));
                Err(e)
            }
        }
    }

    /// Immediate re-read; returns when it is done, with its error if any.
    pub async fn refresh(&self) -> Result<()> {
        self.shared.passes.request(Duration::from_secs(30), "Obsidian").await
    }

    fn read(&self) -> Result<Read> {
        let c = crate::conventions::get();
        self.read_with(&c.done_statuses, &c.pick_statuses)
    }

    /// A pass, with the done and "to pick" statuses.
    fn read_with(&self, done: &[String], pick: &[String]) -> Result<Read> {
        let sh = &self.shared;
        if sh.boards.is_empty() {
            bail!("{}", tr("no board: add [[obsidian.boards]]"));
        }
        if !sh.vault.is_dir() {
            bail!("{}", tr!("vault not found: {path}", path = sh.vault.display()));
        }
        let mut cards = Vec::new();
        let mut pick_boards = Vec::new();
        let mut statuses: Vec<StatusOption> = Vec::new();
        let mut boards = Vec::new();
        let mut vault_index: Option<HashMap<String, PathBuf>> = None;
        for board in &sh.boards {
            let text = std::fs::read_to_string(&board.file).with_context(|| tr!("reading {path}", path = board.file.display()))?;
            let lanes = parse_board(&text);
            let lane_statuses: Vec<StatusOption> = lanes
                .iter()
                .map(|l| StatusOption { closed: l.complete || done.contains(&status_of(&l.name)), status: status_of(&l.name), color: None })
                .collect();
            for s in &lane_statuses {
                if !statuses.iter().any(|x| x.status == s.status) {
                    statuses.push(s.clone());
                }
            }
            boards.push(SprintBoard { id: board.file.strip_prefix(&sh.vault).unwrap_or(&board.file).to_string_lossy().into_owned(), name: board.name.clone(), statuses: lane_statuses.clone() });
            let dir = board.file.parent().unwrap_or(&sh.vault);
            let mut index: Option<HashMap<String, PathBuf>> = None;
            let mut to_pick = Vec::new();
            for (lane, status) in lanes.iter().zip(&lane_statuses) {
                let mut seen: HashMap<&str, usize> = HashMap::new();
                for card in &lane.cards {
                    let nth = seen.entry(card.text.as_str()).or_default();
                    let rank = *nth;
                    *nth += 1;
                    if card.checked || status.closed {
                        continue;
                    }
                    let note = link(&card.text).and_then(|target| self.resolve(&target, dir, &mut index, &mut vault_index));
                    let entry = self.entry(board, card, rank, &status.status, note, &lane_statuses);
                    if pick.contains(&status.status) {
                        to_pick.push(entry.ticket.clone());
                    }
                    cards.push(entry);
                }
            }
            if !to_pick.is_empty() {
                pick_boards.push(PickBoard {
                    name: board.name.clone(),
                    kind: "board".into(),
                    tone: "info".into(),
                    pick: true,
                    sync: false,
                    url: self.url_of(Some(&board.file)),
                    tickets: to_pick,
                    statuses: lane_statuses.clone(),
                });
            }
        }
        let sprint = Sprint {
            name: "Kanban".into(),
            list_id: String::new(),
            url: self.url_of(None),
            start: None,
            end: None,
            tickets: cards.iter().filter(|e| !pick.contains(&e.ticket.status)).map(|e| e.ticket.clone()).collect(),
            statuses,
            boards,
        };
        Ok(Read { sprint, pickup: Pickup { statuses: pick.to_vec(), boards: pick_boards }, cards })
    }

    /// The note a link names: a path in the vault, or a name found in the board's folder first.
    fn resolve(&self, target: &str, dir: &Path, index: &mut Option<HashMap<String, PathBuf>>, vault_index: &mut Option<HashMap<String, PathBuf>>) -> Option<PathBuf> {
        let vault = &self.shared.vault;
        if target.contains('/') {
            let p = vault.join(format!("{target}.md"));
            return p.is_file().then_some(p);
        }
        let local = index.get_or_insert_with(|| markdown_files(dir)).get(target).cloned();
        local.or_else(|| vault_index.get_or_insert_with(|| markdown_files(vault)).get(target).cloned())
    }

    fn entry(&self, board: &BoardRef, card: &Card, nth: usize, status: &str, note: Option<PathBuf>, statuses: &[StatusOption]) -> Entry {
        let front = note.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| frontmatter(&t).0).unwrap_or_default();
        let tags = tags(&card.text);
        let field = |k: &str| front.get(k).map(String::as_str).filter(|v| !v.is_empty());
        let rel = |p: &Path| p.strip_prefix(&self.shared.vault).unwrap_or(p).to_string_lossy().into_owned();
        let key = match &note {
            Some(n) => rel(n),
            None => card.text.clone(),
        };
        let updated = note.as_ref().and_then(|n| std::fs::metadata(n).ok()?.modified().ok()).map(DateTime::<Utc>::from);
        // Without a note's id, one leading the card's title (`**C-68 — Build**`).
        let (titled_id, title) = split_id(&clean(&card.text));
        let ticket = Ticket {
            fields: Vec::new(),
            waiting_on: Vec::new(),
            blocking: Vec::new(),
            id: field("id").map(String::from).or(titled_id).unwrap_or_default(),
            // Two cards reading the same: the later ones keyed by their rank too.
            task_id: if nth == 0 { format!("{}#{key}", rel(&board.file)) } else { format!("{}#{key}#{nth}", rel(&board.file)) },
            parent: None,
            priority: field("priority").and_then(priority).or_else(|| tags.iter().find_map(|t| priority(t))),
            task_type: None,
            task_type_icon: None,
            list: Some(ListRef { id: rel(&board.file), name: board.name.clone() }),
            repo: board.repo.clone(),
            description: None,
            time_estimate: None,
            time_spent: None,
            tags: Vec::new(),
            name: field("title").map(String::from).unwrap_or(title),
            status: status.to_string(),
            closed: false,
            assignees: Vec::new(),
            url: self.url_of(Some(note.as_deref().unwrap_or(&board.file))),
            created_at: field("created").and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).and_then(|d| d.and_hms_opt(0, 0, 0)).map(|d| d.and_utc()),
            updated_at: updated,
        };
        Entry { ticket, board: board.file.clone(), card: card.text.clone(), nth, note, body: card.body.clone(), tags, statuses: statuses.to_vec() }
    }

    /// `obsidian://open` on the vault, or on one of its files.
    fn url_of(&self, file: Option<&Path>) -> String {
        let vault = self.shared.vault.file_name().map(|v| v.to_string_lossy().into_owned()).unwrap_or_default();
        let mut url = format!("obsidian://open?vault={}", encode(&vault));
        if let Some(f) = file {
            let rel = f.strip_prefix(&self.shared.vault).unwrap_or(f).with_extension("");
            url.push_str(&format!("&file={}", encode(&rel.to_string_lossy())));
        }
        url
    }

    /// A card's preview: `id` is its key (`task_id`), or its displayed id (the first board's card
    /// that has it).
    pub async fn ticket(&self, id: &str) -> Result<TaskView> {
        let Entry { ticket: t, note, body: card_body, tags, statuses, .. } = self.find(id)?;
        let (front, body) = match &note {
            Some(p) => {
                let text = std::fs::read_to_string(p).with_context(|| tr!("reading {path}", path = p.display()))?;
                let (front, body) = frontmatter(&text);
                (front, body.trim().to_string())
            }
            None => (BTreeMap::new(), String::new()),
        };
        // Its comments are those of the note's `## Comments`.
        let (body, found) = edit::comments(&body);
        // A note without a body (or none): the card's own description.
        let body = if body.is_empty() { card_body } else { body };
        let comments = found
            .into_iter()
            .enumerate()
            .map(|(n, (stamp, text))| Comment { id: n.to_string(), author: None, text, date: stamp_date(&stamp), replies: 0, mentions: Vec::new() })
            .collect();
        let mut all_tags: Vec<String> = tags.iter().map(|t| t.trim_start_matches('#').to_string()).collect();
        if let Some(kind) = front.get("type").filter(|k| !k.is_empty()) {
            all_tags.insert(0, kind.clone());
        }
        Ok(TaskView {
            fields: Vec::new(),
            task_id: t.task_id,
            id: t.id,
            name: t.name,
            task_type: None,
            task_type_icon: None,
            status_color: None,
            status: t.status,
            statuses,
            closed: false,
            priority: t.priority,
            assignees: Vec::new(),
            creator: None,
            tags: all_tags,
            list: t.list.map(|l| l.name),
            list_id: None,
            parent: None,
            url: t.url,
            description: body,
            created_at: t.created_at,
            updated_at: t.updated_at,
            due_at: None,
            time_estimate: None,
            time_spent: None,
            checklists: Vec::new(),
            subtasks: Vec::new(),
            comments,
            attachments: Vec::new(),
        })
    }

    /// The open cards of the boards that answer a search.
    pub fn search(&self, query: &str) -> Vec<Ticket> {
        crate::tracker::found(self.shared.cards.read().unwrap().iter().map(|e| e.ticket.clone()), query)
    }

    /// A card of the last pass: by its key (`task_id`), or by its displayed id (the first board's
    /// card that has it).
    fn find(&self, id: &str) -> Result<Entry> {
        let cards = self.shared.cards.read().unwrap();
        cards
            .iter()
            .find(|e| e.ticket.task_id == id)
            .or_else(|| cards.iter().find(|e| !e.ticket.id.is_empty() && e.ticket.id.eq_ignore_ascii_case(id)))
            .cloned()
            .with_context(|| tr!("\"{id}\" is not an open card of the boards", id = id))
    }

    /// Moves the card to the lane of `status` on its board. Its note's `status`, if it has one,
    /// follows: to the value the notes of that lane's cards have.
    pub async fn set_status(&self, hub: &Hub, task_id: &str, status: &str) -> Result<()> {
        let e = self.find(task_id)?;
        if e.ticket.status == status {
            return Ok(());
        }
        anyhow::ensure!(e.statuses.iter().any(|s| s.status == status), "{}", tr!("no lane \"{lane}\" on its board", lane = status));
        let note_status = match &e.note {
            Some(n) if frontmatter(&std::fs::read_to_string(n)?).0.contains_key("status") => self.note_status(&e.board, status),
            _ => None,
        };
        tracing::info!("obsidian: {} → {status}", e.ticket.task_id);
        rewrite(&e.board, |text| edit::move_card(text, e.spot(), status))?;
        if let (Some(note), Some(value)) = (&e.note, note_status) {
            rewrite(note, |text| Ok(edit::set_field(text, "status", Some(&value))))?;
        }
        // Read back now: the window follows without waiting for the next pass.
        self.publish(hub)
    }

    /// The `status` a card's note takes in the lane of `status`: the most common among the notes of
    /// that lane's cards, on its board or else on another; none without an example.
    fn note_status(&self, board: &Path, status: &str) -> Option<String> {
        let mut boards: Vec<&BoardRef> = self.shared.boards.iter().filter(|b| b.file == board).collect();
        boards.extend(self.shared.boards.iter().filter(|b| b.file != board));
        let mut vault_index = None;
        for b in boards {
            let Ok(text) = std::fs::read_to_string(&b.file) else { continue };
            let dir = b.file.parent().unwrap_or(&self.shared.vault);
            let mut index = None;
            let mut counts: HashMap<String, usize> = HashMap::new();
            let cards = parse_board(&text).into_iter().filter(|l| status_of(&l.name) == status).flat_map(|l| l.cards);
            for card in cards.take(10) {
                let Some(note) = link(&card.text).and_then(|t| self.resolve(&t, dir, &mut index, &mut vault_index)) else { continue };
                let Ok(note) = std::fs::read_to_string(note) else { continue };
                if let Some(v) = frontmatter(&note).0.remove("status").filter(|v| !v.is_empty()) {
                    *counts.entry(v).or_default() += 1;
                }
            }
            if let Some((value, _)) = counts.into_iter().max_by_key(|(v, n)| (*n, std::cmp::Reverse(v.clone()))) {
                return Some(value);
            }
        }
        None
    }

    /// Sets the card's priority (`None`: none): in its note's `priority` if it has one, else in its
    /// `#p0`–`#p3` tag; failing both, the note gets the field, a card without one the tag.
    pub async fn set_priority(&self, hub: &Hub, task_id: &str, priority: Option<crate::state::Priority>) -> Result<()> {
        let e = self.find(task_id)?;
        let level = priority.map(|p| LEVELS[p.rank()]);
        tracing::info!("obsidian: {} priority → {}", e.ticket.task_id, level.unwrap_or("none"));
        let in_note = match &e.note {
            Some(n) => frontmatter(&std::fs::read_to_string(n)?).0.contains_key("priority") || !edit::has_priority_tag(&e.card),
            None => false,
        };
        match &e.note {
            Some(note) if in_note => rewrite(note, |text| Ok(edit::set_field(text, "priority", level)))?,
            _ => rewrite(&e.board, |text| edit::edit_card(text, e.spot(), |t| edit::with_priority_tag(t, level)))?,
        }
        self.publish(hub)
    }

    /// Comments on the card: at the end of its note's `## Comments`, or, without a note, in its
    /// description.
    pub async fn add_comment(&self, hub: &Hub, task_id: &str, text: &str) -> Result<()> {
        let e = self.find(task_id)?;
        let lines = edit::comment_lines(&Local::now().format(STAMP).to_string(), text)?;
        tracing::info!("obsidian: comment on {}", e.ticket.task_id);
        match &e.note {
            Some(note) => rewrite(note, |t| Ok(edit::add_comment(t, &lines)))?,
            None => rewrite(&e.board, |t| edit::append_to_card(t, e.spot(), &lines))?,
        }
        self.publish(hub)
    }
}

impl Actions {
    /// Creates a card at the end of the lane of `status` on `board` (its list id). A board whose
    /// cards link notes gets a note too, numbered after the others (`T-013 — Title.md`) and shaped
    /// as the latest, the description under its title; its card links it. On a board without, a
    /// description gets a note of its own beside the board (`Title.md`), a title alone a plain
    /// card. Returns the new card's key.
    pub async fn create(&self, hub: &Hub, new: &crate::tracker::NewTicket) -> Result<String> {
        let (board, status) = (new.board.as_str(), new.status.as_str());
        let title = new.title.split_whitespace().collect::<Vec<_>>().join(" ");
        anyhow::ensure!(!title.is_empty(), "{}", tr("the card needs a title"));
        let sh = &self.shared;
        let b = sh.boards.iter().find(|b| b.file.strip_prefix(&sh.vault).unwrap_or(&b.file).to_string_lossy() == board).with_context(|| tr!("no board \"{board}\"", board = board))?;
        let text = std::fs::read_to_string(&b.file).with_context(|| tr!("reading {path}", path = b.file.display()))?;
        anyhow::ensure!(parse_board(&text).iter().any(|l| status_of(&l.name) == status), "{}", tr!("no lane \"{lane}\" on {board}", lane = status, board = b.name));
        let created = Local::now().format("%Y-%m-%d").to_string();
        // A board whose cards link numbered notes: the next one. (A note of its own, `Title.md`,
        // linked from a card, makes no numbering to follow.)
        let numbered = self.notes_folder(b, &text).and_then(|folder| {
            let names: Vec<String> = std::fs::read_dir(&folder).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".md")).collect();
            let id = edit::next_id(names.iter().map(String::as_str))?;
            Some((folder, names, id))
        });
        let (card, note) = match numbered {
            Some((folder, names, id)) => {
                let stem = format!("{id} — {}", file_safe(&title));
                // The latest note (highest number) shapes the new one.
                let latest = names.iter().filter(|n| edit::next_id([n.as_str()]).is_some()).max_by_key(|n| n.split(|c: char| !c.is_ascii_digit()).find(|s| !s.is_empty()).and_then(|s| s.parse::<u64>().ok()));
                let template = latest.and_then(|n| std::fs::read_to_string(folder.join(n)).ok());
                let mut fields = vec![("id", id.clone()), ("title", yaml_str(&title)), ("created", created)];
                if let Some(s) = self.note_status(&b.file, status) {
                    fields.push(("status", s));
                }
                let path = folder.join(format!("{stem}.md"));
                write_note(&path, &edit::new_note(template.as_deref(), &fields, &title, &new.description))?;
                (format!("[[{stem}]]"), Some(path))
            }
            None if !new.description.trim().is_empty() => {
                let stem = file_safe(&title);
                anyhow::ensure!(!stem.is_empty(), "{}", tr("the card needs a title"));
                let folder = b.file.parent().unwrap_or(&sh.vault);
                let fields = [("title", yaml_str(&title)), ("created", created)];
                let path = folder.join(format!("{stem}.md"));
                write_note(&path, &edit::new_note(None, &fields, &title, &new.description))?;
                (format!("[[{stem}]]"), Some(path))
            }
            None => (title.clone(), None),
        };
        if let Err(e) = rewrite(&b.file, |t| edit::add_card(t, status, &card)) {
            // No card for it: the note goes too, or the next try would find it in the way.
            if let Some(note) = note {
                let _ = std::fs::remove_file(note);
            }
            return Err(e);
        }
        tracing::info!("obsidian: new card on {} → {status}", b.name);
        self.publish(hub)?;
        let cards = sh.cards.read().unwrap();
        let on_board = |e: &&Entry| e.board == b.file && e.card == card;
        // The last one reading so: the new card ends its lane.
        Ok(cards.iter().rev().find(on_board).map(|e| e.ticket.task_id.clone()).unwrap_or_default())
    }

    /// The folder of the notes a board's cards link (the most common one), if they link any.
    fn notes_folder(&self, board: &BoardRef, text: &str) -> Option<PathBuf> {
        let dir = board.file.parent().unwrap_or(&self.shared.vault);
        let (mut index, mut vault_index) = (None, None);
        let mut counts: HashMap<PathBuf, usize> = HashMap::new();
        for card in parse_board(text).iter().flat_map(|l| &l.cards) {
            if let Some(folder) = link(&card.text).and_then(|t| self.resolve(&t, dir, &mut index, &mut vault_index)).and_then(|n| n.parent().map(Path::to_path_buf)) {
                *counts.entry(folder).or_default() += 1;
            }
        }
        counts.into_iter().max_by_key(|(p, n)| (*n, std::cmp::Reverse(p.clone()))).map(|(p, _)| p)
    }
}

/// A new note, never over another.
fn write_note(path: &Path, text: &str) -> Result<()> {
    tracing::info!("obsidian: new note {}", path.display());
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => anyhow::anyhow!(tr!("{path} exists already", path = path.display())),
        _ => anyhow::Error::from(e).context(tr!("writing {path}", path = path.display())),
    })?;
    std::io::Write::write_all(&mut file, text.as_bytes()).with_context(|| tr!("writing {path}", path = path.display()))
}

/// A title as a file name Obsidian accepts: without `* " \ / < > : | ? # ^ [ ]`.
fn file_safe(title: &str) -> String {
    let cleaned: String = title.chars().map(|c| if "*\"\\/<>:|?#^[]".contains(c) { ' ' } else { c }).collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A YAML string value: as is when plain, quoted when it could read otherwise.
fn yaml_str(value: &str) -> String {
    let plain = !value.contains(": ") && !value.contains(" #") && !value.starts_with(|c: char| "[]{}&*!|>'\"%@`#,?:-".contains(c));
    if plain { value.to_string() } else { format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")) }
}

/// Prism's priorities and their Obsidian levels.
/// Each priority's tag, from the most urgent (`Priority::rank`).
const LEVELS: [&str; 4] = ["p0", "p1", "p2", "p3"];

/// A comment's stamp, in local time.
const STAMP: &str = "%Y-%m-%d %H:%M";

fn stamp_date(stamp: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(stamp, STAMP).ok().or_else(|| NaiveDate::parse_from_str(stamp, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0))?;
    Local.from_local_datetime(&naive).single().map(|d| d.with_timezone(&Utc))
}

/// Rewrites a file of the vault: read, changed, then replaced whole (written beside it, then
/// renamed), unless it changed in the meantime.
fn rewrite(path: &Path, change: impl FnOnce(&str) -> Result<String>) -> Result<()> {
    let before = std::fs::read_to_string(path).with_context(|| tr!("reading {path}", path = path.display()))?;
    let after = change(&before)?;
    if after == before {
        return Ok(());
    }
    let name = path.file_name().context("not a file")?.to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.prism"));
    std::fs::write(&tmp, &after).with_context(|| tr!("writing {path}", path = tmp.display()))?;
    if std::fs::read_to_string(path).ok().as_deref() != Some(before.as_str()) {
        std::fs::remove_file(&tmp).ok();
        bail!("{}", tr!("{path} changed meanwhile: try again", path = path.display()));
    }
    std::fs::rename(&tmp, path).with_context(|| tr!("replacing {path}", path = path.display()))
}

impl Entry {
    fn spot(&self) -> edit::Spot<'_> {
        edit::Spot { text: &self.card, status: &self.ticket.status, nth: self.nth }
    }
}

/// A pass over the boards.
struct Read {
    sprint: Sprint,
    pickup: Pickup,
    /// Every open card, the sprint's and To pick's.
    cards: Vec<Entry>,
}

#[derive(Debug, PartialEq)]
struct Lane {
    name: String,
    /// Marked complete in Obsidian (`**Complete**`): its cards are done.
    complete: bool,
    cards: Vec<Card>,
}

#[derive(Debug, PartialEq)]
struct Card {
    checked: bool,
    /// Its first line.
    text: String,
    /// Its indented lines, unindented: the card's description.
    body: String,
}

/// The lanes of a board's file, up to the archive (`***`) or the plugin's settings.
fn parse_board(text: &str) -> Vec<Lane> {
    let lines: Vec<&str> = text.split('\n').collect();
    edit::layout(&lines)
        .into_iter()
        .map(|l| Lane {
            complete: l.complete.is_some(),
            cards: l.cards.iter().map(|c| Card { checked: c.checked, text: c.text.clone(), body: edit::body(&lines, c) }).collect(),
            name: l.name,
        })
        .collect()
}

static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]+))?\]\]").unwrap());
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s)(#[\p{L}\p{N}_/-]+)").unwrap());
static TITLED_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Z][A-Z0-9]*-\d+)\s*[—–:-]\s*(.*)$").unwrap());
static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"@@?\{[^}]*\}").unwrap());

/// A lane's name as a status: without its leading emoji, lowercase (`🚧 En cours` → `en cours`).
fn status_of(lane: &str) -> String {
    lane.trim_start_matches(|c: char| !c.is_alphanumeric()).trim().to_lowercase()
}

/// The note a card links first (`[[T-12]]`, `[[folder/T-12|alias]]`).
fn link(text: &str) -> Option<String> {
    LINK.captures(text).map(|c| c[1].trim().to_string())
}

fn tags(text: &str) -> Vec<String> {
    TAG.captures_iter(text).map(|c| c[1].to_string()).collect()
}

/// A title led by an id (`C-68 — Build`, `T-12: Fix`): the id, and the rest; otherwise the title.
fn split_id(title: &str) -> (Option<String>, String) {
    match TITLED_ID.captures(title) {
        Some(c) if !c[2].trim().is_empty() => (Some(c[1].to_string()), c[2].trim().to_string()),
        _ => (None, title.to_string()),
    }
}

/// A card's text as a title: links by their alias or note, without tags, dates nor bold.
fn clean(text: &str) -> String {
    let text = LINK.replace_all(text, |c: &regex::Captures| c.get(2).unwrap_or_else(|| c.get(1).unwrap()).as_str().to_string());
    let text = TAG.replace_all(&text, "");
    let text = DATE.replace_all(&text, "").replace("**", "");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `p0` to `p3` (a frontmatter value or a tag), or a ClickUp-like level.
fn priority(value: &str) -> Option<crate::state::Priority> {
    let value = value.trim().trim_start_matches('#').to_lowercase();
    match LEVELS.iter().position(|l| *l == value) {
        Some(rank) => Some(crate::state::Priority::ALL[rank]),
        None if value == "medium" => Some(crate::state::Priority::Normal),
        None => crate::state::Priority::of(&value),
    }
}

/// A note's frontmatter (its `key: value` lines; lists are kept as written) and the rest.
fn frontmatter(text: &str) -> (BTreeMap<String, String>, &str) {
    let mut fields = BTreeMap::new();
    let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) else { return (fields, text) };
    let Some(end) = rest.find("\n---") else { return (fields, text) };
    for line in rest[..end].lines() {
        if let Some((k, v)) = line.split_once(':')
            && !k.starts_with(char::is_whitespace)
        {
            fields.insert(k.trim().to_string(), v.trim().trim_matches(|c| c == '"' || c == '\'').to_string());
        }
    }
    let body = rest[end + 4..].trim_start_matches(|c| c != '\n').trim_start_matches('\n');
    (fields, body)
}

/// The Markdown files under a folder, by name without `.md` (the first found wins, as Obsidian's
/// shortest-path links do). Hidden folders (`.obsidian`, `.git`, `.trash`) are skipped.
fn markdown_files(dir: &Path) -> HashMap<String, PathBuf> {
    let mut found = HashMap::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        let mut subdirs = Vec::new();
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                subdirs.push(path);
            } else if let Some(stem) = name.strip_suffix(".md") {
                found.entry(stem.to_string()).or_insert(path);
            }
        }
        if depth < 12 {
            stack.extend(subdirs.into_iter().map(|p| (p, depth + 1)));
        }
    }
    found
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Priority;

    const BOARD: &str = "---\nkanban-plugin: board\n---\n\n## 🎯 Next\n\n- [ ] [[T-2]] #p1 #ui\n\n## 📥 Backlog\n\n- [ ] Plain card #chore @{2026-10-12}\n\tAcceptance: it reads.\n\t- [ ] a step\n- [ ] **C-68 — A standalone build** #dist\n\tWhy: testers.\n- [x] Checked [[T-9|the alias]]\n\n## ✅ Done\n\n**Complete**\n- [x] [[T-1]] #core\n\n***\n\n## Archive\n\n- [x] Old\n\n%% kanban:settings\n```\n{\"kanban-plugin\":\"board\"}\n```\n%%\n";

    #[test]
    fn reads_lanes_cards_and_completion() {
        let lanes = parse_board(BOARD);
        assert_eq!(lanes.iter().map(|l| (l.name.as_str(), l.complete, l.cards.len())).collect::<Vec<_>>(), [("🎯 Next", false, 1), ("📥 Backlog", false, 3), ("✅ Done", true, 1)]);
        assert_eq!(lanes[1].cards[0], Card { checked: false, text: "Plain card #chore @{2026-10-12}".into(), body: "Acceptance: it reads.\n- [ ] a step".into() });
        assert_eq!(lanes[1].cards[1].body, "Why: testers.");
        assert!(lanes[1].cards[2].checked);
    }

    #[test]
    fn lanes_read_as_statuses() {
        assert_eq!(status_of("🚧 En cours"), "en cours");
        assert_eq!(status_of("⏸ Ignoré"), "ignoré");
        assert_eq!(status_of("In Progress"), "in progress");
        assert_eq!(status_of("Milestone 0 — First 30 seconds"), "milestone 0 — first 30 seconds");
    }

    #[test]
    fn cards_read_as_titles_links_and_tags() {
        assert_eq!(link("[[T-2]] #p1").as_deref(), Some("T-2"));
        assert_eq!(link("see [[Tickets/T-9#Notes|alias]]").as_deref(), Some("Tickets/T-9"));
        assert_eq!(link("no link"), None);
        assert_eq!(clean("Checked [[T-9|the alias]] #ui @{2026-10-12} @@{10:00}"), "Checked the alias");
        assert_eq!(clean("[[T-2]] #p1 #epic/gui"), "T-2");
        assert_eq!(tags("[[T-2]] #p1 #epic/gui C#"), ["#p1", "#epic/gui"]);
    }

    #[test]
    fn ids_leading_titles() {
        assert_eq!(split_id(&clean("**C-68 — A standalone build** #dist")), (Some("C-68".into()), "A standalone build".into()));
        assert_eq!(split_id("T-12: Fix it"), (Some("T-12".into()), "Fix it".into()));
        assert_eq!(split_id("Write notes"), (None, "Write notes".into()));
        assert_eq!(split_id("C-68"), (None, "C-68".into()), "an id alone stays the title");
        // A note named after its id (`T-260 — Title.md`) and without frontmatter: its link says it.
        assert_eq!(split_id(&clean("[[T-260 — Add the build recipe]] #build")), (Some("T-260".into()), "Add the build recipe".into()));
    }

    #[test]
    fn priorities_from_levels_or_tags() {
        assert_eq!(priority("p0").map(Priority::name), Some("urgent"));
        assert_eq!(priority("#p3").map(Priority::name), Some("low"));
        assert_eq!(priority("High").map(Priority::name), Some("high"));
        assert_eq!(priority("#ui"), None);
    }

    #[test]
    fn reads_the_frontmatter() {
        let (f, body) = frontmatter("---\nid: T-12\ntitle: \"Fix: the parser\"\ntags: [a, b]\n  nested: x\n---\n# Body\n");
        assert_eq!((f["id"].as_str(), f["title"].as_str(), f["tags"].as_str()), ("T-12", "Fix: the parser", "[a, b]"));
        assert!(!f.contains_key("nested"));
        assert_eq!(body, "# Body\n");
        assert_eq!(frontmatter("no front").1, "no front");
    }

    /// A vault on disk: cards resolve their notes in the board's folder, then anywhere in the vault.
    #[test]
    fn reads_boards_into_the_sprint() {
        let vault = std::env::temp_dir().join(format!("prism-obsidian-{}", std::process::id())).join("Notes");
        let tickets = vault.join("Projects/Things/Tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        std::fs::create_dir_all(vault.join("Elsewhere")).unwrap();
        std::fs::write(vault.join("Projects/Things/Board.md"), BOARD).unwrap();
        std::fs::write(tickets.join("T-2.md"), "---\nid: T-2\ntitle: Parse the lanes\npriority: p2\ncreated: 2026-10-01\n---\nBody\n").unwrap();
        let cfg: ObsidianConfig = toml::from_str(&format!("vault = {:?}\n[[boards]]\npath = \"Projects/Things/Board.md\"\nrepo = \"Things\"\n", vault.to_string_lossy())).unwrap();
        let actions = Actions::new(&cfg);
        let Read { sprint, cards, .. } = actions.read().unwrap();
        assert_eq!(sprint.tickets.len(), 3, "open cards only");
        let t = &sprint.tickets[0];
        assert_eq!((t.id.as_str(), t.name.as_str(), t.status.as_str(), t.priority.map(Priority::name), t.repo.as_deref()), ("T-2", "Parse the lanes", "next", Some("normal"), Some("Things")));
        assert_eq!(t.task_id, "Projects/Things/Board.md#Projects/Things/Tickets/T-2.md");
        assert_eq!(t.url, "obsidian://open?vault=Notes&file=Projects%2FThings%2FTickets%2FT-2");
        assert_eq!(t.list.as_ref().map(|l| l.name.as_str()), Some("Things"));
        let plain = &sprint.tickets[1];
        assert_eq!((plain.id.as_str(), plain.name.as_str(), plain.status.as_str()), ("", "Plain card", "backlog"));
        let titled = &sprint.tickets[2];
        assert_eq!((titled.id.as_str(), titled.name.as_str()), ("C-68", "A standalone build"));
        *actions.shared.cards.write().unwrap() = cards.clone();
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        assert_eq!(rt.block_on(actions.ticket("C-68")).unwrap().description, "Why: testers.", "the card's own description");
        assert_eq!(rt.block_on(actions.ticket("T-2")).unwrap().description, "Body", "the note's body");
        assert_eq!(sprint.statuses.iter().map(|s| (s.status.as_str(), s.closed)).collect::<Vec<_>>(), [("next", false), ("backlog", false), ("done", true)]);
        assert_eq!(cards[0].note.as_deref(), Some(tickets.join("T-2.md").as_path()));
        // Backlog's cards ready to be picked: To pick's, out of the sprint.
        let read = actions.read_with(&[], &["backlog".into()]).unwrap();
        assert_eq!(read.sprint.tickets.iter().map(|t| t.status.as_str()).collect::<Vec<_>>(), ["next"]);
        let b = &read.pickup.boards[0];
        assert_eq!((b.name.as_str(), b.tickets.len(), b.pick, b.statuses.len()), ("Things", 2, true, 3));
        assert_eq!(read.cards.len(), 3, "every open card can still be previewed");
        std::fs::remove_dir_all(vault.parent().unwrap()).ok();
    }

    /// Changes from the window land in the files: the card moves, its note's status follows the
    /// lane's other notes, the priority and the comments go where they live.
    #[test]
    fn writes_back_to_the_vault() {
        let vault = std::env::temp_dir().join(format!("prism-obsidian-w-{}", std::process::id())).join("Notes");
        let tickets = vault.join("P/Tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        let board = "---\nkanban-plugin: board\n---\n\n## 📥 Backlog\n\n- [ ] [[T-1]]\n- [ ] Loose card #p2\n\n## 🚧 En cours\n\n- [ ] [[T-9]]\n\n## ✅ Terminé\n\n**Complete**\n\n";
        std::fs::write(vault.join("P/Board.md"), board).unwrap();
        std::fs::write(tickets.join("T-1.md"), "---\nid: T-1\ntitle: One\nstatus: todo\npriority: p3\n---\nBody\n").unwrap();
        std::fs::write(tickets.join("T-9.md"), "---\nid: T-9\nstatus: in-progress\n---\n").unwrap();
        let cfg: ObsidianConfig = toml::from_str(&format!("vault = {:?}\n[[boards]]\npath = \"P/Board.md\"\nrepo = \"P\"\n", vault.to_string_lossy())).unwrap();
        let actions = Actions::new(&cfg);
        let hub = Hub::new();
        actions.publish(&hub).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let note = || std::fs::read_to_string(tickets.join("T-1.md")).unwrap();
        let file = || std::fs::read_to_string(vault.join("P/Board.md")).unwrap();

        rt.block_on(actions.set_status(&hub, "T-1", "en cours")).unwrap();
        assert!(file().contains("## 🚧 En cours\n\n- [ ] [[T-9]]\n- [ ] [[T-1]]\n"), "{}", file());
        assert!(note().contains("status: in-progress\n"), "learned from T-9: {}", note());
        let snap = hub.subscribe().borrow().clone();
        assert_eq!(snap.sprint.unwrap().tickets.iter().find(|t| t.id == "T-1").map(|t| t.status.clone()).as_deref(), Some("en cours"), "published at once");

        rt.block_on(actions.set_priority(&hub, "T-1", Some(Priority::Urgent))).unwrap();
        assert!(note().contains("priority: p0\n"));
        let loose = "P/Board.md#Loose card #p2";
        rt.block_on(actions.set_priority(&hub, loose, Some(Priority::High))).unwrap();
        assert!(file().contains("- [ ] Loose card #p1\n"), "{}", file());

        rt.block_on(actions.add_comment(&hub, "T-1", "Looks good")).unwrap();
        let view = rt.block_on(actions.ticket("T-1")).unwrap();
        assert_eq!((view.description.as_str(), view.comments.len(), view.comments[0].text.as_str()), ("Body", 1, "Looks good"));
        assert!(view.comments[0].date.is_some());
        rt.block_on(actions.add_comment(&hub, "P/Board.md#Loose card #p1", "On the card")).unwrap();
        assert!(file().contains("- [ ] Loose card #p1\n\t- **"), "{}", file());

        rt.block_on(actions.set_status(&hub, "T-1", "terminé")).unwrap();
        assert!(file().contains("**Complete**\n\n- [x] [[T-1]]"), "{}", file());
        assert!(note().contains("status: in-progress\n"), "no example in Terminé: the note's status stays");
        std::fs::remove_dir_all(vault.parent().unwrap()).ok();
    }


    /// A new card on a board with notes gets the next numbered note, shaped as the latest one; on a
    /// board without, a plain card.
    #[test]
    fn creates_cards_and_notes() {
        let new = |board: &str, status: &str, title: &str, description: &str| crate::tracker::NewTicket { board: board.into(), status: status.into(), title: title.into(), description: description.into(), assign_me: false };
        let vault = std::env::temp_dir().join(format!("prism-obsidian-c-{}", std::process::id())).join("Notes");
        let tickets = vault.join("P/Tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        std::fs::write(vault.join("P/Board.md"), "## Backlog\n\n- [ ] [[T-008 — Old]]\n\n## Doing\n\n").unwrap();
        std::fs::write(vault.join("Q.md"), "## Todo\n\n- [ ] C-1 — loose\n").unwrap();
        std::fs::write(tickets.join("T-008 — Old.md"), "---\nid: T-008\ntitle: Old\nstatus: todo\ntags: [x]\n---\n# Old\n\n## Context\n\nwhy\n").unwrap();
        let cfg: ObsidianConfig = toml::from_str(&format!("vault = {:?}\n[[boards]]\npath = \"P/Board.md\"\n[[boards]]\npath = \"Q.md\"\n", vault.to_string_lossy())).unwrap();
        let actions = Actions::new(&cfg);
        let hub = Hub::new();
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let key = rt.block_on(actions.create(&hub, &new("P/Board.md", "backlog", "Fix: the  parser", ""))).unwrap();
        assert_eq!(key, "P/Board.md#P/Tickets/T-009 — Fix the parser.md");
        let note = std::fs::read_to_string(tickets.join("T-009 — Fix the parser.md")).unwrap();
        assert!(note.starts_with("---\nid: T-009\ntitle: \"Fix: the parser\"\nstatus: todo\ntags: []\ncreated: "), "{note}");
        assert!(note.ends_with("---\n# Fix: the parser\n\n## Context\n"), "{note}");
        let board = std::fs::read_to_string(vault.join("P/Board.md")).unwrap();
        assert!(board.contains("- [ ] [[T-008 — Old]]\n- [ ] [[T-009 — Fix the parser]]\n"), "{board}");
        let t = rt.block_on(actions.ticket(&key)).unwrap();
        assert_eq!((t.id.as_str(), t.name.as_str()), ("T-009", "Fix: the parser"));
        let plain = rt.block_on(actions.create(&hub, &new("Q.md", "todo", "C-2 — another", ""))).unwrap();
        assert_eq!(plain, "Q.md#C-2 — another");
        assert!(rt.block_on(actions.create(&hub, &new("Q.md", "nowhere", "x", ""))).is_err());
        // A description: under the title of the numbered note; on a board without notes, in a note
        // of its own beside the board, which the card links.
        let key = rt.block_on(actions.create(&hub, &new("P/Board.md", "doing", "Described", "Why it matters.\n"))).unwrap();
        let note = std::fs::read_to_string(tickets.join("T-010 — Described.md")).unwrap();
        assert!(note.ends_with("# Described\n\nWhy it matters.\n\n## Context\n"), "{note}");
        assert_eq!(key, "P/Board.md#P/Tickets/T-010 — Described.md");
        let key = rt.block_on(actions.create(&hub, &new("Q.md", "todo", "Loose: one", "Body"))).unwrap();
        let note = std::fs::read_to_string(vault.join("Loose one.md")).unwrap();
        assert!(note.starts_with("---\ntitle: \"Loose: one\"\ncreated: ") && note.ends_with("---\n# Loose: one\n\nBody\n"), "{note}");
        assert!(std::fs::read_to_string(vault.join("Q.md")).unwrap().contains("- [ ] [[Loose one]]\n"));
        assert_eq!(key, "Q.md#Loose one.md");
        // Never over a note that exists; the board takes cards still, its loose note no numbering.
        let again = rt.block_on(actions.create(&hub, &new("Q.md", "todo", "Loose one", "Again"))).unwrap_err().to_string();
        assert!(again.contains("exists"), "{again}");
        assert!(!std::fs::read_to_string(vault.join("Q.md")).unwrap().contains("[[Loose one]]\n- [ ] [[Loose one]]"));
        assert_eq!(rt.block_on(actions.create(&hub, &new("Q.md", "todo", "C-3 — plain", ""))).unwrap(), "Q.md#C-3 — plain");
        rt.block_on(actions.create(&hub, &new("Q.md", "todo", "Second loose", "Body"))).unwrap();
        assert!(vault.join("Second loose.md").exists());
        std::fs::remove_dir_all(vault.parent().unwrap()).ok();
    }


    /// A change to a board's file is read at once, not at the next interval.
    #[tokio::test]
    async fn follows_file_changes() {
        let vault = std::env::temp_dir().join(format!("prism-obsidian-n-{}", std::process::id())).join("Notes");
        std::fs::create_dir_all(vault.join("P")).unwrap();
        std::fs::write(vault.join("P/Board.md"), "## Todo\n\n- [ ] One\n").unwrap();
        let cfg: ObsidianConfig = toml::from_str(&format!("vault = {:?}\n[[boards]]\npath = \"P/Board.md\"\n", vault.to_string_lossy())).unwrap();
        let hub = Hub::new();
        let reader = prism_base::rt::spawn(Actions::new(&cfg).run(hub.clone()));
        let count = |hub: &Hub| hub.subscribe().borrow().sprint.as_ref().map_or(0, |s| s.tickets.len());
        let wait = |want: usize| {
            let hub = hub.clone();
            async move {
                for _ in 0..50 {
                    if count(&hub) == want {
                        return true;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                false
            }
        };
        assert!(wait(1).await, "first read");
        std::fs::write(vault.join("P/Board.md"), "## Todo\n\n- [ ] One\n- [ ] Two\n").unwrap();
        assert!(wait(2).await, "the change is read within 5 s, the fallback being 60 s");
        reader.abort();
        std::fs::remove_dir_all(vault.parent().unwrap()).ok();
    }

}
