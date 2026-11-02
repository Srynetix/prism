import { describe, expect, it } from 'vitest'

/** Every source of the window, as text. */
const sources = import.meta.glob(['./**/*.ts', './**/*.svelte', '!./bindings/**', '!./bridge.ts', '!./**/*.test.ts'], { query: '?raw', import: 'default', eager: true }) as Record<string, string>

describe('bridge', () => {
  it('is the only way to the core', () => {
    expect(Object.keys(sources).length).toBeGreaterThan(100)
    const direct = Object.entries(sources)
      .filter(([, text]) => /^\s*import (?!type )[^\n]*from '@tauri-apps\/api\/(core|event)'/m.test(text))
      .map(([file]) => file)
    expect(direct, 'import invoke, listen, hasCore or isDesktop from bridge.ts').toEqual([])
  })
})
