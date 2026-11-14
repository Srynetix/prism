import type { MrRef } from './mr-ref'
import { Channel, hasCore, invoke, isDesktop } from './bridge'
import { closeSettings, saveNow, settingsView } from './settings.svelte'
import { closeHelp, helpView } from './help-view.svelte'
import { pageTransition } from './page-transition'
import { used } from './feature-use.svelte'
import { live } from './live.svelte'
import { toast } from './toast.svelte'
import { trackerName } from './tracker'
import { isMac, baseName } from './platform'
import { matches, parseShortcut } from './shortcut'
import type { Agent, AgentProvider, AgentsSettings } from './types'
import type { LaunchStep } from './bindings/LaunchStep'
import type { AgentContextDetail } from './bindings/AgentContextDetail'
import type { AgentActivity } from './bindings/AgentActivity'
import type { AgentConversation } from './bindings/AgentConversation'
import type { SlashCommand } from './bindings/SlashCommand'
import type { AgentShellInfo } from './bindings/AgentShellInfo'
import type { TaskSource } from './bindings/TaskSource'
import type { AgentFileListing } from './bindings/AgentFileListing'
import type { AgentDiff } from './bindings/AgentDiff'
import type { FilePreview } from './bindings/FilePreview'
import type { AgentDownload } from './bindings/AgentDownload'
import type { AgentArtifact } from './bindings/AgentArtifact'
import type { AgentPage } from './bindings/AgentPage'
import { t } from './i18n.svelte'
import type { WorktreePlan } from './bindings/WorktreePlan'
export type { WorktreePlan }

/**
 * Agents view: Claude Code sessions that Prism launches
 * in its tmux server, and the terminal of the one being watched. Like Settings, it takes the
 * place of the grid without changing the current view.
 */
export const agentsView = $state({
  open: false,
  selected: null as string | null,
  creating: false,
  /** Launch proposed by an action, which the launcher picks up when it opens. */
  draft: null as Launch | null,
  /** The same job in Claude Desktop, offered as a fallback in the launcher. */
  desktopHref: null as string | null,
  /** The agent the launcher's draft repeats (Launch again), which it says. */
  sameAs: null as string | null,
  /** The prepared agent the launcher edits: replaced once launched or prepared again. */
  replacing: null as string | null,
  /** To do once the agent is launched (or Claude Desktop opened): move the ticket to Validation… */
  onLaunch: null as (() => Promise<void>) | null,
  /** The launcher's short form, asked by a project's +. */
  quick: false,
  /** A side panel to open on the selected agent, asked from outside (a block): taken by the view. */
  side: null as SideTab | null,
  /** Several agents side by side (set by the view): ⌥+arrows then move between them, not in the terminal. */
  split: false,
})

export type AgentsLayout = '1' | '2' | '2r' | '4'
/**
 * The layout chord: ⌃1, ⌃2, ⌃4 on a Mac; Ctrl+Alt+1… elsewhere, where Ctrl+digit changes page.
 * Read from the physical key, whatever the keyboard layout.
 */
export function layoutKey(e: KeyboardEvent): '1' | '2' | '4' | null {
  const mods = isMac ? e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey : e.ctrlKey && e.altKey && !e.metaKey && !e.shiftKey
  const d = /^Digit([124])$/.exec(e.code)?.[1]
  return mods && d ? (d as '1' | '2' | '4') : null
}
/** ⌥+arrow while the view is split: the focus goes to the agent on that side. */
export function paneArrow(e: KeyboardEvent): 'ArrowLeft' | 'ArrowRight' | 'ArrowUp' | 'ArrowDown' | null {
  if (!agentsView.split || !e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return null
  return ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key) ? (e.key as 'ArrowLeft') : null
}

/**
 * Opens the Agents view, a page after the rail's others: from a page, it slides in as they do;
 * from Settings or Help, at once.
 */
export function openAgents(id?: string) {
  if (id) agentsView.selected = id
  if (agentsView.open && !settingsView.open && !helpView.open) return
  const fromPage = !agentsView.open && !settingsView.open && !helpView.open
  used('agents:view')
  const apply = () => {
    closeSettings()
    closeHelp()
    projectsView.open = false
    agentsView.open = true
  }
  if (fromPage) void pageTransition(apply, 'down')
  else apply()
}
/** Closes the Agents view, and the Projects page: a page of the rail, Settings or Help takes their place. */
export const closeAgents = () => {
  agentsView.open = false
  projectsView.open = false
}

/**
 * The Projects page (`ProjectsView.svelte`): every repository and its workstreams, one project shown
 * (`repo`), or the Cleanup tab over them all. Like the Agents view, it takes the grid's place.
 */
export const projectsView = $state({
  open: false,
  repo: null as string | null,
  tab: 'project' as 'project' | 'cleanup',
})

