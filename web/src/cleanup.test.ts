import { describe, expect, it } from 'vitest'
import { countsOver, daysCounted, differences, features, mostUsed, neverUsed, pruned, usesOf, type Feature, type UsageDoc } from './cleanup'

const doc = (days: UsageDoc['days'], since = '2026-09-01', kept: string[] = []): UsageDoc => ({ since, days, kept })

describe('usage counts', () => {
  it('cover the days since the first one counted', () => {
    expect(daysCounted(doc({}, '2026-10-30'), '2026-10-30')).toBe(1)
    expect(daysCounted(doc({}, '2026-10-01'), '2026-10-30')).toBe(30)
  })
  it('sum the last 30 days only', () => {
    const d = doc({ '2026-10-30': { 'block:mrs': 2 }, '2026-10-01': { 'block:mrs': 1, 'palette:mr': 4 }, '2026-09-30': { 'block:mrs': 9 } })
    expect(countsOver(d, '2026-10-30')).toEqual({ 'block:mrs': 3, 'palette:mr': 4 })
  })
  it('drop the days beyond what is kept', () => {
    expect(Object.keys(pruned({ '2026-10-30': {}, '2026-09-01': {}, '2026-08-31': {} }, '2026-10-30'))).toEqual(['2026-10-30', '2026-09-01'])
  })
})

describe('on, never used', () => {
  const f = (id: string, counts: string[], when?: () => boolean): Feature => ({ id, label: id, area: '', path: [id, 'enabled'], counts, when })
  const list = [f('forge', ['block:mrs', 'palette:mr']), f('agents', ['action:', 'block:sessions']), f('mail', ['block:mail']), f('clickup', ['block:sprint'], () => false)]
  const on = (p: string[]) => p[0] !== 'mail'
  it('counts a feature by its ids and prefixes', () => {
    expect(usesOf(list[1], { 'action:review-fix': 3, 'action:implement': 1, 'block:mrs': 7 })).toBe(4)
  })
  it('lists what is on, unused and not kept', () => {
    expect(neverUsed(list, { 'block:mrs': 1 }, on, []).map((x) => x.id)).toEqual(['agents'])
    expect(neverUsed(list, {}, on, ['agents']).map((x) => x.id)).toEqual(['forge'])
  })
  it('names a switch for each feature', () => {
    for (const x of features(() => true)) expect(x.path.at(-1) === 'enabled' || x.path.length === 2).toBe(true)
  })
})

describe('most used', () => {
  it('most first, then by name', () => {
    expect(mostUsed({ a: 1, b: 5, c: 5, d: 2 }, 3)).toEqual([['b', 5], ['c', 5], ['d', 2]])
  })
})

describe('different from default', () => {
  const defaults = { alerts: { your_turn_minutes: 0, digest_times: ['09:00'], quiet_hours: null }, appearance: { theme_motion: true, theme: 'dark' }, console: { aliases: {} }, views: [], repos: [] }
  it('lists the keys set to something else, down the tables', () => {
    const values = { alerts: { your_turn_minutes: 10, digest_times: ['09:00'], quiet_hours: '19:00-08:00' }, appearance: { theme_motion: false, theme: 'dark' } }
    expect(differences(values, defaults)).toEqual([
      { path: ['alerts', 'your_turn_minutes'], value: 10, def: 0 },
      { path: ['alerts', 'quiet_hours'], value: '19:00-08:00', def: null },
      { path: ['appearance', 'theme_motion'], value: false, def: true },
    ])
  })
  it('leaves out your data, unknown keys and lists of tables', () => {
    const values = { views: [{ name: 'X' }], repos: [{ name: 'app' }], console: { aliases: { ci: 'goto ci' } }, nope: { a: 1 }, alerts: { rules: [{ a: 1 }] } }
    expect(differences(values, defaults)).toEqual([])
  })
})
