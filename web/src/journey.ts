import { t as tr, tn } from './i18n.svelte'
import { conventions, isDone, mrTickets, namesTicket } from './conventions'
import { awaitsMyReview, changesRequested, failedStage, nextStep, rebaseFailed, ticketIndex, waitsForYou } from './rules'
import { meOn } from './forge'
import { mrKey } from './mr-ref'
import { hasMergeQueue, repoName } from './repositories'
import { ticketRef } from './tracker'
import { cliOf } from './agent-cli'
import { span } from './time'
import { envSteps } from './thread.svelte'
import type { Agent, MergeRequest, Snapshot, StatusOption, Ticket } from './types'
import { baseName } from './platform'

/*
 * A ticket's journey, from refinement to done, as Prism already reads it: the tracker's status,
 * the agents linked to the ticket, the MRs that name it (their CI, review and merge). No store of
 * its own: everything is derived from the snapshot. The ticket panel's Journey section draws it
 * as a dated journal; the In progress block, one row per active ticket of mine with its strip,
 * the reason in one line and the action that moves it (journey-actions.ts).
 */

/** The seven steps of the strip, in order. */
export const STEPS = ['refined', 'dev', 'mr', 'review', 'merged', 'validate', 'done'] as const
export type StepId = (typeof STEPS)[number]
export const stepLabel = (id: StepId): string =>
  ({ refined: tr('Refined'), dev: tr('Dev'), mr: tr('MR'), review: tr('Review'), merged: tr('Merged'), validate: tr('Validation'), done: tr('Done') })[id]

/** A step of the strip: passed, the current one (failed: `ko`), still ahead. */
export type StepState = 'done' | 'now' | 'ko' | 'todo'
/** My part in it: my MR, a review asked of me, or the ticket assigned to me. */
export type Role = 'author' | 'reviewer' | 'assignee'
/** Whose move it is: mine, or I wait for someone (a reviewer, the CI, an agent at work). */
export type Move = 'me' | 'others'
/**
 * The block's order: someone waits on me (a review asked, feedback on my MR, an agent's
 * question), then what failed, then the normal flow, then what has not started.
 */
export type Urgency = 0 | 1 | 2 | 3

export interface Journey {
  /** The ticket's `ticketRef`, or `mr:` and the MR for an MR without a ticket Prism reads. */
  key: string
  ticket: Ticket | null
  /** The ticket's list statuses: what a move to the next status offers. */
  statuses: StatusOption[]
  /** The ticket's id, or the MR's key. */
  id: string
  title: string
  /** The MR that speaks for it: the one asking for my review, else the most blocked open one, else the last merged. */
  mr: MergeRequest | null
  agents: Agent[]
  step: StepId
  /** The current step failed (red CI, conflicts, failed rebase). */
  failed: boolean
  /** Why it stands there, in one line. */
  why: string
  role: Role
  move: Move
  urgency: Urgency
  /** Since when it waits, for "waiting on others". */
  since: string | null
}

const beforeDev = (s: string) => conventions().before_dev_statuses.includes(s)
const inProgress = (s: string) => conventions().in_progress_statuses.includes(s)
const inReview = (s: string) => conventions().review_statuses.includes(s)
const toValidate = (s: string) => conventions().after_merge_statuses.includes(s) || (conventions().validation_statuses ?? []).includes(s)
/** Before the first status of development in the flow: still being refined. */
function refining(status: string): boolean {
  const flow = conventions().workflow_statuses
  const start = flow.findIndex((s) => beforeDev(s) || inProgress(s))
  const at = flow.indexOf(status)
  return start > 0 && at >= 0 && at < start
}
const firstName = (name: string) => name.split(' ')[0]

