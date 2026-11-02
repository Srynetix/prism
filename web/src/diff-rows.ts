/**
 * An MR's diff as the Diff view lays it out: rows side by side, the changed part of a modified
 * line, and a fingerprint to tell a file seen before from a changed one.
 */
import type { Token } from './highlight'
import type { DiffLine } from './mr-review.svelte'

/** Fingerprint of a file's diff: a viewed file goes back to "to view" if it changes. */
export function hash(text: string) {
  let h = 5381
  for (let i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) | 0
  return (h >>> 0).toString(36)
}

/**
 * A test file, by its path, across the usual conventions: a `test`/`tests`/`__tests__`/`spec`
 * folder (and fixtures, snapshots, e2e), or a name like `x.test.ts`, `x_spec.rb`, `test_x.py`,
 * `x_test.go`, `XTest.java`, `XTests.cs`, Django's `tests.py`.
 */
export function isTestPath(path: string) {
  const parts = path.toLowerCase().split('/')
  const name = path.split('/').pop() ?? ''
  if (parts.slice(0, -1).some((d) => /^(tests?|__tests__|specs?|__mocks__|__snapshots__|fixtures|testdata|e2e|cypress|playwright)$/.test(d))) return true
  return /[._-](test|spec|e2e)s?\.[^.]+$/i.test(name) || /^(test_|(conftest|tests?)\.py$)/i.test(name) || /[a-z0-9](Tests?|Spec|IT)\.(java|kt|scala|cs|swift|groovy|php)$/.test(name)
}

/** Changes within a modified line: the `[start, end)` range that differs from its twin. */
export type Mark = [number, number]

export interface Row {
  left: number | null
  right: number | null
  hunk?: string
  /** A hunk's row: its line's index. */
  at?: number
}

/**
 * Removed then added lines in a row: paired in order, and for each pair, what
 * remains once the common start and end are stripped. A line rewritten end to end
 * (less than a third in common) is not marked: it would be noise.
 */
export function marksOf(lines: DiffLine[]): Map<number, Mark> {
  const out = new Map<number, Mark>()
  let i = 0
  while (i < lines.length) {
    if (lines[i].kind !== 'del') {
      i++
      continue
    }
    const dels: number[] = []
    while (i < lines.length && lines[i].kind === 'del') dels.push(i++)
    const adds: number[] = []
    while (i < lines.length && lines[i].kind === 'add') adds.push(i++)
    for (let k = 0; k < Math.min(dels.length, adds.length); k++) {
      const a = lines[dels[k]].text
      const b = lines[adds[k]].text
      let p = 0
      while (p < a.length && p < b.length && a[p] === b[p]) p++
      let s = 0
      while (s < a.length - p && s < b.length - p && a[a.length - 1 - s] === b[b.length - 1 - s]) s++
      const common = p + s
      if (common === a.length && common === b.length) continue
      if (common < Math.max(a.length, b.length) / 3) continue
      out.set(dels[k], [p, a.length - s])
      out.set(adds[k], [p, b.length - s])
    }
  }
  return out
}

/** Side by side: unchanged lines on both sides, removed on the left facing the added. */
export function splitRows(lines: DiffLine[]): Row[] {
  const rows: Row[] = []
  let i = 0
  while (i < lines.length) {
    const l = lines[i]
    if (l.kind === 'hunk') {
      rows.push({ left: null, right: null, hunk: l.text, at: i })
      i++
    } else if (l.kind === 'ctx') {
      rows.push({ left: i, right: i })
      i++
    } else {
      const dels: number[] = []
      while (i < lines.length && lines[i].kind === 'del') dels.push(i++)
      const adds: number[] = []
      while (i < lines.length && lines[i].kind === 'add') adds.push(i++)
      for (let k = 0; k < Math.max(dels.length, adds.length); k++) rows.push({ left: dels[k] ?? null, right: adds[k] ?? null })
    }
  }
  return rows
}

