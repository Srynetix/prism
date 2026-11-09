//! Feedback to the source (`crate::feedback`): the composer's repositories, its similar issues,
//! the issue it opens, and the requests followed (Settings › General › Feedback).

use super::State;

use crate::app::AppState;
use crate::feedback::{self, ContextLine, Issue, Request, Target};

use super::Cmd;

/// What the composer opens with.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FeedbackInfo {
    /// `[feedback] enabled`.
    pub enabled: bool,
    pub follow: bool,
    /// Where feedback can go, Prism first.
    pub targets: Vec<Target>,
    /// What Prism can attach of itself.
    pub context: Vec<ContextLine>,
    /// A capture of the window can be put on the clipboard (macOS, Linux).
    pub capture: bool,
}

fn demo_target() -> Target {
    let labels = crate::config::FeedbackConfig::default().labels;
    Target { id: "prism".into(), name: "Prism".into(), repo: "glab:acme/prism".into(), forge: "gitlab".into(), project: "acme/prism".into(), web: "https://gitlab.com/acme/prism".into(), why: "Prism and its blocks · [updates] source".into(), labels }
}

#[prism_macros::command]
pub async fn feedback_info(state: State<'_, AppState>) -> Cmd<FeedbackInfo> {
    let capture = cfg!(any(target_os = "macos", target_os = "linux"));
    if crate::demo::active() {
        return Ok(FeedbackInfo { enabled: true, follow: true, targets: vec![demo_target()], context: feedback::context(&[], &state.build, &crate::deps::sample()), capture });
    }
    let (cfg, packs) = feedback::load(&crate::config_path())?;
    let tools = crate::deps::list(&cfg, false).await;
    Ok(FeedbackInfo { enabled: cfg.feedback.enabled, follow: cfg.feedback.follow, targets: feedback::targets(&cfg, &packs), context: feedback::context(&packs, &state.build, &tools), capture })
}

/// The issues of `target` that look like `title`.
#[prism_macros::command]
pub async fn feedback_similar(target: String, title: String) -> Cmd<Vec<Issue>> {
    if crate::demo::active() {
        let issue = |number, title: &str, state: &str, votes| Issue { number, title: title.into(), url: format!("https://gitlab.com/acme/prism/-/issues/{number}"), state: state.into(), votes, not_planned: false };
        return Ok(if title.trim().len() < 4 { Vec::new() } else { vec![issue(198, "Show the commit behind each pod", "open", 4), issue(152, "Deploy history in the CI view", "closed", 2)] });
    }
    feedback::similar(&feedback::find(&crate::config_path(), &target)?, &title).await.map_err(Into::into)
}

/// A 👍 on issue `number` of `target`.
#[prism_macros::command]
pub async fn feedback_vote(target: String, number: u64) -> Cmd {
    feedback::vote(&feedback::find(&crate::config_path(), &target)?, number).await.map_err(Into::into)
}

/// The issue's text as it will be sent: what was written, and the lines attached.
#[prism_macros::command]
pub fn feedback_body(text: String, context: Vec<String>) -> String {
    feedback::body(&text, &context)
}

/// Opens the issue on `target`, and remembers it. `capture`: the window's, uploaded with it
/// (GitLab's uploads, GitHub's user attachments); a GitLab upload that fails opens nothing, rather
/// than an issue without the picture it was about; GitHub's, the issue without it, the capture to
/// paste from the clipboard (`paste_capture`).
#[prism_macros::command]
pub async fn feedback_create(state: State<'_, AppState>, target: String, kind: String, title: String, text: String, context: Vec<String>, capture: Option<bool>) -> Cmd<Request> {
    if !feedback::KINDS.contains(&kind.as_str()) {
        return Err(format!("unknown kind {kind}: {}", feedback::KINDS.join(", ")).into());
    }
    let t = feedback::find(&crate::config_path(), &target)?;
    let shot = feedback::capture_path(&state.store);
    let markdown = if capture == Some(true) { feedback::upload(&t, &shot).await? } else { None };
    let text = feedback::with_capture(&text, markdown.as_deref());
    let issue = feedback::create(&t, &kind, &title, &feedback::body(&text, &context)).await?;
    let request = Request { repo: t.repo.clone(), name: t.name.clone(), kind, opened: chrono::Utc::now().to_rfc3339(), issue, paste_capture: false };
    feedback::remember(&state.store, request.clone())?;
    // The agent's draft and the capture have served: they are not kept.
    feedback::clear_drafts(&state.store);
    let _ = std::fs::remove_file(shot);
    Ok(Request { paste_capture: capture == Some(true) && markdown.is_none(), ..request })
}