/** The MRs of a ticket in the snapshot, in its repository: the most recent open one first, then the merged ones. */
export function ticketMrs(s: Snapshot, t: Pick<Ticket, 'id' | 'repo'>): MergeRequest[] {
  const primary = repoName(null, s)
  return s.merge_requests
    .filter((m) => namesTicket(m, t.id) && (!t.repo || (m.repo || primary) === t.repo))
    .sort((a, b) => Number(b.state === 'opened') - Number(a.state === 'opened') || b.updated_at.localeCompare(a.updated_at))
}

/** Of several open MRs, the most blocked one speaks for the ticket: a conflict, a red CI, then the latest. */
function speaking(mrs: MergeRequest[]): MergeRequest | undefined {
  const open = mrs.filter((m) => m.state === 'opened')
  return open.find((m) => m.conflicts || rebaseFailed(m)) ?? open.find((m) => m.pipeline === 'failed') ?? open[0]
}

/** The agents launched on the ticket (linked by hand, or named in their title or branch), oldest first. */
export const ticketAgents = (s: Snapshot, id: string): Agent[] => s.agents.filter((a) => a.ticket === id).sort((a, b) => a.created_at.localeCompare(b.created_at))

/** The step an open MR holds the ticket at, and whether it failed there. */
function mrStep(m: MergeRequest): { step: StepId; failed: boolean } {
  if (m.draft) return { step: 'mr', failed: false }
  if (m.conflicts || rebaseFailed(m) || m.pipeline === 'failed') return { step: 'mr', failed: true }
  if (m.approvals_left > 0 || changesRequested(m) || m.open_threads > 0) return { step: 'review', failed: false }
  return { step: 'merged', failed: false }
}

/** Why a red MR is red, in a few words. */
function failure(m: MergeRequest): string {
  if (m.conflicts) return tr('Conflicts with {target}', { target: m.target_branch })
  if (rebaseFailed(m)) return tr('Rebase failed')
  const stage = m.head_pipeline ? failedStage(m.head_pipeline) : undefined
  return stage ? tr('CI failed: {stage}', { stage }) : tr('CI failed')
}

/**
 * A ticket's journey (or an MR's, without a ticket Prism reads): its step, the reason, my role and
 * whose move it is. `review`: the MR asking for my review, which then speaks for the ticket;
 * `alone`: without a ticket, the MR the row is about.
 */
