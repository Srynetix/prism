<script lang="ts">
  import { t, tc, tn } from '../i18n.svelte'
  import { forgeName } from '../forge'
  import type { MentionSource } from '../mentions.svelte'
  import MrThread from './MrThread.svelte'
  import Markdown from './Markdown.svelte'
  import type { MdMarks, MdTarget } from '../markdown'
  import { previewOf, tkey, type Preview } from '../md-diff'
  import CommentBox from './CommentBox.svelte'
  import type { ReviewHooks } from '../agent-review.svelte'
  import Icon from './Icon.svelte'
  import { highlightDiff, type Token } from '../highlight'
  import { expandLines, fits, gapsOf, hash, isTestPath, marksOf, pieces, splitRows, wholeLines, type Mark, type Shown } from '../diff-rows'
  import { pref, setPref } from '../layout.svelte'
  import { untrack } from 'svelte'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { diffStats, parseDiff, sameHunk, type DiffLine, type Discussion, type FileDiff, type LineRef, type MrDiff } from '../mr-review.svelte'
  import { keys } from '../platform'
  import { fileTree, hunkRange, numOf, quoteOf as quoteLines, refOf, sideOf, threadIndex, threadsAt as threadsIn, treeRows as rowsOf, type Side, type TreeRow } from '../mr-diff'

  /**
   * An MR's diff, for review:
   * - unified or side-by-side rendering (`diff.mode`, remembered);
   * - syntax highlighting (highlight.ts) and, within a modified line, the part that changed;
   * - one collapsible block per file, a "viewed" checkbox (then collapsed, and unchecked by
   *   itself if the file changes), the test files checked at once, and the file list on the left
   *   when there is room;
   * - threads under their line, and the gutter "+" to open one.
   * `focus` expands a file and scrolls to it (from a thread in the Discussion tab).
   * Without `oncomment` (an agent's diff), no "+": read-only. `iid` is then only used
   * to remember viewed files (`agent:<id>`).
   * Only files near the visible area have their lines in the DOM (and are highlighted):
   * the others are just a box of the same height, which keeps large MRs smooth.
   * A Markdown file can also be read rendered, next to its diff or alone (`diff.md`, remembered):
   * `source` reads the file whole (a new file is rebuilt from its diff), and the blocks the diff
   * changes are marked, lit from one side to the other, and lead to their lines.
   * `review` (an agent's diff): notes to the agent instead of threads — the gutter "+" (shift-click
   * a second line for a range), actions on a hunk, each note under the last line it covers.
   * `image` reads an image of the diff (PNG, JPEG, GIF, WebP), its new version or (`old`) its former
   * one: shown before and after, side by side, in place of the binary file's empty diff.
   * With `source`, the lines around the hunks are shown on demand: on a hunk's header (↑ the ones
   * above it, ↓ the ones under the hunk before, or all), and after the last; read from the file
   * whole, refused if it has moved since the diff. An MR's thread cannot start on them (the forge
   * refuses a line outside the diff), a note to the agent can.
   * `phone` (the remote): one file at a time, unified, its lines wrapped; ‹ › and the file list
   * go from one to the next, Viewed moves on to the next one not yet viewed.
   */
  let {
    iid, diff, discussions, me, onreply, onresolve, oncomment, focus = null, forge, source, image, review, phone = false, people,
  }: {
    iid: number | string
    diff: MrDiff
    discussions: Discussion[]
    me: string | null
    onreply?: (discussion: string, body: string) => Promise<void>
    onresolve?: (discussion: string, resolved: boolean) => Promise<void>
    oncomment?: (line: LineRef, body: string) => Promise<void>
    focus?: { path: string; line: number | null } | null
    forge?: string
    /** Who a comment on the forge can mention (`@`). */
    people?: MentionSource
    /** A file of the diff, whole, in its new version: the Markdown preview. */
    source?: (path: string) => Promise<string>
    /** An image of the diff as a `data:` URL: its new version, or (`old`) the former one. */
    image?: (path: string, old: boolean) => Promise<string>
    review?: ReviewHooks
    phone?: boolean
  } = $props()

  /** Beyond this, a file is collapsed on open. */
  const BIG = 400

  type Mode = 'unified' | 'split'
  // A phone has no room for two sides.
  let mode = $state<Mode>(untrack(() => phone) ? 'unified' : pref<Mode>('diff.mode', 'unified'))
  function setMode(m: Mode) {
    mode = m
    setPref('diff.mode', m)
  }

  const parsed = $derived(
    diff.files.map((f) => {
      const base = f.collapsed ? [] : parseDiff(f.diff)
      const path = f.new_path || f.old_path
      const h = hash(f.diff)
      // A new or removed file is whole in its diff.
      const gaps = source && !f.new_file && !f.deleted_file ? gapsOf(base) : []
      return { f, base, gaps, stats: diffStats(f.diff), path, hash: h, key: `${path}@${h}` }
    }),
  )

  /* ---------- Lines around the hunks ---------- */

  /** Lines shown per click. */
  const STEP = 20
  /** Per file (`key`): its gaps' lines shown. */
  const opened = new SvelteMap<string, Shown[]>()
  /** Per file (`key`): read whole, for its gaps (and its Markdown preview). */
  const wholes = new SvelteMap<string, { text: string | null; error: string | null }>()
  const reads = new Map<string, Promise<string>>()
  function readWhole(key: string, path: string): Promise<string> {
    let p = reads.get(key)
    if (!p) {
      p = source!(path)
      reads.set(key, p)
      p.then(
        (text) => alive && wholes.set(key, { text, error: null }),
        (e) => alive && wholes.set(key, { text: null, error: String(e) }),
      )
    }
    return p
  }
  /** The file's lines, once read, if it still matches its diff. */
  const wholeOf = (x: (typeof parsed)[number]) => {
    const text = wholes.get(x.key)?.text
    if (text == null) return null
    const lines = wholeLines(text)
    return fits(x.base, lines) ? lines : null
  }
  const gapError = (x: Entry) => {
    const w = wholes.get(x.key)
    if (w?.error) return t('The file cannot be read: {error}', { error: w.error })
    return w?.text != null && !wholeOf(x) ? t('The file has changed since this diff') : null
  }
  /** Shows more of gap `k`: ↑ over the hunk below, ↓ under the hunk above, or all of it. */
  async function expand(x: Entry, k: number, how: 'up' | 'down' | 'all') {
    try {
      await readWhole(x.key, x.f.new_path)
    } catch {
      return
    }
    const cur = [...(opened.get(x.key) ?? [])]
    const s = cur[k] ?? { top: 0, bottom: 0 }
    cur[k] = how === 'all' ? { top: Infinity, bottom: 0 } : how === 'up' ? { ...s, bottom: s.bottom + STEP } : { ...s, top: s.top + STEP }
    for (let i = 0; i < cur.length; i++) cur[i] ??= { top: 0, bottom: 0 }
    opened.set(x.key, cur)
  }

  /** Each file's lines as shown, its gaps opened or not; by what is shown, so a click redoes only its file. */
  const shaped = new Map<string, ReturnType<typeof shape>>()
  function shape(x: (typeof parsed)[number], shown: Shown[] | undefined) {
    const ex = shown ? expandLines(x.base, x.gaps, shown, wholeOf(x)) : expandLines(x.base, x.gaps, [], null)
    return { lines: ex.lines, gapAt: ex.at, left: ex.left, marks: marksOf(ex.lines), rows: splitRows(ex.lines) }
  }
  const files = $derived(
    parsed.map((x) => {
      const shown = x.gaps.length ? opened.get(x.key) : undefined
      const hl = shown ? `${x.key}#${shown.map((s) => `${s.top}.${s.bottom}`).join(',')}` : x.key
      let s = shaped.get(hl)
      if (!s) shaped.set(hl, (s = shape(x, shown)))
      return { ...x, ...s, hl }
    }),
  )
  const total = $derived(files.reduce((n, x) => ({ add: n.add + x.stats.add, del: n.del + x.stats.del }), { add: 0, del: 0 }))

  /* ---------- Viewed, collapsed files ---------- */

  type Viewed = Record<string, Record<string, string>>
  let viewed = $state<Viewed>(pref<Viewed>('mr.viewed', {}))
  const isViewed = (path: string, h: string) => viewed[iid]?.[path] === h
  function setViewed(path: string, h: string, on: boolean) {
    setViewedAll([{ path, hash: h }], on)
  }
  function setViewedAll(xs: { path: string; hash: string }[], on: boolean) {
    const mine = { ...(viewed[iid] ?? {}) }
    const next = new Set(folded)
    for (const x of xs) {
      if (on) mine[x.path] = x.hash
      else delete mine[x.path]
      if (on) next.add(x.path)
      else next.delete(x.path)
    }
    viewed = { ...viewed, [iid]: mine }
    setPref('mr.viewed', viewed)
    folded = next
  }
  const seen = $derived(files.filter((x) => isViewed(x.path, x.hash)).length)

  /** The test files: checked all at once (or unchecked, once they all are). */
  const tests = $derived(files.filter((x) => isTestPath(x.path)))
  const testsSeen = $derived(tests.filter((x) => isViewed(x.path, x.hash)).length)
  const toggleTests = () => setViewedAll(tests, testsSeen < tests.length)

  // svelte-ignore state_referenced_locally
  let folded = $state<Set<string>>(new Set(files.filter((x) => x.lines.length > BIG || isViewed(x.path, x.hash)).map((x) => x.path)))
  function toggle(path: string) {
    const next = new Set(folded)
    if (next.has(path)) next.delete(path)
    else next.add(path)
    folded = next
  }
  const foldAll = (on: boolean) => (folded = on ? new Set(files.map((x) => x.path)) : new Set())

  /* ---------- File list or tree ---------- */

  type Nav = 'list' | 'tree'
  let nav = $state(pref<Nav>('diff.nav', 'list'))
  function setNav(n: Nav) {
    nav = n
    setPref('diff.nav', n)
    if (!showFiles) toggleFiles()
  }
  /** Narrow drawer: the file list shows above the diff, on demand. */
  let showFiles = $state(pref<boolean>('diff.files', false))
  function toggleFiles() {
    showFiles = !showFiles
    setPref('diff.files', showFiles)
  }

  type Entry = (typeof files)[number]

  /** Tree: folders first, then files; a folder holding only a folder merged with it (`mr-diff.ts`). */
  const tree = $derived(fileTree(files))

  /** Collapsed folders in the tree. */
  const shut = new SvelteSet<string>()
  const treeRows = $derived<TreeRow<Entry>[]>(rowsOf(tree, shut))
  const toggleDir = (path: string) => (shut.has(path) ? shut.delete(path) : shut.add(path))

  /* ---------- On-demand rendering ---------- */

  /**
   * Files less than a screen and a half from the visible area: the only ones whose lines are
   * rendered. A file that moves away leaves a box of its measured height (or estimated, if it has
   * never been shown) so that the scrollbar does not move.
   */
  const near = new SvelteSet<string>()
  const heights = new Map<string, number>()
  /** Height of a code line (11px × 1.5): enough to estimate a never-shown file. */
  const LINE = 16.5
  let io: IntersectionObserver | undefined
  /**
   * The area the diff scrolls in: the MR drawer's body, the agent panel's diff… Its nearest
   * scrolling ancestor, so that "near" is measured against it: with the window as the root, the
   * margin does not reach past that area's edge, and the files below would render only once seen,
   * their real height then pushing the rest away.
   */
  function scroller(node: HTMLElement): HTMLElement | null {
    for (let p = node.parentElement; p; p = p.parentElement) {
      const y = getComputedStyle(p).overflowY
      if (y === 'auto' || y === 'scroll') return p
    }
    return null
  }
  function lazy(node: HTMLElement, path: string) {
    node.dataset.path = path
    io ??= new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const p = (e.target as HTMLElement).dataset.path!
          if (e.isIntersecting) near.add(p)
          else if (near.has(p) && composing?.path !== p && drafting?.path !== p) {
            const body = e.target.querySelector<HTMLElement>('.fbody')
            if (body) heights.set(`${mode}:${mdView}:${p}`, body.offsetHeight)
            near.delete(p)
          }
        }
      },
      { root: scroller(node), rootMargin: '1500px 0px' },
    )
    io.observe(node)
    return { destroy: () => io?.unobserve(node) }
  }
  $effect(() => () => io?.disconnect())
  const ghost = (x: (typeof files)[number]) => heights.get(`${mode}:${mdView}:${x.path}`) ?? (mode === 'unified' ? x.lines.length : x.rows.length) * LINE

  /* ---------- Highlighting ---------- */

  /**
   * Syntax highlighting, file by file and in the background, as files
   * approach the visible area; a line stays plain text until its file is
   * ready. By fingerprint: a file that changes is re-highlighted.
   */
  const colors = new SvelteMap<string, (Token[] | null)[] | null>()
  const queued = new Set<string>()
  const queue: (typeof files)[number][] = []
  let pumping = false
  let alive = true
  $effect(() => () => {
    alive = false
  })
  async function pump() {
    if (pumping) return
    pumping = true
    while (alive && queue.length) {
      const x = queue.shift()!
      // Yield to rendering between two files.
      await new Promise((r) => setTimeout(r, 0))
      let tokens: (Token[] | null)[] | null = null
      try {
        // The file whole, so a hunk inside a docstring or comment begun above it reads as such.
        if (source && !x.f.new_file && !x.f.deleted_file) await readWhole(x.key, x.f.new_path).catch(() => null)
        tokens = await highlightDiff(x.path, x.lines, wholeOf(x))
      } catch (e) {
        console.warn('highlight', x.path, e)
      }
      if (alive) colors.set(x.hl, tokens)
    }
    pumping = false
  }
  $effect(() => {
    for (const x of files) {
      if (x.f.collapsed || !x.lines.length || queued.has(x.hl) || !near.has(x.path) || folded.has(x.path)) continue
      queued.add(x.hl)
      queue.push(x)
    }
    pump()
  })

  /* ---------- Markdown preview ---------- */

  type MdView = 'diff' | 'both' | 'preview'
  let mdView = $state(pref<MdView>('diff.md', 'both'))
  function setMdView(v: MdView) {
    mdView = v
    setPref('diff.md', v)
  }
  /** A Markdown file that still exists, with its diff: it can be read rendered. */
  const previewable = (x: Entry) => /\.(md|markdown)$/i.test(x.path) && !x.f.deleted_file && !x.f.collapsed
  const viewOf = (x: Entry): MdView => (previewable(x) ? mdView : 'diff')

  /** The file's new version, by fingerprint: read once, when it nears the visible area. */
  const docs = new SvelteMap<string, { text: string | null; error: string | null }>()
  const asked = new Set<string>()
  $effect(() => {
    if (mdView === 'diff') return
    for (const x of files) {
      if (!previewable(x) || asked.has(x.key) || !near.has(x.path) || folded.has(x.path)) continue
      asked.add(x.key)
      const key = x.key
      if (x.f.new_file) {
        docs.set(key, { text: x.lines.filter((l) => l.kind === 'add').map((l) => l.text).join('\n'), error: null })
      } else if (!source) {
        docs.set(key, { text: null, error: 'the file cannot be read here' })
      } else {
        docs.set(key, { text: null, error: null })
        readWhole(key, x.f.new_path).then(
          (text) => alive && docs.set(key, { text, error: null }),
          (e) => alive && docs.set(key, { text: null, error: String(e) }),
        )
      }
    }
  })

  const previews = $derived.by(() => {
    const out = new Map<string, Preview>()
    for (const x of files) {
      const text = docs.get(x.key)?.text
      if (text != null) out.set(x.key, previewOf(text, x.lines))
    }
    return out
  })

  /** Block or removal lit, by the mouse over it or over one of its rows. */
  let lit = $state<{ key: string; target: MdTarget } | null>(null)
  const litRows = $derived.by(() => {
    if (!lit) return new Set<number>()
    return new Set(previews.get(lit.key)?.rows.get(tkey(lit.target)) ?? [])
  })
  const isLit = (key: string, i: number | null | undefined) => lit?.key === key && i !== null && i !== undefined && litRows.has(i)
  function hoverRow(key: string, i: number | null) {
    const t = i === null ? undefined : previews.get(key)?.target.get(i)
    lit = t ? { key, target: t } : null
  }
  function marksFor(x: Entry, p: Preview): MdMarks {
    return {
      of: (b) => p.marks.get(b) ?? null,
      gone: p.gone,
      lit: lit?.key === x.key ? lit.target : null,
      onhover: (t) => (lit = t ? { key: x.key, target: t } : null),
      onpick: (t) => {
        const first = Math.min(...(p.rows.get(tkey(t)) ?? []))
        if (!Number.isFinite(first)) return
        root?.querySelector(`[data-file="${CSS.escape(x.path)}"] [data-row="${first}"]`)?.scrollIntoView({ block: 'center' })
      },
    }
  }

  /* ---------- Images ---------- */

  /** An image the webview shows (no SVG: the backend refuses it, its diff is text anyway). */
  const isImage = (x: Entry) => !!image && /\.(png|jpe?g|gif|webp)$/i.test(x.path)
  type Pic = { url: string | null; error: string | null; w?: number; h?: number }
  /**
   * Before and after, by path, read when the file nears the visible area. A binary file's diff
   * is empty, its fingerprint says nothing: read again whenever the diff is.
   */
  const pics = new SvelteMap<string, { old: Pic | null; now: Pic | null }>()
  const picsAsked = new Set<string>()
  $effect.pre(() => {
    void diff
    untrack(() => {
      pics.clear()
      picsAsked.clear()
    })
  })
  $effect(() => {
    for (const x of files) {
      if (!isImage(x) || picsAsked.has(x.path) || !near.has(x.path) || folded.has(x.path)) continue
      picsAsked.add(x.path)
      const path = x.path
      const read = (p: string, old: boolean): Pic => {
        image!(p, old).then(
          (url) => alive && setPic(path, old, { url, error: null }),
          (e) => alive && setPic(path, old, { url: null, error: String(e) }),
        )
        return { url: null, error: null }
      }
      pics.set(path, { old: x.f.new_file ? null : read(x.f.old_path, true), now: x.f.deleted_file ? null : read(x.f.new_path, false) })
    }
  })
  function setPic(path: string, old: boolean, pic: Pic) {
    const cur = pics.get(path)
    if (cur) pics.set(path, old ? { ...cur, old: pic } : { ...cur, now: pic })
  }
  /** Its size, once loaded: shown under it, the change of size being often the point. */
  function measured(path: string, old: boolean, img: HTMLImageElement) {
    const cur = pics.get(path)?.[old ? 'old' : 'now']
    if (cur) setPic(path, old, { ...cur, w: img.naturalWidth, h: img.naturalHeight })
  }

  /* ---------- Comments and threads ---------- */

  /**
   * The lines being commented on: a file, its first line and its last (their keys), in one hunk.
   * Shift-click another line's "+", or drag along the "+": a range.
   */
  let composing = $state<{ path: string; a: string; b: string } | null>(null)
  const keyOf = (path: string, l: DiffLine) => `${path}:${l.old ?? ''}:${l.new ?? ''}`
  const linesOf = (path: string) => files.find((x) => x.path === path)?.lines ?? []
  /** The range being commented on, as indexes in its file's lines, the first first. */
  function composed(path: string): [number, number] | null {
    if (composing?.path !== path) return null
    const lines = linesOf(path)
    const a = lines.findIndex((l) => keyOf(path, l) === composing!.a)
    const b = lines.findIndex((l) => keyOf(path, l) === composing!.b)
    return a < 0 || b < 0 ? null : a < b ? [a, b] : [b, a]
  }
  function compose(path: string, l: DiffLine, extend: boolean) {
    const lines = linesOf(path)
    const at = composing?.path === path ? lines.findIndex((x) => keyOf(path, x) === composing!.a) : -1
    if (extend && at >= 0 && sameHunk(lines, at, lines.indexOf(l))) composing = { ...composing!, b: keyOf(path, l) }
    else composing = { path, a: keyOf(path, l), b: keyOf(path, l) }
  }
  const inComposing = (path: string, l: DiffLine) => {
    const r = composed(path)
    const i = r ? linesOf(path).indexOf(l) : -1
    return !!r && i >= r[0] && i <= r[1]
  }
  const labelOf = (l: DiffLine) => l.new ?? l.old ?? ''

  /** Dragging along the gutter's "+" from a line: the lines it goes over join the range. */
  let dragging: string | null = null
  function pressPlus(e: PointerEvent, path: string, l: DiffLine) {
    if (e.button !== 0 || e.pointerType === 'touch') return
    e.preventDefault()
    pick(path, l, e.shiftKey)
    dragging = path
  }
  function dragOver(e: PointerEvent) {
    if (!dragging) return
    const cell = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>('[data-i]')
    if (cell?.closest<HTMLElement>('[data-path]')?.dataset.path !== dragging) return
    const l = linesOf(dragging)[Number(cell.dataset.i)]
    if (l && l.kind !== 'hunk' && !l.extra) pick(dragging, l, true)
  }

  /** Threads per file, and per line (`mr-diff.ts`). */
  const threads = $derived(threadIndex(discussions))
  const threadsAt = (path: string, l: DiffLine) => threadsIn(threads, path, l)
  const threadCount = (path: string) => threads.count.get(path) ?? 0

  /* ---------- Review notes (an agent's diff) ---------- */

  /** The note being written (or edited): its file, side and range, grown by shift-click. */
  let drafting = $state<{ path: string; side: Side; from: number; to: number; edit?: string; body?: string } | null>(null)
  function pick(path: string, l: DiffLine, extend: boolean) {
    if (!review) return compose(path, l, extend)
    const side = sideOf(l)
    const n = numOf(l)
    if (n === null) return
    if (extend && drafting && !drafting.edit && drafting.path === path && drafting.side === side) drafting = { ...drafting, from: Math.min(drafting.from, n), to: Math.max(drafting.to, n) }
    else drafting = { path, side, from: n, to: n }
  }
  const inDraft = (path: string, l: DiffLine) => {
    const n = numOf(l)
    return !!drafting && !drafting.edit && drafting.path === path && drafting.side === sideOf(l) && n !== null && n >= drafting.from && n <= drafting.to
  }
  /** The text of a range of a file, on one side: quoted to the agent. */
  const quoteOf = (path: string, side: Side, from: number, to: number) => quoteLines(linesOf(path), side, from, to)
  const notesAt = (path: string, l: DiffLine) => (review?.notes ?? []).filter((n) => n.status !== 'resolved' && n.path === path && n.side === sideOf(l) && n.to === numOf(l))
  /** Notes of a file whose last line is no longer in its diff: shown at its top. */
  function orphans(x: Entry) {
    const shown = new Set(x.lines.filter((l) => l.kind !== 'hunk').map((l) => `${sideOf(l)}:${numOf(l)}`))
    return (review?.notes ?? []).filter((n) => n.status !== 'resolved' && n.path === x.path && !shown.has(`${n.side}:${n.to}`))
  }
  const noteCount = (path: string) => (review?.notes ?? []).filter((n) => n.status !== 'resolved' && n.path === path).length
  /** Actions on a hunk: each writes a note on it, to complete or send as is; the agent does the work. */
  const HUNK_ACTIONS: [string, string, string][] = [
    [t('Explain'), t('Ask the agent to explain this hunk'), 'Explain this change: why it is needed, and what it does.'],
    [t('Add a test'), t('Ask the agent for a test of this hunk'), 'Add a test covering this change.'],
    [t('Revert'), t('Ask the agent to undo this hunk'), 'Undo this change: restore what was there before.'],
  ]
  function hunkNote(path: string, lines: DiffLine[], at: number, body: string) {
    const r = hunkRange(lines, at)
    if (r && review) review.add({ path, ...r, quote: quoteOf(path, r.side, r.from, r.to), body })
  }
  const STARTERS = ['Why this?', 'Simpler:', 'Add a test:', 'Undo this:']
  const range = (n: { from: number; to: number }) => (n.from === n.to ? t('line {n}', { n: n.from }) : t('lines {from}–{to}', { from: n.from, to: n.to }))
  const hhmm = (iso?: string) => (iso ? new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : '')

  /* ---------- Navigation ---------- */

  /** The phone's file, by its rank. */
  let shownFile = $state(0)
  const pickAt = (i: number) => {
    shownFile = Math.max(0, Math.min(files.length - 1, i))
    root?.scrollIntoView({ block: 'start' })
  }
  /** Viewed on a phone: on to the next file not yet viewed. */
  function seenAndNext(x: Entry, on: boolean) {
    setViewed(x.path, x.hash, on)
    if (!on) return
    const next = files.findIndex((y, i) => i > shownFile && !isViewed(y.path, y.hash))
    if (next >= 0) pickAt(next)
  }
  /** The test files checked on a phone: off a test file shown, on to the first file not yet viewed. */
  function testsAndNext() {
    toggleTests()
    const x = files[shownFile]
    if (!x || !isViewed(x.path, x.hash)) return
    const next = files.findIndex((y) => !isViewed(y.path, y.hash))
    if (next >= 0) pickAt(next)
  }

  let root: HTMLElement | undefined = $state()
  /** The bar's height: it wraps when narrow (an agent's panel), and the file headers stick under it. */
  let barH = $state(0)
  /**
   * Scrolls to a file (and a line). Twice: the files above, rendered on the
   * way, do not always have the estimated height and shift the target.
   */
  function reveal(path: string, line: number | null, block: ScrollLogicalPosition) {
    // A phone shows one file: that one.
    if (phone) shownFile = Math.max(0, files.findIndex((x) => x.path === path))
    if (folded.has(path)) toggle(path)
    near.add(path)
    const go = () => {
      const el = root?.querySelector<HTMLElement>(`[data-file="${CSS.escape(path)}"]`)
      const at = line !== null ? el?.querySelector<HTMLElement>(`[data-line="${line}"]`) : null
      ;(at ?? el)?.scrollIntoView({ block })
    }
    requestAnimationFrame(() => {
      go()
      setTimeout(() => requestAnimationFrame(go), 50)
    })
  }
  const jump = (path: string) => reveal(path, null, 'start')
  $effect(() => {
    if (!focus || !root) return
    const target = focus
    untrack(() => reveal(target.path, target.line, 'center'))
  })

  /**
   * The messages of the diff, in reading order: the threads (an MR's), the notes not resolved (an
   * agent's diff). `n` / `N` and the bar's buttons go from one to the next, the hidden ones unfolded.
   */
  const messages = $derived.by(() => {
    const order = new Map(files.map((x, i) => [x.path, i]))
    const out: { path: string; line: number | null }[] = []
    for (const d of discussions) if (d.path !== null && order.has(d.path)) out.push({ path: d.path, line: d.line })
    for (const n of review?.notes ?? []) if (n.status !== 'resolved' && order.has(n.path)) out.push({ path: n.path, line: n.to })
    return out.sort((a, b) => order.get(a.path)! - order.get(b.path)! || (a.line ?? 0) - (b.line ?? 0))
  })
  /** The message last gone to; none yet: -1. */
  let at = $state(-1)
  function goMessage(step: 1 | -1) {
    const n = messages.length
    if (!n) return
    at = at < 0 || at >= n ? (step > 0 ? 0 : n - 1) : (at + step + n) % n
    const m = messages[at]
    reveal(m.path, m.line, 'center')
  }

  /** File at the top of the visible area: the one `]`, `[` and `v` target (keys.svelte.ts). */
  function current(): number {
    const heads = [...(root?.querySelectorAll<HTMLElement>('section.file') ?? [])]
    const top = ((root && scroller(root)) ?? root)?.getBoundingClientRect().top ?? 0
    let i = 0
    heads.forEach((h, n) => {
      if (h.getBoundingClientRect().top <= top + 40) i = n
    })
    return i
  }
  function onkeydown(e: KeyboardEvent) {
    if ((e.target as HTMLElement | null)?.closest?.('textarea, input')) return
    if (e.key === ']' || e.key === '[') {
      const i = current() + (e.key === ']' ? 1 : -1)
      const x = files[Math.max(0, Math.min(files.length - 1, i))]
      if (x) jump(x.path)
      e.preventDefault()
    } else if ((e.key === 'n' || e.key === 'N') && !e.metaKey && !e.ctrlKey && !e.altKey) {
      goMessage(e.key === 'n' ? 1 : -1)
      e.preventDefault()
    } else if (e.key === 'v' && !e.metaKey && !e.ctrlKey) {
      const x = files[current()]
      if (x) setViewed(x.path, x.hash, !isViewed(x.path, x.hash))
      e.preventDefault()
    }
  }
