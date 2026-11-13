import { describe, expect, it } from 'vitest'
import { byDay, conversations, conversationState, isAssistant, proposalsOf } from './assistant.svelte'
import { sourceToAdd } from './clones.svelte'
import type { Agent, Snapshot } from './types'
import type { AssistantProposal } from './bindings/AssistantProposal'

const agent = (o: Partial<Agent> = {}): Agent => ({ id: 'a1', title: 'Ticket CSV export', state: 'running', status: 'idle', created_at: '2026-11-13T09:00:00Z', launch: { assistant: true }, ...o }) as unknown as Agent
const proposal = (o: Partial<AssistantProposal> = {}): AssistantProposal => ({ id: 1, action: 'ticket', agent: 'a1', at: '2026-11-13T09:01:00Z', title: 'Export the orders as CSV', text: '…', why: '', ...o })
const snap = (agents: Agent[], proposals: AssistantProposal[] = []) => ({ agents, assistant: { proposals } }) as unknown as Snapshot

describe('the assistant', () => {
  it('keeps its conversations apart from the agents, the latest first', () => {
    const mine = agent({ id: 'a1', created_at: '2026-11-12T09:00:00Z' })
    const later = agent({ id: 'a2', created_at: '2026-11-13T09:00:00Z' })
    const other = agent({ id: 'x', launch: { title: 'Fix' } as Agent['launch'] })
    expect(isAssistant(other)).toBe(false)
    expect(conversations(snap([mine, other, later])).map((a) => a.id)).toEqual(['a2', 'a1'])
  })

  it('says where a conversation stands: a proposal first', () => {
    const s = snap([agent()], [proposal()])
    expect(conversationState(agent(), s).tone).toBe('wait')
    expect(proposalsOf(s, 'a1')).toHaveLength(1)
    expect(proposalsOf(s, 'a2')).toHaveLength(0)
    expect(conversationState(agent({ status: 'running' }), snap([])).tone).toBe('run')
    expect(conversationState(agent({ status: 'needs_you' }), snap([])).tone).toBe('wait')
    expect(conversationState(agent({ state: 'exited' }), snap([])).tone).toBe('idle')
    expect(conversationState(agent(), snap([])).tone).toBe('done')
  })

  it('groups its conversations by day', () => {
    const now = new Date('2026-11-13T15:00:00')
    const days = byDay([agent({ id: 'a', created_at: '2026-11-13T10:00:00' }), agent({ id: 'b', created_at: '2026-11-13T08:00:00' }), agent({ id: 'c', created_at: '2026-11-12T08:00:00' }), agent({ id: 'd', created_at: '2026-11-02T08:00:00' })], now)
    expect(days.map((d) => d.list.map((a) => a.id))).toEqual([['a', 'b'], ['c'], ['d']])
    expect(days[0].day).toBe('Today')
    expect(days[1].day).toBe('Yesterday')
  })

  it('reads a repository to add as the Add panel takes it', () => {
    expect(sourceToAdd('~/code/billing')).toEqual({ mode: 'folder', value: '~/code/billing' })
    expect(sourceToAdd('/Users/me/billing')).toEqual({ mode: 'folder', value: '/Users/me/billing' })
    expect(sourceToAdd('acme/billing')).toEqual({ mode: 'clone', value: 'acme/billing' })
    expect(sourceToAdd('https://gitlab.com/acme/tools/billing.git')).toEqual({ mode: 'clone', value: 'acme/tools/billing' })
    expect(sourceToAdd('git@github.com:acme/billing.git')).toEqual({ mode: 'clone', value: 'acme/billing' })
  })
})