export function journeyOf(s: Snapshot, t: Ticket | null, statuses: StatusOption[], review?: MergeRequest, alone?: MergeRequest): Journey {
  const me = s.tracker_me
  const assigned = !!t && !!me && t.assignees.some((a) => a.id === me.id)
  const mrs = t ? ticketMrs(s, t) : [review ?? alone].filter((m) => !!m)
  // An MR of the ticket asking for my review: it speaks for it, as the row of a review would.
  review ??= mrs.find((m) => awaitsMyReview(m, meOn(s, m)))
  const open = review ?? speaking(mrs)
  const merged = mrs.find((m) => m.state === 'merged')
  const agents = t ? ticketAgents(s, t.id) : []
  const running = agents.filter((a) => a.state === 'running')
  const mine = !!open && open.author === meOn(s, open)
  const authored = mine || (!open && !!merged && merged.author === meOn(s, merged))
  const role: Role = review ? 'reviewer' : authored ? 'author' : 'assignee'
  const subject = open ?? mrs[0]
  const id = t?.id ?? (subject ? mrKey(subject) : '')
  const base = { key: t ? ticketRef(t.id, t.repo) : `mr:${id}`, ticket: t, statuses, id, title: t?.name ?? subject?.title ?? '', agents, role }
  const j = (step: StepId, why: string, move: Move, urgency: Urgency, since: string | null, mr: MergeRequest | null = open ?? merged ?? null, failed = false): Journey =>
    ({ ...base, mr, step, failed, why, move, urgency, since })

  if (t && isDone(t)) return j('done', t.status, 'others', 3, t.updated_at)
  if (review && awaitsMyReview(review, meOn(s, review))) {
    const at = mrStep(review)
    return j(at.step, tr('{name} waits for your review', { name: firstName(review.author_name) }), 'me', 0, review.updated_at, review, at.failed)
  }
  // An agent of the ticket asks me something: it waits, whatever its MR's state.
  const asking = running.find((a) => waitsForYou(a.status))
  if (open) {
    const at = mrStep(open)
    if (mine && changesRequested(open)) return j(at.step, tr('{names} asked for changes', { names: open.changes_requested_by.join(', ') }), 'me', 0, open.updated_at)
    if (mine && open.open_threads > 0) return j(at.step, tn(open.open_threads, '{n} unresolved thread', '{n} unresolved threads'), 'me', 0, open.updated_at)
    if (asking) return j(at.step, tr('Agent “{title}” waits for you', { title: asking.title }), 'me', 0, asking.last_activity, open, at.failed)
    if (at.failed) return j(at.step, failure(open), mine ? 'me' : 'others', 1, open.updated_at, open, true)
    const next = nextStep(open, meOn(s, open), s.merge_queue_label)
    const myMove = mine && (next.tone === 'todo' || next.tone === 'blocked' || (next.tone === 'ready' && !!next.primary))
    return j(at.step, open.draft ? tr('Draft') : next.label, myMove ? 'me' : 'others', 2, open.updated_at)
  }
  if (asking) return j('dev', tr('Agent “{title}” waits for you', { title: asking.title }), 'me', 0, asking.last_activity)
  if (merged) {
    const validating = t && toValidate(t.status)
    return j('validate', validating ? tr('Merged, to validate') : tr('{mr} merged, move the ticket on', { mr: mrKey(merged) }), assigned || !t ? 'me' : 'others', 2, merged.merged_at ?? merged.updated_at, merged)
  }
  if (!t) return j('dev', '', 'others', 3, null)
  if (toValidate(t.status)) return j('validate', tr('To validate'), assigned ? 'me' : 'others', 2, t.updated_at)
  if (running.length) return j('dev', tr('Agent “{title}” at work', { title: running[0].title }), 'others', 2, running[0].last_activity ?? running[0].created_at)
  if (refining(t.status)) return j('refined', tr('Being refined ({status})', { status: t.status }), 'others', 3, t.updated_at)
  if (inReview(t.status)) return j('review', tr('In review, but no open MR'), assigned ? 'me' : 'others', 1, t.updated_at)
  if (inProgress(t.status) || agents.length) return j('dev', agents.length ? tr('No MR yet') : tr('In progress, no MR yet'), assigned ? 'me' : 'others', 2, t.updated_at)
  if (beforeDev(t.status)) return j('dev', assigned ? tr('Refined, not started') : t.assignees.length ? tr('Taken by {name}', { name: firstName(t.assignees[0].name) }) : tr('Nobody on it'), assigned || !t.assignees.length ? 'me' : 'others', 3, t.updated_at)
  return j('dev', t.status, assigned ? 'me' : 'others', 2, t.updated_at)
}

/** The strip: the steps before the current one passed, it now (or failed), the rest ahead; all passed once done. */
export function stepStates(j: Pick<Journey, 'step' | 'failed' | 'ticket'>): StepState[] {
  const at = STEPS.indexOf(j.step)
  if (j.step === 'done') return STEPS.map(() => 'done')
  return STEPS.map((_, i) => (i < at ? 'done' : i === at ? (j.failed ? 'ko' : 'now') : 'todo'))
}

/** Under the strip: the step and its MR, an MR of someone else with its author. */
export function where(j: Journey): string {
  const mr = j.mr ? ` · ${mrKey(j.mr)}` : ''
  const from = j.role === 'reviewer' && j.mr ? ` ${tr('(from {name})', { name: firstName(j.mr.author_name) })}` : ''
  return `${stepLabel(j.step)}${mr}${from}`
}

/**
 * The In progress block: my active tickets — of the sprint (or outside it) assigned to me and not
 * done, those with an open MR of mine or a running agent of mine — and the MRs asking for my
 * review. One row per ticket; an MR without a ticket Prism reads is a row of its own. Ordered:
 * someone waits on me, then failures, then the normal flow, then what has not started; within
 * each, what waits the longest first.
 */
