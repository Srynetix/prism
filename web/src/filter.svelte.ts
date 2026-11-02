import type { MergeRequest, Snapshot, Ticket } from './types'
import type { Alert } from './rules'
import { asTicket, conventions, mrTickets, namesTicket, ticketPrefix } from './conventions'
import { mrKey } from './mr-ref'
import { meOn, mrNumber } from './forge'
import { dayKey } from './calendar'
import { tn } from './i18n.svelte'

/**
 * Global filter: text query, "Mine" toggle and viewed day, kept in the URL
 * (`#q=ABC-4688&mine=1&date=2026-10-02`) to survive a reload.
 */
export const filter = $state(readHash())

/** Local day in `YYYY-MM-DD` format; by hand, not with Intl, slow in WebKit and called per trace. */
export { dayKey }
export const todayKey = () => dayKey(new Date())

/**
 * The picker goes back as far as the time entry history: `[clickup] time_history_days`, and at
 * least the 1st of the previous month (as `history_start` in the core).
 */
export function historyStart(): string {
  const back = new Date()
  back.setDate(back.getDate() - conventions().time_history_days)
  const month = new Date()
  month.setDate(1)
  month.setMonth(month.getMonth() - 1)
  return dayKey(back < month ? back : month)
}

function readHash() {
  const params = new URLSearchParams(location.hash.slice(1))
  const date = params.get('date') ?? ''
  return { query: params.get('q') ?? '', mine: params.get('mine') === '1', date: /^\d{4}-\d{2}-\d{2}$/.test(date) ? date : '' }
}

/** Viewed day: the picker's, otherwise today. */
export const selectedDay = () => filter.date || todayKey()

export function setDate(day: string) {
  const min = historyStart()
  const clamped = day > todayKey() ? todayKey() : day < min ? min : day
  filter.date = clamped === todayKey() ? '' : clamped
  writeHash()
}

export function shiftDate(days: number) {
  const d = new Date(`${selectedDay()}T12:00`)
  d.setDate(d.getDate() + days)
  setDate(dayKey(d))
}

function writeHash() {
  const params = new URLSearchParams()
  if (filter.query.trim()) params.set('q', filter.query.trim())
  if (filter.mine) params.set('mine', '1')
  if (filter.date) params.set('date', filter.date)
  const hash = params.toString()
  history.replaceState(null, '', hash ? `#${hash}` : location.pathname + location.search)
}

export function setQuery(q: string) {
  filter.query = q
  writeHash()
}

export function toggleMine() {
  filter.mine = !filter.mine
  writeHash()
}

addEventListener('hashchange', () => Object.assign(filter, readHash()))

/** My MRs: those I authored. Not those where I am a reviewer: I am one on all of them. */
const isMyMr = (s: Snapshot, m: MergeRequest) => !!meOn(s, m) && m.author === meOn(s, m)
const isMyTicket = (s: Snapshot, t: Ticket) => !!s.tracker_me && t.assignees.some((a) => a.id === s.tracker_me!.id)

/** Tickets of the filterable ClickUp blocks: sprint (next and out of sprint included), refinement and "To pick". */
const clickupTickets = (s: Snapshot): Ticket[] => [
  ...(s.sprint?.tickets ?? []),
  ...(s.next_sprint?.tickets ?? []),
  ...(s.outside_sprint?.tickets ?? []),
  ...(s.refinement?.boards.flatMap((b) => b.tickets) ?? []),
  ...(s.pickup?.boards.flatMap((b) => b.tickets) ?? []),
]

/** "Mine" objects: my MRs (author) and my tickets (assigned), as a focus. */
export function mineFocus(s: Snapshot): Focus {
  const mrs = new Set(s.merge_requests.filter((m) => isMyMr(s, m)).map(mrKey))
  const tickets = new Set(clickupTickets(s).filter((t) => isMyTicket(s, t)).map((t) => t.id))
  const branches = new Set(s.merge_requests.filter((m) => !m.repo && mrs.has(mrKey(m))).map((m) => m.source_branch))
  return { tickets, mrs, branches, sessions: new Set(s.sessions.map((c) => c.id)), summary: 'mine' }
}

/**
 * "Mine" view: Claude sessions (all mine) and protected branches (the whole
 * team's business) stay visible; the rest is restricted to my MRs and my tickets.
 */
export function applyMine(s: Snapshot): Snapshot {
  const f = mineFocus(s)
  const view = apply(s, f)
  // Protected branches' pipelines, the only ones collected: all of them.
  return { ...view, pipelines: s.pipelines }
}

/** Alerts always relevant in "Mine" view, whatever the cited object. */
const ALWAYS_MINE = new Set<Alert['kind']>(['Source', 'Infra', 'CI', 'ArgoCD', 'Claude', 'Review', 'MR', 'Worktrees', 'Data'])

export function alertIsMine(a: Alert, f: Focus): boolean {
  return ALWAYS_MINE.has(a.kind) || !!a.block || alertMatches(a, f)
}

/** Set of objects linked to the query, after ticket ↔ MR ↔ branch propagation. */
export interface Focus {
  tickets: Set<string>
  mrs: Set<string>
  branches: Set<string>
  sessions: Set<string>
  summary: string
}

