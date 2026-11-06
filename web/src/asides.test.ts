import { describe, expect, it } from 'vitest'
import { asideOfCall, byRepo, inFilter, isSuggest, launchOf, openOfAgent } from './asides.svelte'
import type { Aside, Snapshot } from './types'
import type { ConversationTool } from './agents.svelte'

const aside = (o: Partial<Aside> = {}): Aside => ({
  id: 1,
  repo: 'shop',
  title: 'Remove the dead helper',
  tldr: 'Nothing calls it.',
  prompt: 'Remove round_half_even from src/round.rs.',
  files: ['src/round.rs'],
  needs_current_branch: false,
  seen: [{ agent: 'a1', name: 'VAT fix', cli: 'claude', branch: 'fix/vat', tldr: 'dead', at: '2026-11-06T10:00:00Z' }],
  state: 'open',
  at: '2026-11-06T10:00:00Z',
  files_changed: false,
  ...o,
})
const snap = (asides: Aside[]) => ({ asides, agents: [] }) as unknown as Snapshot

describe('the tools in a Chat', () => {
  it('propose with suggest_task, flagged by the core', () => {
    const call = (tool: string, aside: boolean) => ({ tool, aside }) as ConversationTool
    expect(isSuggest(call('mcp__plugin_prism_prism__suggest_task', true))).toBe(true)
    expect(isSuggest(call('mcp__prism__dismiss_task', true)) || isSuggest(call('suggest_task', false))).toBe(false)
  })

  it('find their line by the task_id of their answer, or the one withdrawn', () => {
    const s = snap([aside({ id: 3 }), aside({ id: 7 })])
    const call = (o: Partial<ConversationTool>) => ({ tool: 'mcp__prism__suggest_task', aside: true, kind: 'other', what: null, state: 'ok', output: null, diff: [], added: 0, removed: 0, at: null, ms: null, ...o }) as ConversationTool
    expect(asideOfCall(call({ output: '{\n  "task_id": 7,\n  "status": "open"\n}' }), s)?.id).toBe(7)
    expect(asideOfCall(call({ tool: 'mcp__prism__dismiss_task', what: '#3' }), s)?.id).toBe(3)
    expect(asideOfCall(call({ state: 'running' }), s)).toBeUndefined()
  })
})

describe('the list', () => {
  it('badges the open ones only, not those kept for later', () => {
    expect(openOfAgent(snap([aside(), aside({ id: 2, state: 'later' })]), 'a1').map((a) => a.id)).toEqual([1])
    expect(inFilter(aside({ state: 'later' }), 'open') && inFilter(aside({ state: 'then' }), 'launched')).toBe(true)
  })

  it('groups by repository, those without one last', () => {
    expect(byRepo([aside({ repo: undefined }), aside({ repo: 'web' }), aside({ repo: 'api' })]).map(([r]) => r)).toEqual(['api', 'web', ''])
  })
})

describe('launchOf', () => {
  it('gives the launcher the core’s launch, a worktree or a folder', () => {
    const spec = { title: 'T', cwd: null, prompt: 'P', worktree: { repo: 'shop', name: null, branch: null, base: 'fix/vat', also: [] }, permission_mode: null, category: 'Development', tags: [], provider: 'claude', model: null, effort: 'high', then: [], from: 'aside:1', prism_mcp: false } as const
    expect(launchOf(spec as never)).toEqual({ title: 'T', prompt: 'P', category: 'Development', provider: 'claude', model: undefined, effort: 'high', from: 'aside:1', worktree: { repo: 'shop', name: undefined, branch: undefined, base: 'fix/vat' } })
    expect(launchOf({ ...spec, worktree: null, cwd: '/code/shop' } as never).cwd).toBe('/code/shop')
  })
})
