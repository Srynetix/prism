import { afterEach, describe, expect, it, vi } from 'vitest'

// The module follows the page (its address, its window coming back): a bare one under Node.
const page = globalThis as Record<string, unknown>
page.location ??= { hash: '', pathname: '/', search: '' }
page.history ??= { replaceState: () => {} }
page.addEventListener ??= () => {}
page.document ??= { hidden: false, addEventListener: () => {} }
page.window ??= globalThis

describe('today', () => {
  afterEach(() => {
    vi.useRealTimers()
    vi.resetModules()
  })

  it('moves to the new day, at midnight or on waking; a day picked stays', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date(2026, 10, 19, 23, 59, 30))
    vi.resetModules()
    const f = await import('./filter.svelte')
    expect(f.todayKey()).toBe('2026-11-19')
    expect(f.selectedDay()).toBe('2026-11-19')
    // Midnight passes with Prism open: its minute's check moves it.
    vi.advanceTimersByTime(60_000)
    expect(f.todayKey()).toBe('2026-11-20')
    expect(f.selectedDay()).toBe('2026-11-20')
    // Asleep two days, its timers standing still: the date read again as they fire on waking.
    f.setDate('2026-11-19')
    vi.setSystemTime(new Date(2026, 10, 22, 8, 0, 0))
    vi.advanceTimersByTime(60_000)
    expect(f.todayKey()).toBe('2026-11-22')
    expect(f.selectedDay(), 'a day picked by hand').toBe('2026-11-19')
  })
})
