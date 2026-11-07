import { describe, expect, it } from 'vitest'

/**
 * A `var(--name)` whose `--name` nothing defines is no value at all: the declaration is dropped
 * and the element inherits (the custom blocks' editor showed scripts in the text's font, its
 * `--font-mono` the app never had: it is `--mono`). Defined: in a stylesheet or a component's
 * style (`--name:`), on an element (`style:--name`, `style="--name: …"`), or by a script
 * (`setProperty('--name'`, `'--name':`). A name built at run time (`--shiki-token-${…}`) is not checked.
 */
const SOURCES = import.meta.glob<string>(['./**/*.{svelte,ts,css}', '!./**/*.test.ts', '!./bindings/**'], { query: '?raw', import: 'default', eager: true })

function defined(): Set<string> {
  const names = new Set<string>()
  for (const src of Object.values(SOURCES)) {
    for (const m of src.matchAll(/(?:^|[\s;{("'`:])--([\w-]+)\s*['"`]?\s*:/g)) names.add(m[1])
    for (const m of src.matchAll(/style:--([\w-]+)/g)) names.add(m[1])
    for (const m of src.matchAll(/setProperty\(\s*['"`]--([\w-]+)/g)) names.add(m[1])
  }
  return names
}

describe('CSS variables', () => {
  it('uses none that nothing defines', () => {
    const names = defined()
    const missing: string[] = []
    for (const [path, src] of Object.entries(SOURCES)) {
      for (const m of src.matchAll(/var\(\s*--([\w-]*\w)\b(?!-)/g)) {
        if (names.has(m[1])) continue
        missing.push(`${path}:${src.slice(0, m.index).split('\n').length}: --${m[1]}`)
      }
    }
    expect(missing).toEqual([])
  })
})