export function journeys(s: Snapshot): Journey[] {
  const me = s.tracker_me
  const index = ticketIndex(s)
  const rows = new Map<string, Journey>()
  const ticketRow = (t: Ticket, statuses: StatusOption[], review?: MergeRequest) => {
    const key = ticketRef(t.id, t.repo)
    if (!review && rows.has(key)) return
    rows.set(key, journeyOf(s, t, statuses, review))
  }
  const mrRow = (m: MergeRequest, review?: MergeRequest) => {
    const ids = mrTickets(m)
    const read = ids.map((id) => index.get(ticketRef(id, repoName(m.repo, s)))).filter((e) => !!e)
    for (const e of read) if (!isDone(e.t) || m.state === 'opened') ticketRow(e.t, e.statuses, review)
    if (!read.length) rows.set(`mr:${mrKey(m)}`, journeyOf(s, null, [], review, m))
  }
  for (const { t, statuses } of index.values()) if (me && !isDone(t) && t.assignees.some((a) => a.id === me.id) && !t.task_type?.match(/^epic$/i)) ticketRow(t, statuses)
  for (const m of s.merge_requests) if (m.state === 'opened' && m.author === meOn(s, m)) mrRow(m)
  for (const a of s.agents) {
    const e = a.state === 'running' && a.ticket ? index.get(ticketRef(a.ticket, null)) ?? [...index.values()].find((x) => x.t.id === a.ticket) : undefined
    if (e && !isDone(e.t)) ticketRow(e.t, e.statuses)
  }
  // A review asked of me speaks for its ticket, even one of mine.
  for (const m of s.merge_requests) if (awaitsMyReview(m, meOn(s, m))) mrRow(m, m)
  const time = (j: Journey) => (j.since ? Date.parse(j.since) : Infinity)
  return [...rows.values()].filter((j) => j.step !== 'done').sort((a, b) => a.urgency - b.urgency || time(a) - time(b) || a.id.localeCompare(b.id))
}

/** The block's segmented filter: what is my move, what waits on others, everything. */
export type JourneyFilter = 'me' | 'others' | 'all'
export const filterJourneys = (rows: Journey[], f: JourneyFilter) => (f === 'all' ? rows : rows.filter((j) => j.move === f))

/* ---------- The journal (ticket panel) ---------- */

export type Stage = 'refinement' | 'development' | 'review' | 'delivery'
export interface JournalEvent {
  key: string
  text: string
  detail?: string
  /** When it happened; none: an event Prism has no date for, or a step still ahead. */
  at: string | null
  /** `ok` done, `ko` failed, `now` where it stands, `todo` ahead (dashed). */
  tone: 'ok' | 'ko' | 'now' | 'todo'
  mr?: MergeRequest
}
export interface JournalStage {
  stage: Stage
  label: string
  events: JournalEvent[]
}
export const stageLabel = (s: Stage): string => ({ refinement: tr('Refinement'), development: tr('Development'), review: tr('Review'), delivery: tr('Delivery') })[s]

/** Under an MR's opening: its pipeline, in a few words. */
function ciWords(m: MergeRequest): string {
  if (m.state !== 'opened') return ''
  if (m.pipeline === 'failed') return failure(m)
  if (m.conflicts || rebaseFailed(m)) return failure(m)
  if (m.pipeline === 'success') return tr('CI green')
  if (m.pipeline && ['created', 'waiting_for_resource', 'preparing', 'pending', 'running'].includes(m.pipeline)) return tr('CI running')
  return ''
}

/**
 * The ticket's journal, by stage: each event Prism reads for it, dated where it can, then the steps
 * still ahead (dashed). A stage with nothing to show is left out.
 */
