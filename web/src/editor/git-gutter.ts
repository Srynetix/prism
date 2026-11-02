import { Chunk } from '@codemirror/merge'
import { EditorState, RangeSet, StateEffect, StateField, Text, type Extension } from '@codemirror/state'
import { Decoration, EditorView, GutterMarker, WidgetType, gutter, type DecorationSet } from '@codemirror/view'

/**
 * The editor's git gutter: each line against the file as committed (`HEAD`) — added, modified,
 * or where lines were removed — kept up to date as it is typed in. A click on a mark peeks at the
 * change: the lines as committed, and Revert hunk, which puts them back in the editor (saved with it).
 */

export type HunkKind = 'add' | 'mod' | 'del'

/** The committed text the file is compared to; `null`: nothing to compare (the gutter stays empty). */
export const setBase = StateEffect.define<string | null>()
/** The change peeked at (its start in the editor), or none. */
export const peekAt = StateEffect.define<number | null>()

interface Diff {
  base: Text | null
  chunks: readonly Chunk[]
}

export const gitDiff = StateField.define<Diff>({
  create: () => ({ base: null, chunks: [] }),
  update(d, tr) {
    for (const e of tr.effects) {
      if (e.is(setBase)) {
        const base = e.value === null ? null : Text.of(e.value.split('\n'))
        return { base, chunks: base ? Chunk.build(base, tr.state.doc) : [] }
      }
    }
    if (!d.base || !tr.docChanged) return d
    return { base: d.base, chunks: Chunk.updateB(d.chunks, d.base, tr.state.doc, tr.changes) }
  },
})

const peeked = StateField.define<number | null>({
  create: () => null,
  update(v, tr) {
    for (const e of tr.effects) if (e.is(peekAt)) return e.value
    if (v !== null && tr.docChanged) return tr.changes.mapPos(v)
    return v
  },
})

export const kindOf = (c: Chunk): HunkKind => (c.fromA === c.toA ? 'add' : c.fromB === c.toB ? 'del' : 'mod')

/** The change starting at `pos` in the editor (or holding it). */
function chunkAt(state: EditorState, pos: number): Chunk | null {
  const { chunks } = state.field(gitDiff)
  return chunks.find((c) => c.fromB === pos) ?? chunks.find((c) => pos >= c.fromB && pos <= c.endB) ?? null
}

/** The chunk's lines put back as committed. */
export function revertChunk(view: EditorView, c: Chunk) {
  const { base } = view.state.field(gitDiff)
  if (!base) return
  const doc = view.state.doc
  const to = Math.min(c.toB, doc.length)
  let insert = base.sliceString(c.fromA, Math.min(c.toA, base.length))
  // At the end of one text and not of the other: the line break the chunk counts on the other side.
  if (c.toB > doc.length && c.toA <= base.length && insert.endsWith('\n')) insert = insert.slice(0, -1)
  else if (c.toA > base.length && c.toB <= doc.length && to > c.fromB) insert += '\n'
  view.dispatch({ changes: { from: c.fromB, to, insert }, effects: peekAt.of(null), userEvent: 'revert' })
}

/** The committed lines of a chunk, for its peek. */
export function committedLines(state: EditorState, c: Chunk): string[] {
  const { base } = state.field(gitDiff)
  if (!base || c.fromA === c.toA) return []
  return base.sliceString(c.fromA, Math.min(c.toA, base.length)).replace(/\n$/, '').split('\n')
}

/** Its changes, in order, and the one peeked at. */
export const hunks = (state: EditorState) => state.field(gitDiff).chunks
export const peekedHunk = (state: EditorState) => {
  const at = state.field(peeked)
  return at === null ? null : chunkAt(state, at)
}

class Mark extends GutterMarker {
  constructor(readonly kind: HunkKind) {
    super()
    this.elementClass = `cm-git-${kind}`
  }
  eq(o: Mark) {
    return o.kind === this.kind
  }
}
const MARKS = { add: new Mark('add'), mod: new Mark('mod'), del: new Mark('del') }

function marks(state: EditorState) {
  const doc = state.doc
  const out: { from: number; mark: Mark }[] = []
  for (const c of state.field(gitDiff).chunks) {
    const kind = kindOf(c)
    if (kind === 'del') {
      // Removed lines: marked on the line they were above (the last one at the end).
      out.push({ from: doc.lineAt(Math.min(c.fromB, doc.length)).from, mark: MARKS.del })
      continue
    }
    const first = doc.lineAt(c.fromB).number
    const last = doc.lineAt(Math.min(c.endB, doc.length)).number
    for (let n = first; n <= last; n++) out.push({ from: doc.line(n).from, mark: MARKS[kind] })
  }
  return RangeSet.of(out.map((m) => m.mark.range(m.from)), true)
}

/** Each peek's cleanup, by its element: CodeMirror may destroy it through another, equal, widget. */
const cleanups = new WeakMap<HTMLElement, () => void>()

/** The peek, under the change: rendered by the pane (`onpeek`), which has the buttons. */
class PeekWidget extends WidgetType {
  constructor(readonly at: number, readonly render: (dom: HTMLElement, at: number, view: EditorView) => () => void) {
    super()
  }
  eq(o: PeekWidget) {
    return o.at === this.at
  }
  toDOM(view: EditorView) {
    const dom = document.createElement('div')
    dom.className = 'cm-git-peek'
    cleanups.set(dom, this.render(dom, this.at, view))
    return dom
  }
  destroy(dom: HTMLElement) {
    cleanups.get(dom)?.()
  }
  ignoreEvent() {
    return true
  }
}

/**
 * The gutter, and its peek drawn by `render` (mounted in the element it is given, at the change
 * starting at `at`; it returns its cleanup).
 */
export function gitGutter(render: (dom: HTMLElement, at: number, view: EditorView) => () => void): Extension {
  const peekDeco = EditorView.decorations.compute([peeked, gitDiff], (state): DecorationSet => {
    const c = peekedHunk(state)
    if (!c) return Decoration.none
    const doc = state.doc
    const after = kindOf(c) === 'del' ? Math.max(0, c.fromB - 1) : Math.min(c.endB, doc.length)
    const pos = kindOf(c) === 'del' && c.fromB === 0 ? 0 : doc.lineAt(after).to
    return Decoration.set([Decoration.widget({ widget: new PeekWidget(c.fromB, render), block: true, side: kindOf(c) === 'del' && c.fromB === 0 ? -1 : 1 }).range(pos)])
  })
  return [
    gitDiff,
    peeked,
    peekDeco,
    gutter({
      class: 'cm-git-gutter',
      markers: (v) => marks(v.state),
      domEventHandlers: {
        mousedown(view, line) {
          const c = chunkAt(view.state, line.from) ?? view.state.field(gitDiff).chunks.find((x) => kindOf(x) === 'del' && view.state.doc.lineAt(Math.min(x.fromB, view.state.doc.length)).from === line.from)
          if (!c) return false
          const open = view.state.field(peeked)
          view.dispatch({ effects: peekAt.of(open === c.fromB ? null : c.fromB) })
          return true
        },
      },
    }),
  ]
}
