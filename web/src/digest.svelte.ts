import { isDesktop } from './bridge'
import { alertSettings } from './conventions'
import { canNotify, notify } from './notify'
import { pref, setPref, zen } from './layout.svelte'
import { live } from './live.svelte'
import { prismName } from './profiles.svelte'
import { t } from './i18n.svelte'

/**
 * When Prism interrupts (`[alerts] digest`, `quiet_hours`, `focus_events`). Live, each new event
 * notifies at once; in digest mode it is held, and one summary goes out at each of
 * `digest_times`, the `digest_urgent` kinds still at once. Quiet hours and focus (a calendar
 * event named so, or Zen mode) hold everything, urgent included; what they held goes out as a
 * summary once they end. A held event gone before the summary (a pipeline green again) is told
 * as resolved meanwhile. Meeting and note reminders are not events: they keep their time.
 */

/** What a notification is about, for `digest_urgent`. */
export type EventKind = 'infra' | 'deploy' | 'agent' | 'pipeline' | 'review' | 'data' | 'block' | 'routine'
export interface DeskEvent {
  key: string
  title: string
  body: string
  kind: EventKind
}
export interface Held extends DeskEvent {
  /** When it appeared (ISO). */
  at: string
}
/** A summary gone out: what it held, what resolved meanwhile, what went out at once since the one before. */
export interface Delivered {
  at: string
  held: Held[]
  resolved: Held[]
  sent: Held[]
}
interface DigestState {
  held: Held[]
  resolved: Held[]
  sent: Held[]
  /** The last summary's time (ISO); none yet: the first due time from now on. */
  last: string | null
  delivered: Delivered | null
}

const KEY = 'digest'
export const digest = $state<DigestState>({ held: [], resolved: [], sent: [], last: null, delivered: null, ...pref<Partial<DigestState>>(KEY, {}) })
const save = () => setPref(KEY, $state.snapshot(digest))

/** `HH:MM` → minutes of the day; `null` when it does not read. */
export function minutes(hm: string): number | null {
  const m = /^\s*(\d{1,2}):(\d{2})\s*$/.exec(hm)
  if (!m) return null
  const h = Number(m[1])
  const min = Number(m[2])
  return h < 24 && min < 60 ? h * 60 + min : null
}

/** `19:00-08:00`, over midnight or not. */
export function inSpan(span: string, at: Date): boolean {
  const [a, b] = span.split('-').map(minutes)
  if (a == null || b == null || a === b) return false
  const m = at.getHours() * 60 + at.getMinutes()
  return a < b ? m >= a && m < b : m >= a || m < b
}

/** The last digest time at or before `at`: today's, otherwise yesterday's last. */
export function lastDue(times: string[], at: Date): Date | null {
  const day = times.map(minutes).filter((m): m is number => m !== null).sort((x, y) => x - y)
  if (!day.length) return null
  const now = at.getHours() * 60 + at.getMinutes()
  const today = day.filter((m) => m <= now).at(-1)
  const d = new Date(at)
  d.setSeconds(0, 0)
  if (today !== undefined) {
    d.setHours(Math.floor(today / 60), today % 60)
    return d
  }
  d.setDate(d.getDate() - 1)
  const last = day.at(-1)!
  d.setHours(Math.floor(last / 60), last % 60)
  return d
}

/** The next digest time after `at`, for the status bar. */
export function nextDue(times: string[], at: Date): Date | null {
  const day = times.map(minutes).filter((m): m is number => m !== null).sort((x, y) => x - y)
  if (!day.length) return null
  const now = at.getHours() * 60 + at.getMinutes()
  const next = day.find((m) => m > now)
  const d = new Date(at)
  d.setSeconds(0, 0)
  if (next === undefined) d.setDate(d.getDate() + 1)
  const m = next ?? day[0]
  d.setHours(Math.floor(m / 60), m % 60)
  return d
}

/** Quiet hours, or focus: nothing goes out. */
export function silenced(at: Date): 'quiet' | 'focus' | null {
  const cfg = alertSettings()
  if (cfg.quiet_hours && inSpan(cfg.quiet_hours, at)) return 'quiet'
  if (zen.on) return 'focus'
  const word = cfg.focus_events?.trim().toLowerCase()
  const ms = at.getTime()
  if (word && (live.snapshot?.agenda?.events ?? []).some((e) => !e.canceled && e.title.toLowerCase().includes(word) && Date.parse(e.start) <= ms && Date.parse(e.end) > ms)) return 'focus'
  return null
}

/**
 * The new events of a pass, routed: those to notify now are returned, the others held. Held
 * events gone from `current` move to "resolved meanwhile".
 */
export function route(fresh: DeskEvent[], current: Set<string>, at = new Date()): DeskEvent[] {
  const cfg = alertSettings()
  const iso = at.toISOString()
  let changed = false
  const gone = digest.held.filter((h) => !current.has(h.key))
  if (gone.length) {
    digest.held = digest.held.filter((h) => current.has(h.key))
    digest.resolved = [...digest.resolved, ...gone]
    changed = true
  }
  const off = silenced(at)
  const urgent = new Set(cfg.digest_urgent ?? [])
  const now = off ? [] : cfg.digest ? fresh.filter((e) => urgent.has(e.kind)) : fresh
  const hold = fresh.filter((e) => !now.includes(e))
  if (hold.length) {
    digest.held = [...digest.held.filter((h) => !hold.some((e) => e.key === h.key)), ...hold.map((e) => ({ ...e, at: iso }))]
    changed = true
  }
  if (now.length && cfg.digest) {
    digest.sent = [...digest.sent, ...now.map((e) => ({ ...e, at: iso }))]
    changed = true
  }
  if (changed) save()
  return now
}

/**
 * Every minute: a summary due (a digest time passed since the last one, or quiet hours or focus
 * just ended with events held) goes out, unless still silenced.
 */
export async function tick(at = new Date()) {
  const cfg = alertSettings()
  if (silenced(at)) return
  if (!digest.last) {
    digest.last = at.toISOString()
    return save()
  }
  const due = cfg.digest ? lastDue(cfg.digest_times ?? [], at) : null
  const timeFor = due ? due.getTime() > Date.parse(digest.last) : false
  // Live mode: what quiet hours or focus held goes out as soon as they end.
  if (timeFor || (!cfg.digest && digest.held.length)) await deliver(at)
}

/** The summary now: one notification, and the panel's content. */
export async function deliver(at = new Date()) {
  const held = digest.held
  const resolved = digest.resolved
  const sent = digest.sent
  if (held.length || resolved.length) {
    digest.delivered = { at: at.toISOString(), held, resolved, sent }
    if (isDesktop() && (await canNotify())) {
      const head = [held.length && t('{n} for you', { n: held.length }), resolved.length && t('{n} resolved meanwhile', { n: resolved.length })].filter(Boolean).join(' · ')
      const lines = held.slice(0, 3).map((h) => `${h.title}: ${h.body}`)
      notify({ title: t('{name} · digest', { name: prismName() }), body: [head, ...lines, held.length > 3 ? t('+{n} more', { n: held.length - 3 }) : ''].filter(Boolean).join('\n') })
    }
  }
  digest.held = []
  digest.resolved = []
  digest.sent = []
  digest.last = at.toISOString()
  save()
}

/** The last summary read: the status bar stops showing it. */
export function clearDelivered() {
  digest.delivered = null
  save()
}

let timer: ReturnType<typeof setInterval> | null = null
export function startDigest() {
  if (timer) return
  timer = setInterval(() => void tick(), 30_000)
}
