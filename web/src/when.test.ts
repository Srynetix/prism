import { describe, expect, it, vi } from 'vitest'
import { parseWhen, whenLabel } from './when'

// The format of `[appearance] locale`, whatever the machine's.
vi.mock('./locale', () => ({ locale: () => 'en-GB' }))

// Wednesday 14 October 2026, 10:00 local time.
const now = new Date(2026, 9, 14, 10, 0)
const at = (y: number, mo: number, d: number, h: number, mi = 0) => new Date(y, mo - 1, d, h, mi)

describe('parseWhen', () => {
  it('reads a delay', () => {
    expect(parseWhen('+20m', now)).toEqual(at(2026, 10, 14, 10, 20))
    expect(parseWhen('20 min', now)).toEqual(at(2026, 10, 14, 10, 20))
    expect(parseWhen('in 2h', now)).toEqual(at(2026, 10, 14, 12))
    expect(parseWhen('dans 1h30', now)).toEqual(at(2026, 10, 14, 11, 30))
    expect(parseWhen('+1d', now)).toEqual(at(2026, 10, 15, 10))
  })

  it('reads hours alone as a time, not a delay', () => {
    expect(parseWhen('14h', now)).toEqual(at(2026, 10, 14, 14))
    expect(parseWhen('14h30', now)).toEqual(at(2026, 10, 14, 14, 30))
    expect(parseWhen('2pm', now)).toEqual(at(2026, 10, 14, 14))
    expect(parseWhen('9:15am', now)).toEqual(at(2026, 10, 15, 9, 15))
  })

  it('moves a time already past today to tomorrow', () => {
    expect(parseWhen('9', now)).toEqual(at(2026, 10, 15, 9))
  })

  it('reads a day, at 9:00 without a time', () => {
    expect(parseWhen('tomorrow', now)).toEqual(at(2026, 10, 15, 9))
    expect(parseWhen('demain 14h', now)).toEqual(at(2026, 10, 15, 14))
    expect(parseWhen('fri', now)).toEqual(at(2026, 10, 16, 9))
    // The same weekday: next week's.
    expect(parseWhen('mercredi', now)).toEqual(at(2026, 10, 21, 9))
    expect(parseWhen('20/10 à 15h', now)).toEqual(at(2026, 10, 20, 15))
    // A date already past this year: next year's.
    expect(parseWhen('1/3', now)).toEqual(at(2027, 3, 1, 9))
  })

  it('returns null when nothing is understood', () => {
    expect(parseWhen('', now)).toBeNull()
    expect(parseWhen('someday', now)).toBeNull()
    expect(parseWhen('31/2', now)).toBeNull()
    expect(parseWhen('25h', now)).toBeNull()
  })
})

describe('whenLabel', () => {
  it('names today and tomorrow', () => {
    expect(whenLabel(at(2026, 10, 14, 14, 30), now)).toBe('today 14:30')
    expect(whenLabel(at(2026, 10, 15, 9), now)).toBe('tomorrow 09:00')
  })
})