</script>

<svelte:window onkeydown={(e) => root?.closest('aside[data-kbd]') && onkeydown(e)} onpointermove={dragOver} onpointerup={() => (dragging = null)} />

<!-- A code line: highlighted tokens, the changed part emphasised. -->
{#snippet code(hl: string, i: number, lines: DiffLine[], marks: Map<number, Mark>)}
  {@const l = lines[i]}
  <span class="sign">{l.kind === 'add' ? '+' : l.kind === 'del' ? '−' : ' '}</span>{#each pieces(colors.get(hl)?.[i], l.text, marks.get(i)) as p, n (n)}<span style:color={p.color} class:it={p.italic} class:bd={p.bold} class:chg={p.changed}>{p.text}</span>{/each}
{/snippet}

<!-- The gutter "+", to comment on the line (a note to the agent: shift-click extends it to a range). -->
{#snippet plus(path: string, l: DiffLine)}
  {#if (oncomment && !l.extra) || review}<button class="plus" class:note={!!review} onpointerdown={(e) => pressPlus(e, path, l)} onclick={(e) => (e.detail === 0 || phone) && pick(path, l, e.shiftKey)} title={review ? t('Note to the agent on this line (drag, or shift-click another line: a range)') : t('Comment on this line (drag, or shift-click another line: several)')} aria-label={review ? t('Note on line {n}', { n: l.new ?? l.old ?? '' }) : t('Comment on line {n}', { n: l.new ?? l.old ?? '' })}>+</button>{/if}
{/snippet}

<!-- A note to the agent: a draft (editable), or sent (changed since, or not), with its range. -->
{#snippet note(n: ReviewHooks['notes'][number])}
  {#if drafting?.edit === n.id}
    <CommentBox label={t('Save note')} value={n.body} accent="var(--src-claude)" autofocus onsend={async (body) => review?.edit(n.id, body)} oncancel={() => (drafting = null)} />
  {:else}
    <div class="rnote" class:sent={n.status === 'sent'}>
      <div class="rh">
        <b>{n.status === 'draft' ? t('Note') : tc('mrs', 'Sent')}</b><span class="mono">{range(n)}{n.side === 'old' ? ` ${t('(removed)')}` : ''}</span>
        <span class="grow"></span>
        {#if n.status === 'draft'}
          <button class="link" onclick={() => (drafting = { path: n.path, side: n.side, from: n.from, to: n.to, edit: n.id })}>{t('Edit')}</button>
          <button class="link" onclick={() => review?.remove(n.id)} title={t('Remove this note')}>{tc('mrs', 'Remove')}</button>
        {:else}
          {#if n.changed}<span class="rtag changed" title={t('The agent has changed these lines since')}>{tc('mrs', 'changed since')}</span>{:else}<span class="rtag">{t('sent {time}', { time: hhmm(n.sentAt) })}</span>{/if}
          <button class="link" onclick={() => review?.resolve(n.id)} title={t('Fold it; forgotten at the next commit')}>{t('Resolve')}</button>
        {/if}
      </div>
      <div class="rb">{n.body}</div>
    </div>
  {/if}
{/snippet}

<!-- Under a line: its threads, then the comment in progress. -->
{#snippet extras(path: string, f: FileDiff, ls: DiffLine[], span: number)}
  {@const here = ls.length === 1 ? threadsAt(path, ls[0]) : [...new Set(ls.flatMap((l) => threadsAt(path, l)))]}
  {#if here.length}
    <tr class="inline"><td colspan={span}>
      {#each here as d (d.id)}<MrThread discussion={d} {me} {forge} {people} compact onreply={(body) => onreply?.(d.id, body) ?? Promise.resolve()} onresolve={(r) => onresolve?.(d.id, r) ?? Promise.resolve()} />{/each}
    </td></tr>
  {/if}
  {#if review}
    {@const mine = ls.flatMap((l) => notesAt(path, l))}
    {#if mine.length}
      <tr class="inline"><td colspan={span}>{#each mine as n (n.id)}{@render note(n)}{/each}</td></tr>
    {/if}
    {#if drafting && !drafting.edit && drafting.path === path && ls.some((l) => sideOf(l) === drafting!.side && numOf(l) === drafting!.to)}
      {@const d = drafting}
      <tr class="inline"><td colspan={span}>
        <CommentBox label={t('Add note')} accent="var(--src-claude)" starters={STARTERS} placeholder={keys(t('Note to the agent on {range}… (⌘↵ to add, Esc to cancel; shift-click a line for a range)', { range: range(d) }))} autofocus onsend={async (body) => review?.add({ path, side: d.side, from: d.from, to: d.to, quote: quoteOf(path, d.side, d.from, d.to), body })} oncancel={() => (drafting = null)} />
      </td></tr>
    {/if}
  {/if}
  {@const r = composed(path)}
  {#if r}
    {@const lines = linesOf(path)}
    {#if ls.includes(lines[r[1]])}
      <tr class="inline"><td colspan={span}>
        <CommentBox {forge} {people} label={tc('action', 'Comment')} placeholder={keys(r[0] === r[1] ? t('Comment on line {n}… (⌘↵ to send, Esc to cancel; shift-click a line for several)', { n: labelOf(lines[r[0]]) }) : t('Comment on lines {from}–{to}… (⌘↵ to send, Esc to cancel)', { from: labelOf(lines[r[0]]), to: labelOf(lines[r[1]]) }))} autofocus onsend={(body) => oncomment?.(refOf(f, lines, r), body) ?? Promise.resolve()} oncancel={() => (composing = null)} />
      </td></tr>
    {/if}
  {/if}
{/snippet}

<!-- A file's diff: unified, or side by side. With its preview, a row lights its block. -->
{#snippet table(x: Entry)}
  {@const { f, lines, path, key, marks, rows } = x}
  {@const hover = viewOf(x) === 'both' && previews.has(key) && !f.new_file}
  {#if mode === 'unified'}
    <table class="fbody" data-path={path} style:--pick={review ? undefined : 'var(--src-gitlab)'} onmouseleave={hover ? () => hoverRow(key, null) : undefined}>
      <colgroup><col class="cn" /><col class="cn" /><col /></colgroup>
      <tbody>
        {#each lines as l, i (i)}
          {#if l.kind === 'hunk'}
            <tr class="hunk"><td colspan="3" class="mono">{@render more(x, x.gapAt.get(i))}{l.text}{@render hunkActs(path, lines, i)}</td></tr>
          {:else}
            <tr class={l.kind} class:lit={isLit(key, i)} class:picked={review ? inDraft(path, l) : inComposing(path, l)} data-line={l.new ?? l.old} data-row={i} data-i={i} onmouseenter={hover ? () => hoverRow(key, i) : undefined}>
              <td class="n">{@render plus(path, l)}{l.old ?? ''}</td>
              <td class="n">{l.new ?? ''}</td>
              <td class="code">{@render code(x.hl, i, lines, marks)}</td>
            </tr>
            {@render extras(path, f, [l], 3)}
          {/if}
        {/each}
        {@render tail(x, 3)}
      </tbody>
    </table>
  {:else}
    <table class="fbody split" data-path={path} style:--pick={review ? undefined : 'var(--src-gitlab)'} onmouseleave={hover ? () => hoverRow(key, null) : undefined}>
      <colgroup><col class="cn" /><col /><col class="cn" /><col /></colgroup>
      <tbody>
        {#each rows as r, n (n)}
          {#if r.hunk}
            <tr class="hunk"><td colspan="4" class="mono">{@render more(x, x.gapAt.get(r.at ?? -1))}{r.hunk}{@render hunkActs(path, lines, r.at ?? 0)}</td></tr>
          {:else}
            {@const left = r.left !== null ? lines[r.left] : undefined}
            {@const right = r.right !== null ? lines[r.right] : undefined}
            <tr class:lit={isLit(key, r.right) || isLit(key, r.left)} class:picked={[left, right].some((x) => !!x && (review ? inDraft(path, x) : inComposing(path, x)))} data-line={right?.new ?? left?.old} data-row={r.right ?? r.left} onmouseenter={hover ? () => hoverRow(key, r.right ?? r.left) : undefined}>
              {#if left}
                <td class="n {left.kind}" data-i={r.left}>{@render plus(path, left)}{left.old}</td>
                <td class="code {left.kind} side" data-i={r.left}>{@render code(x.hl, r.left!, lines, marks)}</td>
              {:else}
                <td class="n empty"></td><td class="code empty"></td>
              {/if}
              {#if right}
                <td class="n {right.kind}" data-i={r.right}>{#if right !== left}{@render plus(path, right)}{/if}{right.new}</td>
                <td class="code {right.kind} side" data-i={r.right}>{@render code(x.hl, r.right!, lines, marks)}</td>
              {:else}
                <td class="n empty"></td><td class="code empty"></td>
              {/if}
            </tr>
            {@render extras(path, f, [...new Set([left, right].filter((x) => !!x))] as DiffLine[], 4)}
          {/if}
        {/each}
        {@render tail(x, 4)}
      </tbody>
    </table>
  {/if}
{/snippet}

<!-- On a hunk's header, its gap's hidden lines: ↑ those over it, ↓ those under the hunk above, or all. -->
{#snippet more(x: Entry, k: number | undefined)}
  {@const rest = k === undefined ? 0 : x.left[k]}
  {#if k !== undefined && rest !== 0}
    {@const err = gapError(x)}
    <span class="more" title={err ?? undefined}>
      {#if err}
        <button disabled aria-label={err}><Icon name="chevronUp" /></button>
      {:else if rest !== null && rest <= STEP}
        <button onclick={() => expand(x, k, 'all')} title={tn(rest, 'Show the {n} hidden line', 'Show the {n} hidden lines')}><Icon name="chevronUp" /><Icon name="chevronDown" /> {rest}</button>
      {:else}
        <button onclick={() => expand(x, k, 'up')} title={t('Show {n} more lines above', { n: STEP })}><Icon name="chevronUp" /></button>
        {#if k > 0}<button onclick={() => expand(x, k, 'down')} title={t('Show {n} more lines below the hunk above', { n: STEP })}><Icon name="chevronDown" /></button>{/if}
        {#if rest !== null}<button onclick={() => expand(x, k, 'all')} title={tn(rest, 'Show the {n} hidden line', 'Show the {n} hidden lines')}>{t('all {n}', { n: rest })}</button>{/if}
      {/if}
    </span>
  {/if}
{/snippet}

<!-- After the last hunk: the rest of the file, unknown until read. -->
{#snippet tail(x: Entry, span: number)}
  {@const k = x.gaps.length - 1}
  {@const rest = k >= 0 ? x.left[k] : 0}
  {#if k >= 0 && rest !== 0}
    {@const err = gapError(x)}
    <tr class="hunk end"><td colspan={span} class="mono">
      <span class="more" title={err ?? undefined}>
        {#if err}
          <button disabled aria-label={err}><Icon name="chevronDown" /></button><span class="muted">{err}</span>
        {:else}
          <button onclick={() => expand(x, k, 'down')} title={t('Show {n} more lines below', { n: STEP })}><Icon name="chevronDown" /></button>
          {#if rest !== null}<button onclick={() => expand(x, k, 'all')} title={tn(rest, 'Show the {n} hidden line', 'Show the {n} hidden lines')}>{t('all {n}', { n: rest })}</button>{/if}
        {/if}
      </span>
    </td></tr>
  {/if}
{/snippet}

<!-- On a hunk's header, for an agent's diff: a note on the whole hunk. -->
{#snippet hunkActs(path: string, lines: DiffLine[], at: number)}
  {#if review && hunkRange(lines, at)}
    <span class="hacts">{#each HUNK_ACTIONS as [label, title, body] (label)}<button onclick={() => hunkNote(path, lines, at, body)} {title}>{label}</button>{/each}</span>
  {/if}
{/snippet}

<!-- A Markdown file rendered, its changed blocks marked. -->
{#snippet preview(x: Entry)}
  {@const p = previews.get(x.key)}
  {@const d = docs.get(x.key)}
  {#if p}
    <!-- A new file is all added: no marks, they would only repeat it. -->
    <div class="rendered"><Markdown text="" parsed={p.blocks} {forge} marks={x.f.new_file ? undefined : marksFor(x, p)} /></div>
  {:else if d?.error}
    <p class="muted pad">{t('No preview: {error}', { error: d.error })}</p>
  {:else}
    <p class="muted pad">Reading {x.f.new_path}…</p>
  {/if}
{/snippet}

<!-- An image file: its former version and its new one, side by side. -->
{#snippet images(x: Entry)}
  {@const p = pics.get(x.path)}
  <div class="fbody pics">
    {#each [[true, t('Before'), p?.old], [false, t('After'), p?.now]] as const as [old, label, pic] (label)}
      {#if old ? !x.f.new_file : !x.f.deleted_file}
        <figure class="pic" class:old class:now={!old}>
          <figcaption><b>{label}</b>{#if pic?.w}<span class="mono">{pic.w} × {pic.h}</span>{/if}</figcaption>
          {#if pic?.url}
            <div class="checker"><img src={pic.url} alt="{label}: {old ? x.f.old_path : x.f.new_path}" onload={(e) => measured(x.path, old, e.currentTarget as HTMLImageElement)} /></div>
          {:else if pic?.error}
            <p class="muted">{t('Not shown: {error}', { error: pic.error })}</p>
          {:else}
            <p class="muted">{t('Reading…')}</p>
          {/if}
        </figure>
      {/if}
    {/each}
  </div>
{/snippet}

<!-- A file in the left list: flat (with its folder) or in the tree. -->
{#snippet item(x: Entry, depth: number, flat: boolean)}
  {@const v = isViewed(x.path, x.hash)}
  <div class="titem" class:viewed={v} style:padding-left={flat ? null : `${4 + depth * 12}px`}>
    <input type="checkbox" checked={v} onchange={(e) => setViewed(x.path, x.hash, e.currentTarget.checked)} title={t('Viewed')} aria-label={t('{path} viewed', { path: x.path })} />
    <button onclick={() => jump(x.path)} title={x.path}>
      <span class="base">{x.path.split('/').pop()}</span>
      {#if flat}<span class="dir">{x.path.split('/').slice(0, -1).join('/')}</span>{/if}
    </button>
    {#if threadCount(x.path)}<span class="threads"><Icon name="comment" /> {threadCount(x.path)}</span>{/if}
    <span class="add">+{x.stats.add}</span><span class="del">−{x.stats.del}</span>
  </div>
{/snippet}

<div class="diff" class:phone bind:this={root} style:--bar-h={barH ? `${barH}px` : null}>
  {#if phone}
    {@const x = files[shownFile]}
    <div class="bar pbar" bind:offsetHeight={barH}>
      <button class="nav" onclick={() => pickAt(shownFile - 1)} disabled={shownFile <= 0} aria-label={t('Previous file')}><Icon name="chevronLeft" /></button>
      <select class="pfile" value={shownFile} onchange={(e) => pickAt(Number(e.currentTarget.value))} aria-label={t('File')}>
        {#each files as y, i (y.path)}<option value={i}>{isViewed(y.path, y.hash) ? '✓ ' : ''}{i + 1}/{files.length} · {y.path.split('/').pop()}{threadCount(y.path) ? ` · ${threadCount(y.path)} 💬` : ''}</option>{/each}
      </select>
      <button class="nav" onclick={() => pickAt(shownFile + 1)} disabled={shownFile >= files.length - 1} aria-label={t('Next file')}><Icon name="chevronRight" /></button>
      {#if x}<label class="seen big"><input type="checkbox" checked={isViewed(x.path, x.hash)} onchange={(e) => seenAndNext(x, e.currentTarget.checked)} />{t('Viewed')}</label>{/if}
      <span class="psum"><span class="add">+{total.add}</span> <span class="del">−{total.del}</span> · {t('{seen}/{total} viewed', { seen, total: files.length })}{#if tests.length}{@const all = testsSeen === tests.length}{' '}· <button class="tests" class:on={all} onclick={testsAndNext} aria-pressed={all} title={all ? t('Uncheck the test files') : tn(tests.length, 'Mark the {n} test file as viewed, to review the code first', 'Mark the {n} test files as viewed, to review the code first')}><Icon name="check" /> {t('Tests')} <b>{testsSeen} / {tests.length}</b></button>{/if}{#if messages.length}{' '}· <button class="link" onclick={() => goMessage(1)}><Icon name="comment" /> {messages.length}</button>{/if}</span>
    </div>
  {:else}
  <div class="bar" bind:offsetHeight={barH}>
    <span class="sum"><b>{diff.files.length}</b> file{diff.files.length > 1 ? 's' : ''} <span class="add">+{total.add}</span> <span class="del">−{total.del}</span></span>
    <span class="progress" title={t('Files marked as viewed (v)')}><i style="width: {files.length ? (100 * seen) / files.length : 0}%"></i></span>
    <span class="muted">{seen} / {files.length} viewed</span>
    {#if tests.length}
      {@const all = testsSeen === tests.length}
      <button class="tests" class:on={all} onclick={toggleTests} aria-pressed={all} title={all ? t('Uncheck the test files') : tn(tests.length, 'Mark the {n} test file as viewed, to review the code first', 'Mark the {n} test files as viewed, to review the code first')}><Icon name="check" /> {t('Tests')} <b>{testsSeen} / {tests.length}</b></button>
    {/if}
    {#if diff.truncated}<span class="warn">· {t('too large: the last files are not loaded')}</span>{/if}
    <span class="grow"></span>
    {#if messages.length}
      <span class="seg msgs" role="group" aria-label={t('Messages')}>
        <button onclick={() => goMessage(-1)} title={t('Previous message (⇧N)')} aria-label={t('Previous message')}><Icon name="chevronLeft" /></button>
        <span class="pos" title={review ? t('Notes in this diff') : t('Threads in this diff')}><Icon name="comment" /> {at >= 0 && at < messages.length ? `${at + 1} / ` : ''}{messages.length}</span>
        <button onclick={() => goMessage(1)} title={t('Next message (n)')}>{t('Next message')} <Icon name="chevronRight" /></button>
      </span>
    {/if}
    <button class="link files-btn" class:on={showFiles} onclick={toggleFiles} aria-pressed={showFiles}>{t('Files')}</button>
    <span class="seg" role="group" aria-label={t('File list layout')}>
      <button class:on={nav === 'list'} onclick={() => setNav('list')} title={t('Flat file list')}>{t('List')}</button>
      <button class:on={nav === 'tree'} onclick={() => setNav('tree')} title={t('Files as a folder tree')}>{t('Tree')}</button>
    </span>
    <button class="link" onclick={() => foldAll(false)}>{t('Expand all')}</button>
    <button class="link" onclick={() => foldAll(true)}>{t('Collapse all')}</button>
    <span class="seg" role="group" aria-label={t('Diff layout')}>
      <button class:on={mode === 'unified'} onclick={() => setMode('unified')}>{t('Unified')}</button>
      <button class:on={mode === 'split'} onclick={() => setMode('split')}>{t('Split')}</button>
    </span>
  </div>
  {/if}

  <div class="layout">
    {#if !phone}
    <nav class="tree" class:shown={showFiles} aria-label={t('Files')}>
      {#if nav === 'list'}
        {#each files as x (x.path)}
          {@render item(x, 0, true)}
        {/each}
      {:else}
        {#each treeRows as r ('dir' in r ? `d:${r.dir.path}` : `f:${r.file.path}`)}
          {#if 'dir' in r}
            <button class="tdir" style:padding-left="{4 + r.depth * 12}px" onclick={() => toggleDir(r.dir.path)} aria-expanded={!shut.has(r.dir.path)} title={r.dir.path}>
              <span class="chev"><Icon name={shut.has(r.dir.path) ? 'chevronRight' : 'chevronDown'} /></span>
              <span class="dname">{r.dir.name}</span>
              <span class="add">+{r.dir.add}</span><span class="del">−{r.dir.del}</span>
            </button>
          {:else}
            {@render item(r.file, r.depth, false)}
          {/if}
        {/each}
      {/if}
    </nav>
    {/if}

    <div class="files">
      {#each files as x, rank (x.path)}
        {@const { f, lines, stats, path, hash: h, key, marks, rows } = x}
        {@const v = isViewed(path, h)}
        {#if !phone || rank === shownFile}
        <section class="file" class:viewed={v} data-file={path} use:lazy={path}>
          <div class="fhead">
            <button class="fold" onclick={() => toggle(path)} aria-expanded={!folded.has(path)}>
              <span class="chev"><Icon name={folded.has(path) ? 'chevronRight' : 'chevronDown'} /></span>
              {#if f.new_file}<span class="tag new">{t('new')}</span>{:else if f.deleted_file}<span class="tag gone">{t('deleted')}</span>{:else if f.renamed_file}<span class="tag">{t('renamed')}</span>{/if}
              <span class="path mono" title={f.renamed_file ? `${f.old_path} → ${f.new_path}` : path}>{f.renamed_file ? `${f.old_path} → ${f.new_path}` : path}</span>
              <span class="add">+{stats.add}</span><span class="del">−{stats.del}</span>
              {#if threadCount(path)}<span class="threads" title={t('Threads on this file')}><Icon name="comment" /> {threadCount(path)}</span>{/if}
              {#if noteCount(path)}<span class="threads notes" title={t('Notes to the agent on this file')}><Icon name="comment" /> {noteCount(path)}</span>{/if}
            </button>
            {#if previewable(x)}
              <span class="seg md-view" role="group" aria-label={t('Markdown view')}>
                <button class:on={mdView === 'diff'} onclick={() => setMdView('diff')} title={t('The diff only')}>{t('Diff')}</button>
                <button class:on={mdView === 'both'} onclick={() => setMdView('both')} title={t('The diff and the rendered file, side by side')}>{t('Both')}</button>
                <button class:on={mdView === 'preview'} onclick={() => setMdView('preview')} title={t('The rendered file only')}>{t('Preview')}</button>
              </span>
            {/if}
            <label class="seen"><input type="checkbox" checked={v} onchange={(e) => setViewed(path, h, e.currentTarget.checked)} />{t('Viewed')}</label>
          </div>
          {#if !folded.has(path)}
            {#each orphans(x) as n (n.id)}<div class="orphan"><span class="muted">{t('No longer in the diff ({path}, {range}):', { path: n.path, range: range(n) })}</span>{@render note(n)}</div>{/each}
            {#if isImage(x) && (f.collapsed || !lines.length)}
              {#if near.has(path)}{@render images(x)}{:else}<div class="fbody ghost" style:height="200px"></div>{/if}
            {:else if f.collapsed}
              <p class="muted pad">{t('No preview (binary, too large, or beyond the size limit) — open it in {forge}.', { forge: forgeName() })}</p>
            {:else if !near.has(path)}
              <div class="fbody ghost" style:height="{ghost(x)}px"></div>
            {:else if viewOf(x) === 'diff'}
              {@render table(x)}
            {:else}
              <div class="fbody md-body {viewOf(x)}">
                {#if viewOf(x) === 'both'}<div class="pane">{@render table(x)}</div>{/if}
                <div class="pane render">{@render preview(x)}</div>
              </div>
            {/if}
          {/if}
        </section>
        {/if}
      {:else}
        <p class="muted">{t('No change.')}</p>
      {/each}
    </div>
  </div>
</div>

<style>
  .diff { display: flex; flex-direction: column; gap: 10px; container-type: inline-size; }
  /* The phone: one file at a time, its bar a stepper. */
  .pbar { gap: 6px 8px; }
  .pbar .nav { display: inline-flex; padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); color: var(--text); }
  .pbar .nav:disabled { opacity: 0.35; }
  .pfile { flex: 1; min-width: 0; padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); color: var(--text); font: inherit; font-size: 13px; }
  .seen.big { font-size: 13px; }
  .seen.big input { width: 18px; height: 18px; }
  .psum { flex-basis: 100%; color: var(--text-muted); font-size: 12px; }
  .psum .tests { padding: 3px 8px; font-size: 12px; }
  .psum .link { display: inline-flex; align-items: center; gap: 3px; padding: 0; border: 0; background: none; color: var(--info); font: inherit; }
  .phone td.code { font-size: 11.5px; }
  .phone col.cn { width: 30px; }
  /* Up against the scrolling area's edges, whatever its padding (`--diff-pad-y`, `--diff-pad-x`: the MR drawer's by default). */
  .bar { position: sticky; top: calc(-1 * var(--diff-pad-y, 12px)); z-index: 3; display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px; margin: calc(-1 * var(--diff-pad-y, 12px)) calc(-1 * var(--diff-pad-x, 16px)) 0; padding: 8px var(--diff-pad-x, 16px); background: var(--surface); border-bottom: 1px solid var(--border); font-size: var(--fs-meta); }
  .msgs .pos { display: inline-flex; align-items: center; gap: 4px; padding: 0 6px; color: var(--text-muted); white-space: nowrap; }
  .msgs :global(svg) { width: 12px; height: 12px; vertical-align: -2px; }
  .sum { color: var(--text-muted); }
  .sum b { color: var(--text); }
  .progress { width: 70px; height: 5px; border-radius: var(--r3); background: var(--surface-muted); overflow: hidden; }
  .progress i { display: block; height: 100%; background: var(--ok); }
  .grow { flex: 1; }
  .tests { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; cursor: pointer; }
  .tests:hover { color: var(--text); }
  .tests.on { border-color: color-mix(in srgb, var(--ok) 55%, var(--border)); color: var(--ok); }
  .tests b { font-weight: 500; }
  .tests :global(svg) { width: 11px; height: 11px; }
  .link { border: none; background: none; padding: 0; color: var(--info); font: inherit; cursor: pointer; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { border: none; background: none; padding: 1px 9px; color: var(--text-muted); font: inherit; cursor: pointer; }
  .seg button + button { border-left: 1px solid var(--border); }
  .seg button.on { background: var(--surface-muted); color: var(--text); font-weight: 500; }
  .add { color: var(--ok); font-family: var(--mono); font-size: var(--fs-tag); }
  .del { color: var(--ko); font-family: var(--mono); font-size: var(--fs-tag); }
  .warn { color: var(--warn); }
  .muted { color: var(--text-muted); }
  .mono { font-family: var(--mono); }

  /* File list on the left when the drawer is wide. */
  .layout { display: grid; grid-template-columns: 1fr; gap: 12px; align-items: start; }
  .tree { display: none; flex-direction: column; gap: 1px; overflow: auto; font-size: var(--fs-meta); }
  /* Narrow: above the diff, on demand ("Files"). */
  .tree.shown { display: flex; max-height: 40vh; padding: 4px; border: 1px solid var(--border); border-radius: var(--r8); }
  .link.files-btn.on { font-weight: 600; }
  @container (min-width: 900px) {
    .layout { grid-template-columns: 230px minmax(0, 1fr); }
    .tree, .tree.shown { position: sticky; top: calc(var(--bar-h, 38px) - var(--diff-pad-y, 12px) + 4px); display: flex; max-height: calc(100vh - 140px); padding: 0; border: none; }
    .files-btn { display: none; }
  }
  .titem { display: flex; align-items: center; gap: 5px; padding: 2px 4px; border-radius: var(--r5); }
  .titem:hover { background: var(--surface-muted); }
  .titem button { flex: 1; min-width: 0; display: flex; flex-direction: column; border: none; background: none; padding: 0; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .titem .base { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: var(--fs-tag); }
  .titem .dir { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; color: var(--text-muted); font-size: 9.5px; }
  .titem.viewed button { opacity: 0.5; }
  .tdir { display: flex; align-items: center; gap: 5px; padding: 2px 4px; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; text-align: left; cursor: pointer; }
  .tdir:hover { background: var(--surface-muted); }
  .tdir .dname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: var(--fs-tag); }
  .titem input, .seen input { --tick: var(--ok); margin: 0; }

  .files { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  /* `clip` and not `hidden`: the sticky header follows the drawer's scroll, not the block's. */
  .file { border: 1px solid var(--border); border-radius: var(--r8); overflow: clip; }
  .file.viewed { opacity: 0.75; }
  /* Under the bar, whatever its height (the bar sticks at the area's very edge, `top` counts from inside its padding). */
  .fhead { position: sticky; top: calc(var(--bar-h, 38px) - var(--diff-pad-y, 12px)); z-index: 2; display: flex; align-items: center; gap: 8px; padding-right: 10px; border-bottom: 1px solid var(--border); background: var(--surface-muted); }
  .fold { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; padding: 5px 10px; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .seen { display: inline-flex; align-items: center; gap: 4px; font-size: var(--fs-tag); color: var(--text-muted); cursor: pointer; white-space: nowrap; }
  .chev { color: var(--text-muted); }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; font-size: var(--fs-mono); }
  .tag { padding: 0 5px; border-radius: var(--r4); font-size: var(--fs-tag); background: var(--surface); color: var(--text-muted); }
  .tag.new { background: var(--ok-bg); color: var(--ok); }
  .tag.gone { background: var(--ko-bg); color: var(--ko); }
  .threads { font-size: var(--fs-tag); color: var(--info); white-space: nowrap; }
  .pad { margin: 0; padding: 8px 10px; font-size: var(--fs-meta); }
  .ghost { background: repeating-linear-gradient(transparent 0 16.5px, color-mix(in srgb, var(--text-muted) 4%, transparent) 16.5px 33px); }

  table { width: 100%; border-collapse: collapse; table-layout: fixed; font-family: var(--mono); font-size: 11px; line-height: 1.5; }
  td { padding: 0 6px; vertical-align: top; }
  col.cn { width: 42px; }
  td.n { position: relative; text-align: right; color: var(--text-muted); user-select: none; }
  td.code { white-space: pre-wrap; overflow-wrap: anywhere; color: var(--shiki-foreground, var(--text)); }
  /* Gutter "+": visible on line hover. */
  .plus { position: absolute; left: 2px; top: 0; width: 16px; height: 16px; padding: 0; border: none; border-radius: var(--r4); background: var(--src-gitlab); color: #fff; font: inherit; font-size: 12px; line-height: 16px; cursor: pointer; opacity: 0; }
  tr:hover .plus, td.n:hover .plus, .plus:focus-visible { opacity: 1; }
  .it { font-style: italic; }
  .bd { font-weight: 600; }
  .sign { display: inline-block; width: 12px; color: var(--text-muted); user-select: none; }
  tr.add td, td.add { background: color-mix(in srgb, var(--ok) 12%, transparent); }
  tr.del td, td.del { background: color-mix(in srgb, var(--ko) 12%, transparent); }
  tr.add .sign, td.add .sign { color: var(--ok); }
  tr.del .sign, td.del .sign { color: var(--ko); }
  /* The changed part of a modified line, stronger than the line. */
  tr.add .chg, td.add .chg { background: color-mix(in srgb, var(--ok) 32%, transparent); border-radius: var(--r2); }
  tr.del .chg, td.del .chg { background: color-mix(in srgb, var(--ko) 32%, transparent); border-radius: var(--r2); }
  /* A comment's grey vanishes on that stronger color (Nord: 1.2:1): there, the text's own color. */
  .chg { --shiki-token-comment: var(--shiki-foreground, var(--text)); }
  td.empty { background: repeating-linear-gradient(-45deg, transparent 0 4px, color-mix(in srgb, var(--text-muted) 7%, transparent) 4px 8px); }
  /* Side by side: a rule separates the two versions. */
  .split td.n:nth-child(3) { border-left: 1px solid var(--border); }
  tr.hunk td { padding: 2px 8px; background: color-mix(in srgb, var(--info) 8%, transparent); color: var(--text-muted); }
  /* A hunk's hidden lines, on demand. */
  .more { display: inline-flex; gap: 3px; margin-right: 8px; vertical-align: 1px; font-family: var(--sans); }
  .more button { display: inline-flex; align-items: center; gap: 1px; padding: 0 5px; border: 1px solid var(--border); border-radius: var(--r4); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 15px; cursor: pointer; }
  .more button:hover:not(:disabled) { border-color: var(--info); color: var(--info); }
  .more button:disabled { opacity: 0.45; cursor: default; }
  .more :global(svg) { width: 11px; height: 11px; }
  .more .muted { margin-left: 4px; font-size: var(--fs-tag); }
  /* An agent's diff: a note's "+", the lines being noted, the hunk's actions, the notes. */
  .plus.note { background: var(--src-claude); }
  /* The lines picked: for a note to the agent, or a comment on the forge (`--pick`). */
  tr.picked > td { box-shadow: inset 0 0 0 999px color-mix(in srgb, var(--pick, var(--src-claude)) 12%, transparent); }
  tr.picked > td:first-child { box-shadow: inset 3px 0 0 var(--pick, var(--src-claude)), inset 0 0 0 999px color-mix(in srgb, var(--pick, var(--src-claude)) 12%, transparent); }
  .hacts { display: none; float: right; gap: 4px; font-family: var(--sans); }
  tr.hunk:hover .hacts { display: inline-flex; }
  .hacts button { padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r9); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .hacts button:hover { border-color: var(--src-claude); color: var(--src-claude); }
  .threads.notes { color: var(--src-claude); }
  .rnote { border: 1px solid color-mix(in srgb, var(--src-claude) 45%, var(--border)); border-radius: var(--r7); background: color-mix(in srgb, var(--src-claude) 6%, var(--surface)); font-family: var(--sans); line-height: 1.45; }
  .rnote + .rnote, :global(.box) + .rnote { margin-top: 6px; }
  .rnote.sent { border-color: var(--border); background: var(--surface); }
  .rh { display: flex; align-items: center; gap: 6px; padding: 3px 8px 0; font-size: var(--fs-tag); color: var(--text-muted); }
  .rh b { color: var(--src-claude); }
  .rnote.sent .rh b { color: var(--text-muted); }
  .rb { padding: 2px 8px 6px; font-size: var(--fs-body); white-space: pre-wrap; overflow-wrap: anywhere; }
  .rtag { padding: 0 6px; border-radius: var(--r9); background: var(--surface-muted); font-size: 10px; }
  .rtag.changed { background: var(--info-bg); color: var(--info); }
  .orphan { display: flex; flex-direction: column; gap: 4px; padding: 6px 8px; border-bottom: 1px solid var(--border); font-size: var(--fs-tag); }
  tr.inline td { padding: 6px 8px 8px 96px; background: var(--bg); font-family: inherit; }
  tr.inline :global(.thread) { font-family: var(--sans); line-height: 1.45; }
  tr.inline :global(.thread + .thread) { margin-top: 6px; }

  /* Markdown file: its diff and its rendering side by side (each scrolls on its own), or the rendering alone. */
  .md-view { flex-shrink: 0; font-size: var(--fs-tag); }
  .md-view button { padding: 0 7px; }
  .md-body.both { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .md-body.both .pane { min-width: 0; max-height: 75vh; overflow: auto; }
  .md-body.both .pane + .pane { border-left: 1px solid var(--border); }
  @container (max-width: 760px) {
    .md-body.both { grid-template-columns: minmax(0, 1fr); }
    .md-body.both .pane + .pane { border-left: none; border-top: 1px solid var(--border); }
  }
  .rendered { padding: 10px 16px 14px 20px; }
  /* An image's two versions, side by side (stacked when narrow), on a checkerboard for transparency. */
  .pics { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(240px, 100%), 1fr)); gap: 10px; padding: 10px; }
  .pic { display: flex; flex-direction: column; gap: 6px; min-width: 0; margin: 0; padding: 6px; border-radius: var(--r6); }
  .pic.old { background: color-mix(in srgb, var(--ko) 10%, transparent); }
  .pic.now { background: color-mix(in srgb, var(--ok) 10%, transparent); }
  .pic figcaption { display: flex; align-items: baseline; gap: 8px; font-size: var(--fs-tag); color: var(--text-muted); }
  .pic.old b { color: var(--ko); }
  .pic.now b { color: var(--ok); }
  .pic p { margin: 0; font-size: var(--fs-meta); }
  .checker { display: flex; justify-content: center; border-radius: var(--r4); background: repeating-conic-gradient(color-mix(in srgb, var(--text-muted) 18%, transparent) 0 25%, transparent 0 50%) 0 0 / 16px 16px; }
  .checker img { display: block; max-width: 100%; max-height: 60vh; object-fit: contain; }
  /* A row whose block is lit in the rendering, or the reverse. */
  tr.lit > td { background-color: color-mix(in srgb, var(--accent) 18%, var(--surface)); }
</style>