/// The forge's new issue page, filled in: without glab or gh.
#[prism_macros::command]
pub fn feedback_url(target: String, kind: String, title: String, text: String, context: Vec<String>) -> Cmd<String> {
    let t = if crate::demo::active() { demo_target() } else { feedback::find(&crate::config_path(), &target)? };
    Ok(feedback::new_issue_url(&t, &kind, &title, &feedback::body(&text, &context)))
}

/// The issues opened from Prism; `refresh`: their state read again on their forges.
#[prism_macros::command]
pub async fn feedback_requests(state: State<'_, AppState>, refresh: bool) -> Cmd<Vec<Request>> {
    if crate::demo::active() {
        let request = |number, title: &str, state: &str, votes| Request {
            repo: "glab:acme/prism".into(),
            name: "Prism".into(),
            kind: "feature".into(),
            opened: "2026-10-01T09:00:00Z".into(),
            issue: Issue { number, title: title.into(), url: format!("https://gitlab.com/acme/prism/-/issues/{number}"), state: state.into(), votes, not_planned: false },
            paste_capture: false,
        };
        return Ok(vec![request(214, "Split the Agents view", "closed", 6), request(198, "Show the commit behind each pod", "open", 4)]);
    }
    if refresh {
        return feedback::follow(&state.store).await.map_err(Into::into);
    }
    Ok(feedback::requests(&state.store))
}

/// An agent drafts the issue (Draft with an agent): it reads the code (its clone, or the forge with
/// glab/gh), looks for a duplicate, and writes the draft to a file Prism reads back
/// (`draft_state`). It never opens the issue: that stays the composer's, after a read. The agent
/// shows in the list, its conversation mine: its id, the draft's.
#[prism_macros::command]
pub async fn feedback_draft_start(state: State<'_, AppState>, target: String, kind: String, title: String, text: String) -> Cmd<String> {
    let t = feedback::find(&crate::config_path(), &target)?;
    // Its file named by its agent's id, in its prompt.
    let id = crate::agents::draft::new_id();
    let path = feedback::draft_path(&state.store, &id).ok_or("no draft id")?;
    let clone = state.hub.subscribe().borrow().repos.iter().find(|r| r.project.as_deref() == Some(t.project.as_str()) && !r.path.is_empty()).map(|r| std::path::PathBuf::from(&r.path));
    let (title, text) = (title.trim(), text.trim());
    let launch = crate::agents::Launch {
        title: format!("Feedback: {}", if title.is_empty() { t.name.as_str() } else { title }).chars().take(60).collect(),
        prompt: Some(feedback::draft_prompt(&t, &kind, title, text, &path, clone.is_some())),
        cwd: clone,
        worktree: None,
        permission_mode: None,
        category: None,
        tags: vec!["feedback".into()],
        provider: None,
        model: None,
        effort: None,
        then: Vec::new(),
        from: None,
        prism_mcp: false,
    };
    // Hours: it may ask me what I have in mind first. One at a time, the window's or a phone's: a
    // new one gives up the one before (its agent stays, its conversation mine; its own file).
    let spec = crate::agents::draft::Spec { id: Some(id), slot: Some("feedback"), launch, read: crate::agents::draft::Read::File(path), hidden: false, timeout: std::time::Duration::from_secs(4 * 3600) };
    state.agents.draft(spec).await.map_err(Into::into)
}

/// A feedback draft that Prism no longer knows (restarted meanwhile, or long over): its file,
/// when its agent wrote it; none otherwise.
#[prism_macros::command]
pub fn feedback_draft_read(state: State<'_, AppState>, id: String) -> Option<FeedbackDraft> {
    let path = feedback::draft_path(&state.store, &id)?;
    let (title, text) = crate::agents::draft::parse_titled(&std::fs::read_to_string(path).ok()?)?;
    Some(FeedbackDraft { title, text })
}

/// A feedback draft read from its file.
#[derive(serde::Serialize)]
pub struct FeedbackDraft {
    pub title: String,
    pub text: String,
}
