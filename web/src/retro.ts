import { byTicket, tracesByDay, type Trace, type TrailState } from './blocks/trail/trail'
import { conventions, isDone, namesTicket } from './conventions'
import { dayKey } from './filter.svelte'
import { meOn } from './forge'
import { locale } from './locale'
import { t as tr, tn } from './i18n.svelte'
import type { Note } from './notes.svelte'
import { tagsOf } from './notes.svelte'
import { pointsMarkdown, type RetroPoint } from './retro-points.svelte'
import type { Snapshot, Ticket } from './types'

/**
 * My sprint: the current sprint seen from your side, for the retro. Your tickets (delivered,
 * carried and why, estimate against time), where your time went and how long you could focus,
 * from the Day trail's traces; your reviews, agents and notes tagged #retro. Compared with your
 * own past sprints only (kept locally, `RetroSummary`), never with anyone else: the team shows as
 * one line. Facts Prism saw, not judgments.
 */

const MIN = 60_000
const DAY = 86_400_000
/** A day with more ticket switches than this is flagged. */
const SWITCHES = 8
/** Time beyond this share of the estimate is flagged. */
const OVERRUN = 1.5
/** Meetings beyond this share of the tracked time are flagged. */
const MEETINGS = 0.3

export interface MyTicket {
  t: Ticket
  done: boolean
  /** Minutes: ClickUp's time estimate; logged; estimated from the traces. */
  estimate: number | null
  logged: number
  traced: number
  /** From its first move into an in-progress status to done, as the trail saw them. */
  cycleDays: number | null
  /** Carried: why, in a few words. */
  why: string | null
}

export interface RetroDay {
  day: string
  /** Minutes: the longest stretch without a meeting between the day's first and last trace. */
  focus: number
  switches: number
  meetings: number
  traced: number
}

export interface Observation {
  good: boolean
  text: string
}

/** One sprint's figures, kept to compare the next ones with (`pref('retro.history')`). */
export interface RetroSummary {
  name: string
  end: string | null
  delivered: number
  committed: number
  /** Minutes traced, and in meetings. */
  tracked: number
  meetings: number
  focusMedian: number
  switchesMedian: number
  reviews: number
  agents: number
}

export interface Retro {
  name: string
  start: string
  end: string | null
  /** Day of the sprint, and how many it has. */
  day: number
  days: number
  mine: MyTicket[]
  delivered: number
  /** Minutes traced: on sprint tickets, on other tickets, on no ticket; in meetings. */
  time: { sprint: number; other: number; none: number; meetings: number }
  daily: RetroDay[]
  /** Your approvals and comments on others' MRs. */
  reviews: number
  /** Your MRs merged in the sprint, and those an agent worked on (its branch). */
  merged: number
  byAgents: number
  agents: number
  cost: number
  notes: Note[]
  observations: Observation[]
  team: { done: number; total: number }
  /** The trail is on: time, focus and reviews are known. */
  traced: boolean
  summary: RetroSummary
}

const lower = (xs: string[] | undefined) => new Set((xs ?? []).map((x) => x.toLowerCase()))
export const median = (xs: number[]) => {
  if (!xs.length) return 0
  const s = [...xs].sort((a, b) => a - b)
  const m = Math.floor(s.length / 2)
  return s.length % 2 ? s[m] : Math.round((s[m - 1] + s[m]) / 2)
}
/** `45 min`, `3 h`, `3 h 20`. */
export const hours = (min: number) => (min < 60 ? `${Math.round(min)} min` : `${Math.floor(min / 60)} h${min % 60 ? ` ${String(Math.round(min % 60)).padStart(2, '0')}` : ''}`)

/** The longest stretch of `[from, to]` no meeting covers, in minutes. */
export function longestFree(from: number, to: number, meetings: [number, number][]): number {
  const busy = meetings.filter(([a, b]) => b > from && a < to).sort((x, y) => x[0] - y[0])
  let best = 0
  let cursor = from
  for (const [a, b] of busy) {
    best = Math.max(best, a - cursor)
    cursor = Math.max(cursor, b)
  }
  best = Math.max(best, to - cursor)
  return Math.round(Math.max(0, best) / MIN)
}

