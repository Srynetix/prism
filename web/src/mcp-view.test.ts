import { describe, expect, it } from 'vitest'
import { counts, mcpView, openProposal, proposals, shown, title } from './mcp-view.svelte'
import type { Proposal } from './bindings/Proposal'
import type { Snapshot } from './bindings/Snapshot'

const proposal = (id: number, kind: Proposal['kind'] = 'config', ops: string[] = []): Proposal =>
  ({ id, kind, client: 'claude', why: 'why', at: '2026-11-09T10:00:00Z', text: '', diff: ops.map((op) => ({ op, text: 'x' })), touches: [], overrides: [] }) as unknown as Proposal
const snap = (list: Proposal[]) => ({ mcp: { proposals: list } }) as unknown as Snapshot

describe('MCP proposals', () => {
  it('names each kind', () => {
    expect(title(proposal(3))).toBe('Config proposal #3')
    expect(title(proposal(4, 'layout'))).toBe('Pages proposal #4')
    expect(title(proposal(5, 'custom_block'))).toBe('New custom block #5')
  })

  it('counts the lines added and removed', () => {
    expect(counts(proposal(1, 'config', ['+', '+', '-', ' ']))).toEqual({ add: 2, del: 1 })
  })

  it('shows the one asked for, else the oldest; none without a snapshot', () => {
    const s = snap([proposal(1), proposal(2)])
    expect(proposals(null)).toEqual([])
    openProposal(2)
    expect(shown(s)?.id).toBe(2)
    openProposal()
    expect(mcpView.open).toBe(true)
    expect(shown(s)?.id).toBe(1)
    openProposal(9)
    expect(shown(s)?.id).toBe(1)
    expect(shown(snap([]))).toBeUndefined()
  })
})
