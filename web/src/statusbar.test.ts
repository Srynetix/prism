import { describe, expect, it } from 'vitest'
import { arrange, DEFAULT_ORDER, fullOrder, overflow, SIDE_BREAK } from './statusbar'

describe('fullOrder', () => {
  it('reads an empty order as the default one', () => {
    expect(fullOrder([])).toEqual(DEFAULT_ORDER)
  })
  it('keeps the order given and puts back what it leaves out after its default neighbour', () => {
    const order = fullOrder(['clock', SIDE_BREAK, 'claude'])
    expect(order[0]).toBe('clock')
    expect(order.indexOf('claude')).toBeGreaterThan(order.indexOf(SIDE_BREAK))
    expect(order.indexOf('pipeline')).toBe(order.indexOf('claude') + 1)
    expect(new Set(order).size).toBe(DEFAULT_ORDER.length)
  })
  it('drops unknown ids and repeats, and adds a missing side break', () => {
    const order = fullOrder(['nope', 'clock', 'clock'])
    expect(order.filter((id) => id === 'clock')).toHaveLength(1)
    expect(order).not.toContain('nope')
    expect(order).toContain(SIDE_BREAK)
  })
})

describe('arrange', () => {
  it('puts what waits for you first, in its own zone, and leaves hidden items out', () => {
    const [left, right] = arrange([], ['shells'])
    expect(left.zones[0].zone).toBe('you')
    expect(left.zones[0].items[0].id).toBe('claude')
    expect(left.zones.flatMap((z) => z.items.map((i) => i.id))).not.toContain('shells')
    expect(right.zones.at(-1)!.items.at(-1)!.id).toBe('clock')
  })
})

describe('overflow', () => {
  const widths = new Map([['claude', 100], ['repos', 80], ['shells', 60], ['feedback', 30], ['clock', 40]])
  const shown = ['claude', 'repos', 'shells', 'feedback', 'clock']
  it('moves nothing when the bar has room', () => {
    expect(overflow(shown, widths, 400, 30)).toEqual([])
  })
  it('moves the least needed first, never what waits for you nor the clock', () => {
    expect(overflow(shown, widths, 250, 30)).toEqual(['feedback', 'shells'])
    expect(overflow(shown, widths, 10, 30)).toEqual(['feedback', 'shells', 'repos'])
  })
  it('brings back a narrow item that fits once a wider one is gone', () => {
    expect(overflow(shown, new Map([...widths, ['shells', 120]]), 290, 30)).toEqual(['shells'])
  })
})
