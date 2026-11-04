import { BLOCK_DEFS } from './blocks'
import { BLOCK_LABELS, type BlockId } from './views'
import { t } from './i18n.svelte'

/**
 * Settings › Cleanup: what is on and never used, what is used most, and what differs from its
 * default, from the counts of feature-use.svelte.ts. Pure, for the tests: the page gives the values.
 */

/** The counts as saved (usage.json): uses by day (`2026-10-30`) and id, and the lines kept for good. */
export interface UsageDoc {
  /** The first day counted. */
  since: string
  days: Record<string, Record<string, number>>
  /** Features "Keep" took off the never-used list. */
  kept: string[]
}

/** The span the page judges over, and the counts it waits for before saying a feature is unused. */
export const WINDOW_DAYS = 30
/** Days kept in usage.json: a window, and as much again. */
export const KEEP_DAYS = 60

const pad = (n: number) => String(n).padStart(2, '0')
/** A local day: `2026-10-30`. */
export const dayOf = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
/** Whole days from `a` to `b` (two `dayOf`). */
export const daysBetween = (a: string, b: string) => Math.round((Date.parse(`${b}T00:00:00Z`) - Date.parse(`${a}T00:00:00Z`)) / 86_400_000)

/** Without the days older than `KEEP_DAYS` before `today`. */
export const pruned = (days: UsageDoc['days'], today: string): UsageDoc['days'] => Object.fromEntries(Object.entries(days).filter(([d]) => daysBetween(d, today) < KEEP_DAYS))

/** Days counted so far, today included. */
export const daysCounted = (doc: UsageDoc, today: string) => Math.max(1, daysBetween(doc.since, today) + 1)

/** Uses by id over the last `days` days, today included. */
export function countsOver(doc: UsageDoc, today: string, days = WINDOW_DAYS): Record<string, number> {
  const out: Record<string, number> = {}
  for (const [day, ids] of Object.entries(doc.days)) {
    const age = daysBetween(day, today)
    if (age < 0 || age >= days) continue
    for (const [id, n] of Object.entries(ids)) out[id] = (out[id] ?? 0) + n
  }
  return out
}

/** A feature that has a switch: its key, and the counts that say it is used (an id, or a prefix ending with `:`). */
export interface Feature {
  id: string
  label: string
  /** Where it lives in Settings (a root of the tree). */
  area: string
  /** Its switch: Turn off writes `false` there. */
  path: string[]
  counts: string[]
  /** Listed only then (ClickUp: when it is the tracker). */
  when?: () => boolean
}

/** Uses of a feature: the sum of its ids and prefixes. */
export const usesOf = (f: Pick<Feature, 'counts'>, counts: Record<string, number>) =>
  Object.entries(counts).reduce((sum, [id, n]) => sum + (f.counts.some((c) => (c.endsWith(':') ? id.startsWith(c) : id === c)) ? n : 0), 0)

/** On, never used over the window, and not kept: what Turn off is offered for. */
export const neverUsed = (features: Feature[], counts: Record<string, number>, isOn: (path: string[]) => boolean, kept: string[]) =>
  features.filter((f) => (f.when?.() ?? true) && isOn(f.path) && !kept.includes(f.id) && !usesOf(f, counts))

/** The ids used most, most first. */
export const mostUsed = (counts: Record<string, number>, n = 8): [string, number][] =>
  Object.entries(counts).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, n)

/** Tables that hold your data rather than settings: never "different from default". */
const NOT_SETTINGS = new Set(['profile', 'packs', 'demo', 'views', 'templates', 'repos', 'workspaces', 'pods', 'environments', 'argocd'])

export interface Difference {
  path: string[]
  value: unknown
  /** `null`: no default (an optional key). */
  def: unknown
}

const isTable = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v)

/**
 * The keys config.toml sets to another value than their default: walked down its tables, a key
 * without a known default (a map of names, a block not built in) and the lists of tables left out.
 */
export function differences(values: Record<string, unknown>, defaults: Record<string, unknown>, path: string[] = []): Difference[] {
  const out: Difference[] = []
  for (const [key, value] of Object.entries(values)) {
    if (!path.length && NOT_SETTINGS.has(key)) continue
    if (!(key in defaults)) continue
    const def = defaults[key]
    const at = [...path, key]
    if (isTable(value)) {
      if (isTable(def)) out.push(...differences(value, def, at))
      continue
    }
    if (Array.isArray(value) && value.some(isTable)) continue
    if (JSON.stringify(value) !== JSON.stringify(def ?? null)) out.push({ path: at, value, def: def ?? null })
  }
  return out
}

