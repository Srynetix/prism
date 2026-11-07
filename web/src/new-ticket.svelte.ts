import { hasCore, invoke } from './bridge'
import { live } from './live.svelte'
import { offers } from './tracker'
import type { SprintBoard } from './bindings/SprintBoard'

/**
 * A new ticket, on whichever tracker (`create_ticket`): on one of the sprint's boards, in one of
 * its open lanes, with a description in Markdown and assigned to me if asked. From the Sprint
 * block's + (computer and phone) and the palette's "New ticket…" (a dialog).
 */

/** The boards a ticket can be created on; none: no creating here. */
export const ticketBoards = (): SprintBoard[] => (hasCore() && offers('create') ? (live.snapshot?.sprint?.boards ?? []) : [])

export interface TicketDraft {
  board: string
  status: string
  title: string
  description: string
  assignMe: boolean
}

/** Creates it; its key. */
export const createTicket = (d: TicketDraft) =>
  invoke<string>('create_ticket', { board: d.board, status: d.status, title: d.title.trim(), description: d.description.trim() || null, assignMe: d.assignMe })

/** The palette's dialog. */
export const newTicketDialog = $state({ open: false })
export const openNewTicket = () => (newTicketDialog.open = true)
export const closeNewTicket = () => (newTicketDialog.open = false)
