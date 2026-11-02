/**
 * Side panes (MR, ticket, logs, event): a click outside a pane closes it, except
 * in a pane stacked above it (opened from the first), a menu, a select's or an input's list of
 * choices (app-drawn at the window's root: `SelectMenu`, `suggest.ts`) or an enlarged image.
 * `data-drawer` carries the stacking rank of each pane. A pinned pane (`data-pinned`, drawer-pins.ts)
 * stays.
 */
export function clickedOutside(e: PointerEvent, drawer: HTMLElement | undefined): boolean {
  const target = e.target as Element | null
  if (!drawer || drawer.dataset.pinned === 'true' || !target?.isConnected || drawer.contains(target)) return false
  const other = target.closest<HTMLElement>('[data-drawer]')
  if (other && Number(other.dataset.drawer) > Number(drawer.dataset.drawer)) return false
  return !target.closest('[role=menu], [role=listbox], [role=dialog], .lightbox')
}
