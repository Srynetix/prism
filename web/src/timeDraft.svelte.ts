/** Time entry being prepared: shared so that a block (an MR…) can open the panel prefilled. */
import { setPref } from './layout.svelte'

/**
 * `asked` / `focused`: the "Log time" asks, and the last one whose duration was focused: kept here,
 * not in the drawer, which may be mounted by the ask itself (the toolbar's ⋯ at half a screen).
 */
export const timeDraft = $state({ open: false, ticket: '', description: '', asked: 0, focused: 0 })

/** Opened on a ticket ("Log time"): on its day's form, its ticket filled in, the duration focused. */
export function openTimeDraft(ticket: string, description = '') {
  timeDraft.ticket = ticket
  timeDraft.description = description
  setPref('time.view', 'day')
  timeDraft.open = true
  timeDraft.asked++
}
