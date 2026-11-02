import { fuzzy } from './fuzzy'
import { t, tc } from './i18n.svelte'

/** The palette's lines and their search (palette.svelte.ts gathers them from the snapshot). */
export type Kind = 'agent' | 'mr' | 'ticket' | 'file' | 'page' | 'block' | 'pipeline' | 'env' | 'pod' | 'shell' | 'closed' | 'command' | 'setting' | 'help'

export const KIND_LABEL: Record<Kind, string> = {
  agent: t('Agents'), mr: t('Merge requests'), ticket: t('Tickets'), file: t('Files'), page: t('Pages'), block: t('Blocks'), pipeline: t('Pipelines'), env: t('Environments'), pod: t('Pods'), shell: t('Open shells'), closed: t('Closed lately'), command: t('Commands'), setting: t('Settings'), help: t('Help'),
}
/** One of a kind: the header of an entry's actions. */
export const KIND_ONE: Record<Kind, string> = {
  agent: t('Agent'), mr: t('Merge request'), ticket: t('Ticket'), file: t('File'), page: t('Page'), block: t('Block'), pipeline: t('Pipeline'), env: t('Environment'), pod: t('Pod'), shell: t('Shell'), closed: t('Closed panel'), command: t('Command'), setting: t('Setting'), help: t('Help'),
}

/** An action of an entry (⌘↵): a menu's item, a quick answer. */
export interface PaletteAction {
  key: string
  label: string
  hint?: string
  group?: string
  /** A second ↵ is needed, with this message. */
  confirm?: string
  run?: () => unknown
  href?: string
}

export interface Entry {
  /** Stable: remembered among the recent ones. */
  id: string
  kind: Kind
  title: string
  hint?: string
  /** Also matched, not shown: a branch, tags, keys of a Settings page. */
  words?: string
  keys?: string
  pill?: { label: string; tone: string }
  /** It waits for you (an agent asking, an MR to review): first among equals. */
  waiting?: boolean
  /** A command's words (`<text>`, `[prompt]`); `required`: nothing without them. */
  usage?: string
  required?: boolean
  /** Found by the tracker for this query (palette.svelte.ts `found`): kept whatever its title. */
  found?: boolean
  run: (words?: string) => unknown
  actions?: () => PaletteAction[]
}

export interface Row {
  entry: Entry
  /** Matched positions in the title. */
  at: number[]
  /** Words typed after a command's name: ↵ runs it with them. */
  words?: string
}

/** Per group, at most this many lines while typing (all when nothing else matches). */
const PER_GROUP = 5
const ORDER: Kind[] = ['agent', 'mr', 'ticket', 'file', 'page', 'command', 'block', 'pipeline', 'env', 'pod', 'shell', 'closed', 'setting', 'help']

export interface Group {
  title: string
  rows: Row[]
}

/**
 * What the query finds, grouped; nothing typed: the shells still open and the panels closed lately
 * (to go back to them), the recent ones, what waits for you, the pages.
 */
export function search(query: string, all: Entry[], recentIds: string[] = []): Group[] {
  const q = query.trim()
  if (!q) {
    const byId = new Map(all.map((e) => [e.id, e]))
    const recentRows = recentIds.map((id) => byId.get(id)).filter((e): e is Entry => !!e && e.kind !== 'shell' && e.kind !== 'closed').map((entry) => ({ entry, at: [] }))
    const seen = new Set(recentRows.map((r) => r.entry.id))
    const waiting = all.filter((e) => e.waiting && !seen.has(e.id)).map((entry) => ({ entry, at: [] }))
    const pages = all.filter((e) => e.kind === 'page').map((entry) => ({ entry, at: [] }))
    const kind = (k: Kind) => all.filter((e) => e.kind === k).map((entry) => ({ entry, at: [] }))
    return [{ title: KIND_LABEL.shell, rows: kind('shell') }, { title: KIND_LABEL.closed, rows: kind('closed') }, { title: t('Recent'), rows: recentRows }, { title: t('Waiting for you'), rows: waiting }, { title: t('Pages'), rows: pages }].filter((g) => g.rows.length)
  }
  // A command and its words: `note buy milk` runs `note` with them.
  const [head, ...rest] = q.split(/\s+/)
  const named = all.find((e) => e.kind === 'command' && e.usage && e.title === head.toLowerCase())
  if (named && rest.length) return [{ title: tc('shell', 'Run'), rows: [{ entry: named, at: [], words: rest.join(' ') }] }]
  const scored = all.flatMap((entry) => {
    const inTitle = fuzzy(entry.title, q)
    // The hidden words (a branch, keys of a Settings page) as typed: each word found as is.
    const hidden = `${entry.title} ${entry.words ?? ''} ${entry.hint ?? ''}`.toLowerCase()
    const m = inTitle ?? (q.toLowerCase().split(/\s+/).every((w) => hidden.includes(w)) ? fuzzy(hidden, q) : null)
    if (!m) return entry.found ? [{ entry, at: [], score: -100 }] : []
    const bonus = (entry.waiting ? 2 : 0) + (recentIds.includes(entry.id) ? 3 : 0) - (inTitle ? 0 : 4)
    return [{ entry, at: inTitle?.at ?? [], score: m.score + bonus }]
  })
  scored.sort((a, b) => b.score - a.score)
  // Groups in the order of their best line; kinds as tie-break.
  const kinds = [...new Set(scored.map((r) => r.entry.kind))].sort((a, b) => scored.findIndex((r) => r.entry.kind === a) - scored.findIndex((r) => r.entry.kind === b) || ORDER.indexOf(a) - ORDER.indexOf(b))
  const many = kinds.length > 1
  return kinds.map((k) => ({ title: KIND_LABEL[k], rows: scored.filter((r) => r.entry.kind === k).slice(0, many ? PER_GROUP : 50).map(({ entry, at }) => ({ entry, at })) }))
}

