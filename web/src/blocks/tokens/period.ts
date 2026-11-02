import type { TokenDay } from './types'

/**
 * The periods of the Tokens block: a week (Monday to Sunday) or a calendar month, `offset` back
 * from the current one; its days one by one (those without a reply at 0), its totals, and the
 * same totals for the period before, to compare.
 */
export type Span = 'week' | 'month'

export const total = (d: Pick<TokenDay, 'input' | 'output' | 'cache_read' | 'cache_write'>) => d.input + d.output + d.cache_read + d.cache_write

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`

/** The first and last day of the period `offset` back (0: the current one), local dates. */
export function bounds(span: Span, offset: number, today = new Date()): { from: Date; to: Date } {
  if (span === 'month') {
    const from = new Date(today.getFullYear(), today.getMonth() - offset, 1)
    return { from, to: new Date(from.getFullYear(), from.getMonth() + 1, 0) }
  }
  const monday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - ((today.getDay() + 6) % 7) - 7 * offset)
  return { from: monday, to: new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + 6) }
}

/** A day's tools; one kept before the other tools were read has none (Rust sends `[]`): Claude Code's. */
const toolsOfDay = (d: TokenDay): [string, number][] => (d.tools?.length ? d.tools : [['claude', total(d)]])

/** The tools of the history, most tokens first (a day kept before the other tools were read: Claude Code's). */
export function toolsOf(all: TokenDay[]): string[] {
  const n = new Map<string, number>()
  for (const d of all) for (const [t, x] of toolsOfDay(d)) n.set(t, (n.get(t) ?? 0) + x)
  return [...n].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([t]) => t)
}

/**
 * The days as one tool had them: each day's own part when it was kept apart. A day kept before
 * that is all its tool's when it has only one; with several (rare: the few days between the
 * other tools and this), the tool's share of everything, its models and projects left out.
 */
export function ofTool(all: TokenDay[], tool: string): TokenDay[] {
  const out: TokenDay[] = []
  for (const d of all) {
    const own = d.by_tool?.[tool]
    if (own) {
      out.push(own)
      continue
    }
    const tools = toolsOfDay(d)
    const n = tools.find(([t]) => t === tool)?.[1]
    if (!n) continue
    if (tools.length === 1) {
      out.push(d)
      continue
    }
    const k = total(d) ? n / total(d) : 0
    const part = (x: number) => Math.round(x * k)
    out.push({ ...d, input: part(d.input), output: part(d.output), cache_read: part(d.cache_read), cache_write: part(d.cache_write), replies: part(d.replies), cost: d.cost * k, unpriced: part(d.unpriced), models: [], projects: [], tools: [[tool, n]], by_tool: undefined })
  }
  return out
}

export interface Totals {
  tokens: number
  input: number
  output: number
  cache_read: number
  cache_write: number
  replies: number
  cost: number
  unpriced: number
  models: [string, number][]
  projects: [string, number][]
  tools: [string, number][]
}

export interface Period {
  from: Date
  to: Date
  /** Every day of the period, in order: its tokens (0 without a reply), and whether it is still to come. */
  days: { date: string; day: Date; tokens: number; cost: number; future: boolean }[]
  totals: Totals
  /** The period before: whole, or, for the period under way, as many of its first days as have passed. */
  before: Totals
  /** `before` covers only the days passed so far (the period is under way). */
  partial: boolean
}

export function sum(days: TokenDay[]): Totals {
  const t: Totals = { tokens: 0, input: 0, output: 0, cache_read: 0, cache_write: 0, replies: 0, cost: 0, unpriced: 0, models: [], projects: [], tools: [] }
  const models = new Map<string, number>()
  const projects = new Map<string, number>()
  const tools = new Map<string, number>()
  for (const d of days) {
    t.tokens += total(d)
    t.input += d.input
    t.output += d.output
    t.cache_read += d.cache_read
    t.cache_write += d.cache_write
    t.replies += d.replies
    t.cost += d.cost
    t.unpriced += d.unpriced
    for (const [m, n] of d.models) models.set(m, (models.get(m) ?? 0) + n)
    for (const [p, n] of d.projects) projects.set(p, (projects.get(p) ?? 0) + n)
    // A day kept before the other tools were read: all Claude Code's.
    for (const [x, n] of toolsOfDay(d)) tools.set(x, (tools.get(x) ?? 0) + n)
  }
  const ranked = (m: Map<string, number>) => [...m].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
  return { ...t, models: ranked(models), projects: ranked(projects), tools: ranked(tools) }
}

export function period(all: TokenDay[], span: Span, offset: number, today = new Date()): Period {
  const { from, to } = bounds(span, offset, today)
  const prev = bounds(span, offset + 1, today)
  const byDate = new Map(all.map((d) => [d.date, d]))
  const now = iso(today)
  const days: Period['days'] = []
  for (let d = new Date(from); d <= to; d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1)) {
    const date = iso(d)
    const x = byDate.get(date)
    days.push({ date, day: d, tokens: x ? total(x) : 0, cost: x?.cost ?? 0, future: date > now })
  }
  const within = (a: Date, b: Date) => all.filter((d) => d.date >= iso(a) && d.date <= iso(b))
  // Under way: against the same days of the period before (Monday to today, the 1st to the 15th).
  const elapsed = days.filter((d) => !d.future).length
  const partial = elapsed < days.length
  const prevTo = partial ? new Date(prev.from.getFullYear(), prev.from.getMonth(), prev.from.getDate() + elapsed - 1) : prev.to
  return { from, to, days, totals: sum(within(from, to)), before: sum(within(prev.from, prevTo < prev.to ? prevTo : prev.to)), partial }
}

/** `830`, `12k`, `3.4M`, `1.2B`. */
export function tokens(n: number): string {
  if (n >= 1e9) return `${+(n / 1e9).toFixed(1)}B`
  if (n >= 1e6) return `${+(n / 1e6).toFixed(1)}M`
  if (n >= 1e3) return `${Math.round(n / 1e3)}k`
  return `${n}`
}

export const dollars = (n: number) => (n >= 100 ? `$${Math.round(n)}` : `$${n.toFixed(2)}`)

/** `+12 %`, `−30 %` against the period before; null without anything before. */
export function change(now: number, before: number): string | null {
  if (!before) return null
  const p = Math.round(((now - before) / before) * 100)
  return `${p >= 0 ? '+' : '−'}${Math.abs(p)} %`
}
