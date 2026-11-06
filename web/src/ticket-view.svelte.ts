import { hasCore, invoke } from './bridge'
import type { Mention } from './bindings/Mention'
import { trackerOn } from './live.svelte'
import { rememberPanel } from './recent-panels.svelte'
import { t } from './i18n.svelte'
import type { StatusOption, TaskView } from './types'

/**
 * Preview of a ClickUp ticket, opened over the dashboard. The content is read on opening
 * (never collected in the background) and kept for a few minutes, while navigating between a ticket,
 * its subtasks and the cited tickets. `trail`: the tickets opened before, to go back.
 */
export const ticketView = $state({
  id: null as string | null,
  trail: [] as string[],
  task: null as TaskView | null,
  error: null as string | null,
  loading: false,
  /** Status change in progress, and the last one's error. */
  saving: false,
  saveError: null as string | null,
})

const KEEP_MS = 5 * 60_000
const cache = new Map<string, { at: number; task: TaskView }>()

/** The browser preview has no access to ClickUp; nor the app when ClickUp is off. */
export const canPreview = () => hasCore() && trackerOn()

export function openTicket(id: string) {
  if (!trackerOn()) return
  if (ticketView.id && ticketView.id !== id) ticketView.trail.push(ticketView.id)
  show(id)
}

export function backTicket() {
  const prev = ticketView.trail.pop()
  if (prev) show(prev)
}

export function closeTicket() {
  const id = ticketView.id
  if (id) rememberPanel({ key: `ticket:${id}`, kind: 'ticket', title: ticketView.task ? `${id} ${ticketView.task.name}` : id, hint: t('Ticket'), reopen: () => openTicket(id) })
  ticketView.id = null
  ticketView.trail = []
  ticketView.task = null
  ticketView.error = null
}

export async function refreshTicket() {
  if (!ticketView.id) return
  cache.delete(ticketView.id)
  await show(ticketView.id)
}

async function show(id: string) {
  ticketView.id = id
  ticketView.error = null
  ticketView.saveError = null
  const hit = cache.get(id)
  if (hit && Date.now() - hit.at < KEEP_MS) {
    ticketView.task = hit.task
    ticketView.loading = false
    return
  }
  ticketView.task = null
  ticketView.loading = true
  try {
    const task = await invoke<TaskView>('get_ticket', { id })
    cache.set(id, { at: Date.now(), task })
    // Another ticket may have been opened in the meantime.
    if (ticketView.id === id) ticketView.task = task
  } catch (e) {
    if (ticketView.id === id) ticketView.error = String(e)
  } finally {
    if (ticketView.id === id) ticketView.loading = false
  }
}

/**
 * Status change of a ticket, wherever it comes from (preview or board). The snapshot follows
 * on the Rust side; the preview and its cache are updated here. Rejects with ClickUp's error.
 */
export async function changeStatus(taskId: string, status: string, options: StatusOption[]) {
  await invoke<void>('set_ticket_status', { taskId, status })
  const option = options.find((s) => s.status === status)
  const apply = (t: TaskView): TaskView =>
    t.task_id !== taskId
      ? t
      : { ...t, status, closed: option?.closed ?? t.closed, status_color: option?.color ?? t.status_color, updated_at: new Date().toISOString() }
  for (const [key, hit] of cache) cache.set(key, { ...hit, task: apply(hit.task) })
  if (ticketView.task) ticketView.task = apply(ticketView.task)
}

/**
 * Assign a ticket to me, or remove me from it. The snapshot follows on the Rust side; this ticket's preview,
 * if cached, will be re-read. Rejects with ClickUp's error.
 */
export async function setMine(taskId: string, assign: boolean) {
  await invoke<void>('set_ticket_mine', { taskId, assign })
  for (const [key, hit] of cache) if (hit.task.task_id === taskId) cache.delete(key)
  if (ticketView.task?.task_id === taskId) await refreshTicket()
}

/**
 * Add a ticket to the current sprint, in addition to its list. The snapshot follows on the Rust side;
 * this ticket's preview, if cached, will be re-read. Rejects with ClickUp's error.
 */
export async function addToSprint(taskId: string) {
  await invoke<void>('add_to_sprint', { taskId })
  for (const [key, hit] of cache) if (hit.task.task_id === taskId) cache.delete(key)
  if (ticketView.task?.task_id === taskId) await refreshTicket()
}

export type Priority = 'urgent' | 'high' | 'normal' | 'low'

/**
 * Priority of a ticket, wherever it comes from (row, card, preview, menu); `null`: none. The
 * snapshot follows on the Rust side; the preview and its cache are updated here. Rejects with ClickUp's error.
 */
export async function setPriority(taskId: string, priority: Priority | null) {
  await invoke<void>('set_ticket_priority', { taskId, priority })
  const apply = (t: TaskView): TaskView => (t.task_id === taskId ? { ...t, priority } : t)
  for (const [key, hit] of cache) cache.set(key, { ...hit, task: apply(hit.task) })
  if (ticketView.task) ticketView.task = apply(ticketView.task)
}

/**
 * A ticket's estimate in hours (`null`: cleared): the tracker's own (`field` null) or one of the
 * estimate custom fields. The snapshot follows on the Rust side; the preview and its cache here.
 */
export async function setEstimate(taskId: string, field: string | null, hours: number | null) {
  await invoke<void>('set_ticket_estimate', { taskId, field, hours })
  const label = hours === null ? null : String(Math.round(hours * 100) / 100)
  const apply = (t: TaskView): TaskView => {
    if (t.task_id !== taskId) return t
    if (field === null) return { ...t, time_estimate: hours === null ? null : Math.round(hours * 3_600_000) }
    return { ...t, fields: t.fields.map((f) => (f.name.toLowerCase() === field.toLowerCase() ? { ...f, values: label === null ? [] : [{ label, color: null, order: null }] } : f)) }
  }
  for (const [key, hit] of cache) cache.set(key, { ...hit, task: apply(hit.task) })
  if (ticketView.task) ticketView.task = apply(ticketView.task)
}

/** Comment on the displayed ticket (`mentions`: those it notifies); the preview is re-read to show the comment. */
export async function addComment(taskId: string, text: string, mentions: Mention[] = []) {
  await invoke<void>('add_ticket_comment', { taskId, text, mentions })
  if (ticketView.task?.task_id === taskId) await refreshTicket()
}

/** Edit one of my comments on the displayed ticket. */
export async function editComment(taskId: string, comment: string, text: string, mentions: Mention[] = []) {
  await invoke<void>('edit_ticket_comment', { taskId, comment, text, mentions })
  if (ticketView.task?.task_id === taskId) await refreshTicket()
}

/** New status of the ticket displayed in the preview. */
export async function setStatus(status: string) {
  const task = ticketView.task
  if (!task || ticketView.saving || status === task.status) return
  ticketView.saving = true
  ticketView.saveError = null
  try {
    await changeStatus(task.task_id, status, task.statuses)
  } catch (e) {
    ticketView.saveError = String(e)
    // The picker goes back to the actual status.
    if (ticketView.task?.task_id === task.task_id) ticketView.task = { ...ticketView.task }
  } finally {
    ticketView.saving = false
  }
}

/**
 * Click on a ticket identifier: the preview; ⌘-click (or middle click) keeps the link to
 * ClickUp. Outside the app, the link opens normally.
 */
export function previewClick(id: string) {
  return (e: MouseEvent) => {
    if (!canPreview() || e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return
    e.preventDefault()
    openTicket(id)
  }
}
