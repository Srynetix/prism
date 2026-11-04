use super::*;

#[test]
fn a_subagent_is_read_as_its_transcript_grows() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("prism-sub-grows-{}.jsonl", std::process::id()));
    let call = |id: &str| serde_json::json!({ "type": "assistant", "message": { "content": [{ "type": "tool_use", "id": id, "name": "Grep", "input": {} }] } }).to_string() + "\n";
    let done = |id: &str| serde_json::json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": id }] } }).to_string() + "\n";
    let (names, mut fold) = (Names { prefix: None }, SubFold::default());
    let mut append = |text: &str| {
        std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap().write_all(text.as_bytes()).unwrap();
        let len = std::fs::metadata(&path).unwrap().len();
        fold.catch_up(&path, len, &names);
        fold.sub()
    };
    let _ = std::fs::remove_file(&path);
    let all: String = (0..10).map(|i| call(&format!("c{i}"))).collect();
    assert_eq!(append(&all).count, 10);
    // A result cut in two writes: once whole, counted; that of a call no longer shown, ignored.
    let result = done("c9");
    let (head, tail) = result.split_at(20);
    assert_eq!(append(head).steps.last().unwrap().state, "running");
    let sub = append(&(tail.to_string() + &done("c0")));
    assert_eq!((sub.count, sub.steps.len(), sub.steps.last().unwrap().state, sub.steps[0].state), (10, SUB_STEPS, "ok", "running"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_transcripts_kept_weigh_at_most_their_budget_but_the_ones_in_use() {
    let ago = |s: u64| Instant::now() - std::time::Duration::from_secs(s);
    let kept = |used, mb: u64| Kept { used, bytes: mb << 20, slot: Slot::default() };
    let names = |m: &HashMap<PathBuf, Kept>| {
        let mut n: Vec<_> = m.keys().map(|p| p.display().to_string()).collect();
        n.sort();
        n
    };
    // 300 MB: the idle one used least leaves; the current one stays, even the least used.
    let mut map: HashMap<PathBuf, Kept> = [("cur", kept(ago(100), 94)), ("old", kept(ago(90), 94)), ("idle", kept(ago(60), 94)), ("open", kept(ago(1), 18))].into_iter().map(|(p, k)| (PathBuf::from(p), k)).collect();
    evict(&mut map, Path::new("cur"));
    assert_eq!(names(&map), ["cur", "idle", "open"]);
    // Over the budget with every other one in use (read seconds ago): all stay.
    let mut busy: HashMap<PathBuf, Kept> = [("a", kept(ago(2), 200)), ("b", kept(ago(1), 200))].into_iter().map(|(p, k)| (PathBuf::from(p), k)).collect();
    evict(&mut busy, Path::new("a"));
    assert_eq!(names(&busy), ["a", "b"]);
}

#[test]
fn only_new_leaves_out_the_turns_held() {
    let turn = |n: u32, prompt: &str| Turn { n, prompt: prompt.into(), ..Turn::default() };
    let mut first = Conversation { turns: vec![turn(1, "a"), turn(2, "b")], ..Conversation::default() };
    first.only_new(&[]);
    assert!(first.kept.is_empty() && first.turns.iter().all(|t| !t.sig.is_empty()));
    let have: Vec<String> = first.turns.iter().map(|t| t.sig.clone()).collect();
    // The last turn moved, a third began: the first one is held.
    let mut next = Conversation { turns: vec![turn(1, "a"), turn(2, "b, more"), turn(3, "c")], ..Conversation::default() };
    next.only_new(&have);
    assert_eq!(next.kept, vec![1]);
    assert_eq!(next.turns.iter().map(|t| t.n).collect::<Vec<_>>(), vec![2, 3]);
    // Numbered anew (read from further back): the same words under another number are sent.
    let mut again = Conversation { turns: vec![turn(2, "a")], ..Conversation::default() };
    again.only_new(&have);
    assert!(again.kept.is_empty());
}
use serde_json::json;
use std::io::Write;

fn file(name: &str, lines: &[Value]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("prism-conversation-{name}-{}.jsonl", std::process::id()));
    std::fs::write(&path, lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n")).unwrap();
    path
}

fn assistant(content: Value) -> Value {
    json!({ "type": "assistant", "timestamp": "2026-10-17T10:00:05Z", "message": { "model": "claude-opus-5-5", "content": content } })
}

fn user(text: &str) -> Value {
    json!({ "type": "user", "timestamp": "2026-10-17T10:00:00Z", "message": { "content": text } })
}

#[test]
fn reads_only_what_was_written_since() {
    let path = file("since", &[user("first"), assistant(json!([{ "type": "text", "text": "one" }]))]);
    assert_eq!(read(&path, None, 10).total, 1);
    let more = [user("second"), assistant(json!([{ "type": "text", "text": "two" }]))].map(|l| format!("\n{l}")).concat();
    std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(more.as_bytes()).unwrap();
    let c = read(&path, None, 10);
    assert_eq!(c.turns.iter().map(|t| (t.n, t.prompt.as_str())).collect::<Vec<_>>(), [(1, "first"), (2, "second")]);
    assert_eq!(c.turns[1].entries, [Entry::Text { text: "two".into() }]);
}

#[test]
fn signatures_kept_between_reads_are_those_of_the_turns_as_they_are() {
    let call = assistant(json!([{ "type": "tool_use", "id": "b1", "name": "Bash", "input": { "command": "make" } }]));
    let path = file("sigs", &[user("first"), call, user("second"), assistant(json!([{ "type": "text", "text": "two" }]))]);
    let signed = |c: &Conversation| c.turns.iter().map(|t| (t.sig.clone(), sign(&Turn { sig: String::new(), ..t.clone() }))).collect::<Vec<_>>();
    let mut before = read(&path, None, 10);
    before.only_new(&[]);
    assert!(signed(&before).iter().all(|(kept, fresh)| kept == fresh));
    // The first turn's command ends after the second turn began: that turn alone moved.
    let more = format!("\n{}", json!({ "type": "user", "timestamp": "2026-10-17T10:01:00Z", "message": { "content": [{ "type": "tool_result", "tool_use_id": "b1", "content": "ok" }] } }));
    std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(more.as_bytes()).unwrap();
    let mut after = read(&path, None, 10);
    after.only_new(&[]);
    let sigs = signed(&after);
    assert!(sigs.iter().all(|(kept, fresh)| kept == fresh), "{sigs:?}");
    assert_ne!(after.turns[0].sig, before.turns[0].sig);
    assert_eq!(after.turns[1].sig, before.turns[1].sig);
}

#[test]
fn reads_further_back_when_more_turns_are_asked() {
    // A first turn, then more than the end read first.
    let filler = assistant(json!([{ "type": "text", "text": "x".repeat(10_000) }]));
    let mut lines = vec![user("first")];
    lines.extend(std::iter::repeat_n(filler, (TAIL_BYTES / 10_000 + 10) as usize));
    lines.push(user("last"));
    let path = file("back", &lines);
    let c = read(&path, None, 1);
    assert_eq!((c.turns[0].prompt.as_str(), c.partial), ("last", true));
    let c = read(&path, None, 5);
    assert_eq!((c.total, c.partial, c.turns[0].prompt.as_str()), (2, false, "first"));
}

#[test]
fn a_shell_command_is_a_turn_with_its_output() {
    let path = file("shell", &[
        user("first"),
        json!({ "type": "user", "message": { "content": "<bash-input>git status</bash-input>" } }),
        json!({ "type": "user", "message": { "content": "<bash-stdout>clean</bash-stdout><bash-stderr></bash-stderr>" } }),
        // Sent while it worked: the output comes with the command.
        json!({ "type": "user", "message": { "content": "<bash-input>ls nope</bash-input><bash-stdout></bash-stdout><bash-stderr>no such file</bash-stderr>" } }),
    ]);
    let c = read(&path, None, 10);
    let shown = |t: &Turn| match &t.entries[..] {
        [Entry::Tool(tool)] => (t.prompt.clone(), tool.what.clone(), tool.state, tool.output.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(c.turns.len(), 3);
    assert_eq!(shown(&c.turns[1]), ("! git status".into(), Some("git status".into()), "ok", Some("clean".into())));
    assert_eq!(shown(&c.turns[2]), ("! ls nope".into(), Some("ls nope".into()), "failed", Some("no such file".into())));
}

#[test]
fn reads_claude_codes_turns() {
    let path = file("claude", &[
        json!({ "type": "user", "timestamp": "2026-10-17T10:00:00Z", "message": { "content": "add the CSV export" } }),
        json!({ "type": "user", "isMeta": true, "message": { "content": "<system-reminder>x</system-reminder>" } }),
        assistant(json!([{ "type": "text", "text": "Looking first." }, { "type": "tool_use", "id": "e1", "name": "Edit", "input": { "file_path": "/repo/src/a.ts", "old_string": "import x\nold()", "new_string": "import x\nnew()\nmore()" } }])),
        json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "e1", "content": "ok" }] } }),
        assistant(json!([{ "type": "tool_use", "id": "b1", "name": "Bash", "input": { "command": "npm test" } }])),
        json!({ "type": "user", "timestamp": "2026-10-17T10:01:00Z", "message": { "content": [{ "type": "tool_result", "tool_use_id": "b1", "is_error": true, "content": "1 failed" }] } }),
        json!({ "type": "assistant", "isSidechain": true, "message": { "content": [{ "type": "text", "text": "a subagent's" }] } }),
        json!({ "type": "user", "message": { "content": [{ "type": "text", "text": "[Request interrupted by user]" }] } }),
        json!({ "type": "user", "message": { "content": "<command-name>/compact</command-name>\n<command-args></command-args>" } }),
        json!({ "type": "user", "message": { "content": "<local-command-stdout>done</local-command-stdout>" } }),
        json!({ "type": "user", "message": { "content": "<pasted_content id=\"a1\">\nfirst line\nsecond line\n</pasted_content id=\"a1\">" } }),
    ]);
    let c = read(&path, Some(Path::new("/repo")), 10);
    assert_eq!((c.total, c.partial), (3, false));
    let t = &c.turns[0];
    assert_eq!((t.prompt.as_str(), t.at.as_deref(), t.last.as_deref()), ("add the CSV export", Some("2026-10-17T10:00:00Z"), Some("2026-10-17T10:01:00Z")));
    assert_eq!(t.entries[0], Entry::Text { text: "Looking first.".into() });
    let Entry::Tool(edit) = &t.entries[1] else { panic!() };
    assert_eq!((edit.what.as_deref(), edit.state, edit.added, edit.removed), (Some("src/a.ts"), "ok", 2, 1));
    assert_eq!(edit.diff, vec![" import x", "-old()", "+new()", "+more()"]);
    let Entry::Tool(bash) = &t.entries[2] else { panic!() };
    assert_eq!((bash.state, bash.output.as_deref()), ("failed", Some("1 failed")));
    assert_eq!(t.entries[3], Entry::Note { text: "Interrupted".into() });
    assert_eq!(t.entries.len(), 4, "a subagent's words stay out");
    assert_eq!((c.turns[1].prompt.as_str(), c.turns[1].entries.len()), ("/compact", 0));
    assert_eq!(c.turns[2].prompt, "first line\nsecond line", "a paste reads as itself");
}