/** A value as config.toml writes it, short. */
export const shown = (v: unknown): string => (v === null || v === undefined ? t('none') : typeof v === 'boolean' ? (v ? t('on') : t('off')) : typeof v === 'string' ? `"${v}"` : JSON.stringify(v))

/* ---------- The features with a switch, and what says they are used ---------- */

/** Prism's own, then the blocks with a source and a switch of their own (their `enabled`). */
export function features(trackerIsClickup: () => boolean): Feature[] {
  const own: Feature[] = [
    { id: 'forge', label: t('Forge: merge requests and CI'), area: t('Code'), path: ['forge', 'enabled'], counts: ['block:mrs', 'block:queue', 'block:ci', 'block:worktrees', 'palette:mr', 'palette:pipeline'] },
    { id: 'clickup', label: 'ClickUp', area: t('Work'), path: ['clickup', 'enabled'], counts: ['block:sprint', 'block:refinement', 'block:pickup', 'block:retro', 'palette:ticket'], when: trackerIsClickup },
    { id: 'calendar', label: t('Calendar'), area: t('Work'), path: ['calendar', 'enabled'], counts: ['block:agenda'] },
    { id: 'mail', label: t('Mail'), area: t('Work'), path: ['mail', 'enabled'], counts: ['block:mail'] },
    { id: 'renovate', label: t('Renovate'), area: t('CI'), path: ['renovate', 'enabled'], counts: ['block:renovate'] },
    { id: 'vulns', label: t('Vulnerabilities'), area: t('Security'), path: ['vulnerabilities', 'enabled'], counts: ['block:vulns'] },
    { id: 'agents', label: t('Agents'), area: t('Agents'), path: ['agents', 'enabled'], counts: ['block:sessions', 'agents:', 'action:', 'palette:agent', 'quick-answer', 'quick-action'] },
    { id: 'quick-answers', label: t('Quick answers'), area: t('Agents'), path: ['agents', 'quick_answers', 'enabled'], counts: ['quick-answer'] },
    { id: 'clusters', label: t('Clusters: pods, environments, ArgoCD'), area: t('Infra'), path: ['clusters', 'enabled'], counts: ['block:pods', 'block:envs', 'block:argocd', 'palette:pod', 'palette:env'] },
    { id: 'palette', label: t('Palette'), area: t('Interface'), path: ['palette', 'enabled'], counts: ['palette:'] },
    { id: 'tracker-search', label: t('Ticket search in the palette'), area: t('Interface'), path: ['palette', 'tracker_search'], counts: ['palette:tracker-search'] },
    { id: 'page-swipe', label: t('Swipe between pages'), area: t('Interface'), path: ['appearance', 'page_swipe'], counts: ['page-swipe'] },
    { id: 'threads', label: t('Ticket threads'), area: t('Interface'), path: ['appearance', 'ticket_threads'], counts: ['thread'] },
  ]
  const blocks = BLOCK_DEFS.flatMap((b): Feature[] => {
    const spec = b.settings?.sections.find((s) => s.id === b.settings!.node.toggle)
    if (!spec) return []
    const source = b.source ?? b.id
    return [{ id: `block:${b.id}`, label: t('Block {name}', { name: b.label }), area: b.settings!.node.label, path: [...spec.path, 'enabled'], counts: BLOCK_DEFS.filter((o) => (o.source ?? o.id) === source).map((o) => `block:${o.id}`) }]
  })
  return [...own, ...blocks]
}

/** A count's name, for Most used. */
export function labelOf(id: string): string {
  const [kind, rest = ''] = id.split(/:(.*)/s)
  if (kind === 'block') return t('Block {name}', { name: rest in BLOCK_LABELS ? t(BLOCK_LABELS[rest as BlockId]) : rest })
  if (kind === 'palette') return rest === 'tracker-search' ? t('Ticket search in the palette') : t('Palette: {what}', { what: rest })
  if (kind === 'action') return t('Agent action: {action}', { action: rest })
  if (id === 'agents:view') return t('Agents view')
  if (id === 'quick-answer') return t('Quick answers')
  if (id === 'quick-action') return t('Quick actions')
  if (id === 'thread') return t('Ticket threads')
  if (id === 'page-swipe') return t('Swipe between pages')
  return id
}