/** The Projects page is on the rail once a repository is configured. */
export const projectsOn = () => (live.snapshot?.repositories?.length ?? 0) > 0
/** Its ⌘ digit: after the `pages`, and after the Agents view when agents are on. */
export const projectsDigit = (pages: number) => pages + (agentsEnabled() ? 2 : 1)

/** Opens the Projects page, on `repo` when given, on its Cleanup tab with `cleanup`. */
export function openProjects(repo?: string | null, cleanup = false) {
  if (repo) projectsView.repo = repo
  projectsView.tab = cleanup ? 'cleanup' : 'project'
  if (projectsView.open && !settingsView.open && !helpView.open) return
  const fromPage = !agentsView.open && !projectsView.open && !settingsView.open && !helpView.open
  used('projects:view')
  const apply = () => {
    closeSettings()
    closeHelp()
    agentsView.open = false
    projectsView.open = true
  }
  if (fromPage) void pageTransition(apply, 'down')
  else apply()
}
/** Opens the agent on its shell `n` (the Shell panel). */
export function openAgentShell(id: string, n: number) {
  shellTabs[id] = n
  agentsView.side = 'shell'
  openAgents(id)
}
/** Opens the launcher prefilled by an action (title, prompt, place). */
/**
 * What a launcher reads its draft from and gives back to: the Agents view's own (`agentsView`), or
 * the drawer's (`launcherDrawer`). Each consumes its own draft only.
 */
export interface LaunchSlot {
  draft: Launch | null
  desktopHref: string | null
  sameAs: string | null
  replacing: string | null
  onLaunch: (() => Promise<void>) | null
  /** Opened from a project's +: the short form (its prompt first, the rest under More options). */
  quick?: boolean
}

/**
 * The launcher as a drawer, over the panel an action came from (a ticket, an MR, a worktree…):
 * that panel stays behind, the view stays where it is; once launched, a toast offers to open the
 * agent. `origin`: what it came from, for its ← (a ticket's id, an MR's number).
 */
export const launcherDrawer = $state<LaunchSlot & { open: boolean; origin: string }>({ open: false, origin: '', draft: null, desktopHref: null, sameAs: null, replacing: null, onLaunch: null, quick: false })
export const closeLauncherDrawer = () => (launcherDrawer.open = false)

/** An agent launched (or prepared) from outside the Agents view: said, with a way to it. */
export function toastLaunched(id: string, title: string, prepared = false) {
  toast(t(prepared ? 'Agent prepared: {title}' : 'Agent launched: {title}', { title: title || t('Agent') }), 6000, { label: t('Open it'), run: () => openAgents(id) })
}

/**
 * An agent launched at once, without the launcher. On a phone it opens; in the window, where it was
 * launched from stays (a toast opens it), unless that is the Agents view: it is picked there. Its
 * id, or `null` when it failed (said in a toast).
 */
export async function launchNow(launch: Launch): Promise<string | null> {
  try {
    const id = await spawnAgent(launch)
    if (isDesktop() && !(agentsView.open && !settingsView.open && !helpView.open)) toastLaunched(id, launch.title)
    else openAgents(id)
    return id
  } catch (e) {
    toast(String(e))
    return null
  }
}

/**
 * The launcher, prefilled: in the Agents view while it is the one shown (its New, ⌘N, an agent's
 * Launch again…), otherwise as a drawer over what is open, which the launch leaves in place. On a
 * phone, always the drawer's slot, shown as the phone's own sheet (`remote/PhoneLauncher.svelte`).
 */
export function openLauncher(draft: Launch, desktopHref?: string | null, onLaunch?: (() => Promise<void>) | null, origin?: string, quick = false) {
  const slot: LaunchSlot = { draft, sameAs: null, replacing: null, desktopHref: desktopHref ?? null, onLaunch: onLaunch ?? null, quick }
  if (!agentsView.open || !isDesktop()) {
    Object.assign(launcherDrawer, slot, { open: true, origin: origin ?? '' })
    return
  }
  openAgents()
  agentsView.creating = true
  Object.assign(agentsView, slot)
}

/**
 * The effect of an action, once launched or prepared (the ticket moved to Validation and assigned
 * to me…): only once, never blocking the agent. Its failure stays in sight a while, with Retry:
 * `done` says what happened before it (`Agent started`).
 */
export async function afterLaunch(onLaunch: (() => Promise<void>) | null | undefined, done: string) {
  if (!onLaunch) return
  try {
    await onLaunch()
  } catch (e) {
    console.error('action effect:', e)
    toast(t('{done}, but {tracker} was not updated: {error}', { done, tracker: trackerName(), error: e instanceof Error ? e.message : String(e) }), 20_000, { label: t('Retry'), run: () => void afterLaunch(onLaunch, done) })
  }
}

/** Agents only run in the app: the browser preview has no tmux server. */
export const agentsAvailable = hasCore

