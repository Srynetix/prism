import { beforeEach, describe, expect, it } from 'vitest'
import { live } from './live.svelte'
import { mrActions } from './claude-actions'
import type { MergeRequest, Snapshot } from './types'

const mr = (over: Partial<MergeRequest> = {}) =>
  ({ iid: 12, title: 'Shop: cart', state: 'opened', source_branch: 'feat/cart', target_branch: 'develop', web_url: 'https://forge/mr/12', repo: null, pipeline: null, conflicts: false, ...over }) as unknown as MergeRequest

const snapshot = (worktrees: { path: string; branch: string; repo: string; is_main: boolean }[] = []) =>
  ({
    repositories: [{ name: 'shop', primary: true, worktrees: true }],
    repos: [{ name: 'shop', path: '/code/shop' }],
    worktrees: worktrees.map((w) => ({ prunable: false, ...w })),
  }) as unknown as Snapshot

const review = () => mrActions(mr()).find((a) => a.id === 'review')!

describe('the review of an MR', () => {
  beforeEach(() => {
    live.snapshot = snapshot()
  })

  it('starts in the worktree of its branch when there is one, never in the clone', () => {
    live.snapshot = snapshot([
      { path: '/code/shop', branch: 'develop', repo: 'shop', is_main: true },
      { path: '/wt/cart', branch: 'feat/cart', repo: 'shop', is_main: false },
    ])
    const a = review()
    expect(a.launch).toMatchObject({ cwd: '/wt/cart' })
    expect(a.launch).not.toHaveProperty('worktree')
    // The same place as Review-fix.
    expect(mrActions(mr()).find((x) => x.id === 'review-fix')!.launch).toMatchObject({ cwd: '/wt/cart' })
  })

  it('makes a worktree that checks its branch out otherwise, a colleague’s too', () => {
    const a = review()
    expect(a.launch).toMatchObject({ worktree: { branch: 'feat/cart', repo: 'shop' } })
    expect(a.launch).not.toHaveProperty('cwd')
  })

  it('tells the agent it is already on the branch, not to make another worktree', () => {
    const a = review()
    expect(a.prompt).toMatch(/^Review the code of .*12\. You are already in a worktree on branch feat\/cart .*without creating another worktree/)
    expect(a.launch?.permission_mode).toBeUndefined()
  })

  it('asks Claude Desktop to switch to the branch first, without the worktree sentence', () => {
    const q = decodeURIComponent(new URL(review().href!).searchParams.get('q')!)
    expect(q).toMatch(/^Switch to branch feat\/cart of .*12\. Then, review the code of .*12\.$/)
    expect(q).not.toContain('worktree')
  })

  it('keeps the worktree sentence for Claude Desktop in the worktree it opens', () => {
    live.snapshot = snapshot([{ path: '/wt/cart', branch: 'feat/cart', repo: 'shop', is_main: false }])
    const href = new URL(review().href!)
    expect(href.searchParams.get('folder')).toBe('/wt/cart')
    expect(href.searchParams.get('q')).toContain('You are already in a worktree on branch feat/cart')
  })
})
