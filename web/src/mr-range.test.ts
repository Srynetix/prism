import { describe, expect, it } from 'vitest'

// The module follows the page's address (filter.svelte.ts): none under Node.
const page = globalThis as { location?: unknown; addEventListener?: unknown }
page.location ??= { hash: '' }
page.addEventListener ??= () => {}
const { parseDiff, positionOf, sameHunk } = await import('./mr-review.svelte')

const DIFF = ['@@ -10,4 +10,5 @@', ' a', '-b', '+B', '+C', ' d', '@@ -40,2 +41,2 @@', ' x', '-y', '+Y'].join('\n')

describe('a range of lines', () => {
  const lines = parseDiff(DIFF)

  it('places each line in both files, as GitLab numbers them', () => {
    // ' a' 10/10, '-b' old 11, '+B' new 11, '+C' new 12, ' d' 12/13
    expect(lines.slice(1, 6).map((_, i) => positionOf(lines, i + 1))).toEqual([[10, 10], [11, 11], [12, 11], [12, 12], [12, 13]])
    // A removed line, then an added one: each takes the other file's next line.
    expect(positionOf(lines, 8)).toEqual([41, 42])
    expect(positionOf(lines, 9)).toEqual([42, 42])
  })

  it('stays in one hunk, without a line shown only for context', () => {
    expect(sameHunk(lines, 1, 5)).toBe(true)
    expect(sameHunk(lines, 5, 1)).toBe(true)
    expect(sameHunk(lines, 4, 8)).toBe(false)
    expect(sameHunk([...lines.slice(0, 3), { ...lines[1], extra: true }, ...lines.slice(3)], 1, 5)).toBe(false)
  })
})
