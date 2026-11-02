import { describe, expect, it } from 'vitest'
import { GONE_MS, isArchived, setArchived, toForget } from './agent-archive.svelte'

const agent = (status: 'needs_you' | 'running' | 'waiting_tool', last_activity: string) => ({ id: 'a1', status, last_activity })

describe('archived agents', () => {
  it('stay out while they work, even archived at their prompt', () => {
    setArchived(agent('needs_you', '2026-10-20T10:00:00Z'), true)
    expect(isArchived(agent('needs_you', '2026-10-20T10:00:00Z'))).toBe(true)
    expect(isArchived(agent('running', '2026-10-20T10:05:00Z'))).toBe(true)
  })
  it('come back once they wait for you again', () => {
    expect(isArchived(agent('waiting_tool', '2026-10-20T10:06:00Z'))).toBe(false)
  })
  it('come back when unarchived', () => {
    setArchived(agent('running', '2026-10-20T10:07:00Z'), true)
    expect(isArchived(agent('running', '2026-10-20T10:07:00Z'))).toBe(true)
    setArchived(agent('running', '2026-10-20T10:07:00Z'), false)
    expect(isArchived(agent('running', '2026-10-20T10:07:00Z'))).toBe(false)
  })
})

describe('archived agents gone', () => {
  const map = { gone: '2026-10-20T10:00:00Z', here: '2026-10-20T10:00:00Z' }
  const here = { id: 'here', status: 'running' as const, last_activity: '2026-10-20T10:01:00Z' }
  const snap = (answered: string[], errors: Record<string, string> = {}) => ({ agents: [here], sessions: [], answered, errors }) as never
  it('are forgotten once gone from both lists for a while, the others kept', () => {
    expect(toForget(snap(['sessions']), map, 0)).toEqual([])
    expect(toForget(snap(['sessions']), map, GONE_MS - 1)).toEqual([])
    expect(toForget(snap(['sessions']), map, GONE_MS)).toEqual(['gone'])
  })
  it('are kept while the sessions are not read (a reload, a failing source), the wait starting again', () => {
    expect(toForget(snap(['sessions']), map, 0)).toEqual([])
    expect(toForget(snap([]), map, GONE_MS)).toEqual([])
    expect(toForget(snap(['sessions'], { sessions: 'boom' }), map, GONE_MS)).toEqual([])
    expect(toForget(snap(['sessions']), map, GONE_MS + 1)).toEqual([])
    expect(toForget(snap(['sessions']), map, 2 * GONE_MS + 1)).toEqual(['gone'])
  })
})
