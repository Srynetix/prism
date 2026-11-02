import { pref, setPref } from './layout.svelte'
import { t } from './i18n.svelte'

/**
 * Resizable columns of a table: a `<table>` with a header row, or a grid whose rows are subgrids
 * (Environments, its header row marked `data-columns-head`). `{@attach columns('envs')}` on the
 * table, `{@attach grip}` on each header cell that can be resized: a handle on its right edge.
 * Dragging it sets that column's width; the others keep the width they had (the first drag fixes
 * them all). A double-click gives the table back its own widths. Remembered per table (pref
 * `columns.<id>`), in px; null for a column hidden when they were fixed (a narrow block hides
 * some): it keeps its own width.
 */
type Widths = (number | null)[]
const key = (id: string) => `columns.${id}`
const MIN = 24

/** The widths set on a table, or null: its own. */
export const widths = (id: string): Widths | null => pref<Widths | null>(key(id), null)

/** A `<table>`'s header cells, or a grid's header row's. */
const headers = (el: HTMLElement): HTMLElement[] =>
  el instanceof HTMLTableElement ? [...(el.tHead?.rows[0]?.cells ?? [])] : [...(el.querySelector('[data-columns-head]')?.children ?? [])].map((c) => c as HTMLElement)
const hidden = (c: HTMLElement) => getComputedStyle(c).display === 'none'

/**
 * The columns' widths as shown now; null for a hidden one, or one with nothing to show (auto). In
 * a grid, a hidden cell gives up its track: the visible ones take the tracks in order.
 */
function measure(el: HTMLElement): Widths {
  const cells = headers(el)
  if (el instanceof HTMLTableElement) return cells.map((c) => c.getBoundingClientRect().width)
  const tracks = getComputedStyle(el).gridTemplateColumns.split(' ').map((w) => parseFloat(w) || 0)
  let k = 0
  return cells.map((c) => {
    if (hidden(c)) return null
    const w = Math.round(tracks[k++] ?? 0)
    return w > 0 ? w : null
  })
}

/** A grid's template: the visible columns in order, the tracks left over at 0, then the rest of the width. */
function template(cells: HTMLElement[], w: Widths): string {
  const shown = cells.flatMap((c, i) => (hidden(c) ? [] : [w[i] === null ? 'auto' : `${w[i]}px`]))
  return [...shown, ...Array(cells.length - shown.length).fill('0px'), 'minmax(0, 1fr)'].join(' ')
}

/**
 * On the table: applies the widths set, while they match its columns (a compact form has others),
 * and again when its size hides or shows some. `shape`: what changes its columns (compact or
 * not), to apply them again when it changes.
 */
export function columns(id: string, shape?: unknown) {
  void shape
  return (el: HTMLElement) => {
    const w = widths(id)
    el.dataset.columns = id
    const apply = () => {
      const cells = headers(el)
      const fits = !!w && cells.length === w.length
      if (fits) el.dataset.sized = ''
      else delete el.dataset.sized
      if (el instanceof HTMLTableElement) {
        // The last column takes what is left of the width; wider than the block, it scrolls.
        el.style.tableLayout = fits ? 'fixed' : ''
        el.style.minWidth = fits ? `${w!.reduce<number>((a, b) => a + (b ?? 0), 0)}px` : ''
        cells.forEach((c, i) => (c.style.width = fits && w![i] !== null && i < cells.length - 1 ? `${w![i]}px` : ''))
      } else {
        // A variable, not the template itself: the table's CSS decides where it applies.
        el.style.setProperty('--cols', fits ? template(cells, w!) : '')
      }
    }
    apply()
    if (!w || el instanceof HTMLTableElement) return
    // A narrower block hides columns (container queries): their tracks go to 0.
    const seen = new ResizeObserver(apply)
    seen.observe(el)
    return () => seen.disconnect()
  }
}

/** On a header cell: the handle that resizes its column. */
export function grip(cell: HTMLElement) {
  const handle = document.createElement('span')
  handle.className = 'col-grip'
  handle.title = t('Drag to resize the column · double-click: automatic widths')
  handle.addEventListener('click', (e) => e.stopPropagation())
  handle.addEventListener('dblclick', (e) => {
    e.stopPropagation()
    const table = cell.closest<HTMLElement>('[data-columns]')
    if (table) setPref(key(table.dataset.columns!), null)
  })
  handle.addEventListener('pointerdown', (e) => {
    const table = cell.closest<HTMLElement>('[data-columns]')
    if (!table || e.button !== 0) return
    e.preventDefault()
    e.stopPropagation()
    const id = table.dataset.columns!
    const index = [...cell.parentElement!.children].indexOf(cell)
    // The widths shown, those set kept where a column is hidden now.
    const set = table.dataset.sized !== undefined ? widths(id) : null
    const start = measure(table).map((x, i) => x ?? set?.[i] ?? null)
    const from = start[index] ?? cell.getBoundingClientRect().width
    const x0 = e.clientX
    handle.setPointerCapture(e.pointerId)
    document.body.classList.add('col-resizing')
    let frame = 0
    const move = (ev: PointerEvent) => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        const next = [...start]
        next[index] = Math.max(MIN, Math.round(from + ev.clientX - x0))
        setPref(key(id), next)
      })
    }
    const up = () => {
      handle.removeEventListener('pointermove', move)
      document.body.classList.remove('col-resizing')
    }
    handle.addEventListener('pointermove', move)
    handle.addEventListener('pointerup', up, { once: true })
    handle.addEventListener('pointercancel', up, { once: true })
  })
  cell.classList.add('col-resizable')
  cell.append(handle)
  return () => {
    handle.remove()
    cell.classList.remove('col-resizable')
  }
}
