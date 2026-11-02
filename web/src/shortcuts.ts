import { consoleKey } from './console.svelte'
import { quickNotesKey } from './quick-notes.svelte'
import { shortcutLabel } from './shortcut'
import { keys as forPlatform } from './platform'
import { t } from './i18n.svelte'

/**
 * Every keyboard shortcut, by section: the "?" overlay (KeyHelp.svelte) and Help's reference
 * show this same list. `console` and `notes` stand for keys set in config.toml (`keyLabel`).
 * Written the macOS way, adapted to the system below.
 */
const LIST: { title: string; keys: [string, string][] }[] = [
  {
    title: 'Move',
    keys: [
      ['j / k', 'next / previous row (then the next block of the column)'],
      ['h / l', 'block in the column on the left / right'],
      ['J / K, ⇧↓ / ⇧↑', 'next / previous block'],
      ['gg / G', 'first / last row'],
      ['Ctrl-d / Ctrl-u', '10 rows down / up'],
    ],
  },
  {
    title: 'Row',
    keys: [
      ['Enter / o', 'open: preview, drawer or main link'],
      ['O', 'open in the browser (ClickUp, GitLab, GitHub)'],
      ['a', 'run the row action (Assign me, Fix, Review…, or an Alerts quick fix)'],
      ['.', 'actions menu ⋯ (j / k, Enter, Esc)'],
      ['x', 'mark an Alerts item as seen / new'],
      ['t', 'thread: its ticket, worktree, agent, MR, pipeline and environments'],
    ],
  },
  {
    title: 'Side panel',
    keys: [
      ['Ctrl-w l / Tab', 'focus the open panel (MR, ticket, logs…)'],
      ['j / k, Ctrl-d / u', 'scroll it; gg / G top / bottom'],
      ['J / K', 'next / previous button or link, Enter to use it'],
      ['] / [', 'diff: next / previous file'],
      ['n / N', 'diff: next / previous message (thread, or note to the agent)'],
      ['v', 'diff: mark the current file as viewed'],
      ['Ctrl-w h / Tab / Esc', 'back to the blocks (Esc again closes)'],
    ],
  },
  {
    title: 'Block and view',
    keys: [
      ['r', 'reload ClickUp now'],
      ['za', 'collapse / expand the block'],
      ['zs', 'short / normal'],
      ['zz', 'zoom the block'],
      ['1 … 9', 'view'],
      ['gt / gT', 'next / previous view'],
    ],
  },
  {
    title: 'Agents',
    keys: [
      ['⌘6', 'Agents view'],
      ['⌘N', 'new agent'],
      ['⌘[ / ⌘]', 'previous / next agent'],
      ['⌘B', 'collapse / expand the agents list'],
      ['⌘.', 'interrupt the agent (Esc)'],
      ['⇧-click', 'an agent action launches at once'],
    ],
  },
  {
    title: 'Everywhere',
    keys: [
      ['/', 'filter'],
      ['m', 'only mine'],
      ['i', 'what is waiting for me'],
      ['Esc', 'close, clear the filter, leave the cursor'],
      ['console', 'console (commands, help)'],
      ['notes', 'quick notes, over any view'],
      ['⌘K', 'command palette: go to anything, run a command (in an agent’s terminal: clears it)'],
      ['⌘⇧T', 'reopen the panel closed last (MR, ticket, logs, shell…)'],
      ['⌘,', 'settings'],
      ['⌘⇧F', 'Zen mode: rail and header folded away'],
      ['⌘? / F1', 'Help: guides and references'],
      ['?', 'the keyboard shortcuts'],
    ],
  },
]

export const SHORTCUTS = LIST.map((s) => ({ title: t(s.title), keys: s.keys.map(([k, what]): [string, string] => [forPlatform(k), t(what)]) }))

/** The key shown for a shortcut: the configured one for the console and the quick notes. */
export const keyLabel = (k: string) => (k === 'console' ? shortcutLabel(consoleKey()) : k === 'notes' ? shortcutLabel(quickNotesKey()) : k)
