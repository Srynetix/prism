import { describe, expect, it } from 'vitest'
import { matchPath, parseAsk, rankPaths } from './quick-open'

const PATHS = ['src/cart/checkout.ts', 'src/tests/checkout.test.ts', 'docs/checkout-flow.md', 'src/http/stream/chunks.ts', 'src/db/schema-keys.ts', 'README.md']

describe('quick open', () => {
  it('finds a file by letters of its name, the name first', () => {
    const found = rankPaths(PATHS, 'chkout', () => 0).map((r) => r.path)
    expect(found[0]).toBe('src/cart/checkout.ts')
    expect(found.slice(1, 3).sort()).toEqual(['docs/checkout-flow.md', 'src/tests/checkout.test.ts'])
    expect(matchPath('src/cart/checkout.ts', 'zz')).toBeNull()
  })
  it('matches folders too, words in any order', () => {
    expect(rankPaths(PATHS, 'test cart', () => 0)).toEqual([])
    expect(rankPaths(PATHS, 'tests check', () => 0).map((r) => r.path)).toEqual(['src/tests/checkout.test.ts'])
    expect(rankPaths(PATHS, 'cart/', () => 0).map((r) => r.path)[0]).toBe('src/cart/checkout.ts')
  })
  it('puts boosted files first among equals, and alone with no query', () => {
    const boost = (p: string) => (p === 'src/tests/checkout.test.ts' ? 100 : 0)
    expect(rankPaths(PATHS, 'checkout', boost)[0].path).toBe('src/tests/checkout.test.ts')
    expect(rankPaths(PATHS, '', boost)[0].path).toBe('src/tests/checkout.test.ts')
  })
  it('marks the matched letters', () => {
    const m = matchPath('src/cart/checkout.ts', 'chk')!
    expect(m.at.map((i) => 'src/cart/checkout.ts'[i]).join('')).toBe('chk')
    expect(m.at[0]).toBe('src/cart/'.length)
  })
  it('reads a line, a symbol, text, or a file with its line', () => {
    expect(parseAsk(':42')).toEqual({ kind: 'line', line: 42 })
    expect(parseAsk('@apply')).toEqual({ kind: 'symbol', query: 'apply' })
    expect(parseAsk('#total ')).toEqual({ kind: 'text', query: 'total' })
    expect(parseAsk('checkout.ts:12')).toEqual({ kind: 'file', query: 'checkout.ts', line: 12 })
    expect(parseAsk('chk')).toEqual({ kind: 'file', query: 'chk', line: null })
  })
})
