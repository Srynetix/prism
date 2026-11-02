import { describe, expect, it } from 'vitest'
import { addMonths, byTicket, hours, monthWeeks, targetOf, tone, weekOf } from './timesheet'
import type { TimeEntry } from './types'

const entry = (start: string, minutes: number, ticket: string | null, id = start): TimeEntry => ({
  id,
  ticket,
  task_name: ticket ? `Task ${ticket}` : 'Meetings',
  task_url: null,
  start: new Date(start).toISOString(),
  duration_ms: minutes * 60000,
  description: '',
})

describe('timesheet', () => {
  it('formats hours', () => {
    expect(hours(45 * 60000)).toBe('45m')
    expect(hours(65 * 60000)).toBe('1h05')
  })

  it('finds the Monday → Sunday week of a day', () => {
    expect(weekOf('2026-10-14')).toEqual(['2026-10-12', '2026-10-13', '2026-10-14', '2026-10-15', '2026-10-16', '2026-10-17', '2026-10-18'])
    expect(weekOf('2026-10-18')[0]).toBe('2026-10-12')
    expect(weekOf('2026-11-01')[0]).toBe('2026-10-26')
  })

  it('moves by months, across years', () => {
    expect(addMonths('2026-10-31', 1)).toBe('2026-11-01')
    expect(addMonths('2026-01-15', -1)).toBe('2025-12-01')
  })

  it('covers a month with whole weeks', () => {
    const weeks = monthWeeks('2026-10-14')
    expect(weeks[0][0]).toBe('2026-09-28')
    expect(weeks.at(-1)?.at(-1)).toBe('2026-11-01')
    expect(weeks).toHaveLength(5)
  })

  it('counts the target on working days only', () => {
    expect(targetOf(weekOf('2026-10-14'), 8)).toBe(40)
  })

  it('groups entries by ticket over the given days, most logged first', () => {
    const rows = byTicket(
      [entry('2026-10-12T09:00', 60, 'ACME-1', 'a'), entry('2026-10-12T10:00', 30, 'ACME-1', 'b'), entry('2026-10-13T09:00', 120, null), entry('2026-10-05T09:00', 600, 'ACME-2')],
      weekOf('2026-10-14'),
    )
    expect(rows.map((r) => [r.key, r.total / 60000])).toEqual([
      ['Meetings', 120],
      ['ACME-1', 90],
    ])
    expect(rows[1].days.get('2026-10-12')).toBe(90 * 60000)
  })

  it('tones a day', () => {
    expect(tone('2026-10-12', 8, 8, '2026-10-14')).toBe('ok')
    expect(tone('2026-10-12', 4, 8, '2026-10-14')).toBe('warn')
    expect(tone('2026-10-11', 0, 8, '2026-10-14')).toBe('')
    expect(tone('2026-10-14', 4, 8, '2026-10-14')).toBe('')
  })
})
