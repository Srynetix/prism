import { invoke } from './bridge'
import type { SessionStatus } from './types'

/**
 * A session marked read (`read_marks` in the core): its turn over, it waits for me no more — out
 * of « Waiting for you », the alerts and the counts — and shows as stopped, until it moves again.
 */

/** An agent's question: it waits for an answer, never marked read. */
const QUESTION = 'Answer its question'

/** Its turn over and asking nothing: it may be marked read. */
export const canMarkRead = (x: { status: SessionStatus; needs_action?: string | null }) => (x.status === 'needs_you' || x.status === 'review_ready') && x.needs_action !== QUESTION

/** Marked read: an agent or a Claude Desktop session. */
export const markRead = (id: string) => invoke('session_read', { id })