#[test]
fn reads_a_prompt_sent_while_it_worked() {
    let queued = |prompt: &str, extra: Value| {
        let mut a = json!({ "type": "queued_command", "commandMode": "prompt", "prompt": prompt, "origin": { "kind": "human" } });
        a.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        json!({ "type": "attachment", "timestamp": "2026-10-17T10:00:30Z", "attachment": a })
    };
    let path = file("queued", &[
        json!({ "type": "user", "timestamp": "2026-10-17T10:00:00Z", "message": { "content": "add the CSV export" } }),
        assistant(json!([{ "type": "text", "text": "Working." }])),
        queued("also the JSON one", json!({})),
        queued("<system-reminder>x</system-reminder>", json!({ "isMeta": true })),
        queued("from another agent", json!({ "isMeta": true, "origin": { "kind": "peer", "from": "a1" } })),
        assistant(json!([{ "type": "text", "text": "Both done." }])),
    ]);
    let c = read(&path, None, 10);
    assert_eq!(c.turns.iter().map(|t| t.prompt.as_str()).collect::<Vec<_>>(), ["add the CSV export", "also the JSON one"]);
    assert_eq!(c.turns[1].at.as_deref(), Some("2026-10-17T10:00:30Z"));
    assert_eq!(c.turns[1].entries, vec![Entry::Text { text: "Both done.".into() }]);
}

