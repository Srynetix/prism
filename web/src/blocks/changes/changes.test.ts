import { describe, expect, it } from 'vitest'
import { around, byHour, closedIn, counts, markdown, rangeOf, rows, sentence, visible, type ChangeEvent } from './changes'

const T0 = Date.parse('2026-11-05T10:00:00Z')
const MIN = 60_000

function ev(id: string, minutesAgo: number, over: Partial<ChangeEvent> = {}): ChangeEvent {
  return { id, at: new Date(T0 - minutesAgo * MIN).toISOString(), source: 'pods', kind: 'restart', title: 'orders-api', severity: 'warn', workloads: [], images: [], env: 'staging', app: 'orders-api', ...over } as ChangeEvent
}

describe('changes', () => {
  it('ranges end now; since yesterday starts at midnight', () => {
    expect(rangeOf('1h', T0)).toEqual({ from: T0 - 60 * MIN, to: T0 })
    const y = new Date(rangeOf('since_yesterday', T0).from)
    expect([y.getHours(), y.getMinutes()]).toEqual([0, 0])
    expect(T0 - y.getTime()).toBeGreaterThan(24 * 60 * MIN)
    expect(rangeOf('custom', T0, { from: 1, to: 2 })).toEqual({ from: 1, to: 2 })
  })

  it('filters by env (none: every env), range, source and search', () => {
    const events = [ev('a', 5), ev('b', 5, { env: 'prod' }), ev('c', 5, { env: undefined, source: 'mrs', kind: 'merged', title: 'Fix the cart', app: 'shop' }), ev('d', 300)]
    const f = { env: 'staging', range: { from: T0 - 60 * MIN, to: T0 }, hidden: new Set<string>(), query: '' }
    expect(visible(events, f).map((e) => e.id)).toEqual(['a', 'c'])
    expect(visible(events, { ...f, hidden: new Set(['mrs']) }).map((e) => e.id)).toEqual(['a'])
    expect(visible(events, { ...f, query: 'cart' }).map((e) => e.id)).toEqual(['c'])
    expect(counts(events, f, ['argocd', 'mrs', 'pods'])).toEqual([{ source: 'argocd', n: 0 }, { source: 'mrs', n: 1 }, { source: 'pods', n: 1 }])
  })

  it('folds repeats on the same app, close together', () => {
    const r = rows([ev('r1', 10), ev('r2', 8), ev('r3', 6), ev('other', 7, { app: 'catalog-api' }), ev('late', 100)])
    expect(r.map((x) => [x.key, x.items.length])).toEqual([['r1', 3], ['other', 1], ['late', 1]])
    expect(sentence(r[0]).what).toContain('3')
    // A sync never folds.
    expect(rows([ev('s1', 3, { kind: 'sync' }), ev('s2', 2, { kind: 'sync' })])).toHaveLength(2)
  })

  it('groups by hour, newest first', () => {
    const g = byHour(rows([ev('a', 5), ev('b', 125)]))
    expect(g).toHaveLength(2)
    expect(g[0].rows[0].key).toBe('a')
  })

  it('centers on the first crash of a run, the linked change highlighted', () => {
    const events = [ev('sync', 60, { kind: 'sync', source: 'argocd' }), ev('c1', 50, { kind: 'crash_loop', related: 'sync' }), ev('c2', 30, { kind: 'oom_killed' }), ev('c3', 5, { kind: 'restart' }), ev('old', 300, { kind: 'restart' })]
    const a = around(events, 'staging', 'orders-api', T0)
    expect(a.crash).toBe('c1')
    expect(a.highlight).toBe('sync')
    expect(a.range).toEqual({ from: T0 - 50 * MIN - 120 * MIN, to: T0 })
    expect(around([], 'staging', 'orders-api', T0).range.from).toBe(T0 - 120 * MIN)
  })

  it('clips the closed ranges', () => {
    expect(closedIn([{ from: new Date(T0 - 90 * MIN).toISOString(), to: new Date(T0 - 30 * MIN).toISOString() }], { from: T0 - 60 * MIN, to: T0 })).toEqual([{ from: T0 - 60 * MIN, to: T0 - 30 * MIN }])
  })

  it('copies as Markdown, oldest first, the link named', () => {
    const md = markdown(rows([ev('sync', 20, { kind: 'sync', source: 'argocd', title: 'orders-api-staging', revision: '4f2a9c1' }), ev('crash', 10, { kind: 'crash_loop', related: 'sync', seen: true })]), 'staging', { from: T0 - 60 * MIN, to: T0 })
    const lines = md.split('\n')
    expect(lines[0]).toContain('staging')
    expect(lines[2]).toContain('4f2a9c1')
    expect(lines[3]).toContain('CrashLoopBackOff')
    expect(lines[3]).toContain('orders-api-staging')
  })
})
