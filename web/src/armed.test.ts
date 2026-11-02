import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { armer } from './armed.svelte'

describe('armer', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('confirms on the second click on the same key', () => {
    const a = armer()
    expect(a.confirm('rm:1')).toBe(false)
    expect(a.is('rm:1')).toBe(true)
    expect(a.confirm('rm:1')).toBe(true)
    expect(a.is('rm:1')).toBe(false)
  })

  it('arms one key at a time', () => {
    const a = armer()
    a.confirm('rm:1')
    expect(a.confirm('rm:2')).toBe(false)
    expect(a.is('rm:1')).toBe(false)
    expect(a.confirm('rm:2')).toBe(true)
  })

  it('disarms after a while, or when asked', () => {
    const a = armer(3000)
    a.confirm()
    vi.advanceTimersByTime(2999)
    expect(a.is()).toBe(true)
    vi.advanceTimersByTime(1)
    expect(a.is()).toBe(false)
    expect(a.confirm()).toBe(false)
    a.disarm()
    expect(a.confirm()).toBe(false)
  })
})