/// As Claude Code writes an image pasted in its prompt: whole, before the words.
#[test]
fn finds_the_images_pasted_in_a_prompt() {
    let image = json!({ "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo=" } });
    let path = file(
        "pasted",
        &[
            user("first"),
            json!({ "type": "user", "message": { "content": [image, { "type": "text", "text": "[Image #1] What colour?" }] } }),
            json!({ "type": "user", "isMeta": true, "message": { "content": [{ "type": "text", "text": "[Image: source: /tmp/x/images/2.png]" }] } }),
        ],
    );
    let c = read(&path, None, 10);
    assert_eq!(c.turns[0].pasted, None);
    let pasted = c.turns[1].pasted.expect("its image");
    assert_eq!(pasted.count, 1);
    assert_eq!(pasted_image(&path, pasted.line, 0).unwrap(), "data:image/png;base64,iVBORw0KGgo=");
    assert!(pasted_image(&path, pasted.line, 1).is_err());
    assert!(pasted_image(&path, 0, 0).is_err(), "another line");
    let _ = std::fs::remove_file(path);
}

#[test]
fn unwraps_pasted_text_where_it_is() {
    assert_eq!(unpasted("see:\n<pasted_content id=\"b2\">\nx = 1\n</pasted_content id=\"b2\">\nthanks"), "see:\nx = 1\nthanks");
    assert_eq!(unpasted("<pasted_content id=\"c\">\nunclosed"), "<pasted_content id=\"c\">\nunclosed");
    assert_eq!(prompt_of("<div>my HTML</div>"), None, "other tags stay the CLI's");
}

#[test]
fn reads_codexs_rollout() {
    let path = file("codex", &[
        json!({ "type": "response_item", "payload": { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "<environment_context>x</environment_context>" }] } }),
        json!({ "type": "response_item", "timestamp": "2026-10-17T10:00:00Z", "payload": { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "fix the test" }] } }),
        json!({ "type": "turn_context", "payload": { "model": "gpt-6.1-sol", "cwd": "/x" } }),
        json!({ "type": "event_msg", "payload": { "type": "token_count", "info": { "last_token_usage": { "input_tokens": 1_000_000, "cached_input_tokens": 500_000, "output_tokens": 100_000 }, "total_token_usage": { "total_tokens": 1_100_000 } } } }),
        // Sent again unchanged: once.
        json!({ "type": "event_msg", "payload": { "type": "token_count", "info": { "last_token_usage": { "input_tokens": 1_000_000, "cached_input_tokens": 500_000, "output_tokens": 100_000 }, "total_token_usage": { "total_tokens": 1_100_000 } } } }),
        json!({ "type": "response_item", "payload": { "type": "function_call", "name": "shell", "call_id": "c1", "arguments": "{\"command\":[\"bash\",\"-lc\",\"cargo test\"]}" } }),
        json!({ "type": "response_item", "payload": { "type": "function_call_output", "call_id": "c1", "output": "{\"output\":\"test result: ok\",\"metadata\":{\"exit_code\":0}}" } }),
        json!({ "type": "response_item", "payload": { "type": "custom_tool_call", "name": "apply_patch", "call_id": "p1", "input": "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-old\n+new\n*** End Patch" } }),
        json!({ "type": "response_item", "payload": { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "Fixed." }] } }),
    ]);
    let c = read(&path, None, 10);
    assert_eq!(c.total, 1);
    let t = &c.turns[0];
    assert_eq!(t.prompt, "fix the test");
    assert_eq!((t.tokens, t.model.as_deref(), c.model.as_deref()), (1_100_000, Some("gpt-6.1-sol"), Some("gpt-6.1-sol")));
    assert!((t.cost.unwrap() - 2.05).abs() < 1e-9);
    let Entry::Tool(shell) = &t.entries[0] else { panic!() };
    assert_eq!((shell.kind, shell.what.as_deref(), shell.state, shell.output.as_deref()), ("command", Some("bash -lc cargo test"), "ok", Some("test result: ok")));
    let Entry::Tool(patch) = &t.entries[1] else { panic!() };
    assert_eq!((patch.kind, patch.what.as_deref(), patch.state, patch.added, patch.removed), ("write", Some("src/a.rs"), "running", 1, 1));
    assert_eq!(t.entries[2], Entry::Text { text: "Fixed.".into() });
}

