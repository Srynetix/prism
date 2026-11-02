import { mrTickets } from './conventions'
import { hoursOf } from './tracker'
import type { AgentCost } from './bindings/AgentCost'
import type { TicketField } from './bindings/TicketField'
import type { Ticket, TimeEntry } from './types'

/**
 * Actual against estimate (`[tracker] actuals`): a ticket's time logged — everyone's, as the tracker
 * sums it, failing that mine — and its agents' active time and cost (`agent-costs.json`, closed
 * agents too), against its estimate. The ticket panel's Actual section shows one ticket; My sprint's
 * Actuals the sprint's done tickets, and what the last sprints say by type and by estimate.
 * Minutes throughout; a gap is a share (+0.45: 45 % over the estimate).
 */

const MIN = 60_000
/** A gap within this share either way: well estimated. */
export const CLOSE = 0.15
/** Beyond this share over: far over. */
export const FAR = 0.4

/** A ticket's estimate in minutes: the tracker's own, otherwise the first estimate field holding hours. */
export function estimateOf(t: { time_estimate: number | null; fields?: TicketField[] }): number | null {
  if (t.time_estimate) return Math.round(t.time_estimate / MIN)
  for (const f of t.fields ?? []) {
    const h = f.estimate ? hoursOf(f) : null
    if (h) return Math.round(h * 60)
  }
  return null
}

/**
 * The agents of each ticket: the one it names, otherwise the tickets of its branch's MR (an agent
 * launched on a branch, its ticket found later in the MR's title). Those of none under `''`.
 */
export function agentsByTicket(costs: AgentCost[], mrs: { source_branch: string; ticket: string | null; tickets?: string[] }[]): Map<string, AgentCost[]> {
  const byBranch = new Map<string, string[]>()
  for (const m of mrs) {
    const ids = mrTickets(m)
    if (ids.length && !byBranch.has(m.source_branch)) byBranch.set(m.source_branch, ids)
  }
  const out = new Map<string, AgentCost[]>()
  for (const c of costs) {
    const ids = c.ticket ? [c.ticket] : ((c.branch && byBranch.get(c.branch)) ?? [''])
    for (const id of ids) {
      const list = out.get(id)
      if (list) list.push(c)
      else out.set(id, [c])
    }
  }
  return out
}

export interface Actual {
  estimate: number | null
  /** Logged by everyone (the tracker's sum, never below mine); by me; by the others, when the tracker tells everyone's. */
  logged: number
  mine: number
  others: number | null
  agents: AgentCost[]
  agentMinutes: number
  /** Dollars, of the agents with a known price; null: none has one. */
  agentCost: number | null
  tokens: number
  /** Logged, plus the agents' active time unless the setting counts the time logged only. */
  actual: number
  gap: number | null
}

export const agentMinutes = (agents: AgentCost[]) => Math.round(agents.reduce((n, a) => n + a.active_ms, 0) / MIN)
export function agentCost(agents: AgentCost[]): number | null {
  const priced = agents.filter((a) => a.cost !== null)
  return priced.length ? priced.reduce((n, a) => n + (a.cost ?? 0), 0) : null
}
/** The gap of an actual against its estimate; none without an estimate. */
export const gapOf = (actual: number, estimate: number | null) => (estimate ? actual / estimate - 1 : null)

export function actualOf(x: { estimate: number | null; spentMs?: number | null; mineMs: number; agents: AgentCost[]; loggedOnly: boolean }): Actual {
  const mine = Math.round(x.mineMs / MIN)
  const spent = x.spentMs ? Math.round(x.spentMs / MIN) : null
  const logged = Math.max(spent ?? 0, mine)
  const minutes = agentMinutes(x.agents)
  const actual = logged + (x.loggedOnly ? 0 : minutes)
  return {
    estimate: x.estimate,
    logged,
    mine,
    others: spent !== null ? Math.max(0, spent - mine) : null,
    agents: x.agents,
    agentMinutes: minutes,
    agentCost: agentCost(x.agents),
    tokens: x.agents.reduce((n, a) => n + a.tokens, 0),
    actual,
    gap: gapOf(actual, x.estimate),
  }
}

/** My time logged on a ticket over the history read, in milliseconds. */
export const mineMs = (entries: TimeEntry[], id: string) => entries.filter((e) => e.ticket === id).reduce((n, e) => n + e.duration_ms, 0)

/** How a gap reads: within `CLOSE`, a little over, far over (or far under). */
export function tone(gap: number | null): 'ok' | 'warn' | 'ko' | 'none' {
  if (gap === null) return 'none'
  if (Math.abs(gap) <= CLOSE) return 'ok'
  return gap > FAR || gap < -FAR ? 'ko' : 'warn'
}

/** `+45 %`, `−20 %`. */
export const pct = (gap: number) => `${gap >= 0 ? '+' : '−'}${Math.round(Math.abs(gap) * 100)} %`

/**
 * A ticket's type, from where `[tracker] ticket_type` says: empty, the tracker's task type;
 * `tags`, its first tag; otherwise the custom field of that name (case aside). Null: none.
 */
export function typeOf(t: Pick<Ticket, 'task_type' | 'fields'> & { tags?: string[] }, source: string): string | null {
  const s = source.trim().toLowerCase()
  if (!s) return t.task_type ?? null
  if (s === 'tags') return t.tags?.[0] ?? null
  return t.fields?.find((f) => f.name.toLowerCase() === s)?.values[0]?.label ?? null
}

/** The estimate's tier: to the half hour, the steps teams estimate in. */
export const tierOf = (minutes: number) => Math.max(30, Math.round(minutes / 30) * 30)

