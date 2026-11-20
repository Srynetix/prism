/**
 * The interface's widths (app.css, « Widths »): wide from 1 200 px, half a screen from 720 px,
 * narrow under it. For what CSS cannot do alone: a drawer instead of a column, a panel over the
 * terminal.
 */
const belowWide = typeof matchMedia === 'undefined' ? null : matchMedia('(max-width: 1199.98px)')
const state = $state({ belowWide: belowWide?.matches ?? false })
belowWide?.addEventListener('change', (e) => (state.belowWide = e.matches))

/** Under 1 200 px: half a screen or narrower. */
export const squeezed = (): boolean => state.belowWide