/// As Command Code 1.88.0 writes it: a `message` per line, its tools in snake_case, its usage
/// in camelCase with its cost.
#[test]
fn reads_command_codes_transcript() {
    let usage = json!({ "inputTokens": 100, "outputTokens": 20, "cacheReadTokens": 800, "cacheWriteTokens": 80, "costUsd": 0.25 });
    let path = file("command-code", &[
        json!({ "type": "session", "id": "s1", "cwd": "/x", "version": "1.88.0", "timestamp": "2026-10-24T10:00:00Z" }),
        json!({ "type": "message", "id": "u1", "timestamp": "2026-10-24T10:00:00Z", "message": { "role": "user", "content": [{ "type": "text", "text": "fix the test" }], "meta": { "source": "user" } } }),
        json!({ "type": "message", "id": "a1", "timestamp": "2026-10-24T10:00:02Z", "model": "claude-sonnet-5-5", "usage": usage, "message": { "role": "assistant", "content": [
            { "type": "text", "text": "Running it." },
            { "type": "tool_use", "id": "t1", "name": "shell_command", "input": { "command": "cargo test", "description": "tests" } },
            { "type": "tool_use", "id": "t2", "name": "edit_file", "input": { "file_path": "/x/src/a.rs", "old_string": "old", "new_string": "new" } },
            { "type": "tool_use", "id": "t3", "name": "todo_write", "input": { "todos": [{ "content": "Fix", "status": "in_progress", "activeForm": "Fixing" }] } },
        ], "meta": { "source": "model" } } }),
        json!({ "type": "message", "id": "r1", "timestamp": "2026-10-24T10:00:05Z", "message": { "role": "user", "content": [
            { "type": "tool_result", "tool_use_id": "t1", "content": [{ "type": "text", "text": "test result: ok" }] },
            { "type": "tool_result", "tool_use_id": "t2", "content": [{ "type": "text", "text": "edited" }] },
        ], "meta": { "source": "tool" } } }),
        json!({ "type": "message", "id": "a2", "timestamp": "2026-10-24T10:00:06Z", "model": "claude-sonnet-5-5", "usage": usage, "message": { "role": "assistant", "content": [{ "type": "text", "text": "Fixed." }], "meta": { "source": "model" } } }),
    ]);
    let c = read(&path, Some(Path::new("/x")), 10);
    assert_eq!(c.total, 1);
    let t = &c.turns[0];
    assert_eq!(t.prompt, "fix the test");
    assert_eq!((t.tokens, t.model.as_deref(), c.model.as_deref()), (2_000, Some("claude-sonnet-5-5"), Some("claude-sonnet-5-5")));
    assert!((t.cost.unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(t.entries[0], Entry::Text { text: "Running it.".into() });
    let Entry::Tool(shell) = &t.entries[1] else { panic!() };
    assert_eq!((shell.kind, shell.what.as_deref(), shell.state, shell.output.as_deref()), ("command", Some("cargo test"), "ok", Some("test result: ok")));
    let Entry::Tool(edit) = &t.entries[2] else { panic!() };
    assert_eq!((edit.kind, edit.what.as_deref(), edit.state, edit.added, edit.removed), ("write", Some("src/a.rs"), "ok", 1, 1));
    assert_eq!((c.plan.len(), c.plan[0].text.as_str(), c.plan[0].status.as_str()), (1, "Fix", "in_progress"));
    assert_eq!(t.entries.last(), Some(&Entry::Text { text: "Fixed.".into() }));
}

/// As Claude Code 2.1.303 writes it: the plan in its file, `ExitPlanMode` without it (or with
/// a stale one), refused with words, edited, then approved with words.
#[test]
fn reads_a_plan_its_versions_and_their_answers() {
    let f = "/Users/x/.claude/plans/fix.md";
    let tool = |id: &str, name: &str, input: Value| assistant(json!([{ "type": "tool_use", "id": id, "name": name, "input": input }]));
    let result = |id: &str, content: &str, error: bool, extra: Value| json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": id, "content": content, "is_error": error }] }, "toolUseResult": extra });
    let mut lines = vec![
        user("Plan the fix"),
        json!({ "type": "attachment", "attachment": { "type": "plan_mode", "planFilePath": f } }),
        tool("w1", "Write", json!({ "file_path": f, "content": "# Fix\n1. Change the operator.\n2. Add a test in test_calc.py." })),
        tool("p1", "ExitPlanMode", json!({})),
        result("w1", "File created", false, json!({})),
    ];
    let c = read(&file("plan-waiting", &lines), None, 5);
    let Entry::Tool(t) = &c.turns[0].entries[1] else { panic!() };
    let p = t.proposal.as_ref().unwrap();
    assert_eq!((p.call.as_str(), p.version, p.verdict, p.text.as_str()), ("p1", 1, "waiting", "# Fix\n1. Change the operator.\n2. Add a test in test_calc.py."));

    lines.extend([
        result("p1", "The user doesn't want to proceed with this tool use. To tell you how to proceed, the user said:\nPut the test in tests/.\n\nNote: The user's next message may contain a correction.", true, json!("Error: …")),
        tool("e1", "Edit", json!({ "file_path": f, "old_string": "in test_calc.py", "new_string": "in tests/test_calc.py" })),
        tool("p2", "ExitPlanMode", json!({ "plan": "# Fix (stale)" })),
        result("e1", "updated", false, json!({})),
        json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "p2", "content": "User has approved your plan." }, { "type": "text", "text": "Keep the test minimal." }] }, "toolUseResult": { "plan": "# Fix\n1. Change the operator.\n2. Add a test in tests/test_calc.py.\n", "filePath": f } }),
    ]);
    let c = read(&file("plan-answered", &lines), None, 5);
    assert_eq!(c.turns.len(), 1, "words with an approval are no prompt");
    let plans: Vec<_> = c.turns[0].entries.iter().filter_map(|e| match e { Entry::Tool(t) => t.proposal.clone(), _ => None }).collect();
    assert_eq!((plans[0].verdict, plans[0].feedback.as_deref()), ("changes", Some("Put the test in tests/.")));
    assert_eq!((plans[1].version, plans[1].verdict, plans[1].feedback.as_deref()), (2, "approved", Some("Keep the test minimal.")));
    assert_eq!(plans[1].text, "# Fix\n1. Change the operator.\n2. Add a test in tests/test_calc.py.", "its file as edited, not the call's stale plan");
}

#[test]
fn reads_a_question_and_its_answers() {
    let input = json!({ "questions": [{ "question": "Which pet?", "header": "Pet", "multiSelect": false, "options": [{ "label": "Cat", "description": "" }, { "label": "Dog", "description": "" }] }] });
    let asked = [json!({ "type": "user", "message": { "content": "ask me" } }), assistant(json!([{ "type": "tool_use", "id": "q1", "name": "AskUserQuestion", "input": input }]))];
    let c = read(&file("ask", &asked), None, 1);
    let Entry::Tool(t) = &c.turns[0].entries[0] else { panic!() };
    let ask = t.ask.as_ref().unwrap();
    assert_eq!((t.state, ask.call.as_str(), ask.questions[0].options[1].label.as_str(), &ask.answers), ("running", "q1", "Dog", &vec![None]));
    let answered = [asked[0].clone(), asked[1].clone(), json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "q1", "content": "answered" }] }, "toolUseResult": { "answers": { "Which pet?": "Dog" } } })];
    let c = read(&file("answered", &answered), None, 1);
    let Entry::Tool(t) = &c.turns[0].entries[0] else { panic!() };
    assert_eq!((t.state, &t.ask.as_ref().unwrap().answers), ("ok", &vec![Some("Dog".into())]));
}

