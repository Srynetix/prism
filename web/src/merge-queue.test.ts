import { describe, expect, it } from 'vitest'
import { byTarget, mergePriority, mergeQueue, priorityLevels, queueRepos } from './merge-queue'
import type { MergeRequest, RepoInfo, Snapshot } from './types'

const mr = (iid: number, over: Partial<MergeRequest> = {}): MergeRequest => ({
  iid, repo: null, project: null, title: `MR ${iid}`, url: '', state: 'opened', draft: false, source_branch: `b${iid}`, target_branch: 'main', ticket: null,
  author: 'me', author_name: 'Me', assignees: [], reviewers: [], approved_by: [], changes_requested_by: [], approvals_left: 1, pipeline: 'success',
  merge_status: 'mergeable', conflicts: false, needs_rebase: false, merge_error: null, auto_merge: false, labels: [], head_pipeline: null, open_threads: 0,
  size: null, created_at: `2026-10-${String(iid).padStart(2, '0')}T08:00:00Z`, updated_at: '2026-10-20T08:00:00Z', merged_at: null, ...over,
}) as MergeRequest

const repo = (name: string, over: Partial<RepoInfo> = {}) => ({ name, primary: false, merge_queue: false, ...over }) as RepoInfo
const snap = (merge_requests: MergeRequest[], label: string | null = 'merge-queue') =>
  ({ merge_requests, merge_queue_label: label, repositories: [repo('shop', { primary: true, merge_queue: true }), repo('helm')] }) as unknown as Snapshot

const iids = (ms: MergeRequest[]) => ms.map((m) => m.iid)

describe('mergeQueue', () => {
  it('puts the labeled MRs in the order the job takes them: the oldest first', () => {
    const q = mergeQueue(snap([mr(9, { labels: ['merge-queue'] }), mr(3, { labels: ['merge-queue'], auto_merge: true }), mr(5, { labels: ['merge-queue', 'x'] })]), 'shop')
    expect(iids(q.queued)).toEqual([3, 5, 9])
    expect(q.auto).toEqual([])
    expect(q.hasQueue).toBe(true)
  })

  it('splits the rest into auto-merge and approved without the label', () => {
    const q = mergeQueue(snap([mr(1, { auto_merge: true }), mr(2, { approved_by: ['sam'] }), mr(4), mr(6, { approved_by: ['sam'], changes_requested_by: ['jo'] })]), 'shop')
    expect(iids(q.auto)).toEqual([1])
    expect(iids(q.ready)).toEqual([2])
  })

  it('leaves drafts, merged MRs and other repositories out', () => {
    const q = mergeQueue(snap([mr(1, { draft: true, labels: ['merge-queue'] }), mr(2, { state: 'merged', labels: ['merge-queue'] }), mr(3, { repo: 'helm', labels: ['merge-queue'] })]), 'shop')
    expect(iids(q.queued)).toEqual([])
  })

  it('has no queue where the repository has none: the label means nothing there', () => {
    const q = mergeQueue(snap([mr(3, { repo: 'helm', labels: ['merge-queue'], approved_by: ['sam'] })]), 'helm')
    expect(q.hasQueue).toBe(false)
    expect(iids(q.ready)).toEqual([3])
  })

  it('falls back to the number for an MR read without its opening date', () => {
    const q = mergeQueue(snap([mr(8, { labels: ['merge-queue'], created_at: null }), mr(7, { labels: ['merge-queue'], created_at: null })]), 'shop')
    expect(iids(q.queued)).toEqual([7, 8])
  })
})

describe('queueRepos', () => {
  it('lists the repositories with a queue first, then those with an MR to show', () => {
    expect(queueRepos(snap([mr(1, { repo: 'helm', auto_merge: true })]))).toEqual(['shop', 'helm'])
    expect(queueRepos(snap([mr(1, { repo: 'helm' })], null))).toEqual([])
  })
})

describe('byTarget', () => {
  it('splits the queue by target branch, each in its own order, the main branch first', () => {
    const q = mergeQueue(snap([mr(4, { labels: ['merge-queue'], target_branch: 'develop' }), mr(2, { labels: ['merge-queue'] }), mr(3, { auto_merge: true, target_branch: 'develop' }), mr(1, { labels: ['merge-queue'], target_branch: 'develop' })]), 'shop')
    const bs = byTarget(q)
    expect(bs.map((b) => b.target)).toEqual(['main', 'develop'])
    expect(iids(bs[0].queued)).toEqual([2])
    expect(iids(bs[1].queued)).toEqual([1, 4])
    expect(iids(bs[1].auto)).toEqual([3])
  })
})

describe('merge priority', () => {
  const prio = (merge_requests: MergeRequest[], prefix: string | null = 'merge-priority::') => ({ ...snap(merge_requests), merge_priority_prefix: prefix }) as Snapshot
  const q = ['merge-queue']

  it('takes the lowest priority first, then the MRs without one, the oldest first within each', () => {
    const s = prio([mr(1, { labels: q }), mr(2, { labels: [...q, 'merge-priority::1'] }), mr(3, { labels: [...q, 'merge-priority::0'] }), mr(4, { labels: [...q, 'merge-priority::1'] }), mr(5, { labels: [...q, 'merge-priority::0'] })])
    expect(iids(mergeQueue(s, 'shop').queued)).toEqual([3, 5, 2, 4, 1])
  })

  it('keeps the oldest first without a prefix, or with labels that are not numbers', () => {
    const ms = [mr(1, { labels: q }), mr(2, { labels: [...q, 'merge-priority::0'] }), mr(3, { labels: [...q, 'merge-priority::high'] })]
    expect(iids(mergeQueue(prio(ms, null), 'shop').queued)).toEqual([1, 2, 3])
    expect(iids(mergeQueue(prio(ms), 'shop').queued)).toEqual([2, 1, 3])
  })

  it('reads the number after the prefix, the lowest of several', () => {
    expect(mergePriority(mr(1, { labels: ['merge-priority::2', 'merge-priority::10'] }), 'merge-priority::')).toBe(2)
    expect(mergePriority(mr(1, { labels: ['priority::1'] }), 'merge-priority::')).toBeNull()
    expect(mergePriority(mr(1, { labels: ['merge-priority::1'] }), '')).toBeNull()
  })
})

describe('priority levels', () => {
  it('offers 0 to 3, and any other an open MR carries', () => {
    const s = { ...snap([mr(1, { labels: ['merge-priority::7'] }), mr(2, { labels: ['merge-priority::1'] })]), merge_priority_prefix: 'merge-priority::' }
    expect(priorityLevels(s)).toEqual([0, 1, 2, 3, 7])
  })
})
