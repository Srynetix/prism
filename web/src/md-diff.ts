import { parse, type Block, type MdTarget } from './markdown'
import type { DiffLine } from './mr-review.svelte'

/**
 * A file's preview against its diff. A run of removed then added lines changes the blocks
 * its added lines fall in; a run of removed lines alone leaves a marker before the block that
 * now follows. Each diff row leads to its block (or marker), each block to its rows.
 */
export interface Preview {
  blocks: Block[]
  marks: Map<number, 'add' | 'mod'>
  gone: Map<number, { count: number; text: string }>
  /** Diff rows (indices in `lines`) of a block, or of a removal. */
  rows: Map<string, number[]>
  /** Block or removal of a diff row. */
  target: Map<number, MdTarget>
}
export const tkey = (t: MdTarget) => ('block' in t ? `b${t.block}` : `g${t.gone}`)

export function previewOf(text: string, lines: DiffLine[]): Preview {
  const blocks = parse(text)
  const blockAt = (n: number) => blocks.findIndex((b) => !!b.lines && b.lines[0] <= n && n <= b.lines[1])
  const marks = new Map<number, 'add' | 'mod'>()
  const gone = new Map<number, { count: number; text: string }>()
  const rows = new Map<string, number[]>()
  const target = new Map<number, MdTarget>()
  const link = (t: MdTarget, ids: number[]) => {
    rows.set(tkey(t), [...(rows.get(tkey(t)) ?? []), ...ids])
    for (const id of ids) target.set(id, t)
  }
  let lastNew = 0
  let i = 0
  while (i < lines.length) {
    const l = lines[i]
    if (l.kind === 'hunk') {
      lastNew = Number(/\+(\d+)/.exec(l.text)?.[1] ?? 1) - 1
      i++
      continue
    }
    if (l.kind === 'ctx') {
      lastNew = l.new ?? lastNew
      i++
      continue
    }
    const dels: number[] = []
    while (i < lines.length && lines[i].kind === 'del') dels.push(i++)
    const adds: number[] = []
    while (i < lines.length && lines[i].kind === 'add') adds.push(i++)
    if (!adds.length) {
      // Removed lines alone: before the first block starting after them.
      let at = blocks.findIndex((b) => !!b.lines && b.lines[0] > lastNew)
      if (at < 0) at = blocks.length
      const text = dels.map((d) => lines[d].text).filter((t) => t.trim())
      const prev = gone.get(at)
      gone.set(at, { count: (prev?.count ?? 0) + dels.length, text: [prev?.text, ...text].filter(Boolean).join('\n').slice(0, 400) })
      link({ gone: at }, dels)
      continue
    }
    lastNew = lines[adds[adds.length - 1]].new ?? lastNew
    // Each added line goes to its block; a blank one between two blocks, to the one before it.
    let block = -1
    for (const id of adds) {
      const at = blockAt(lines[id].new ?? 0)
      if (at >= 0) block = at
      if (block >= 0) link({ block }, [id])
    }
    // The removed lines go to the first block the run changes: that one was rewritten.
    const first = adds.map((id) => blockAt(lines[id].new ?? 0)).find((b) => b >= 0)
    if (dels.length && first !== undefined) {
      link({ block: first }, dels)
      marks.set(first, 'mod')
    }
  }
  // Otherwise a block made only of added lines is new, one partly added is changed.
  const added = new Set<number>()
  for (const l of lines) if (l.kind === 'add' && l.new !== null) added.add(l.new)
  for (const key of rows.keys()) {
    if (!key.startsWith('b')) continue
    const n = Number(key.slice(1))
    if (marks.has(n)) continue
    const [s, e] = blocks[n].lines!
    let all = true
    for (let k = s; k <= e && all; k++) all = added.has(k)
    marks.set(n, all ? 'add' : 'mod')
  }
  return { blocks, marks, gone, rows, target }
}
