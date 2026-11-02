import { positionOf, type DiffLine, type Discussion, type FileDiff, type LineRef } from './mr-review.svelte'

/**
 * What a diff's view works out of its files and lines (MrDiffView.svelte), apart so that it can be
 * tested: the file tree, the threads by line, the forge's anchor of a range, and the lines a note
 * on a hunk covers.
 */

/* ---------- File tree ---------- */

export interface DiffDir<F> {
  name: string
  path: string
  dirs: Map<string, DiffDir<F>>
  files: F[]
  add: number
  del: number
}
export type TreeRow<F> = { dir: DiffDir<F>; depth: number } | { file: F; depth: number }

/**
 * The files' folders, each with its lines added and removed; a folder that only contains one
 * folder is merged with it (`src/components`), as in VS Code.
 */
export function fileTree<F extends { path: string; stats: { add: number; del: number } }>(files: F[]): DiffDir<F> {
  const top: DiffDir<F> = { name: '', path: '', dirs: new Map(), files: [], add: 0, del: 0 }
  for (const x of files) {
    let d = top
    for (const name of x.path.split('/').slice(0, -1)) {
      const path = d.path ? `${d.path}/${name}` : name
      let sub = d.dirs.get(name)
      if (!sub) d.dirs.set(name, (sub = { name, path, dirs: new Map(), files: [], add: 0, del: 0 }))
      sub.add += x.stats.add
      sub.del += x.stats.del
      d = sub
    }
    d.files.push(x)
  }
  const compact = (d: DiffDir<F>): DiffDir<F> => {
    let c = d
    while (c.files.length === 0 && c.dirs.size === 1) {
      const only = [...c.dirs.values()][0]
      c = { ...only, name: `${c.name}/${only.name}` }
    }
    return { ...c, dirs: new Map([...c.dirs].map(([k, v]) => [k, compact(v)])) }
  }
  return { ...top, dirs: new Map([...top.dirs].map(([k, v]) => [k, compact(v)])) }
}

/** The tree as rows: folders first, then files, in alphabetical order; those under a `shut` folder left out. */
export function treeRows<F extends { path: string }>(tree: DiffDir<F>, shut: { has(path: string): boolean }): TreeRow<F>[] {
  const out: TreeRow<F>[] = []
  const byName = <T>(key: (t: T) => string) => (a: T, b: T) => key(a).localeCompare(key(b))
  const walk = (d: DiffDir<F>, depth: number) => {
    for (const sub of [...d.dirs.values()].sort(byName((x: DiffDir<F>) => x.name))) {
      out.push({ dir: sub, depth })
      if (!shut.has(sub.path)) walk(sub, depth + 1)
    }
    for (const file of [...d.files].sort(byName((x: F) => x.path))) out.push({ file, depth })
  }
  walk(tree, 0)
  return out
}

/* ---------- Threads ---------- */

export interface ThreadIndex {
  /** By `path:o<old line>` or `path:n<new line>`. */
  at: Map<string, Discussion[]>
  /** By file. */
  count: Map<string, number>
}

/** Threads per file, and per line: number in the new file, or in the old one for a removed line. */
export function threadIndex(discussions: Discussion[]): ThreadIndex {
  const at = new Map<string, Discussion[]>()
  const count = new Map<string, number>()
  for (const d of discussions) {
    if (d.path === null) continue
    count.set(d.path, (count.get(d.path) ?? 0) + 1)
    if (d.line === null) continue
    const k = `${d.path}:${d.old_side ? 'o' : 'n'}${d.line}`
    at.set(k, [...(at.get(k) ?? []), d])
  }
  return { at, count }
}

const NONE: Discussion[] = []
/** The threads under a line: on its old number for a removed one, on its new one otherwise. */
export function threadsAt(index: ThreadIndex, path: string, l: DiffLine): Discussion[] {
  const old = l.kind === 'del' ? index.at.get(`${path}:o${l.old}`) : undefined
  const now = l.new !== null ? index.at.get(`${path}:n${l.new}`) : undefined
  return old && now ? [...old, ...now] : (old ?? now ?? NONE)
}

/** The forge's anchor of lines `[from, to]` of a file: the last one, and on several the first one. */
export function refOf(f: FileDiff, lines: DiffLine[], [from, to]: [number, number]): LineRef {
  const l = lines[to]
  const ref: LineRef = { old_path: f.old_path, new_path: f.new_path, old_line: l.old, new_line: l.new }
  if (from === to) return ref
  const s = lines[from]
  return { ...ref, pos: positionOf(lines, to), start: { old_line: s.old, new_line: s.new, pos: positionOf(lines, from) } }
}

/* ---------- Review notes (an agent's diff) ---------- */

export type Side = 'new' | 'old'
/** A line's side and number for a note: a removed line by its old number, any other by its new one. */
export const sideOf = (l: DiffLine): Side => (l.kind === 'del' ? 'old' : 'new')
export const numOf = (l: DiffLine) => (l.kind === 'del' ? l.old : l.new)

/** The text of lines `from`–`to` of one side: what a note quotes to the agent. */
export function quoteOf(lines: DiffLine[], side: Side, from: number, to: number): string[] {
  const inside = (n: number | null) => n !== null && n >= from && n <= to
  return lines.filter((l) => l.kind !== 'hunk' && (side === 'old' ? l.kind !== 'add' && inside(l.old) : l.kind !== 'del' && inside(l.new))).map((l) => l.text)
}

/** A hunk's changed lines, as one note's range: the added ones, or the removed ones of a pure removal. */
export function hunkRange(lines: DiffLine[], at: number): { side: Side; from: number; to: number } | null {
  const body: DiffLine[] = []
  for (let i = at + 1; i < lines.length && lines[i].kind !== 'hunk'; i++) body.push(lines[i])
  const adds = body.filter((l) => l.kind === 'add')
  const side: Side = adds.length ? 'new' : 'old'
  const nums = (adds.length ? adds.map((l) => l.new) : body.filter((l) => l.kind === 'del').map((l) => l.old)).filter((n): n is number => n !== null)
  return nums.length ? { side, from: Math.min(...nums), to: Math.max(...nums) } : null
}
