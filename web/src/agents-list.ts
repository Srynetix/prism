import { tildePath } from './platform'
import type { Agent, ClaudeSession, SessionStatus } from './types'
import { t } from './i18n.svelte'

/**
 * The Agents view's list: Prism's agents and Claude Desktop's sessions as entries, grouped by
 * status, kind, tag or repository, sorted within each group. Pure: the view keeps the choices.
 */
export type Entry = { kind: 'agent'; id: string; status: SessionStatus; title: string; when: string | null; created: string | null; a: Agent } | { kind: 'desktop'; id: string; status: SessionStatus; title: string; when: string | null; created: string | null; c: ClaudeSession }
export type Filter = 'all' | 'agent' | 'desktop'
export type SortBy = 'activity' | 'created' | 'name' | 'place'
/** `category`: by kind (the agent's `category`). */
export type GroupBy = 'status' | 'category' | 'tag' | 'repo'
export interface Group {
  name: string
  items: Entry[]
}

export const agentEntry = (a: Agent): Entry => ({ kind: 'agent', id: a.id, status: a.status, title: a.title, when: a.last_activity, created: a.created_at, a })
/** A desktop session does not say when it was created: its last activity stands in. */
export const desktopEntry = (c: ClaudeSession): Entry => ({ kind: 'desktop', id: c.id, status: c.status, title: c.title, when: c.last_activity, created: c.last_activity, c })

/** What is waiting on me first, then what is working, then the rest, each from newest to oldest. */
export const GROUPS: [string, SessionStatus[]][] = [
  ['Waiting for you', ['waiting_tool', 'needs_you']],
  ['Running', ['running', 'background']],
  ['Finished', ['review_ready', 'completed', 'idle']],
]
/**
 * Order within each group: last activity (newest first, the default), creation (newest
 * first), name or branch / folder (A to Z).
 */
export const SORTS: [SortBy, string][] = [['activity', t('Last activity')], ['created', t('Created')], ['name', t('Name')], ['place', t('Branch')]]
/** The groupings of the "Group by" menu, in its order. */
export const GROUP_BY: [GroupBy, string][] = [['status', 'Status'], ['category', 'Kind'], ['tag', 'Tag'], ['repo', 'Repository']]
const NO_CATEGORY = 'No kind'
const NO_TAG = 'No tag'
const NO_REPO = 'No repository'
const DESKTOP = 'Claude Desktop'
/** A group's name to show: those of Prism's own in the window's language (their English name is their key). */
export const groupLabel = (name: string) => ([...GROUPS.map(([n]) => n), NO_CATEGORY, NO_TAG, NO_REPO].includes(name) ? t(name) : name)

export const where = (e: Entry) => (e.kind === 'agent' ? (e.a.branch ?? tildePath(e.a.cwd)) : (e.c.branch ?? e.c.folder))
export const tagsOf = (e: Entry) => (e.kind === 'agent' ? (e.a.tags ?? []) : [])
const categoryOf = (e: Entry) => (e.kind === 'desktop' ? DESKTOP : (e.a.category ?? NO_CATEGORY))
const rank = (e: Entry) => GROUPS.findIndex(([, sts]) => sts.includes(e.status))
const recent = (x: Entry, y: Entry) => (y.when ?? '').localeCompare(x.when ?? '')
const alpha = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' })

export function byKey(sortBy: SortBy) {
  return (x: Entry, y: Entry) =>
    sortBy === 'created' ? (y.created ?? '').localeCompare(x.created ?? '') || recent(x, y)
    : sortBy === 'name' ? alpha(x.title, y.title) || recent(x, y)
    : sortBy === 'place' ? alpha(where(x), where(y)) || recent(x, y)
    : recent(x, y)
}

/**
 * Grouping by status, or by kind: those of `[[agents.kinds]]` in their order, then one removed
 * from the config but still carried, agents without a kind, and Claude Desktop. Within a kind,
 * what is waiting on me first, as by status. Grouping by tag: one group per tag (`#api`), the
 * suggested ones first; an agent with several tags shows in each of their groups. Grouping by
 * repository: the one each works in (`repoOf`, a Claude Desktop session's too), those of the
 * config in their order, then one removed from it, then those outside every repository.
 */
