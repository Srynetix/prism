import { describe, expect, it } from 'vitest'
import { firstBelow, ScrollAnchor } from './scroll-anchor'

/** Blocks one under the other in a scrolling area, as the diff's files: their tops worked out. */
function page(heights: number[]) {
  const blocks = heights.map((h, i) => ({ i, h }))
  const area = { scrollTop: 0 }
  const topOf = (b: (typeof blocks)[number]) => blocks.slice(0, b.i).reduce((n, x) => n + x.h, 0) - area.scrollTop
  const anchor = new ScrollAnchor(area, () => blocks.find((b) => topOf(b) + b.h > 0) ?? null, (a, b) => a.i < b.i)
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

  it('a block seen for the first time moves nothing', () => {
    const { area, anchor } = page([200, 200])
    area.scrollTop = 250
    anchor.note()
    expect(anchor.hold([{ el: { i: -1, h: 300 }, height: 300 }])).toBe(0)
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
