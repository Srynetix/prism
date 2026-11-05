import { locale } from './locale'
import { t, tn } from './i18n.svelte'
import type { DeskEvent } from './digest.svelte'
import type { Snapshot } from './types'
import type { Routine } from './bindings/Routine'
import type { RoutineRun } from './bindings/RoutineRun'
import type { RoutineRunState } from './bindings/RoutineRunState'
import type { RoutineView } from './bindings/RoutineView'
import type { Routines } from './bindings/Routines'

/**
 * Routines (`[[agents.routines]]`), what reads the snapshot only: "To review" (blocked or failed
 * whatever their delivery, ready when delivered there), the states' words, the cron in words, the
 * Digest's events. What acts (the commands, the builtins' inputs): `routines.svelte.ts`.
 */

export const BUILTINS = ['builtin:alerts-triage', 'builtin:retro-draft'] as const
export const builtinLabel = (a: string) => (a === 'builtin:alerts-triage' ? t('Alerts triage') : a === 'builtin:retro-draft' ? t('Retro draft') : a)

const EMPTY: Routines = { routines: [], runs: [], keep_running: false }
export const routinesOf = (s: Snapshot | null | undefined): Routines => s?.routines ?? EMPTY

/** Waits in "To review" (the core's `RoutineRun::to_review`). */
export function toReview(r: RoutineRun): boolean {
  if (r.archived) return false
  return r.state === 'blocked' || r.state === 'failed' || r.state === 'too_long' || (r.state === 'ready' && r.deliver === 'review')
}
export const underWay = (r: RoutineRun) => r.state === 'running' || r.state === 'blocked'

/** The pile, the ones waiting on me first (blocked), then the most recent. */
export function reviewPile(s: Snapshot | null | undefined): RoutineRun[] {
  return routinesOf(s)
    .runs.filter(toReview)
    .sort((a, b) => Number(b.state === 'blocked') - Number(a.state === 'blocked') || b.due.localeCompare(a.due))
}

export const STATE_LABEL: Record<RoutineRunState, string> = {
  running: t('running'),
  blocked: t('blocked'),
  ready: t('ready'),
  nothing: t('nothing to do'),
  failed: t('failed'),
  too_long: t('too long'),
  skipped: t('skipped'),
}
export const STATE_ICON: Record<RoutineRunState, string> = { running: '…', blocked: '?', ready: '✓', nothing: '–', failed: '!', too_long: '!', skipped: '·' }

/** The agents the routines launched: archived at once, back when they wait for an answer. */
export function routineAgents(s: Snapshot | null | undefined): Map<string, RoutineRun> {
  return new Map(routinesOf(s).runs.flatMap((r) => (r.agent ? [[r.agent, r] as const] : [])))
}

/** The MR of a run's branch, when the forge has one. */
export function runMr(s: Snapshot | null | undefined, r: RoutineRun) {
  return r.branch ? s?.merge_requests?.find((m) => m.source_branch === r.branch && (!r.repo || m.repo === r.repo || m.repo?.endsWith(`/${r.repo}`))) : undefined
}

const hm = (iso: string) => new Date(iso).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
const dayHm = (iso: string) => new Date(iso).toLocaleString(locale(), { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })
export { hm as runTime, dayHm as nextTime }

/** "03:00 → 03:07", or since when it waits. */
export function runSpan(r: RoutineRun): string {
  const start = r.started ?? r.due
  if (r.ended) return `${hm(start)} → ${hm(r.ended)}`
  return hm(start)
}

/* The cron in words. */

