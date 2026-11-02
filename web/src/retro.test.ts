import { describe, expect, it } from 'vitest'
import type { Trace, TrailState } from './blocks/trail/trail'
import type { Snapshot } from './types'

// The trail's module follows the page's address (filter.svelte.ts): none under Node.
const page = globalThis as { location?: unknown; addEventListener?: unknown }
page.location ??= { hash: '' }
page.addEventListener ??= () => {}
const { dayFigures, hours, longestFree, median, retro } = await import('./retro')
const { live } = await import('./live.svelte')
const { pointsMarkdown } = await import('./retro-points.svelte')

const t = (hm: string, day = '2026-10-06') => Date.parse(`${day}T${hm}:00`)
const tr = (hm: string, kind: Trace['kind'], ticket?: string, minutes?: number): Trace => ({ id: `${kind}${hm}`, at: '', t: t(hm), kind, title: '', ticket, minutes, end: minutes ? t(hm) + minutes * 60_000 : undefined })

describe('figures', () => {
  it('formats and averages', () => {
    expect(hours(45)).toBe('45 min')
    expect(hours(120)).toBe('2 h')
    expect(hours(200)).toBe('3 h 20')
    expect(median([5, 1, 3])).toBe(3)
    expect(median([])).toBe(0)
  })
  it('finds the longest stretch without a meeting', () => {
    expect(longestFree(t('09:00'), t('17:00'), [[t('10:00'), t('11:00')], [t('14:00'), t('14:30')]])).toBe(180)
    expect(longestFree(t('09:00'), t('10:00'), [])).toBe(60)
  })
  it('counts switches between subjects, not traces', () => {
    const d = dayFigures([tr('09:00', 'commit', 'A'), tr('09:30', 'commit', 'A'), tr('10:00', 'meeting', undefined, 60), tr('11:30', 'commit', 'B'), tr('12:00', 'commit', 'A')], '2026-10-06')!
    expect(d.switches).toBe(2)
    expect(d.meetings).toBe(60)
    expect(d.focus).toBe(60)
  })
})

describe('retro', () => {
  it('splits my tickets into delivered and carried, with why', () => {
    const me = { id: 'u1', name: 'Me' }
    const ticket = (id: string, status: string, over = {}) => ({ id, task_id: id, name: id, status, closed: false, assignees: [me], waiting_on: [], blocking: [], time_estimate: null, ...over })
    const s = {
      sprint: { name: 'S42', start: '2026-09-29T00:00:00Z', end: '2026-10-10T00:00:00Z', tickets: [ticket('A', 'done'), ticket('B', 'in progress', { waiting_on: [{ id: 'X-1', closed: false }] }), ticket('C', 'to do', { assignees: [] })] },
      tracker_me: me,
      me: 'me',
      merge_requests: [],
      agents: [],
      time: { entries: [] },
      conventions: { done_statuses: ['done'], in_progress_statuses: ['in progress'], before_dev_statuses: ['to do'] },
    } as unknown as Snapshot
    // The statuses' meaning comes from the snapshot's conventions.
    live.snapshot = s
    const r = retro(s, { events: [] } as unknown as TrailState, [], [], Date.parse('2026-10-08T12:00:00Z'))!
    expect(r.mine.map((m) => [m.t.id, m.done, m.why])).toEqual([['A', true, null], ['B', false, 'blocked by X-1']])
    expect(r.team).toEqual({ done: 1, total: 3 })
    expect(r.observations.some((o) => o.text.includes('B carried: blocked by X-1'))).toBe(true)
  })
})

describe('pointsMarkdown', () => {
  it('groups the points by kind, and says nothing without any', () => {
    const p = (text: string, kind: 'keep' | 'change' | 'idea') => ({ id: text, text, kind, created_at: '2026-10-15T10:00:00Z' })
    expect(pointsMarkdown([])).toEqual([])
    expect(pointsMarkdown([p('Pairing on reviews', 'keep'), p('Too many meetings', 'change'), p('A demo every Friday', 'idea'), p('Async standup', 'keep')])).toEqual([
      '', '### My points', '**To keep**', '- Pairing on reviews', '- Async standup', '**To change**', '- Too many meetings', '**Idea**', '- A demo every Friday',
    ])
  })
})
