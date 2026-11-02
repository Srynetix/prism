import { pref, setPref } from './layout.svelte'
import { alertId } from './alert-bands'
import { isBusy } from './agenda.svelte'
import type { Alert } from './rules'
import type { Snapshot } from './types'
import { locale } from './locale'
import { t } from './i18n.svelte'

/*
 * Alerts put off (⏾): out of the list until a time, or until they change (their text: a new
 * count, a new state). Shared with the phone: put off on one, off on both. A mark lasts a week at
 * most, as "seen" does: a problem that stays does not stay hidden.
 */
const TTL_DAYS = 7
const SNOOZED = 'todo.snoozed'
const FIRST = 'todo.firstSeen'

/** `until`: ms, or `null` while its text stays `text`. `at`: when it was put off. */
interface Snooze { until: number | null; text: string; at: number }

/** Read each time: the phone's changes come in through the shared prefs. */
const marks = () => pref<Record<string, Snooze>>(SNOOZED, {})
/** Written without the marks expired. */
function save(next: Record<string, Snooze>) {
  const now = Date.now()
  setPref(SNOOZED, Object.fromEntries(Object.entries(next).filter(([, s]) => now - s.at < TTL_DAYS * 86_400_000 && (s.until === null || s.until > now))))
}

/** Put off at `now`: its time not come, or its text unchanged. */
export function isSnoozed(a: Alert, now = Date.now()): boolean {
  const s = marks()[alertId(a)]
  if (!s) return false
  return s.until === null ? s.text === a.text : s.until > now
}

/** When it comes back: ms, `null` when it changes. */
export const snoozedUntil = (a: Alert) => marks()[alertId(a)]?.until

export function snooze(alerts: Alert[], until: number | null) {
  const at = Date.now()
  save({ ...marks(), ...Object.fromEntries(alerts.map((a) => [alertId(a), { until, text: a.text, at }])) })
}

export function wake(alerts: Alert[]) {
  const ids = new Set(alerts.map(alertId))
  save(Object.fromEntries(Object.entries(marks()).filter(([id]) => !ids.has(id))))
}

/*
 * Since when each alert holds, when its data does not say (`Alert.since`): the first time this
 * window saw it. Forgotten once it is gone.
 */
const firstSeen = $state<Record<string, number>>(pref<Record<string, number>>(FIRST, {}))

/** Notes the new alerts and forgets those gone; written only when that changes something. */
export function noteAlerts(alerts: Alert[], now = Date.now()) {
  const ids = new Set(alerts.map(alertId))
  let changed = false
  for (const id of ids) {
    if (id in firstSeen) continue
    firstSeen[id] = now
    changed = true
  }
  for (const id of Object.keys(firstSeen)) {
    if (ids.has(id)) continue
    delete firstSeen[id]
    changed = true
  }
  if (changed) setPref(FIRST, { ...firstSeen })
}

/** Minutes it has held: from its data, otherwise from when it was first seen; `null` unknown. */
export function alertMinutes(a: Alert, now = Date.now()): number | null {
  const from = a.since ? new Date(a.since).getTime() : firstSeen[alertId(a)]
  return from && !Number.isNaN(from) ? Math.max(0, (now - from) / 60_000) : null
}

/** A choice of ⏾: its label, and until when (`null`: until it changes). */
export interface SnoozeChoice { key: string; label: string; detail?: string; until: number | null }

const hm = (ms: number) => new Date(ms).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })

/**
 * The times offered: an hour, this afternoon (before 13:30), the next working morning, the end of
 * the meeting under way or the next one today, and "when it changes".
 */
export function snoozeChoices(snap: Snapshot | null, now = Date.now()): SnoozeChoice[] {
  const out: SnoozeChoice[] = [{ key: 'hour', label: t('1 hour'), until: now + 3_600_000 }]
  const d = new Date(now)
  const at = (days: number, h: number, m = 0) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + days, h, m).getTime()
  if (now < at(0, 13, 30)) out.push({ key: 'afternoon', label: t('This afternoon'), detail: hm(at(0, 14)), until: at(0, 14) })
  // Friday, Saturday: Monday.
  const skip = d.getDay() === 5 ? 3 : d.getDay() === 6 ? 2 : 1
  out.push({ key: 'morning', label: skip > 1 ? t('Monday morning') : t('Tomorrow morning'), detail: hm(at(skip, 9)), until: at(skip, 9) })
  if (snap?.agenda?.access === 'granted') {
    const tonight = at(1, 0)
    const meeting = snap.agenda.events
      .filter((e) => !e.all_day && isBusy(e) && new Date(e.end).getTime() > now && new Date(e.start).getTime() < tonight)
      .sort((a, b) => a.start.localeCompare(b.start))[0]
    if (meeting) out.push({ key: 'meeting', label: t('After my meeting'), detail: hm(new Date(meeting.end).getTime()), until: new Date(meeting.end).getTime() })
  }
  out.push({ key: 'change', label: t('When it changes'), detail: t('a new count, a new state'), until: null })
  return out
}