const pad = (n: number) => String(n).padStart(2, '0')
function days(): string[] {
  // Sunday first, as cron counts them.
  const base = new Date(2026, 10, 1) // a Sunday
  return Array.from({ length: 7 }, (_, i) => new Date(base.getFullYear(), base.getMonth(), base.getDate() + i).toLocaleDateString(locale(), { weekday: 'long' }))
}
const NAMES = ['sun', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat']

/** A field's values, or null when it is more than a plain list (steps, ranges are spread). */
function values(field: string, min: number, max: number, names: string[] = []): number[] | null {
  const out: number[] = []
  for (const part of field.split(',')) {
    const [range, stepText] = part.split('/')
    const step = stepText ? Number(stepText) : 1
    if (!Number.isInteger(step) || step < 1) return null
    const num = (s: string) => {
      const i = names.indexOf(s.toLowerCase())
      return i >= 0 ? i + min : Number(s)
    }
    let [a, b] = range === '*' ? [min, max] : range.includes('-') ? range.split('-').map(num) : [num(range), stepText ? max : num(range)]
    if (!Number.isInteger(a) || !Number.isInteger(b) || a < min || b > max || a > b) return null
    for (; a <= b; a += step) out.push(a)
  }
  return [...new Set(out)].sort((x, y) => x - y)
}

/** `0 3 * * 1-5` → "Monday to Friday at 03:00"; what reads too oddly stays the cron. */
export function cronWords(cron: string): string {
  const f = cron.trim().split(/\s+/)
  if (f.length !== 5) return cron
  const [mi, ho, dom, mon, dow] = f
  const minutes = values(mi, 0, 59)
  const hours = values(ho, 0, 23)
  if (!minutes || !hours) return cron
  let time: string
  // A pace rather than times of day: alone when every day.
  const paced = ho === '*' || ho.startsWith('*/')
  if (mi.startsWith('*/') && ho === '*') time = tn(Number(mi.slice(2)), 'every minute', 'every {n} minutes')
  else if (mi === '*' && ho === '*') time = t('every minute')
  else if (ho === '*' && minutes.length === 1) time = t('every hour at :{m}', { m: pad(minutes[0]) })
  else if (hours.length * minutes.length <= 4) time = t('at {times}', { times: hours.flatMap((h) => minutes.map((m) => `${pad(h)}:${pad(m)}`)).join(', ') })
  else if (ho.startsWith('*/') && minutes.length === 1) time = t('every {n} hours at :{m}', { n: ho.slice(2), m: pad(minutes[0]) })
  else return cron
  if (mon !== '*') return `${cron}`
  const wd = values(dow, 0, 7, NAMES)?.map((d) => d % 7)
  const names = days()
  let when = ''
  if (dom !== '*') {
    const d = values(dom, 1, 31)
    if (!d) return cron
    when = t('on day {days} of the month', { days: d.join(', ') })
  } else if (dow === '*') when = t('every day')
  else if (!wd) return cron
  else {
    const set = [...new Set(wd)].sort()
    const weekdays = set.length === 5 && [1, 2, 3, 4, 5].every((d) => set.includes(d))
    if (weekdays) when = t('Monday to Friday')
    else if (set.length === 7) when = t('every day')
    else if (set.length === 2 && set.includes(0) && set.includes(6)) when = t('weekends')
    else when = t('on {days}', { days: set.map((d) => names[d]).join(', ') })
  }
  const text = paced && dom === '*' && dow === '*' ? time : `${when} ${time}`
  return text.charAt(0).toUpperCase() + text.slice(1)
}

/** `every = "6h"` → "every 6 h". */
export function everyWords(every: string): string {
  const m = /^\s*(\d+)\s*([mhd])\s*$/.exec(every)
  if (!m) return t('every {pace}', { pace: every })
  const n = Number(m[1])
  return m[2] === 'm' ? tn(n, 'every minute', 'every {n} minutes') : m[2] === 'h' ? tn(n, 'every hour', 'every {n} hours') : tn(n, 'every day', 'every {n} days')
}

export const whenWords = (r: Pick<Routine, 'cron' | 'every'>) => (r.cron?.trim() ? cronWords(r.cron) : r.every?.trim() ? everyWords(r.every) : t('never'))

/** Its view in the snapshot (next times, trust, error). */
export const viewOf = (s: Snapshot | null | undefined, name: string): RoutineView | undefined => routinesOf(s).routines.find((r) => r.name === name)

/* The Digest: a ready run delivered there is an event of kind `routine`, for a day. */

export function routineEvents(s: Snapshot): DeskEvent[] {
  const since = Date.now() - 86_400_000
  return routinesOf(s)
    .runs.filter((r) => r.deliver === 'digest' && r.state === 'ready' && r.ended && Date.parse(r.ended) > since)
    .map((r) => ({ key: `routine:${r.id}`, title: t('{name} ready', { name: r.routine }), body: r.said ?? t('its agent finished'), kind: 'routine' as const }))
}
