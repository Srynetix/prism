//! A transcript folded into turns, whatever CLI wrote it: the turn under way, its calls and their
//! results, the plan, usage, background tasks. Each CLI's lines are read in `formats`.

use super::*;

/// A fold's turns: read as a slice, changed only through `push`, `last_mut` and `get_mut`, each
/// of which forgets the signature of the turn it hands out (`Turn::sig`, kept between reads so
/// that a Chat holding all of a long session's turns signs only those that moved).
mod turns {
    use super::Turn;

    #[derive(Default)]
    pub(in crate::conversation) struct Turns(Vec<Turn>);

    impl std::ops::Deref for Turns {
        type Target = [Turn];
        fn deref(&self) -> &[Turn] {
            &self.0
        }
    }

    impl Turns {
        pub(in crate::conversation) fn push(&mut self, t: Turn) {
            self.0.push(t);
        }

        pub(in crate::conversation) fn last_mut(&mut self) -> Option<&mut Turn> {
            self.0.last_mut().map(changed)
        }

        pub(in crate::conversation) fn get_mut(&mut self, i: usize) -> Option<&mut Turn> {
            self.0.get_mut(i).map(changed)
        }

        /// Signs those of `range` not signed since they last changed, but those `fresh` says are
        /// filled from elsewhere at each read (`subagents`, `waiting_plan`).
        pub(in crate::conversation) fn sign(&mut self, range: std::ops::Range<usize>, fresh: impl Fn(&Turn) -> bool) {
            for t in &mut self.0[range] {
                if t.sig.is_empty() && !fresh(t) {
                    t.sig = super::sign(t);
                }
            }
        }
    }

    fn changed(t: &mut Turn) -> &mut Turn {
        t.sig.clear();
        t
    }
}
pub(super) use turns::Turns;

pub(super) struct Fold {
    pub(super) names: Names,
    pub(super) turns: Turns,
    /// Tool calls waiting for their result: their turn and entry.
    pub(super) calls: HashMap<String, (usize, usize)>,
    /// Replies counted (a reply's blocks are written apart, its usage with each).
    pub(super) replies: HashSet<String>,
    pub(super) plan: Vec<PlanStep>,
    /// Background tasks launched, waiting for their id: by their call's.
    pub(super) launching: HashMap<String, Background>,
    pub(super) background: Vec<Background>,
    /// Notifications read: one may be written twice (queued, then delivered).
    pub(super) notifications: HashSet<String>,
    /// The model last seen (Codex: its `turn_context`).
    pub(super) model: Option<String>,
    /// Codex's running total: a `token_count` sent again unchanged counts once.
    pub(super) codex_total: u64,
    /// Plan files (Claude Code's plan mode) as its `Write` and `Edit` calls left them, by path;
    /// the one in use; and the plans proposed so far.
    pub(super) plan_files: HashMap<String, String>,
    pub(super) plan_file: Option<String>,
    pub(super) proposals: u32,
    /// Where the line being folded starts in the transcript.
    pub(super) line: u64,
}

impl Fold {
    pub(super) fn new(names: Names) -> Self {
        Self { names, turns: Turns::default(), calls: HashMap::new(), replies: HashSet::new(), plan: Vec::new(), launching: HashMap::new(), background: Vec::new(), notifications: HashSet::new(), model: None, codex_total: 0, plan_files: HashMap::new(), plan_file: None, proposals: 0, line: 0 }
    }

    /// The turn under way; one without a prompt when the transcript starts with the agent's words.
    pub(super) fn current(&mut self, at: Option<&str>) -> &mut Turn {
        if self.turns.is_empty() {
            self.begin(String::new(), at);
        }
        let t = self.turns.last_mut().unwrap();
        if at.is_some() {
            t.last = at.map(String::from);
        }
        t
    }

    pub(super) fn begin(&mut self, prompt: String, at: Option<&str>) {
        let n = self.turns.len() as u32 + 1;
        self.turns.push(Turn { n, prompt, at: at.map(String::from), last: at.map(String::from), ..Turn::default() });
    }

    /// The prompt just begun came with `count` images, on the line being folded.
    pub(super) fn pasted(&mut self, count: usize) {
        if let (Some(t), true) = (self.turns.last_mut(), count > 0) {
            t.pasted = Some(Pasted { line: self.line, count: count as u32 });
        }
    }

