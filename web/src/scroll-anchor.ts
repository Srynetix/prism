/**
 * Scroll anchoring, done by hand. WebKit (Prism's view on macOS) has none: a block above the
 * visible area that changes height (a file of a diff drawn as it nears, highlighted, an image
 * loaded, a thread opened) moves everything under it, and the view seems to scroll by itself.
 * Each block's height is known (a `ResizeObserver`); noted at each scroll, the first one visible
 * (the anchor), and the line of it at the top. A block above it that changes height: the scroll
 * moves as much. The anchor itself: as much as its line moved within it (a change above that line,
 * out of sight). One under it: nothing moves. What Chrome's `overflow-anchor` does.
 * By heights and not by the anchor's position on screen: a block may be drawn before the scroll is
 * noted (a note reads the layout as it is then), its change would be missed.
 */

/** What scrolls: its offset only, so that it can be faked. */
export interface Scroller {
  scrollTop: number
}

export interface AnchorHooks<E> {
  /** The block at the top of the visible area. */
  find: () => E | null
  /** `a` comes before `b` in the page, and is not it. */
  above: (a: E, b: E) => boolean
  /** Still in the page (a block removed is no anchor). */
  alive?: (el: E) => boolean
  /**
   * In the anchor, the line at the top: a function giving where it is within the anchor (from its
   * top), `NaN` once it is gone; none if the anchor has no lines.
   */
  line?: (anchor: E) => (() => number) | null
}

export class ScrollAnchor<E> {
  /** The block at the top when last noted, its line, and where that line was within it. */
  private anchor: E | null = null
  private line: (() => number) | null = null
  private lineAt = NaN
  /** Each block's last height. */
  private heights = new Map<E, number>()

  constructor(
    private scroller: Scroller,
    private hooks: AnchorHooks<E>,
  ) {}

  /** Notes the block at the top, and its line. */
  note() {
    this.anchor = this.hooks.find()
    this.line = this.anchor && this.hooks.line ? this.hooks.line(this.anchor) : null
    this.lineAt = this.line ? this.line() : NaN
  }

  /** Blocks resized (their new heights): those above the anchor scroll as much. Returns by how much. */
  hold(sizes: readonly { el: E; height: number }[]): number {
    const alive = this.hooks.alive ?? (() => true)
    if (this.anchor && !alive(this.anchor)) this.note()
    const anchor = this.anchor ?? this.hooks.find()
    let moved = 0
    for (const { el, height } of sizes) {
      const was = this.heights.get(el)
      this.heights.set(el, height)
      if (was === undefined || !anchor) continue
      if (el === anchor) {
        // Its line moved within it: something above that line, out of sight, changed.
        const now = this.line ? this.line() : NaN
        if (Number.isFinite(now) && Number.isFinite(this.lineAt)) moved += now - this.lineAt
      } else if (this.hooks.above(el, anchor)) moved += height - was
    }
    if (Math.abs(moved) >= 0.5) this.scroller.scrollTop += moved
    else moved = 0
    this.note()
    return moved
  }

  /** A block gone: its height forgotten, and it is no anchor any more. */
  forget(el: E) {
    this.heights.delete(el)
    if (this.anchor === el) {
      this.anchor = null
      this.line = null
    }
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
 * The engine anchors the scroll itself (`overflow-anchor`: Chromium, Firefox), over everything in
 * the area: left to it. WebKit does not.
 */
export const anchorsItself = () => typeof CSS !== 'undefined' && CSS.supports?.('overflow-anchor', 'auto') === true

/**
 * Holds `area`'s scroll (a scrolling element, or the page's) over the blocks observed: notes at
 * each scroll, holds at each change of their size. Where the engine anchors by itself, only
 * `onresize` (the sizes) is called: its anchoring covers the whole area, not only these blocks.
 */
export function holdScroll(area: HTMLElement, hooks: Omit<AnchorHooks<HTMLElement>, 'above' | 'alive'>, onresize?: (entries: ResizeObserverEntry[]) => void, manual = !anchorsItself()) {
  const page = area === document.scrollingElement
  const anchor = new ScrollAnchor<HTMLElement>(area, {
    ...hooks,
    alive: (el) => el.isConnected,
    above: (a, b) => a.isConnected && b.isConnected && !!(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) && !a.contains(b),
  })
  const target: EventTarget = page ? window : area
  const onscroll = () => anchor.note()
  if (manual) target.addEventListener('scroll', onscroll, { passive: true })
  const sizes = new ResizeObserver((entries) => {
    onresize?.(entries)
    if (manual) anchor.hold(entries.map((e) => ({ el: e.target as HTMLElement, height: e.borderBoxSize?.[0]?.blockSize ?? e.contentRect.height })))
  })
  if (manual) anchor.note()
  return {
    observe: (el: HTMLElement) => sizes.observe(el),
    unobserve: (el: HTMLElement) => {
      sizes.unobserve(el)
      anchor.forget(el)
    },
    note: () => manual && anchor.note(),
    destroy() {
      sizes.disconnect()
      target.removeEventListener('scroll', onscroll)
    },
  }
}
