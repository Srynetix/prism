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

/**
 * The palette's dialog; `draft`: its fields prefilled (Prism's assistant's proposal changed before
 * it is created), `created`: told the new ticket's key.
 */
export const newTicketDialog = $state({ open: false, draft: null as Partial<TicketDraft> | null, created: null as ((key: string, d: TicketDraft) => void) | null })
export const openNewTicket = (draft: Partial<TicketDraft> | null = null, created: ((key: string, d: TicketDraft) => void) | null = null) => Object.assign(newTicketDialog, { open: true, draft, created })
export const closeNewTicket = () => Object.assign(newTicketDialog, { open: false, draft: null, created: null })

/** The lane a new ticket goes in on `board` by default: the first one to pick, else the first open one. */
export function defaultLane(board: SprintBoard | undefined, pick: string[]): string {
  const lanes = board?.statuses.filter((s) => !s.closed) ?? []
  return (lanes.find((s) => pick.includes(s.status)) ?? lanes[0])?.status ?? ''
}
