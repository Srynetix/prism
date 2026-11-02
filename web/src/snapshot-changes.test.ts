import { describe, expect, it } from 'vitest'
import { applyChanges } from './snapshot-changes'

describe('applyChanges', () => {
  it('changes an array item by its index, the rest kept as is', () => {
    const old = { agents: [{ id: 'a', status: 'running' }, { id: 'b', status: 'idle' }], days: [1, 2, 3], same: { x: 1 } }
    const next = applyChanges(old, [{ path: ['agents', '1', 'status'], value: 'needs_you' }, { path: ['days', '2'], value: 4 }]) as typeof old
    expect(next).toEqual({ agents: [{ id: 'a', status: 'running' }, { id: 'b', status: 'needs_you' }], days: [1, 2, 4], same: { x: 1 } })
    expect(Array.isArray(next.agents) && Array.isArray(next.days)).toBe(true)
    expect(next.agents[0]).toBe(old.agents[0])
    expect(next.same).toBe(old.same)
    expect(old.agents[1].status).toBe('idle')
  })

  it('replaces the whole, adds and removes keys', () => {
    expect(applyChanges({ a: 1 }, [{ path: [], value: { b: 2 } }])).toEqual({ b: 2 })
    expect(applyChanges({ a: 1, b: 2 }, [{ path: ['b'], delete: true }, { path: ['c'], value: null }])).toEqual({ a: 1, c: null })
  })
})
