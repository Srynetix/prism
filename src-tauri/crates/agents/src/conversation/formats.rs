//! Each CLI's transcript lines (Claude Code's, Command Code's, Codex's rollout) read into the
//! fold, and what they hold in their own way: pasted images and text, `!` commands, diffs.

use super::*;

impl Fold {
    /// A line of Claude Code's transcript.
    pub(super) fn claude(&mut self, r: &Value) {
        if r.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let at = r.get("timestamp").and_then(Value::as_str);
        let flag = |k: &str| r.get(k).and_then(Value::as_bool) == Some(true);
        match r.get("type").and_then(Value::as_str) {
            Some("system") if r.get("subtype").and_then(Value::as_str) == Some("compact_boundary") => {
                self.push(at, Entry::Note { text: "Conversation compacted".into() });
            }
            // A slash command's output: a skill you typed launched as a subagent.
            Some("system") if r.get("subtype").and_then(Value::as_str) == Some("local_command") => {
                self.typed_fork(r.get("content").and_then(Value::as_str).unwrap_or_default(), at);
            }
            // A notification that came while it worked, delivered with its next request.
            // Plan mode: the file its plan goes in.
            Some("attachment") if r.pointer("/attachment/type").and_then(Value::as_str) == Some("plan_mode") => {
                if let Some(f) = r.pointer("/attachment/planFilePath").and_then(Value::as_str) {
                    self.plan_file = Some(f.to_string());
                }
            }
            Some("attachment") if r.pointer("/attachment/commandMode").and_then(Value::as_str) == Some("task-notification") => {
                self.notified(r.pointer("/attachment/prompt").and_then(Value::as_str).unwrap_or_default(), at);
            }
            // A prompt sent while it worked: written only this way, never as a `user` line. Yours, not
            // a reminder (isMeta) nor a message from another agent (origin peer).
            Some("attachment")
                if r.pointer("/attachment/commandMode").and_then(Value::as_str) == Some("prompt")
                    && r.pointer("/attachment/isMeta").and_then(Value::as_bool) != Some(true)
                    && r.pointer("/attachment/origin/kind").and_then(Value::as_str).is_none_or(|k| k == "human") =>
            {
                let (text, found) = reminders(r.pointer("/attachment/prompt").and_then(Value::as_str).unwrap_or_default());
                if let Some(prompt) = prompt_of(&text) {
                    self.begin(prompt, at);
                }
                self.reminded("harness", None, found);
            }
            // A hook's context (`additionalContext`), what the harness reminds it of: out of the
            // conversation shown, kept to be read.
            Some("attachment") => self.attached(r.get("attachment").unwrap_or(&Value::Null)),
            // Words the CLI sends as yours but are not (a skill's text, a caveat): its reminders kept.
            Some("user") if flag("isMeta") => {
                let Some(content) = r.pointer("/message/content") else { return };
                self.reminded("harness", None, reminders(&content_text(content)).1);
            }
            Some("user") if !flag("isCompactSummary") => {
                let Some(content) = r.pointer("/message/content") else { return };
                let mut words = Vec::new();
                // A plan approved: the words in the same message came with it, not a prompt.
                let mut approved = None;
                match content {
                    Value::String(s) => words.push(s.as_str()),
                    Value::Array(blocks) => {
                        for b in blocks {
                            match b.get("type").and_then(Value::as_str) {
                                Some("tool_result") => {
                                    let failed = b.get("is_error").and_then(Value::as_bool) == Some(true);
                                    let (output, found) = b.get("content").map(result_text).unwrap_or_default();
                                    self.tool_reminded(b.get("tool_use_id").and_then(Value::as_str), found);
                                    self.launched(b.get("tool_use_id").and_then(Value::as_str), r.get("toolUseResult"), failed);
                                    if let Some(answers) = r.pointer("/toolUseResult/answers") {
                                        self.answered(b.get("tool_use_id").and_then(Value::as_str), answers);
                                    }
                                    let id = b.get("tool_use_id").and_then(Value::as_str);
                                    if self.plan_answered(id, failed, &output, r.pointer("/toolUseResult/plan").and_then(Value::as_str)) && !failed {
                                        approved = id.and_then(|id| self.calls.get(id).copied());
                                    }
                                    self.forked(id, r.get("toolUseResult"));
                                    if !failed {
                                        self.resumed(id, r.get("toolUseResult"), at);
                                    }
                                    self.result(id, at, failed, b.get("content").map(|_| output));
                                }
                                Some("text") => words.extend(b.get("text").and_then(Value::as_str)),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                let joined = words.join("\n");
                // Its reminders apart: the prompt shown alone, they kept in its turn.
                let (text, found) = reminders(&joined);
                let text = text.as_ref();
                if let Some((t, e)) = approved {
                    let words = text.trim();
                    if let Some(Entry::Tool(Tool { proposal: Some(p), .. })) = self.turns.get_mut(t).and_then(|turn| turn.entries.get_mut(e)).filter(|_| !words.is_empty()) {
                        p.feedback = Some(words.to_string());
                    }
                } else if text.trim_start().starts_with("<task-notification>") {
                    self.notified(text, at);
                } else if let Some(note) = interruption(text) {
                    self.push(at, Entry::Note { text: note.into() });
                } else if let Some(command) = tagged(text, "bash-input") {
                    // A `!` command: a turn of its own, its output written with it (sent while it worked) or next.
                    self.begin(format!("! {command}"), at);
                    self.call(Some(SHELL), at, Tool { tool: "Bash".into(), kind: "command", what: Some(command.to_string()), state: "running", ..Tool::default() });
                    if let Some((output, failed)) = shell_output(text) {
                        self.result(Some(SHELL), at, failed, Some(output));
                    }
                } else if let Some((output, failed)) = shell_output(text) {
                    self.result(Some(SHELL), at, failed, Some(output));
                } else if let Some(prompt) = prompt_of(text) {
                    self.begin(prompt, at);
                    self.pasted(images(r).len());
                }
                self.reminded("harness", None, found);
            }
            Some("assistant") => {
                let Some(msg) = r.get("message") else { return };
                match msg.get("model").and_then(Value::as_str) {
                    Some("<synthetic>") => return,
                    Some(m) if !m.is_empty() => self.model = Some(m.to_string()),
                    _ => {}
                }
                self.usage(msg);
                for b in msg.get("content").and_then(Value::as_array).into_iter().flatten() {
                    match b.get("type").and_then(Value::as_str) {
                        Some("thinking") => self.thinking(at, b.get("thinking").and_then(Value::as_str).unwrap_or_default()),
                        Some("redacted_thinking") => self.thinking(at, ""),
                        Some("text") => {
                            let text = b.get("text").and_then(Value::as_str).unwrap_or_default().trim();
                            if !text.is_empty() {
                                self.push(at, Entry::Text { text: cut(text, TEXT_CHARS) });
                            }
                        }
                        Some("tool_use") => {
                            let name = b.get("name").and_then(Value::as_str).unwrap_or("tool");
                            let input = b.get("input").unwrap_or(&Value::Null);
                            let mut tool = Tool { tool: name.into(), kind: kind_of(name), what: self.names.what(name, input), state: "running", ..Tool::default() };
                            claude_diff(&mut tool, input);
                            Self::aside_call(&mut tool, input);
                            let id = b.get("id").and_then(Value::as_str);
                            if name == "AskUserQuestion" {
                                tool.ask = crate::question::parse(id.unwrap_or_default(), input);
                            }
                            self.plan_written(name, input);
                            if name == "ExitPlanMode" {
                                tool.proposal = Some(self.proposed(id.unwrap_or_default(), input));
                            }
                            if tool.kind == "agent" {
                                tool.call = id.map(String::from);
                                tool.prompt = input.get("prompt").and_then(Value::as_str).map(String::from);
                                tool.agent = input.get("subagent_type").and_then(Value::as_str).filter(|t| !t.is_empty() && *t != "general-purpose").map(String::from);
                            }
                            self.planned(name, input);
                            self.background_call(name, id, at, input, tool.what.clone());
                            self.call(id, at, tool);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// A line of Command Code's transcript: Claude Code's messages under other names (`type`
    /// `message` and its role, its tools in snake_case, its usage in camelCase with its own cost),
    /// folded as Claude Code's. Its questions' answers are in their results' text, a line each; a
    /// plan's verdict too ("Plan not approved", "Plan approved" and the plan), the comments of its
    /// review in a message of their own ("steering").
    pub(super) fn command_code(&mut self, r: &Value) {
        let Some(msg) = r.get("message") else { return };
        let role = msg.get("role").and_then(Value::as_str).unwrap_or_default();
        if msg.pointer("/meta/source").and_then(Value::as_str) == Some("steering") {
            let text = msg.get("content").map(content_text).unwrap_or_default();
            if text.starts_with("I reviewed the plan") {
                let comments: Vec<&str> = text.lines().filter_map(|l| l.trim().strip_prefix("Comment: ")).collect();
                self.plan_reviewed((!comments.is_empty()).then(|| comments.join("\n")));
                return;
            }
        }
        let model = r.get("model").and_then(Value::as_str).unwrap_or_default();
        let mut content = msg.get("content").cloned().unwrap_or(Value::Null);
        for b in content.as_array_mut().into_iter().flatten() {
            let claude = match b.get("name").and_then(Value::as_str) {
                Some("read_file") => "Read",
                Some("write_file") => "Write",
                Some("edit_file") => "Edit",
                Some("shell_command") => "Bash",
                Some("glob") => "Glob",
                Some("grep") => "Grep",
                Some("read_directory") => "LS",
                Some("todo_write") => "TodoWrite",
                Some("agent") => "Agent",
                Some("web_fetch") => "WebFetch",
                Some("web_search") => "WebSearch",
                Some("ask_user_question") => "AskUserQuestion",
                Some("exit_plan_mode") => "ExitPlanMode",
                _ => continue,
            };
            b["name"] = claude.into();
        }
        let at = r.get("timestamp").cloned().unwrap_or(Value::Null);
        let mut line = serde_json::json!({ "type": role, "timestamp": at, "message": { "model": model } });
        // Before its result, which forgets its call.
        for b in content.as_array_mut().into_iter().flatten().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
            let text = b.get("content").map(content_text).unwrap_or_default();
            self.answered_text(b.get("tool_use_id").and_then(Value::as_str), &text);
            // A plan refused, as Claude Code's is: its result an error.
            if text.starts_with("Plan not approved") {
                b["is_error"] = true.into();
            } else if text.starts_with("Plan approved")
                && let Some((_, plan)) = text.split_once("\n## Approved Plan").and_then(|(_, rest)| rest.split_once(":\n"))
            {
                line["toolUseResult"] = serde_json::json!({ "plan": plan });
            }
        }
        line["message"]["content"] = content;
        self.claude(&line);
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Usage {
            #[serde(default)]
            input_tokens: u64,
            #[serde(default)]
            output_tokens: u64,
            #[serde(default)]
            cache_read_tokens: u64,
            #[serde(default)]
            cache_write_tokens: u64,
            cost_usd: Option<f64>,
        }
        let Some(u) = r.get("usage").and_then(|u| Usage::deserialize(u).ok()) else { return };
        if r.get("id").and_then(Value::as_str).is_some_and(|id| self.replies.insert(id.to_string())) {
            self.spent(model, u.input_tokens + u.output_tokens + u.cache_read_tokens + u.cache_write_tokens, u.cost_usd);
        }
    }

    /// A line of Codex's rollout.
    pub(super) fn codex(&mut self, r: &Value) {
        let at = r.get("timestamp").and_then(Value::as_str);
        let p = &r["payload"];
        match (r.get("type").and_then(Value::as_str), p.get("type").and_then(Value::as_str)) {
            (Some("compacted"), _) => self.push(at, Entry::Note { text: "Conversation compacted".into() }),
            (Some("event_msg"), Some("turn_aborted")) => self.push(at, Entry::Note { text: "Interrupted".into() }),
            (Some("turn_context"), _) => {
                if let Some(m) = p.get("model").and_then(Value::as_str).filter(|m| !m.is_empty()) {
                    self.model = Some(m.to_string());
                }
            }
            // Its input counts the cached part.
            (Some("event_msg"), Some("token_count")) => {
                let Some(info) = p.get("info").and_then(|i| CodexInfo::deserialize(i).ok()) else { return };
                if info.total_token_usage.total_tokens <= self.codex_total {
                    return;
                }
                self.codex_total = info.total_token_usage.total_tokens;
                let u = info.last_token_usage;
                let model = self.model.clone().unwrap_or_default();
                self.spent(&model, u.input_tokens + u.output_tokens, u.cost(&model));
            }
            // Its reasoning: its summary, when it writes one.
            (Some("response_item"), Some("reasoning")) => {
                let summary = p.get("summary").and_then(Value::as_array).into_iter().flatten();
                let text = summary.filter_map(|s| s.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n\n");
                self.thinking(at, &text);
            }
            (Some("response_item"), Some("message")) => {
                let text: Vec<&str> = p.get("content").and_then(Value::as_array).into_iter().flatten().filter_map(|b| b.get("text").and_then(Value::as_str)).collect();
                match p.get("role").and_then(Value::as_str) {
                    Some("assistant") => {
                        let text = text.join("\n");
                        if !text.trim().is_empty() {
                            self.push(at, Entry::Text { text: cut(text.trim(), TEXT_CHARS) });
                        }
                    }
                    // Its context (`<environment_context>`, `<user_instructions>`…), not a prompt of yours.
                    Some("user") if !text.iter().all(|t| t.trim_start().starts_with('<')) => {
                        let prompt = text.into_iter().filter(|t| !t.trim_start().starts_with('<')).collect::<Vec<_>>().join("\n");
                        self.begin(prompt.trim().to_string(), at);
                        self.pasted(images(r).len());
                    }
                    _ => {}
                }
            }
            (Some("response_item"), Some(kind @ ("function_call" | "custom_tool_call" | "local_shell_call"))) => {
                let name = p.get("name").and_then(Value::as_str).unwrap_or(if kind == "local_shell_call" { "local_shell" } else { "shell" });
                let input = p.get("input").or(p.get("arguments")).or(p.get("action")).unwrap_or(&Value::Null);
                let mut tool = Tool { tool: name.into(), kind: kind_of(name), state: "running", ..Tool::default() };
                self.planned(name, input);
                if name == "apply_patch" {
                    codex_patch(&mut tool, input.as_str().unwrap_or_default());
                } else {
                    tool.what = codex_what(input).or_else(|| input.get("command").map(|c| c.to_string()));
                }
                Self::aside_call(&mut tool, input);
                self.call(p.get("call_id").and_then(Value::as_str), at, tool);
            }
            (Some("response_item"), Some("function_call_output" | "custom_tool_call_output")) => {
                let (failed, output) = codex_output(p.get("output").unwrap_or(&Value::Null));
                self.result(p.get("call_id").and_then(Value::as_str), at, failed, output);
            }
            _ => {}
        }
    }
}

/// The images of a prompt's line, as `data:` URLs, in order: Claude Code's `image` blocks (base64),
/// Codex's `input_image` ones.
pub(super) fn images(r: &Value) -> Vec<String> {
    let blocks = r.pointer("/message/content").or(r.pointer("/payload/content")).and_then(Value::as_array).into_iter().flatten();
    blocks
        .filter_map(|b| match b.get("type").and_then(Value::as_str) {
            Some("image") if b.pointer("/source/type").and_then(Value::as_str) == Some("base64") => {
                let kind = b.pointer("/source/media_type").and_then(Value::as_str).filter(|k| k.starts_with("image/"))?;
                Some(format!("data:{kind};base64,{}", b.pointer("/source/data").and_then(Value::as_str)?))
            }
            Some("input_image") => b.get("image_url").and_then(Value::as_str).filter(|u| u.starts_with("data:image/")).map(String::from),
            _ => None,
        })
        .collect()
}

/// Image `n` pasted in the prompt on the transcript's line starting at byte `line` (`Pasted`).
pub fn pasted_image(transcript: &Path, line: u64, n: usize) -> Result<String> {
    use std::io::BufRead;
    let mut f = std::fs::File::open(transcript)?;
    f.seek(SeekFrom::Start(line))?;
    let mut buf = Vec::new();
    std::io::BufReader::new(f).read_until(b'\n', &mut buf)?;
    let r: Value = serde_json::from_slice(buf.strip_suffix(b"\n").unwrap_or(&buf)).context("not a prompt's line (its transcript was written again?)")?;
    images(&r).into_iter().nth(n).context("no such image in its prompt")
}

/// Your words, unless the CLI wrote them: a slash command reads as itself, its output and the
/// caveats it adds are left out. Text pasted in (Prism sends several lines as a paste) reads as
/// itself, without the envelope Claude Code wraps it in.
pub(super) fn prompt_of(text: &str) -> Option<String> {
    let unwrapped = unpasted(text);
    let t = unwrapped.trim();
    if t.is_empty() {
        return None;
    }
    if t.starts_with('<') {
        let command = tagged(t, "command-name")?;
        return Some(match tagged(t, "command-args").filter(|a| !a.is_empty()) {
            Some(args) => format!("{command} {args}"),
            None => command.to_string(),
        });
    }
    (!t.starts_with("Caveat:")).then(|| t.to_string())
}

/// What `<name>…</name>` holds in `text`, trimmed.
pub(super) fn tagged<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.split_once(&format!("<{name}>")).and_then(|(_, rest)| rest.split_once(&format!("</{name}>"))).map(|(v, _)| v.trim())
}

/// The call a `!` command stands for until its output comes.
pub(super) const SHELL: &str = "!";

/// A `!` command's output, as Claude Code writes it (`<bash-stdout>`, `<bash-stderr>`): both, and
/// whether it failed (only errors).
pub(super) fn shell_output(text: &str) -> Option<(String, bool)> {
    let (out, err) = (tagged(text, "bash-stdout"), tagged(text, "bash-stderr"));
    if out.is_none() && err.is_none() {
        return None;
    }
    let (out, err) = (out.unwrap_or_default(), err.unwrap_or_default());
    let both = [out, err].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
    Some((both, out.is_empty() && !err.is_empty()))
}

/// `<pasted_content id="x">…</pasted_content id="x">` (Claude Code's envelope around a paste),
/// each one replaced by what it holds.
pub(super) fn unpasted(text: &str) -> std::borrow::Cow<'_, str> {
    const OPEN: &str = "<pasted_content";
    const CLOSE: &str = "</pasted_content";
    if !text.contains(OPEN) {
        return text.into();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(OPEN) {
        let Some(body) = rest[i..].find('>').map(|j| i + j + 1) else { break };
        let Some(end) = rest[body..].find(CLOSE).map(|j| body + j) else { break };
        let after = rest[end..].find('>').map_or(rest.len(), |j| end + j + 1);
        out.push_str(&rest[..i]);
        out.push_str(rest[body..end].trim_matches('\n'));
        rest = &rest[after..];
    }
    out.push_str(rest);
    out.into()
}

/// The `<system-reminder>…</system-reminder>` blocks Claude Code slips into what it sends (its
/// harness's reminders, hooks' context, notes after a tool's output) taken out of `text`: what is
/// left, and what each held (trimmed, those empty left out). Only a block whose opening tag starts
/// a line and that is closed counts: one quoted mid-sentence, in a file's lines (`  148→
/// "<system-reminder>…"`) or a lone tag (a `grep` for it) stays as written.
pub(crate) fn reminders(text: &str) -> (std::borrow::Cow<'_, str>, Vec<String>) {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    if !text.contains(OPEN) {
        return (text.into(), Vec::new());
    }
    let at_line_start = |i: usize| text[..i].rsplit('\n').next().is_some_and(|before| before.trim().is_empty());
    // The next block from `from`: where it starts, where its body is, where it ends.
    let next = |from: usize| {
        text[from..].match_indices(OPEN).map(|(j, _)| from + j).filter(|&i| at_line_start(i)).find_map(|i| {
            let body = i + OPEN.len();
            // Closed before another one opens: a lone tag before a block is not its start.
            text[body..].find(CLOSE).map(|j| body + j).filter(|&close| !text[body..close].contains(OPEN)).map(|close| (i, body, close))
        })
    };
    let Some(mut block) = next(0) else { return (text.into(), Vec::new()) };
    let (mut out, mut found) = (String::with_capacity(text.len()), Vec::new());
    let mut from = 0;
    loop {
        let (i, body, close) = block;
        // What came before it, the spaces leading its line left out.
        out.push_str(text[from..i].trim_end_matches([' ', '\t']));
        let inner = text[body..close].trim();
        if !inner.is_empty() {
            found.push(inner.to_string());
        }
        // The line break that ended its line goes with it.
        let after = close + CLOSE.len();
        from = if text[after..].starts_with("\r\n") { after + 2 } else if text[after..].starts_with('\n') { after + 1 } else { after };
        match next(from) {
            Some(b) => block = b,
            None => break,
        }
    }
    out.push_str(&text[from..]);
    (out.into(), found)
}

/// A tool result's text without the reminders Claude Code adds to it (a block of their own, or
/// closing its text), and theirs.
pub(super) fn result_text(v: &Value) -> (String, Vec<String>) {
    let text = content_text(v);
    let (clean, found) = reminders(&text);
    if found.is_empty() {
        return (text, found);
    }
    (clean.trim_end().to_string(), found)
}

pub(super) fn interruption(text: &str) -> Option<&'static str> {
    text.trim().starts_with("[Request interrupted by user").then_some("Interrupted")
}

/// A tool result's text: text, or blocks of text (images left out).
pub(super) fn content_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// A Codex output: JSON with its `output` and exit code, or plain text (`Exit code: n` heading it).
pub(super) fn codex_output(v: &Value) -> (bool, Option<String>) {
    let s = match v {
        Value::String(s) => s.clone(),
        other => content_text(other),
    };
    if let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&s) {
        let code = o.get("metadata").and_then(|m| m.get("exit_code")).and_then(Value::as_i64).unwrap_or(0);
        return (code != 0, o.get("output").and_then(Value::as_str).map(String::from));
    }
    let code = s.lines().find_map(|l| l.strip_prefix("Exit code: ").or(l.strip_prefix("Process exited with code "))).and_then(|c| c.trim().parse::<i64>().ok()).unwrap_or(0);
    let body = s.split_once("\nOutput:\n").map_or(s.as_str(), |(_, o)| o).to_string();
    (code != 0 || s.starts_with("Script failed"), Some(body))
}

/// An edit's diff from Claude Code's call: what it replaces and with what, or a file written whole.
pub(super) fn claude_diff(tool: &mut Tool, input: &Value) {
    let field = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    let pairs: Vec<(String, String)> = match tool.tool.as_str() {
        "Edit" => vec![(field(input, "old_string").unwrap_or_default(), field(input, "new_string").unwrap_or_default())],
        "MultiEdit" => input.get("edits").and_then(Value::as_array).into_iter().flatten().map(|e| (field(e, "old_string").unwrap_or_default(), field(e, "new_string").unwrap_or_default())).collect(),
        "Write" => vec![(String::new(), field(input, "content").unwrap_or_default())],
        _ => return,
    };
    for (old, new) in pairs {
        let (old, new): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
        // The lines both keep at the start and the end are context, not change.
        let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let tail = old[head..].iter().rev().zip(new[head..].iter().rev()).take_while(|(a, b)| a == b).count();
        if head > 0 {
            tool.diff.push(format!(" {}", old[head - 1]));
        }
        for l in &old[head..old.len() - tail] {
            tool.removed += 1;
            tool.diff.push(format!("-{l}"));
        }
        for l in &new[head..new.len() - tail] {
            tool.added += 1;
            tool.diff.push(format!("+{l}"));
        }
    }
    tool.diff.truncate(DIFF_LINES);
}

/// Codex's patch: the files it touches (`*** Update File: …`) and its lines.
pub(super) fn codex_patch(tool: &mut Tool, patch: &str) {
    let mut files = Vec::new();
    for l in patch.lines() {
        if let Some(f) = ["*** Update File: ", "*** Add File: ", "*** Delete File: "].iter().find_map(|p| l.strip_prefix(p)) {
            files.push(f.trim().to_string());
        } else if let Some(rest) = l.strip_prefix('+') {
            tool.added += 1;
            tool.diff.push(format!("+{rest}"));
        } else if let Some(rest) = l.strip_prefix('-') {
            tool.removed += 1;
            tool.diff.push(format!("-{rest}"));
        } else if let Some(rest) = l.strip_prefix(' ') {
            tool.diff.push(format!(" {rest}"));
        }
    }
    tool.diff.truncate(DIFF_LINES);
    tool.what = match files.len() {
        0 => None,
        1 => files.pop(),
        n => Some(format!("{} (+{} files)", files[0], n - 1)),
    };
}

/// The last lines of an output, trimmed.
pub(super) fn tail(s: &str) -> String {
    let lines: Vec<&str> = s.trim_end().lines().collect();
    let kept = lines[lines.len().saturating_sub(OUTPUT_LINES)..].join("\n");
    let n = kept.chars().count();
    if n > OUTPUT_CHARS { kept.chars().skip(n - OUTPUT_CHARS).collect() } else { kept }
}

/// Milliseconds from one instant to another (ISO 8601).
pub(super) fn millis(from: &str, to: &str) -> Option<u64> {
    let (from, to) = (chrono::DateTime::parse_from_rfc3339(from).ok()?, chrono::DateTime::parse_from_rfc3339(to).ok()?);
    u64::try_from((to - from).num_milliseconds()).ok()
}

/// The entities Claude Code escapes in a notification's tags.
pub(super) fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

pub(super) fn cut(s: &str, chars: usize) -> String {
    if s.chars().count() > chars { format!("{}…", s.chars().take(chars).collect::<String>()) } else { s.to_string() }
}
