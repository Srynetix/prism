import { describe, expect, it } from 'vitest'
import { expandLines, fits, gapsOf, hash, isTestPath, marksOf, pieces, splitRows, wholeLines } from './diff-rows'
import { diffStats, parseDiff } from './mr-review.svelte'

const DIFF = ['@@ -1,4 +1,4 @@ fn main', ' keep', '-const rate = 0.2', '+const rate = 0.25', '-gone', ' end'].join('\n')

describe('parseDiff', () => {
  it('numbers the old and new lines', () => {
    const lines = parseDiff(DIFF)
    expect(lines.map((l) => [l.kind, l.old, l.new])).toEqual([
      ['hunk', null, null],
      ['ctx', 1, 1],
      ['del', 2, null],
      ['add', null, 2],
      ['del', 3, null],
      ['ctx', 4, 3],
    ])
    expect(diffStats(DIFF)).toEqual({ add: 1, del: 2 })
  })
})

describe('marksOf', () => {
  it('marks what differs between a removed line and the added one facing it', () => {
    const lines = parseDiff(DIFF)
    const marks = marksOf(lines)
    // `0.2` → `0.25`: the common start and end stripped.
    expect(marks.get(2)).toEqual([16, 16])
    expect(marks.get(3)).toEqual([16, 17])
    // A removed line without a twin is not marked.
    expect(marks.has(4)).toBe(false)
  })

  it('leaves a line rewritten end to end unmarked', () => {
    const marks = marksOf(parseDiff(['@@ -1 +1 @@', '-alpha beta', '+zzz'].join('\n')))
    expect(marks.size).toBe(0)
  })
})

describe('splitRows', () => {
  it('puts removed lines on the left, facing the added ones', () => {
    expect(splitRows(parseDiff(DIFF))).toEqual([
      { left: null, right: null, hunk: '@@ -1,4 +1,4 @@ fn main', at: 0 },
      { left: 1, right: 1 },
      { left: 2, right: 3 },
      { left: 4, right: null },
      { left: 5, right: 5 },
    ])
  })
})

describe('pieces', () => {
  it('splits highlighted tokens at the changed range', () => {
    const out = pieces([{ text: 'const ' }, { text: 'rate = 0.25' }], 'const rate = 0.25', [16, 17])
    expect(out.map((p) => [p.text, p.changed])).toEqual([
      ['const ', false],
      ['rate = 0.2', false],
      ['5', true],
    ])
  })
})

describe('hash', () => {
  it('changes with the diff', () => {
    expect(hash(DIFF)).toBe(hash(DIFF))
    expect(hash(DIFF)).not.toBe(hash(`${DIFF} `))
  })
})

describe('isTestPath', () => {
  it('knows test files by folder or by name', () => {
    for (const p of ['web/src/diff-rows.test.ts', 'src/a.spec.tsx', 'pkg/x_test.go', 'tests/test_api.py', 'app/conftest.py', 'shop/tests.py', 'shop/test.py', 'spec/models/user_spec.rb', 'src/__tests__/a.ts', 'src/test/java/FooTest.java', 'Foo.Tests/BarTests.cs', 'crates/core/tests/it.rs', 'e2e/login.ts', 'src/__snapshots__/a.snap'])
      expect(isTestPath(p), p).toBe(true)
  })
  it('leaves the code alone', () => {
    for (const p of ['src/latest.ts', 'src/contest.py', 'src/testing.rs', 'src/Attest.java', 'src/inspect.ts', 'docs/testing.md', 'src/protest/x.ts', 'shop/contests.py', 'shop/test.pyc'])
      expect(isTestPath(p), p).toBe(false)
  })
})

describe('expandLines', () => {
  // A 30-line file, line 10 and line 25 changed.
  const whole = Array.from({ length: 30 }, (_, i) => `l${i + 1}`)
  const TWO = ['@@ -9,3 +9,3 @@', ' l9', '-old10', '+l10', ' l11', '@@ -24,3 +24,3 @@', ' l24', '-old25', '+l25', ' l26'].join('\n')

  it('finds the gaps before, between and after the hunks', () => {
    expect(gapsOf(parseDiff(TWO))).toEqual([
      { from: 1, to: 8, shift: 0 },
      { from: 12, to: 23, shift: 0 },
      { from: 27, to: null, shift: 0 },
    ])
    // An empty side sits after its line: a pure addition after line 4 shifts the rest.
    expect(gapsOf(parseDiff(['@@ -4,0 +5,2 @@', '+a', '+b'].join('\n')))).toEqual([
      { from: 1, to: 4, shift: 0 },
      { from: 7, to: null, shift: -2 },
    ])
  })

  it('shows a gap from its ends, and joins the hunks once it is all shown', () => {
    const lines = parseDiff(TWO)
    const gaps = gapsOf(lines)
    expect(fits(lines, whole)).toBe(true)
    const none = expandLines(lines, gaps, [], null)
    expect(none.lines).toEqual(lines)
    expect(none.left).toEqual([8, 12, null])
    const some = expandLines(lines, gaps, [{ top: 0, bottom: 2 }, { top: 3, bottom: 0 }, { top: 0, bottom: 0 }], whole)
    expect(some.left).toEqual([6, 9, 4])
    expect(some.lines.slice(0, 3).map((l) => [l.kind, l.old, l.new, l.extra])).toEqual([
      ['hunk', null, null, undefined],
      ['ctx', 7, 7, true],
      ['ctx', 8, 8, true],
    ])
    const all = expandLines(lines, gaps, [{ top: 0, bottom: 0 }, { top: Infinity, bottom: 0 }, { top: Infinity, bottom: 0 }], whole)
    expect(all.lines.filter((l) => l.kind === 'hunk')).toHaveLength(1)
    expect(all.lines.at(-1)).toMatchObject({ text: 'l30', old: 30, new: 30 })
    expect(all.left).toEqual([8, 0, 0])
    expect([...all.at.values()]).toEqual([0])
  })

  it('refuses a file that has moved since the diff', () => {
    expect(fits(parseDiff(TWO), wholeLines(['x', ...whole].join('\n') + '\n'))).toBe(false)
  })
})