/** A done ticket of the sprint, as the Actuals table shows it (and keeps it for the next sprints). */
export interface ActualRow {
  id: string
  name: string
  type: string | null
  estimate: number | null
  logged: number
  agentMinutes: number
  agentCost: number | null
  actual: number
  gap: number | null
  sprint: string
}

export function actualRows(tickets: Ticket[], agents: Map<string, AgentCost[]>, entries: TimeEntry[], opts: { loggedOnly: boolean; typeSource: string; sprint: string }): ActualRow[] {
  return tickets.map((t) => {
    const a = actualOf({ estimate: estimateOf(t), spentMs: t.time_spent, mineMs: mineMs(entries, t.id), agents: agents.get(t.id) ?? [], loggedOnly: opts.loggedOnly })
    return { id: t.id, name: t.name, type: typeOf(t, opts.typeSource), estimate: a.estimate, logged: a.logged, agentMinutes: a.agentMinutes, agentCost: a.agentCost, actual: a.actual, gap: a.gap, sprint: opts.sprint }
  })
}

/** The median of figures that may be fractions (gaps); null for none. */
export function medianOf(xs: number[]): number | null {
  if (!xs.length) return null
  const s = [...xs].sort((a, b) => a - b)
  const m = Math.floor(s.length / 2)
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2
}

export interface Totals {
  count: number
  /** Of the estimated tickets: their estimates, and their actual. */
  estimated: number
  estimatedActual: number
  /** Of them all. */
  actual: number
  agentMinutes: number
  agentCost: number | null
  /** The agents' cost per ticket, on average. */
  perTicket: number | null
  medianGap: number | null
}

export function totals(rows: ActualRow[]): Totals {
  const est = rows.filter((r) => r.estimate)
  const costs = rows.map((r) => r.agentCost).filter((c): c is number => c !== null)
  const cost = costs.length ? costs.reduce((n, c) => n + c, 0) : null
  return {
    count: rows.length,
    estimated: est.reduce((n, r) => n + (r.estimate ?? 0), 0),
    estimatedActual: est.reduce((n, r) => n + r.actual, 0),
    actual: rows.reduce((n, r) => n + r.actual, 0),
    agentMinutes: rows.reduce((n, r) => n + r.agentMinutes, 0),
    agentCost: cost,
    perTicket: cost !== null && rows.length ? cost / rows.length : null,
    medianGap: medianOf(est.map((r) => r.gap ?? 0)),
  }
}

/** What a group of estimated tickets says: how many, their median actual and gap. */
export interface Trend {
  type: string | null
  /** The estimate's tier; null: the type's tickets of every tier. */
  tier: number | null
  n: number
  medianActual: number
  medianGap: number
}

/**
 * The trends of estimated tickets (those of the last sprints, together): by type, and by type and
 * estimate tier; a group of fewer than `min` tickets says nothing. The largest gaps first.
 */
export function trends(rows: ActualRow[], min = 2): { byType: Trend[]; byTier: Trend[] } {
  const est = rows.filter((r) => r.estimate)
  const group = (key: (r: ActualRow) => string, tier: boolean): Trend[] => {
    const groups = new Map<string, ActualRow[]>()
    for (const r of est) groups.set(key(r), [...(groups.get(key(r)) ?? []), r])
    return [...groups.values()]
      .filter((g) => g.length >= min)
      .map((g) => ({ type: g[0].type, tier: tier ? tierOf(g[0].estimate!) : null, n: g.length, medianActual: Math.round(medianOf(g.map((r) => r.actual))!), medianGap: medianOf(g.map((r) => r.gap ?? 0))! }))
      .sort((a, b) => Math.abs(b.medianGap) - Math.abs(a.medianGap) || b.n - a.n)
  }
  return { byType: group((r) => r.type ?? '', false), byTier: group((r) => `${r.type ?? ''}\n${tierOf(r.estimate!)}`, true) }
}

/** The agents of no ticket within `[from, to]` (milliseconds): the Unlinked line. */
export function unlinked(agents: Map<string, AgentCost[]>, from: number, to: number): AgentCost[] {
  return (agents.get('') ?? []).filter((a) => Date.parse(a.started) <= to && Date.parse(a.last) >= from)
}

/** `2.1 M`, `840 k`, `950`: a count of tokens, short. */
export function tokensShort(n: number): string {
  if (n >= 1e6) return `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)} M`
  if (n >= 1e3) return `${Math.round(n / 1e3)} k`
  return String(n)
}

/** A model as a row has room for: `claude-opus-5-5` → `opus`; the others as their CLI names them. */
export const shortModel = (m: string | null) => (m ? m.replace(/^claude-/, '').replace(/-\d+(?:[-.]\d+)*$/, '') : '')

/**
 * A cell of CSV: quoted when it holds a separator, a quote or a line break. A text a spreadsheet
 * would read as a formula (a ticket's name starting with `=`, `+`, `-`, `@`) is kept text (`'`).
 */
const cell = (v: string | number | null) => {
  let s = v === null ? '' : String(v)
  if (typeof v === 'string' && /^[=+\-@\t\r]/.test(s)) s = `'${s}`
  return /[",\n;]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s
}

/** The table as CSV, durations in hours (decimals), gaps in percent, the cost in dollars. */
export function actualsCsv(rows: ActualRow[], header: string[]): string {
  const h = (min: number | null) => (min === null ? null : Math.round((min / 60) * 100) / 100)
  const lines = rows.map((r) => [r.sprint, r.id, r.name, r.type, h(r.estimate), h(r.logged), h(r.agentMinutes), r.agentCost === null ? null : Math.round(r.agentCost * 100) / 100, h(r.actual), r.gap === null ? null : Math.round(r.gap * 100)])
  return [header, ...lines].map((l) => l.map(cell).join(',')).join('\n') + '\n'
}
