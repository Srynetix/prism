import { consoleKey } from './console.svelte'
import { quickNotesKey } from './quick-notes.svelte'
import { parseShortcut, shortcutLabel } from './shortcut'
import { chord, chordKey, extraShift, isMac, keys as forPlatform } from './platform'
import { live } from './live.svelte'
import { views } from './views'
import { agentsEnabled, projectsDigit, projectsOn } from './agents.svelte'
import { t } from './i18n.svelte'
import { assistantOn } from './assistant.svelte'
import { homeShellKey, homeShellOn } from './shell-drawer.svelte'

/**
 * THE keyboard shortcuts' registry: each one declared once, here. App.svelte, keys.svelte.ts and
 * AgentsView.svelte match their chords through `hit(id, e)`; the "?" overlay (KeyHelp.svelte) and
 * Help's reference list them from `shortcutSections()`, so neither can drift from what the keys do.
 *
 * `keys`: what is shown, written the macOS way (`platform.ts` `keys()` adapts it to the system), or
 * computed (the pages' digits, a key set in config.toml). `label`: its English text, the key of
 * `t()`. `os`: only on that system (Linux's own keys); `when`: listed only then (the Agents view on). `match`: the chord it
 * answers to, for those App, keys.svelte.ts or the Agents view look up here; the vim-style keys
 * of the grid (j / k, gg, za…) are matched by keys.svelte.ts' own switch, declared here for the help.
 */
export type ShortcutGroup = 'Move' | 'Row' | 'Side panel' | 'Block and view' | 'Agents' | 'Everywhere'
export interface ShortcutDef {
  id: string
  keys: string | (() => string)
  label: string
  group: ShortcutGroup
  /** Only on macOS, or only elsewhere (Linux, Windows). */
  os?: 'mac' | 'other'
  when?: () => boolean
  match?: (e: KeyboardEvent) => boolean
}

/** The command palette's key (`[palette] key`, ⌘K by default). */
export const paletteKey = () => parseShortcut(live.snapshot?.palette?.key || 'Cmd+K')

/** ⌘ + `k` (Ctrl elsewhere), without a Shift of its own (a Shift only added to leave the terminal does not count). */
const cmd = (k: string) => (e: KeyboardEvent) => chord(e) && !e.altKey && !(e.shiftKey && !extraShift(e)) && chordKey(e) === k
/**
 * A size step from a chord's key: + (`1`), − (`-1`), 0 (`0`), whatever the layout. By the character
 * typed, else by the physical key: ⌥ changes the character on a Mac (⌥= types ≠), and 0 is the à
 * key unshifted on an AZERTY keyboard. Elsewhere, Ctrl+Alt is also AltGr (Windows): only the
 * character typed counts then, so AltGr+à (@) stays a character.
 */
export function sizeStep(e: KeyboardEvent): 1 | -1 | 0 | null {
  const k = chordKey(e)
  if (k === '=' || k === '+') return 1
  if (k === '-' || k === '_') return -1
  if (k === '0') return 0
  if (e.altKey && !isMac) return null
  if (e.code === 'NumpadAdd') return 1
  if (e.code === 'NumpadSubtract') return -1
  // ⌘⇧0 on a US keyboard types ): not a reset.
  if ((e.code === 'Digit0' || e.code === 'Numpad0') && !e.shiftKey) return 0
  if (e.altKey) {
    if (k === '≠' || k === '±' || e.code === 'Equal') return 1
    if (k === '–' || k === '—' || e.code === 'Minus') return -1
  }
  return null
}
/** ⌘⇧ + `k`. */
const cmdShift = (k: string) => (e: KeyboardEvent) => chord(e) && e.shiftKey && chordKey(e).toLowerCase() === k

/** The Agents view's digit: after the pages (⌘5 with four). */
export const agentsDigit = () => String(views().length + 1)
/** The Projects page's: after the Agents view when it is on. */
export const projectsKey = () => String(projectsDigit(views().length))

const pagesKeys = (mod: string) => () => (views().length > 1 ? `${mod}1 … ${mod}${views().length}` : `${mod}1`)