const DEFAULTS: AgentsSettings = { enabled: true, provider: 'claude', target: 'agent', worktrees_dir: null, branch_prefix: 'agent/', permission_mode: 'default', font_size: 12.5, chat_font_size: 13.5, scrollback: 5000, accounts: [], kinds: [{ name: 'Refinement', when: 'refine' }, { name: 'Development', when: 'implement' }, { name: 'Review', when: 'review' }], tags: [], prompt_language: 'en', tasks_depth: 3, fix_log_lines: 60, claude: { bin: null, model: null, effort: null, args: [], cost_alert: 0, show_desktop: true, count_desktop: false, sessions_dir: '', transcripts: '', sessions_poll: 5, refresh_token: true }, 'command-code': { bin: null, model: null, effort: null, args: [] }, opencode: { bin: null, model: null, effort: null, args: [] }, codex: { bin: null, model: null, effort: null, args: [] }, quick_answers: { enabled: true, answers: [] }, quick_actions: [], routines: [], overlaps: { enabled: true, level: 'file', alert: 'lines', ignore: [] }, aside: { enabled: true, desktop: true, default_action: 'launch', worktree: true, stale_days: 14, statusbar: false }, assistant: { enabled: true, explore_seconds: 240 }, terminal_keys: ['Cmd+K'], view: 'chat', fold_steps: true, chat_reminders: 'marker', header_meta: 'read', comment_answers: true, suggest_switch: true, handoff: 'prism', handoff_turns: 8, handoff_review: true, prism_mcp: false }
/** `[agents]` from config.toml, with its defaults until the snapshot has brought it. */
export const agentsSettings = (): AgentsSettings => live.snapshot?.agents_settings ?? DEFAULTS
/**
 * A shortcut of Prism's that an agent's terminal keeps for itself (`[agents] terminal_keys`,
 * `Cmd+K` by default: it clears the terminal): pressed there, Prism leaves it alone.
 */
export const terminalKeeps = (e: KeyboardEvent) => agentsSettings().terminal_keys.some((k) => matches(e, parseShortcut(k)))
/**
 * Terminal text size (`[agents] font_size`), and its keyboard adjustment (⌘+ / ⌘- / ⌘0
 * in the Agents view): applied right away to all terminals, saved a moment after the
 * last key.
 */
export const TERMINAL_FONT = { min: 8, max: 28, step: 0.5 }
const termFont = $state({ size: null as number | null })
export const terminalFontSize = () => termFont.size ?? agentsSettings().font_size
let saving: ReturnType<typeof setTimeout> | undefined
/** `delta` in points; `0`: the default size. */
export function zoomTerminal(delta: number) {
  const { min, max, step } = TERMINAL_FONT
  const next = delta === 0 ? DEFAULTS.font_size : Math.min(max, Math.max(min, Math.round((terminalFontSize() + delta) / step) * step))
  termFont.size = next
  clearTimeout(saving)
  saving = setTimeout(() => void saveNow(['agents', 'font_size'], next).finally(() => (termFont.size = null)), 600)
}

/**
 * Chat text size (`[agents] chat_font_size`), the same way: ⌘+ / ⌘- / ⌘0 in the Agents view while
 * the agent shown reads as its Chat.
 */
export const CHAT_FONT = { min: 10, max: 24, step: 0.5 }
const chatFont = $state({ size: null as number | null })
export const chatFontSize = () => chatFont.size ?? agentsSettings().chat_font_size ?? DEFAULTS.chat_font_size
let savingChat: ReturnType<typeof setTimeout> | undefined
/** `delta` in points; `0`: the default size. */
export function zoomChat(delta: number) {
  const { min, max, step } = CHAT_FONT
  const next = delta === 0 ? DEFAULTS.chat_font_size : Math.min(max, Math.max(min, Math.round((chatFontSize() + delta) / step) * step))
  chatFont.size = next
  clearTimeout(savingChat)
  savingChat = setTimeout(() => void saveNow(['agents', 'chat_font_size'], next).finally(() => (chatFont.size = null)), 600)
}

/** The Agents view is there: in the app, and `[agents] enabled`. */
export const agentsEnabled = () => hasCore() && agentsSettings().enabled