/** Pieces of a line: highlighted tokens, split by the changed range. */
export interface Piece extends Token {
  changed: boolean
}
export function pieces(tokens: Token[] | null | undefined, text: string, mark: Mark | undefined): Piece[] {
  const list: Token[] = tokens ?? [{ text }]
  if (!mark) return list.map((t) => ({ ...t, changed: false }))
  const out: Piece[] = []
  let at = 0
  for (const t of list) {
    const end = at + t.text.length
    const cuts = [at, Math.min(Math.max(mark[0], at), end), Math.min(Math.max(mark[1], at), end), end]
    for (let c = 0; c < 3; c++) {
      if (cuts[c + 1] > cuts[c]) out.push({ ...t, text: t.text.slice(cuts[c] - at, cuts[c + 1] - at), changed: c === 1 })
    }
    at = end
  }
  return out
}

/* ---------- Lines around the hunks ---------- */

/**
 * A stretch of the file the diff leaves out: before the first hunk, between two, after the last.
 * Numbered in the new file, `to` null after the last hunk (up to the end, unknown until the file
 * is read); unchanged, so its old numbers are its new ones shifted by `shift`.
 */
export interface Gap {
  from: number
  to: number | null
  shift: number
}

/** A gap's lines shown: from its top (under the hunk above), from its bottom (over the hunk below). */
export interface Shown {
  top: number
  bottom: number
}

/** One gap before each hunk, and one after the last. An empty hunk (`+4,0`) sits after its line. */
export function gapsOf(lines: DiffLine[]): Gap[] {
  const out: Gap[] = []
  let nextOld = 1
  let nextNew = 1
  for (const l of lines) {
    const h = l.kind === 'hunk' ? /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(l.text) : null
    if (!h) continue
    const [o, oc, n, nc] = [Number(h[1]), Number(h[2] ?? 1), Number(h[3]), Number(h[4] ?? 1)]
    const firstOld = oc ? o : o + 1
    const firstNew = nc ? n : n + 1
    out.push({ from: nextNew, to: firstNew - 1, shift: firstOld - firstNew })
    nextOld = firstOld + oc
    nextNew = firstNew + nc
  }
  if (out.length) out.push({ from: nextNew, to: null, shift: nextOld - nextNew })
  return out
}

/** The whole file's lines, its final newline not one more line. */
export const wholeLines = (text: string) => {
  const out = text.split('\n')
  if (out.at(-1) === '') out.pop()
  return out
}

/** The file read matches the diff: each of its new lines where the diff says. Not, if it has moved since. */
export const fits = (lines: DiffLine[], whole: string[]) => lines.every((l) => l.kind === 'hunk' || l.kind === 'del' || whole[l.new! - 1] === l.text)

/**
 * The diff with the gaps' lines shown, from `whole` (the file, new version): a gap opened up to
 * the end loses its hunk header, the hunks around it joined. `at`: a header's index → its gap; `left`:
 * per gap, the lines still hidden (null: unknown, the file not read).
 */
export function expandLines(lines: DiffLine[], gaps: Gap[], shown: Shown[], whole: string[] | null) {
  const sizeOf = (g: Gap) => (g.to !== null ? Math.max(0, g.to - g.from + 1) : whole ? Math.max(0, whole.length - g.from + 1) : null)
  const out: DiffLine[] = []
  const at = new Map<number, number>()
  const left: (number | null)[] = []
  const take = (g: Gap, from: number, to: number) => {
    for (let n = from; n <= to; n++) out.push({ kind: 'ctx', text: whole![n - 1], old: n + g.shift, new: n, extra: true })
  }
  /** Gap `k`'s lines shown from its top and its bottom, within its size. */
  const parts = (k: number) => {
    const g = gaps[k]
    const size = sizeOf(g)
    if (!whole || size === null) return { g, top: 0, bottom: 0, rest: size }
    const top = Math.min(shown[k]?.top ?? 0, size)
    const bottom = Math.min(shown[k]?.bottom ?? 0, size - top)
    return { g, top, bottom, rest: size - top - bottom }
  }
  let k = 0
  for (const l of lines) {
    if (l.kind !== 'hunk' || k >= gaps.length) {
      out.push(l)
      continue
    }
    const { g, top, bottom, rest } = parts(k)
    left.push(rest)
    take(g, g.from, g.from + top - 1)
    if (rest !== 0 || top + bottom === 0) {
      at.set(out.length, k)
      out.push(l)
    }
    take(g, g.to! - bottom + 1, g.to!)
    k++
  }
  if (k < gaps.length) {
    const { g, top, rest } = parts(k)
    left.push(rest)
    take(g, g.from, g.from + top - 1)
  }
  return { lines: out, at, left }
}
