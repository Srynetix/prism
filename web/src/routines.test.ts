import { describe, expect, it } from 'vitest'
import { cronWords, everyWords, reviewPile, routineEvents, toReview } from './routines'
import type { RoutineRun } from './bindings/RoutineRun'
import type { Snapshot } from './types'

const run = (o: Partial<RoutineRun> = {}): RoutineRun => ({
  id: 'r', routine: 'Nightly', repo: 'api', agent: null, due: '2026-11-05T02:00:00Z', started: '2026-11-05T02:00:00Z', ended: null, state: 'ready', deliver: 'review',
  caught_up: false, manual: false, note: null, said: null, files: 3, branch: null, archived: false, cost: null, ...o,
})

describe('cronWords', () => {
  it('writes the usual crons in words', () => {
    expect(cronWords('0 3 * * 1-5')).toBe('Monday to Friday at 03:00')
    expect(cronWords('30 8 * * *')).toBe('Every day at 08:30')
    expect(cronWords('*/15 * * * *')).toBe('Every 15 minutes')
    expect(cronWords('0 12 * * 0,6')).toBe('Weekends at 12:00')
    expect(cronWords('0 9 1 * *')).toBe('On day 1 of the month at 09:00')
  })
  it('leaves what reads too oddly as it is', () => {
    expect(cronWords('0 3 * 1 *')).toBe('0 3 * 1 *')
    expect(cronWords('nope')).toBe('nope')
    expect(cronWords('0-59/7 1-23/5 * * *')).toBe('0-59/7 1-23/5 * * *')
  })
  it('says a pace', () => {
    expect(everyWords('30m')).toBe('every 30 minutes')
    expect(everyWords('1h')).toBe('every hour')
    expect(everyWords('2d')).toBe('every 2 days')
  })
})

describe('To review', () => {
  it('holds the blocked and failed whatever their delivery, the ready delivered there', () => {
    expect(toReview(run())).toBe(true)
    expect(toReview(run({ deliver: 'digest' }))).toBe(false)
    expect(toReview(run({ deliver: 'digest', state: 'blocked' }))).toBe(true)
    expect(toReview(run({ state: 'too_long' }))).toBe(true)
    expect(toReview(run({ state: 'nothing' }))).toBe(false)
    expect(toReview(run({ archived: true, state: 'failed' }))).toBe(false)
  })
  it('puts the blocked first, then the most recent', () => {
    const s = { routines: { routines: [], keep_running: true, runs: [run({ id: 'old', due: '2026-11-04T02:00:00Z' }), run({ id: 'new' }), run({ id: 'asks', state: 'blocked', due: '2026-11-03T02:00:00Z' })] } } as unknown as Snapshot
    expect(reviewPile(s).map((r) => r.id)).toEqual(['asks', 'new', 'old'])
  })
  it('gives the Digest the ready runs delivered there, for a day', () => {
    const now = new Date().toISOString()
    const s = { routines: { routines: [], keep_running: true, runs: [run({ id: 'd', deliver: 'digest', ended: now, said: '5 points' }), run({ id: 'v', ended: now }), run({ id: 'old', deliver: 'digest', ended: '2020-01-01T00:00:00Z' })] } } as unknown as Snapshot
    expect(routineEvents(s)).toEqual([{ key: 'routine:d', title: 'Nightly ready', body: '5 points', kind: 'routine' }])
  })
})
