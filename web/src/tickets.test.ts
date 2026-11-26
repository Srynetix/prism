import { afterEach, describe, expect, it } from 'vitest'
import { unblockedTo } from './tickets'
import { live } from './live.svelte'
import type { Dependency, Snapshot, StatusOption, Ticket } from './types'

const ticket = (status: string, waiting_on: Dependency[]): Ticket => ({
  id: 'ABC-1', task_id: 't1', parent: null, priority: null, task_type: null, task_type_icon: null, list: null, repo: null, description: null,
  time_estimate: null, name: 'Ticket', status, closed: false, assignees: [], url: '', created_at: null, updated_at: null, fields: [], waiting_on, blocking: [],
}) as Ticket

const dep = (status: string, closed = false): Dependency => ({ task_id: `d-${status}`, id: 'ABC-9', name: 'Blocker', status, closed, url: '' })
const lanes = ['ready', 'to do', 'in progress', 'complete'].map((status): StatusOption => ({ status, color: null, closed: false }))

describe('unblockedTo', () => {
  afterEach(() => {
    live.snapshot = null
  })
  const conventions = (over: object = {}) => {
    live.snapshot = { conventions: { done_statuses: ['complete'], waiting_statuses: ['ready'], unblocked_status: 'to do', ...over } } as unknown as Snapshot
  }

  it('offers the next status once every blocker is done, whatever its case', () => {
    conventions()
    expect(unblockedTo(ticket('ready', [dep('Complete'), dep('', true)]), lanes)).toBe('to do')
    conventions({ waiting_statuses: ['Ready'], unblocked_status: 'TO DO' })
    expect(unblockedTo(ticket('ready', [dep('complete')]), lanes)).toBe('to do')
  })

  it('waits while a blocker is open, and says nothing of a ticket never blocked or elsewhere', () => {
    conventions()
    expect(unblockedTo(ticket('ready', [dep('complete'), dep('review')]), lanes)).toBeNull()
    expect(unblockedTo(ticket('ready', []), lanes)).toBeNull()
    expect(unblockedTo(ticket('in progress', [dep('complete')]), lanes)).toBeNull()
  })

  it('needs the status in its list, and the conventions of an older Prism lack the keys', () => {
    conventions()
    expect(unblockedTo(ticket('ready', [dep('complete')]), lanes.filter((s) => s.status !== 'to do'))).toBeNull()
    conventions({ waiting_statuses: undefined, unblocked_status: undefined })
    expect(unblockedTo(ticket('ready', [dep('complete')]), lanes)).toBeNull()
  })
})
