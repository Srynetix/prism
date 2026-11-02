import { describe, expect, it } from 'vitest'
import { answersFor, fill } from './quick-answers'
import type { Agent } from './types'

const agent = (over: Partial<Agent> = {}) => ({ title: 'Export CSV', cwd: '/w/prism-acme-4688', ticket: 'ACME-4688', branch: 'agent/acme-4688', mr: null, state: 'running', status: 'needs_you', ...over }) as Agent

describe('quick answers', () => {
  it('fills the variables, or gives up on a missing one', () => {
    expect(fill('Open the MR for {ticket} on {branch} in {folder}', agent())).toBe('Open the MR for ACME-4688 on agent/acme-4688 in prism-acme-4688')
    expect(fill('Review {mr}', agent())).toBeNull()
    expect(fill('Keep {this} as is', agent())).toBe('Keep {this} as is')
  })

  it('offers them at the prompt only, in the prompt language by default', () => {
    expect(answersFor(undefined, 'fr', agent()).map((q) => q.label)).toContain('Fais-le')
    expect(answersFor(undefined, 'en', agent())[1].sent).toBe('Go ahead, do it.')
    expect(answersFor(undefined, 'en', agent({ status: 'running' }))).toEqual([])
    expect(answersFor(undefined, 'en', agent({ status: 'waiting_tool' }))).toEqual([])
    expect(answersFor({ enabled: false, answers: [] }, 'en', agent())).toEqual([])
  })

  it('takes the config list, without the answers the agent cannot fill', () => {
    const cfg = { enabled: true, answers: [{ label: 'MR', text: 'Review {mr}', submit: true }, { label: 'Go', text: 'Go on {ticket}', submit: true }] }
    expect(answersFor(cfg, 'en', agent()).map((q) => q.sent)).toEqual(['Go on ACME-4688'])
  })
})