/** What a launch requires: a folder, or a new worktree (name, existing branch of an MR). */
export interface Launch {
  title: string
  cwd?: string
  prompt?: string
  /**
   * A new worktree; `repo`: its repository (`[[repos]] name`), absent: the first with worktrees;
   * `base`: the branch on origin its new branch starts from, absent: the repository's base;
   * `also`: other repositories given a worktree of the same name, the agent working in all of them.
   */
  worktree?: { name?: string; branch?: string; repo?: string; base?: string; also?: string[] }
  /** `acceptEdits`, `plan`; absent, the mode from the CLI's settings. */
  permission_mode?: string
  /** One of the `[[agents.kinds]]`; absent, the one its prompt's command names. */
  category?: string
  /** Its first tags (free, several). */
  tags?: string[]
  /** Its CLI; absent, its repository's (`[[repos]] agent`), failing that `[agents] provider`. */
  provider?: AgentProvider
  /** Its CLI's account (`[[agents.accounts]]`, its name); absent, the CLI's default one. */
  account?: string
  /** Its model, kept on resume; absent, its CLI's (`[agents.<cli>] model`). */
  model?: string
  /** Its reasoning effort, kept on resume; absent, its CLI's (`[agents.<cli>] effort`). */
  effort?: string
  /** Its "Then" steps, kept with the launch (Prism runs them). */
  then?: LaunchStep[]
  /** The action that opened the launcher (`Implement`…), kept with the launch. */
  from?: string
  /** Prism's MCP server and skill given to it whatever `[agents] prism_mcp` says (Configure Prism). */
  prism_mcp?: boolean
}

/**
 * The launch an agent was started with, to start the same again: a new worktree as asked then (the
 * next free name), or the folder it was given while it still exists; failing that, a new worktree
 * of that folder's repository. Its "Then" steps come back, not how they went.
 */
export function launchAgain(a: Agent): Launch {
  // Launched before Prism kept it: what the agent still says.
  const l = a.launch ?? { title: a.title, cwd: a.cwd, prompt: a.prompt, worktree: null, permission_mode: null, category: a.category, tags: a.tags, provider: a.provider, account: a.account, model: null, effort: null, then: [], from: null }
  const s = live.snapshot
  const opt = <T>(v: T | null | undefined) => v ?? undefined
  const base: Launch = { title: l.title, prompt: opt(l.prompt), permission_mode: opt(l.permission_mode), category: opt(l.category), tags: l.tags.length ? l.tags : undefined, provider: opt(l.provider), account: opt(l.account), model: opt(l.model), effort: opt(l.effort), then: l.then.length ? l.then : undefined, from: l.from?.startsWith('aside:') ? undefined : opt(l.from) }
  if (l.worktree) return { ...base, worktree: { repo: opt(l.worktree.repo), name: opt(l.worktree.name), branch: opt(l.worktree.branch), base: opt(l.worktree.base) } }
  const known = !!l.cwd && (!!s?.repos?.some((r) => r.path === l.cwd) || !!s?.worktrees?.some((w) => w.path === l.cwd && !w.prunable))
  if (known) return { ...base, cwd: l.cwd! }
  const repo = s?.worktrees?.find((w) => w.path === l.cwd)?.repo
  return { ...base, worktree: { repo } }
}
/**
 * The agent just launched: the snapshot only lists it a moment later. Until then (15 s at most)
 * it stays the selected one, rather than the view falling back to the first of the list.
 */
let launched: { id: string; at: number } | null = null
export const planWorktree = (title: string, worktree: NonNullable<Launch['worktree']>) =>
  invoke<WorktreePlan | null>('agent_worktree_plan', { title, worktree })

/** `quiet`: an agent working for Prism (a quick action's draft), not picked as just launched. */
export async function spawnAgent(launch: Launch, quiet = false): Promise<string> {
  const id = await invoke<string>('agent_spawn', { launch })
  if (!quiet) launched = { id, at: Date.now() }
  return id
}
/** Declares an agent without starting it (Prepare): it waits in the list for Start. */
export async function prepareAgent(launch: Launch): Promise<string> {
  const id = await invoke<string>('agent_prepare', { launch })
  launched = { id, at: Date.now() }
  return id
}
/** Starts a prepared agent, under the same id. */
export const startPrepared = (id: string) => invoke('agent_start', { id })
export const justLaunched = (id: string | null) => !!id && launched?.id === id && Date.now() - launched.at < 15_000
export const removeWorktree = (id: string) => invoke('agent_remove_worktree', { id })
export const interruptAgent = (id: string) => invoke('agent_interrupt', { id })
/** Takes a terminal (`id`: an agent or `shell:<agent>:<n>`) out of tmux's copy mode, before a key. */
export const leaveCopyMode = (id: string) => invoke('agent_leave_copy_mode', { id })
/** tmux's copy mode, read: the pane's view stays still until it leaves it. */
export const inCopyMode = (id: string) => invoke<boolean>('agent_in_copy_mode', { id })
/** Also opens the agent in the `[open_with]` terminal. */
export const openExternal = (id: string) => invoke('agent_open_external', { id })

