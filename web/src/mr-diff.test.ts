import { describe, expect, it } from 'vitest'
import type { Discussion, FileDiff } from './mr-review.svelte'

// The module follows the page's address (filter.svelte.ts): none under Node.
const page = globalThis as { location?: unknown; addEventListener?: unknown }
page.location ??= { hash: '' }
page.addEventListener ??= () => {}
const { parseDiff } = await import('./mr-review.svelte')
const { fileTree, hunkRange, numOf, quoteOf, refOf, sideOf, threadIndex, threadsAt, treeRows } = await import('./mr-diff')

const file = (path: string, add = 1, del = 0) => ({ path, stats: { add, del } })
const names = (rows: ReturnType<typeof treeRows<{ path: string }>>) => rows.map((r) => `${'  '.repeat(r.depth)}${'dir' in r ? `${r.dir.name}/` : r.file.path}`)

describe('the file tree', () => {
  const tree = fileTree([file('web/src/components/B.svelte', 2, 1), file('web/src/components/A.svelte', 3), file('web/src/main.ts'), file('README.md', 0, 4)])

  it('merges a folder holding only a folder, and adds up each folder’s lines', () => {
    const web = tree.dirs.get('web')!
    expect(web.name).toBe('web/src')
    expect([web.add, web.del]).toEqual([6, 1])
    expect(web.dirs.get('components')!.path).toBe('web/src/components')
  })

  it('lists folders first, then files, by name; a shut folder hides what it holds', () => {
    expect(names(treeRows(tree, new Set()))).toEqual(['web/src/', '  components/', '    web/src/components/A.svelte', '    web/src/components/B.svelte', '  web/src/main.ts', 'README.md'])
    expect(names(treeRows(tree, new Set(['web/src/components'])))).toEqual(['web/src/', '  components/', '  web/src/main.ts', 'README.md'])
  })
})

const DIFF = ['@@ -10,4 +10,5 @@', ' a', '-b', '+B', '+C', ' d', '@@ -40,2 +40,1 @@', ' x', '-y'].join('\n')
const lines = parseDiff(DIFF)

describe('threads', () => {
  const thread = (id: string, path: string | null, line: number | null, old_side = false) => ({ id, path, line, old_side }) as Discussion
  const index = threadIndex([thread('t1', 'a.ts', 11), thread('t2', 'a.ts', 11, true), thread('t3', 'a.ts', null), thread('t4', null, null)])

  it('counts a file’s threads, those on no line too', () => {
    expect(index.count.get('a.ts')).toBe(3)
    expect(index.count.size).toBe(1)
  })

  it('hangs a thread under its line, on the old side for a removed one', () => {
    expect(threadsAt(index, 'a.ts', lines[2]).map((d) => d.id)).toEqual(['t2'])
    expect(threadsAt(index, 'a.ts', lines[3]).map((d) => d.id)).toEqual(['t1'])
    expect(threadsAt(index, 'a.ts', lines[1])).toEqual([])
  })
})

describe('a range’s anchor', () => {
  const f = { old_path: 'a.ts', new_path: 'a.ts' } as FileDiff

  it('is its line, and on several its first one too', () => {
    expect(refOf(f, lines, [3, 3])).toEqual({ old_path: 'a.ts', new_path: 'a.ts', old_line: null, new_line: 11 })
    expect(refOf(f, lines, [2, 4])).toEqual({ old_path: 'a.ts', new_path: 'a.ts', old_line: null, new_line: 12, pos: [12, 12], start: { old_line: 11, new_line: null, pos: [11, 11] } })
  })
})

describe('notes on an agent’s diff', () => {
  it('number a removed line on the old side, any other on the new one', () => {
    expect([sideOf(lines[2]), numOf(lines[2])]).toEqual(['old', 11])
    expect([sideOf(lines[3]), numOf(lines[3])]).toEqual(['new', 11])
    expect([sideOf(lines[5]), numOf(lines[5])]).toEqual(['new', 13])
  })

  it('quote the lines of their side', () => {
    expect(quoteOf(lines, 'new', 10, 12)).toEqual(['a', 'B', 'C'])
    expect(quoteOf(lines, 'old', 10, 11)).toEqual(['a', 'b'])
  })

  it('cover a hunk’s added lines, or the removed ones of a pure removal', () => {
    expect(hunkRange(lines, 0)).toEqual({ side: 'new', from: 11, to: 12 })
    expect(hunkRange(lines, 6)).toEqual({ side: 'old', from: 41, to: 41 })
    expect(hunkRange(parseDiff(['@@ -1,1 +1,1 @@', ' same'].join('\n')), 0)).toBeNull()
  })
})
