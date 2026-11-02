import { live } from './live.svelte'
import type { Dependency, Ticket } from './types'
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

/** A dependency's other ticket is finished: closed, or in an end status (`done_statuses`). */
export const dependencyDone = (d: Dependency) => d.closed || conventions().done_statuses.includes(d.status)

/** The tickets `t` still waits on. */
export const openBlockers = (t: Ticket) => (t.waiting_on ?? []).filter((d) => !dependencyDone(d))

/** Waiting on at least one unfinished ticket. */
export const isBlocked = (t: Ticket) => openBlockers(t).length > 0