export const REGISTRY: ShortcutDef[] = [
  // Move
  { id: 'row', keys: 'j / k', label: 'next / previous row (then the next block of the column)', group: 'Move' },
  { id: 'column', keys: 'h / l', label: 'block in the column on the left / right', group: 'Move' },
  { id: 'block', keys: 'J / K, ⇧↓ / ⇧↑', label: 'next / previous block', group: 'Move' },
  { id: 'ends', keys: 'gg / G', label: 'first / last row', group: 'Move' },
  { id: 'scroll', keys: 'Ctrl-d / Ctrl-u', label: '10 rows down / up', group: 'Move' },
  // Row
  { id: 'open', keys: 'Enter / o', label: 'open: preview, drawer or main link', group: 'Row' },
  { id: 'browser', keys: 'O', label: 'open in the browser (ClickUp, GitLab, GitHub)', group: 'Row' },
  { id: 'action', keys: 'a', label: 'run the row action (Assign me, Fix, Review…, or an Alerts quick fix)', group: 'Row' },
  { id: 'menu', keys: '.', label: 'actions menu ⋯ (j / k, Enter, Esc)', group: 'Row' },
  { id: 'seen', keys: 'x', label: 'mark an Alerts item as seen / new', group: 'Row' },
  { id: 'thread', keys: 't', label: 'thread: its ticket, worktree, agent, MR, pipeline and environments', group: 'Row' },
  // Side panel
  { id: 'pane', keys: 'Ctrl-w l / Tab', label: 'focus the open panel (MR, ticket, logs…)', group: 'Side panel' },
  { id: 'pane-scroll', keys: 'j / k, Ctrl-d / u', label: 'scroll it; gg / G top / bottom', group: 'Side panel' },
  { id: 'pane-next', keys: 'J / K', label: 'next / previous button or link, Enter to use it', group: 'Side panel' },
  { id: 'diff-file', keys: '] / [', label: 'diff: next / previous file', group: 'Side panel' },
  { id: 'diff-message', keys: 'n / N', label: 'diff: next / previous message (thread, or note to the agent)', group: 'Side panel' },
  { id: 'diff-viewed', keys: 'v', label: 'diff: mark the current file as viewed', group: 'Side panel' },
  { id: 'logs-full', keys: 'f', label: 'logs: full screen, or back', group: 'Side panel', match: (e) => e.key === 'f' && !e.metaKey && !e.ctrlKey && !e.altKey },
  { id: 'pane-back', keys: 'Ctrl-w h / Tab / Esc', label: 'back to the blocks (Esc again closes)', group: 'Side panel' },
  // Block and view
  { id: 'reload', keys: 'r', label: 'reload ClickUp now', group: 'Block and view' },
  { id: 'collapse', keys: 'za', label: 'collapse / expand the block', group: 'Block and view' },
  { id: 'short', keys: 'zs', label: 'short / normal', group: 'Block and view' },
  { id: 'zoom', keys: 'zz', label: 'zoom the block', group: 'Block and view' },
  { id: 'page', keys: pagesKeys('⌘'), label: 'page, in the rail’s order (then Agents, Projects)', group: 'Block and view' },
  { id: 'view', keys: pagesKeys(''), label: 'view', group: 'Block and view' },
  { id: 'cycle', keys: 'gt / gT', label: 'next / previous view', group: 'Block and view' },
  { id: 'rename', keys: 'F2', label: 'rename the page shown (in the Agents view: the agent)', group: 'Block and view', match: (e) => e.key === 'F2' && !e.metaKey && !e.ctrlKey && !e.altKey },
  // Agents
  { id: 'agents', keys: () => `⌘${agentsDigit()}`, label: 'Agents view', group: 'Agents', when: () => agentsEnabled() },
  { id: 'projects', keys: () => `⌘${projectsKey()}`, label: 'Projects page', group: 'Agents', when: () => projectsOn() },
  { id: 'new-agent', keys: '⌘N', label: 'new agent', group: 'Agents', when: () => agentsEnabled(), match: cmd('n') },
  { id: 'agent-cycle', keys: '⌘[ / ⌘]', label: 'previous / next agent', group: 'Agents', match: (e) => cmd('[')(e) || cmd(']')(e) },
  { id: 'agents-list', keys: '⌘B', label: 'collapse / expand the agents list', group: 'Agents', match: cmd('b') },
  { id: 'interrupt', keys: '⌘.', label: 'interrupt the agent (Esc)', group: 'Agents', match: cmd('.') },
  { id: 'mark-read', keys: '⌘E', label: 'mark the agent read', group: 'Agents', match: cmd('e') },
  { id: 'agent-zoom', keys: '⌘↵', label: 'side by side: the agent alone, or back', group: 'Agents', match: cmd('enter') },
  { id: 'text-size', keys: '⌘⌥+ / ⌘⌥- / ⌘⌥0', label: 'text size of the terminal or the Chat', group: 'Agents', match: (e) => chord(e) && e.altKey && sizeStep(e) !== null },
  { id: 'agents-layout', keys: () => (isMac ? '⌃1 / ⌃2 / ⌃4' : 'Ctrl+Alt+1 / 2 / 4'), label: 'one, two or four agents side by side', group: 'Agents' },
  { id: 'shift-click', keys: '⇧-click', label: 'an agent action launches at once', group: 'Agents' },
  // Everywhere
  { id: 'filter', keys: '/', label: 'filter', group: 'Everywhere' },
  { id: 'mine', keys: 'm', label: 'only mine', group: 'Everywhere' },
  { id: 'inbox', keys: 'i', label: 'what is waiting for me', group: 'Everywhere' },
  { id: 'escape', keys: 'Esc', label: 'close, clear the filter, leave the cursor', group: 'Everywhere' },
  { id: 'console', keys: () => shortcutLabel(consoleKey()), label: 'console (commands, help)', group: 'Everywhere' },
  { id: 'notes', keys: () => shortcutLabel(quickNotesKey()), label: 'quick notes, over any view', group: 'Everywhere' },
  { id: 'palette', keys: () => shortcutLabel(paletteKey()), label: 'command palette: go to anything, run a command (in an agent’s terminal: clears it)', group: 'Everywhere' },
  { id: 'reopen', keys: '⌘⇧T', label: 'reopen the panel closed last (MR, ticket, logs, shell…)', group: 'Everywhere', match: cmdShift('t') },
  { id: 'home-shell', keys: () => (isMac ? '⌃`' : 'Ctrl+`'), label: 'a shell in the home folder, from anywhere', group: 'Everywhere', when: homeShellOn, match: homeShellKey },
  { id: 'assistant', keys: '⌥↵', label: 'the Assistant: ask Prism, its conversations (out of a field)', group: 'Everywhere', when: () => assistantOn(), match: (e) => e.altKey && e.key === 'Enter' && !e.metaKey && !e.ctrlKey && !e.shiftKey },
  { id: 'window-zoom', keys: '⌘+ / ⌘- / ⌘0', label: 'zoom the whole window in, out, back to 100%', group: 'Everywhere', match: (e) => chord(e) && !e.altKey && sizeStep(e) !== null },
  { id: 'settings', keys: '⌘,', label: 'settings', group: 'Everywhere', match: (e) => chord(e) && chordKey(e) === ',' },
  { id: 'zen', keys: '⌘⇧F', label: 'Zen mode: rail and header folded away', group: 'Everywhere', match: cmdShift('f') },
  { id: 'help', keys: '⌘? / F1', label: 'Help: guides and references', group: 'Everywhere', match: (e) => (chord(e) && chordKey(e) === '?') || e.key === 'F1' },
  { id: 'keys', keys: '?', label: 'the keyboard shortcuts', group: 'Everywhere' },
  // Where no menu bar does it (Linux, Windows).
  { id: 'fullscreen', keys: 'F11', label: 'Full screen', group: 'Everywhere', os: 'other', match: (e) => !isMac && e.key === 'F11' },
  { id: 'quit', keys: '⌘Q', label: 'Quit Prism', group: 'Everywhere', os: 'other', match: (e) => !isMac && chord(e) && chordKey(e) === 'q' },
]
const BY_ID = new Map(REGISTRY.map((s) => [s.id, s]))

/** The shortcut `id` answers to this key (its `match`). */
export function hit(id: string, e: KeyboardEvent): boolean {
  const s = BY_ID.get(id)
  if (!s?.match) throw new Error(`shortcut ${id}: nothing to match`)
  return s.match(e)
}

/** A shortcut's keys as shown, on this system. */
export const shortcutKeys = (id: string) => {
  const s = BY_ID.get(id)
  return s ? forPlatform(typeof s.keys === 'function' ? s.keys() : s.keys) : ''
}

const GROUPS: ShortcutGroup[] = ['Move', 'Row', 'Side panel', 'Block and view', 'Agents', 'Everywhere']
const onThisSystem = (s: ShortcutDef) => !s.os || (s.os === 'mac') === isMac

/** Every shortcut by section, as it is now (the pages' digits, the keys set in config.toml, the Agents view on or off). */
export function shortcutSections(): { title: string; keys: [string, string][] }[] {
  return GROUPS.map((g) => ({
    title: t(g),
    keys: REGISTRY.filter((s) => s.group === g && onThisSystem(s) && (!s.when || s.when())).map((s): [string, string] => [shortcutKeys(s.id), t(s.label)]),
  }))
}
