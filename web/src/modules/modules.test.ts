import { describe, expect, it } from 'vitest'
import { addWords, CONTRACT, refusal } from './modules.svelte'
import core from '../../../src-tauri/crates/core/src/modules.rs?raw'

describe('a module’s declarations', () => {
  it('are checked against the contract the core gives', () => {
    expect(core.match(/pub const CONTRACT: u32 = (\d+);/)?.[1]).toBe(String(CONTRACT))
  })

  const blocks = () => []
  it('are refused for another contract or another Svelte', () => {
    expect(refusal({ prism: CONTRACT, svelte: '5.57.0', blocks }, '5.57.2')).toBeNull()
    expect(refusal({ prism: CONTRACT + 1, svelte: '5.57.2', blocks }, '5.57.2')).toContain(`contract ${CONTRACT + 1}`)
    expect(refusal({ prism: CONTRACT, svelte: '5.58.0', blocks }, '5.57.2')).toContain('5.58.0')
    expect(refusal({ svelte: '5.57.2', blocks }, '5.57.2')).toContain('(prism)')
    expect(refusal({ prism: CONTRACT, blocks }, '5.57.2')).toContain('(svelte)')
    expect(refusal({ prism: CONTRACT, svelte: '5.57.2' }, '5.57.2')).toContain('blocks()')
  })

  it('bring words under Prism’s', () => {
    const dict = { Agents: 'Agents (Prism)' }
    addWords(dict, { Agents: 'Agents (module)', Hello: 'Bonjour' })
    expect(dict).toEqual({ Agents: 'Agents (Prism)', Hello: 'Bonjour' })
  })
})
