import type { MergeRequest, Snapshot } from '../../types'
import { dayKey } from '../../filter.svelte'
import { awaitsMyReview } from '../../rules'
import { conventions, mrTickets } from '../../conventions'
import { locale } from '../../locale'
import { t, tn, tc } from '../../i18n.svelte'
import type { TrailKind } from '../../bindings/TrailKind'
import type { TrailEvent } from '../../bindings/TrailEvent'
import type { TrailState } from '../../bindings/TrailState'

/** The journal and its traces: Rust's types (`crates/blocks/src/trail`), generated in `bindings/`. */
export type { TrailKind, TrailEvent, TrailState }

/** A trace as the block shows it: the journal's, and the window's own (agents launched, time logged). */
export type TraceKind = TrailKind | 'agent' | 'time'
export interface Trace extends Omit<TrailEvent, 'kind'> {
  kind: TraceKind
  /** When it ends: a meeting's, an agent's last activity, a time entry's. */
  end?: number
  /** An agent's cost so far (its conversation, at the API prices). */
  cost?: number
  t: number
}

const MIN = 60_000

/** The traces of `day` (`YYYY-MM-DD`, local), oldest first. */
export const traces = (state: TrailState | undefined, s: Snapshot, day: string): Trace[] => tracesByDay(state, s, [day]).get(day) ?? []

/**
 * The traces of each of `days` (`YYYY-MM-DD`, local), oldest first, in one pass over the journal,
 * the agents and the time entries: a sprint's days at once (My sprint) cost one day's read.
 */
export function tracesByDay(state: TrailState | undefined, s: Snapshot, days: string[]): Map<string, Trace[]> {
  const out = new Map(days.map((d) => [d, [] as Trace[]]))
  const add = (at: number, trace: () => Trace) => out.get(dayKey(new Date(at)))?.push(trace())
  for (const e of state?.events ?? []) {
    const at = Date.parse(e.at)
    add(at, () => ({ ...e, t: at, end: e.minutes ? at + e.minutes * MIN : undefined }))
  }
  for (const a of s.agents ?? []) {
    const at = Date.parse(a.created_at)
    add(at, () => {
      const last = a.last_activity ? Date.parse(a.last_activity) : at
      return { id: `agent:${a.id}`, at: a.created_at, t: at, end: Math.max(at, last), kind: 'agent', title: a.title, ticket: a.ticket ?? undefined, branch: a.branch ?? undefined, cost: a.usage?.cost ?? undefined }
    })
  }
  for (const e of s.time.entries) {
    const at = Date.parse(e.start)
    add(at, () => ({ id: `time:${e.id}`, at: e.start, t: at, end: at + e.duration_ms, kind: 'time', title: e.description || e.task_name || '', ticket: e.ticket ?? undefined, minutes: Math.round(e.duration_ms / MIN) }))
  }
  for (const list of out.values()) list.sort((a, b) => a.t - b.t)
  return out
}

/* ---------- Timeline ---------- */

/** A row of the timeline: one trace, or several alike in a row (commits on a branch, comments on an MR). */
export interface Row {
  key: string
  t: number
  kind: TraceKind
  items: Trace[]
}
export type TimelineItem = { row: Row } | { gap: { from: number; to: number } }

/** Two traces alike close enough share a row. */
const SAME_ROW_MS = 90 * MIN
const sameRow = (a: Trace, b: Trace) =>
  a.kind === b.kind && b.t - a.t < SAME_ROW_MS && ((a.kind === 'commit' && a.branch === b.branch && a.repo === b.repo) || (a.kind === 'mr_commented' && a.mr === b.mr && a.repo === b.repo))

/**
 * The day's rows, with the stretches without any trace longer than `gapMinutes` between 9:00 and
 * 18:30 (and before now): time worked that leaves no trace, often time not logged. Logged time
 * covers a stretch like any other trace.
 */
export function timeline(list: Trace[], gapMinutes: number, day: string, now = Date.now()): TimelineItem[] {
  const rows: Row[] = []
  for (const tr of list) {
    const last = rows[rows.length - 1]
    if (last && sameRow(last.items[last.items.length - 1], tr)) last.items.push(tr)
    else rows.push({ key: tr.id, t: tr.t, kind: tr.kind, items: [tr] })
  }
  if (!gapMinutes) return rows.map((row) => ({ row }))
  const dayStart = Date.parse(`${day}T09:00`)
  const dayEnd = Math.min(Date.parse(`${day}T18:30`), now)
  const out: TimelineItem[] = []
  let busy = dayStart
  for (const row of rows) {
    if (row.t - busy >= gapMinutes * MIN && row.t > dayStart) out.push({ gap: { from: busy, to: Math.min(row.t, dayEnd) } })
    out.push({ row })
    busy = Math.max(busy, ...row.items.map((i) => i.end ?? i.t))
  }
  if (dayEnd - busy >= gapMinutes * MIN && list.length) out.push({ gap: { from: busy, to: dayEnd } })
  return out
}

