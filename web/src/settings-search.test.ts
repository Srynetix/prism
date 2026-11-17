import { describe, expect, it } from 'vitest'
import { DEFAULT_KEYS } from './bindings/default-keys'
import { buildIndex, near, norm, pagesFor, parse, search } from './settings-search'
import { SECTIONS } from './settings-sections'
import { groupsOf, NODES, nodeOf, pageOf, TREE } from './settings-tree'

const index = buildIndex()
const keys = (q: string, value: (k: string) => string = () => '') => search(index, q, (e) => value(e.key)).map((e) => e.key)

describe('settings search', () => {
  it('ignores accents, case and quotes', () => {
    expect(norm('Préférences d’Éditeur')).toBe("preferences d'editeur")
  })

  it('finds letters in order only close together', () => {
    expect(near('wrkt', 'worktrees')).toBe(true)
    expect(near('vim', 'video in memory')).toBe(false)
    expect(near('wtre', 'when the tree')).toBe(false)
  })

  it('keeps the tables written in brackets', () => {
    expect(parse('[forge] main')).toEqual({ words: ['main'], tables: ['forge'] })
    expect(keys('[forge] main')[0]).toBe('forge.main_branch')
    expect(keys('[forge] main').every((k) => k.startsWith('forge.'))).toBe(true)
  })

  it('lists keys, every word found, the label first', () => {
    expect(keys('chat font')[0]).toBe('agents.chat_font_size')
    expect(keys('agents.font_size')[0]).toBe('agents.font_size')
    expect(keys('wrkt')).toContain('worktrees.stale_days')
    expect(keys('zzzz qqq')).toEqual([])
  })

  it('finds a key by a synonym, a choice or its value', () => {
    expect(keys('dark')).toContain('appearance.theme')
    expect(keys('comfortable')).toContain('appearance.density')
    expect(keys('helix', (k) => (k === 'open_with.editor' ? 'helix' : ''))).toContain('open_with.editor')
  })

  it('finds a provider’s keys in its page', () => {
    const e = search(index, 'merge queue label')[0]
    expect(e.key).toBe('gitlab.merge_queue_label')
    expect(e.page.id).toBe('forge')
    expect(e.provider?.id).toBe('forge-gitlab')
  })

  it('names the pages answering as chips', () => {
    expect(pagesFor('renov').map((p) => p.id)).toEqual(['renovate'])
  })
})

describe('settings tree', () => {
  it('has seven domains, each page in one of its groups', () => {
    expect(TREE.map((r) => r.id)).toEqual(['general', 'interface', 'work', 'code', 'agents-root', 'infra', 'remote-root'])
    for (const r of TREE) expect(groupsOf(r).flatMap((g) => g.pages).length).toBe(r.children?.length)
  })

  it('keeps a page naming no group of its domain, after the groups', () => {
    const root = { id: 'd', label: 'D', groups: [{ id: 'g', label: 'G' }], children: [{ id: 'a', label: 'A', group: 'g' }, { id: 'b', label: 'B' }] }
    expect(groupsOf(root).map((g) => [g.label, g.pages.map((p) => p.id)])).toEqual([['G', ['a']], ['', ['b']]])
  })

  it('shows every section on one page exactly', () => {
    const shown = [...NODES.values()].flatMap((n) => n.sections ?? [])
    const unplaced = SECTIONS.map((s) => s.id).filter((id) => !shown.includes(id))
    expect(unplaced).toEqual([])
  })

  it('keeps the former links: a section, a provider, a page moved', () => {
    expect(pageOf('gitlab')?.id).toBe('forge')
    expect(nodeOf('gitlab')?.id).toBe('forge-gitlab')
    expect(pageOf('cli-codex')?.id).toBe('agents')
    expect(pageOf('ci')?.id).toBe('renovate')
    expect(pageOf('security')?.id).toBe('vulns')
    expect(pageOf('digest')?.id).toBe('notifications')
    expect(pageOf('envs')?.id).toBe('kubernetes')
  })

  it('opens every page the help and the code link to', () => {
    const texts = Object.values(import.meta.glob(['./**/*.{md,ts,svelte}', '!./**/*.test.ts', '!./bindings/**'], { query: '?raw', import: 'default', eager: true })) as string[]
    const ids = new Set(texts.flatMap((x) => [...x.matchAll(/prism:settings\/([a-z_-]+)|openSettings\(\s*'([a-z_-]+)'/g)].map((m) => m[1] ?? m[2])))
    const dead = [...ids].filter((id) => id !== 'overview' && !pageOf(id))
    expect(dead).toEqual([])
  })
})

describe('settings defaults', () => {
  /**
   * The defaults come from Rust (`config::defaults`): each key Settings shows has one there, but an
   * optional key whose absence means « the other setting's » (`[agents.assistant] model`).
   */
  it('come from Rust for every key of the schema', () => {
    const has = new Set(DEFAULT_KEYS)
    const known = (k: string) => has.has(k) || DEFAULT_KEYS.some((d) => d.startsWith(`${k}.`))
    const missing = index.filter((e) => !e.field.optional && !known(e.key)).map((e) => e.key)
    expect(missing).toEqual([])
  })
})
