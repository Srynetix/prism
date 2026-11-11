import { describe, expect, it } from 'vitest'
import { admitted, CLASHES, CORE_BLOCK_IDS, OWNER, RUNTIME_COMPONENTS, RUNTIME_DEFS } from './runtime'
import type { BlockComponent, BlockDef } from './block'

describe('the modules’ blocks', () => {
  it('keep the core’s ids as views.ts has them', async () => {
    const { BLOCK_LABELS } = await import('../views')
    const { BLOCK_DEFS } = await import('.')
    const core = Object.keys(BLOCK_LABELS).filter((id) => !BLOCK_DEFS.some((b) => b.id === id))
    expect([...CORE_BLOCK_IDS].sort()).toEqual(core.sort())
  })

  it('are taken in when their id is free, said otherwise', () => {
    const def = (id: string) => ({ id, label: id, help: { purpose: '', points: [], source: '' } }) as BlockDef<never>
    RUNTIME_DEFS.splice(0, RUNTIME_DEFS.length, def('hello'), def('mrs'), def('docker'), def('hello'), def('Bad id'), def('later'))
    for (const b of RUNTIME_DEFS) OWNER[b.id] = 'm'
    for (const id of ['mrs', 'later']) RUNTIME_COMPONENTS[id] = {} as BlockComponent<never>
    CLASHES.splice(0)
    expect(admitted([def('docker')]).map((b) => b.id)).toEqual(['hello', 'later'])
    expect(CLASHES.map((c) => c.block)).toEqual(['mrs', 'docker', 'hello', 'Bad id'])
    expect('mrs' in RUNTIME_COMPONENTS).toBe(false)
    expect('later' in RUNTIME_COMPONENTS).toBe(true)
    RUNTIME_DEFS.splice(0)
    CLASHES.splice(0)
  })
})