export function groupEntries(shown: Entry[], o: { groupBy: GroupBy; sortBy: SortBy; categories: string[]; tags: string[]; repos?: string[]; repoOf?: (e: Entry) => string | null }): Group[] {
  const sort = byKey(o.sortBy)
  if (o.groupBy === 'status') return GROUPS.map(([name, sts]) => ({ name, items: shown.filter((e) => sts.includes(e.status)).sort(sort) })).filter((g) => g.items.length)
  const inGroup = (x: Entry, y: Entry) => (o.sortBy === 'activity' ? rank(x) - rank(y) : 0) || sort(x, y)
  if (o.groupBy === 'tag')
    return [
      ...o.tags.map((tag) => ({ name: `#${tag}`, items: shown.filter((e) => tagsOf(e).includes(tag)) })),
      { name: NO_TAG, items: shown.filter((e) => e.kind === 'agent' && !tagsOf(e).length) },
      { name: DESKTOP, items: shown.filter((e) => e.kind === 'desktop') },
    ]
      .map((g) => ({ ...g, items: g.items.sort(inGroup) }))
      .filter((g) => g.items.length)
  if (o.groupBy === 'repo') {
    const repoOf = (e: Entry) => o.repoOf?.(e) ?? NO_REPO
    return [...new Set([...(o.repos ?? []), ...shown.map(repoOf).filter((r) => r !== NO_REPO), NO_REPO])]
      .map((name) => ({ name, items: shown.filter((e) => repoOf(e) === name).sort(inGroup) }))
      .filter((g) => g.items.length)
  }
  // Configured names in their order, then one removed from the config but still carried, then none and Desktop.
  const [names, of, none] = [o.categories, categoryOf, NO_CATEGORY]
  const carried = shown.map(of).filter((c) => c !== none && c !== DESKTOP && !names.includes(c))
  return [...new Set([...names, ...carried, none, DESKTOP])]
    // By last activity, what is waiting on me comes first, as by status; another order is applied as is.
    .map((name) => ({ name, items: shown.filter((e) => of(e) === name).sort(inGroup) }))
    .filter((g) => g.items.length)
}

type Task = 'refine' | 'implement' | 'review' | 'validate' | 'cve' | 'other'
export const TASK_ICON: Record<Task, 'search' | 'code' | 'review' | 'validate' | 'security' | 'terminal'> = { refine: 'search', implement: 'code', review: 'review', validate: 'validate', cve: 'security', other: 'terminal' }
/** An agent's task: its kind, otherwise its title ("· Refine", "Implement"…). */
export const taskOf = (e: Entry): Task => taskIn(`${e.kind === 'agent' ? (e.a.category ?? '') : ''} ${e.title}`)
/** A kind's icon, from its name ("Refinement", "Code review"…); a name it does not know: a tag. */
export const kindIcon = (name: string) => (taskIn(name) === 'other' ? 'category' : TASK_ICON[taskIn(name)])
function taskIn(words: string): Task {
  const text = words.toLowerCase()
  if (/refine|refinement|spec/.test(text)) return 'refine'
  if (/review/.test(text)) return 'review'
  if (/validat/.test(text)) return 'validate'
  if (/\bcve\b|vulnerab/.test(text)) return 'cve'
  if (/implement|develop|fix|rebase|backport/.test(text)) return 'implement'
  return 'other'
}
/** What we recognize: the ticket number, or the MR's (`!1621`, `#1621` on GitHub); otherwise the start of the title. */
export function shortOf(e: Entry): string {
  const ticket = e.kind === 'agent' ? e.a.ticket : e.c.ticket
  const n = ticket?.match(/(\d+)$/)?.[1] ?? e.title.match(/[!#]\d+/)?.[0]
  if (n) return n
  const word = e.title.replace(/^[^\p{L}\p{N}]+/u, '').split(/[\s·:]+/)[0] ?? ''
  return word.length > 5 ? `${word.slice(0, 4)}…` : word || '—'
}
