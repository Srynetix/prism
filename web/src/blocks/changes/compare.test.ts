import { describe, expect, it } from 'vitest'
import type { ChangeEvent } from './changes'
import { appOf, compare, crashAfter, defaultPair, rank, sameApp, span } from './compare'

const T0 = Date.parse('2026-11-05T10:00:00Z')
const H = 3600_000

function ev(id: string, hoursAgo: number, over: Partial<ChangeEvent> = {}): ChangeEvent {
  return { id, at: new Date(T0 - hoursAgo * H).toISOString(), source: 'argocd', kind: 'sync', title: id, severity: 'info', workloads: [], images: [], env: 'staging', app: 'api', ...over } as ChangeEvent
}

describe('compare', () => {
  it('knows an application under its environment’s name', () => {
    expect(sameApp(ev('a', 1, { app: 'orders-api-staging' }), ev('b', 1, { env: 'prod', app: 'orders-api-prod' }))).toBe(true)
    expect(sameApp(ev('a', 1, { app: 'x', workloads: ['orders-api'] }), ev('b', 1, { env: 'prod', app: 'y', workloads: ['orders-api'] }))).toBe(true)
    expect(sameApp(ev('a', 1, { app: 'orders' }), ev('b', 1, { env: 'prod', app: 'billing' }))).toBe(false)
    expect(appOf(ev('a', 1, { env: 'prod', app: 'orders-api-prod' }))).toBe('orders-api')
  })

  it('links by revision, else by the same app’s image, each arrival once', () => {
    const events = [
      ev('s1', 50, { revision: 'a1c39e0f' }),
      ev('s1-again', 49, { revision: 'a1c39e0' }),
      ev('p1', 2 * 24, { env: 'prod', revision: 'a1c39e0' }),
      ev('p1-again', 20, { env: 'prod', revision: 'a1c39e0' }),
      ev('s2', 30, { app: 'web-staging', revision: '11111111', images: ['web-staging:3.4.1'] }),
      ev('r2', 30, { app: 'web', source: 'pods', kind: 'rollout', images: ['web:3.4.1'] }),
      ev('p2', 10, { env: 'prod', app: 'web-prod', revision: '22222222', images: ['web-prod:3.4.1'] }),
      ev('rp2', 10, { env: 'prod', app: 'web', source: 'pods', kind: 'rollout', images: ['web:3.4.1'] }),
    ]
    const { matches } = compare(events, 'staging', 'prod')
    expect(matches.map((m) => [m.a.id, m.b.id, m.basis, m.label])).toEqual([
      ['s1', 'p1', 'revision', 'a1c39e0f'],
      ['s2', 'p2', 'image', 'web-prod:3.4.1'],
    ])
    expect(matches[0].delay).toBe(2 * H)
  })

  it('links merged MRs by their iid', () => {
    const mr = { repo: 'shop', iid: 412, title: 'Pagination', url: null }
    const events = [ev('m1', 30, { source: 'mrs', kind: 'merged', mr }), ev('m2', 3, { env: 'prod', source: 'mrs', kind: 'merged', mr })]
    expect(compare(events, 'staging', 'prod').matches.map((m) => m.label)).toEqual(['!412'])
  })

  it('a crash on A after the change, before its next deployment', () => {
    const sync = ev('s', 10, { revision: '5d20aa13' })
    const crash = ev('c', 9.9, { source: 'pods', kind: 'crash_loop', severity: 'ko', app: 'api' })
    const next = ev('s2', 5, { revision: '6e31bb24' })
    const late = ev('c2', 4, { source: 'pods', kind: 'oom_killed', severity: 'ko' })
    const events = [sync, crash, next, late, ev('p', 2, { env: 'prod', revision: '5d20aa13' })]
    expect(crashAfter(sync, events)?.id).toBe('c')
    expect(crashAfter(next, events)?.id).toBe('c2')
    expect(compare(events, 'staging', 'prod').matches[0].crash?.id).toBe('c')
  })

  it('waits: per application B runs, what A deployed after the last change B got', () => {
    const events = [
      ev('old', 40, { revision: 'aaaaaaa1' }),
      ev('got', 30, { revision: 'aaaaaaa2' }),
      ev('new1', 8, { revision: 'aaaaaaa3' }),
      ev('new2', 4, { revision: 'aaaaaaa4' }),
      ev('web', 6, { app: 'web', revision: 'bbbbbbb1' }),
      ev('roll', 6, { app: 'web', source: 'pods', kind: 'rollout', images: ['web:1'] }),
      ev('p', 20, { env: 'prod', revision: 'aaaaaaa2' }),
    ]
    const { waiting } = compare(events, 'staging', 'prod')
    // web never ran on prod: not waiting for it.
    expect(waiting.map((w) => [w.e.id, w.earlier.map((e) => e.id)])).toEqual([['new2', ['new1']]])
    expect(compare(events, 'prod', 'staging').waiting).toEqual([])
  })

  it('links by the application’s own image only, its tag', () => {
    const s = ev('s', 10, { revision: '11111111', images: ['api:2.0', 'istio-proxy:1.20'] })
    const p = ev('p', 5, { env: 'prod', revision: '22222222', images: ['api:1.9', 'istio-proxy:1.20'] })
    expect(compare([s, p], 'staging', 'prod').matches).toEqual([])
    expect(compare([s, p], 'staging', 'prod').waiting.map((w) => w.e.id)).toEqual(['s'])
    const r = ev('r', 10, { revision: '33333333', images: ['registry:5000/team/api:2.1'] })
    const q = ev('q', 5, { env: 'prod', revision: '44444444', images: ['registry:5000/team/api-prod:2.1'] })
    expect(compare([r, q], 'staging', 'prod').matches.map((m) => m.label)).toEqual(['registry:5000/team/api-prod:2.1'])
    const u = ev('u', 10, { revision: '55555555', images: ['registry:5000/api'] })
    const v = ev('v', 5, { env: 'prod', revision: '66666666', images: ['registry:5000/api'] })
    expect(compare([u, v], 'staging', 'prod').matches).toEqual([])
  })

  it('a crash a day after, or after another image rolled out, is not the change’s', () => {
    const sync = ev('s', 100, { revision: '5d20aa13', images: ['api:1'] })
    const old = ev('c', 70, { source: 'pods', kind: 'crash_loop', severity: 'ko' })
    expect(crashAfter(sync, [sync, old])).toBeNull()
    const roll = ev('r', 99, { source: 'pods', kind: 'rollout', images: ['api:2'] })
    const crash = ev('c2', 98, { source: 'pods', kind: 'oom_killed', severity: 'ko' })
    expect(crashAfter(sync, [sync, roll, crash])).toBeNull()
    const own = ev('r1', 99.95, { source: 'pods', kind: 'rollout', images: ['api:1'] })
    expect(crashAfter(sync, [sync, own, crash])?.id).toBe('c2')
  })

  it('orders the pair by the roles', () => {
    expect(['prod', 'preprod', 'staging', 'dev'].sort((a, b) => rank(a) - rank(b))).toEqual(['dev', 'staging', 'preprod', 'prod'])
    expect(defaultPair(['prod', 'preprod', 'staging'], [], null)).toEqual(['staging', 'preprod'])
    expect(defaultPair(['prod', 'preprod', 'staging'], [], 'preprod')).toEqual(['preprod', 'prod'])
    expect(defaultPair(['prod', 'preprod', 'staging'], [], 'prod')).toEqual(['preprod', 'prod'])
    expect(defaultPair(['prod', 'staging'], ['staging', 'prod'], 'prod')).toEqual(['staging', 'prod'])
    expect(defaultPair(['prod'], [], null)).toBeNull()
    expect(defaultPair(['prod', 'staging'], ['staging', 'production'], null)).toEqual(['staging', 'prod'])
  })

  it('says a delay shortly', () => {
    expect(span(45 * 60_000)).toBe('45 min')
    expect(span(20 * H)).toBe('20 h')
    expect(span(52 * H)).toBe('2 d 4 h')
    expect(span(-48 * H)).toBe('2 d')
  })
})
