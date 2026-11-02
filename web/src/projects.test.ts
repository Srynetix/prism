import { describe, expect, it } from 'vitest'
import { nextTicket, projectsOf } from './projects'
import type { Group } from './agents-list'
import type { LeftSession, Ticket } from './types'

const now = Date.parse('2026-10-25T12:00:00Z')
const left = (repo: string, days: number) => ({ repo, id: repo, closed_at: new Date(now - days * 86_400_000).toISOString() }) as LeftSession
const group = (name: string) => ({ name, items: [] }) as Group

describe('projectsOf', () => {
  it('puts those with agents first, then those left lately, and rests the others', () => {
    const { active, resting } = projectsOf({ groups: [group('synth')], repos: ['car', 'synth', 'gui', 'things'], left: [left('car', 6), left('things', 2), left('gui', 40)], next: () => null, now })
    expect(active.map((p) => p.name)).toEqual(['synth', 'things', 'car'])
    expect(resting.map((p) => p.name)).toEqual(['gui'])
    expect(active[1].left?.repo).toBe('things')
    expect(active[0].left).toBeNull()
  })
})

describe('nextTicket', () => {
  const tk = (id: string, repo: string, status: string, closed = false) => ({ id, repo, status, closed }) as Ticket
  it('takes the first status first, in its repository, unblocked', () => {
    const tickets = [tk('S-2', 'synth', 'backlog'), tk('S-1', 'synth', 'Prioritaire'), tk('T-1', 'things', 'prioritaire'), tk('S-3', 'synth', 'prioritaire')]
    expect(nextTicket(tickets, ['prioritaire', 'backlog'], 'synth', (t) => t.id === 'S-1')?.id).toBe('S-3')
    expect(nextTicket(tickets, ['prioritaire'], 'gui', () => false)).toBeNull()
  })
})
