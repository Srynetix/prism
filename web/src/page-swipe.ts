import { used } from './feature-use.svelte'
/**
 * Two fingers up or down on the trackpad, over the rail, change page (`[appearance] page_swipe`),
 * where nothing is left to scroll that way: a gesture that starts on what does not scroll, or on
 * what is already at its end in that direction, goes to the next or previous page once it has
 * travelled far enough. Never over the pages themselves: there it would fight their lists' scroll. A gesture that starts by scrolling a list stays a scroll to its end, its
 * momentum included; one gesture changes page once. A mouse wheel's notches, spaced out, never
 * add up to a gesture; a pinch (⌃ + wheel) is left alone.
 */

/** Without a wheel event for this long, the gesture is over: the next one starts a new one. */
const GAP_MS = 180
/** How far a gesture travels before the page changes, in pixels. */
const DISTANCE = 140
/** And in how many events at least: a trackpad sends many small ones, a mouse one big one. */
const EVENTS = 4

type Mode = 'scroll' | 'page' | 'done'

/** Whether an element between `target` and `root` (included) still scrolls in direction `dir`. */
export function canScroll(target: Element | null, root: Element, dir: number): boolean {
  for (let el = target; el; el = el.parentElement) {
    if (/(auto|scroll|overlay)/.test(getComputedStyle(el).overflowY) && el.scrollHeight > el.clientHeight + 1) {
      if (dir > 0 ? el.scrollTop + el.clientHeight < el.scrollHeight - 1 : el.scrollTop > 0) return true
    }
    if (el === root) break
  }
  return false
}

/** Attachment on the rail: `step(1)` for the next page, `step(-1)` the previous one, while `on()`. */
export const pageSwipe = (step: (dir: 1 | -1) => void, on: () => boolean) => (node: HTMLElement) => {
  let last = -Infinity
  let mode: Mode = 'scroll'
  let travelled = 0
  let events = 0
  const onWheel = (e: WheelEvent) => {
    if (e.ctrlKey) return
    const fresh = e.timeStamp - last > GAP_MS
    last = e.timeStamp
    if (fresh) {
      mode = on() && !canScroll(e.target as Element, node, Math.sign(e.deltaY)) ? 'page' : 'scroll'
      travelled = 0
      events = 0
    }
    // Mostly sideways (a wide table): not a page change.
    if (mode !== 'page' || Math.abs(e.deltaY) < Math.abs(e.deltaX) * 2) return
    travelled += e.deltaMode === WheelEvent.DOM_DELTA_LINE ? e.deltaY * 16 : e.deltaY
    events++
    if (Math.abs(travelled) >= DISTANCE && events >= EVENTS) {
      mode = 'done'
      used('page-swipe')
      step(travelled > 0 ? 1 : -1)
    }
  }
  node.addEventListener('wheel', onWheel, { passive: true })
  return () => node.removeEventListener('wheel', onWheel)
}
