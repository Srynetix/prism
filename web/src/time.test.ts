import { describe, expect, it } from 'vitest'
import { parseDuration } from './time'

describe('parseDuration', () => {
  it.each([
    ['1h30', 90],
    ['1h30m', 90],
    ['2h', 120],
    ['45m', 45],
    ['45min', 45],
    ['45', 45],
    ['1.5', 90],
    ['1,5', 90],
    [' 1H05 ', 65],
  ])('%s → %i minutes', (input, minutes) => expect(parseDuration(input)).toBe(minutes))

  it.each(['', 'h', '1h300', 'soon', '-5'])('%s is not a duration', (input) => expect(parseDuration(input)).toBeNull())
})
