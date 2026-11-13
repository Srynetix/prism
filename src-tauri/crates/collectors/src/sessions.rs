use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::Result;
use chrono::DateTime;
use serde::Deserialize;

use crate::{
    config::SessionsConfig,
    state::{ArchivedSession, ClaudeSession, Hub, SessionOrigin, SessionStatus},
    ticket,
};

const SOURCE: &str = "sessions";
/// A transcript written more recently than this belongs to a session that is working.
const ACTIVE_WRITE: Duration = Duration::from_secs(90);
/// Size read at the end of the transcript to find the last exchange.
const TAIL_BYTES: u64 = 256 * 1024;

/// Reads the state of the desktop's Claude Code sessions from disk: nothing leaves the machine,
/// and only metadata and the end-of-turn status are kept, never the conversation.
/// `scope`: the sessions the active profile shows (see `profile::SessionScope`).
pub async fn run(cfg: SessionsConfig, scope: crate::profile::SessionScope, hub: Hub) {
    let every = Duration::from_secs(cfg.poll_seconds.max(1));
    let mut transcripts: HashMap<String, PathBuf> = HashMap::new();
    let mut first = true;
    loop {
        if !std::mem::take(&mut first) {
            hub.pause(every).await;
        }
        let dir = cfg.dir.clone();
        let projects = cfg.transcripts.clone();
        let mut cache = std::mem::take(&mut transcripts);
        let scope = scope.clone();
        let result = tokio::task::spawn_blocking(move || {
            let r = collect(&dir, &projects, &scope, &mut cache);
            (r, cache)
        })
        .await;

        let Ok((result, cache)) = result else { continue };
        transcripts = cache;
        match result {
            Ok((list, archived)) => {
                let mut list = list;
                // Those marked read wait for me no more, until they move again.
                for c in &mut list {
                    prism_core::read_marks::apply(&c.id, &mut c.status, &mut c.needs_action, c.last_activity, &mut c.read);
                }
                hub.update(|s| {
                    let changed = s.archived_sessions != archived;
                    s.archived_sessions = archived;
                    changed
                });
                // Prism's agents share the list: only the desktop sessions are replaced.
                hub.update(|s| {
                    let changed = !s.sessions.iter().filter(|c| c.origin == SessionOrigin::Desktop).eq(list.iter());
                    s.sessions.retain(|c| c.origin != SessionOrigin::Desktop);
                    s.sessions.extend(list);
                    changed
                });
                // After the data: whoever sees the source as answered also sees what it brought.
                hub.set_error(SOURCE, None);
            }
            Err(e) => {
                tracing::warn!("sessions: {e:#}");
                hub.set_error(SOURCE, Some(format!("{e:#}")));
            }
        }
    }
}

fn collect(dir: &Path, projects: &Path, shown: &crate::profile::SessionScope, transcripts: &mut HashMap<String, PathBuf>) -> Result<(Vec<ClaudeSession>, Vec<ArchivedSession>)> {
    let mut out = Vec::new();
    let mut archived = Vec::new();
    // No Claude Desktop app (Codex alone, or the CLI only): no session, and nothing wrong.
    if !dir.is_dir() {
        return Ok((out, archived));
    }
    let groups = read_groups(dir);
    // Desktop tree: <account>/<organization>/local_<id>.json
    for account in read_dirs(dir)? {
        for org in read_dirs(&account)? {
            let scope = format!(
                "{}/{}",
                account.file_name().unwrap_or_default().to_string_lossy(),
                org.file_name().unwrap_or_default().to_string_lossy(),
            );
            let scope_groups = groups.get(&scope);
            for entry in std::fs::read_dir(&org)?.flatten() {
                let path = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !(name.starts_with("local_") && name.ends_with(".json")) {
                    continue;
                }
                let Ok(raw) = std::fs::read(&path) else { continue };
                let Ok(meta) = serde_json::from_slice::<RawSession>(&raw) else { continue };
                let folder = meta.worktree_path.as_deref().filter(|p| !p.is_empty()).or(meta.cwd.as_deref()).unwrap_or_default();
                if !shown.keeps(Path::new(folder)) {
                    continue;
                }
                if meta.is_archived {
                    // Only those that had a worktree count: the Worktrees view lists them.
                    if let Some(worktree) = meta.worktree_path.filter(|p| !p.is_empty()) {
                        archived.push(ArchivedSession {
                            id: meta.session_id,
                            title: meta.title.unwrap_or_else(|| "Untitled".into()),
                            worktree,
                            branch: meta.branch.filter(|b| !b.is_empty()),
                            last_activity: meta.last_activity_at.and_then(DateTime::from_timestamp_millis),
                        });
                    }
                    continue;
                }
                let transcript = meta.cli_session_id.as_deref().and_then(|id| locate(projects, id, transcripts));
                let mut session = summarize(meta, transcript.as_deref());
                if let Some(g) = scope_groups {
                    g.place(&mut session);
                }
                out.push(session);
            }
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.last_activity));
    archived.sort_by_key(|a| std::cmp::Reverse(a.last_activity));
    Ok((out, archived))
}