/** `!12` (GitLab), `#12` (GitHub), or a number alone. */
const MR = /^[!#]?(\d+)$/

export function resolve(s: Snapshot, raw: string): Focus | null {
  const q = raw.trim()
  if (!q) return null

  const tickets = new Set<string>()
  const mrs = new Set<string>()
  const sessions = new Set<string>()
  const allTickets = clickupTickets(s)

  // The id as typed (`c-68` → `C-68`): a board's prefix is not necessarily the tracker's.
  const ticketMatch = asTicket(q)
  const mrMatch = q.match(MR)
  if (ticketMatch) {
    tickets.add(ticketMatch)
  } else if (mrMatch) {
    // A number alone may designate an MR or a ticket: keep what exists.
    const n = Number(mrMatch[1])
    for (const m of s.merge_requests) if (m.iid === n) mrs.add(mrKey(m))
    const id = `${ticketPrefix()}-${n}`
    if (!/^[!#]/.test(q) && ticketPrefix() && (allTickets.some((t) => t.id === id) || s.merge_requests.some((m) => namesTicket(m, id)))) {
      tickets.add(id)
    }
  } else {
    const needle = q.toLowerCase()
    const has = (v: string | null | undefined) => !!v && v.toLowerCase().includes(needle)
    for (const m of s.merge_requests) if (has(m.title) || has(m.source_branch)) mrs.add(mrKey(m))
    for (const t of allTickets) if (has(t.name)) tickets.add(t.id)
    for (const c of s.sessions) if (has(c.title) || has(c.branch)) sessions.add(c.id)
  }

  // Propagation: ticket → MRs → tickets of the MRs → branches.
  for (const m of s.merge_requests) if (mrTickets(m).some((id) => tickets.has(id))) mrs.add(mrKey(m))
  for (const m of s.merge_requests) if (mrs.has(mrKey(m))) for (const id of mrTickets(m)) tickets.add(id)
  // Branches serve pipelines and sessions.
  const branches = new Set(s.merge_requests.filter((m) => mrs.has(mrKey(m))).map((m) => m.source_branch))
  for (const c of s.sessions) {
    if ((c.ticket && tickets.has(c.ticket)) || (c.branch && branches.has(c.branch))) sessions.add(c.id)
  }
  for (const c of s.sessions) if (sessions.has(c.id) && c.branch) branches.add(c.branch)

  const parts = [
    tickets.size && tn(tickets.size, '{n} ticket', '{n} tickets'),
    mrs.size && tn(mrs.size, '{n} MR', '{n} MRs'),
    sessions.size && tn(sessions.size, '{n} session', '{n} sessions'),
  ].filter(Boolean)
  return { tickets, mrs, branches, sessions, summary: parts.length ? parts.join(' · ') : 'no match' }
}

/** Pipeline target or job ref (`!1957`, `#1957` on GitHub, `refs/merge-requests/1957/head`, branch). */
function targetMatches(f: Focus, target: string | null | undefined): boolean {
  if (!target) return false
  const mr = target.match(/^[!#](\d+)$/) ?? target.match(/^refs\/merge-requests\/(\d+)\//)
  return mr ? f.mrs.has(mrNumber(mr[1])) : f.branches.has(target)
}

/** Snapshot view restricted to the focus; pods other than CI jobs stay visible. */
export function apply(s: Snapshot, f: Focus | null): Snapshot {
  if (!f) return s
  return {
    ...s,
    sessions: s.sessions.filter((c) => f.sessions.has(c.id)),
    merge_requests: s.merge_requests.filter((m) => f.mrs.has(mrKey(m))),
    pipelines: s.pipelines.filter((p) => targetMatches(f, p.target)),
    pending_jobs: s.pending_jobs.filter((j) => targetMatches(f, j.target)),
    pods: s.pods.filter((p) => p.role !== 'job' || targetMatches(f, p.job_ref)),
    sprint: s.sprint && { ...s.sprint, tickets: s.sprint.tickets.filter((t) => f.tickets.has(t.id)) },
    next_sprint: s.next_sprint && { ...s.next_sprint, tickets: s.next_sprint.tickets.filter((t) => f.tickets.has(t.id)) },
    refinement: s.refinement && { ...s.refinement, boards: s.refinement.boards.map((b) => ({ ...b, tickets: b.tickets.filter((t) => f.tickets.has(t.id)) })) },
    pickup: s.pickup && { ...s.pickup, boards: s.pickup.boards.map((b) => ({ ...b, tickets: b.tickets.filter((t) => f.tickets.has(t.id)) })) },
    outside_sprint: s.outside_sprint && { ...s.outside_sprint, tickets: s.outside_sprint.tickets.filter((t) => f.tickets.has(t.id)) },
  }
}

const escape = (v: string) => v.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&')

/** An alert is kept if it cites one of the focus objects (`!195` must not catch `!1957`). */
export function alertMatches(a: Alert, f: Focus | null, titles: string[] = []): boolean {
  if (!f) return true
  const ids = [...f.tickets, ...f.mrs, ...f.branches, ...titles]
  const cited = ids.some((id) => new RegExp(`${escape(id)}(?![\\w-])`).test(a.text))
  return cited || (!!a.url && [...f.mrs].some((k) => new RegExp(`/(merge_requests|pull)/${k.replace(/^.*[!#]/, '')}(?!\\d)`).test(a.url!)))
}
