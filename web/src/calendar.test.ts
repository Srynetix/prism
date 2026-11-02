import { describe, expect, it } from 'vitest'
import { addDays, monthGrid, weekStart } from './calendar'

describe('monthGrid', () => {
  it('starts on the week’s first day and ends with the month’s last week', () => {
    // October 2026: Thursday 1st, Saturday 31st.
    const weeks = monthGrid(2026, 9, 1)
    expect(weeks[0][0]).toEqual({ key: '2026-09-28', day: 28, inMonth: false })
    expect(weeks[0][3]).toEqual({ key: '2026-10-01', day: 1, inMonth: true })
    expect(weeks.at(-1)?.at(-1)).toEqual({ key: '2026-11-01', day: 1, inMonth: false })
    expect(weeks.every((w) => w.length === 7)).toBe(true)
    expect(weeks.flat().filter((d) => d.inMonth)).toHaveLength(31)
  })

  it('follows a week starting on Sunday', () => {
    const weeks = monthGrid(2026, 9, 7)
    expect(weeks[0][0].key).toBe('2026-09-27')
    expect(weeks[0][4].key).toBe('2026-10-01')
    expect(weeks.at(-1)?.at(-1)?.key).toBe('2026-10-31')
  })

  it('adds no empty week when the month starts on the week’s first day', () => {
    // February 2027: Monday 1st, Sunday 28th — four weeks exactly.
    const weeks = monthGrid(2027, 1, 1)
    expect(weeks).toHaveLength(4)
    expect(weeks[0][0].key).toBe('2027-02-01')
  })
})

describe('addDays', () => {
  it('crosses months and years', () => {
    expect(addDays('2026-10-31', 1)).toBe('2026-11-01')
    expect(addDays('2027-01-01', -1)).toBe('2026-12-31')
    expect(addDays('2026-10-21', -7)).toBe('2026-10-14')
  })
})

describe('weekStart', () => {
  it('is Monday in France, Sunday in the United States', () => {
    expect(weekStart('fr-FR')).toBe(1)
    expect(weekStart('en-US')).toBe(7)
  })
})