fn summarize(m: RawSession, transcript: Option<&Path>) -> ClaudeSession {
    let summary = m.post_turn_summary.unwrap_or_default();
    let tail = transcript.and_then(last_exchange);
    let written = transcript
        .and_then(|t| std::fs::metadata(t).ok()?.modified().ok())
        .and_then(|t| SystemTime::now().duration_since(t).ok());

    let mid_turn = matches!(tail, Some(Exchange::ToolCall | Exchange::ToolResult | Exchange::UserPrompt));
    let status = match (mid_turn, written) {
        (true, Some(age)) if age < ACTIVE_WRITE => SessionStatus::Running,
        // A tool call left without follow-up: permission to grant, or a long-running command.
        (true, Some(_)) if tail == Some(Exchange::ToolCall) => SessionStatus::WaitingTool,
        _ => match summary.status_category.as_deref() {
            Some("blocked") => SessionStatus::NeedsYou,
            Some("review_ready") => SessionStatus::ReviewReady,
            Some("completed") => SessionStatus::Completed,
            _ => SessionStatus::Idle,
        },
    };

    let worktree = m.worktree_path.clone().filter(|p| !p.is_empty());
    let folder = worktree.as_deref().or(m.cwd.as_deref()).unwrap_or_default();
    let title = m.title.unwrap_or_else(|| "Untitled".into());
    let path = Some(folder.to_string()).filter(|f| !f.is_empty());
    ClaudeSession {
        id: m.session_id,
        ticket: ticket::find_in_session(&title, m.branch.as_deref()),
        folder: Path::new(folder).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        worktree,
        branch: m.branch.filter(|b| !b.is_empty()),
        title,
        status,
        needs_action: summary.needs_action.filter(|s| !s.trim().is_empty()),
        read: false,
        last_activity: m.last_activity_at.and_then(DateTime::from_timestamp_millis),
        group: None,
        group_order: None,
        order_in_group: None,
        origin: SessionOrigin::Desktop,
        transcript: transcript.map(|t| t.display().to_string()),
        path,
    }
}

/// Desktop sidebar groups, by `<account>/<organization>` scope.
struct ScopeGroups {
    /// (id, name) in sidebar order.
    groups: Vec<(String, String)>,
    /// `code:<session id>` → group id.
    assignments: HashMap<String, String>,
    /// group id → sessions in manual order.
    order: HashMap<String, Vec<String>>,
}

impl ScopeGroups {
    fn place(&self, session: &mut ClaudeSession) {
        let key = format!("code:{}", session.id);
        let Some(group_id) = self.assignments.get(&key) else { return };
        let Some(index) = self.groups.iter().position(|(id, _)| id == group_id) else { return };
        session.group = Some(self.groups[index].1.clone());
        session.group_order = Some(index as u32);
        session.order_in_group = self
            .order
            .get(group_id)
            .and_then(|o| o.iter().position(|k| *k == key))
            .map(|i| i as u32);
    }
}

