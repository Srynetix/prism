import { beforeAll, describe, expect, it, vi } from 'vitest'
import { canScroll, pageSwipe } from './page-swipe'

/** A fake element: scrollable when `overflow` and taller content. */
const el = (o: { parent?: object; overflow?: string; scrollTop?: number; scrollHeight?: number; clientHeight?: number } = {}) =>
  ({ parentElement: o.parent ?? null, overflow: o.overflow ?? 'visible', scrollTop: o.scrollTop ?? 0, scrollHeight: o.scrollHeight ?? 100, clientHeight: o.clientHeight ?? 100 }) as unknown as Element & { scrollTop: number }

beforeAll(() => {
  vi.stubGlobal('getComputedStyle', (e: { overflow: string }) => ({ overflowY: e.overflow }))
  vi.stubGlobal('WheelEvent', { DOM_DELTA_LINE: 1 })
})

/** The grid with a list in it, and a swipe of `n` events of `dy` from `t` ms, 16 ms apart. */
function setup() {
  const root = el()
  const list = el({ parent: root, overflow: 'auto', scrollHeight: 500, clientHeight: 100 })
  let handler: (e: object) => void = () => {}
  const node = { addEventListener: (_: string, h: (e: object) => void) => (handler = h), removeEventListener: () => {} } as unknown as HTMLElement
  Object.assign(node, { parentElement: null })
  const step = vi.fn()
  pageSwipe(step, () => true)(node)
  const swipe = (target: Element, dy: number, n: number, t: number) => {
    for (let i = 0; i < n; i++) handler({ target, deltaY: dy, deltaX: 0, deltaMode: 0, ctrlKey: false, timeStamp: t + i * 16 })
  }
  return { root, list, step, swipe }
}

describe('canScroll', () => {
  it('sees a list with room left in that direction', () => {
    const root = el()
    const list = el({ parent: root, overflow: 'auto', scrollTop: 0, scrollHeight: 500 })
    expect(canScroll(list, root, 1)).toBe(true)
    expect(canScroll(list, root, -1)).toBe(false)
  })
})

describe('pageSwipe', () => {
  it('changes page once, from what does not scroll', () => {
    const { root, step, swipe } = setup()
    swipe(root, 30, 20, 0)
    expect(step).toHaveBeenCalledTimes(1)
    expect(step).toHaveBeenCalledWith(1)
  })

  it('leaves a list that still scrolls alone, momentum included', () => {
    const { list, step, swipe } = setup()
    swipe(list, 30, 10, 0)
    // The list reaches its end while the same gesture goes on.
    list.scrollTop = 400
    swipe(list, 30, 10, 160)
    expect(step).not.toHaveBeenCalled()
    // A new gesture, the list at its end: the page changes.
    swipe(list, 30, 10, 1000)
    expect(step).toHaveBeenCalledWith(1)
  })

  it('goes back up from the top of a list', () => {
    const { list, step, swipe } = setup()
    swipe(list, -30, 10, 0)
    expect(step).toHaveBeenCalledWith(-1)
  })

  it('ignores a mouse wheel’s spaced notches and a short nudge', () => {
    const { root, step, swipe } = setup()
    for (let i = 0; i < 5; i++) swipe(root, 100, 1, i * 300)
    swipe(root, 10, 6, 5000)
    expect(step).not.toHaveBeenCalled()
  })
})
