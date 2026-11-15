import { beforeEach, describe, expect, it, vi } from 'vitest'

const live = { snapshot: null as Record<string, unknown> | null }
const saved: unknown[] = []
vi.mock('./live.svelte', () => ({ live }))
vi.mock('./settings.svelte', () => ({ saveEdits: async (e: { value: unknown }[]) => void saved.push(e[0].value) }))
vi.mock('./toast.svelte', () => ({ toast: vi.fn() }))
vi.mock('./layout.svelte', () => ({ currentView: () => ({ id: 'a' }), editing: { on: false }, forgetViews: vi.fn(), layout: { views: {}, modes: {} }, setView: vi.fn(async () => {}), toggleEditing: vi.fn() }))

const { deletePage, hidePage, movePage, resetToPack } = await import('./pages.svelte')
const { toast } = await import('./toast.svelte')

beforeEach(() => {
  saved.length = 0
  live.snapshot = { views: [
    { name: 'A', id: 'a', columns: ['todo'] },
    { name: 'H', id: 'h', columns: [], hidden: true },
    { name: 'B', id: 'b', columns: ['mrs'], origin: 'pack' },
    { name: 'C', id: 'c', columns: ['notes'], origin: 'copy' },
  ] }
})

describe('the pages from the rail', () => {
  it('moves a page among the rail’s, a hidden one keeping its place, a pack’s still followed', async () => {
    movePage('c', 0)
    await Promise.resolve()
    expect(saved[0]).toEqual([
      { name: 'C', id: 'c', columns: ['notes'] },
      { name: 'H', id: 'h', columns: [], hidden: true },
      { name: 'A', id: 'a', columns: ['todo'] },
      { id: 'b' },
    ])
  })

  it('hides a pack’s page by its id, never the last one shown', async () => {
    await hidePage('b')
    expect((saved[0] as unknown[])[2]).toEqual({ id: 'b', hidden: true })
    live.snapshot = { views: [{ name: 'A', id: 'a', columns: [] }, { name: 'H', id: 'h', columns: [], hidden: true }] }
    await hidePage('a')
    expect(saved).toHaveLength(1)
  })

  it('deletes a page with Undo, never the last one', async () => {
    await deletePage('a')
    expect((saved[0] as { id: string }[]).map((r) => r.id)).toEqual(['h', 'b', 'c'])
    const undo = vi.mocked(toast).mock.calls.at(-1)![2]!
    undo.run()
    await Promise.resolve()
    expect((saved[1] as { id: string }[]).map((r) => r.id)).toEqual(['a', 'h', 'b', 'c'])
    live.snapshot = { views: [{ name: 'A', id: 'a', columns: [] }] }
    await deletePage('a')
    expect(saved).toHaveLength(2)
  })

  it('puts a changed page back to its pack: its id alone', async () => {
    await resetToPack('c')
    expect((saved[0] as unknown[])[3]).toEqual({ id: 'c' })
  })
})