/// Reads `claude_desktop_config.json`, next to the sessions folder. An app-internal format:
/// on failure, the sessions simply stay ungrouped.
fn read_groups(sessions_dir: &Path) -> HashMap<String, ScopeGroups> {
    let Some(path) = sessions_dir.parent().map(|d| d.join("claude_desktop_config.json")) else {
        return HashMap::new();
    };
    let Ok(raw) = std::fs::read(&path) else { return HashMap::new() };
    let Ok(cfg) = serde_json::from_slice::<serde_json::Value>(&raw) else { return HashMap::new() };
    let Some(scopes) = cfg["preferences"]["epitaxyPrefs"]["dframe-group-scopes"].as_object() else {
        return HashMap::new();
    };
    scopes
        .iter()
        .map(|(scope, v)| {
            let groups = v["groups"]
                .as_array()
                .map(|gs| {
                    gs.iter()
                        .filter_map(|g| Some((g["id"].as_str()?.to_string(), g["name"].as_str()?.to_string())))
                        .collect()
                })
                .unwrap_or_default();
            let assignments = v["assignments"]
                .as_object()
                .map(|a| a.iter().filter_map(|(k, g)| Some((k.clone(), g.as_str()?.to_string()))).collect())
                .unwrap_or_default();
            let order = v["order"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .map(|(g, keys)| {
                            let keys = keys.as_array().map(|k| k.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                            (g.clone(), keys)
                        })
                        .collect()
                })
                .unwrap_or_default();
            (scope.clone(), ScopeGroups { groups, assignments, order })
        })
        .collect()
}

/// Finds a session's transcript (`~/.claude/projects/<encoded folder>/<id>.jsonl`), once and for all.
fn locate(projects: &Path, id: &str, cache: &mut HashMap<String, PathBuf>) -> Option<PathBuf> {
    if let Some(p) = cache.get(id).filter(|p| p.exists()) {
        return Some(p.clone());
    }
    let file = format!("{id}.jsonl");
    let found = read_dirs(projects).ok()?.into_iter().map(|d| d.join(&file)).find(|p| p.exists())?;
    cache.insert(id.to_string(), found.clone());
    Some(found)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Exchange {
    UserPrompt,
    ToolCall,
    ToolResult,
    TurnEnded,
}

/// Nature of the transcript's last exchange, ignoring technical records.
/// Only the type and the stop reason are read: never the text.
fn last_exchange(path: &Path) -> Option<Exchange> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;

    buf.split(|b| *b == b'\n').rev().find_map(|line| {
        let r: RawRecord = serde_json::from_slice(line).ok()?;
        let msg = r.message?;
        match r.kind.as_str() {
            "assistant" => Some(match msg.stop_reason.as_deref() {
                Some("tool_use") => Exchange::ToolCall,
                _ => Exchange::TurnEnded,
            }),
            "user" => Some(match msg.content {
                serde_json::Value::Array(items) if items.iter().any(|i| i.get("type").and_then(|t| t.as_str()) == Some("tool_result")) => {
                    Exchange::ToolResult
                }
                _ => Exchange::UserPrompt,
            }),
            _ => None,
        }
    })
}

fn read_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(std::fs::read_dir(dir)?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSession {
    session_id: String,
    cli_session_id: Option<String>,
    cwd: Option<String>,
    worktree_path: Option<String>,
    branch: Option<String>,
    title: Option<String>,
    last_activity_at: Option<i64>,
    #[serde(default)]
    is_archived: bool,
    post_turn_summary: Option<RawSummary>,
}

#[derive(Deserialize, Default)]
struct RawSummary {
    status_category: Option<String>,
    needs_action: Option<String>,
}

#[derive(Deserialize)]
struct RawRecord {
    #[serde(rename = "type")]
    kind: String,
    message: Option<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    stop_reason: Option<String>,
    #[serde(default)]
    content: serde_json::Value,
}

#[cfg(test)]
mod tests {
    #[test]
    fn no_claude_desktop_is_no_session() {
        let missing = std::env::temp_dir().join("prism-no-claude-desktop").join("claude-code-sessions");
        let (sessions, archived) = super::collect(&missing, &missing, &Default::default(), &mut Default::default()).unwrap();
        assert!(sessions.is_empty() && archived.is_empty());
    }
}
