import { live } from './live.svelte'
import { parseShortcut } from './shortcut'

/** Quick note entry: the Notes block over any view, with the `[notes] key` shortcut. */
export const quickNotes = $state({ open: false, opened: 0 })

export const DEFAULT_NOTES_KEY = 'Cmd+Shift+N'
export const quickNotesKey = () => parseShortcut(live.snapshot?.notes?.key || DEFAULT_NOTES_KEY)

export function openQuickNotes() {
  quickNotes.open = true
  // Each opening gives focus back to the input, even when already open.
  quickNotes.opened++
}
export const closeQuickNotes = () => (quickNotes.open = false)
export const toggleQuickNotes = () => (quickNotes.open ? closeQuickNotes() : openQuickNotes())
