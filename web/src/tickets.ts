import { live } from './live.svelte'
import type { Dependency, StatusOption, Ticket } from './types'
import { conventions } from './conventions'

/**
 * Ticket known by its internal identifier, in all ClickUp blocks of the full snapshot
 * (not the filtered view: the parent of a displayed ticket may be hidden by the filter).
 */
export function byTaskId(taskId: string | null | undefined): Ticket | undefined {
  if (!taskId) return undefined
  return known().find((t) => t.task_id === taskId)
}

/** Descendants of a ticket (children, grandchildren…), each only once. */
export function descendantsOf(taskId: string): Ticket[] {
  const all = known()
  const seen = new Set([taskId])
  const out: Ticket[] = []
  for (let level = [taskId]; level.length; ) {
    const next = all.filter((t) => t.parent && level.includes(t.parent) && !seen.has(t.task_id))
    next.forEach((t) => seen.add(t.task_id))
    out.push(...next)
    level = next.map((t) => t.task_id)
  }
  return out
}

/** The tickets of the full snapshot; a ticket in the sprint and in its board is there only once. */
function known(): Ticket[] {
  const s = live.snapshot
  if (!s) return []
  const all = [...(s.sprint?.tickets ?? []), ...(s.refinement?.boards.flatMap((b) => b.tickets) ?? []), ...(s.pickup?.boards.flatMap((b) => b.tickets) ?? []), ...(s.outside_sprint?.tickets ?? [])]
  const seen = new Set<string>()
  return all.filter((t) => !seen.has(t.task_id) && !!seen.add(t.task_id))
}

/** A dependency's other ticket is finished: closed, or in an end status (`done_statuses`, whatever its case). */
const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase()

export const dependencyDone = (d: Dependency) => d.closed || conventions().done_statuses.some((s) => same(s, d.status))

/** The tickets `t` still waits on. */
export const openBlockers = (t: Ticket) => (t.waiting_on ?? []).filter((d) => !dependencyDone(d))

/**
 * A ticket waiting on its blockers (`waiting_statuses`) whose blockers are all done: the status it
 * can move to (`unblocked_status`), when its list has it; null otherwise. The Sync alert's rule.
 */
export function unblockedTo(t: Ticket, statuses: StatusOption[]): string | null {
  const c = conventions()
  const to = c.unblocked_status ?? ''
  const deps = t.waiting_on ?? []
  if (t.closed || !to || same(t.status, to) || !deps.length || !(c.waiting_statuses ?? []).some((s) => same(s, t.status)) || !deps.every(dependencyDone)) return null
  // Its list's own spelling.
  return statuses.find((s) => same(s.status, to))?.status ?? null
}

/** Waiting on at least one unfinished ticket. */
export const isBlocked = (t: Ticket) => openBlockers(t).length > 0
