import { describe, expect, it } from 'vitest'
import { hours } from './retro'

// The retro's own figures are tested where they are computed: crates/blocks/src/trail/retro.rs.
describe('hours', () => {
  it('formats minutes', () => {
    expect(hours(45)).toBe('45 min')
    expect(hours(120)).toBe('2 h')
    expect(hours(200)).toBe('3 h 20')
    expect(hours(65)).toBe('1 h 05')
  })
})
