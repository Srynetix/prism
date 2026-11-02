import { describe, expect, it } from 'vitest'
import { digest, inSpan, lastDue, minutes, nextDue, route, type DeskEvent } from './digest.svelte'
import { live } from './live.svelte'
import type { Snapshot } from './types'

const at = (hm: string, day = 14) => new Date(2026, 9, day, Number(hm.slice(0, 2)), Number(hm.slice(3)))
const ev = (key: string, kind: DeskEvent['kind'] = 'pipeline'): DeskEvent => ({ key, title: key, body: '', kind })
const settings = (over: Record<string, unknown>) => {
  live.snapshot = { alerts: { digest: true, digest_times: ['09:00', '12:30', '17:30'], digest_urgent: ['infra', 'agent'], quiet_hours: '', focus_events: 'Focus', ...over }, agenda: null } as unknown as Snapshot
}

describe('times', () => {
  it('reads HH:MM and spans over midnight', () => {
    expect(minutes('09:05')).toBe(545)
    expect(minutes('24:00')).toBeNull()
    expect(inSpan('19:00-08:00', at('23:10'))).toBe(true)
    expect(inSpan('19:00-08:00', at('07:59'))).toBe(true)
    expect(inSpan('19:00-08:00', at('08:00'))).toBe(false)
    expect(inSpan('12:00-14:00', at('13:00'))).toBe(true)
  })
  it('finds the last and next digest times', () => {
    const times = ['17:30', '09:00', '12:30']
    expect(lastDue(times, at('13:00'))).toEqual(at('12:30'))
    expect(lastDue(times, at('08:00'))).toEqual(at('17:30', 13))
    expect(nextDue(times, at('13:00'))).toEqual(at('17:30'))
    expect(nextDue(times, at('18:00'))).toEqual(at('09:00', 15))
  })
})

describe('route', () => {
  it('holds all but the urgent kinds in digest mode, and tells what resolved', () => {
    settings({})
    digest.held = []
    digest.resolved = []
    digest.sent = []
    const now = route([ev('ci'), ev('pod', 'infra')], new Set(['ci', 'pod']), at('10:00'))
    expect(now.map((e) => e.key)).toEqual(['pod'])
    expect(digest.held.map((h) => h.key)).toEqual(['ci'])
    route([], new Set(['pod']), at('10:05'))
    expect(digest.held).toEqual([])
    expect(digest.resolved.map((h) => h.key)).toEqual(['ci'])
  })
  it('lets everything through live, nothing in quiet hours', () => {
    settings({ digest: false, quiet_hours: '19:00-08:00' })
    digest.held = []
    expect(route([ev('ci')], new Set(['ci']), at('10:00')).length).toBe(1)
    expect(route([ev('pod', 'infra')], new Set(['pod']), at('22:00'))).toEqual([])
    expect(digest.held.map((h) => h.key)).toEqual(['pod'])
  })
})
