import { describe, expect, it } from 'vitest'
import { arcs, chains, depths, layout, lineage, tree } from './mr-deps'
import { mrKey } from './mr-ref'
import type { MergeRequest, Snapshot } from './types'
import type { MrDep } from './bindings/MrDep'

const mr = (iid: number, over: Partial<MergeRequest> = {}): MergeRequest => ({ iid, repo: null, state: 'opened', created_at: `2026-11-0${iid % 9}T10:00:00Z`, labels: [], ...over }) as MergeRequest
const dep = (iid: number, on: number, via: MrDep['via'] = 'stacked', done = false): MrDep => ({ repo: null, iid, on_repo: null, on_iid: on, via, done })

// 410 ← 412 ← 418, 412 ← 88 (declared); 405 (merged) ← 409 ← 91 (tickets); 401 alone.
const all = [mr(410), mr(412), mr(418), mr(88), mr(405, { state: 'merged' }), mr(409), mr(91), mr(401)]
const snap = { merge_requests: all, mr_deps: [dep(412, 410), dep(418, 412), dep(88, 412, 'declared'), dep(409, 405, 'declared', true), dep(91, 409, 'ticket')] } as unknown as Snapshot
const open = all.filter((m) => m.state === 'opened')
const keys = (ms: MergeRequest[]) => ms.map((m) => m.iid)

describe('chains', () => {
  it('links the MRs, the longest chain first, the bases first, the lone ones apart', () => {
    const got = chains(snap, open, { tickets: true, merged: true })
    expect(got.chains.map((c) => keys(c.mrs))).toEqual([[410, 412, 418, 88], [405, 409, 91]])
    expect(keys(got.alone)).toEqual([401])
  })

  it('leaves out the merged MRs and the dependencies via tickets when asked', () => {
    const got = chains(snap, open, { tickets: false, merged: false })
    expect(got.chains.map((c) => keys(c.mrs))).toEqual([[410, 412, 418, 88]])
    expect(keys(got.alone)).toEqual([409, 91, 401])
  })
})

describe('the views of a chain', () => {
  const [first] = chains(snap, open, { tickets: true, merged: true }).chains
  it('a column per depth, a band per chain', () => {
    expect(Object.fromEntries(depths(first))).toEqual({ '!410': 0, '!412': 1, '!88': 2, '!418': 2 })
    const { placed, cols, rows } = layout(chains(snap, open, { tickets: true, merged: true }).chains)
    expect(cols).toBe(3)
    expect(rows).toBe(3)
    expect(placed.map((p) => [p.mr.iid, p.col, p.row])).toEqual([[410, 0, 0], [412, 1, 0], [418, 2, 0], [88, 2, 1], [405, 0, 2], [409, 1, 2], [91, 2, 2]])
  })

  it('the tree indents each MR under the one it waits for', () => {
    expect(tree(first).map((r) => [r.mr.iid, r.depth])).toEqual([[410, 0], [412, 1], [418, 2], [88, 2]])
  })

  it('lights up its whole lineage', () => {
    expect([...lineage(snap.mr_deps, '!88')].sort()).toEqual(['!410', '!412', '!88'])
  })

  it('survives a cycle', () => {
    const loop = { mrs: [mr(1), mr(2)], deps: [dep(1, 2, 'declared'), dep(2, 1, 'declared')] }
    expect(tree(loop).map((r) => r.mr.iid).sort()).toEqual([1, 2])
    expect(depths(loop).size).toBe(2)
  })
})

describe('arcs', () => {
  it('backwards when an MR comes before the one it waits for; tickets and merged left out', () => {
    const queued = [mr(410), mr(418), mr(412), mr(91), mr(409)]
    expect(arcs(snap, queued).map((a) => [mrKey(a.dep), a.from, a.to, a.back])).toEqual([['!412', 2, 0, false], ['!418', 1, 2, true]])
  })
})