/// As Command Code 1.88.2 writes it once answered: the answers in the result's text, a line
/// each; before, its mod's question (`Conversation::waiting`) stands in, a turn of its own.
#[test]
fn reads_command_codes_question() {
    let input = json!({ "questions": [{ "header": "Pet", "question": "Which pet?", "options": [{ "label": "Cat" }, { "label": "Dog" }] }] });
    let ask = crate::question::parse("call_1", &input).unwrap();
    let mut c = Conversation::default();
    c.waiting(crate::status::Waiting::Question(ask.clone()));
    let Entry::Tool(t) = &c.turns[0].entries[0] else { panic!() };
    assert_eq!((c.total, c.turns[0].n, t.state, t.ask.as_ref()), (1, 1, "running", Some(&ask)));
    let path = file("command-code-ask", &[
        json!({ "type": "message", "id": "u1", "timestamp": "2026-11-04T10:07:32Z", "message": { "role": "user", "content": [{ "type": "text", "text": "ask me" }] } }),
        json!({ "type": "message", "id": "a1", "timestamp": "2026-11-04T10:07:32Z", "message": { "role": "assistant", "content": [{ "type": "tool_use", "id": "call_1", "name": "ask_user_question", "input": input }] } }),
        json!({ "type": "message", "id": "r1", "timestamp": "2026-11-04T10:07:32Z", "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "call_1", "content": [{ "type": "text", "text": "Which pet? → Dog\n\nContinue with the user's answers in mind." }] }] } }),
    ]);
    let mut c = read(&path, None, 10);
    c.waiting(crate::status::Waiting::Question(ask));
    assert_eq!(c.total, 1, "its call in the transcript: no second one");
    let Entry::Tool(t) = &c.turns[0].entries[0] else { panic!() };
    assert_eq!((t.tool.as_str(), t.state, &t.ask.as_ref().unwrap().answers), ("AskUserQuestion", "ok", &vec![Some("Dog".into())]));
}

/// As Command Code 1.88.2 wrote it, tried: v1 sent back with a comment (its review a "steering"
/// message), v2 approved; before, the mod's plan (`Conversation::waiting`) stands in.
#[test]
fn reads_command_codes_plans() {
    let msg = |id: &str, role: &str, source: &str, content: Value| json!({ "type": "message", "id": id, "timestamp": "2026-11-04T10:30:00Z", "message": { "role": role, "content": content, "meta": { "source": source } } });
    let write = |id: &str, text: &str| json!([{ "type": "tool_use", "id": id, "name": "write_file", "input": { "file_path": "~/.commandcode/plans/hello-txt.md", "content": text } }]);
    let result = |id: &str, text: &str| json!([{ "type": "tool_result", "tool_use_id": id, "content": [{ "type": "text", "text": text }] }]);
    let lines = [
        msg("u1", "user", "user", json!([{ "type": "text", "text": "Plan hello.txt" }])),
        msg("a1", "assistant", "model", write("w1", "# Plan\n\n1. Write hi.\n")),
        msg("r1", "user", "tool", result("w1", "File created")),
        msg("a2", "assistant", "model", json!([{ "type": "tool_use", "id": "p1", "name": "exit_plan_mode", "input": { "plan": "1. Write hi." } }])),
        msg("r2", "user", "tool", result("p1", "Plan not approved — staying in plan mode. STOP and ask the user what they would like changed.")),
        msg("s1", "user", "steering", json!([{ "type": "text", "text": "I reviewed the plan \"Hello\" saved at /x/hello-txt.md and commented on specific lines:\n\n- Line 1: \"# Plan\"\n  Comment: Write bye instead.\n\nAddress every comment." }])),
        msg("a3", "assistant", "model", write("w2", "# Plan\n\n1. Write bye.\n")),
        msg("r3", "user", "tool", result("w2", "updated")),
        msg("a4", "assistant", "model", json!([{ "type": "tool_use", "id": "p2", "name": "exit_plan_mode", "input": {} }])),
        msg("r4", "user", "tool", result("p2", "Plan approved — exited plan mode, now in auto-accept mode.\n\nYour plan is saved at: /x/hello-txt.md\n\n## Approved Plan:\n# Plan\n\n1. Write bye.\n")),
    ];
    let mut c = read(&file("command-code-plans", &lines), None, 10);
    assert_eq!(c.total, 1, "its review is no prompt of yours");
    let plans: Vec<&crate::plan::Proposal> = c.turns[0].entries.iter().filter_map(|e| match e {
        Entry::Tool(t) => t.proposal.as_ref(),
        _ => None,
    }).collect();
    assert_eq!(plans.iter().map(|p| (p.version, p.verdict, p.feedback.as_deref())).collect::<Vec<_>>(), [(1, "changes", Some("Write bye instead.")), (2, "approved", None)]);
    assert_eq!((plans[0].text.as_str(), plans[1].text.as_str()), ("# Plan\n\n1. Write hi.", "# Plan\n\n1. Write bye."));
    c.waiting(crate::status::Waiting::Plan { call: "p3".into(), text: "# Plan\n\n1. Write ciao.\n".into() });
    let Entry::Tool(t) = &c.turns[1].entries[0] else { panic!() };
    let p = t.proposal.as_ref().unwrap();
    assert_eq!((t.state, p.version, p.verdict, p.text.as_str()), ("running", 3, "waiting", "# Plan\n\n1. Write ciao."));
}

#[test]
fn reads_thinking_times_usage_plan_and_a_subagent() {
    let dir = std::env::temp_dir().join(format!("prism-conversation-sub-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("s1/subagents")).unwrap();
    let line = |v: Value| v.to_string();
    let main = [
        json!({ "type": "user", "timestamp": "2026-10-17T10:00:00Z", "message": { "content": "fix the flaky test" } }),
        json!({ "type": "assistant", "timestamp": "2026-10-17T10:00:06Z", "message": { "id": "m1", "model": "claude-opus-5-5", "usage": { "input_tokens": 10, "output_tokens": 90, "cache_read_input_tokens": 900 }, "content": [{ "type": "thinking", "thinking": "Jitter, surely.", "signature": "x" }] } }),
        json!({ "type": "assistant", "timestamp": "2026-10-17T10:00:07Z", "message": { "id": "m1", "model": "claude-opus-5-5", "usage": { "input_tokens": 10, "output_tokens": 90, "cache_read_input_tokens": 900 }, "content": [{ "type": "tool_use", "id": "t1", "name": "TodoWrite", "input": { "todos": [{ "content": "Reproduce", "status": "completed" }, { "content": "Fix", "status": "in_progress" }] } }] } }),
        json!({ "type": "assistant", "timestamp": "2026-10-17T10:00:08Z", "message": { "id": "m2", "model": "claude-opus-5-5", "content": [{ "type": "tool_use", "id": "a1", "name": "Agent", "input": { "description": "Callers", "prompt": "Who builds RetryPolicy?", "subagent_type": "Explore" } }] } }),
        json!({ "type": "user", "timestamp": "2026-10-17T10:00:14.500Z", "message": { "content": [{ "type": "tool_result", "tool_use_id": "a1", "content": "two" }] } }),
    ];
    std::fs::write(dir.join("s1.jsonl"), main.map(line).join("\n")).unwrap();
    let sub = [
        json!({ "type": "user", "isSidechain": true, "message": { "content": "Who builds RetryPolicy?" } }),
        json!({ "type": "assistant", "message": { "content": [{ "type": "tool_use", "id": "g1", "name": "Grep", "input": { "pattern": "RetryPolicy\\(" } }] } }),
        json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "g1", "content": "2 files" }] } }),
        json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "Two callers." }] } }),
    ];
    std::fs::write(dir.join("s1/subagents/agent-x.jsonl"), sub.map(line).join("\n")).unwrap();
    std::fs::write(dir.join("s1/subagents/agent-y.jsonl"), line(json!({ "type": "user", "message": { "content": "another task" } }))).unwrap();
    let c = read(&dir.join("s1.jsonl"), None, 5);
    let t = &c.turns[0];
    assert_eq!(t.entries[0], Entry::Thinking { text: "Jitter, surely.".into(), ms: Some(6000) });
    assert_eq!(t.tokens, 1000, "a reply's usage counted once");
    assert!(t.cost.is_some());
    assert_eq!((t.model.as_deref(), c.model.as_deref()), (Some("claude-opus-5-5"), Some("claude-opus-5-5")));
    assert_eq!(c.plan.iter().map(|p| (p.text.as_str(), p.status.as_str())).collect::<Vec<_>>(), vec![("Reproduce", "completed"), ("Fix", "in_progress")]);
    let Entry::Tool(agent) = &t.entries[2] else { panic!() };
    assert_eq!((agent.kind, agent.ms, agent.at.as_deref(), agent.agent.as_deref()), ("agent", Some(6500), Some("2026-10-17T10:00:08Z"), Some("Explore")));
    let sub = agent.sub.as_ref().unwrap();
    assert_eq!((sub.count, sub.steps[0].tool.as_str(), sub.steps[0].state, sub.said.as_deref()), (1, "Grep", "ok", Some("Two callers.")));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn keeps_the_last_turns() {
    let lines: Vec<Value> = (1..=5).map(|i| json!({ "type": "user", "message": { "content": format!("prompt {i}") } })).collect();
    let c = read(&file("last", &lines), None, 2);
    assert_eq!((c.total, c.turns.len(), c.turns[0].n, c.turns[0].prompt.as_str()), (5, 2, 4, "prompt 4"));
}

