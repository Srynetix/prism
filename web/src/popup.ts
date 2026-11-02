/**
 * Placement of fixed-position popups (⋯ menus, manual jobs, theme, block help) against
 * their trigger, without ever leaving the window: measured once shown, they
 * open below the trigger if they fit there, otherwise on the side with the most
 * room, limited in height (and scrolling) when no side is enough; horizontally,
 * they stay within the window. Repositioned when their content, the window or a scroll
 * changes.
 */
export interface Placement {
  /** Edge of the trigger the popup aligns to: `end` (right), `start` (left), `center`. */
  align?: 'start' | 'end' | 'center'
  /** `below`: under the trigger (or above); `right`: to its right, aligned on its bottom. */
  side?: 'below' | 'right'
  gap?: number
}

/** Margin kept between a popup and the window edge. */
const MARGIN = 8

export function anchored(trigger: () => HTMLElement | null | undefined, { align = 'end', side = 'below', gap = 4 }: Placement = {}) {
  return (el: HTMLElement) => {
    // Ceiling set by the popup's CSS (⋯ menu: 380 px), read before our own styles.
    const cssMax = parseFloat(getComputedStyle(el).maxHeight) || Infinity
    const place = () => {
      const t = trigger()
      if (!t) return
      const r = t.getBoundingClientRect()
      // Displayed height: the whole content (scrolling part included, plus borders), within the
      // CSS ceiling. Placing by the whole content would leave a gap above the
      // button when the popup opens upwards. Measured without our own limit: once limited, a
      // popup whose body scrolls measures the limit, not its content; the limit would then be
      // lifted, the popup grow back, be limited again… and jump at every frame.
      const limit = el.style.maxHeight
      el.style.maxHeight = 'none'
      const h = Math.min(el.scrollHeight + (el.offsetHeight - el.clientHeight), cssMax)
      el.style.maxHeight = limit
      const w = el.offsetWidth
      const vw = window.innerWidth
      const vh = window.innerHeight
      let top: number
      let left: number
      let max: number
      if (side === 'right') {
        max = vh - 2 * MARGIN
        top = Math.min(Math.max(MARGIN, r.bottom - Math.min(h, max)), vh - MARGIN - Math.min(h, max))
        left = r.right + gap
      } else {
        const below = vh - r.bottom - gap - MARGIN
        const above = r.top - gap - MARGIN
        const down = h <= below || below >= above
        max = down ? below : above
        top = down ? r.bottom + gap : r.top - gap - Math.min(h, above)
        left = align === 'end' ? r.right - w : align === 'center' ? r.left + r.width / 2 - w / 2 : r.left
      }
      left = Math.min(Math.max(MARGIN, left), vw - w - MARGIN)
      set(el, 'top', `${Math.round(top)}px`)
      set(el, 'left', `${Math.round(left)}px`)
      set(el, 'right', 'auto')
      set(el, 'bottom', 'auto')
      // Without a limit on our side, the CSS ceiling applies again.
      set(el, 'maxHeight', h > max ? `${Math.floor(max)}px` : '')
    }
    place()
    const observer = new ResizeObserver(place)
    observer.observe(el)
    // The page scrolled, not the popup's own list: measuring lifts its height limit for a moment,
    // which brought its scroll back to the top at each move (a phone, where menus scroll).
    const scrolled = (e: Event) => {
      if (!(e.target instanceof Node && el.contains(e.target))) place()
    }
    window.addEventListener('resize', place)
    window.addEventListener('scroll', scrolled, true)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', place)
      window.removeEventListener('scroll', scrolled, true)
    }
  }
}

/** Writes only what changes: a rewritten style would retrigger the resize observer. */
function set(el: HTMLElement, key: 'top' | 'left' | 'right' | 'bottom' | 'maxHeight', value: string) {
  if (el.style[key] !== value) el.style[key] = value
}

/** Closing of the open popup: only one at a time in the whole app. */
let closeOpen: (() => void) | null = null

/**
 * To set on a shown popup: opening it closes the one that already was (another ⋯ menu,
 * a block's help…), even when the click that opens it does not bubble up to the window.
 */
export function exclusive(close: () => void) {
  return () => {
    if (closeOpen !== close) closeOpen?.()
    closeOpen = close
    return () => {
      if (closeOpen === close) closeOpen = null
    }
  }
}

export interface Dismiss {
  /** The button that opens it: a click there is its own (a toggle), not one outside. */
  trigger?: () => HTMLElement | null | undefined
  /** A resize of the window closes it too (a popup placed against its trigger). */
  resize?: boolean
}

/**
 * To set on a shown popup: a click outside it and its trigger, or Escape, closes it. The click
 * that opened it, still on its way up to the window when the popup appears, does not count.
 */
export function dismissable(close: () => void, { trigger, resize = false }: Dismiss = {}) {
  return (el: HTMLElement) => {
    const opened = performance.now()
    const click = (e: MouseEvent) => {
      const t = e.target as Node | null
      if (e.timeStamp <= opened || !t?.isConnected || el.contains(t) || trigger?.()?.contains(t)) return
      close()
    }
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close()
    }
    window.addEventListener('click', click)
    window.addEventListener('keydown', key)
    if (resize) window.addEventListener('resize', close)
    return () => {
      window.removeEventListener('click', click)
      window.removeEventListener('keydown', key)
      window.removeEventListener('resize', close)
    }
  }
}
