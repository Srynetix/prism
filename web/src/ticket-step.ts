import { primaryRepo, trackerName } from './tracker'
import { t as tr, tc } from './i18n.svelte'
import { live } from './live.svelte'
import { agentsTarget, runClaude, ticketActions } from './claude-actions'
import { actorName } from './agent-cli'
import { primaryAction, type Primary } from './mr-actions'
import { meOn } from './forge'
import { awaitingApproval, isApproved, isDone, nextStep, type StepTone } from './rules'
import { conventions, namesTicket } from './conventions'
import { changeStatus, setMine } from './ticket-view.svelte'
import type { MergeRequest, Person, StatusOption, Ticket } from './types'
import { mrKey } from './mr-ref'

/**
 * Next step of a ticket, in plain words, with the action that unblocks it: the same idea as for
 * MRs. While an MR is open, it is that MR's step (CI, review, rebase…); otherwise the
 * ticket's status and its merged MR say what is left to do: take it, start it, move it
 * forward after the merge.
 */
export interface TicketStep {
  label: string
  tone: StepTone
  primary: Primary | null
}

/** Flow statuses from the config (`[clickup]`): before dev, in progress, in review (the ticket waits for its MR), after merge. */
const beforeDev = (s: string) => conventions().before_dev_statuses.includes(s)
const inProgress = (s: string) => conventions().in_progress_statuses.includes(s)
const inReview = (s: string) => conventions().review_statuses.includes(s)
const afterMerge = (s: string) => conventions().after_merge_statuses.includes(s)

/**
 * MRs of a ticket in the full snapshot: the most recent open one first, then the merged ones.
 * `repo`: a ticket whose id is only unique in its repository (Obsidian) takes that repository's.
 */
export function mrsOf(id: string, repo?: string | null): MergeRequest[] {
  const all = live.snapshot?.merge_requests.filter((m) => namesTicket(m, id) && (!repo || (m.repo || primaryRepo()) === repo)) ?? []
  return all.sort((a, b) => Number(b.state === 'opened') - Number(a.state === 'opened') || b.updated_at.localeCompare(a.updated_at))
}

/** Move the ticket to the first of these statuses that its board knows. */
function move(t: Ticket, statuses: StatusOption[], wanted: string[]): Primary | null {
  const to = wanted.find((w) => statuses.some((s) => s.status === w))
  return to ? { label: `→ ${to}`, title: tr('Move {id} to “{status}” in {tracker}', { id: t.id, status: to, tracker: trackerName() }), run: () => changeStatus(t.task_id, to, statuses) } : null
}

function implement(t: Ticket): Primary | null {
  const a = ticketActions(t).find((x) => x.id === 'implement')
  if (a?.launch && agentsTarget()) return { label: a.label, title: tr('{actor} implements the ticket in a worktree — opens the agent launcher', { actor: actorName() }), run: () => runClaude(a) }
  return a?.href ? { label: a.label, title: tr('{actor} implements the ticket — opens a Claude Code session', { actor: actorName() }), href: a.href } : null
}

const assign = (t: Ticket): Primary => ({ label: tr('Assign me'), title: tr('Assign {id} to me in {tracker}', { id: t.id, tracker: trackerName() }), run: () => setMine(t.task_id, true) })

export function ticketStep(t: Ticket, me: Person | null, statuses: StatusOption[]): TicketStep {
  if (isDone(t)) return { label: tc('tickets', 'Done'), tone: 'idle', primary: null }
  const mine = !!me && t.assignees.some((a) => a.id === me.id)
  const mrs = mrsOf(t.id, t.repo)
  // Of several open MRs, the most blocked one speaks for the ticket: a conflict first.
  const open = mrs.find((m) => m.state === 'opened' && m.conflicts) ?? mrs.find((m) => m.state === 'opened')
  const merged = mrs.find((m) => m.state === 'merged')

  if (open) {
    if (beforeDev(t.status) && !open.draft) {
      return { label: tr('{mr} open, still {status}', { mr: mrKey(open), status: t.status }), tone: 'todo', primary: move(t, statuses, [...conventions().in_progress_statuses, ...conventions().review_statuses]) }
    }
    const opened = mrs.filter((m) => m.state === 'opened')
    if (awaitingApproval(t.status) && opened.every((m) => !m.draft && isApproved(m))) {
      const to = conventions().approved_statuses ?? []
      if (to.length) return { label: tr('{mr} approved, still {status}', { mr: mrKey(open), status: t.status }), tone: 'todo', primary: move(t, statuses, to) }
    }
    const queue = live.snapshot?.merge_queue_label
    const step = nextStep(open, meOn(live.snapshot, open), queue)
    return { label: step.label, tone: step.tone, primary: primaryAction(step.primary, open, queue) }
  }
  if (merged && !afterMerge(t.status)) {
    return { label: tr('{mr} merged, move on', { mr: mrKey(merged) }), tone: 'todo', primary: move(t, statuses, conventions().after_merge_statuses) }
  }
  // An EPIC's work, and its MRs, live in its subtasks.
  if (t.task_type?.toLowerCase() === 'epic' && !beforeDev(t.status)) return { label: tr('See subtasks'), tone: 'idle', primary: null }
  if (afterMerge(t.status)) return { label: tr('To validate'), tone: mine ? 'todo' : 'wait', primary: null }
  if (inReview(t.status)) return { label: tr('No open MR'), tone: 'blocked', primary: null }
  if (beforeDev(t.status)) {
    if (!t.assignees.length) return { label: tr('Pick it'), tone: 'todo', primary: assign(t) }
    if (mine) return { label: tr('Start'), tone: 'todo', primary: implement(t) }
    return { label: tr('Taken by {name}', { name: t.assignees[0].name.split(' ')[0] }), tone: 'idle', primary: null }
  }
  if (inProgress(t.status)) {
    return mine ? { label: tr('No MR yet'), tone: 'todo', primary: implement(t) } : { label: tr('In progress'), tone: 'wait', primary: null }
  }
  return { label: t.status, tone: 'idle', primary: null }
}