export function journal(s: Snapshot, t: Ticket, j: Journey, sprint?: string | null): JournalStage[] {
  const at = STEPS.indexOf(j.step)
  const done = j.step === 'done'
  const ahead = (step: StepId) => !done && STEPS.indexOf(step) > at
  const mrs = ticketMrs(s, t).sort((a, b) => (a.created_at ?? a.updated_at).localeCompare(b.created_at ?? b.updated_at))
  const stages: Record<Stage, JournalEvent[]> = { refinement: [], development: [], review: [], delivery: [] }

  const estimate = t.time_estimate ? tr('estimated {time}', { time: span(t.time_estimate) }) : ''
  const inSprint = sprint ? tr('in {sprint}', { sprint }) : ''
  stages.refinement.push({ key: 'created', text: tr('Created'), detail: [estimate, inSprint].filter(Boolean).join(' · ') || undefined, at: t.created_at, tone: 'ok' })
  if (j.step === 'refined') stages.refinement.push({ key: 'refining', text: tr('Being refined ({status})', { status: t.status }), at: t.updated_at, tone: 'now' })

  for (const a of j.agents) {
    const end = a.last_activity ?? a.created_at
    const took = Date.parse(end) - Date.parse(a.created_at)
    const where = a.worktree ? (a.branch ?? baseName(a.worktree)) : a.branch
    const detail = [cliOf(a.provider).name, took > 60_000 ? span(took) : '', where ? tr('worktree {name}', { name: where }) : ''].filter(Boolean).join(' · ')
    const working = a.state === 'running'
    stages.development.push({ key: `agent:${a.id}`, text: tr('Agent “{title}”', { title: a.title }), detail, at: a.created_at, tone: working ? 'now' : 'ok' })
  }
  // A worktree on the ticket that no agent of it works in: work by hand, or by a Desktop session.
  const agentTrees = new Set(j.agents.map((a) => a.worktree).filter(Boolean))
  for (const w of s.worktrees.filter((w) => !w.is_main && w.ticket === t.id && !agentTrees.has(w.path))) {
    stages.development.push({ key: `wt:${w.path}`, text: tr('Worktree {name}', { name: w.branch ?? baseName(w.path) }), detail: w.changes ? tn(w.changes, '{n} change not committed', '{n} changes not committed') : undefined, at: w.last_commit, tone: 'ok' })
  }
  if (j.step === 'dev' && !j.agents.some((a) => a.state === 'running')) stages.development.push({ key: 'dev', text: j.why || tr('In progress'), at: null, tone: 'now' })
  for (const m of mrs) {
    const ci = ciWords(m)
    const red = m.state === 'opened' && (m.pipeline === 'failed' || m.conflicts || rebaseFailed(m))
    stages.development.push({ key: `mr:${mrKey(m)}`, text: m.draft ? tr('{mr} opened as a draft', { mr: mrKey(m) }) : tr('{mr} opened', { mr: mrKey(m) }), detail: ci || undefined, at: m.created_at, tone: red ? 'ko' : m.draft && m.state === 'opened' ? 'now' : 'ok', mr: m })
  }
  if (!mrs.length && ahead('mr')) stages.development.push({ key: 'todo:mr', text: tr('MR'), at: null, tone: 'todo' })

  for (const m of mrs.filter((m) => !m.draft || m.state === 'merged')) {
    const k = mrKey(m)
    const multi = mrs.length > 1 ? `${k}: ` : ''
    if (m.reviewers.length) stages.review.push({ key: `rev:${k}`, text: `${multi}${tn(m.reviewers.length, 'Reviewer: {names}', 'Reviewers: {names}', { names: m.reviewers.join(', ') })}`, at: null, tone: 'ok', mr: m })
    if (m.changes_requested_by.length) stages.review.push({ key: `chg:${k}`, text: `${multi}${tr('{names} asked for changes', { names: m.changes_requested_by.join(', ') })}`, at: null, tone: m.state === 'opened' ? 'now' : 'ok', mr: m })
    if (m.state === 'opened' && m.open_threads > 0) stages.review.push({ key: `thr:${k}`, text: `${multi}${tn(m.open_threads, '{n} unresolved thread', '{n} unresolved threads')}`, at: null, tone: 'now', mr: m })
    if (m.approved_by.length) stages.review.push({ key: `ok:${k}`, text: `${multi}${tr('Approved by {names}', { names: m.approved_by.join(', ') })}`, at: null, tone: 'ok', mr: m })
    else if (m.state === 'opened' && m.approvals_left > 0) stages.review.push({ key: `wait:${k}`, text: `${multi}${tn(m.approvals_left, '{n} approval left', '{n} approvals left')}`, at: null, tone: 'now', mr: m })
  }
  if (ahead('review') || (j.step === 'mr' && !stages.review.length)) stages.review.push({ key: 'todo:review', text: tr('Review'), at: null, tone: 'todo' })

  for (const m of mrs) {
    const k = mrKey(m)
    if (m.state === 'merged') stages.delivery.push({ key: `merged:${k}`, text: tr('{mr} merged into {target}', { mr: k, target: m.target_branch }), at: m.merged_at, tone: 'ok', mr: m })
    else if (m.state === 'opened' && s.merge_queue_label && m.labels.includes(s.merge_queue_label) && hasMergeQueue(m, s)) stages.delivery.push({ key: `queue:${k}`, text: tr('{mr} in the merge queue', { mr: k }), at: null, tone: 'now', mr: m })
    else if (m.state === 'opened' && m.auto_merge) stages.delivery.push({ key: `auto:${k}`, text: tr('{mr} merges once its CI passes', { mr: k }), at: null, tone: 'now', mr: m })
  }
  // The environments updated since the last merge (the thread's reading: a time, not a proof of the commit).
  const lastMerge = mrs.map((m) => m.merged_at).filter((x): x is string => !!x).sort().at(-1) ?? null
  for (const e of lastMerge ? envSteps(s, lastMerge) : []) {
    if (e.since) stages.delivery.push({ key: `env:${e.env}`, text: tr('Rolled out on {env} since the merge', { env: e.env }), detail: tr('a rollout after the merge, not a proof it carries the change'), at: e.at, tone: 'ok' })
    else if (!done) stages.delivery.push({ key: `env:${e.env}`, text: tr('Rollout on {env}', { env: e.env }), at: null, tone: 'todo' })
  }
  const queue = mrs.some((m) => hasMergeQueue(m, s)) && !!s.merge_queue_label
  if (ahead('merged') || (j.step === 'merged' && !stages.delivery.length)) stages.delivery.push({ key: 'todo:merge', text: queue ? tr('Merge (merge queue)') : tr('Merge'), at: null, tone: j.step === 'merged' ? 'now' : 'todo' })
  if (j.step === 'validate') stages.delivery.push({ key: 'validate', text: toValidate(t.status) ? tr('To validate ({status})', { status: t.status }) : tr('Validation'), detail: j.why, at: null, tone: 'now' })
  else if (ahead('validate')) stages.delivery.push({ key: 'todo:validate', text: tr('Validation'), at: null, tone: 'todo' })
  stages.delivery.push(done ? { key: 'done', text: tr('Done ({status})', { status: t.status }), at: t.updated_at, tone: 'ok' } : { key: 'todo:done', text: tr('Done'), at: null, tone: 'todo' })

  // Within a stage: the dated events in order, then the undated ones as they came, the steps ahead last.
  const order = (e: JournalEvent) => (e.tone === 'todo' ? 2 : e.at ? 0 : 1)
  return (Object.keys(stages) as Stage[])
    .map((stage) => ({ stage, label: stageLabel(stage), events: stages[stage].map((e, i) => ({ e, i })).sort((a, b) => order(a.e) - order(b.e) || (a.e.at && b.e.at ? a.e.at.localeCompare(b.e.at) : a.i - b.i)).map(({ e }) => e) }))
    .filter((g) => g.events.length)
}