#[test]
fn follows_background_tasks() {
    let notif = |at: &str, body: &str| json!({ "type": "user", "timestamp": at, "origin": { "kind": "task-notification" }, "message": { "content": format!("<task-notification>\n{body}\n</task-notification>") } });
    let path = file("background", &[
        json!({ "type": "user", "timestamp": "2026-10-17T10:00:00Z", "message": { "content": "run it all" } }),
        assistant(json!([
            { "type": "tool_use", "id": "t1", "name": "Bash", "input": { "command": "npm test", "description": "Run the tests", "run_in_background": true } },
            { "type": "tool_use", "id": "t2", "name": "Bash", "input": { "command": "ls" } },
            { "type": "tool_use", "id": "t3", "name": "Monitor", "input": { "command": "tail -f log", "description": "Watch the log" } },
            { "type": "tool_use", "id": "t4", "name": "Agent", "input": { "description": "Look around", "prompt": "p", "run_in_background": true } },
            { "type": "tool_use", "id": "t5", "name": "Bash", "input": { "command": "npm run dev", "run_in_background": true } },
        ])),
        json!({ "type": "user", "toolUseResult": { "backgroundTaskId": "b1", "stdout": "" }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": "Command running in background with ID: b1" }] } }),
        json!({ "type": "user", "toolUseResult": { "stdout": "a" }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "t2", "content": "a" }] } }),
        json!({ "type": "user", "toolUseResult": { "taskId": "m1", "persistent": false }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "t3", "content": "started" }] } }),
        json!({ "type": "user", "toolUseResult": { "agentId": "a1", "isAsync": true, "status": "async_launched" }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "t4", "content": "launched" }] } }),
        json!({ "type": "user", "toolUseResult": { "backgroundTaskId": "b2" }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "t5", "content": "running" }] } }),
        notif("2026-10-17T10:02:00Z", "<task-id>b1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>completed</status>\n<summary>Background command \"Run the tests\" completed (exit code 0) &gt; ok</summary>"),
        notif("2026-10-17T10:02:10Z", "<task-id>m1</task-id>\n<summary>Monitor event</summary>\n<event>line 42</event>"),
        assistant(json!([{ "type": "tool_use", "id": "t6", "name": "TaskStop", "input": { "task_id": "b2" } }])),
        json!({ "type": "attachment", "attachment": { "type": "queued_command", "commandMode": "task-notification", "prompt": "<task-notification>\n<task-id>a1</task-id>\n<status>failed</status>\n<summary>Agent failed</summary>\n</task-notification>" } }),
        json!({ "type": "user", "message": { "content": "<task-notification>\n<task-id>m1</task-id>\n<summary>Monitor event</summary>\n<event>line 42</event>\n</task-notification>" } }),
    ]);
    let c = read(&path, None, 10);
    assert_eq!(c.turns.len(), 1, "a notification is no prompt");
    let b = |id: &str| c.background.iter().find(|b| b.id == id).unwrap();
    assert_eq!(c.background.len(), 4, "a call in the foreground is no task");
    assert_eq!((b("b1").kind, b("b1").what.as_deref(), b("b1").state, b("b1").ended.as_deref()), ("command", Some("Run the tests"), "completed", Some("2026-10-17T10:02:00Z")));
    assert_eq!(b("b1").summary.as_deref(), Some("Background command \"Run the tests\" completed (exit code 0) > ok"));
    assert_eq!((b("m1").kind, b("m1").state, b("m1").events, b("m1").summary.as_deref()), ("monitor", "running", 1, Some("line 42")));
    assert_eq!((b("a1").kind, b("a1").state, b("a1").summary.as_deref()), ("agent", "failed", Some("Agent failed")));
    assert_eq!((b("b2").what.as_deref(), b("b2").state), (Some("npm run dev"), "stopped"));
}

