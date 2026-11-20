/**
 * Scroll anchoring, done by hand. WebKit (Prism's view on macOS) has none: a block above the
 * visible area that changes height (a file of a diff drawn as it nears, highlighted, an image
 * loaded, a thread opened) moves everything under it, and the view seems to scroll by itself.
 * Each block's height is known (a `ResizeObserver`); noted at each scroll, the first one visible
 * (the anchor). A block above it that changes height: the scroll moves as much, what is read stays
 * where it is. One in it or under it: nothing moves. What Chrome's `overflow-anchor` does.
 * By heights and not by the anchor's position: a block may be drawn before the scroll is noted
 * (a note reads the layout as it is then), its change would be missed.
 */

/** What scrolls: its offset only, so that it can be faked. */
export interface Scroller {
  scrollTop: number
}

export class ScrollAnchor<E> {
  /** The block at the top when last noted. */
  private anchor: E | null = null
  /** Each block's last height. */
  private heights = new Map<E, number>()

  /**
   * `find`: the block at the top of the visible area; `above`: `a` comes before `b` in the page,
   * and is not it.
   */
  constructor(
    private scroller: Scroller,
    private find: () => E | null,
    private above: (a: E, b: E) => boolean,
  ) {}

  /** Notes the block at the top. */
  note() {
    this.anchor = this.find()
  }

  /** Blocks resized (their new heights): those above the anchor scroll as much. Returns by how much. */
  hold(sizes: readonly { el: E; height: number }[]): number {
    const anchor = this.anchor ?? this.find()
    let moved = 0
    for (const { el, height } of sizes) {
      const was = this.heights.get(el)
      this.heights.set(el, height)
      if (was !== undefined && anchor && el !== anchor && this.above(el, anchor)) moved += height - was
    }
    if (Math.abs(moved) >= 0.5) this.scroller.scrollTop += moved
    else moved = 0
    this.note()
    return moved
  }

  /** A block gone: its height forgotten. */
  forget(el: E) {
    this.heights.delete(el)
  }
}

/** The first of `items` (in order down the page) whose bottom is past `edge`: by halves. */
export function firstBelow<T>(items: readonly T[], bottomOf: (t: T) => number, edge: number): T | null {
  let lo = 0
  let hi = items.length
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    if (bottomOf(items[mid]) > edge) hi = mid
    else lo = mid + 1
  }
  return items[lo] ?? null
}

/**
 * Holds `area`'s scroll (a scrolling element, or the page's) over the blocks observed: notes at
 * each scroll, holds at each change of their size. The browser's own anchoring off, not to
 * correct twice.
 */
export function holdScroll(area: HTMLElement, find: () => HTMLElement | null, onresize?: (entries: ResizeObserverEntry[]) => void) {
  const page = area === document.scrollingElement
  const anchor = new ScrollAnchor<HTMLElement>(area, find, (a, b) => !!(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) && !a.contains(b))
  const target: EventTarget = page ? window : area
  const onscroll = () => anchor.note()
  target.addEventListener('scroll', onscroll, { passive: true })
  const was = area.style.overflowAnchor
  area.style.overflowAnchor = 'none'
  const sizes = new ResizeObserver((entries) => {
    onresize?.(entries)
    anchor.hold(entries.map((e) => ({ el: e.target as HTMLElement, height: e.borderBoxSize?.[0]?.blockSize ?? e.contentRect.height })))
  })
  anchor.note()
  return {
    observe: (el: HTMLElement) => sizes.observe(el),
    unobserve: (el: HTMLElement) => {
      sizes.unobserve(el)
      anchor.forget(el)
    },
    note: () => anchor.note(),
    destroy() {
      sizes.disconnect()
      target.removeEventListener('scroll', onscroll)
      area.style.overflowAnchor = was
    },
  }
}
