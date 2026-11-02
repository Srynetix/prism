/** Time entry being prepared: shared so that a block (an MR…) can open the panel prefilled. */
export const timeDraft = $state({ open: false, ticket: '', description: '' })

export function openTimeDraft(ticket: string, description = '') {
  timeDraft.ticket = ticket
  timeDraft.description = description
  timeDraft.open = true
}
