import { describe, expect, it } from 'vitest'
import { awaitsMyReview, canQueue, waitingAlert, isApproved, mergeState, nextStep, onlyWaitsForCi } from './rules'
import type { MergeRequest } from './types'

const mr = (over: Partial<MergeRequest> = {}): MergeRequest => ({
  iid: 1,
  repo: null,
  project: null,
  title: 'feat: x',
  url: '',
  state: 'opened',
  draft: false,
  source_branch: 'feat/x',
  target_branch: 'main',
  ticket: null,
  author: 'me',
  author_name: 'Me',
  reviewers: [],
  approved_by: [],
  changes_requested_by: [],
  approvals_left: 0,
  pipeline: 'success',
  merge_status: 'mergeable',
  conflicts: false,
  needs_rebase: false,
  auto_merge: false,
  head_pipeline: null,
  open_threads: 0,
  updated_at: '2026-10-14T08:00:00Z',
  merged_at: null,
  ...over,
}) as MergeRequest

describe('mergeState', () => {
  it('names what blocks an MR first', () => {
    expect(mergeState(mr({ draft: true, conflicts: true }))).toEqual({ label: 'draft', tone: '' })
    expect(mergeState(mr({ conflicts: true, pipeline: 'failed' }))).toEqual({ label: 'conflicts', tone: 'ko' })
    expect(mergeState(mr({ merge_status: 'need_rebase' })).label).toBe('rebase')
    expect(mergeState(mr({ pipeline: 'failed' })).label).toBe('blocked by CI')
    expect(mergeState(mr({ approvals_left: 1 })).label).toBe('in review')
    expect(mergeState(mr({ open_threads: 2 })).label).toBe('open threads')
    expect(mergeState(mr())).toEqual({ label: 'ready', tone: 'ok' })
    expect(mergeState(mr({ merge_status: 'ci_still_running' })).label).toBe('ci still running')
  })
})

describe('canQueue', () => {
  it('keeps out of the merge queue an MR its job could not rebase', () => {
    expect(canQueue(mr())).toBe(true)
    expect(canQueue(mr({ conflicts: true }))).toBe(false)
    expect(canQueue(mr({ merge_error: 'Rebase failed: conflicts' }))).toBe(false)
    expect(canQueue(mr({ draft: true }))).toBe(false)
    expect(canQueue(mr({ approvals_left: 1 }))).toBe(false)
  })
})

describe('nextStep', () => {
  it('offers the action that unblocks my MR', () => {
    expect(nextStep(mr({ conflicts: true }), 'me')).toEqual({ label: 'Conflicts', tone: 'blocked', primary: 'claude-rebase' })
    expect(nextStep(mr({ pipeline: 'failed' }), 'me').primary).toBe('claude-fix')
    expect(nextStep(mr({ merge_error: 'Rebase failed: conflicts' }), 'me').label).toBe('Rebase failed')
    expect(nextStep(mr({ needs_rebase: true, merge_status: 'unchecked' }), 'me')).toEqual({ label: 'Rebase needed', tone: 'todo', primary: 'rebase' })
    expect(nextStep(mr(), 'me')).toEqual({ label: 'Ready to merge', tone: 'ready', primary: 'merge' })
  })

  it("leaves someone else's MR to them, except my review", () => {
    expect(nextStep(mr({ pipeline: 'failed' }), 'you').primary).toBe('logs')
    expect(nextStep(mr({ reviewers: ['you'], approvals_left: 1 }), 'you')).toEqual({ label: 'Review it', tone: 'todo', primary: 'claude-review' })
    expect(nextStep(mr({ reviewers: ['you'], approved_by: ['you'], approvals_left: 1 }), 'you')).toEqual({ label: 'In review', tone: 'wait' })
    expect(nextStep(mr(), 'you').primary).toBeUndefined()
  })

  it('waits for CI and auto-merge', () => {
    expect(nextStep(mr({ pipeline: 'running', merge_status: 'unchecked', approvals_left: 1 }), 'me')).toEqual({ label: 'CI running', tone: 'wait' })
    expect(nextStep(mr({ auto_merge: true, pipeline: 'running' }), 'me')).toEqual({ label: 'Merges after CI', tone: 'auto' })
  })

  it('offers auto-merge on my MR that only waits for its CI', () => {
    expect(nextStep(mr({ pipeline: 'running' }), 'me')).toEqual({ label: 'CI running', tone: 'wait', primary: 'automerge' })
    expect(nextStep(mr({ pipeline: 'running' }), 'you').primary).toBeUndefined()
    expect(nextStep(mr({ pipeline: 'running', approvals_left: 1 }), 'me').primary).toBeUndefined()
    expect(nextStep(mr({ pipeline: 'running', open_threads: 1 }), 'me').primary).toBeUndefined()
  })

  it('tells an MR that only waits for its CI, in the merge queue or not', () => {
    expect(onlyWaitsForCi(mr({ pipeline: 'running' }))).toBe(true)
    expect(onlyWaitsForCi(mr({ pipeline: 'running', labels: ['queue'] }))).toBe(true)
    expect(onlyWaitsForCi(mr({ pipeline: 'running', auto_merge: true }))).toBe(false)
    expect(onlyWaitsForCi(mr({ pipeline: 'running', needs_rebase: true }))).toBe(false)
    expect(onlyWaitsForCi(mr({ pipeline: 'success' }))).toBe(false)
  })

  it('ignores a queue label on a repository without a merge queue', () => {
    expect(nextStep(mr({ labels: ['queue'] }), 'me', 'queue').primary).toBe('merge')
  })
})

describe('changes requested', () => {
  const asked = { reviewers: ['sam'], approved_by: ['jo'], changes_requested_by: ['sam'] }
  it('keeps an approved MR from going "to merge"', () => {
    expect(isApproved(mr({ approved_by: ['jo'] }))).toBe(true)
    expect(isApproved(mr(asked))).toBe(false)
  })
  it('is the state and the next step of the MR, before its review', () => {
    expect(mergeState(mr({ ...asked, approvals_left: 1 }))).toEqual({ label: 'changes requested', tone: 'warn' })
    expect(nextStep(mr(asked), 'me')).toEqual({ label: 'Changes requested', tone: 'todo' })
    expect(nextStep(mr({ ...asked, reviewers: ['sam', 'you'] }), 'you')).toEqual({ label: 'Changes requested', tone: 'wait' })
  })
  it('no longer awaits the review of the one who asked', () => {
    expect(awaitsMyReview(mr({ author: 'rin', ...asked }), 'sam')).toBe(false)
    expect(awaitsMyReview(mr({ author: 'rin', reviewers: ['sam'] }), 'sam')).toBe(true)
  })
})

describe('waiting sessions', () => {
  const since = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString()
  const session = (over: object) => ({ id: 's', title: 't', folder: '', worktree: null, branch: null, ticket: null, status: 'needs_you', needs_action: null, last_activity: since(30), origin: 'agent', ...over }) as unknown as Parameters<typeof waitingAlert>[0]
  it('lets an agent’s plain turn end go after a short while', () => {
    expect(waitingAlert(session({ last_activity: since(5) }))).toBe(true)
    expect(waitingAlert(session({}))).toBe(false)
  })
  it('keeps a question, a permission or a Desktop session for hours', () => {
    expect(waitingAlert(session({ needs_action: 'Answer its question' }))).toBe(true)
    expect(waitingAlert(session({ status: 'waiting_tool' }))).toBe(true)
    expect(waitingAlert(session({ origin: 'desktop' }))).toBe(true)
  })
})
