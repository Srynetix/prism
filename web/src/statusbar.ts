import { BLOCK_DEFS } from './blocks'

/**
 * Status bar: a line under all views, made of small items. An item with nothing to say (no
 * pipeline, no meeting…) is not displayed.
 *
 * Its order is `[appearance] statusbar` (empty: `DEFAULT_ORDER`), `|` parting the left side from
 * the right; `[appearance] statusbar_hidden` the items hidden (neither set: `DEFAULT_HIDDEN`). Each
 * item belongs to a zone, what it asks of you: `you` (waits for an action), `work` (context),
 * `machine` (the computer and Prism); consecutive items of a zone are drawn together, between thin
 * separators. Squeezed, the bar moves its least needed items (`rank`) into a ⋯ at its end, never
 * those of `you`.
 *
 * By default, a balanced bar: what waits for you (agents, my pipeline, the next meeting), then the
 * work (repositories, sprint day), and on the right the computer (plan usage, Prism, the clock);
 * every other item is hidden until « Customize… » or a right-click shows it.
 */
type CoreStatus = 'alerts' | 'live' | 'prism' | 'repos' | 'pipeline' | 'meeting' | 'reminder' | 'claude' | 'profiles' | 'machines' | 'shells' | 'digest' | 'routines' | 'mcp' | 'aside' | 'usage' | 'sprint' | 'audio' | 'clock' | 'feedback'
/** A core item, or a block's (its id: `BlockDef.status`). */
export type StatusId = CoreStatus | (string & {})
export type Zone = 'you' | 'work' | 'machine'
/** In `[appearance] statusbar`: the left side ends there. */
export const SIDE_BREAK = '|'

/** `rank`: kept in the bar when it lacks room, the higher first; `Infinity`: never moved into the ⋯. */
export interface StatusItem { id: StatusId; label: string; zone: Zone; rank: number }

/** The core's items, in their default order. */
const CORE: StatusItem[] = [
  { id: 'claude', label: 'Agents waiting', zone: 'you', rank: Infinity },
  { id: 'pipeline', label: 'My pipeline', zone: 'you', rank: Infinity },
  { id: 'meeting', label: 'Next meeting', zone: 'you', rank: Infinity },
  { id: 'reminder', label: 'Reminders', zone: 'you', rank: Infinity },
  { id: 'digest', label: 'Notifications digest', zone: 'you', rank: Infinity },
  { id: 'routines', label: 'Routines to review', zone: 'you', rank: Infinity },
  { id: 'mcp', label: 'Agents on Prism MCP', zone: 'you', rank: Infinity },
  { id: 'alerts', label: 'What waits for you', zone: 'you', rank: Infinity },
  { id: 'repos', label: 'Repositories', zone: 'work', rank: 5 },
  { id: 'sprint', label: 'Sprint day', zone: 'work', rank: 4 },
  { id: 'profiles', label: 'Other profiles', zone: 'work', rank: 5 },
  { id: 'machines', label: 'Other computers', zone: 'work', rank: 5 },
  { id: 'shells', label: 'Open shells', zone: 'work', rank: 3 },
  { id: 'aside', label: 'In passing', zone: 'work', rank: 3 },
  { id: 'usage', label: 'Plan usage', zone: 'machine', rank: 6 },
  { id: 'prism', label: 'Prism: updates and problems', zone: 'machine', rank: 7 },
  { id: 'audio', label: 'Audio', zone: 'machine', rank: 2 },
  { id: 'feedback', label: 'Feedback', zone: 'machine', rank: 1 },
  { id: 'live', label: 'Connection', zone: 'machine', rank: 8 },
  { id: 'clock', label: 'Clock', zone: 'machine', rank: Infinity },
]
const CORE_BREAK = 'usage'

/**
 * Former ids, still read in a config: `update`, `packs` and `problems` are one item now, `prism`
 * (an update of Prism, packs or modules to update, problems).
 */
export const ALIASES: Record<string, StatusId> = { update: 'prism', packs: 'prism', problems: 'prism' }
const current = (id: string) => ALIASES[id] ?? id

/** Every item, the blocks' after the item they name (`BlockDef.status`) and in its zone; without one, by their side. */
export const STATUS_ITEMS: StatusItem[] = BLOCK_DEFS.reduce<StatusItem[]>(
  (list, b) => {
    if (!b.status) return list
    const at = list.findIndex((x) => x.id === current(b.status!.after ?? ''))
    const item: StatusItem = { id: b.id, label: b.status.label, zone: list[at]?.zone ?? (b.status.side === 'left' ? 'work' : 'machine'), rank: 2 }
    return at < 0 ? [...list, item] : [...list.slice(0, at + 1), item, ...list.slice(at + 1)]
  },
  [...CORE],
)
const BY_ID = new Map(STATUS_ITEMS.map((i) => [i.id, i]))
export const statusItem = (id: StatusId) => BY_ID.get(id)

