import type { Side } from './overlaps'

/** The overlap drawer: one overlap (its id, `overlapId`), as one of its sides opened it. */
export const overlapView = $state({ open: false, id: '', side: 'a' as Side })

export function openOverlap(id: string, side: Side = 'a') {
  Object.assign(overlapView, { open: true, id, side })
}

export const closeOverlap = () => Object.assign(overlapView, { open: false, id: '' })