/** What a trace is about, to count switches: its ticket, else its MR, else its repository. */
const subject = (tr: Trace) => tr.ticket ?? (tr.mr ? `${tr.repo ?? ''}!${tr.mr}` : undefined)

export function dayFigures(list: Trace[], day: string): RetroDay | null {
  const work = list.filter((tr) => tr.kind !== 'meeting' && tr.kind !== 'time')
  if (!work.length) return null
  const meetings = list.filter((tr) => tr.kind === 'meeting').map((tr): [number, number] => [tr.t, tr.end ?? tr.t + (tr.minutes ?? 0) * MIN])
  const from = Math.min(...work.map((tr) => tr.t))
  const to = Math.max(...work.map((tr) => (tr.kind === 'agent' ? tr.t : (tr.end ?? tr.t))))
  let switches = 0
  let last: string | undefined
  for (const tr of work) {
    const s = subject(tr)
    if (!s) continue
    if (last && s !== last) switches++
    last = s
  }
  const { tickets, meetings: met } = byTicket(list, () => undefined)
  return { day, focus: longestFree(from, to, meetings), switches, meetings: met, traced: tickets.reduce((n, g) => n + g.traced, 0) }
}

/** Why a ticket was carried: blocked, never started, red CI, otherwise its status. */
function carriedWhy(t: Ticket, s: Snapshot): string {
  const waiting = (t.waiting_on ?? []).filter((d) => !d.closed)
  if (waiting.length) return tr('blocked by {list}', { list: waiting.map((d) => d.id || d.name).join(', ') })
  if (lower(conventions().before_dev_statuses).has(t.status.toLowerCase())) return tr('never started')
  const mr = s.merge_requests.find((m) => namesTicket(m, t.id) && m.state === 'opened')
  if (mr?.pipeline === 'failed') return tr('red CI')
  if (mr) return tr('MR in review')
  return t.status
}

