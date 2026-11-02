import { beforeEach, describe, expect, it, vi } from 'vitest'

const invoke = vi.fn()
const agent: { id: string; title: string; state: string; status: string; needs_action?: string } = { id: 'a1', title: 'Agent', state: 'running', status: 'needs_you' }
const saved: Record<string, unknown> = { 'agents.inbox': { a1: ['first', 'second'] } }
vi.mock('./bridge', () => ({ invoke: (...a: unknown[]) => invoke(...a), isDesktop: () => true }))
vi.mock('./live.svelte', () => ({ live: { snapshot: { agents: [agent], inbox: { a1: 2 } } } }))
vi.mock('./layout.svelte', () => ({ pref: (k: string, d: unknown) => saved[k] ?? d, setPref: (k: string, v: unknown) => (saved[k] = v) }))

invoke.mockResolvedValue('queued')
const { ready, sendPrompt, waiting } = await import('./agent-inbox.svelte')
await new Promise((r) => setTimeout(r))
const atLoad = invoke.mock.calls.map((c) => c.slice())

describe('the inbox', () => {
  beforeEach(() => invoke.mockClear())

  it('gives the queue the window kept to Prism once, in order', () => {
    expect(atLoad).toEqual([
      ['agent_send', { id: 'a1', text: 'first' }],
      ['agent_send', { id: 'a1', text: 'second' }],
    ])
    expect(saved['agents.inbox']).toEqual({})
  })

  it('leaves sending and holding to Prism', async () => {
    invoke.mockResolvedValueOnce('already-sent')
    expect(await sendPrompt('a1', 'fix the test')).toBe('already-sent')
    expect(invoke).toHaveBeenCalledWith('agent_send', { id: 'a1', text: 'fix the test' })
    expect(waiting('a1')).toBe(2)
    expect(waiting('a3')).toBe(0)
  })

  it('is ready without a menu on its screen', () => {
    Object.assign(agent, { status: 'needs_you', needs_action: 'Answer its question' })
    expect(ready('a1')).toBe(false)
    agent.needs_action = undefined
    expect(ready('a1')).toBe(true)
    agent.status = 'waiting_tool'
    expect(ready('a1')).toBe(false)
  })
})