/** Same rule as the Rust core (`worktree::slug`), for the launcher's preview. */
export function slug(text: string): string {
  return text
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+/, '')
    .slice(0, 40)
    .replace(/-+$/, '')
}
export const resumeAgent = (id: string) => invoke('agent_resume', { id })
/** Restarts Claude on its conversation (`claude --resume`), even mid-work. */
export const restartAgent = (id: string) => invoke('agent_restart', { id })
export const stopAgent = (id: string) => invoke('agent_stop', { id })
/** Stops the agent if it runs, closes it and removes its worktree: why the worktree stayed, if it did. */
export const closeAgent = (id: string) => invoke<string | null>('agent_close', { id })
/** Opens the agent's folder (its worktree, or its folder) in the editor, the terminal or the Finder. */
export const openAgentFolder = (id: string, target: 'editor' | 'terminal' | 'finder') => invoke('agent_open', { id, with: target })
/** An agent's panels' types, generated from the Rust ones (crates/agents). */
export type { AgentContextDetail } from './bindings/AgentContextDetail'
export type { AgentActivity } from './bindings/AgentActivity'
export type { ConversationTool } from './bindings/ConversationTool'
export type { AgentQuestion } from './bindings/AgentQuestion'
export type { ProposedPlan } from './bindings/ProposedPlan'
export type { ConversationEntry } from './bindings/ConversationEntry'
export type { ConversationTurn } from './bindings/ConversationTurn'
export type { AgentConversation } from './bindings/AgentConversation'
export type { BackgroundTask } from './bindings/BackgroundTask'
export type { SlashCommand } from './bindings/SlashCommand'
export type { AgentShellInfo } from './bindings/AgentShellInfo'
export type { TaskSource } from './bindings/TaskSource'
export type { AgentTask } from './bindings/AgentTask'
export type { AgentFile } from './bindings/AgentFile'
export type { AgentFileListing } from './bindings/AgentFileListing'
export type { AgentDiff } from './bindings/AgentDiff'
export type { FilePreview } from './bindings/FilePreview'
export type { AgentArtifact } from './bindings/AgentArtifact'
export type { AgentPage } from './bindings/AgentPage'
export type { PlanStep } from './bindings/PlanStep'
export type { LiveStatus } from './bindings/LiveStatus'

/** Tabs of an agent's right panel. */
export type SideTab = 'files' | 'diff' | 'web' | 'tasks' | 'shell' | 'activity' | 'launch' | 'notes' | 'plan'

/** What an agent did (Activity tab), from its hook log: metadata only. */
export const agentActivity = (id: string) => invoke<AgentActivity>('agent_activity', { id })
/** A model as the lists show it: `claude-opus-5-5` → `opus-5-5`, the others as their CLI names them. */
export const modelName = (m: string) => m.replace(/^claude-/, '')

/** An answer to one of its questions: the choices picked (from 1), and your own words. */
export interface QuestionAnswer {
  picks: number[]
  text?: string
}
/** Answers its question, as its menu's keys; from a phone, your own words ask its passkey. */
export const answerAgent = (id: string, call: string, answers: QuestionAnswer[]) => invoke('agent_answer', { id, call, answers })
/** Its Plan tab is offered: it has proposed a plan, or was launched to plan first. */
export const plans = (a: Agent) => a.planned || a.launch?.permission_mode === 'plan'
/** Answers its plan (`call`, its `ExitPlanMode`) through its menu: approved, or with your words (refused: it plans again; or approved with them). */
export const answerPlan = (id: string, call: string, how: 'approve' | 'approve_each' | 'approve_auto' | 'changes' | 'approve_with', text?: string) => invoke('agent_plan_answer', { id, call, how, text: text ?? null })
/**
 * Sets its reasoning effort (`null`: its CLI's), kept for its resumes; true when it took it at
 * once (Claude Code at its prompt), false when it will from its next resume.
 */
export const setAgentEffort = (id: string, effort: string | null) => invoke<boolean>('agent_set_effort', { id, effort })
/**
 * Its last `turns`; null for a CLI without a transcript, or before it has one. `have`: the `sig` of
 * the turns held, which come back as their number only (`kept`), unchanged.
 */
export const agentConversation = (id: string, turns: number, have: string[] = []) => invoke<AgentConversation | null>('agent_conversation', { id, turns, have })
/** Its context's detail (Claude Code, Codex); null for another CLI or before a first reply. */
export const agentContextDetail = (id: string) => invoke<AgentContextDetail | null>('agent_context_detail', { id })
/** The slash commands its CLI takes, for its Chat box's autocomplete (Claude Code asked, about a second). */
export const agentSlashCommands = (id: string) => invoke<SlashCommand[]>('agent_slash_commands', { id })
/** Compacts its conversation (`/compact`), refused while it works. */
export const compactAgent = (id: string) => invoke('agent_compact', { id })
/** Clears its conversation (`/clear`) once its turn is over: the transcript stays, the agent starts afresh. */
export const clearAgent = (id: string) => invoke('agent_clear', { id })
/**
 * Files dropped or pasted on a prompt, saved where the agent reads them: a running agent's (`id`)
 * in its folder's `.prism/attach/` (paths relative to it), the launcher's in Prism's working folder
 * (absolute paths). Returns their paths, in order.
 */