export function retro(s: Snapshot, trail: TrailState | undefined, notes: Note[], history: RetroSummary[], now = Date.now()): Retro | null {
  const sprint = s.sprint
  if (!sprint?.start) return null
  const start = Date.parse(sprint.start)
  const end = sprint.end ? Date.parse(sprint.end) : now
  const until = Math.min(now, end)
  const me = s.tracker_me
  const mineT = me ? sprint.tickets.filter((t) => t.assignees.some((a) => a.id === me.id)) : []
  const sprintIds = new Set(sprint.tickets.map((t) => t.id))

  // The trail, day by day over the sprint so far.
  const time = { sprint: 0, other: 0, none: 0, meetings: 0 }
  const tracedBy = new Map<string, number>()
  const daily: RetroDay[] = []
  let reviews = 0
  const sprintDays: string[] = []
  for (let d = new Date(start); d.getTime() <= until; d = new Date(d.getTime() + DAY)) sprintDays.push(dayKey(d))
  const byDay = tracesByDay(trail, s, sprintDays)
  for (const day of sprintDays) {
    const list = (byDay.get(day) ?? []).filter((tr) => tr.t >= start && tr.t <= until)
    if (!list.length) continue
    const { tickets, meetings } = byTicket(list, () => undefined)
    time.meetings += meetings
    for (const g of tickets) {
      if (!g.ticket) time.none += g.traced
      else if (sprintIds.has(g.ticket)) time.sprint += g.traced
      else time.other += g.traced
      if (g.ticket) tracedBy.set(g.ticket, (tracedBy.get(g.ticket) ?? 0) + g.traced)
    }
    reviews += list.filter((tr) => (tr.kind === 'mr_approved' || tr.kind === 'mr_commented') && tr.others).length
    // Focus and switches need the trail's own traces: agents alone would tell a wrong day.
    const f = trail ? dayFigures(list, day) : null
    if (f) daily.push(f)
  }

  // A ticket's cycle: its first move into progress, then into done, as the trail recorded them.
  const progress = lower(conventions().in_progress_statuses)
  const doneSt = lower(conventions().done_statuses)
  const cycle = (id: string): number | null => {
    const moves = (trail?.events ?? []).filter((e) => e.kind === 'ticket_moved' && e.ticket === id).sort((a, b) => a.at.localeCompare(b.at))
    const from = moves.find((e) => e.to && progress.has(e.to.toLowerCase()))
    const to = [...moves].reverse().find((e) => e.to && doneSt.has(e.to.toLowerCase()))
    return from && to && to.at > from.at ? Math.round(((Date.parse(to.at) - Date.parse(from.at)) / DAY) * 10) / 10 : null
  }
  const logged = (id: string) => Math.round(s.time.entries.filter((e) => e.ticket === id && Date.parse(e.start) >= start).reduce((n, e) => n + e.duration_ms, 0) / MIN)
  const mine: MyTicket[] = mineT.map((t) => {
    const done = isDone(t)
    return { t, done, estimate: t.time_estimate ? Math.round(t.time_estimate / MIN) : null, logged: logged(t.id), traced: tracedBy.get(t.id) ?? 0, cycleDays: done ? cycle(t.id) : null, why: done ? null : carriedWhy(t, s) }
  })
  mine.sort((a, b) => Number(b.done) - Number(a.done) || a.t.id.localeCompare(b.t.id))
  const delivered = mine.filter((m) => m.done).length

  const merged = s.merge_requests.filter((m) => m.state === 'merged' && m.author === meOn(s, m) && m.merged_at && Date.parse(m.merged_at) >= start)
  const agents = s.agents.filter((a) => Date.parse(a.created_at) >= start)
  const agentBranches = new Set(agents.map((a) => a.branch).filter(Boolean))
  const byAgents = merged.filter((m) => agentBranches.has(m.source_branch)).length
  const cost = agents.reduce((n, a) => n + (a.usage?.cost ?? 0), 0)
  const retroNotes = notes.filter((n) => tagsOf(n.text).includes('retro') && Date.parse(n.created_at) >= start)
  const teamDone = sprint.tickets.filter(isDone).length

  const tracked = time.sprint + time.other + time.none
  const summary: RetroSummary = {
    name: sprint.name,
    end: sprint.end,
    delivered,
    committed: mine.length,
    tracked,
    meetings: time.meetings,
    focusMedian: median(daily.map((d) => d.focus)),
    switchesMedian: median(daily.map((d) => d.switches)),
    reviews,
    agents: agents.length,
  }
  const previous = history.filter((h) => h.name !== sprint.name).at(-1)
  const observations = observe({ mine, daily, time, tracked, merged: merged.length, byAgents, summary, previous })
  const days = Math.max(1, Math.round((end - start) / DAY))
  return {
    name: sprint.name,
    start: sprint.start,
    end: sprint.end,
    day: Math.min(days, Math.max(1, Math.floor((now - start) / DAY) + 1)),
    days,
    mine,
    delivered,
    time,
    daily,
    reviews,
    merged: merged.length,
    byAgents,
    agents: agents.length,
    cost,
    notes: retroNotes,
    observations,
    team: { done: teamDone, total: sprint.tickets.length },
    traced: !!trail,
    summary,
  }
}

const weekday = (day: string) => new Date(`${day}T12:00:00`).toLocaleDateString(locale(), { weekday: 'long', day: 'numeric', month: 'short' })

