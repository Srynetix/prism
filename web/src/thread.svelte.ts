import { conventions, namesTicket } from './conventions'
import { rememberPanel } from './recent-panels.svelte'
import { t } from './i18n.svelte'
import { used } from './feature-use.svelte'
import { mrKey, sameMr, type MrRef } from './mr-ref'
import { alerts, ticketIndex, type Alert } from './rules'
import type { Agent, CalendarEvent, MergeRequest, Pipeline, Snapshot, StatusOption, Ticket, Worktree } from './types'

/**
 * A piece of work's thread: its ticket, worktree, agents, MR, pipeline and the environments
 * updated since its merge, found from any of them. Prism joins what it already knows: the ticket
 * id in a branch, an MR's branch, an agent's ticket, branch or linked MR, a worktree's branch.
 * Opened from any row (`t`, or its ⋯ menu): `ThreadPanel.svelte`.
 */
export interface ThreadRef {
  ticket?: string
  repo?: string | null
  mr?: MrRef
  agent?: string
  branch?: string
}

/** An environment, as far as the thread goes: updated since the MR's merge, or not yet. */
export interface EnvStep {
  env: string
  /** Its workloads' last update. */
  at: string | null
  /** Updated after the merge: a deploy happened since (a time, not a proof of the commit). */
  since: boolean
}

export interface Thread {
  ticket: Ticket | null
  statuses: StatusOption[]
  /** Its MRs, the one the thread is about first. */
  mrs: MergeRequest[]
  mr: MergeRequest | null
  branch: string | null
  worktree: Worktree | null
  agents: Agent[]
  pipeline: Pipeline | null
  envs: EnvStep[]
  /** The alerts about it (drift between the ticket and its MR, a red pipeline…). */
  drift: Alert[]
  /** Time logged on its ticket, in ms (the time entries Prism read). */
  logged: number
  /** What its agents cost, at the API prices. */
  cost: number
  meetings: CalendarEvent[]
  /** Something should be there by now and is not, by link: `worktree`, `mr`, `env:<name>`. */
  missing: Set<string>
}

/** Environments shown in a thread, at most (in the Environments block's order). */
const MAX_ENVS = 3
/** In staging and not in production after this long: the next environment is flagged. */
const STUCK_DAYS = 3

const lower = (xs: string[] | undefined) => (xs ?? []).map((x) => x.toLowerCase())
const inProgress = (t: Ticket) => lower(conventions().in_progress_statuses).includes(t.status.toLowerCase())

/** `drift`: also the alerts about it (Alerts' whole pass: for the drawer, not for every row). */
export function resolveThread(ref: ThreadRef, s: Snapshot, now = Date.now(), drift = true): Thread | null {
  const agent = ref.agent ? s.agents.find((a) => a.id === ref.agent) : undefined
  const mrOf = (r: MrRef) => s.merge_requests.find((m) => sameMr(m, r))
  let mr = ref.mr ? mrOf(ref.mr) : agent?.mr ? mrOf(agent.mr) : undefined
  let id = ref.ticket ?? mr?.ticket ?? agent?.ticket ?? null
  let branch = ref.branch ?? mr?.source_branch ?? agent?.branch ?? null
  const repo = ref.repo ?? mr?.repo ?? null
  const index = ticketIndex(s)
  const entry = id ? [...index.values()].find((e) => e.t.id === id && (!repo || !e.t.repo || e.t.repo === repo)) : undefined
  const ticket = entry?.t ?? null

  const found = s.merge_requests
    .filter((m) => (id && namesTicket(m, id) && (!ticket?.repo || (m.repo ?? null) === (ticket.repo ?? null) || !m.repo)) || (branch && m.source_branch === branch))
    .sort((a, b) => Number(b.state === 'opened') - Number(a.state === 'opened') || b.updated_at.localeCompare(a.updated_at))
  mr ??= found[0]
  const mrs = mr ? [mr, ...found.filter((m) => m !== mr)] : found
  id ??= mr?.ticket ?? null
  branch ??= mr?.source_branch ?? null

  const worktree = s.worktrees.find((w) => !w.is_main && !w.prunable && ((!!branch && w.branch === branch) || (!!id && w.ticket === id))) ?? null
  branch ??= worktree?.branch ?? null
  const agents = s.agents.filter((a) => a.state !== 'gone' && ((!!id && a.ticket === id) || (!!branch && a.branch === branch) || (!!mr && !!a.mr && sameMr(a.mr, mr))))
  if (!ticket && !mr && !worktree && !agents.length) return null

  const pipeline = mr?.state === 'opened' ? mr.head_pipeline : null
  const envs = envSteps(s, mr?.merged_at ?? null)
  const missing = new Set<string>()
  if (ticket && inProgress(ticket) && !worktree && !mrs.length && !agents.length) missing.add('worktree')
  if (worktree && !mrs.length && (worktree.ahead > 0 || worktree.unpushed > 0)) missing.add('mr')
  // The first environment updated since the merge, the next one not, for days: it waits there.
  const first = envs.findIndex((e) => e.since)
  const next = envs[first + 1]
  if (first >= 0 && next && !next.since && mr?.merged_at && now - Date.parse(mr.merged_at) > STUCK_DAYS * 86_400_000) missing.add(`env:${next.env}`)

  const keys = [...(id ? [id] : []), ...mrs.map((m) => mrKey(m))]
  const about = drift && keys.length ? alerts(s).filter((a) => keys.some((k) => new RegExp(`(^|[^\\w])${k.replace(/[.*+?^${}()|[\]\\!]/g, '\\$&')}(?![\\w])`).test(a.text))) : []
  const logged = id ? (s.time?.entries ?? []).filter((e) => e.ticket === id).reduce((n, e) => n + e.duration_ms, 0) : 0
  const cost = agents.reduce((n, a) => n + (a.usage?.cost ?? 0), 0)
  const meetings = id ? (s.agenda?.events ?? []).filter((e) => !e.canceled && Date.parse(e.end) > now && e.title.includes(id!)) : []
  return { ticket, statuses: entry?.statuses ?? [], mrs, mr: mr ?? null, branch, worktree, agents, pipeline, envs, drift: about, logged, cost, meetings, missing }
}

