import { describe, expect, it } from 'vitest'
import type { Transport } from '../bridge'
import type { Snapshot } from '../types'
import { phoneProfiles, routed, sideOf } from './phone-profiles.svelte'

const transport = (name: string, calls: string[]): Transport => ({
  invoke: async <T>(command: string) => {
    calls.push(`${name}:${command}`)
    return undefined as T
  },
  listen: async () => () => {},
})

describe('the phone in All', () => {
  it('sends a call about another profile’s agent to that profile, any other to the page’s own', async () => {
    const calls: string[] = []
    const state = { connected: true, away: false, hello: null, ended: false, error: null }
    phoneProfiles.sides.perso = { snap: { agents: [{ id: 'p-1' }] } as unknown as Snapshot, state, transport: transport('perso', calls) }
    const page = routed(transport('own', calls))
    await page.invoke('agent_quick_answer', { id: 'p-1', text: '1' })
    await page.invoke('agent_allow', { agent: 'p-1' })
    await page.invoke('agent_quick_answer', { id: 'own-1' })
    await page.invoke('get_snapshot')
    expect(calls).toEqual(['perso:agent_quick_answer', 'perso:agent_allow', 'own:agent_quick_answer', 'own:get_snapshot'])
    expect([sideOf('p-1'), sideOf('own-1')]).toEqual(['perso', null])
    delete phoneProfiles.sides.perso
  })
})