/* ---------- By ticket ---------- */

export interface TicketTime {
  /** `''`: traces on no ticket. */
  ticket: string
  title: string
  traces: Trace[]
  /** Minutes Prism estimates from the traces, and minutes already logged. */
  traced: number
  logged: number
  /** When its first trace was: where an entry logged from here starts. */
  first: number
  /** What its agents of the day cost (API prices), when known. */
  cost: number
}

/** A title without its ticket id up front (`SHOP-441 · Refine…` → `Refine…`). */
export const withoutTicket = (title: string, ticket: string) => title.replace(new RegExp(`^\\s*${ticket}\\s*[·:\\-–]?\\s*`, 'i'), '') || title

/** The time a trace claims before it: since the previous one, at most this. */
const CLAIM_MAX = 45
/** The day's first trace, a lone one: this much. */
const CLAIM_FIRST = 15

/**
 * Time by ticket: each trace claims the time since the previous one (at most 45 minutes,
 * meetings excluded), an agent its own run; logged time is counted apart. Meetings are a group
 * of their own. An estimate only: what Prism proposes to log, never logs by itself.
 */
export function byTicket(list: Trace[], titleOf: (ticket: string) => string | undefined): { tickets: TicketTime[]; meetings: number } {
  const groups = new Map<string, TicketTime>()
  const group = (tr: Trace) => {
    const ticket = tr.ticket ?? ''
    let g = groups.get(ticket)
    if (!g) groups.set(ticket, (g = { ticket, title: (ticket && titleOf(ticket)) || (ticket ? withoutTicket(tr.title, ticket) : t('No ticket')), traces: [], traced: 0, logged: 0, first: tr.t, cost: 0 }))
    return g
  }
  let meetings = 0
  let prev: number | null = null
  for (const tr of list) {
    if (tr.kind === 'meeting') {
      meetings += tr.minutes ?? 0
      prev = Math.max(prev ?? 0, tr.end ?? tr.t)
      continue
    }
    const g = group(tr)
    g.traces.push(tr)
    g.cost += tr.cost ?? 0
    g.first = Math.min(g.first, tr.t)
    if (tr.kind === 'time') {
      g.logged += tr.minutes ?? 0
      continue
    }
    // An agent is two traces: its launch, and its last activity (it may run without you).
    for (const at of tr.kind === 'agent' && tr.end && tr.end - tr.t > 5 * MIN ? [tr.t, tr.end] : [tr.t]) {
      g.traced += prev === null ? CLAIM_FIRST : Math.max(5, Math.min(CLAIM_MAX, Math.round((at - prev) / MIN)))
      prev = Math.max(prev ?? 0, at)
    }
  }
  const tickets = [...groups.values()].sort((a, b) => (b.ticket ? 1 : 0) - (a.ticket ? 1 : 0) || b.traced - a.traced)
  return { tickets, meetings }
}

/** Minutes left to log on a ticket, rounded to 5; nothing under 10 minutes. */
export const toLog = (g: TicketTime) => {
  const m = Math.round((g.traced - g.logged) / 5) * 5
  return g.ticket && m >= 10 ? m : 0
}

/* ---------- Daily ---------- */

/** The working day before `day`. */
export function previousWorkday(day: string): string {
  const d = new Date(`${day}T12:00`)
  do d.setDate(d.getDate() - 1)
  while (d.getDay() === 0 || d.getDay() === 6)
  return dayKey(d)
}

export const mrLabel = (n: number | undefined) => `!${n ?? '?'}`

/**
 * The daily of `day`: what was done the working day before (from its traces), what is planned
 * (my MRs to get green or merged, reviews waiting for me, my tickets in progress, the day's
 * meetings), and what blocks (my tickets waiting on others). Plain text, to edit and paste.
 */
