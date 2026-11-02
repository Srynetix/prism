import type { TimeEntry } from './types'
import { t } from './i18n.svelte'

/**
 * Time entries by day, week and month, for the time tracker's Day, Week and Month views. Days are
 * local `YYYY-MM-DD` keys; weeks run Monday → Sunday; the target counts Monday to Friday only.
 */

const pad = (n: number) => String(n).padStart(2, '0')
/** Local day key of a date (as `dayKey`, without the locale). */
export const keyOf = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
const at = (k: string) => new Date(`${k}T12:00`)
export const entryDay = (e: TimeEntry) => keyOf(new Date(e.start))
export const total = (entries: TimeEntry[]) => entries.reduce((n, e) => n + e.duration_ms, 0)

/** `45m`, `1h05`, `12h00`. */
export function hours(ms: number): string {
  const m = Math.round(ms / 60000)
  return m < 60 ? t('{n}m', { n: m }) : `${Math.floor(m / 60)}h${pad(m % 60)}`
}

export const isWorkday = (k: string) => ![0, 6].includes(at(k).getDay())

/** The day `n` days after `k`. */
export function addDays(k: string, n: number): string {
  const d = at(k)
  d.setDate(d.getDate() + n)
  return keyOf(d)
}

/** The seven days (Monday → Sunday) of the week of `k`. */
export function weekOf(k: string): string[] {
  const monday = addDays(k, -((at(k).getDay() + 6) % 7))
  return Array.from({ length: 7 }, (_, i) => addDays(monday, i))
}

/** `YYYY-MM` of a day. */
export const monthOf = (k: string) => k.slice(0, 7)

/** The first day of the month `n` months after that of `k`. */
export function addMonths(k: string, n: number): string {
  const d = new Date(Number(k.slice(0, 4)), Number(k.slice(5, 7)) - 1 + n, 1, 12)
  return keyOf(d)
}

/** The weeks (Monday → Sunday) covering the month of `k`, days outside it included. */
export function monthWeeks(k: string): string[][] {
  const first = `${monthOf(k)}-01`
  const next = addMonths(k, 1)
  const weeks: string[][] = []
  for (let monday = weekOf(first)[0]; monday < next; monday = addDays(monday, 7)) weeks.push(weekOf(monday))
  return weeks
}

/** Time logged per day. */
export function byDay(entries: TimeEntry[]): Map<string, number> {
  const days = new Map<string, number>()
  for (const e of entries) days.set(entryDay(e), (days.get(entryDay(e)) ?? 0) + e.duration_ms)
  return days
}

export interface TicketRow {
  /** Ticket id, otherwise the task's name. */
  key: string
  ticket: string | null
  name: string
  url: string | null
  total: number
  /** Time per day. */
  days: Map<string, number>
}

/** Time per ticket over `days`, most logged first. */
export function byTicket(entries: TimeEntry[], days: string[]): TicketRow[] {
  const keep = new Set(days)
  const rows = new Map<string, TicketRow>()
  for (const e of entries) {
    const day = entryDay(e)
    if (!keep.has(day)) continue
    const key = e.ticket ?? e.task_name ?? '—'
    const row = rows.get(key) ?? { key, ticket: e.ticket, name: e.task_name ?? '', url: e.task_url, total: 0, days: new Map() }
    row.total += e.duration_ms
    row.days.set(day, (row.days.get(day) ?? 0) + e.duration_ms)
    rows.set(key, row)
  }
  return [...rows.values()].sort((a, b) => b.total - a.total || a.key.localeCompare(b.key))
}

/** Target of `days`: `dayMs` per working day. */
export const targetOf = (days: string[], dayMs: number) => days.filter(isWorkday).length * dayMs

/** Green when the target is met, orange for a past incomplete working day. */
export function tone(k: string, ms: number, dayMs: number, today: string): '' | 'ok' | 'warn' {
  return ms >= dayMs ? 'ok' : isWorkday(k) && k < today ? 'warn' : ''
}
