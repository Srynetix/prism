import { beforeAll, describe, expect, it } from 'vitest'
import { resolve } from './i18n.svelte'
import { DICTIONARIES, loadDictionary } from './i18n'
import fr from './i18n/fr-all'

beforeAll(() => loadDictionary('fr'))

describe('language', () => {
  it('follows the setting, then the system, English failing that', () => {
    expect(resolve('fr')).toBe('fr')
    expect(resolve('FR-ca')).toBe('fr')
    expect(resolve('de')).toBe('en')
    expect(resolve('en')).toBe('en')
  })
})

describe('dictionaries', () => {
  it('keep every placeholder of the English text', () => {
    const names = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(',')
    const wrong = Object.entries(DICTIONARIES.fr).filter(([en, fr]) => names(en.replace(/^[\w-]+\|/, '')) !== names(fr))
    expect(Object.keys(DICTIONARIES.fr).length).toBeGreaterThan(1000)
    expect(wrong).toEqual([])
  })

  /**
   * A word the code no longer says leaves the dictionary with it: each key is written as a literal
   * somewhere in the code. Those `DYNAMIC` reach `t()` from data (a container's state, an ArgoCD
   * health, a pod's). The blocks of another repository keep their words in their own dictionary.
   */
  it('hold only words the code says', () => {
    const DYNAMIC = ['removing', 'update_failed', 'Suspended', 'terminating']
    const code = Object.entries(import.meta.glob(['./**/*.{ts,svelte}', '!./i18n/**', '!./bindings/**', '!./**/*.test.ts'], { query: '?raw', import: 'default', eager: true }))
      .map(([, text]) => text as string)
      .join('\n')
    const said = (en: string) => [`'${en.replace(/'/g, "\\'").replace(/\n/g, '\\n')}'`, `"${en}"`, `\`${en}\``].some((q) => code.includes(q))
    const unsaid = Object.keys(fr).filter((k) => !DYNAMIC.includes(k) && !said(k.replace(/^[\w-]+\|/, '')))
    expect(unsaid).toEqual([])
  })
})