function observe(x: { mine: MyTicket[]; daily: RetroDay[]; time: Retro['time']; tracked: number; merged: number; byAgents: number; summary: RetroSummary; previous?: RetroSummary }): Observation[] {
  const out: Observation[] = []
  const { previous: p, summary: now } = x
  if (p) {
    const d = now.delivered - p.delivered
    if (d) out.push({ good: d > 0, text: d > 0 ? tn(d, '{n} ticket more delivered than in {sprint} ({now} against {before}).', '{n} tickets more delivered than in {sprint} ({now} against {before}).', { sprint: p.name, now: now.delivered, before: p.delivered }) : tn(-d, '{n} ticket fewer delivered than in {sprint} ({now} against {before}).', '{n} tickets fewer delivered than in {sprint} ({now} against {before}).', { sprint: p.name, now: now.delivered, before: p.delivered }) })
    const f = now.focusMedian - p.focusMedian
    if (Math.abs(f) >= 20) out.push({ good: f > 0, text: tr(f > 0 ? 'Your median longest focus grew by {by} ({median} a day).' : 'Your median longest focus shrank by {by} ({median} a day).', { by: hours(Math.abs(f)), median: hours(now.focusMedian) }) })
    if (now.reviews !== p.reviews && Math.abs(now.reviews - p.reviews) >= 3) out.push({ good: now.reviews > p.reviews, text: tn(now.reviews, '{n} review given, against {before} in {sprint}.', '{n} reviews given, against {before} in {sprint}.', { before: p.reviews, sprint: p.name }) })
  }
  for (const m of x.mine) {
    const spent = Math.max(m.logged, m.traced)
    if (m.estimate && spent > m.estimate * OVERRUN) out.push({ good: false, text: tr('{id} took {spent} for {estimate} estimated.', { id: m.t.id, spent: hours(spent), estimate: hours(m.estimate) }) })
  }
  const carried = x.mine.filter((m) => !m.done)
  for (const m of carried.slice(0, 3)) out.push({ good: false, text: tr('{id} carried: {why}.', { id: m.t.id, why: m.why ?? '' }) })
  const busiest = [...x.daily].sort((a, b) => b.switches - a.switches)[0]
  if (busiest && busiest.switches >= SWITCHES) out.push({ good: false, text: tr('{day}: {n} switches between tickets and MRs.', { day: weekday(busiest.day), n: busiest.switches }) })
  const best = [...x.daily].sort((a, b) => b.focus - a.focus)[0]
  if (best && best.focus >= 120) out.push({ good: true, text: tr('Your longest focus: {time} on {day}.', { time: hours(best.focus), day: weekday(best.day) }) })
  if (x.tracked + x.time.meetings > 0 && x.time.meetings / (x.tracked + x.time.meetings) > MEETINGS) out.push({ good: false, text: tr('Meetings took {n} % of your tracked time.', { n: Math.round((100 * x.time.meetings) / (x.tracked + x.time.meetings)) }) })
  if (x.merged && x.byAgents) out.push({ good: true, text: tr('Agents worked on {n} of your {merged} merged MRs.', { n: x.byAgents, merged: x.merged }) })
  return out
}

/** The retro as Markdown, with the points written for it: to paste in the team's retro, or to hand to an agent. */
export function retroMarkdown(r: Retro, points: RetroPoint[] = []): string {
  const lines = [
    tr('## My sprint · {name}', { name: r.name }),
    '',
    tr('- Delivered {n} of my {total} tickets (team: {done}/{all}).', { n: r.delivered, total: r.mine.length, done: r.team.done, all: r.team.total }),
  ]
  if (r.traced) {
    const all = r.time.sprint + r.time.other + r.time.none + r.time.meetings
    if (all) lines.push(tr('- Time traced: {sprint} on sprint tickets, {other} on others, {none} on no ticket, {meetings} in meetings.', { sprint: hours(r.time.sprint), other: hours(r.time.other), none: hours(r.time.none), meetings: hours(r.time.meetings) }))
    if (r.daily.length) lines.push(tr('- Longest focus, median: {focus} a day; switches, median: {switches} a day.', { focus: hours(median(r.daily.map((d) => d.focus))), switches: median(r.daily.map((d) => d.switches)) }))
    lines.push(tr('- Reviews given: {n}.', { n: r.reviews }))
  }
  if (r.agents) lines.push(tr('- Agents: {n} launched, ${cost}; {by} of my {merged} merged MRs.', { n: r.agents, cost: r.cost.toFixed(2), by: r.byAgents, merged: r.merged }))
  const carried = r.mine.filter((m) => !m.done)
  if (carried.length) lines.push('', tr('### Carried'), ...carried.map((m) => `- ${m.t.id} ${m.t.name}: ${m.why}`))
  if (r.observations.length) lines.push('', tr('### Seen by Prism'), ...r.observations.map((o) => `- ${o.good ? '▲' : '▼'} ${o.text}`))
  lines.push(...pointsMarkdown(points))
  if (r.notes.length) lines.push('', tr('### My notes'), ...r.notes.map((n) => `- ${n.text.replace(/\s*#retro\b/gi, '').trim()}`))
  return lines.join('\n')
}