export async function attachFiles(files: File[], id: string | null): Promise<string[]> {
  const out: string[] = []
  for (const original of files) {
    const f = await fitImage(original)
    out.push(await invoke<string>('agent_attach_file', { id, name: f.name || 'pasted.png', data: await base64(f) }))
  }
  return out
}
/** A file's bytes in base64: a third more than raw, where an array of numbers is three and a half times. */
function base64(f: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader()
    r.onload = () => resolve(String(r.result).slice(String(r.result).indexOf(',') + 1))
    r.onerror = () => reject(r.error)
    r.readAsDataURL(f)
  })
}
/** Images an agent reads as they are. */
const READ = ['image/png', 'image/jpeg', 'image/gif', 'image/webp']
const FIT_SIDE = 2048
const FIT_BYTES = 1_500_000
/**
 * An image an agent would not read (a phone's HEIC) or a heavy one (a phone's photo, a large
 * screenshot): a JPEG of 2048 pixels at most, which it reads as well and a phone's socket carries.
 * Any other file, or one the webview cannot decode, as it is.
 */
export async function fitImage(f: File): Promise<File> {
  if (!f.type.startsWith('image/') || f.type === 'image/gif' || f.type === 'image/svg+xml') return f
  if (READ.includes(f.type) && f.size <= FIT_BYTES) return f
  try {
    const bitmap = await createImageBitmap(f)
    const scale = Math.min(1, FIT_SIDE / Math.max(bitmap.width, bitmap.height))
    const canvas = document.createElement('canvas')
    canvas.width = Math.round(bitmap.width * scale)
    canvas.height = Math.round(bitmap.height * scale)
    canvas.getContext('2d')?.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
    bitmap.close()
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/jpeg', 0.85))
    if (!blob) return f
    const name = (f.name || 'image').replace(/\.[^.]*$/, '') + '.jpg'
    return new File([blob], name, { type: 'image/jpeg' })
  } catch {
    return f
  }
}
/**
 * Images chosen for an agent's Chat box: saved where it reads them, their `@path` added to its
 * draft (`text`, `write`), sent with it; the box's strip shows them from there.
 */
export async function attachToDraft(id: string, files: File[], text: string, write: (t: string) => void): Promise<void> {
  const paths = await attachFiles(files, id)
  if (paths.length) write(`${text.trimEnd()} ${paths.map(mention).join(' ')} `.trimStart())
}
/** The draft without that image's `@path`. */
export const withoutMention = (text: string, path: string) => text.replace(mention(path), '').replace(/ {2,}/g, ' ').replace(/^ /, '')
/** A path as Claude Code reads it in a prompt: `@path`, quoted when it has a space. */
export const mention = (path: string) => (/\s/.test(path) ? `@"${path}"` : `@${path}`)
/** The files a drag carries (not text, not an element of the page). */
/** The files of a drop, a paste or a file picker. */
export const filesOf = (list: FileList | null | undefined) => [...(list ?? [])]
export const draggedFiles = (e: DragEvent) => !!e.dataTransfer && [...e.dataTransfer.types].includes('Files')

/** Types `text` in the agent's prompt, without submitting it. */
export const insertInAgent = (id: string, text: string) => invoke('agent_prompt', { id, text, enter: false })
/** Sends `text` to the agent as a prompt (`enter`), or only types it in its prompt. */
export const promptAgent = (id: string, text: string, enter: boolean) => invoke('agent_prompt', { id, text, enter })
/** One of an agent's shells: login shells in its folder, kept by tmux like the agent. */
export const agentShells = (id: string) => invoke<AgentShellInfo[]>('agent_shells', { id })
/** Opens the agent's shell `n` (again after an `exit`), or a new one (`null`): the key its terminal attaches to. */
export const agentShell = (id: string, n: number | null) => invoke<string>('agent_shell', { id, n })
export const closeShell = (id: string, n: number) => invoke('agent_close_shell', { id, n })
/** Whose shells the Shells pane shows: an agent's, or a worktree's (shell-drawer.svelte.ts). */
export interface ShellOwner {
  /** Its key in `shellTabs`. */
  id: string
  title: string
  folder: string
  list: () => Promise<AgentShellInfo[]>
  open: (n: number | null) => Promise<string>
  close: (n: number) => Promise<unknown>
}
export const agentShellOwner = (a: Agent): ShellOwner => ({
  id: a.id,
  title: a.title,
  folder: a.worktree ?? a.cwd,
  list: () => agentShells(a.id),
  open: (n) => agentShell(a.id, n),
  close: (n) => closeShell(a.id, n),
})
/** The shell shown in each agent's (or worktree's) Shells pane, by number: where Tasks run. */
export const shellTabs = $state<Record<string, number>>({})
export const shellNumber = (key: string) => Number(key.split(':').pop())
export const agentTasks = (id: string) => invoke<TaskSource[]>('agent_tasks', { id })

