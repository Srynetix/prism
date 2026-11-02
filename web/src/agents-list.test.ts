import { describe, expect, it } from 'vitest'
import { agentEntry, groupEntries, shortOf, taskOf } from './agents-list'
import type { Agent } from './types'

const agent = (id: string, status: Agent['status'], extra: Partial<Agent> = {}) =>
  agentEntry({ id, status, title: id, last_activity: `2026-10-19T10:0${id.length}:00Z`, created_at: null, cwd: '/x', tags: [], ...extra } as unknown as Agent)

describe('groupEntries', () => {
  const list = [agent('a', 'running'), agent('bb', 'needs_you', { category: 'Review', tags: ['api'] }), agent('ccc', 'completed', { category: 'Gone', tags: ['api', 'ui'] })]
  const base = { sortBy: 'activity' as const, categories: ['Review'], tags: ['api', 'ui'] }

  it('puts what waits for me first, by status', () => {
    expect(groupEntries(list, { ...base, groupBy: 'status' }).map((g) => [g.name, g.items.map((e) => e.id)])).toEqual([
      ['Waiting for you', ['bb']],
      ['Running', ['a']],
      ['Finished', ['ccc']],
    ])
  })

  it('keeps a kind no longer configured, after the configured ones', () => {
    expect(groupEntries(list, { ...base, groupBy: 'category' }).map((g) => g.name)).toEqual(['Review', 'Gone', 'No kind'])
  })

  it('shows an agent in each of its tags', () => {
    expect(groupEntries(list, { ...base, groupBy: 'tag' }).map((g) => [g.name, g.items.map((e) => e.id)])).toEqual([
      ['#api', ['bb', 'ccc']],
      ['#ui', ['ccc']],
      ['No tag', ['a']],
    ])
  })

  it('groups by repository, the configured ones first, then those outside them', () => {
    const repoOf = (e: { id: string }) => ({ a: 'infra', bb: 'app' })[e.id] ?? null
    expect(groupEntries(list, { ...base, groupBy: 'repo', repos: ['app', 'docs'], repoOf }).map((g) => [g.name, g.items.map((e) => e.id)])).toEqual([
      ['app', ['bb']],
      ['infra', ['a']],
      ['No repository', ['ccc']],
    ])
  })
})

describe('an entry in the rail', () => {
  it('reads its task and its number', () => {
    const e = agent('x', 'running', { title: 'Review !1621', ticket: null })
    expect([taskOf(e), shortOf(e)]).toEqual(['review', '!1621'])
  })
})
