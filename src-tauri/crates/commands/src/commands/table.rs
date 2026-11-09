//! Every command the window invokes, once: its function, its class from a phone
//! (`prism_core::remote`), and what the demo profile does with it. `tauri_handler()` registers
//! them with Tauri (the window's own, `desk`, its app does), `class()` answers the remote, and the
//! demo profile's guard is derived from the class: a command that acts (`Act`, `Critical`) is
//! refused there, unless it answers the demo itself (`[demo: own]`); one that acts on the computer or
//! an account is refused too (`[demo: refused]`).
//! `block_call` takes the class its block gives the command (`remote::block_class`).

use prism_core::remote::Class::{self, Act, Critical, Never, Read};

use super::*;

commands! {
    // The window's own, answered by its app (`src-tauri/src/commands`): its window, menu, tray,
    // clipboard, Dock icon, audio, the profiles it relaunches on, its capture, its terminals.
    desk {
        list_profiles: Never,
        create_profile: Never,
        setup_profile: Never,
        rename_profile: Never,
        remove_profile: Never,
        switch_profile: Never,
        open_profile: Never,
        stop_profile: Never,
        set_help_guides: Never,
        set_tray: Never,
        font_face: Never,
        youtube_relay: Never,
        copy_text: Never,
        paste_text: Never,
        quit_app: Never,
        toggle_fullscreen: Never,
        audio_sources: Never,
        audio_feed: Never,
        now_playing: Never,
        now_playing_command: Never,
        feedback_capture: Never,
        // Read-only from a phone (tmux's `-r`), through its socket: typing is `agent_write`.
        agent_attach: Read,
        // Other computers' Prism, each in a window of this one (`[[machines]]`).
        machine_list: Never,
        machine_token: Never,
        machine_pair: Never,
        machine_open: Never,
        machine_main: Never,
        machine_connect: Never,
        machine_call: Never,
        machine_attach: Never,
        machine_write: Never,
        machine_resize: Never,
        machine_detach: Never,
        machine_export: Never,
        // Dictation (`[dictation]`): the computer's microphone, transcribed on it; its models.
        dictation_start: Never,
        dictation_stop: Never,
        dictation_cancel: Never,
        dictation_status: Never,
        dictation_download: Never,
        dictation_cancel_download: Never,
        dictation_remove: Never,
    }
    // Config, packs, tokens, updates: never from a phone.
    config::read_config: Never,
    // An agent's proposal applied from the window only (a custom block is trusted at the desk).
    config::mcp_proposal_answer: Never,
    config::mcp_installs: Never,
    config::mcp_install: Never [demo: refused],
    // A worktree's shell: typed in at the desk only.
    agents::worktree_shells: Never,
    agents::worktree_shell: Never,
    agents::worktree_close_shell: Never [demo: refused],
    // A worktree's branch pulled or switched (Worktrees): at the desk only.
    agents::worktree_branches: Never,
    agents::worktree_pull: Never [demo: refused],
    agents::worktree_switch: Never [demo: refused],
    // A pod's shell (`kubectl exec`): the desk only, the phone never reaches the clusters' insides.
    infra::pod_shells: Never,
    infra::pod_shell: Never,
    infra::pod_close_shell: Never [demo: refused],
    // Both, listed in the status bar.
    agents::open_shells: Never,
    agents::close_open_shell: Never [demo: refused],
    config::preview_config: Never,
    config::merged_config: Never,
    config::save_config: Never,
    config::reload_collectors: Never,
    config::open_config: Never,
    config::pack_import: Never,
    config::pack_update: Never,
    config::pack_remove: Never,
    config::pack_check: Never,
    config::update_check: Never,
    config::update_install: Never,
    config::check_setting: Never,
    config::detect_setup: Never,
    config::token_state: Never,
    config::store_token: Never,
    // More › Settings on a phone (`[remote] settings`): read, and the few keys of PHONE_SETTINGS saved.
    config::remote_settings_read: Read [phone: sync],
    config::remote_settings_save: Critical [phone: state, edits: Vec<crate::settings::Edit>; sync],
    // Written to config.toml, as a setting saved from a phone: its passkey.
    config::quick_action_add: Critical [phone: state, action: crate::config::QuickAction; sync],
    infra::kube_contexts: Never,
    integrations::calendar_names: Never,
    config::installed_apps: Never,
    agents::agent_models: Never,
    agents::agent_efforts: Read [phone: state, cli: crate::agents::provider::Provider, model: Option<String>],
    agents::agent_command_line: Read [phone: state, preview: crate::agents::LaunchPreview; sync],
    integrations::mail_sign_in_start: Never [demo: refused],
    integrations::mail_sign_in_wait: Never [demo: refused],
    integrations::mail_sign_out: Never [demo: refused],
    integrations::google_state: Never,
    integrations::google_sign_in: Never [demo: refused],
    integrations::google_sign_out: Never [demo: refused],
    // Clones.
    config::find_clones: Never,
    config::clone_repo: Never,
    config::inspect_folder: Never,
    config::init_repo: Never [demo: refused],
    config::create_project: Never [demo: refused],
    config::publish_repo: Never [demo: refused],
    // More › New project on a phone (`[remote] projects`): in the working folder only, the passkey each time.
    config::workspaces: Read [phone: sync],
    config::remote_project_create: Critical [demo: refused] [phone: workspace: String, name: String, remote: Option<crate::clones::NewRemote>],
    config::remote_project_search: Read [phone: workspace: String, query: String],
    config::remote_project_clone: Critical [demo: refused] [phone: workspace: String, project: String],
    config::remote_project_publish: Critical [demo: refused] [phone: name: String, remote: crate::clones::NewRemote],
    // What opens something on the computer, or belongs to its window.
    system::open_repo: Never [demo: refused],
    system::open_logs: Never,
    integrations::mail_open: Never [demo: refused],
    integrations::open_in_calendar: Never [demo: refused],
    agents::agent_open: Never [demo: refused],
    agents::agent_open_file: Never [demo: refused],
    agents::agent_files_export: Never [demo: refused],
    agents::agent_open_external: Never [demo: refused],
    system::reduce_transparency: Never,
    system::font_faces: Never,
    system::save_csv: Never,
    system::store_load: Never,
    system::store_save: Never,
    // Its class is its block's (`remote::block_class`), read before this one; the blocks answer the
    // demo themselves.
    system::block_call: Never [demo: own] [phone: block: String, command: String, args: Option<Value>],
    system::shared_load: Read [phone: state, name: String; sync],
    // The window's alerts of the blocks of another repository, for the routines and Changes.
    system::alerts_external: Never,
    system::shared_save: Act [demo: own] [phone: state, name: String, value: Value; sync],
    // Agents and stores outlive it: nothing is lost.
    system::restart_app: Act [demo: own],
    // The computer itself: `[remote] power` and the passkey every time (the remote's server).
    system::mac_power: Critical [demo: refused] [phone: action: String, password: Option<String>],
    system::logs_info: Never,
    system::logs_tail: Never,
    system::dependencies: Never,
    system::cli_command: Never,
    system::cli_command_install: Never,
    system::cli_command_remove: Never,
    system::problems_clear: Never,
    remote::remote_pair_start: Never,
    remote::remote_pair_confirm: Never,
    remote::remote_pair_cancel: Never,
    remote::remote_revoke: Never,
    remote::remote_peer_pair_start: Never,
    remote::remote_peer_pair_answer: Never,
    remote::remote_peer_pair_cancel: Never,
    remote::remote_peer_revoke: Never,
    remote::remote_away_checks: Never,
    remote::remote_rename: Never,
    remote::remote_release: Never,
    remote::remote_restart: Never,
    // Feedback: its composer on a phone too; the capture belongs to the window.
    feedback::feedback_info: Read [phone: state],
    feedback::feedback_similar: Read [phone: target: String, title: String],
    feedback::feedback_body: Read,
    feedback::feedback_url: Read [phone: target: String, kind: String, title: String, text: String, context: Vec<String>; sync],
    feedback::feedback_requests: Read [phone: state, refresh: bool],
    feedback::feedback_vote: Act [phone: target: String, number: u64],
    feedback::feedback_create: Act [phone: state, target: String, kind: String, title: String, text: String, context: Vec<String>, capture: Option<bool>],
    // An agent launched to draft it, its words in its prompt as a launch's from a phone.
    feedback::feedback_draft_start: Act [phone: state, target: String, kind: String, title: String, text: String],
    // Its file, once Prism no longer knows the draft.
    feedback::feedback_draft_read: Read [phone: state, id: String],
    // Reading.
    system::get_snapshot: Read,
    // The window's own: a phone gets its snapshot over its socket.
    system::snapshot_sync: Never,
    tracker::get_ticket: Read [phone: state, id: String],
    tracker::search_tickets: Read [phone: state, query: String],
    tracker::ticket_commits: Read [phone: state, id: String],
    tracker::ticket_image: Read [phone: state, url: String],
    tracker::refresh_tickets: Read [phone: state],
    // My sprint (`crate::retro`).
    tracker::retro_get: Read [phone: state, points: Option<Vec<crate::blocks::trail::retro::RetroPoint>>; sync],
    tracker::retro_keep: Act [demo: own] [phone: state, history: Option<Vec<crate::blocks::trail::retro::RetroSummary>>; sync],
    forge::mr_detail: Read [phone: state, repo: Option<String>, iid: u64],
    forge::mr_diff: Read [phone: state, repo: Option<String>, iid: u64],
    forge::mr_commits: Read [phone: state, repo: Option<String>, iid: u64],
    forge::mr_file: Read [phone: state, repo: Option<String>, iid: u64, path: String],
    forge::mr_image: Read [phone: state, repo: Option<String>, iid: u64, path: String, old: bool],
    forge::forge_members: Read [phone: state, repo: Option<String>],
    tracker::ticket_members: Read [phone: state, list: String],
    forge::forge_labels: Read [phone: state, repo: Option<String>],
    // An image of a forge text, fetched to be shown.
    forge::forge_upload: Read [phone: state, project: Option<String>, path: String],
    forge::forge_avatar: Read [phone: url: String],
    forge::forge_custom_emoji: Read [phone: state, project: Option<String>],
    forge::forge_emoji_image: Read [phone: url: String],
    forge::pipeline_jobs: Read [phone: state, project: Option<String>, pipeline: u64],
    forge::job_log: Read [phone: state, project: Option<String>, job: u64],
    forge::check_renovate: Read [demo: refused] [phone: state, marker: String],
    integrations::calendar_event: Read [phone: id: String],
    integrations::calendar_day: Read [phone: day: String],
    infra::workload_pods: Read [phone: state, context: String, namespace: String, deployment: String],
    infra::pod_log: Read [phone: state, context: String, namespace: String, pod: String, container: Option<String>, previous: bool, lines: Option<u32>],
    infra::pod_usage_history: Read [phone: state, context: String, namespace: String, pod: String, minutes: u32],
    agents::agent_detach: Read,
    agents::agent_in_copy_mode: Read [phone: state, id: String],
    agents::agent_activity: Read [phone: state, id: String; sync],
    agents::agent_costs: Read [phone: state; sync],
    agents::agent_context_detail: Read [phone: state, id: String],
    agents::agent_slash_commands: Read [phone: state, id: String],
    agents::agent_tasks: Read [phone: state, id: String],
    agents::agent_git_state: Read [phone: state, id: String],
    agents::launch_tasks: Read [phone: state, cwd: Option<String>, repo: Option<String>],
    agents::agent_files: Read [phone: state, id: String, ignored: Option<bool>],
    agents::agent_files_download: Read [phone: state, id: String, paths: Vec<String>, name: String],
    agents::agent_pages: Read [phone: state, id: String, ignored: bool],
    agents::agent_artifacts: Read [phone: state, id: String; sync],
    agents::agent_diff: Read [phone: state, id: String, branch: bool],
    agents::overlap_diff: Read [phone: state, a: String, b: String],
    agents::agent_task_files_changed: Read [phone: state, id: String],
    agents::agent_file_text: Read [phone: state, id: String, path: String],
    agents::agent_diff_image: Read [phone: state, id: String, path: String, branch: bool, old: bool],
    agents::agent_page_text: Read [phone: state, id: String, path: String; sync],
    agents::agent_file_preview: Read [phone: state, id: String, path: String],
    agents::agent_files_search: Read [phone: state, id: String, query: crate::agents::files::Query],
    agents::agent_files_stamps: Read [phone: state, id: String, paths: Vec<String>; sync],
    agents::agent_attachment: Read [phone: state, id: String, path: String; sync],
    agents::agent_pasted_image: Read [phone: state, id: String, line: u64, n: u32; sync],
    agents::agent_shells: Read [phone: state, id: String],
    agents::agent_set_effort: Act [phone: state, id: String, effort: Option<String>],
    agents::agent_conversation: Read [phone: state, id: String, turns: Option<u32>, have: Option<Vec<String>>],
    agents::agent_installed: Read [phone: state],
    agents::agent_handoff_material: Read [phone: state, id: String, turns: Option<u32>],
    agents::session_source: Read [phone: state, source: crate::agents::session_source::Source],
    // A draft asked of an agent (a quick action's prompt, a feedback issue): read back by Prism.
    agents::draft_state: Read [phone: state, id: String],
    agents::agent_leg_conversation: Read [phone: state, id: String, leg: u32, turns: Option<u32>],
    agents::agent_history: Read [phone: state, id: String, lines: Option<u32>, screen: Option<bool>],
    agents::agent_worktree_plan: Read [phone: state, title: String, worktree: crate::agents::NewWorktree],
    // Tickets and time.
    tracker::set_ticket_status: Act [phone: state, task_id: String, status: String],
    tracker::set_ticket_mine: Act [phone: state, task_id: String, assign: bool],
    tracker::set_ticket_priority: Act [phone: state, task_id: String, priority: Option<crate::state::Priority>],
    tracker::set_ticket_estimate: Act [phone: state, task_id: String, field: Option<String>, hours: Option<f64>],
    tracker::add_ticket_comment: Act [phone: state, task_id: String, text: String, mentions: Option<Vec<crate::collectors::task_view::Mention>>],
    tracker::edit_ticket_comment: Act [phone: state, task_id: String, comment: String, text: String, mentions: Option<Vec<crate::collectors::task_view::Mention>>],
    tracker::add_to_sprint: Act [phone: state, task_id: String],
    tracker::create_ticket: Act [phone: state, board: String, status: String, title: String, description: Option<String>, assign_me: Option<bool>],
    tracker::log_time: Act [phone: state, entry: crate::collectors::time::NewEntry],
    tracker::update_time: Act [phone: state, id: String, entry: crate::collectors::time::NewEntry],
    tracker::delete_time: Act [phone: state, id: String],
    // MRs and CI.
    forge::approve_mr: Act [phone: state, repo: Option<String>, iid: u64, approve: bool],
    forge::request_changes_mr: Act [phone: state, repo: Option<String>, iid: u64, on: bool, body: String],
    forge::comment_mr: Act [phone: state, repo: Option<String>, iid: u64, discussion: Option<String>, body: String],
    forge::comment_line: Act [phone: state, repo: Option<String>, iid: u64, line: crate::forge::LineRef, body: String],
    forge::resolve_thread: Act [phone: state, repo: Option<String>, iid: u64, discussion: String, resolved: bool],
    forge::set_draft: Act [phone: state, repo: Option<String>, iid: u64, draft: bool],
    forge::set_mr_people: Act [phone: state, repo: Option<String>, iid: u64, reviewers: bool, usernames: Vec<String>],
    forge::rebase_mr: Act [phone: state, repo: Option<String>, iid: u64],
    forge::retry_pipeline: Act [phone: state, id: u64],
    forge::retry_job: Act [phone: state, project: Option<String>, job: u64],
    forge::cancel_pipeline: Act [phone: state, id: u64],
    forge::play_renovate: Act [phone: state],
    infra::rescan_vulnerabilities: Act [demo: own],
    // Agents: their permission rules still apply to what they do.
    agents::agent_spawn: Act,
    agents::agent_prepare: Act,
    // A hidden agent, its prompt Prism's (the session's own prompts, as data), in a folder Prism shows.
    agents::quick_action_draft: Act [phone: state, source: crate::agents::session_source::Source, repo: String, name: String],
    agents::draft_stop: Act [phone: state, id: String],
    agents::agent_start: Act,
    agents::agent_resume: Act [phone: state, id: String],
    agents::agent_reopen: Act [phone: state, repo: String],
    agents::agent_restart: Act [phone: state, id: String],
    agents::agent_interrupt: Act [phone: state, id: String],
    agents::agent_key: Act [phone: state, id: String, key: String],
    agents::agent_allow: Act [phone: state, id: String],
    // Its question's choices; with your words, typing (the remote's server: `[remote] typing`, passkey).
    agents::agent_answer: Act [phone: state, id: String, call: String, answers: Vec<crate::agents::question::Answer>],
    // Its plan approved; with your words, typing (as its question's).
    agents::agent_plan_answer: Act [phone: state, id: String, call: String, how: crate::agents::plan::How, text: Option<String>],
    // Free text sent to an agent, Enter included: from a phone, `agent_type` (typing, its passkey).
    agents::agent_prompt: Never [demo: refused],
    agents::agent_send: Never [demo: refused],
    // A quick answer: the config's text, filled and checked on the computer (no free text).
    agents::agent_quick_answer: Act [phone: state, id: String, label: String, text: String],
    // Trusting a folder lets its settings, hooks and MCP servers run: from a phone, its passkey.
    agents::agent_trust: Critical [phone: state, id: String, kind: crate::state::GateKind, parent: Option<bool>],
    agents::agent_refuse_gate: Act [phone: state, id: String, kind: crate::state::GateKind],
    // The agent's own suggestion, checked on its screen (no free text).
    agents::agent_send_suggestion: Act [phone: state, id: String, text: String],
    agents::agent_compact: Act [phone: state, id: String],
    // Its transcript stays: the conversation can be resumed.
    agents::agent_clear: Act [phone: state, id: String],
    agents::agent_attach_file: Act [phone: state, id: Option<String>, name: String, data: String; sync],
    agents::agent_leave_copy_mode: Act [phone: state, id: String],
    agents::agent_open_mr: Act [phone: state, id: String],
    // No hook runs; never on a protected branch.
    agents::agent_git_commit: Act [phone: state, id: String, message: String],
    agents::agent_git_push: Act [phone: state, id: String],
    agents::agent_git_sync: Act [phone: state, id: String],
    // A chain's held step skipped, a chain stopped or hidden.
    agents::chain_order: Act [phone: state, id: String, action: String; sync],
    // A task flagged in passing: kept, dismissed, said launched, made a ticket; chained after its
    // agent, it launches one on the computer later (the passkey, as launching one).
    agents::aside_launch: Read [phone: state, id: u64, from_branch: bool],
    agents::aside_action: Act [phone: state, id: u64, action: String, why: Option<String>, target: Option<String>; sync],
    agents::aside_then: Critical [phone: state, id: u64, from_branch: bool; sync],
    agents::aside_ticket: Act [phone: state, id: u64],
    // Routines: a row out of "To review"; a run again, or a routine switched, launches an agent
    // on the computer (the passkey); trusting one, removing one, and the builtins' inputs: never.
    agents::routine_archive: Act [phone: state, runs: Vec<String>; sync],
    agents::routine_rerun: Critical [phone: state, run: String; sync],
    agents::routine_run: Critical [phone: state, name: String; sync],
    agents::routine_enable: Critical [phone: state, name: String, on: bool],
    agents::routine_trust: Never,
    agents::routine_remove: Never,
    agents::agent_rename: Act [phone: state, id: String, title: String; sync],
    agents::agent_set_tags: Act [phone: state, id: String, tags: Vec<String>; sync],
    agents::agent_set_category: Act [phone: state, id: String, category: Option<String>; sync],
    agents::agent_link_ticket: Act [phone: state, id: String, ticket: Option<String>; sync],
    agents::agent_link_mr: Act [phone: state, id: String, mr: Option<crate::state::MrLink>; sync],
    agents::agent_relabel: Act [phone: state, kind: crate::agents::Label, from: String, to: String; sync],
    agents::worktree_prune: Act [phone: state],
    // The order of the merge queue: nothing merges sooner than it would have.
    forge::set_merge_priority: Act [phone: state, repo: Option<String>, iid: u64, priority: Option<u32>],
    // A merge, or what ends in one without another gesture.
    forge::merge_mr: Critical [phone: state, repo: Option<String>, iid: u64],
    forge::set_merge_queue: Critical [phone: state, repo: Option<String>, iid: u64, on: bool],
    forge::set_auto_merge: Critical [phone: state, repo: Option<String>, iid: u64, enable: bool],
    // A label can be the merge queue's (set_merge_queue) or one a repo's CI runs or deploys on;
    // a new pipeline runs with the project's secrets on a branch whose jobs never ran (label-gated CI).
    forge::set_mr_labels: Critical [phone: state, repo: Option<String>, iid: u64, labels: Vec<String>],
    forge::run_mr_pipeline: Critical [phone: state, repo: Option<String>, iid: u64],
    // Manual jobs are often deployments, which Prism cannot tell apart.
    forge::play_job: Critical [phone: state, id: u64],
    // Typing in a terminal, running a command, ending an agent, removing a worktree.
    agents::agent_write: Critical [demo: own],
    // A phone's answer box: its passkey, as taking the terminal (the remote's server).
    agents::agent_type: Critical [phone: state, id: String, text: String],
    // Another CLI takes the agent on, its relay's words typed to it: as the answer box (passkey).
    agents::agent_switch: Critical [phone: state, id: String, switch: crate::agents::switch::Switch],
    agents::agent_resize: Critical [demo: own],
    // An idle shell gives nothing by itself: typing in it is taking it (`[remote] typing`, passkey).
    agents::agent_shell: Act [demo: own] [phone: state, id: String, n: Option<u32>],
    agents::agent_close_shell: Critical [phone: state, id: String, n: u32],
    // From a phone, only a task's line as detected in the agent's folder (Tasks), never a free one.
    agents::agent_run: Critical,
    // Over origin's branch: what it had and this folder lacks is lost.
    agents::agent_git_force_push: Critical [phone: state, id: String],
    // A phone writes a file as it types: `[remote] typing`, the passkey (the remote's server).
    agents::agent_file_save: Critical [phone: state, id: String, path: String, original: String, text: String; sync],
    // A held step started: a task the agent may have changed runs.
    agents::chain_go: Critical [phone: state, id: String; sync],
    agents::agent_stop: Critical [phone: state, id: String],
    agents::agent_close: Critical [phone: state, id: String],
    agents::worktree_remove: Critical,
    agents::agent_remove_worktree: Critical [phone: state, id: String],
}