/**
 * Types a command line in the agent's shell `n`, or a new one (`null`), and runs it unless `enter`
 * is false. `interrupt`: Ctrl-C a busy shell first. Returns the shell's key.
 */
export const runInShell = (id: string, n: number | null, command: string, opts: { enter?: boolean; interrupt?: boolean } = {}) => invoke<string>('agent_run', { id, n, command, enter: opts.enter ?? true, interrupt: opts.interrupt ?? false })
/** `ignored`: also the files `.gitignore` leaves out (their heavy folders, dependencies and caches, not walked). */
export const agentFiles = (id: string, ignored = false) => invoke<AgentFileListing>('agent_files', { id, ignored })
/**
 * Files of the agent's folder downloaded: one file as it is, several or a folder (`dir/`) as
 * `<name>.zip`. On the computer, saved where its dialog says (Downloads first); on a phone, prepared
 * by the computer and fetched once from the address it gives. Its name once saved; null: cancelled.
 */
export async function downloadAgentFiles(id: string, paths: string[], name: string): Promise<string | null> {
  const single = paths.length === 1 && !paths[0].endsWith('/')
  const file = single ? paths[0].split('/').pop()! : `${name}.zip`
  if (isDesktop()) {
    const [{ save }, { downloadDir, join }] = await Promise.all([import('@tauri-apps/plugin-dialog'), import('@tauri-apps/api/path')])
    const dest = await save({ defaultPath: await join(await downloadDir(), file) })
    if (!dest) return null
    await invoke('agent_files_export', { id, paths, name, dest })
    return baseName(dest) || file
  }
  // Prepared on the computer, fetched once from its address over HTTP, streamed into the phone's downloads.
  const d = await invoke<AgentDownload>('agent_files_download', { id, paths, name })
  const link = Object.assign(document.createElement('a'), { href: d.url, download: d.name })
  document.body.append(link)
  link.click()
  link.remove()
  return d.name
}
/** Uncommitted (against HEAD), or the whole branch (`branch`: against its base). */
export const agentDiff = (id: string, branch: boolean) => invoke<AgentDiff>('agent_diff', { id, branch })
/** A file of an agent's diff, whole, from its working tree: the Markdown preview. */
export const agentFileText = (id: string, path: string) => invoke<string>('agent_file_text', { id, path })
/**
 * An image of its diff as a `data:` URL: from its working tree, or (`old`) as it is in what the diff
 * compares it to. `version` (what it is read from: a revision, the file's stamp) keeps it among the
 * images read last: the diff view is drawn again each time a file of it changes, and an 8 MB image
 * read again was 60 ms of git and 20 MB sent.
 */
export const agentDiffImage = (id: string, path: string, branch: boolean, old: boolean, version?: string | null) => {
  const read = () => invoke<string>('agent_diff_image', { id, path, branch, old })
  return version ? once(`${id}\ndiff:${branch}:${old}:${version}:${path}`, read) : read()
}
/** A file of the agent's folder for the Files preview (path relative to the folder, as listed). */
export const agentFilePreview = (id: string, path: string) => invoke<FilePreview>('agent_file_preview', { id, path })
/** An image attached to one of its prompts (its path as typed there), as a `data:` URL; once per path. */
export const agentAttachment = (id: string, path: string) => once(`${id}\n${path}`, () => invoke<string>('agent_attachment', { id, path }))
/** Image `n` pasted in one of its prompts (`ConversationTurn.pasted`), as a `data:` URL; once per image. */
export const agentPastedImage = (id: string, line: number, n: number) => once(`${id}\n${line}:${n}`, () => invoke<string>('agent_pasted_image', { id, line, n }))
/** The images read last, the oldest out past `IMAGES` (an attachment can be 10 MB as a `data:` URL). */
const images = new Map<string, Promise<string>>()
const IMAGES = 24
function once(key: string, load: () => Promise<string>): Promise<string> {
  let p = images.get(key)
  if (p) images.delete(key)
  else {
    p = load()
    // Not there (yet, or any more): asked again next time.
    p.catch(() => images.delete(key))
  }
  images.set(key, p)
  for (const old of images.keys()) {
    if (images.size <= IMAGES) break
    images.delete(old)
  }
  return p
}
/** Opens one file of the agent's folder in the `[open_with]` editor. */
export const openAgentFile = (id: string, path: string) => invoke('agent_open_file', { id, path })
export const agentPages = (id: string, ignored: boolean) => invoke<AgentPage[]>('agent_pages', { id, ignored })
export const agentArtifacts = (id: string) => invoke<AgentArtifact[]>('agent_artifacts', { id })
/** Address prefix of an artifact in the Web view (same value as `files::SCRATCHPAD`). */
export const SCRATCHPAD = '@scratchpad'
/** A Markdown or text page of the Web view (`<path>`, or `@scratchpad/<path>`), as `prismfile:` would serve it. */
export const agentPageText = (id: string, path: string) => invoke<string>('agent_page_text', { id, path })
/** A file of the agent's folder, served by the Rust core's `prismfile:` scheme. */
export const agentFileUrl = (id: string, path: string) => `prismfile://localhost/${encodeURIComponent(id)}/${path.split('/').map(encodeURIComponent).join('/')}`