export function daily(state: TrailState | undefined, s: Snapshot, day: string): { yesterday: string[]; today: string[]; blockers: string[] } {
  const me = s.me ?? null
  const before = traces(state, s, previousWorkday(day))
  const yesterday: string[] = []
  const said = new Set<string>()
  const ticketName = (id?: string) => (id ? s.sprint?.tickets.find((tk) => tk.id === id)?.name : undefined)
  const mrTitle = (tr: Trace) => tr.title.replace(/^(draft:\s*)?([A-Z][A-Z0-9]+-\d+\s+)?/i, '')
  const mrOf = (tr: Trace) => `${mrLabel(tr.mr)} ${mrTitle(tr)}${tr.ticket ? ` (${tr.ticket})` : ''}`
  for (const tr of before.filter((x) => x.kind === 'mr_merged')) {
    said.add(tr.ticket ?? `${tr.repo}${tr.mr}`)
    yesterday.push(t('Merged {mr}', { mr: mrOf(tr) }))
  }
  for (const tr of before.filter((x) => x.kind === 'mr_opened' && !said.has(x.ticket ?? `${x.repo}${x.mr}`))) {
    said.add(tr.ticket ?? `${tr.repo}${tr.mr}`)
    yesterday.push(t('Opened {mr}, in review', { mr: mrOf(tr) }))
  }
  // Work on a ticket with no MR to show for it yet: its commits and agents.
  const worked = new Map<string, { commits: number; agents: number; title: string }>()
  for (const tr of before.filter((x) => (x.kind === 'commit' || x.kind === 'agent') && x.ticket && !said.has(x.ticket))) {
    const w = worked.get(tr.ticket!) ?? { commits: 0, agents: 0, title: ticketName(tr.ticket) ?? '' }
    if (tr.kind === 'commit') w.commits++
    else w.agents++
    worked.set(tr.ticket!, w)
  }
  for (const [id, w] of worked) {
    const how = [w.commits && tn(w.commits, '{n} commit', '{n} commits'), w.agents && tn(w.agents, '{n} agent', '{n} agents')].filter(Boolean).join(', ')
    yesterday.push(t('Worked on {ticket} ({how})', { ticket: `${id}${w.title ? ` ${w.title}` : ''}`, how }))
  }
  // Reviews: one line per MR, its comments counted, approved or not.
  const reviews = new Map<string, { mr?: number; title: string; comments: number; approved: boolean }>()
  for (const tr of before.filter((x) => (x.kind === 'mr_commented' || x.kind === 'mr_approved') && x.others !== false)) {
    const k = `${tr.repo}${tr.mr}`
    const r = reviews.get(k) ?? { mr: tr.mr, title: mrTitle(tr), comments: 0, approved: false }
    if (tr.kind === 'mr_approved') r.approved = true
    else r.comments++
    reviews.set(k, r)
  }
  for (const r of reviews.values()) {
    const what = [r.comments && tn(r.comments, '{n} comment', '{n} comments'), r.approved && t('approved')].filter(Boolean).join(', ')
    yesterday.push(t('Reviewed {mr} ({what})', { mr: `${mrLabel(r.mr)} ${r.title}`, what }))
  }
  for (const tr of before.filter((x) => x.kind === 'ticket_moved' && x.to && [...conventions().done_statuses, ...conventions().after_merge_statuses].includes(x.to) && !said.has(x.ticket ?? ''))) {
    yesterday.push(`${tr.ticket} ${tr.title}: ${tr.to}`)
  }

  const today: string[] = []
  const mine = (m: MergeRequest) => !!me && m.author === me && m.state === 'opened'
  for (const m of s.merge_requests.filter(mine)) {
    const red = m.head_pipeline?.status === 'failed'
    const vars = { iid: m.iid, title: m.title }
    if (red) today.push(t('Get !{iid} green: {title}', vars))
    else if (m.approvals_left > 0 && !m.draft) today.push(t('Get !{iid} reviewed: {title}', vars))
    else if (!m.draft && m.merge_status === 'mergeable') today.push(t('Merge !{iid}: {title}', vars))
  }
  for (const m of s.merge_requests.filter((m) => awaitsMyReview(m, me))) today.push(t('Review !{iid} ({author}): {title}', { iid: m.iid, author: m.author_name, title: m.title }))
  const meId = s.tracker_me?.id
  const myTickets = (s.sprint?.tickets ?? []).filter((tk) => meId !== undefined && tk.assignees.some((a) => a.id === meId) && !tk.closed)
  const covered = new Set(s.merge_requests.filter(mine).flatMap(mrTickets))
  for (const tk of myTickets.filter((x) => conventions().in_progress_statuses.includes(x.status) && !covered.has(x.id))) today.push(t('Continue {id} {name}', { id: tk.id, name: tk.name }))
  if (day === dayKey(new Date())) {
    for (const e of (s.agenda?.events ?? []).filter((e) => !e.all_day && !e.canceled && e.my_status !== 'declined' && !/stand-?up|daily/i.test(e.title))) {
      today.push(t('{title} at {time}', { title: e.title, time: new Date(e.start).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' }) }))
    }
  }

  const blockers: string[] = []
  for (const tk of myTickets) {
    const waiting = (tk.waiting_on ?? []).filter((d) => !d.closed)
    if (waiting.length) blockers.push(t('{id} {name} waits on {others}', { id: tk.id, name: tk.name, others: waiting.map((d) => d.id || d.name).join(', ') }))
  }
  return { yesterday, today, blockers }
}

/** The daily as text to paste: three headings, a dash per line. */
export const dailyText = (d: ReturnType<typeof daily>) =>
  [
    [t('Yesterday'), d.yesterday],
    [t('Today'), d.today],
    [t('Blockers'), d.blockers],
  ]
    .map(([title, lines]) => `${t('{title}:', { title: title as string })}\n${(lines as string[]).length ? (lines as string[]).map((l) => `- ${l}`).join('\n') : `- ${tc('blocks', 'none')}`}`)
    .join('\n\n')
