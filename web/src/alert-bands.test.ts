import { describe, expect, it } from 'vitest'
import { alertId, arrangeAlerts, bandOf, objectKey, ruleBand, type BandSettings } from './alert-bands'
import type { Alert } from './rules'

const settings: BandSettings = { band_now: ['question', 'review'], band_later: ['your-turn', 'worktree'], band_hidden: [], escalate_hours: 24 }
const alert = (over: Partial<Alert>): Alert => ({ kind: 'MR', severity: 'warn', text: 'x', ...over })
const onMr = (iid: number, over: Partial<Alert>) => alert({ open: { kind: 'mr', iid, repo: 'shop' }, ...over })

describe('ruleBand', () => {
  it('puts a rule where the settings name it, otherwise by severity', () => {
    expect(ruleBand(alert({ rule: 'question', severity: 'info' }), settings)).toBe('now')
    expect(ruleBand(alert({ rule: 'worktree', severity: 'info' }), settings)).toBe('later')
    expect(ruleBand(alert({ rule: 'mr-ci' }), settings)).toBe('today')
    expect(ruleBand(alert({ rule: 'infra', severity: 'ko' }), settings)).toBe('now')
    expect(ruleBand(alert({ rule: 'sync', severity: 'info' }), settings)).toBe('later')
  })

  it('hides the rules named hidden', () => {
    expect(ruleBand(alert({ rule: 'no-ticket' }), { ...settings, band_hidden: ['no-ticket'] })).toBeNull()
  })
})

describe('bandOf', () => {
  it('raises Today to Now past escalate_hours, never When you can', () => {
    expect(bandOf(alert({ rule: 'mr-ci' }), settings, 23 * 60)).toBe('today')
    expect(bandOf(alert({ rule: 'mr-ci' }), settings, 24 * 60)).toBe('now')
    expect(bandOf(alert({ rule: 'worktree' }), settings, 72 * 60)).toBe('later')
    expect(bandOf(alert({ rule: 'mr-ci' }), { ...settings, escalate_hours: 0 }, 72 * 60)).toBe('today')
  })
})

describe('alertId', () => {
  it('keeps an alert whose count changes, by its rule and object', () => {
    expect(alertId(onMr(3, { rule: 'open-threads', text: '!3: 2 unresolved threads' }))).toBe(alertId(onMr(3, { rule: 'open-threads', text: '!3: 3 unresolved threads' })))
    expect(objectKey(alert({ open: { kind: 'ticket', id: 'PRJ-1' } }))).toBe('ticket:PRJ-1')
  })

  it('falls back on the text without an object', () => {
    expect(alertId(alert({ rule: 'worktree', text: '2 stale' }))).not.toBe(alertId(alert({ rule: 'worktree', text: '3 stale' })))
  })
})

describe('arrangeAlerts', () => {
  const none = () => null

  it('groups an object’s alerts, the one that unblocks the rest first', () => {
    const list = [onMr(7, { rule: 'mr-ci', text: 'ci' }), onMr(7, { rule: 'no-reviewer', text: 'rev' }), onMr(7, { rule: 'open-threads', text: 'threads' }), onMr(8, { rule: 'mr-ci', text: 'other' })]
    const [today] = arrangeAlerts(list, settings, none)
    expect(today.band).toBe('today')
    expect(today.groups.map((g) => g.alerts.map((a) => a.text))).toEqual([['threads', 'ci', 'rev'], ['other']])
  })

  it('puts a group in the most urgent band of its alerts', () => {
    const list = [onMr(7, { rule: 'need-rebase', severity: 'info', text: 'rebase' }), onMr(7, { rule: 'review', severity: 'info', text: 'review' })]
    const bands = arrangeAlerts(list, settings, none)
    expect(bands.map((b) => b.band)).toEqual(['now'])
    expect(bands[0].groups[0].alerts).toHaveLength(2)
  })

  it('keeps each alert alone when grouping is off', () => {
    const list = [onMr(7, { rule: 'mr-ci', text: 'a' }), onMr(7, { rule: 'conflicts', text: 'b' })]
    expect(arrangeAlerts(list, settings, none, false)[0].groups).toHaveLength(2)
  })

  it('orders a band by severity, then the oldest first, and leaves hidden rules out', () => {
    const ages: Record<string, number> = { young: 10, old: 300 }
    const list = [alert({ rule: 'sync', text: 'young' }), alert({ rule: 'sync', text: 'old' }), alert({ rule: 'conflicts', text: 'gone' })]
    const [today] = arrangeAlerts(list, { ...settings, band_hidden: ['conflicts'] }, (a) => ages[a.text] ?? null)
    expect(today.groups.map((g) => g.alerts[0].text)).toEqual(['old', 'young'])
  })
})
