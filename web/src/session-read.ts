import { invoke } from './bridge'
import type { SessionStatus } from './types'

/**
 * A session marked read (`read_marks` in the core): its turn over, it waits for me no more — out
 * of « Waiting for you », the alerts and the counts — and shows as stopped, until it moves again.
 */

/** Its turn over and asking nothing (a question, a plan to approve: its `needs_action`): it may be marked read. */
export const canMarkRead = (x: { status: SessionStatus; needs_action?: string | null }) => (x.status === 'needs_you' || x.status === 'review_ready') && !x.needs_action?.trim()

/** Marked read: agents or Claude Desktop sessions, in one call. */
export const markRead = (ids: string[]) => invoke('session_read', { ids })
