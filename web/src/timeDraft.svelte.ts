/** Time entry being prepared: shared so that a block (an MR…) can open the panel prefilled. */
export const timeDraft = $state({ open: false, ticket: '', description: '', asked: 0 })

/** Opened on a ticket ("Log time"): its ticket filled in, the duration is what is left to type (`asked`: focus it). */
export function openTimeDraft(ticket: string, description = '') {
  timeDraft.ticket = ticket
  timeDraft.description = description
  timeDraft.open = true
  timeDraft.asked++
}
