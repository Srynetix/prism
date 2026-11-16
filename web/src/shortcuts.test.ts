import { describe, expect, it } from 'vitest'
import { REGISTRY } from './shortcuts'

const SOURCES = import.meta.glob<string>(['./**/*.{ts,svelte}', '!./**/*.test.ts', '!./bindings/**', '!./shortcuts.ts'], { query: '?raw', import: 'default', eager: true })

describe('the shortcuts registry', () => {
  /**
   * What the keys do and what the help says come from the same entries: each `hit(id)` names one
   * that has a chord to match, and each one with a chord is matched somewhere (else the help lists
   * a key that does nothing). The grid's vim keys, matched by keys.svelte.ts, have none.
   */
  it('matches every chord it declares, and only those', () => {
    const code = Object.values(SOURCES).join('\n')
    const used = new Set([...code.matchAll(/\bhit\('([\w-]+)'/g)].map((m) => m[1]))
    const matched = new Set(REGISTRY.filter((s) => s.match).map((s) => s.id))
    expect([...used].filter((id) => !matched.has(id))).toEqual([])
    expect([...matched].filter((id) => !used.has(id))).toEqual([])
  })

  it('declares each id once', () => {
    const ids = REGISTRY.map((s) => s.id)
    expect(ids.filter((id, i) => ids.indexOf(id) !== i)).toEqual([])
  })
})
