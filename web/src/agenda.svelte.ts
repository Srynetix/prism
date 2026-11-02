import { invoke, isDesktop } from './bridge'
import { canNotify, notify } from './notify'
import { live } from './live.svelte'
import { alertSettings } from './conventions'
import type { CalendarEvent } from './types'
import { locale } from './locale'
import { t } from './i18n.svelte'
import { rememberPanel } from './recent-panels.svelte'
import type { EventDetails } from './bindings/EventDetails'
import type { Attendee } from './bindings/Attendee'
export type { EventDetails, Attendee }

/** What really keeps one busy: neither declined, nor cancelled, nor marked free. */
export const isBusy = (e: CalendarEvent) => !e.canceled && !e.free && e.my_status !== 'declined'

/** Warn a little before a meeting starts. */
/** Notification before a meeting (`[alerts] meeting_lead_minutes`). */
export const meetingLeadMin = () => alertSettings().meeting_lead_minutes
const CHECK_MS = 20_000

/** Meetings already announced (id + start time: a moved meeting is announced again). */
const announced = new Set<string>()

/**
 * macOS notification a few minutes before each of the day's meetings where one is expected. Its
 * own clock, independent of snapshot changes: the calendar may not move
 * for hours while the time, for its part, moves on.
 */
export function startMeetingAlerts() {
  if (!isDesktop()) return () => {}
  const check = async () => {
    const events = live.snapshot?.agenda?.access === 'granted' ? live.snapshot.agenda.events : []
    const now = Date.now()
    const due = events.filter((e) => {
      const until = new Date(e.start).getTime() - now
      return !e.all_day && isBusy(e) && until > 0 && until <= meetingLeadMin() * 60_000 && !announced.has(`${e.id}:${e.start}`)
    })
    if (!due.length || !(await canNotify())) return
    for (const e of due) {
      announced.add(`${e.id}:${e.start}`)
      const minutes = Math.max(1, Math.round((new Date(e.start).getTime() - now) / 60_000))
      const at = new Date(e.start).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
      const where = e.join_url ? (e.join_url.includes('teams.microsoft') ? ' · Teams' : ' · visio') : e.location ? ` · ${e.location}` : ''
      notify({ title: t('In {minutes} min: {title}', { minutes, title: e.title }), body: `${at}${where}` })
    }
  }
  check()
  const timer = setInterval(check, CHECK_MS)
  return () => clearInterval(timer)
}

/** Detail pane of an event: nothing is kept once closed. */
export const eventView = $state({
  id: null as string | null,
  details: null as EventDetails | null,
  error: null as string | null,
})

export async function openEvent(id: string) {
  Object.assign(eventView, { id, details: null, error: null })
  try {
    const details = await invoke<EventDetails>('calendar_event', { id })
    if (eventView.id === id) eventView.details = details
  } catch (e) {
    if (eventView.id === id) eventView.error = String(e)
  }
}

export function closeEvent() {
  const id = eventView.id
  if (id) rememberPanel({ key: `event:${id}`, kind: 'event', title: eventView.details?.title ?? live.snapshot?.agenda?.events.find((e) => e.id === id)?.title ?? t('Event'), hint: t('Calendar event'), reopen: () => openEvent(id) })
  Object.assign(eventView, { id: null, details: null, error: null })
}

/** The Calendar app, opened on the event: that is where the invitation is answered. */
export const openInCalendar = (id: string) => invoke<void>('open_in_calendar', { id })