    pub(super) fn push(&mut self, at: Option<&str>, entry: Entry) {
        self.current(at).entries.push(entry);
    }

    pub(super) fn call(&mut self, id: Option<&str>, at: Option<&str>, mut tool: Tool) {
        tool.at = at.map(String::from);
        self.push(at, Entry::Tool(tool));
        if let Some(id) = id {
            let t = self.turns.len() - 1;
            self.calls.insert(id.to_string(), (t, self.turns[t].entries.len() - 1));
        }
    }

    /// A question's answers, as Claude Code recorded them.
    pub(super) fn answered(&mut self, id: Option<&str>, answers: &Value) {
        let Some(&(t, e)) = id.and_then(|id| self.calls.get(id)) else { return };
        if let Some(Entry::Tool(Tool { ask: Some(ask), .. })) = self.turns.get_mut(t).and_then(|turn| turn.entries.get_mut(e)) {
            ask.answered(answers);
        }
    }

    /// A plan file written or edited (plan mode): kept as it now reads.
    pub(super) fn plan_written(&mut self, name: &str, input: &Value) {
        let Some(path) = input.get("file_path").and_then(Value::as_str).filter(|p| crate::plan::is_plan_file(p) || self.plan_file.as_deref() == Some(*p)) else { return };
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let edits: Vec<&Value> = match name {
            "Write" => {
                self.plan_files.insert(path.to_string(), s(input, "content"));
                vec![]
            }
            "Edit" => vec![input],
            "MultiEdit" => input.get("edits").and_then(Value::as_array).into_iter().flatten().collect(),
            _ => return,
        };
        if let Some(text) = self.plan_files.get_mut(path) {
            for e in edits {
                let (old, new) = (s(e, "old_string"), s(e, "new_string"));
                if old.is_empty() {
                    continue;
                }
                *text = if e.get("replace_all").and_then(Value::as_bool) == Some(true) { text.replace(&old, &new) } else { text.replacen(&old, &new, 1) };
            }
        }
        self.plan_file = Some(path.to_string());
    }

    /// A plan proposed: its file as last written, failing that the call's own `plan` (which may
    /// lag behind its file).
    pub(super) fn proposed(&mut self, call: &str, input: &Value) -> crate::plan::Proposal {
        self.proposals += 1;
        let file = self.plan_file.clone();
        let text = file.as_ref().and_then(|f| self.plan_files.get(f)).cloned().or_else(|| input.get("plan").and_then(Value::as_str).map(String::from)).unwrap_or_default();
        crate::plan::Proposal { call: call.to_string(), text: cut(text.trim(), TEXT_CHARS), version: self.proposals, verdict: "waiting", feedback: None, file }
    }

    pub(super) fn proposal(&mut self, id: &str) -> Option<&mut crate::plan::Proposal> {
        let &(t, e) = self.calls.get(id)?;
        match self.turns.get_mut(t).and_then(|turn| turn.entries.get_mut(e)) {
            Some(Entry::Tool(Tool { proposal: Some(p), .. })) => Some(p),
            _ => None,
        }
    }

    /// A plan's answer, from its call's result: approved (the plan as approved), or refused with
    /// your words or without. True when the call was a plan's.
    pub(super) fn plan_answered(&mut self, id: Option<&str>, failed: bool, result: &str, approved: Option<&str>) -> bool {
        let Some(p) = id.and_then(|id| self.proposal(id)) else { return false };
        if failed {
            p.feedback = crate::plan::feedback_of(result);
            p.verdict = if p.feedback.is_some() { "changes" } else { "refused" };
        } else {
            p.verdict = "approved";
            if let Some(text) = approved.map(str::trim).filter(|t| !t.is_empty()) {
                p.text = cut(text, TEXT_CHARS);
            }
        }
        true
    }

    /// A call's outcome: its state, and a command's output (or a failure's message).
    pub(super) fn result(&mut self, id: Option<&str>, at: Option<&str>, failed: bool, output: Option<String>) {
        let Some((t, e)) = id.and_then(|id| self.calls.remove(id)) else { return };
        if let Some(at) = at
            && let Some(turn) = self.turns.get_mut(t)
        {
            turn.last = Some(at.to_string());
        }
        if let Some(Entry::Tool(tool)) = self.turns.get_mut(t).and_then(|turn| turn.entries.get_mut(e)) {
            tool.state = if failed { "failed" } else { "ok" };
            tool.ms = tool.at.as_deref().zip(at).and_then(|(from, to)| millis(from, to));
            if failed || tool.kind == "command" {
                tool.output = output.map(|o| tail(&o)).filter(|o| !o.is_empty());
            }
        }
    }