#[test]
fn a_subagent_launched_in_the_background_unasked_runs_until_its_task_ends() {
    let dir = std::env::temp_dir().join(format!("prism-conversation-async-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("s1/subagents")).unwrap();
    let line = |v: Value| v.to_string();
    let mut main = vec![
        json!({ "type": "user", "message": { "content": "look around" } }),
        assistant(json!([{ "type": "tool_use", "id": "a1", "name": "Agent", "input": { "description": "Look around", "prompt": "Where is it built?", "subagent_type": "Explore" } }])),
        json!({ "type": "user", "toolUseResult": { "agentId": "x", "isAsync": true, "status": "async_launched" }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "a1", "content": "launched" }] } }),
    ];
    std::fs::write(dir.join("s1.jsonl"), main.iter().cloned().map(line).collect::<Vec<_>>().join("\n")).unwrap();
    // Its prompt reworded: found by its call's id.
    std::fs::write(dir.join("s1/subagents/agent-x.jsonl"), line(json!({ "type": "user", "message": { "content": "Where is it built? (and more)" } }))).unwrap();
    std::fs::write(dir.join("s1/subagents/agent-x.meta.json"), line(json!({ "toolUseId": "a1", "requestShape": "background" }))).unwrap();
    let sub = |c: &Conversation| {
        let Entry::Tool(t) = &c.turns[0].entries[0] else { panic!() };
        (t.state, t.sub.as_ref().map(|s| s.running))
    };
    let c = read(&dir.join("s1.jsonl"), None, 5);
    assert_eq!(c.background.iter().map(|b| (b.kind, b.state)).collect::<Vec<_>>(), vec![("agent", "running")]);
    assert_eq!(sub(&c), ("ok", Some(true)));
    main.push(json!({ "type": "user", "message": { "content": "<task-notification>\n<task-id>x</task-id>\n<status>completed</status>\n<summary>Done</summary>\n</task-notification>" } }));
    std::fs::write(dir.join("s1.jsonl"), main.into_iter().map(line).collect::<Vec<_>>().join("\n")).unwrap();
    assert_eq!(sub(&read(&dir.join("s1.jsonl"), None, 5)), ("ok", Some(false)));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_skill_forked_in_the_background_is_a_subagent_until_its_task_ends() {
    let dir = std::env::temp_dir().join(format!("prism-conversation-skill-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("s1/subagents")).unwrap();
    let line = |v: Value| v.to_string();
    let mut main = vec![
        json!({ "type": "user", "message": { "content": "review it" } }),
        assistant(json!([{ "type": "tool_use", "id": "k1", "name": "Skill", "input": { "skill": "code-review", "args": "high" } }])),
        json!({ "type": "user", "toolUseResult": { "success": true, "commandName": "code-review", "status": "forked", "background": true, "agentId": "f1" }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "k1", "content": "Launching skill: code-review" }] } }),
    ];
    std::fs::write(dir.join("s1.jsonl"), main.iter().cloned().map(line).collect::<Vec<_>>().join("\n")).unwrap();
    // The skill's own prompt, and no call's id beside it: found by its agent's id.
    std::fs::write(
        dir.join("s1/subagents/agent-f1.jsonl"),
        [
            json!({ "type": "user", "message": { "content": "You are reviewing a pull request" } }),
            assistant(json!([{ "type": "tool_use", "id": "b1", "name": "Bash", "input": { "command": "git diff" } }])),
        ]
        .map(line)
        .join("\n"),
    )
    .unwrap();
    std::fs::write(dir.join("s1/subagents/agent-f1.meta.json"), line(json!({ "name": "code-review", "requestShape": "background" }))).unwrap();
    let tool = |c: &Conversation| {
        let Entry::Tool(t) = &c.turns[0].entries[0] else { panic!() };
        (t.kind, t.agent.clone(), t.what.clone(), t.sub.as_ref().map(|s| (s.count, s.running)))
    };
    let c = read(&dir.join("s1.jsonl"), None, 5);
    assert_eq!(c.background.iter().map(|b| (b.kind, b.what.as_deref(), b.state)).collect::<Vec<_>>(), vec![("agent", Some("code-review"), "running")]);
    assert_eq!(tool(&c), ("agent", Some("Skill".into()), Some("code-review".into()), Some((1, true))));
    main.push(json!({ "type": "user", "message": { "content": "<task-notification>\n<task-id>f1</task-id>\n<tool-use-id>k1</tool-use-id>\n<status>completed</status>\n<summary>Skill \"code-review\" completed</summary>\n</task-notification>" } }));
    std::fs::write(dir.join("s1.jsonl"), main.into_iter().map(line).collect::<Vec<_>>().join("\n")).unwrap();
    assert_eq!(tool(&read(&dir.join("s1.jsonl"), None, 5)).3, Some((1, false)));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_skill_you_typed_forked_in_the_background_is_a_subagent_until_its_task_ends() {
    let dir = std::env::temp_dir().join(format!("prism-conversation-typed-skill-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("s1/subagents")).unwrap();
    let line = |v: Value| v.to_string();
    let mut main = vec![
        json!({ "type": "user", "message": { "content": "/code-review high" } }),
        json!({ "type": "system", "subtype": "local_command", "content": "<local-command-stdout>Running in the background as @code-review</local-command-stdout>\n<forked-skill-launch>{\"agentId\":\"f1\",\"skillName\":\"code-review\",\"description\":\"/code-review high\"}</forked-skill-launch>" }),
    ];
    std::fs::write(dir.join("s1.jsonl"), main.iter().cloned().map(line).collect::<Vec<_>>().join("\n")).unwrap();
    std::fs::write(
        dir.join("s1/subagents/agent-f1.jsonl"),
        [
            json!({ "type": "user", "message": { "content": "You are reviewing a pull request" } }),
            assistant(json!([{ "type": "tool_use", "id": "b1", "name": "Bash", "input": { "command": "git diff" } }])),
        ]
        .map(line)
        .join("\n"),
    )
    .unwrap();
    let tool = |c: &Conversation| {
        let Entry::Tool(t) = &c.turns[0].entries[0] else { panic!() };
        (t.kind, t.agent.clone(), t.what.clone(), t.sub.as_ref().map(|s| (s.count, s.running)))
    };
    let c = read(&dir.join("s1.jsonl"), None, 5);
    assert_eq!(c.turns[0].prompt, "/code-review high");
    assert_eq!(c.background.iter().map(|b| (b.kind, b.what.as_deref(), b.state)).collect::<Vec<_>>(), vec![("agent", Some("code-review"), "running")]);
    assert_eq!(tool(&c), ("agent", Some("Skill".into()), Some("code-review".into()), Some((1, true))));
    main.push(json!({ "type": "user", "message": { "content": "<task-notification>\n<task-id>f1</task-id>\n<status>completed</status>\n<summary>Skill \"code-review\" completed</summary>\n</task-notification>" } }));
    std::fs::write(dir.join("s1.jsonl"), main.into_iter().map(line).collect::<Vec<_>>().join("\n")).unwrap();
    assert_eq!(tool(&read(&dir.join("s1.jsonl"), None, 5)).3, Some((1, false)));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_subagent_woken_by_a_message_works_again_until_its_task_ends() {
    let dir = std::env::temp_dir().join(format!("prism-conversation-resumed-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("s1/subagents")).unwrap();
    let line = |v: Value| v.to_string();
    let mut main = vec![
        json!({ "type": "user", "message": { "content": "build it" } }),
        assistant(json!([{ "type": "tool_use", "id": "a1", "name": "Agent", "input": { "description": "Build the lib", "prompt": "Build the lib", "subagent_type": "Implementer" } }])),
        json!({ "type": "user", "toolUseResult": { "isAsync": true, "status": "async_launched", "agentId": "x1" }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "a1", "content": "Async agent launched" }] } }),
        json!({ "type": "user", "message": { "content": "<task-notification>\n<task-id>x1</task-id>\n<status>completed</status>\n<summary>Agent \"Build the lib\" completed</summary>\n</task-notification>" } }),
        json!({ "type": "user", "message": { "content": "fix the tests" } }),
        assistant(json!([{ "type": "tool_use", "id": "m1", "name": "SendMessage", "input": { "to": "x1", "summary": "Fix the failing tests", "message": "The tests fail" } }])),
        json!({ "type": "user", "toolUseResult": { "success": true, "resumedAgentId": "x1", "revivedAgentId": "x1" }, "message": { "content": [{ "type": "tool_result", "tool_use_id": "m1", "content": "Message sent" }] } }),
    ];
    std::fs::write(dir.join("s1.jsonl"), main.iter().cloned().map(line).collect::<Vec<_>>().join("\n")).unwrap();
    std::fs::write(
        dir.join("s1/subagents/agent-x1.jsonl"),
        [
            json!({ "type": "user", "message": { "content": "Build the lib" } }),
            assistant(json!([{ "type": "tool_use", "id": "b1", "name": "Bash", "input": { "command": "cargo build" } }])),
            json!({ "type": "user", "message": { "content": "The tests fail" } }),
            assistant(json!([{ "type": "tool_use", "id": "b2", "name": "Bash", "input": { "command": "cargo test" } }])),
        ]
        .map(line)
        .join("\n"),
    )
    .unwrap();
    std::fs::write(dir.join("s1/subagents/agent-x1.meta.json"), line(json!({ "toolUseId": "a1", "requestShape": "background" }))).unwrap();
    let card = |c: &Conversation, n: usize| {
        let Entry::Tool(t) = &c.turns[n].entries[0] else { panic!() };
        (t.kind, t.agent.clone(), t.what.clone(), t.sub.as_ref().map(|s| (s.count, s.running)))
    };
    let c = read(&dir.join("s1.jsonl"), None, 5);
    // The first card over, the message's one at work in the background on the same transcript.
    assert_eq!(card(&c, 0), ("agent", Some("Implementer".into()), Some("Build the lib".into()), Some((2, false))));
    assert_eq!(card(&c, 1), ("agent", Some("Implementer".into()), Some("Fix the failing tests".into()), Some((2, true))));
    assert_eq!(c.background.iter().map(|b| b.state).collect::<Vec<_>>(), ["completed", "running"]);
    main.push(json!({ "type": "user", "message": { "content": "<task-notification>\n<task-id>x1</task-id>\n<status>completed</status>\n<summary>Agent \"Build the lib\" completed: tests fixed</summary>\n</task-notification>" } }));
    std::fs::write(dir.join("s1.jsonl"), main.into_iter().map(line).collect::<Vec<_>>().join("\n")).unwrap();
    let c = read(&dir.join("s1.jsonl"), None, 5);
    assert_eq!(card(&c, 1).3, Some((2, false)));
    assert_eq!(c.background.iter().map(|b| b.state).collect::<Vec<_>>(), ["completed", "completed"]);
    std::fs::remove_dir_all(dir).ok();
}
