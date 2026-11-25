import { describe, expect, it, vi } from 'vitest'

vi.mock('./bridge', () => ({ invoke: vi.fn(() => Promise.resolve([])), isDesktop: () => true }))

const { invoke } = await import('./bridge')
const mocked = vi.mocked(invoke)
const { prepareTidy, runTidy, safe, tidy, togglePicked, closeTidy } = await import('./project-branches.svelte')

const branch = (name: string, over: Record<string, unknown> = {}) => ({ name, head: 'abc', last_commit: null, author: 'me', subject: 's', upstream: null, upstream_gone: false, to_push: 0, to_pull: 0, ahead: 1, behind: 0, worktree: null, current: false, locked: null, keep: null, done: [] as string[], ...over })

describe('project branches', () => {
  it('a safe branch is done, with nothing to lose, not locked', () => {
    expect(safe(branch('a', { done: ['MR merged'] }))).toBe(true)
    expect(safe(branch('b'))).toBe(false)
    expect(safe(branch('c', { done: ['MR merged'], keep: '1 unpushed commit' }))).toBe(false)
    expect(safe(branch('d', { done: ['gone from origin'], locked: 'the clone is on it' }))).toBe(false)
  })

  it('a tidy picks the safe ones, then deletes those still picked', async () => {
    mocked.mockResolvedValueOnce([branch('merged', { done: ['MR merged'] }), branch('wip'), branch('gone', { done: ['gone from origin'] })])
    await prepareTidy('app')
    expect(mocked).toHaveBeenLastCalledWith('project_tidy_prepare', { repo: 'app' })
    expect(tidy.picked).toEqual(['merged', 'gone'])
    togglePicked('gone')
    mocked.mockResolvedValueOnce({ deleted: ['merged'], kept: [] })
    await runTidy()
    expect(mocked).toHaveBeenCalledWith('project_tidy', { repo: 'app', branches: ['merged'] })
    expect(tidy.report).toEqual({ deleted: ['merged'], kept: [] })
    closeTidy()
    expect(tidy.repo).toBe(null)
  })
})