    /// Its thinking, timed from the step before.
    pub(super) fn thinking(&mut self, at: Option<&str>, text: &str) {
        let since = self.turns.last().and_then(|t| t.last.clone());
        let ms = since.as_deref().zip(at).and_then(|(from, to)| millis(from, to));
        self.push(at, Entry::Thinking { text: cut(text.trim(), TEXT_CHARS), ms });
    }

    /// A reply's tokens and cost, counted once in its turn.
    pub(super) fn usage(&mut self, msg: &Value) {
        let (Some(id), Some(u)) = (msg.get("id").and_then(Value::as_str), msg.get("usage")) else { return };
        if self.turns.is_empty() || !self.replies.insert(id.to_string()) {
            return;
        }
        let Ok(u) = ClaudeUsage::deserialize(u) else { return };
        let model = msg.get("model").and_then(Value::as_str).unwrap_or_default();
        self.spent(model, u.total(), u.cost(model));
    }

    /// A reply's tokens and cost into the turn under way, answered by `model`.
    pub(super) fn spent(&mut self, model: &str, tokens: u64, cost: Option<f64>) {
        let Some(t) = self.turns.last_mut() else { return };
        t.tokens += tokens;
        if let Some(c) = cost {
            t.cost = Some(t.cost.unwrap_or(0.0) + c);
        }
        if !model.is_empty() {
            t.model = Some(model.to_string());
        }
    }

    /// Its plan, from the call that writes it.
    pub(super) fn planned(&mut self, name: &str, input: &Value) {
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        match name {
            "TodoWrite" => {
                let todos = input.get("todos").and_then(Value::as_array).into_iter().flatten();
                self.plan = todos.enumerate().map(|(i, t)| PlanStep { text: s(t, "content"), status: s(t, "status"), id: (i + 1).to_string() }).collect();
            }
            "TaskCreate" => {
                let id = (self.plan.len() + 1).to_string();
                self.plan.push(PlanStep { text: s(input, "subject"), status: "pending".into(), id });
            }
            "TaskUpdate" => {
                let id = input.get("taskId").map(|v| v.as_str().map_or_else(|| v.to_string(), String::from)).unwrap_or_default();
                let status = s(input, "status");
                if status == "deleted" {
                    self.plan.retain(|p| p.id != id);
                } else if let Some(p) = self.plan.iter_mut().find(|p| p.id == id) {
                    if !status.is_empty() {
                        p.status = status;
                    }
                    if let Some(text) = input.get("subject").and_then(Value::as_str) {
                        p.text = text.into();
                    }
                }
            }
            // Codex: its arguments as JSON text.
            "update_plan" => {
                let args = input.as_str().and_then(|a| serde_json::from_str::<Value>(a).ok()).unwrap_or_else(|| input.clone());
                let steps = args.get("plan").and_then(Value::as_array).into_iter().flatten();
                self.plan = steps.enumerate().map(|(i, p)| PlanStep { text: s(p, "step"), status: s(p, "status"), id: (i + 1).to_string() }).collect();
            }
            _ => {}
        }
    }

    /// A call that starts a background task (or stops one): kept until its result gives its id.
    pub(super) fn background_call(&mut self, name: &str, id: Option<&str>, at: Option<&str>, input: &Value, what: Option<String>) {
        let background = input.get("run_in_background").and_then(Value::as_bool) == Some(true);
        let kind = match name {
            "Bash" if background => "command",
            // Claude Code launches them in the background unasked: its result says so (`isAsync`);
            // a skill, when it runs as a subagent of its own (forked, `background`).
            "Agent" | "Task" | "Skill" => "agent",
            "Monitor" => "monitor",
            "TaskStop" | "KillShell" => {
                let task = input.get("task_id").or(input.get("shell_id")).and_then(Value::as_str);
                if let Some(b) = self.background.iter_mut().find(|b| Some(b.id.as_str()) == task && b.state == "running") {
                    b.state = "stopped";
                    b.ended = at.map(String::from);
                }
                return;
            }
            _ => return,
        };
        let Some(id) = id else { return };
        let description = input.get("description").and_then(Value::as_str).map(str::trim).filter(|d| !d.is_empty());
        let what = description.map(|d| cut(d, 200)).or(what);
        self.launching.insert(id.to_string(), Background { kind, what, state: "running", at: at.map(String::from), call: id.to_string(), ..Background::default() });
    }

