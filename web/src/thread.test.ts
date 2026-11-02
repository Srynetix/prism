import { describe, expect, it } from 'vitest'
import { envRank, parseThreadAttr, resolveThread, threadAttr } from './thread.svelte'
import type { Snapshot } from './types'

const mr = { iid: 12, repo: null, project: null, title: 'Coupons', url: '', state: 'merged', source_branch: 'feat/SHOP-412-coupons', ticket: 'SHOP-412', updated_at: '2026-10-10T10:00:00Z', merged_at: '2026-10-10T10:00:00Z', head_pipeline: null }
const snap = (over: Partial<Record<keyof Snapshot, unknown>> = {}) =>
  ({
    merge_requests: [mr],
    worktrees: [{ repo: 'shop', path: '/w/shop.worktrees/coupons', branch: 'feat/SHOP-412-coupons', ticket: 'SHOP-412', is_main: false, prunable: false, ahead: 0, unpushed: 0 }],
    agents: [{ id: 'a1', title: 'Coupons', state: 'running', status: 'running', ticket: null, branch: 'feat/SHOP-412-coupons', mr: null, usage: { cost: 1.5 } }],
    environments: {
      envs: [
        { env: 'staging', workloads: [{ updated_at: '2026-10-10T12:00:00Z' }] },
        { env: 'staging', workloads: [{ updated_at: '2026-10-09T12:00:00Z' }] },
        { env: 'prod', workloads: [{ updated_at: '2026-10-01T12:00:00Z' }] },
      ],
    },
    time: { entries: [{ ticket: 'SHOP-412', duration_ms: 3_600_000 }, { ticket: 'SHOP-1', duration_ms: 60_000 }] },
    agenda: null,
    sprint: null,
    next_sprint: null,
    pickup: { boards: [] },
    outside_sprint: { tickets: [], list_statuses: {} },
    ...over,
  }) as unknown as Snapshot

describe('resolveThread', () => {
  it('joins an MR, its worktree, agent and environments from the ticket id', () => {
    const th = resolveThread({ ticket: 'SHOP-412' }, snap(), Date.parse('2026-10-14T10:00:00Z'), false)!
    expect(th.mr?.iid).toBe(12)
    expect(th.worktree?.path).toBe('/w/shop.worktrees/coupons')
    expect(th.agents.map((a) => a.id)).toEqual(['a1'])
    expect(th.envs).toEqual([
      { env: 'staging', at: '2026-10-10T12:00:00Z', since: true },
      { env: 'prod', at: '2026-10-01T12:00:00Z', since: false },
    ])
    // Merged four days ago, in staging and not in production: it waits there.
    expect(th.missing.has('env:prod')).toBe(true)
    expect(th.logged).toBe(3_600_000)
    expect(th.cost).toBe(1.5)
  })

  it('finds the same thread from the agent, and flags commits without an MR', () => {
    const s = snap({ merge_requests: [], worktrees: [{ repo: 'shop', path: '/w/x', branch: 'feat/SHOP-412-coupons', ticket: 'SHOP-412', is_main: false, prunable: false, ahead: 2, unpushed: 0 }] })
    const th = resolveThread({ agent: 'a1' }, s, Date.now(), false)!
    expect(th.branch).toBe('feat/SHOP-412-coupons')
    expect(th.missing.has('mr')).toBe(true)
  })

  it('finds an MR that names several tickets from its second one', () => {
    const shared = { ...mr, iid: 13, source_branch: 'fix/shared', ticket: 'SHOP-412', tickets: ['SHOP-412', 'SHOP-415'] }
    const th = resolveThread({ ticket: 'SHOP-415' }, snap({ merge_requests: [shared], worktrees: [], agents: [] }), Date.now(), false)!
    expect(th.mr?.iid).toBe(13)
  })

  it('is nothing when nothing touches it', () => {
    expect(resolveThread({ ticket: 'SHOP-999' }, snap(), Date.now(), false)).toBeNull()
  })
})

describe('threadAttr', () => {
  it('round-trips through data-thread', () => {
    for (const ref of [{ ticket: 'SHOP-1', repo: 'shop' }, { mr: { repo: null, iid: 7 } }, { agent: 'local_a-b:c' }, { branch: 'feat/x', repo: 'api' }]) {
      expect(parseThreadAttr(threadAttr(ref))).toEqual(ref)
    }
  })
})

describe('envRank', () => {
  it('orders environments toward production', () => {
    expect(['prod', 'preprod', 'staging', 'dev'].sort((a, b) => envRank(a) - envRank(b))).toEqual(['dev', 'staging', 'preprod', 'prod'])
  })
})