/** The default order: zone by zone, `you` and `work` on the left. */
export const DEFAULT_ORDER: string[] = STATUS_ITEMS.flatMap((i) => (i.id === CORE_BREAK ? [SIDE_BREAK, i.id] : [i.id]))

/** Shown by default; every other item (core or block) is hidden until shown. */
export const DEFAULT_SHOWN: StatusId[] = ['claude', 'pipeline', 'meeting', 'repos', 'sprint', 'usage', 'prism', 'clock']
/** `[appearance] statusbar_hidden` when neither it nor `statusbar` is set. */
export const DEFAULT_HIDDEN: string[] = STATUS_ITEMS.map((i) => i.id).filter((id) => !DEFAULT_SHOWN.includes(id))

/**
 * `[appearance] statusbar_hidden` as it reads now: a former id stands for its item, which is hidden
 * once every former id it replaces is (`problems` alone hidden: `prism` still says the updates).
 */
export function readHidden(hidden: string[]): string[] {
  const out = hidden.filter((id) => !ALIASES[id])
  for (const to of new Set(Object.values(ALIASES))) {
    const from = Object.keys(ALIASES).filter((a) => ALIASES[a] === to)
    if (!out.includes(to) && from.every((a) => hidden.includes(a))) out.push(to)
  }
  return out
}

/**
 * `[appearance] statusbar` made whole: former ids read as their item, unknown ids dropped, one
 * `|`, and every item it leaves out (one Prism added since, a block turned on) put back after the
 * one before it by default.
 */
export function fullOrder(given: string[]): string[] {
  if (!given.length) return [...DEFAULT_ORDER]
  const order = given.map(current)
  const out = order.filter((id, i) => (id === SIDE_BREAK ? order.indexOf(SIDE_BREAK) === i : BY_ID.has(id) && order.indexOf(id) === i))
  if (!out.includes(SIDE_BREAK)) out.push(SIDE_BREAK)
  DEFAULT_ORDER.forEach((id, i) => {
    if (out.includes(id)) return
    const before = DEFAULT_ORDER.slice(0, i).reverse().find((p) => out.includes(p))
    out.splice(before ? out.indexOf(before) + 1 : 0, 0, id)
  })
  return out
}

/** The two sides, items hidden left out, each cut into runs of one zone. */
export function arrange(order: string[], given: string[]): { side: 'left' | 'right'; zones: { zone: Zone; items: StatusItem[] }[] }[] {
  const full = fullOrder(order)
  const hidden = readHidden(given)
  const at = full.indexOf(SIDE_BREAK)
  const side = (ids: string[]) =>
    ids.filter((id) => !hidden.includes(id)).map((id) => BY_ID.get(id)!).reduce<{ zone: Zone; items: StatusItem[] }[]>((runs, item) => {
      const last = runs[runs.length - 1]
      if (last?.zone === item.zone) last.items.push(item)
      else runs.push({ zone: item.zone, items: [item] })
      return runs
    }, [])
  return [{ side: 'left', zones: side(full.slice(0, at)) }, { side: 'right', zones: side(full.slice(at + 1)) }]
}

/**
 * The items moved into the ⋯ for the bar to fit `room` px: the lowest `rank` first, and of two
 * alike the one further right; then those that fit again in the room left come back. `widths`: each item's last width in the bar (0: nothing to say).
 * `more`: the ⋯'s own width, counted as soon as one item goes.
 */
export function overflow(shown: StatusId[], widths: Map<StatusId, number>, room: number, more: number): StatusId[] {
  let need = shown.reduce((n, id) => n + (widths.get(id) ?? 0), 0)
  if (need <= room) return []
  need += more
  const out: StatusId[] = []
  const candidates = shown.filter((id) => (widths.get(id) ?? 0) > 0 && (BY_ID.get(id)?.rank ?? 0) < Infinity).sort((a, b) => BY_ID.get(a)!.rank - BY_ID.get(b)!.rank || shown.indexOf(b) - shown.indexOf(a))
  for (const id of candidates) {
    if (need <= room) break
    out.push(id)
    need -= widths.get(id)!
  }
  // A wide item gone may leave room for a narrower one taken before it: back in the bar.
  for (const id of [...out].reverse()) {
    if (need + widths.get(id)! > room) continue
    out.splice(out.indexOf(id), 1)
    need += widths.get(id)!
  }
  return out
}