/** Renames the agent in Prism; Claude keeps the launch name. */
export const renameAgent = (id: string, title: string) => invoke('agent_rename', { id, title })
/** Its kind (`null`: none). */
export const setAgentCategory = (id: string, category: string | null) => invoke('agent_set_category', { id, category })
/** The agent's tags, as a whole (trimmed and deduplicated by the core). */
export const setAgentTags = (id: string, tags: string[]) => invoke('agent_set_tags', { id, tags })
/** Links the agent to a ticket by hand; `null`: back to the one found in its title or branch. */
export const linkAgentTicket = (id: string, ticket: string | null) => invoke('agent_link_ticket', { id, ticket })
/** Links the agent to an MR by hand; `null`: back to its branch's. */
export const linkAgentMr = (id: string, mr: MrRef | null) => invoke('agent_link_mr', { id, mr: mr && { repo: mr.repo ?? null, iid: mr.iid } })
export type AgentLabel = 'category' | 'tag'
/** A kind or a tag renamed: the agents carrying it follow (`to` empty: taken off them). */
export const relabelAgents = (kind: AgentLabel, from: string, to: string) => invoke('agent_relabel', { kind, from, to })
/** The kinds' names, in the order of `[[agents.kinds]]`. */
export const kindNames = () => [...new Set((agentsSettings().kinds ?? []).map((k) => k.name).filter(Boolean))]
const KIND_COLORS = ['--violet', '--info', '--ok', '--warn', '--src-clickup', '--src-calendar', '--src-gitlab']
/** A kind's colour, by its place in `[[agents.kinds]]` (one no longer there: muted). */
export function kindColor(name: string): string {
  const i = kindNames().indexOf(name)
  return i < 0 ? 'var(--text-muted)' : `var(${KIND_COLORS[i % KIND_COLORS.length]})`
}
/**
 * The kind an agent launched with this prompt gets (`[[agents.kinds]] when`): the first naming the
 * command it starts with, with or without its plugin. Same rule as the core's `kind_of`.
 */
export function kindOf(prompt: string): string | null {
  const command = /^\s*\/(\S+)/.exec(prompt)?.[1]
  if (!command) return null
  const name = command.split(':').pop()
  const k = (agentsSettings().kinds ?? []).find((k) => k.when.split(/[, ]/).map((w) => w.trim().replace(/^\/+/, '')).some((w) => w && (w === command || w === name)))
  return k?.name ?? null
}
/** Every tag: those suggested by `[agents] tags` in their order, then those in use, A to Z. */
export function allTags(): string[] {
  const used = (live.snapshot?.agents ?? []).flatMap((a) => a.tags ?? [])
  // Once each, even if config.toml repeats one: they key the menus and groups.
  const suggested = [...new Set(agentsSettings().tags ?? [])]
  return [...suggested, ...[...new Set(used)].filter((t) => !suggested.includes(t)).sort((a, b) => a.localeCompare(b))]
}
/** Grants the requested permission once (key 1, "Yes"). */
export const allowAgent = (id: string) => invoke('agent_allow', { id })

/**
 * Link opening a session in the desktop app. The app only accepts `local_<uuid>`
 * identifiers, those of the local Code sessions that Prism reads from disk.
 */
export const desktopUrl = (id: string) => (/^local_[A-Za-z0-9-]{1,64}$/.test(id) ? `claude://code/continue?session=${encodeURIComponent(id)}` : undefined)

/**
 * Attaches a terminal to the agent. Its output arrives as raw bytes (`ArrayBuffer`); an
 * `{ end: true }` object signals the end of the attachment (agent closed, or terminal replaced).
 */
export async function attachAgent(id: string, cols: number, rows: number, onData: (bytes: Uint8Array) => void, onEnd: () => void) {
  const channel = new Channel<ArrayBuffer | { end: true }>()
  channel.onmessage = (m) => (m instanceof ArrayBuffer ? onData(new Uint8Array(m)) : onEnd())
  await invoke('agent_attach', { id, cols, rows, onData: channel })
}
export const detachAgent = (id: string) => invoke('agent_detach', { id }).catch(() => {})
/** Keystrokes and pastes: lost without error if the terminal is not (yet) attached. */
export const writeAgent = (id: string, data: string) => invoke('agent_write', { id, data }).catch(() => {})
export const resizeAgent = (id: string, cols: number, rows: number) => invoke('agent_resize', { id, cols, rows }).catch(() => {})
