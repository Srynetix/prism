import { ALL_BLOCKS, VIEW_COLORS, VIEW_ICONS } from './views'
import { PROVIDER_SPECS } from './bindings/provider-specs'
import { BLOCK_DEFS } from './blocks'
import { isMac, keys } from './platform'
import { forgeName, mrWord, onGithub } from './forge'
import { t, tc } from './i18n.svelte'
import { ALERT_RULES } from './alert-bands'
/**
 * The Settings sections described like config.toml: one per table, its keys in the order of
 * config.example.toml, each with its input type. Settings.svelte displays them all the
 * same way; only Appearance and Console have a layout of their own. Where each one shows
 * in the tree of Settings: settings-tree.ts.
 *
 * Every key applies as it is saved: the collectors start again from the new config when what
 * they read of it changed (bootstrap.rs); only a pack's block that does not reload waits for a
 * restart (`Snapshot.waiting_restart`).
 */
export type Kind = 'text' | 'longtext' | 'path' | 'number' | 'toggle' | 'list' | 'select' | 'checks' | 'table'
/** Choice lists read from the machine. */
export type Source = 'kube' | 'apps' | 'envs' | 'stages' | 'calendars' | 'kinds' | `models:${AgentCli}` | `efforts:${AgentCli}`
/**
 * Live check of a value (checks.rs); `kube_namespace` takes its context from the
 * neighboring key `context`.
 */
export type CheckSpec = 'gitlab_project' | 'kube_context' | 'git_repo' | 'directory' | 'gcloud' | 'agent_tools' | 'agent_cli' | { kind: 'kube_namespace'; context: string } | { kind: 'agent_bin'; cli: AgentCli }

/** The agent CLIs, as `[agents] provider` names them; each has its `[agents.<cli>]` table. */
export const AGENT_CLIS = ['claude', 'command-code', 'opencode', 'codex'] as const
export type AgentCli = (typeof AGENT_CLIS)[number]

export interface Column {
  key: string
  label: string
  kind: Exclude<Kind, 'checks' | 'table'>
  placeholder?: string
  /** Empty: the key is removed from the row. */
  optional?: boolean
  options?: Source | string[]
  width?: string
  check?: CheckSpec
}

export interface FieldSpec {
  key: string
  label: string
  hint?: string
  kind: Kind
  unit?: string
  /** Step of a number; absent: integer. */
  step?: number
  placeholder?: string
  optional?: boolean
  options?: Source | string[]
  /** `checks`: the words shown for each option, by value; absent, the value itself. */
  labels?: Record<string, string>
  columns?: Column[]
  /** Value of a new table row. */
  blank?: Record<string, unknown>
  check?: CheckSpec
  /** `views`: a button opens the pages wizard and its templates. */
  seed?: 'views'
}

export interface SectionSpec {
  id: string
  label: string
  /** As in config.toml: `[gitlab]`, `[[repos]]`. */
  table: string
  /** Path of the table; a `[[…]]` section is a single key of table type. */
  path: string[]
  /** A section of its own layout without fields: the keys it edits (`forge.bare_mr_repo`), for its unsaved changes. */
  keys?: string[]
  /** The tracker whose API token (or Google's client secret) the section stores in the keychain, above its keys. */
  token?: 'clickup' | 'jira' | 'google'
  /** The account the section signs in to (OAuth), above its keys. */
  signin?: 'mail' | 'google'
  /** The forge CLI whose own login the section shows, above its keys. */
  login?: 'gitlab' | 'github'
  intro?: string
  /** Collector reading this section: its last state is shown at the top. */
  source?: string
  fields: FieldSpec[]
}

/** Examples of a CLI's model names, for its model field. */
const MODEL_EXAMPLES: Record<AgentCli, string> = { claude: 'opus, sonnet, claude-opus-5-5', codex: 'gpt-6.1-sol', 'command-code': 'deepseek/deepseek-v4-pro', opencode: 'anthropic/claude-sonnet-5-5' }

/** How an agent CLI is launched (`[agents.<cli>]`): its executable, model and extra arguments. */
const cliFields = (cli: AgentCli): FieldSpec[] => [
  { key: 'bin', label: t('Binary'), hint: t('The executable: a name in the PATH, or a path (~ allowed). Empty: {program} from the PATH. Used by new agents and on resume.', { program: cliProgram(cli) }), kind: 'text', optional: true, placeholder: cliProgram(cli), check: { kind: 'agent_bin', cli } },
  { key: 'model', label: t('Model'), hint: t('Run by new agents and on resume (--model), in the CLI’s own names ({examples}): those it offers are suggested, any other may be typed. The launcher can pick another for one agent. Empty: the CLI’s default.', { examples: MODEL_EXAMPLES[cli] }), kind: 'text', optional: true, placeholder: t('the CLI’s default'), options: `models:${cli}` },
  ...(PROVIDER_SPECS[cli].takes_effort
    ? [{ key: 'effort', label: t('Effort'), hint: t('The reasoning effort new agents and resumes run with ({option}), in the CLI’s own levels: those it offers are suggested. The launcher and an agent’s header can pick another for one agent. Empty: the CLI’s default.', { option: cli === 'claude' ? '--effort' : 'model_reasoning_effort' }), kind: 'text' as const, optional: true, placeholder: t('the CLI’s default'), options: `efforts:${cli}` as const }]
    : []),
  { key: 'args', label: t('Extra arguments'), hint: t('Added to every launch and resume, right after the executable, one per line (--verbose). A running agent keeps its own until it restarts.'), kind: 'list' },
]
/** A CLI's usual executable (`Provider::program`). */
const cliProgram = (cli: AgentCli) => PROVIDER_SPECS[cli].program

/** `none`, then the blocks declaring `what` (`BlockDef.provides`), by their Rust id. */
const providers = (what: 'stages' | 'images') => ['none', ...BLOCK_DEFS.filter((b) => b.provides?.includes(what)).map((b) => b.source ?? b.id)]

/** The alert rules, for the bands' checkboxes. */
const RULE_IDS = ALERT_RULES.map((r) => r.id)
const RULE_LABELS = Object.fromEntries(ALERT_RULES.map((r) => [r.id, r.label]))

const STATUS_HINT = t('Statuses as written in the boards (ClickUp), or lane names (Obsidian).')

