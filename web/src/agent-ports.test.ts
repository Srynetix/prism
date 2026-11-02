import { describe, expect, it, vi } from 'vitest'

const server = (agent: string, ports: number[], state = 'running', extra = {}) => ({ key: `shell:${agent}:${ports[0]}`, agent, shell: 1, command: 'pnpm dev', typed: true, program: 'node', ports, state, tail: null, failed: false, docker: false, since: null, ...extra })
const live = { snapshot: { blocks: { dev_servers: { servers: [server('a1', [5173, 24678]), server('a1', [5173], 'running', { docker: true }), server('a1', [8000], 'stopped'), server('a2', [3000])], outside: [], scan_outside: true, read_at: null } } } as Record<string, unknown> }
vi.mock('./live.svelte', () => ({ live }))

const { agentPorts, devServersOn } = await import('./agent-ports')

describe('agentPorts', () => {
  it('lists its own ports once, not a stopped server’s', () => {
    expect(agentPorts('a1').map((p) => p.port)).toEqual([5173, 24678])
    expect(agentPorts('a2').map((p) => p.port)).toEqual([3000])
  })

  it('has none while the Dev servers block is off', () => {
    live.snapshot = { blocks: {} }
    expect([devServersOn(), agentPorts('a1')]).toEqual([false, []])
  })
})
