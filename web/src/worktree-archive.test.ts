import { describe, expect, it } from 'vitest'
import { isArchived, isHidden, setArchived, setArchivedMany, setHidden } from './worktree-archive.svelte'
import { isArchived as isAgentArchived, setArchived as setAgentArchived } from './agent-archive.svelte'

const wt = (head: string, changes = 0, path = '/w/a', prunable = false) => ({ path, head, changes, prunable })

describe('archived worktrees', () => {
  it('stay out while nothing happens in them', () => {
    setArchived(wt('abc'), true)
    expect(isArchived(wt('abc'))).toBe(true)
  })
  it('come back on a new commit or a change', () => {
    expect(isArchived(wt('def'))).toBe(false)
    setArchived(wt('def'), true)
    expect(isArchived(wt('def', 2))).toBe(false)
  })
  it('come back when unarchived', () => {
    setArchived(wt('ghi'), true)
    setArchived(wt('ghi'), false)
    expect(isArchived(wt('ghi'))).toBe(false)
  })
  it('never a folder gone, several at once', () => {
    setArchivedMany([wt('m', 0, '/w/gone', true), wt('x', 0, '/w/x')], true)
    expect(isArchived(wt('m', 0, '/w/gone', true))).toBe(false)
    expect(isArchived(wt('x', 0, '/w/x'))).toBe(true)
  })
})

describe('a worktree follows its agents', () => {
  const info = (path: string, agents: string[]) => ({ w: wt('h', 0, path), agents: agents.map((id) => ({ id, status: 'running', last_activity: '2026-10-20T10:00:00Z' })), sessions: [] }) as unknown as Parameters<typeof isHidden>[0]

  it('hidden once all of them are archived, whatever happens in it', () => {
    setAgentArchived({ id: 'a1', status: 'running', last_activity: '2026-10-20T10:00:00Z' }, true)
    expect(isHidden(info('/w/f', ['a1']))).toBe(true)
    expect(isHidden(info('/w/f', ['a1', 'a2']))).toBe(false)
    expect(isHidden(info('/w/g', []))).toBe(false)
  })
  it('back with its agents, which it brings back', () => {
    setHidden([info('/w/f', ['a1'])], false)
    expect(isAgentArchived({ id: 'a1', status: 'running', last_activity: '2026-10-20T10:00:00Z' })).toBe(false)
    expect(isHidden(info('/w/f', ['a1']))).toBe(false)
  })
})