export const SCHEMA: SectionSpec[] = [
  {
    id: 'alerts', label: t('Alerts'), table: '[alerts]', path: ['alerts'],
    intro: t('Thresholds of the Alerts block’s lines and of the badges, one per rule.'),
    fields: [
      { key: 'behind_commits', label: t('Branch behind'), hint: t('A branch further behind its base is flagged (worktrees, agents).'), kind: 'number', unit: 'commits' },
      { key: 'pending_job_minutes', label: t('Job waiting'), hint: t('A CI job waiting longer for a runner: runners are short.'), kind: 'number', unit: 'minutes' },
      { key: 'review_wait_hours', label: t('Review waiting'), hint: t('A review waiting for you longer is flagged.'), kind: 'number', unit: 'hours' },
      { key: 'doing_idle_days', label: t('Ticket idle'), hint: t('A ticket in progress with no branch nor MR for longer is flagged.'), kind: 'number', unit: 'days' },
      { key: 'waiting_session_hours', label: t('Session waiting'), hint: t('An agent session waiting for you (a question, a permission, a plan) is flagged this long, then left aside.'), kind: 'number', unit: 'hours' },
      { key: 'your_turn_minutes', label: t('Your turn'), hint: t('An agent whose turn is just over, asking nothing, is flagged this long only: you may well stop there. 0: never.'), kind: 'number', unit: 'minutes' },
      { key: 'argo_out_of_sync_minutes', label: t('ArgoCD out of sync'), hint: t('An application out of sync for longer is flagged.'), kind: 'number', unit: 'minutes' },
      { key: 'meeting_lead_minutes', label: t('Meeting reminder'), hint: t('Notification before a meeting.'), kind: 'number', unit: 'minutes' },
      { key: 'worktree_in_use_hours', label: t('Worktree in use'), hint: t('Activity more recent than this: the worktree is in use.'), kind: 'number', unit: 'hours' },
      { key: 'large_mr_lines', label: t('Large MR'), hint: t('An MR changing more lines (added + removed) is flagged as large.'), kind: 'number', unit: 'lines' },
      { key: 'no_reviewer_minutes', label: t('MR without reviewer'), hint: t('Your MR, out of draft, with no reviewer this long after it was opened is flagged. 0: never.'), kind: 'number', unit: 'minutes' },
      { key: 'open_threads', label: t('Unresolved threads'), hint: t('Your open MR with review threads left unresolved is flagged, with an agent to apply the feedback.'), kind: 'toggle' },
      { key: 'band_now', label: tc('alerts', 'Now'), hint: t('Alerts shown first: someone waits on you, a person or an agent. A rule checked nowhere goes by its severity: Now when broken, When you can for information, Today otherwise.'), kind: 'checks', options: RULE_IDS, labels: RULE_LABELS },
      { key: 'band_later', label: t('When you can'), hint: t('Alerts shown last: nothing presses.'), kind: 'checks', options: RULE_IDS, labels: RULE_LABELS },
      { key: 'band_hidden', label: t('Hidden'), hint: t('Alerts never shown nor counted, in the block, the status bar and the menu bar.'), kind: 'checks', options: RULE_IDS, labels: RULE_LABELS },
      { key: 'escalate_hours', label: t('Escalate after'), hint: t('A Today alert held longer goes up to Now: a review asked yesterday no longer waits politely. 0: never.'), kind: 'number', unit: 'hours' },
      { key: 'group', label: t('Group by MR or ticket'), hint: t('The alerts of one MR or one ticket on a single line, with the action that unblocks the others.'), kind: 'toggle' },
    ],
  },
  {
    id: 'digest', label: t('Notifications'), table: '[alerts]', path: ['alerts'],
    intro: t('When Prism interrupts you. Live: one notification per event as it appears. Digest: a summary at set times, the urgent kinds still at once. Meeting reminders and your own reminders always go out on time.'),
    fields: [
      { key: 'digest', label: t('Digest'), hint: t('Off: one notification per event (live). On: events held, then one summary at the digest times; the status bar shows what waits.'), kind: 'toggle' },
      { key: 'digest_times', label: t('Digest at'), hint: t('When the summary goes out, HH:MM in local time.'), kind: 'list' },
      { key: 'digest_urgent', label: t('Still at once'), hint: t('What notifies at once in digest mode: infra (pods, runners), deploy (ArgoCD), agent (a permission asked), pipeline (a protected branch failed), review (asked of you), data, block (Docker, dev servers, status pages…), routine (a routine delivered to the Digest).'), kind: 'checks', options: ['infra', 'deploy', 'agent', 'pipeline', 'review', 'data', 'block', 'routine'] },
      { key: 'quiet_hours', label: t('Quiet hours'), hint: t('No notification in this span, digest or not, urgent included: 19:00-08:00. Empty: none.'), kind: 'text', optional: true, placeholder: '19:00-08:00' },
      { key: 'focus_events', label: t('Focus events'), hint: t('During a calendar event whose title holds this, nothing notifies, urgent included, and the digest waits for its end. Zen mode does the same. Empty: never.'), kind: 'text', optional: true, placeholder: 'Focus' },
    ],
  },
  {
    id: 'views', label: t('Pages'), table: '[[views]]', path: [],
    intro: t('Pages of the rail, in this order. Empty: one empty Dashboard. Edit layout writes its blocks here, with the page\'s Id, so that a renamed page keeps its sizes.'),
    fields: [
      {
        key: 'views', label: t('Pages'), kind: 'table', blank: { name: '', columns: [] }, seed: 'views',
        hint: t('Blocks: one chip per column, its blocks separated by spaces (todo sessions notes); a --- chip starts a band below, a row of its own columns (one block alone in it spans the width). Blocks: {blocks}. Id: keeps the layout of a renamed page; empty, taken from the name.', { blocks: ALL_BLOCKS.join(', ') }),
        columns: [
          { key: 'name', label: t('Name'), kind: 'text', width: '120px' },
          { key: 'icon', label: t('Icon'), kind: 'select', options: VIEW_ICONS, optional: true, placeholder: 'dashboard', width: '110px' },
          { key: 'color', label: t('Color'), kind: 'select', options: Object.keys(VIEW_COLORS), optional: true, placeholder: 'accent', width: '100px' },
          { key: 'columns', label: t('Blocks by column'), kind: 'list', optional: true },
          { key: 'id', label: t('Id'), kind: 'text', optional: true, placeholder: t('from name'), width: '100px' },
        ],
      },
    ],
  },
  {
    id: 'openwith', label: t('Open with'), table: '[open_with]', path: ['open_with'],
    intro: t('Apps used by the Repos menu of the status bar.'),
    fields: [
      { key: 'editor', label: t('Editor'), hint: t('Opens a repository. Empty: no Editor button.'), kind: 'select', options: 'apps', optional: true, placeholder: t('none') },
      { key: 'terminal', label: t('Terminal'), hint: t('Opens a repository, or an agent (Open in terminal), in a terminal.'), kind: 'select', options: 'apps', optional: true, placeholder: 'Terminal' },
    ],
  },
  {
    id: 'palette', label: t('Palette'), table: '[palette]', path: ['palette'],
    intro: t('One field to go to anything Prism shows — a page, a block, an agent, an MR, a ticket, a pipeline, an environment, a pod, a Settings page or key, a guide — and to run its commands, the console’s included (note …, agent …, remind …: their words after them). ↵ goes or runs, ⌘↵ opens the chosen line’s actions (an MR’s or a ticket’s menu, an agent’s quick answers), ⇥ completes a command. On the phone too, from the search button. In an agent’s terminal, ⌘K stays the terminal’s (Agents › Agent › Keys kept by the terminal).'),
    fields: [
      { key: 'enabled', label: t('Palette'), kind: 'toggle' },
      { key: 'key', label: t('Shortcut'), hint: t('A combination (Cmd+K, Cmd+P, Ctrl+Space). Applied at once.'), kind: 'text', placeholder: 'Cmd+K' },
      { key: 'tracker_search', label: t('Search the tracker'), hint: t('From 3 characters, the tickets Prism does not show are searched in the tracker too (closed, another board): any ticket opens. ClickUp has no search: the tickets of the followed spaces are read once, closed ones included, and kept 10 minutes.'), kind: 'toggle' },
    ],
  },
  {
    id: 'cleanup', label: t('Cleanup'), table: '[cleanup]', path: ['cleanup'],
    fields: [
      { key: 'hide_off', label: t('Hide turned-off features in Settings'), hint: t('A feature whose switch is off leaves the tree on the left; a search still finds it.'), kind: 'toggle' },
    ],
  },
  {
    id: 'files', label: t('Files'), table: '[files]', path: ['files'],
    intro: t('The editor of an agent’s Files tab. ⌘P opens a file by its name (:42 a line, @ a symbol, # text in every file), ⌘⇧F searches the whole folder, ⌘F the file, ⌘S saves — once the agent’s turn is over. Lines selected go to the agent (Ask agent, Add to prompt) or to a note.'),
    fields: [
      { key: 'vim', label: t('Vim keys'), hint: t('Normal, insert and visual modes; :w saves.'), kind: 'toggle' },
      { key: 'wrap', label: t('Wrap long lines'), hint: t('Off: they scroll sideways.'), kind: 'toggle' },
      { key: 'font_size', label: t('Font size'), kind: 'number', unit: 'px', step: 0.5, placeholder: '11.5' },
      { key: 'live_reload', label: t('Live reload'), hint: t('A file open in the editor that the agent changes is read again, where you were kept. Edited by you and not saved: a banner lets you compare, take its version or keep yours.'), kind: 'toggle' },
      { key: 'search_max', label: t('Search results'), hint: t('Lines found at most by the search in files.'), kind: 'number', unit: 'lines', placeholder: '2000' },
    ],
  },
  {
    id: 'agents-quick', label: t('Quick answers'), table: '[agents.quick_answers]', path: ['agents', 'quick_answers'],
    intro: t('One click answers an agent at its prompt: in its view (the line under the terminal), the panes side by side, the Agents block, its alert and the phone. Nothing while it works or asks for a permission. ⇧-click types the text into its input instead, to finish it there. A text may name the agent’s {ticket}, {branch}, {mr}, {title} or {folder}: an answer whose variable the agent lacks is not offered to it.'),
    fields: [
      { key: 'enabled', label: t('Quick answers'), kind: 'toggle' },
      {
        key: 'answers', label: t('Answers'), kind: 'table', blank: { label: '', text: '', submit: true },
        hint: t('In this order; the first three where room is short. Empty: Prism’s (OK, Do it, Continue, No wait, Commit), in the prompt language (Settings › Agents › Agent). Not submitted: typed into the agent’s input, to finish.'),
        columns: [
          { key: 'label', label: t('Label'), kind: 'text', width: '120px' },
          { key: 'text', label: t('Text sent'), kind: 'text', placeholder: t('Go ahead, do it.') },
          { key: 'submit', label: t('Submitted'), kind: 'toggle', width: '80px' },
        ],
      },
    ],
  },
  {
    id: 'agents-overlaps', label: t('Overlaps'), table: '[agents.overlaps]', path: ['agents', 'overlaps'],
    intro: t('Two agents (or worktrees) of a repository changing the same lines, or the same file, told before the merge rather than at the rebase. Each one’s changes against the merge-base of its base (its commits, its uncommitted changes, its new files) are compared with the others’ on the same base; read again as files are written, on a commit, a pull, an agent’s turn over. A chip on both agents (and their MRs, a worktree without agent), a panel of both sides’ hunks, an alert per pair.'),
    fields: [
      { key: 'enabled', label: t('Overlaps'), kind: 'toggle' },
      { key: 'level', label: t('Chip from'), hint: t('lines: only hunks on the same lines (a merge conflict all but certain); file: the same file too, other lines (a merge to check).'), kind: 'select', options: ['lines', 'file'] },
      { key: 'alert', label: t('Alert from'), hint: t('One alert per pair, gone with the overlap; off: chips only.'), kind: 'select', options: ['lines', 'file', 'off'] },
      { key: 'ignore', label: t('Files ignored'), hint: t('Never compared, one glob per line: a name without a folder (*.lock), or a path (**/bindings/**). Lockfiles and generated files are computed again, not merged.'), kind: 'list' },
    ],
  },
  {
    id: 'agents-aside', label: t('In passing'), table: '[agents.aside]', path: ['agents', 'aside'],
    intro: t('What an agent sees while it works but is out of its task (dead code, a stale doc, a missing test, the same bug elsewhere): it flags it with suggest_task, a tool of Prism’s MCP server given to the agents Prism launches (Claude Code, Codex, OpenCode), rather than in a sentence of its summary. A card in its Chat, a ↗ chip on the agent, the « In passing » list of the Agents view (and the aside block): launch it in another agent, after this one, as a ticket, later, or dismiss it — the agent is told at its next step.'),
    fields: [
      { key: 'enabled', label: t('In passing'), hint: t('The suggest_task and dismiss_task tools given to the agents Prism launches, from their next launch or resume.'), kind: 'toggle' },
      { key: 'desktop', label: t('Claude Desktop sessions'), hint: t('Their spawn_task calls, read from their transcripts, fill the list too (read only: Desktop keeps its own chip).'), kind: 'toggle' },
      { key: 'default_action', label: t('Main button'), hint: t('launch: the launcher, prefilled; then: after its agent, in its chain; ticket: a ticket of the tracker.'), kind: 'select', options: ['launch', 'then', 'ticket'] },
      { key: 'worktree', label: t('Launch in a worktree'), hint: t('Off: in the folder of the agent that saw it.'), kind: 'toggle' },
      { key: 'stale_days', label: t('Older after'), hint: t('Open lines older than this fold under Older; nothing is removed on its own. 0: never.'), kind: 'number', unit: 'days' },
      { key: 'statusbar', label: t('Count in the status bar'), hint: t('The open ones; never an alert, never urgent.'), kind: 'toggle' },
    ],
  },
  {
    id: 'agents-dictation', label: t('Dictation'), table: '[dictation]', path: ['dictation'],
    intro: t('A prompt spoken rather than typed, at the computer: the microphone beside an agent’s Chat box, a plan’s comments, a question’s own words and the launcher’s prompt. Held, it records while held; clicked, it starts and the next click stops. The text goes in at the cursor, never sent on its own: you read it over and send it. Esc cancels. Transcribed on this computer only, never online: macOS’s speech recognition on the Mac, or whisper.cpp built into Prism (its model downloaded below). The audio stays in memory and is dropped once read.'),
    fields: [
      { key: 'enabled', label: t('Dictation'), hint: t('The microphone button and the shortcut.'), kind: 'toggle' },
      { key: 'engine', label: t('Engine'), hint: t('auto: macOS’s speech recognition where it recognizes the language on this Mac, whisper otherwise (Linux: whisper). system: macOS’s, forced on the device. whisper: whisper.cpp, the same everywhere, better on technical words.'), kind: 'select', options: ['auto', 'system', 'whisper'] },
      { key: 'whisper_model', label: t('whisper model'), hint: t('base (142 MB) by default; small is better on technical words, three times slower. Downloaded below, when needed.'), kind: 'select', options: ['tiny', 'base', 'small', 'medium'] },
      { key: 'language', label: t('Language'), hint: t('auto: whisper detects it, macOS’s recognition takes the prompts’ language ([agents] prompt_language). Otherwise a code: fr, en…'), kind: 'text', options: ['auto', 'fr', 'en', 'de', 'es', 'it'] },
      { key: 'shortcut', label: t('Shortcut'), hint: isMac ? t('Held to dictate while a field that takes dictation has the focus: Fn (🌐), or a combination (Ctrl+Shift+Space). Fn pressed with another key (Fn+←) is not a dictation. Empty: the button only.') : t('Held to dictate while a field that takes dictation has the focus (Ctrl+Shift+Space). Empty: the button only.'), kind: 'text', optional: true, placeholder: isMac ? 'Fn' : 'Ctrl+Shift+Space' },
      { key: 'toggle_on_click', label: t('Click to start and stop'), hint: t('A click on the microphone starts, another stops: for long dictations. Off: it records only while held.'), kind: 'toggle' },
      { key: 'max_seconds', label: t('Longest recording'), hint: t('A recording forgotten stops by itself and is transcribed.'), kind: 'number', unit: 'seconds' },
      { key: 'hints', label: t('Words to expect'), hint: t('The repository’s, branch’s, ticket’s and the agent’s files’ names, given to the engine to recognize them.'), kind: 'toggle' },
      { key: 'input_device', label: t('Microphone'), hint: t('By name, as listed below. Empty: the system’s.'), kind: 'text', optional: true, placeholder: t('the system’s') },
    ],
  },
  {
    id: 'mcp', label: t('MCP server'), table: '[mcp]', path: ['mcp'],
    intro: t('prism mcp: an MCP server an agent CLI starts. It reads Prism’s help, the commented reference, the schema of every table, your config merged with its packs (no secret: tokens live in the keychain) and Problems; it tests a custom block in a sandbox. A change it makes is a proposal: Prism shows its diff and you apply it, refuse it or edit it. It reaches this Prism through a local socket of the profile; Prism stopped, only the docs and the schema answer.'),
    fields: [
      { key: 'enabled', label: t('MCP server'), hint: t('Off: prism mcp answers nothing.'), kind: 'toggle' },
      { key: 'proposals', label: t('Proposals'), hint: t('Agents may propose changes, applied only by you. Off: strictly read-only, the agent gives the TOML to copy.'), kind: 'toggle' },
    ],
  },
  {
    id: 'agents-mcp', label: t('For the agents Prism launches'), table: '[agents]', path: ['agents'],
    fields: [
      { key: 'prism_mcp', label: t('Prism MCP for every agent'), hint: t('Adds the MCP server and the “prism” skill to each agent Prism launches (Claude Code, Codex, OpenCode). Off: only the agents of “Configure Prism”.'), kind: 'toggle' },
    ],
  },
  {
    id: 'agents-actions', label: t('Quick actions'), table: '[[agents.quick_actions]]', path: ['agents'],
    intro: t('The same job asked again and again of an agent on a repository (bring its main branch up to date, run a scan…): one button each on its project in the Agents view (grouped by repository) and on the phone, and a line in the palette. A click launches the agent at once; ⇧-click, or ⌘↵ in the palette, opens the launcher prefilled, to change it first.'),
    fields: [
      {
        key: 'quick_actions', label: t('Quick actions'), kind: 'table', blank: { name: '', prompt: '', repos: [], worktree: false },
        hint: t('The prompt may name {repo} and {base} (the repository’s default branch). Repositories empty: offered on every one. Worktree: in a new worktree where the repository makes them; off, in its clone as it is. Mode, Model, Effort empty: those of the CLI’s settings.'),
        columns: [
          { key: 'name', label: t('Name'), kind: 'text', width: '130px' },
          { key: 'prompt', label: t('Prompt'), kind: 'longtext', placeholder: t('Update {base} from upstream, then scan with Trivy.') },
          { key: 'repos', label: t('Repositories'), kind: 'list', optional: true, width: '130px' },
          { key: 'worktree', label: t('Worktree'), kind: 'toggle', width: '70px' },
          { key: 'mode', label: t('Mode'), kind: 'select', options: ['default', 'acceptEdits', 'plan'], optional: true, width: '110px' },
          { key: 'kind', label: t('Kind'), kind: 'select', options: 'kinds', optional: true, width: '110px' },
          { key: 'model', label: t('Model'), kind: 'text', optional: true, placeholder: 'opus', width: '100px' },
          { key: 'effort', label: t('Effort'), kind: 'text', optional: true, placeholder: 'high', width: '80px' },
        ],
      },
    ],
  },
  {
    id: 'agents', label: t('Agents'), table: '[agents]', path: ['agents'],
    intro: t('Claude Code, Command Code, OpenCode or Codex sessions in Prism terminals, kept alive by tmux when Prism quits. The agent actions of the menus open their launcher. What every CLI shares is here; each CLI’s own keys (binary, arguments; Claude Desktop for Claude Code) are on its page below.'),
    fields: [
      { key: 'enabled', label: t('Agents view'), hint: t('Off: no Agents view, the agent actions open Claude Desktop.'), kind: 'toggle' },
      { key: 'provider', label: t('Agent CLI'), hint: t('Run by new agents (claude: Claude Code, command-code: Command Code, opencode: OpenCode, codex: Codex) unless their repository names its own (Repositories › Agent), and whose plan usage the status bar shows (none for OpenCode). A running agent keeps its own.'), kind: 'select', options: [...AGENT_CLIS], check: 'agent_tools' },
      { key: 'target', label: t('Agent actions open'), hint: keys(t('agent: the launcher of the Agents view (⇧-click launches at once). desktop: a Claude Desktop session, as before.')), kind: 'select', options: ['agent', 'desktop'] },
      { key: 'worktrees_dir', label: t('Worktrees folder'), hint: t('Where new agent worktrees go. Empty: next to their repository (<repo>.worktrees).'), kind: 'path', optional: true, placeholder: '<repo>.worktrees', check: 'directory' },
      { key: 'branch_prefix', label: t('Branch prefix'), hint: t('Branch of a new worktree: prefix + its name.'), kind: 'text', placeholder: 'agent/' },
      { key: 'permission_mode', label: t('Permissions'), hint: t('Proposed by the launcher, in each CLI’s words (Codex: plan is its read-only sandbox, acceptEdits its workspace-write one). default: the mode of your agent CLI settings.'), kind: 'select', options: ['default', 'acceptEdits', 'plan'] },
      { key: 'font_size', label: t('Terminal text size'), hint: keys(t('Text of the agents terminals, applied to the open ones at once. In the Agents view: ⌘+ / ⌘- to adjust, ⌘0 to reset.')), kind: 'number', unit: 'pt', step: 0.5 },
      { key: 'chat_font_size', label: t('Chat text size'), hint: keys(t('Text of an agent’s Chat on the computer (its conversation, the default 13.5), its steps and cards scaled with it; applied at once. In the Agents view, an agent shown as its Chat: ⌘+ / ⌘- to adjust, ⌘0 to reset. The phone keeps its own size.')), kind: 'number', unit: 'pt', step: 0.5 },
      { key: 'scrollback', label: t('Scrollback'), hint: t('Lines kept by an agent terminal, on top of tmux history.'), kind: 'number', unit: 'lines' },
      {
        key: 'kinds', label: t('Kinds'), kind: 'table', blank: { name: '', when: '' },
        hint: t('What an agent does, one each: a coloured badge in its header and the list. Guessed at launch from the command its prompt starts with, changed in its header. Grouped by kind, the Agents view lists them in this order; their colours follow it. Also editable in the Agents view › Manage, where a rename carries the agents along.'),
        columns: [
          { key: 'name', label: t('Name'), kind: 'text', width: '140px' },
          { key: 'when', label: t('Its commands'), kind: 'text', placeholder: 'refine, /review' },
        ],
      },
      { key: 'tags', label: t('Tags'), hint: t('Suggested when tagging an agent (its header), besides the tags already in use; any other may be typed. Grouped by tag, the Agents view lists the suggested ones first, in this order. Also editable in the Agents view › Manage.'), kind: 'list' },
      { key: 'prompt_language', label: t('Prompt language'), hint: t('Language of the prompts the agent actions write (validate, fix the pipeline, investigate…), and so of the agent’s answers.'), kind: 'select', options: ['fr', 'en'] },
      { key: 'terminal_keys', label: t('Keys kept by the terminal'), hint: t('Prism’s shortcuts an agent’s terminal keeps for itself: pressed there, Prism leaves them alone (Cmd+K clears the terminal; another goes to the program). The others go up to Prism, as everywhere. Cmd+K by default; add Cmd+N, Cmd+1… to keep them too.'), kind: 'list' },
      { key: 'view', label: t('Agents shown as'), hint: t('How an agent reads on the computer: chat, its conversation from its transcript (its questions as cards, a box to write to it), its terminal running under it; terminal, its terminal. Claude Code and Codex only: the others always show their terminal. Terminal | Chat in an agent’s header changes it for that agent alone.'), kind: 'select', options: ['chat', 'terminal'] },
      { key: 'fold_steps', label: t('Fold its steps'), hint: t('In an agent’s Chat, on the computer and the phone: the calls and thoughts between two of its answers on one line, with their counts (files edited, commands, failures in red); a click unfolds them. The turn under way stays unfolded, its last calls shown. Off: one line per call.'), kind: 'toggle' },
      { key: 'comment_answers', label: t('Comment its answers'), hint: t('In an agent’s Chat, on the computer and the phone: a long answer of its (several passages, or long) gets a + beside each passage, as its plans do — your comments, each under its passage, go to it as one prompt, each passage quoted. Its plans (plan mode) are always commented on. Off: its answers read only.'), kind: 'toggle' },
      { key: 'suggest_switch', label: t('Offer another CLI at the limit'), hint: t('An agent whose CLI’s plan is used up: a banner in its view and an alert offer the least used of the other CLIs found on the computer to continue with (Continue with…). Off: nothing offered; its CLI chip and ⋯ menu still hand it over.'), kind: 'toggle' },
      { key: 'handoff', label: t('Relay'), hint: t('How the CLI an agent continues with is told what was done: prism, a relay Prism writes from its transcript and git (its task, plan, last turns, files changed, commits), free and at once, even at its limit; agent, a summary the former agent writes when asked (one more turn of it, not at its limit); task, its first prompt and ticket only. Picked again in the window each time.'), kind: 'select', options: ['prism', 'agent', 'task'] },
      { key: 'handoff_turns', label: t('Turns in the relay'), hint: t('The last turns Prism’s relay recalls: your words and the end of each answer.'), kind: 'number', unit: 'turns' },
      { key: 'handoff_review', label: t('Read the relay first'), hint: t('Continue with… opens a window showing the relay, to edit, before it goes. Off: from its CLI chip, the palette or the ⋯ menu, the relay goes as written, in one click (the agent’s own summary is always waited for there).'), kind: 'toggle' },
      { key: 'header_meta', label: t('Header details'), hint: t('The second line of an agent’s header on the computer (ticket, MR, branch, kind, tags): read, a quiet line to read, changed behind Edit; edit, their fields always there.'), kind: 'select', options: ['read', 'edit'] },
      { key: 'tasks_depth', label: t('Tasks depth'), hint: t('How deep the Tasks panel looks for justfiles, Taskfiles and package.json under the agent’s folder: 0, the folder alone; 1, its subfolders too (a monorepo’s web/); 3, down to apps/api/src. Never in hidden folders, node_modules, target, vendor, dist, build or out.'), kind: 'number', unit: 'levels' },
      { key: 'fix_log_lines', label: t('Logs for a fix'), hint: t('An agent launched from Alerts to fix a failed pipeline gets the end of each failed job’s log in its prompt, this many lines, read by Prism (up to 3 jobs). 0: none, the agent reads the logs itself.'), kind: 'number', unit: 'lines' },
    ],
  },
  {
    id: 'agents-claude', source: 'sessions', label: t('Claude Code'), table: '[agents.claude]', path: ['agents', 'claude'],
    intro: t('Claude Code’s own keys, wherever it runs: as the default CLI or for a repository (Repositories › Agent). Its Claude Desktop sessions, the cost of its conversations and its plan usage’s token too. These keys were [agents]’, [sessions]’ and [usage]’s: still read there, and moved here when changed.'),
    fields: [
      ...cliFields('claude'),
      { key: 'cost_alert', label: t('Cost alert'), hint: t('An agent whose conversation is estimated beyond this (at the models’ API prices; on a subscription, a figure of comparison) is flagged in Alerts. 0: never.'), kind: 'number', unit: '$' },
      { key: 'show_desktop', label: t('Show Desktop sessions'), hint: t('Claude Desktop sessions listed in the Agents view, next to the Prism agents. Only while Claude Code is the default CLI.'), kind: 'toggle' },
      { key: 'count_desktop', label: t('Count Desktop sessions'), hint: t('Off: the agent counters (header, status bar, menu bar or tray, Alerts, notifications) count only Prism agents. On: Claude Desktop sessions too. The Agents view lists both either way.'), kind: 'toggle' },
      { key: 'sessions_dir', label: t('Desktop sessions'), hint: t('Metadata of the Claude Code desktop sessions.'), kind: 'path', check: 'directory' },
      { key: 'transcripts', label: t('Transcripts'), hint: t('To know whether a session is working.'), kind: 'path', check: 'directory' },
      { key: 'sessions_poll', label: t('Poll every'), hint: t('The desktop sessions.'), kind: 'number', unit: 'seconds' },
      { key: 'refresh_token', label: t('Refresh the token'), hint: t('Plan usage: when the Claude Code token has expired, run claude doctor (with the binary above), which renews it without calling the model (at most every 15 min).'), kind: 'toggle' },
    ],
  },
  {
    id: 'agents-command-code', label: t('Command Code'), table: '[agents.command-code]', path: ['agents', 'command-code'],
    intro: t('Command Code’s own keys, wherever it runs: as the default CLI or for a repository (Repositories › Agent).'),
    fields: cliFields('command-code'),
  },
  {
    id: 'agents-opencode', label: t('OpenCode'), table: '[agents.opencode]', path: ['agents', 'opencode'],
    intro: t('OpenCode’s own keys, wherever it runs: as the default CLI or for a repository (Repositories › Agent). It has no plan usage to show.'),
    fields: cliFields('opencode'),
  },
  {
    id: 'agents-codex', label: t('Codex'), table: '[agents.codex]', path: ['agents', 'codex'],
    intro: t('Codex’s own keys, wherever it runs: as the default CLI or for a repository (Repositories › Agent). Prism’s hooks go on its command line (-c), trusted once in the agent’s terminal.'),
    fields: cliFields('codex'),
  },
  {
    id: 'forge', label: t('Forge'), table: '[forge]', path: ['forge'],
    // Its collector's last read shows at the top.
    get source() {
      return onGithub() ? 'github' : 'gitlab'
    },
    intro: t('Where the repositories live, one per profile, and what both forges share. Each forge’s login and own keys are on its page below. The switch, branches, merged window, polling and CI health were [gitlab]’s: still read there, and moved here when changed.'),
    fields: [
      { key: 'provider', label: t('Provider'), hint: t('gitlab (glab) or github (gh): see its page below.'), kind: 'select', options: ['gitlab', 'github'] },
      {
        key: 'enabled', kind: 'toggle',
        get label() {
          return t('{forge} access', { forge: forgeName() })
        },
        get hint() {
          return t('Off (no {forge} account, e.g. Product): neither {forge} nor the local repository are read — no {mrs}, pipelines, worktrees, vulnerabilities, repos or agents — so no access error in Alerts.', { forge: forgeName(), mrs: `${mrWord()}s` })
        },
      },
      { key: 'main_branch', label: t('Main branch'), hint: t('Of every repository that names none of its own (Repositories › Branches); with the release branches, the protected branches whose pipelines are followed. Empty: each remote’s default branch.'), kind: 'text', optional: true, placeholder: t('the remote’s default') },
      { key: 'release_prefix', label: t('Release prefix'), hint: t('Release branches (Git Flow) of every repository that names none of its own: the latest one is the base of new worktrees.'), kind: 'text', placeholder: 'release/' },
      { key: 'merged_days', label: t('Merged MRs'), hint: t('How long a merged MR (pull request on GitHub) stays tracked.'), kind: 'number', unit: 'days' },
      { key: 'poll_seconds', label: t('Poll every'), hint: t('Merge requests and pipelines: no forge reaches your machine by webhook. On GitHub, each read of a repository costs about 3 of the 5000 points of the hourly API quota (shared with gh); Prism slows down when it runs low.'), kind: 'number', unit: 'seconds' },
      { key: 'ci_health', label: t('CI health'), hint: t('The CI health block: failure rate and durations of its repository’s pipelines (workflow runs on GitHub), chosen in Repositories › Blocks that read a repository.'), kind: 'toggle' },
      { key: 'ci_health_days', label: t('CI health window'), kind: 'number', unit: 'days' },
      { key: 'avatars', label: t('Profile pictures'), hint: t('MR authors and project members shown with their GitLab / GitHub profile picture, read once per session. Off, or without one: their initials.'), kind: 'toggle' },
    ],
  },
  {
    id: 'gitlab', source: 'gitlab', label: t('GitLab'), table: '[gitlab]', path: ['gitlab'], login: 'gitlab',
    intro: t('What GitLab alone has. Authentication is glab’s (the system keychain), never this file.'),
    fields: [
      { key: 'project', label: t('Project (former)'), hint: t('Former way to name the project of the repository bare MR references name, still read. Prefer Repositories.'), kind: 'text', optional: true, placeholder: t('see Repositories'), check: 'gitlab_project' },
      { key: 'host', label: t('Host'), hint: t('gitlab.com, or a self-hosted instance: links, glab, image registry (registry.<host>).'), kind: 'text', placeholder: 'gitlab.com' },
      { key: 'merge_queue_label', label: t('Merge queue label'), hint: t('Your merge bot or CI job watching it rebases the MR, then sets auto-merge. Empty: no merge queue button.'), kind: 'text', placeholder: 'merge-queue' },
      { key: 'merge_priority_prefix', label: t('Merge priority prefix'), hint: t('Labels made of it and a number (merge-priority::0) order the merge queue: the lowest number first, then the MRs without one, the oldest first within each. Empty: the oldest first only.'), kind: 'text', optional: true, placeholder: 'merge-priority::' },
    ],
  },
  {
    id: 'github', label: t('GitHub'), table: '[forge]', path: ['forge'], login: 'github',
    intro: t('GitHub has no key of its own: authentication is gh’s (gh auth login, the system keychain), never this file. On GitHub, Prism reads the pull requests, their checks and the protected branches’ checks, acts on pull requests (merge, auto-merge, rebase, approve, comment, draft, people) and on GitHub Actions (jobs, logs, retries, cancel, deployments to approve), scans vulnerabilities (images on ghcr.io, Dependabot’s alerts), measures CI health (GitHub Actions) and follows Renovate; delivery stays GitLab’s for now. The projects of Repositories are owner/repo.'),
    fields: [],
  },
  {
    id: 'tracker', label: t('Tracker'), table: '[tracker]', path: ['tracker'],
    intro: t('Where the tickets live, and the conventions every tracker shares: identifiers and the flow of statuses. These keys were ClickUp’s: still read there, and moved here when changed.'),
    fields: [
      { key: 'provider', label: t('Provider'), hint: t('clickup, obsidian (Obsidian Kanban boards, see Obsidian) or jira (Jira Cloud, see Jira). Each is read only when its own section is on.'), kind: 'select', options: ['clickup', 'obsidian', 'jira'] },
      { key: 'pick_statuses', label: t('Ready statuses'), hint: `${t('A ticket ready to be picked: To pick. ClickUp: of the next sprint and the pick boards; Obsidian: of every board, and they leave the Sprint block.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'ticket_prefix', label: t('Ticket prefix'), hint: t('Empty: no ticket ids. E.g. ABC for ABC-123: found in branches, MR titles, notes and the filter.'), kind: 'text', placeholder: 'ABC' },
      { key: 'workflow_statuses', label: t('Workflow'), hint: `${t('The ticket flow, in order: sorts the statuses in Sprint and the lists.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'before_dev_statuses', label: t('Before dev'), hint: `${t('An open MR on such a ticket is flagged (Sync).')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'in_progress_statuses', label: t('In progress'), hint: `${t('Such a ticket with no branch nor MR for long is flagged; a merged MR on it is not (more MRs may follow).')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'review_statuses', label: t('In review'), hint: `${t('The ticket waits for its MR; the first one is offered when an MR opens.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'approved_statuses', label: tc('settings', 'Approved'), hint: `${t('Its open MRs are approved: such a ticket still in review is flagged (Sync), the first one is offered.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'merge_queue_statuses', label: t('To merge'), hint: `${t('Its open MRs should be in the merge queue: one that is not, in a repository with a merge queue (Settings › Code › Forge › GitLab › Merge queue label), is flagged (Sync), with Merge queue as its fix. Empty: no such rule.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'after_merge_statuses', label: t('After merge'), hint: `${t('A merged MR on a ticket not in one of these (nor in progress) is flagged.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'done_statuses', label: tc('settings', 'Done'), hint: `${t('A finished ticket (a closed one always is).')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'blocked_statuses', label: t('Blocked'), hint: `${t('A blocked ticket is left out of the Sync alerts (merged or open MR, missing branch…).')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'validation_statuses', label: t('Validation'), hint: `${t('On a ticket in one of these, “Validate locally” (agent). Launched from the first, the ticket moves to the last and is assigned to you. Empty: no such action.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'estimate_builtin', label: t('Enter the estimate'), hint: t('The tracker’s own estimate (ClickUp’s time estimate, Jira’s original estimate) typed in hours from the ticket rows and preview. Off: only shown, for a team estimating in custom fields (Estimate fields, ClickUp).'), kind: 'toggle' },
      { key: 'branch_aliases', label: t('Branch aliases'), hint: t('Words standing for the ticket prefix in the branches of agent sessions: agent/<word>-512-… is ticket 512 (a skill plugin naming its branches after itself).'), kind: 'list' },
      { key: 'next_action', label: t('Next action'), hint: t('The card at the top of a ticket’s panel: where it stands and why, with the one action that moves it on (Review-fix, Fix, Review, Implement…).'), kind: 'toggle' },
      { key: 'actuals', label: t('Actual vs estimate'), hint: t('A ticket’s actual time and cost against its estimate: the Actual section of its panel, and the Actuals of My sprint (its done tickets, their gap, the trend by type and by estimate over the last sprints). Actual: the time logged and the agents’ active time.'), kind: 'toggle' },
      { key: 'actuals_logged_only', label: t('Logged time only'), hint: t('The actual is the time logged only, without the agents’ active time.'), kind: 'toggle' },
      { key: 'ticket_type', label: t('Ticket type'), hint: t('Where a ticket’s type comes from, for the trends of the Actuals: empty, the tracker’s task type; tags, its first tag (Jira: label); otherwise the name of a custom field of ClickUp’s Fields.'), kind: 'text', optional: true, placeholder: t('task type') },
    ],
  },
  {
    id: 'clickup', source: 'clickup', label: t('ClickUp'), table: '[clickup]', path: ['clickup'], token: 'clickup',
    intro: t('When on, the token comes from the system keychain (or CLICKUP_TOKEN), never this file. Lists and folders are found by name.'),
    fields: [
      { key: 'enabled', label: t('ClickUp'), hint: t('Off: no ClickUp at all — no token needed; sprint, refinement, tickets and time entries disappear.'), kind: 'toggle' },
      { key: 'sprint_folder', label: t('Sprint folder'), hint: t('Folder of the sprint lists (ClickUp Sprints): the current one holds today, by the dates in its name, “(21/09 - 04/10)”, or its own.'), kind: 'text', placeholder: 'Sprints' },
      {
        key: 'boards', label: t('Boards'), kind: 'table', blank: { name: '', uses: ['pick'] },
        hint: t('Beyond the sprint, found by name. Uses: pick (To pick), refinement (Refinement), sync (read whole, its tickets checked against their MRs in Alerts), time (tasks to log time on). A folder board takes its lists; with Quarters, only “Q4 2026” ones, current and coming. Red and orange boards come before the next sprint in To pick.'),
        columns: [
          { key: 'name', label: t('Name'), kind: 'text', width: '130px' },
          { key: 'list', label: t('List'), kind: 'text', optional: true, placeholder: t('its name') },
          { key: 'folder', label: t('Or folder'), kind: 'text', optional: true },
          { key: 'quarters', label: t('Quarters'), kind: 'toggle', width: '70px' },
          { key: 'uses', label: t('Uses'), kind: 'list' },
          { key: 'tone', label: t('Colour'), kind: 'select', options: ['ko', 'warn', 'info', 'violet', 'ok'], optional: true, placeholder: 'info', width: '90px' },
        ],
      },
      { key: 'refinement_statuses', label: t('Refinement statuses'), hint: `${t('One column each, in this order.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'ongoing_statuses', label: t('Ongoing statuses'), hint: `${t('In progress or further: shown in Sprint when outside the sprint.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'time_tracking', label: t('Time tracking'), hint: t('Time entries on tickets: the Time block and Log time. Off: none read nor offered.'), kind: 'toggle' },
      { key: 'time_history_days', label: t('Time history'), hint: t('Days of logged time read back (Time block, date picker).'), kind: 'number', unit: 'days' },
      { key: 'day_hours', label: t('Hours per day'), hint: t('Time to log per working day (Time block); the week counts five.'), kind: 'number', unit: 'h' },
      { key: 'fields', label: t('Ticket fields'), hint: t('ClickUp custom fields shown on the tickets, by name (Type, Story points…): a pill in their option’s color on each row, a line in the preview. Only those with a value; in this order. Each one can sort the lists. Applied at the next read.'), kind: 'list' },
      { key: 'filter_fields', label: t('Filter fields'), hint: t('Those of Ticket fields also offered as filters and groupings of the lists (a dropdown such as Type); the others are only shown, and sortable (an estimate in hours). Empty: all of them.'), kind: 'list', optional: true, placeholder: t('all the ticket fields') },
      { key: 'estimate_fields', label: t('Estimate fields'), hint: t('Number custom fields holding an estimate in hours, by name (Dev estimate (h)…): typed in from the ticket rows and preview, even when empty; shown after Ticket fields unless listed there. A formula or any other type is never written to.'), kind: 'list' },
      { key: 'avatars', label: t('Profile pictures'), hint: t('Assignees shown with their ClickUp profile picture, read once per session. Off, or without one: their initials.'), kind: 'toggle' },
      { key: 'poll_seconds', label: t('Poll every'), kind: 'number', unit: 'seconds' },
    ],
  },
  {
    id: 'jira', source: 'jira', label: t('Jira'), table: '[jira]', path: ['jira'], token: 'jira',
    intro: t('Jira Cloud as the tracker (Tracker › Provider: jira): the active sprint of a Scrum board, or the open issues of a Kanban one, its columns ordering the statuses. An issue’s status is its Jira status, lowercased, as the Tracker’s flow statuses name it. The API token goes into the system keychain (or JIRA_TOKEN), never this file. From Prism, an issue changes status through a transition of its workflow (one leading there must exist), its priority changes (to the first Jira priority reading as urgent, high, normal or low), comments are added or edited as plain text, it is assigned to you or added to the active sprint, and new issues are created in the board’s project.'),
    fields: [
      { key: 'site', label: t('Site'), hint: t('The Jira Cloud site.'), kind: 'text', placeholder: 'team.atlassian.net' },
      { key: 'email', label: t('Account email'), hint: t('The Atlassian account the API token belongs to.'), kind: 'text', placeholder: 'me@example.com' },
      { key: 'board', label: t('Board'), hint: t('The board’s id, in its URL: …/boards/42. Its active sprint is the Sprint block, its next one shows in To pick.'), kind: 'number' },
      {
        key: 'queues', label: t('Queues'), kind: 'table', blank: { name: '', jql: '', uses: ['pick'] },
        hint: t('Issues followed beyond the sprint, each a JQL query. Uses: pick (To pick: its issues in the Ready statuses, Settings › Work › Tracker), refinement (Refinement, arranged by epic), sync (read whole, its issues checked against their MRs in Alerts). Red and orange queues come before the next sprint in To pick.'),
        columns: [
          { key: 'name', label: t('Name'), kind: 'text', width: '130px' },
          { key: 'jql', label: t('JQL'), kind: 'text', placeholder: 'project = ABC AND type = Bug AND resolution = Unresolved' },
          { key: 'uses', label: t('Uses'), kind: 'list' },
          { key: 'tone', label: t('Colour'), kind: 'select', options: ['ko', 'warn', 'info', 'violet', 'ok'], optional: true, placeholder: 'info', width: '90px' },
        ],
      },
      { key: 'refinement_statuses', label: t('Refinement statuses'), hint: `${t('One column each, in this order. Empty: every open status of the refinement queues’ issues.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'ongoing_statuses', label: t('Ongoing statuses'), hint: `${t('The board’s issues in these statuses outside the active sprint show in Sprint. Empty: Jira’s “In Progress” category.')} ${STATUS_HINT}`, kind: 'list' },
      { key: 'issue_type', label: t('New issue type'), hint: t('The type of an issue created from the Sprint block’s +, as the board’s project names it (Task, Tâche, Story…). It goes into the active sprint.'), kind: 'text', placeholder: 'Task' },
      { key: 'time_tracking', label: t('Time tracking'), hint: t('Your worklogs: the Time block and Log time, read every 3 minutes. Hours per day and history come from Settings › Work › Tracker › ClickUp. Off: none read nor offered.'), kind: 'toggle' },
      { key: 'poll_seconds', label: t('Poll every'), kind: 'number', unit: 'seconds' },
    ],
  },
  {
    id: 'obsidian', source: 'obsidian', label: t('Obsidian'), table: '[obsidian]', path: ['obsidian'],
    intro: t('Obsidian Kanban boards as the tracker (Tracker › Provider: obsidian), read from the vault’s files: nothing to sign in to. A card’s status is its lane (“🚧 En cours” reads “en cours”); it is done when checked, in a lane marked complete, or in a Done status. A card linking a note takes its id, title and priority (p0 to p3) from the note’s frontmatter. From Prism, a card moves between its board’s lanes (its note’s status following the lane’s other notes), its priority changes and comments go under its note’s “## Comments”.'),
    fields: [
      { key: 'vault', label: t('Vault'), hint: t('The vault’s folder: board paths start there, and its name opens the links in Obsidian.'), kind: 'path', check: 'directory' },
      {
        key: 'boards', label: t('Boards'), kind: 'table', blank: { path: '' },
        hint: t('One per repository. Its tickets’ ids resolve in that repository (worktrees, agents, MRs), so two projects can both have a T-12. Absent: the board’s folder names it. Id prefix: a pattern Prism adds -<number> to (T, [ABCLM] for A-07 to M-23, KB|TH), found in branches, MR titles and notes; no capturing group. Absent: Tracker › Ticket prefix.'),
        columns: [
          { key: 'path', label: t('Board'), kind: 'text', placeholder: 'Projects/Things/Board.md' },
          { key: 'repo', label: t('Repository'), kind: 'text', optional: true, placeholder: t('its [[repos]] name'), width: '160px' },
          { key: 'prefix', label: t('Id prefix'), kind: 'text', optional: true, placeholder: t('T, or [ABCLM]'), width: '120px' },
        ],
      },
    ],
  },
  {
    id: 'worktrees', source: 'worktrees', label: t('Worktrees'), table: '[worktrees]', path: ['worktrees'],
    fields: [
      { key: 'repo', label: t('Repository (former)'), hint: t('Former way to name the clone of the repository bare MR references name, still read. Prefer Repositories.'), kind: 'path', optional: true, placeholder: t('see Repositories'), check: 'git_repo' },
      { key: 'base_branch', label: t('Base branch'), hint: t('Of the new worktrees of every repository that names none of its own: auto, the latest release branch, else the main one; or a branch name.'), kind: 'text' },
      { key: 'stale_days', label: t('Stale after'), hint: t('A worktree without commit nor agent session for this long is stale (Worktrees view).'), kind: 'number', unit: 'days' },
      { key: 'poll_seconds', label: t('Poll every'), kind: 'number', unit: 'seconds' },
    ],
  },
  {
    id: 'repos', source: 'repos', label: t('Repositories'), table: '[[repos]]', path: [],
    intro: t('The repositories Prism follows, grouped by working folder, and what it reads of each: on this machine from its clone, on the forge from its project. Open one for its switches and branches. A pack’s repositories merge by name: add the clone of yours (a pack never sets it).'),
    fields: [
      {
        key: 'repos', label: t('Repositories'), kind: 'table', blank: { name: '' },
        columns: [
          { key: 'name', label: t('Name'), kind: 'text', width: '120px' },
          { key: 'path', label: t('Path'), kind: 'path', optional: true, placeholder: t('none: MRs only'), check: 'git_repo' },
          { key: 'project', label: t('Project'), kind: 'text', optional: true, placeholder: 'group/project', check: 'gitlab_project' },
          { key: 'merge_requests', label: t('MRs'), kind: 'toggle', width: '60px' },
          { key: 'primary', label: t('Primary'), kind: 'toggle', width: '70px' },
          { key: 'worktrees', label: t('Worktrees'), kind: 'toggle', width: '80px' },
          { key: 'merge_queue', label: t('Queue'), kind: 'toggle', width: '60px' },
          { key: 'ticket_required', label: t('Ticket'), kind: 'toggle', width: '60px' },
          { key: 'pipelines', label: t('Pipelines'), kind: 'toggle', width: '75px' },
          { key: 'agent', label: t('Agent'), kind: 'select', options: ['claude', 'command-code', 'opencode', 'codex'], optional: true, placeholder: tc('settings', 'default'), width: '130px', check: 'agent_cli' },
          { key: 'forge', label: t('Forge'), kind: 'select', options: ['gitlab', 'github'], optional: true, placeholder: t('profile’s'), width: '110px' },
        ],
      },
    ],
  },
  {
    id: 'repo-blocks', label: t('Blocks that read a repository'), table: '[forge] bare_mr_repo', path: [],
    keys: ['forge.bare_mr_repo', 'forge.ci_health_repo', 'renovate.repo', 'vulnerabilities.repos'],
    fields: [],
  },
  {
    id: 'logs', label: t('Logs'), table: '[logs]', path: ['logs'],
    intro: t('Prism writes its logs to a file of this profile, besides standard error: what to look at, or send, when a source does not read. The RUST_LOG environment variable takes precedence over the level. From config.toml alone (a pack does not set it).'),
    fields: [
      { key: 'level', label: t('Level'), hint: t('info by default; debug to troubleshoot (more lines, the requests’ details), then back to info. error and warn keep only the problems.'), kind: 'select', options: ['error', 'warn', 'info', 'debug', 'trace'] },
    ],
  },
  {
    id: 'updates', label: t('Updates'), table: '[updates]', path: ['updates'],
    intro: t('Prism updates itself from the releases of a repository (prism-v… tags, scripts/release.sh), read with glab or gh: the access you already have, no token. A team’s pack names its private repository, whose builds carry the team’s blocks; the open-source Prism, its public one. On macOS and from an AppImage, Install replaces Prism and restarts it; a .deb or an Arch package is downloaded, for sudo apt install or sudo pacman -U.'),
    fields: [
      { key: 'enabled', label: t('Check for updates'), hint: t('At launch and every 6 hours; a newer Prism shows in the status bar. Off: only Check now reads the releases.'), kind: 'toggle' },
      { key: 'source', label: t('Source'), hint: t('Where the releases are published: glab:group/project or gh:owner/repo. Empty: no updates. Checked once saved.'), kind: 'text', placeholder: 'glab:group/project', optional: true },
      { key: 'packs', label: t('Check the packs'), hint: t('At the same times, the packs’ sources are read again: a pack changed there shows in the status bar, Settings › Packs updates it. Off: only Check now reads them.'), kind: 'toggle' },
    ],
  },
  {
    id: 'feedback', label: t('Feedback'), table: '[feedback]', path: ['feedback'],
    intro: t('Send feedback opens an issue on the repository Prism comes from, or on a pack’s (the one it was imported from), with glab or gh: the access you already have, no token. The composer shows the issues that look like it, and exactly what goes with it: Prism’s version, where it was opened from, the packs; on request the theme, the last errors of the log (paths and addresses stripped) and a capture of the window. Never a value of the config, a ticket or a mail. Console: feedback.'),
    fields: [
      { key: 'enabled', label: t('Send feedback'), hint: t('In the console (feedback), the Help menu, the status bar and each block’s “?”.'), kind: 'toggle' },
      { key: 'follow', label: t('Follow my requests'), hint: t('With the updates (at launch and every 6 hours), the issues opened from Prism are read again: a closed one shows in the status bar, a release that names one says so in its notes.'), kind: 'toggle' },
      { key: 'repo', label: t('Prism’s repository'), hint: t('Where Prism’s own feedback goes, glab:group/project or gh:owner/repo. Empty: [updates] source.'), kind: 'text', placeholder: 'glab:group/project', optional: true },
    ],
  },
  {
    id: 'feedback-labels', label: t('Labels'), table: '[feedback.labels]', path: ['feedback', 'labels'],
    intro: t('The label each kind of feedback gets. GitLab creates a label it does not have; GitHub refuses one: the issue is then opened without it.'),
    fields: [
      { key: 'feature', label: t('Feature'), kind: 'text', placeholder: 'enhancement', optional: true },
      { key: 'bug', label: t('Bug'), kind: 'text', placeholder: 'bug', optional: true },
      { key: 'idea', label: t('Idea'), kind: 'text', placeholder: t('none'), optional: true },
    ],
  },
  {
    id: 'remote', label: t('Access'), table: '[remote]', path: ['remote'],
    intro: t('Prism from a phone: a page the computer serves, which Chrome on Android installs as an app (Safari: Share › Add to Home Screen). Prism listens on 127.0.0.1 only; a tunnel brings the phone to it. Tailscale is the one Prism sets up itself (tailscale serve: an HTTPS name and certificate, the tailnet user checked); with another (WireGuard and a proxy, ssh -L), your proxy brings HTTPS. A tunnel that decrypts elsewhere (Cloudflare Tunnel, ngrok) sees all of Prism in clear. Each phone is paired in Remote › Devices and unlocked with a passkey. Applied on save.'),
    fields: [
      { key: 'enabled', label: t('Remote access'), hint: t('Off by default. On: Prism answers phones through the tunnel below; the status bar says so as long as it is on.'), kind: 'toggle' },
      { key: 'tunnel', label: t('Tunnel'), hint: t('tailscale: Prism runs tailscale serve and only lets your tailnet user in. other: your own tunnel brings HTTPS to the port below, at the address below. relay: this profile is reached through another profile’s Prism open beside it, whose remote has the tunnel: its phones, their passkeys, at /p/<profile>/ there; this one keeps its own rights below, and takes a free port of its own.'), kind: 'select', options: ['tailscale', 'other', 'relay'] },
      { key: 'port', label: t('Port'), hint: t('On 127.0.0.1, never another interface.'), kind: 'number' },
      { key: 'tailscale_socket', label: t('Tailscale socket'), hint: t('Beside a VPN, the Tailscale app fights it for the network: `just tailscale` runs a tailscaled of its own in userspace mode (no interface, route nor DNS change) and prints its socket, to paste here. Empty: the Tailscale app.'), kind: 'text', placeholder: '~/.local/share/prism/tailscale/tailscaled.sock', optional: true },
      { key: 'url', label: t('Address'), hint: t('Tunnel other: the HTTPS address the phone opens. Passkeys are bound to its host: changing it means pairing the phones again. Tailscale: empty, the computer’s tailnet name.'), kind: 'text', placeholder: 'https://prism.example.net', optional: true },
      { key: 'preview_port', label: t('Preview port'), hint: t('The dev servers’ preview (Exposure › Dev servers preview) listens on 127.0.0.1 there, never another interface. Tailscale serves it on the computer’s tailnet name, port 8443.'), kind: 'number' },
      { key: 'preview_url', label: t('Preview address'), hint: t('Tunnel other: the HTTPS address your tunnel brings to the preview port, on the same host as the address above (another port): the phone’s session goes there. Empty: no preview. Tailscale: empty.'), kind: 'text', placeholder: 'https://prism.example.net:8443', optional: true },
    ],
  },
  {
    id: 'remote-exposure', label: t('Exposure'), table: '[remote]', path: ['remote'],
    intro: t('What a phone sees and does. Every command of Prism has a class: read, act, critical or never (config, tokens, updates, profiles, what opens an app on the computer, a new shell). A phone always reads; it acts with Actions on; what is critical (merge, a manual job, a kill, a removal) and typing in a terminal, each with its own switch, are confirmed by the phone’s passkey (biometrics). Never from a phone: the config and its secrets, updates, profiles, what opens an app on the computer, a command line or a shell of its choosing. Each action from a phone is written in Prism’s log, with the phone’s name.'),
    fields: [
      { key: 'tabs', label: t('Bottom tabs'), hint: t('In order, among alerts, mrs, queue, tickets, refinement, agents, worktrees, notes, today, mail, ci, renovate, infra, argocd, sprint, wip, and any block by its id (system, trail, pet, docker, tokens, data…); the other views are under More.'), kind: 'list' },
      { key: 'actions', label: t('Actions'), hint: t('Retry a job, rebase, comment, a ticket’s status or time, an agent’s permission (once), a prompt, a launch (in a new worktree, never in auto mode), an agent’s folder committed, pushed or rebased (Diff › Git bar: never on main or a release branch, no hook run). Off: the phone only reads.'), kind: 'toggle' },
      { key: 'critical', label: t('Critical actions'), hint: t('Merge, the merge queue, auto-merge, an MR’s labels (the queue’s or a CI’s), a new MR pipeline, a manual job (often a deployment), a force push, stopping or closing an agent, removing a worktree with nothing to lose: each confirmed by the phone’s passkey, every time. Off: never from a phone.'), kind: 'toggle' },
      { key: 'power', label: t('Restart or shut down the computer'), hint: t(isMac ? 'More › The computer on the phone: Restart and Shut down, as from the Apple menu (an app with unsaved work may still refuse), each time confirmed by the passkey. With FileVault, a restart also takes the Mac’s password, typed on the phone and never kept, so that the computer comes back by itself instead of waiting for it at startup. Afterwards the phone reaches Prism again once the computer is up and Prism open (Login Items, or Away › Keep Prism running). Off by default.' : 'More › The computer on the phone: Restart and Shut down (systemd: an application with unsaved work may still refuse), each time confirmed by the passkey. Afterwards the phone reaches Prism again once the computer is up and Prism open (Away › Keep Prism running, or your session’s autostart). Off by default.'), kind: 'toggle' },
      { key: 'typing', label: t('Typing to agents'), hint: t('The phone sends a text from its answer box, takes an agent’s terminal (Take) to type in it with a keys bar, and edits a text file of its folder (Files › Edit); all confirmed by its passkey, once per agent for the step-up window. The agent’s window takes the phone’s size while it is taken; a key on the desk takes it back. Off: the phone watches, and answers with its quick answers and keys only.'), kind: 'toggle' },
      { key: 'mail', label: t('Mail'), hint: t('The Mail block on the phone. Off: its data never leaves the computer.'), kind: 'toggle' },
      { key: 'preview', label: t('Dev servers preview'), hint: t('On the phone, a dev server of the Dev servers block opens in its browser, as an agent’s folder serves it, hot reload included: through a second address of the tunnel (Tailscale: the computer’s name, port 8443), which forwards to the server chosen last. Only the ports of the block’s running servers, never one held outside Prism; nothing without the phone’s session. The app runs on its own origin, away from Prism’s page, and never sees the session cookie. On by default.'), kind: 'toggle' },
      { key: 'projects', label: t('New projects'), hint: t('More › New project on the phone: a folder with a git repository and a first commit, in one of Repositories › Working folders only (the phone picks one and names the project, never a folder), on request its project on the forge, pushed, or later (Kept local › Publish); added to the repositories. Or a project of the folder’s forge you are a member of, cloned into it. Each confirmed by the passkey. Without working folders, none.'), kind: 'toggle' },
      { key: 'settings', label: t('Settings'), hint: t('More › Settings on the phone: every setting read (tokens never: they stay in the keychain), and a few changed — notifications (digest, quiet hours, focus), the alert thresholds, the appearance — each save confirmed by the passkey. What runs a program, lets someone in or decides what Prism reads stays on the computer. Off: settings never leave the computer.'), kind: 'toggle' },
    ],
  },
  {
    id: 'remote-sessions', label: t('Sessions'), table: '[remote]', path: ['remote'],
    intro: t('Nothing locks on the desk by itself: the remote stays on until turned off. What expires is a phone’s session, opened again with biometrics, not a new pairing.'),
    fields: [
      { key: 'session_idle_minutes', label: t('Session idle'), hint: t('Without a request from the phone for this long, biometrics again.'), kind: 'number', unit: 'minutes' },
      { key: 'session_max_hours', label: t('Session length'), hint: t('Biometrics again after this long, active or not.'), kind: 'number', unit: 'hours' },
      { key: 'step_up_minutes', label: t('Step-up window'), hint: t('Taking a terminal again within this long asks nothing more. Critical actions ask each time, whatever this says.'), kind: 'number', unit: 'minutes' },
      { key: 'control_idle_minutes', label: t('Give a terminal back'), hint: t('After this long without a key from the phone, the terminal is watched again. 0: only on Release, or when the desk types.'), kind: 'number', unit: 'minutes' },
      { key: 'push', label: t('Notifications'), hint: t('An agent that starts waiting (a permission, a question) wakes the paired phones that have Prism closed, at most once a minute each; each phone turns it on in More › Notifications. The push carries nothing: its text is written by the phone, nothing of Prism goes through Google’s or Apple’s push service. Brave: allow Google services for push messaging in its settings.'), kind: 'toggle' },
    ],
  },
  {
    id: 'remote-away', label: t('Away'), table: '[remote]', path: ['remote'],
    intro: t('Days away from the computer with only the phone: what keeps it within reach. Below, what this computer says now.'),
    fields: [
      { key: 'keep_awake', label: t('Keep the computer awake'), hint: t(isMac ? 'While the remote is on and the computer on power: no idle sleep. The screen may lock and turn off, Prism carries on; lid closed on battery, macOS sleeps anyway.' : 'While the remote is on: no idle sleep (systemd-inhibit). The screen may lock and turn off, Prism carries on.'), kind: 'toggle' },
      { key: 'keep_running', label: t('Keep Prism running'), hint: t(isMac ? 'macOS, while the remote is on: Prism opens at login and again after a crash (a LaunchAgent, ~/Library/LaunchAgents/app.prism.desktop.watch*.plist, runs prism watch every minute). Quit with ⌘Q, it stays closed until the next login. Prism installed as an app only. Off by default.' : 'Linux, while the remote is on: Prism opens again after a crash and at login (a user systemd service running prism watch every minute). Prism installed only, not a build of its repository. Off by default.'), kind: 'toggle' },
    ],
  },
  {
    id: 'remote-peers', label: t('What other computers may do'), table: '[remote.peers]', path: ['remote', 'peers'],
    intro: t('Another computer’s Prism paired with this one (Remote › Devices › Pair a computer, or prism peer pair) opens it in a window as this account would. It always reads, settings included (tokens never: they stay in the keychain). What each switch lets it do is logged with its name. Applied at once.'),
    fields: [
      { key: 'actions', label: t('Actions'), hint: t('A ticket’s status, a comment, a retry, a rebase, an agent launched, answered or resumed, a worktree’s branch pulled. An agent launched runs commands of its own: with Actions on, Typing off does not keep that computer from running code here.'), kind: 'toggle' },
      { key: 'critical', label: t('Critical actions'), hint: t('Merge, the merge queue, a manual job, a force push, an agent stopped or closed, a worktree removed. That computer asks its own user first (Touch ID, polkit, Windows Hello), which guards against someone at it, not against its token in other hands: this switch is what holds here.'), kind: 'toggle' },
      { key: 'typing', label: t('Typing'), hint: t('An agent’s or a shell’s terminal typed in, a text sent to an agent, a file of its folder written, a task run.'), kind: 'toggle' },
      { key: 'settings', label: t('Settings'), hint: t('This computer’s settings saved, packs updated, tokens replaced (written, never read back), phones and computers paired or revoked. Off: it reads them only.'), kind: 'toggle' },
    ],
  },
  {
    id: 'machines', label: t('Machines'), table: '[[machines]]', path: [],
    intro: t('Other computers’ Prism, each opened in a window of this one from the profile menu, as if it ran here: its agents, terminals, worktrees, settings. That Prism has its remote on (a tunnel to it: Tailscale, or ssh -L to 127.0.0.1) and makes a token for this one: prism peer add <name> there, paired from its window the first time with the code prism peer pair shows there (its token kept in this computer’s keychain). The token opens that Prism as its account would: keep it as an SSH key; prism peer remove <name> there revokes it.'),
    fields: [
      {
        key: 'machines', label: t('Machines'), kind: 'table', blank: { name: '', url: '' },
        hint: t('Address: https://<name>.<tailnet>.ts.net, or http://127.0.0.1:<port> at a tunnel’s end. SSH host: its name in ~/.ssh/config, to open its folders here in VS Code (Remote-SSH); empty, they open there only. What is critical there (a merge, a manual job, an agent stopped, a worktree removed, a file of an agent saved) first asks the person at this computer: Touch ID or its password on a Mac, polkit’s agent (password or fingerprint) on Linux, Windows Hello on Windows; No confirmation: it goes without. Asked by the app, not the page.'),
        columns: [
          { key: 'name', label: t('Name'), kind: 'text', width: '120px' },
          { key: 'url', label: t('Address'), kind: 'text', placeholder: 'https://build-srv.tail1234.ts.net' },
          { key: 'ssh', label: t('SSH host'), kind: 'text', optional: true, placeholder: t('none'), width: '120px' },
          { key: 'skip_confirm', label: t('No confirmation'), kind: 'toggle' },
        ],
      },
    ],
  },
  {
    id: 'calendar', label: t('Calendar'), table: '[calendar]', path: ['calendar'],
    intro: t('The Today block’s agenda, meeting countdown and reminders. Where it is read: the macOS Calendar, or Google Calendar on any system; each is set up in its own page below.'),
    fields: [
      { key: 'enabled', label: t('Agenda'), hint: t('Off: no calendar is read, nor its access asked for. No Today block, meeting countdown or reminders.'), kind: 'toggle' },
      { key: 'provider', label: t('Provider'), hint: t('macos (macOS only) or google (any system, Linux included).'), kind: 'select', options: ['macos', 'google'] },
      { key: 'calendars', label: t('Calendars'), hint: t('The calendars read, among the provider’s. None checked: all of them (birthdays and holidays included), or those checked in Google Calendar.'), kind: 'checks', options: 'calendars' },
    ],
  },
  {
    id: 'calendar-macos', label: t('macOS Calendar'), table: '[calendar]', path: ['calendar'],
    intro: t('Read locally through EventKit (macOS only): the accounts added in System Settings › Internet Accounts are part of it — Exchange, Microsoft 365, Google, iCloud. No token nor network call. macOS asks for access on first launch; refused, allow Prism again in System Settings › Privacy & Security › Calendars. Invitations are answered in Calendar. Exchange / Microsoft 365 step by step: see Help › Exchange in the macOS Calendar.'),
    fields: [],
  },
  {
    id: 'mail', source: 'mail', label: t('Mail'), table: '[mail]', path: ['mail'],
    intro: t('The Mail block: the unread messages of your inbox, and the flagged (starred) ones, read every minute. Your mailbox only, never a body, a preview nor an attachment. Where it is read: Microsoft 365 or Gmail; each is set up and signed in to in its own page below.'),
    fields: [
      { key: 'enabled', label: t('Mail'), hint: t('Off by default. On: set up the provider below, sign in, then add the Mail block to a page.'), kind: 'toggle' },
      { key: 'provider', label: t('Provider'), hint: t('microsoft or gmail.'), kind: 'select', options: ['microsoft', 'gmail'] },
      { key: 'flagged', label: t('Flagged'), hint: t('The flagged messages too (starred, in Gmail), read or not.'), kind: 'toggle' },
      { key: 'max', label: t('Messages'), hint: t('Listed at most, newest first.'), kind: 'number', unit: 'messages' },
    ],
  },
  {
    id: 'mail-microsoft', label: t('Microsoft 365'), table: '[mail]', path: ['mail'], signin: 'mail',
    intro: t('Read through Microsoft Graph with the narrowest delegated permission, Mail.ReadBasic: sender, subject, date and states. The application is the one your organization registered in Entra ID; you sign in once here (a code to type at Microsoft’s address), and the refresh token goes into the system keychain, never this file. A message opens in Outlook on the web.'),
    fields: [
      { key: 'client_id', label: t('Application'), hint: t('The “Application (client) ID” of the app your organization registered in Entra ID, allowed the delegated Mail.ReadBasic permission. A team’s: in its pack.'), kind: 'text', placeholder: '00000000-0000-0000-0000-000000000000' },
      { key: 'tenant', label: t('Tenant'), hint: t('The Entra ID tenant, its id or domain. Empty: organizations (any work account).'), kind: 'text', optional: true, placeholder: 'organizations' },
    ],
  },
  {
    id: 'google', label: t('Google account'), table: '[google]', path: ['google'], token: 'google', signin: 'google',
    intro: t('One Google account per profile, shared by Gmail and Google Calendar: Prism asks only for the permissions of those chosen as providers, read-only (Gmail: labels and headers, gmail.metadata; Calendar: your calendars and their events). The OAuth client is your own, a “Desktop app” client of your Google Cloud project: see Help › Signing in to Google. The secret and the refresh token go into the system keychain, never this file.'),
    fields: [
      { key: 'client_id', label: t('Client ID'), hint: t('The “Client ID” of your Desktop app OAuth client (Google Cloud › APIs & Services › Credentials).'), kind: 'text', placeholder: '1234567890-abc….apps.googleusercontent.com' },
    ],
  },

  {
    id: 'clusters', label: t('Clusters'), table: '[clusters]', path: ['clusters'],
    intro: t('Access to the Kubernetes clusters, read from your kubeconfig. These keys were [runners]’: still read there, and moved here when changed.'),
    fields: [
      { key: 'enabled', label: t('Cluster access'), hint: t('Off (no rights on the clusters): neither pods, environments nor ArgoCD are read, so no access error in Alerts.'), kind: 'toggle' },
      { key: 'default_context', label: t('Default context'), hint: t('The cluster a Pods target or ArgoCD’s discovery reads when it names none.'), kind: 'select', options: 'kube', optional: true, check: 'kube_context' },
      { key: 'stages', label: t('Stages from'), hint: t('What names the stages and their environments, read by Environments, ArgoCD and the Env lists here: none (the default), the Env names of the environments below (staging, preprod, prod or production play those roles); otherwise a block of your team declaring them.'), kind: 'select', options: providers('stages') },
      { key: 'shell', label: t('Shell in a pod'), hint: t('A Shell button in a pod’s logs pane: kubectl exec into its container (bash, else sh), with your kubeconfig’s rights. tmux keeps it after closing; never from a phone.'), kind: 'toggle' },
      { key: 'usage_history', label: t('Pod usage over time'), hint: t('A Usage button in a pod’s logs pane: its CPU and memory over time, against its limits. For it, Prism keeps 6 hours of the Pods targets’ readings (metrics-server).'), kind: 'toggle' },
      { key: 'gcp_metrics', label: t('Google Managed Prometheus'), hint: t('For a GKE context (gke_<project>_<location>_<cluster>), that usage is read from Google Cloud Monitoring instead: any pod, an environment’s too, up to 7 days back. With gcloud’s login, which needs the monitoring.viewer role on the project.'), kind: 'toggle' },
    ],
  },
  {
    id: 'pods', source: 'pods', label: t('Pods'), table: '[[pods]]', path: [],
    intro: t('The pods the Pods block follows, live: each target is a cluster and its namespaces. A GitLab runner (its image or app label) and the CI jobs it starts are recognized: a runner down or jobs left without one show in Alerts, as does any pod failing (CrashLoopBackOff, image not found…). The former [runners] namespace is the target “Runners”.'),
    fields: [
      {
        key: 'pods', label: t('Targets'), kind: 'table', blank: { namespaces: [] },
        hint: t('Name: in the block (absent: the context). Context: absent, Clusters › Default context. Namespaces: * matches any characters (app-*, *-prod); empty: every namespace of the cluster.'),
        columns: [
          { key: 'name', label: t('Name'), kind: 'text', optional: true, width: '120px' },
          { key: 'context', label: t('Kube context'), kind: 'select', options: 'kube', optional: true, placeholder: tc('settings', 'default'), check: 'kube_context' },
          { key: 'namespaces', label: t('Namespaces'), kind: 'list' },
        ],
      },
    ],
  },
  {
    id: 'argocd-layout', label: t('ArgoCD layout'), table: '[argocd_layout]', path: ['argocd_layout'],
    intro: t('How an Application’s Helm values path is read: after Root, the first segment naming a stage’s environment is the environment, the one before it the group, the one after the apps segment the component. A customer group comes first: production, outside Environments.'),
    fields: [
      { key: 'namespace_prefix', label: t('Namespaces'), hint: t('Without [[argocd]], the namespaces of the default context’s cluster (Clusters) starting with this.'), kind: 'text', placeholder: 'argocd' },
      { key: 'root', label: t('Root'), hint: t('The path segment where the layout starts. Empty: the whole path.'), kind: 'text', optional: true, placeholder: 'argocd' },
      { key: 'apps_segment', label: t('Apps segment'), hint: t('The segment before the component’s.'), kind: 'text', placeholder: 'apps' },
      { key: 'customer_groups', label: t('Customer groups'), hint: t('Groups of customers’ instances: production, left out of Environments.'), kind: 'list' },
    ],
  },
  {
    id: 'envs', source: 'environments', label: t('Environments'), table: '[[environments]]', path: [],
    intro: t('Environments block: Deployments and pods, read-only. One row per environment and cluster.'),
    fields: [
      {
        key: 'environments', label: t('Environments'), kind: 'table', blank: { env: 'staging', context: '', namespaces: [] },
        columns: [
          { key: 'env', label: t('Env'), kind: 'text', options: 'envs', width: '110px' },
          { key: 'context', label: t('Kube context'), kind: 'select', options: 'kube', check: 'kube_context' },
          { key: 'namespaces', label: t('Namespaces'), kind: 'list', placeholder: t('* for all') },
        ],
      },
    ],
  },
  {
    id: 'argocd', source: 'argocd', label: t('ArgoCD'), table: '[[argocd]]', path: [],
    intro: t('ArgoCD instances, read as Kubernetes objects. None: the argocd* namespaces of the default context’s cluster (Clusters).'),
    fields: [
      {
        key: 'argocd', label: t('Instances'), kind: 'table', blank: { namespace: 'argocd' },
        columns: [
          { key: 'namespace', label: t('Namespace'), kind: 'text', width: '130px' },
          { key: 'context', label: t('Kube context'), kind: 'select', options: 'kube', optional: true, placeholder: t('default context'), check: 'kube_context' },
          { key: 'url', label: t('Web URL'), kind: 'text', optional: true, placeholder: 'https://argocd…' },
          { key: 'envs', label: t('Authoritative for'), kind: 'list', optional: true, options: 'envs' },
        ],
      },
    ],
  },
  {
    id: 'vulns', source: 'vulnerabilities', label: t('Vulnerabilities'), table: '[vulnerabilities]', path: ['vulnerabilities'],
    intro: t('Local Trivy scan of its repositories, once a day.'),
    fields: [
      { key: 'enabled', label: t('Vulnerabilities'), hint: t('Off by default. On: Trivy scans the dependencies of its repositories (Repositories › Blocks that read a repository) and the images a block names (needs trivy).'), kind: 'toggle' },
      { key: 'branches', label: t('Branches'), hint: t('Whose dependencies are scanned, one per line: main (the default), release (the latest release branch, otherwise the main one), or a branch’s name. With several, each project’s line carries its branch.'), kind: 'list', placeholder: 'main' },
      { key: 'components', label: t('Components folder'), hint: t('Its project.json projects are scanned one by one (an Nx monorepo’s apps). Empty (default): the repository as a whole.'), kind: 'text', optional: true, placeholder: 'apps' },
      { key: 'libs', label: t('Libraries folder'), hint: t('Shared libraries scanned besides the components: the project.json projects of an Nx monorepo’s libs. Empty: none.'), kind: 'text', optional: true, placeholder: 'libs' },
      { key: 'images', label: t('Images from'), hint: t('What names the images to scan: none (the default), the dependencies only; otherwise a block of your team naming them, its own settings saying which.'), kind: 'select', options: providers('images') },
      { key: 'skip_dirs', label: t('Skipped folders'), hint: t('Folders of an image Trivy skips (a tool’s cache).'), kind: 'list' },
    ],
  },
  {
    id: 'renovate', source: 'renovate', label: t('Renovate'), table: '[renovate]', path: ['renovate'],
    intro: t('The Renovate block: the Dependency Dashboard issue of its repository’s project (Repositories › Blocks that read a repository), and what runs Renovate, found by their names: a pipeline schedule on GitLab; on GitHub, a workflow, or the Renovate app (its dashboard’s run again box).'),
    fields: [
      { key: 'enabled', label: t('Renovate'), hint: t('Off by default.'), kind: 'toggle' },
      { key: 'dashboard_title', label: t('Dashboard issue'), hint: t('Title of the issue Renovate keeps up to date.'), kind: 'text', placeholder: 'Dependency Dashboard' },
      { key: 'schedule', label: t('Schedule'), hint: t('Description of the pipeline schedule (GitLab), or name of the workflow (GitHub), running Renovate: its Run now button. Empty: the first naming Renovate.'), kind: 'text', placeholder: 'Renovate (Daily)' },
    ],
  },
  {
    id: 'audio', label: t('Audio'), table: '[audio]', path: ['audio'],
    intro: t('Sources of the Audio button of the status bar. A list given here replaces the built-in one.'),
    fields: [
      { key: 'stations', label: t('Radio stations'), kind: 'table', blank: { label: '', url: '' }, columns: [{ key: 'label', label: t('Label'), kind: 'text', width: '180px' }, { key: 'url', label: t('Stream URL'), kind: 'text' }] },
      { key: 'youtube', label: t('YouTube'), hint: t('Video, live or playlist.'), kind: 'table', blank: { label: '', url: '' }, columns: [{ key: 'label', label: t('Label'), kind: 'text', width: '180px' }, { key: 'url', label: t('URL'), kind: 'text' }] },
      { key: 'feeds', label: t('Podcasts'), hint: t('RSS feeds with an audio file per episode.'), kind: 'table', blank: { label: '', url: '' }, columns: [{ key: 'label', label: t('Label'), kind: 'text', width: '180px' }, { key: 'url', label: t('Feed URL'), kind: 'text' }] },
    ],
  },
  // The blocks with a source of their own: their config tables.
  ...BLOCK_DEFS.flatMap((b) => b.settings?.sections ?? []),
]

/** Fixed choices of some lists. */
export const FIXED: Partial<Record<Source, string[]>> = {
  stages: ['release', 'preprod', 'prod'],
}