    /// A background task's launch, its id in its call's result (`backgroundTaskId`, a monitor's
    /// `taskId`, an async subagent's or a skill's forked in the background `agentId`); a failed
    /// launch is no task.
    pub(super) fn launched(&mut self, call: Option<&str>, result: Option<&Value>, failed: bool) {
        let Some(mut b) = call.and_then(|c| self.launching.remove(c)) else { return };
        let field = |k: &str| result.and_then(|r| r.get(k)).and_then(Value::as_str).map(String::from);
        let yes = |k: &str| result.and_then(|r| r.get(k)).and_then(Value::as_bool) == Some(true);
        let id = field("backgroundTaskId").or_else(|| field("taskId")).or_else(|| field("agentId").filter(|_| yes("isAsync") || yes("background")));
        if let (Some(id), false) = (id, failed) {
            b.id = id;
            self.background.push(b);
        }
    }

    /// A skill run as a subagent (`status` forked, in the background or not): its call is one, its
    /// calls read from its agent's transcript.
    pub(super) fn forked(&mut self, call: Option<&str>, result: Option<&Value>) {
        let Some(result) = result.filter(|r| r.get("status").and_then(Value::as_str) == Some("forked")) else { return };
        let Some(agent) = result.get("agentId").and_then(Value::as_str) else { return };
        let Some((t, e)) = call.and_then(|c| self.calls.get(c).copied()) else { return };
        if let Some(Entry::Tool(tool)) = self.turns.get_mut(t).and_then(|turn| turn.entries.get_mut(e)) {
            tool.kind = "agent";
            tool.agent = Some("Skill".into());
            tool.call = call.map(String::from);
            tool.forked = Some(agent.to_string());
        }
    }

    /// A skill you typed that runs as a subagent (`/code-review`): no call in the transcript, only
    /// Claude Code's line saying it was launched (`<forked-skill-launch>`, its agent's id); a call
    /// stands in for it, and in the background, a task until its notification.
    pub(super) fn typed_fork(&mut self, text: &str, at: Option<&str>) {
        let Some(launch) = tagged(text, "forked-skill-launch").and_then(|l| serde_json::from_str::<Value>(l).ok()) else { return };
        let Some(agent) = launch.get("agentId").and_then(Value::as_str) else { return };
        let skill = launch.get("skillName").and_then(Value::as_str).map(String::from);
        let call = format!("forked:{agent}");
        let tool = Tool { tool: "Skill".into(), kind: "agent", agent: Some("Skill".into()), what: skill.clone(), state: "ok", call: Some(call.clone()), forked: Some(agent.to_string()), ..Tool::default() };
        self.call(Some(&call), at, tool);
        if tagged(text, "local-command-stdout").is_some_and(|s| s.contains("in the background")) {
            self.background.push(Background { kind: "agent", what: skill, state: "running", at: at.map(String::from), id: agent.to_string(), call, ..Background::default() });
        }
    }

    /// Claude Code's notification about a background task: its end, or a monitor's event.
    pub(super) fn notified(&mut self, text: &str, at: Option<&str>) {
        let tag = |name: &str| text.split_once(&format!("<{name}>")).and_then(|(_, rest)| rest.split_once(&format!("</{name}>"))).map(|(v, _)| unescape(v.trim()));
        let Some(id) = tag("task-id") else { return };
        if !self.notifications.insert(text.trim().to_string()) {
            return;
        }
        let Some(b) = self.background.iter_mut().rev().find(|b| b.id == id) else { return };
        if let Some(event) = tag("event").filter(|_| tag("status").is_none()) {
            b.events += 1;
            b.summary = Some(cut(&event, 300));
            return;
        }
        b.state = match tag("status").as_deref() {
            Some("completed") => "completed",
            // Stopped by its call, its command ends on a signal: stopped all the same.
            Some("failed") if b.state == "stopped" => "stopped",
            Some("failed") => "failed",
            Some("killed" | "stopped") => "stopped",
            _ => return,
        };
        b.ended = b.ended.take().filter(|_| b.state == "stopped").or(at.map(String::from));
        b.summary = tag("summary").map(|s| cut(&s, 300)).or(b.summary.take());
    }
}