/** Each environment's last update, against the merge: one step per environment name. */
export function envSteps(s: Snapshot, mergedAt: string | null): EnvStep[] {
  const out: EnvStep[] = []
  for (const e of s.environments?.envs ?? []) {
    const at = e.workloads.map((w) => w.updated_at).filter((x): x is string => !!x).sort().at(-1) ?? null
    const prev = out.find((x) => x.env === e.env)
    if (prev) {
      if (at && (!prev.at || at > prev.at)) prev.at = at
    } else out.push({ env: e.env, at, since: false })
  }
  for (const x of out) x.since = !!mergedAt && !!x.at && x.at > mergedAt
  // In the order a change goes through them: the first environments before production.
  return out.sort((a, b) => envRank(a.env) - envRank(b.env)).slice(0, MAX_ENVS)
}

/** An environment's place on the way to production, by its usual names; unknown ones in between. */
export function envRank(env: string): number {
  const e = env.toLowerCase()
  if (/^(dev|develop|test|ci|review|qa|int)/.test(e)) return 0
  if (/^(stag|recette|rec|uat|demo)/.test(e)) return 1
  if (/^(pre|preprod|pp)/.test(e)) return 3
  if (/^(prod|production|live|prd)/.test(e)) return 4
  return 2
}

/** The thread drawer: what it was opened on. */
export const threadView = $state({ ref: null as ThreadRef | null })
export function openThread(ref: ThreadRef) {
  used('thread')
  threadView.ref = ref
}
export function closeThread() {
  const ref = threadView.ref
  if (ref) {
    const what = ref.ticket ?? (ref.mr ? mrKey(ref.mr) : (ref.branch ?? ref.agent ?? ''))
    rememberPanel({ key: `thread:${JSON.stringify(ref)}`, kind: 'thread', title: t('Thread · {what}', { what }), hint: t('Thread'), reopen: () => openThread(ref) })
  }
  threadView.ref = null
}

/** A row's anchor, as `data-thread` carries it: `ticket:<repo>:<id>`, `mr:<repo>:<iid>`, `agent:<id>`, `branch:<repo>:<name>`. */
export const threadAttr = (ref: ThreadRef) =>
  ref.agent ? `agent:${ref.agent}` : ref.mr ? `mr:${ref.mr.repo ?? ''}:${ref.mr.iid}` : ref.ticket ? `ticket:${ref.repo ?? ''}:${ref.ticket}` : ref.branch ? `branch:${ref.repo ?? ''}:${ref.branch}` : ''

export function parseThreadAttr(attr: string): ThreadRef | null {
  const [kind, ...rest] = attr.split(':')
  const repo = rest[0] || null
  const value = rest.slice(1).join(':')
  switch (kind) {
    case 'agent':
      return { agent: rest.join(':') }
    case 'mr':
      return Number(value) ? { mr: { repo, iid: Number(value) } } : null
    case 'ticket':
      return value ? { ticket: value, repo } : null
    case 'branch':
      return value ? { branch: value, repo } : null
  }
  return null
}
