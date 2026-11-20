import { describe, expect, it } from 'vitest'
import { firstBelow, ScrollAnchor } from './scroll-anchor'

/** Blocks one under the other in a scrolling area, as the diff's files: their tops worked out. */
function page(heights: number[]) {
  /** `inner`: where its line at the top is, from its own top (what grows above that line pushes it). */
  const blocks = heights.map((h, i) => ({ i, h, gone: false, inner: 0 }))
  const area = { scrollTop: 0 }
  const topOf = (b: (typeof blocks)[number]) => blocks.slice(0, b.i).reduce((n, x) => n + (x.gone ? 0 : x.h), 0) - area.scrollTop
  const anchor = new ScrollAnchor(area, {
    find: () => blocks.find((b) => !b.gone && topOf(b) + b.h > 0) ?? null,
    above: (a, b) => a.i < b.i,
    alive: (b) => !b.gone,
    line: (b) => () => b.inner,
  })
  /** What the observer reports: the blocks given, at their height now. */
  const report = (...bs: (typeof blocks)[number][]) => anchor.hold(bs.map((b) => ({ el: b, height: b.h })))
  report(...blocks)
  return { blocks, area, topOf, anchor, report }
}

describe('scroll anchor', () => {
  it('a block above that grows does not move the visible one', () => {
    const { blocks, area, topOf, anchor, report } = page([200, 200, 200, 200, 200])
    area.scrollTop = 450
    anchor.note()
    const before = topOf(blocks[2])
    blocks[0].h = 520
    blocks[1].h = 90
    expect(report(blocks[0], blocks[1])).toBe(210)
    expect(topOf(blocks[2])).toBe(before)
    expect(area.scrollTop).toBe(660)
  })

  it('a block under it, or itself, growing moves nothing', () => {
    const { blocks, area, anchor, report } = page([200, 200, 200, 200])
    area.scrollTop = 250
    anchor.note()
    blocks[1].h = 900
    blocks[3].h = 50
    expect(report(blocks[1], blocks[3])).toBe(0)
    expect(area.scrollTop).toBe(250)
  })

  it('holds still while scrolling up through blocks drawn as they near', () => {
    const { blocks, area, topOf, anchor, report } = page(Array(20).fill(100))
    area.scrollTop = 1500
    anchor.note()
    for (let k = 0; k < 10; k++) {
      area.scrollTop -= 60
      anchor.note()
      const first = blocks.find((b) => topOf(b) + b.h > 0)!
      const at = topOf(first)
      // The two blocks above the visible one, drawn: twice their estimate.
      const drawn = blocks.slice(Math.max(0, first.i - 2), first.i).filter((b) => b.h < 200)
      for (const b of drawn) b.h = 200
      report(...drawn)
      expect(topOf(first)).toBe(at)
    }
  })

  it('a block drawn before the scroll is noted still holds', () => {
    const { blocks, area, topOf, anchor, report } = page([200, 200, 200, 200])
    area.scrollTop = 450
    anchor.note()
    const at = topOf(blocks[2])
    // Drawn, then the scroll noted (its layout read as it is now), then the observer.
    blocks[0].h = 330
    area.scrollTop -= 40
    anchor.note()
    report(blocks[0])
    expect(topOf(blocks[2])).toBe(at + 40)
  })

  it('a line of the anchor pushed down by its own top, out of sight, stays', () => {
    const { blocks, area, anchor, report } = page([200, 600, 200])
    area.scrollTop = 500
    blocks[1].inner = 300
    anchor.note()
    // An image loads in its top part: its line at the top is 120 further down within it.
    blocks[1].inner = 420
    blocks[1].h = 720
    expect(report(blocks[1])).toBe(120)
    expect(area.scrollTop).toBe(620)
  })

  it('an anchor removed is no anchor: the next one holds', () => {
    const { blocks, area, topOf, anchor, report } = page([200, 200, 200, 200])
    area.scrollTop = 450
    anchor.note()
    blocks[2].gone = true
    const at = topOf(blocks[3])
    blocks[0].h = 260
    report(blocks[0])
    expect(topOf(blocks[3])).toBe(at)
  })

  it('a block seen for the first time moves nothing', () => {
    const { area, anchor } = page([200, 200])
    area.scrollTop = 250
    anchor.note()
    expect(anchor.hold([{ el: { i: -1, h: 300, gone: false, inner: 0 }, height: 300 }])).toBe(0)
  })
})

describe('firstBelow', () => {
  const bottoms = [10, 30, 30, 60, 100]
  it('finds the first past the edge', () => {
    expect(firstBelow(bottoms, (b) => b, 0)).toBe(10)
    expect(firstBelow(bottoms.map((b, i) => ({ b, i })), (x) => x.b, 30)?.i).toBe(3)
    expect(firstBelow(bottoms, (b) => b, 100)).toBeNull()
    expect(firstBelow([], (b: number) => b, 0)).toBeNull()
  })
})
