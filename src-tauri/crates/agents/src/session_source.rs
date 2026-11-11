//! A past session as the source of a quick action (Create a quick action…): what was asked in
//! it (its first prompt, then the details given after), where it ran and how it was launched.
//! Read from an agent of the list (archived too), the last one closed in a repository (`left`),
//! or a Claude Desktop session (its transcript, kept for Prism, never sent).

use super::*;

/// What a drafting agent is launched `from`: the core's.
pub use prism_core::agents::DRAFTER;

/// The prompts read at most, each cut to this many characters.
const MAX_PROMPTS: usize = 40;
const PROMPT_CHARS: usize = 4_000;

/// Which session.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// An agent of the list.
    Agent(String),
    /// The last agent closed in this repository.
    Left(String),
    /// A Claude Desktop session.
    Desktop(String),
}

/// What a quick action is made from.
#[derive(Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SessionSource {
    pub provider: Provider,
    pub title: String,
    /// The folder it ran in: its worktree, or the folder it was launched in.
    pub dir: String,
    /// It ran in a worktree made for it, not in a clone as it is.
    pub worktree: bool,
    /// What it was launched with (absent: launched before Prism kept it, or a Desktop session).
    pub launch: Option<state::LaunchSpec>,
    /// Your prompts, oldest first: the first one, then those given after. Empty when its
    /// conversation cannot be read (only `launch`'s prompt is known then).
    pub prompts: Vec<String>,
    /// Its conversation could be read (not OpenCode's, nor a closed Codex one).
    pub readable: bool,
    /// When it last ran (ISO 8601).
    pub at: Option<String>,
}

/// The prompts of a transcript, oldest first, the empty ones (a resumed turn) left out.
fn prompts_of(transcript: &std::path::Path, dir: &std::path::Path) -> Vec<String> {
    let c = conversation::read(transcript, Some(dir), MAX_PROMPTS);
    c.turns.iter().map(|t| t.prompt.trim()).filter(|p| !p.is_empty()).map(cut).collect()
}

fn cut(text: &str) -> String {
    if text.chars().count() <= PROMPT_CHARS {
        return text.to_string();
    }
    format!("{}…", text.chars().take(PROMPT_CHARS).collect::<String>())
}

/// Its prompts from its transcript, read off the async runtime.
async fn read_prompts(transcript: Option<PathBuf>, dir: PathBuf) -> Option<Vec<String>> {
    let t = transcript.filter(|t| t.is_file())?;
    tokio::task::spawn_blocking(move || prompts_of(&t, &dir)).await.ok()
}

impl Agents {
    /// The session `source` names, as a quick action is made from it.
    pub async fn session_source(&self, source: Source) -> Result<SessionSource> {
        match source {
            Source::Agent(id) => {
                let record = self.record(&id)?;
                let dir = self.dir(&id)?;
                let prompts = read_prompts(self.transcript(&id)?, dir.clone()).await;
                let at = self.inner.hub.subscribe().borrow().agents.iter().find(|a| a.id == id).and_then(|a| a.last_activity.map(|t| t.to_rfc3339()));
                Ok(from_record(&record, dir, prompts, at.or_else(|| Some(record.created_at.to_rfc3339()))))
            }
            Source::Left(repo) => {
                let path = self.left_path();
                let list = tokio::task::spawn_blocking(move || left::load(&path)).await?;
                let left = list.into_iter().find(|l| l.repo == repo).context(tr("nothing was left in this repository"))?;
                let record = left.record;
                let dir = record.worktree.clone().filter(|w| w.is_dir()).unwrap_or_else(|| record.cwd.clone());
                // Its CLI's session is gone with it: only Claude Code's transcript is found by its id.
                let transcript = if record.provider.finds_transcript() { usage::find_transcript(self.cli_home(record.provider, record.launch.as_ref().and_then(|l| l.account.as_deref())).as_deref(), &record.id) } else { None };
                let prompts = read_prompts(transcript, dir.clone()).await;
                Ok(from_record(&record, dir, prompts, Some(left.closed_at.to_rfc3339())))
            }
            Source::Desktop(id) => {
                let session = self.inner.hub.subscribe().borrow().sessions.iter().find(|s| s.id == id).cloned().context(tr("unknown session"))?;
                let dir = PathBuf::from(session.worktree.clone().unwrap_or_else(|| session.folder.clone()));
                let prompts = read_prompts(session.transcript.as_ref().map(PathBuf::from), dir.clone()).await;
                Ok(SessionSource {
                    provider: Provider::Claude,
                    title: session.title.clone(),
                    dir: dir.display().to_string(),
                    worktree: session.worktree.is_some(),
                    launch: None,
                    readable: prompts.is_some(),
                    prompts: prompts.unwrap_or_default(),
                    at: session.last_activity.map(|t| t.to_rfc3339()),
                })
            }
        }
    }
}

fn from_record(record: &Record, dir: PathBuf, prompts: Option<Vec<String>>, at: Option<String>) -> SessionSource {
    let mut list = prompts.clone().unwrap_or_default();
    // Its first prompt as launched, when its transcript starts later (resumed) or is not read.
    // Compared as read: the transcript's are cut too.
    if let Some(first) = record.prompt.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(cut)
        && !list.contains(&first)
    {
        list.insert(0, first);
    }
    SessionSource {
        provider: record.provider,
        title: record.title.clone(),
        dir: dir.display().to_string(),
        worktree: record.worktree.is_some(),
        launch: record.launch.clone(),
        prompts: list,
        readable: prompts.is_some(),
        at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_prompts_are_cut() {
        assert_eq!(cut("short"), "short");
        let long = "é".repeat(PROMPT_CHARS + 10);
        let c = cut(&long);
        assert!(c.ends_with('…') && c.chars().count() == PROMPT_CHARS + 1);
    }
}
