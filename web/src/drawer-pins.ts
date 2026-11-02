import { appearance } from './fun.svelte'
import { pref, setPref } from './layout.svelte'

/**
 * A side pane pinned (`[appearance] pin_drawers`): a click outside no longer closes it, only its ×
 * (or Escape, where it closes). Remembered by kind of pane (`drawers.pinned`), not by what it shows:
 * the logs pinned stay pinned for the next pod. `clickedOutside` reads it off the pane (`data-pinned`).
 */
export type DrawerKind = 'mr' | 'ticket' | 'logs' | 'pod-logs' | 'shell' | 'event' | 'argo' | 'thread'

export const pinsOn = () => appearance().pin_drawers

const pinned = () => pref<DrawerKind[]>('drawers.pinned', [])

export const isPinned = (kind: DrawerKind) => pinsOn() && pinned().includes(kind)

export function togglePin(kind: DrawerKind) {
  const now = pinned()
  setPref('drawers.pinned', now.includes(kind) ? now.filter((k) => k !== kind) : [...now, kind])
}
