import { read, write } from './persist'
import { isDesktop } from './bridge'
import { live } from './live.svelte'
import { onShared, shareSave } from './shared.svelte'
import { agentsView, closeAgents } from './agents.svelte'
import { pageTransition } from './page-transition'
import { untrack } from 'svelte'
import { closeHelp } from './help-view.svelte'
import { closeSettings, saveEdits } from './settings.svelte'
import { toast } from './toast.svelte'
import { knownBlock, blockLabel, OPS_VIEWS, LAYOUT_VERSION, MAX_BANDS, MERGED_BLOCKS, RENAMED_VIEWS, RESET_VIEWS, bandRanges, compactBands, fitBands, viewById, viewRows, views, type BlockId, type View } from './views'
import { t } from './i18n.svelte'

/**
 * Layout remembered by the app: displayed view, and for each view the proportions of its blocks
 * (adjustable with the mouse) and the state of each block (normal, short, collapsed); plus the
 * session groups and the block settings. The blocks of a view are config.toml's (`columns`).
 */
interface Sizes {
  columns: number[]
  rows: number[][]
  /** Height of each band, when there are several. */
  bands?: number[]
}

/** Normal: all the content; short: a few lines at fixed height; collapsed: the title only. */
export type BlockMode = 'normal' | 'short' | 'collapsed'

interface Layout {
  /** Version of the app views at save time (see `LAYOUT_VERSION`). */
  version: number
  view: string
  views: Record<string, Sizes>
  /**
   * Blocks per column, per view, not yet in config.toml: "Edit layout"'s until written (and while
   * it is on, for an empty column to drop into), or a layout from before they were written there.
   * Absent: config.toml's.
   */
  arrangements: Record<string, BlockId[][]>
  /** Columns of each band of an arrangement (`arrangements`); absent, one band. */
  bands: Record<string, number[]>
  /** State of the blocks, by `<view>:<block>`; absent, normal. */
  modes: Record<string, BlockMode>
  /** Claude session groups opened (true) or collapsed (false) by hand, by name. */
  sessionGroups: Record<string, boolean>
  /** Display settings of the blocks (sprint filters, MR columns…), by key. */
  prefs: Record<string, unknown>
}


/** Default proportions of a layout: the view's if it has not changed shape. */
function defaultSizes(v: View, columns: BlockId[][], bands: number[]): Sizes {
  if (sameShape(v.columns, columns) && sameBands(fitBands(v.bands, v.columns.length), bands)) return structuredClone(v.sizes)
  return { columns: columns.map(() => 1), rows: columns.map((c) => c.map(() => 1)), ...(bands.length > 1 ? { bands: bands.map(() => 1) } : {}) }
}

const sameBands = (a: number[], b: number[]) => a.length === b.length && a.every((n, i) => n === b[i])

const sameShape = (a: unknown[][], b: unknown[][]) => a.length === b.length && a.every((c, i) => c.length === b[i].length)

/** Saved proportions only hold if they still have the shape of the layout (one band: no band heights needed). */
function fits(sizes: Sizes | undefined, columns: BlockId[][], bands: number[]): sizes is Sizes {
  return (
    !!sizes &&
    sizes.columns?.length === columns.length &&
    columns.every((col, i) => sizes.rows?.[i]?.length === col.length) &&
    (bands.length < 2 || sizes.bands?.length === bands.length)
  )
}

function load(): Layout {
  const saved = read<Partial<Layout> & { collapsed?: Record<string, boolean> }>('layout') ?? {}
  // Layout from before the views were split: the changed ones go back to default.
  if ((saved.version ?? 1) < LAYOUT_VERSION) {
    for (const id of RESET_VIEWS) {
      delete saved.arrangements?.[id]
      delete saved.views?.[id]
    }
  }
  // Views may come from config.toml, read afterwards: everything remembered is kept, by
  // view identifier, and matched to the view when it is displayed (`sizesOf`).
  const arrangements: Record<string, BlockId[][]> = {}
  for (const [id, cols] of Object.entries(saved.arrangements ?? {})) {
    // A block gone from the app is forgotten; an emptied column stays (it is removed in edit mode).
    // A merged block takes the place of the first of those it replaces.
    const seen = new Set<BlockId>()
    const a = cols?.map((c) => c.map((b) => MERGED_BLOCKS[b] ?? b).filter((b) => knownBlock(b) && !seen.has(b) && !!seen.add(b)))
    if (a?.length) arrangements[id] = a
  }
  const bands = Object.fromEntries(Object.entries(saved.bands ?? {}).filter(([id]) => arrangements[id]))
  const views = { ...(saved.views ?? {}) }
  // Old setting: blocks collapsed whatever the view.
  const modes: Record<string, BlockMode> = {}
  // A block's state follows the block into the view that now hosts it.
  const home = (block: string) => OPS_VIEWS.find((v) => v.columns.flat().includes(block as BlockId))?.id
  for (const [key, mode] of Object.entries(saved.modes ?? {})) {
    const [view, block] = key.split(':')
    const moved = RENAMED_VIEWS[view] ?? ((saved.version ?? 1) < LAYOUT_VERSION && RESET_VIEWS.includes(view) ? home(block) : undefined)
    modes[`${moved ?? view}:${block}`] = mode
  }
  if (!saved.modes && saved.collapsed) {
    for (const [id, on] of Object.entries(saved.collapsed)) if (on) for (const v of OPS_VIEWS) modes[`${v.id}:${id}`] = 'collapsed'
  }
  const view = RENAMED_VIEWS[saved.view ?? ''] ?? saved.view
  return {
    version: LAYOUT_VERSION,
    view: view ?? OPS_VIEWS[0].id,
    views,
    arrangements,
    bands,
    modes,
    sessionGroups: saved.sessionGroups ?? {},
    prefs: saved.prefs ?? {},
  }
}

export const layout = $state(load())

function save() {
  write('layout', layout)
}

export const currentView = () => viewById(layout.view)
/** Proportions of the view: remembered if they still have the shape of its columns, otherwise default. */
export function sizesOf(v: View): Sizes {
  const saved = layout.views[v.id]
  return fits(saved, columnsOf(v), bandsOf(v)) ? saved : defaultSizes(v, columnsOf(v), bandsOf(v))
}
/** Remembers the view's proportions before they are adjusted with the mouse (`shift` modifies them in place). */
export function ensureSizes(v: View) {
  if (fits(layout.views[v.id], columnsOf(v), bandsOf(v))) return
  layout.views[v.id] = defaultSizes(v, columnsOf(v), bandsOf(v))
  save()
}
/** Blocks of the view, by column: its modified layout, otherwise the default one. */
export const columnsOf = (v: View): BlockId[][] => layout.arrangements[v.id] ?? v.columns
/** Columns of each band of the view, top to bottom: of its modified layout, otherwise the default one. */
export const bandsOf = (v: View): number[] => fitBands(layout.arrangements[v.id] ? layout.bands[v.id] : v.bands, columnsOf(v).length)

/** Block temporarily enlarged to the whole view: never remembered, left when changing view. */
export const zoom = $state<{ id: string | null }>({ id: null })

export function toggleZoom(id: string) {
  zoom.id = zoom.id === id ? null : id
}

/**
 * Switching view: the grid slides in the rail's direction (`pageTransition`); from the Agents
 * view, which comes after the pages in the rail, as from a page below them.
 */
/** The next (`1`) or previous (`-1`) page of the rail; none beyond the first and the last. */
export function stepView(dir: 1 | -1) {
  const all = views()
  const next = all[all.findIndex((v) => v.id === layout.view) + dir]
  if (next) void setView(next.id)
}

export function setView(id: string): Promise<void> {
  const target = viewById(id).id
  // From the Agents view, the page it goes to comes from above it in the rail.
  const fromAgents = agentsView.open
  if (target === layout.view && !fromAgents) {
    zoom.id = null
    return Promise.resolve()
  }
  const apply = () => {
    closeAgents()
    zoom.id = null
    layout.view = target
    save()
  }
  const all = views()
  const from = fromAgents ? all.length : all.findIndex((v) => v.id === layout.view)
  const to = all.findIndex((v) => v.id === target)
  return pageTransition(apply, to > from ? 'down' : 'up')
}

/**
 * Goes to the first page showing this block (pages come from the config), from wherever one is:
 * the Agents view, Settings or Help close, as from the rail. Once there, the block is scrolled
 * into view and pointed out a moment. On no page: says so.
 */
/** Shows a block's page and scrolls to it; a block of another repository (`$extra-blocks`) by its id. */
export function showBlock(id: BlockId | (string & {})) {
  const block = id as BlockId
  const v = views().find((x) => columnsOf(x).flat().includes(block))
  if (!v) return toast(t('{block} is on no page: add it with Edit layout', { block: blockLabel(block) }))
  closeSettings()
  closeAgents()
  closeHelp()
  void setView(v.id).then(() => requestAnimationFrame(() => {
    const el = document.querySelector<HTMLElement>(`[data-block="${block}"]`)
    if (!el) return
    el.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
    el.classList.remove('flash')
    void el.offsetWidth
    el.classList.add('flash')
    el.addEventListener('animationend', () => el.classList.remove('flash'), { once: true })
  }))
}

/** Goes back to the default proportions of the displayed view. */
export function resetLayout() {
  const v = currentView()
  layout.views[v.id] = defaultSizes(v, columnsOf(v), bandsOf(v))
  save()
}

/** State of a block in the displayed view; an enlarged block is always normal. */
export function modeOf(id: string): BlockMode {
  if (zoom.id === id) return 'normal'
  return layout.modes[`${layout.view}:${id}`] ?? 'normal'
}
export const isCollapsed = (id: string) => modeOf(id) === 'collapsed'
export const isShort = (id: string) => modeOf(id) === 'short'

export function setMode(id: string, mode: BlockMode) {
  const key = `${layout.view}:${id}`
  if (mode === 'normal') delete layout.modes[key]
  else layout.modes[key] = mode
  save()
}

/* Free layout: move, add, remove blocks and columns in the displayed view. */

/** "Edit layout" mode: handles, drop targets, addable columns. Never remembered. */
export const editing = $state({ on: false })

export function toggleEditing() {
  editing.on = !editing.on
  zoom.id = null
  // When leaving edit mode, the columns left empty disappear, and the bands left with none.
  if (!editing.on) {
    const v = currentView()
    const cols = columnsOf(v)
    if (cols.some((c) => !c.length) && cols.some((c) => c.length)) {
      const c = compactBands(cols, bandsOf(v))
      const keptBands = bandRanges(bandsOf(v)).flatMap(([start, end], b) => (c.keep.some((k) => k >= start && k < end) ? [b] : []))
      arrange(v, c.columns, { bands: c.bands, from: c.keep, bandsFrom: keptBands })
    }
  }
}

/**
 * New layout of the view. Proportions follow as well as possible: each column keeps its
 * width, each block its height, each band its height; a new column, block or band takes an
 * average share. `bands`: the columns of each band (default: the current ones, the columns added
 * at the end going to the last); `from`, `bandsFrom`: the former index of each column, of each
 * band (default: the same; null for a new one).
 */
function arrange(v: View, next: BlockId[][], opts: { bands?: number[]; from?: (number | null)[]; bandsFrom?: (number | null)[] } = {}) {
  const prev = columnsOf(v)
  const sizes = sizesOf(v)
  const bands = fitBands(opts.bands ?? bandsOf(v), next.length)
  const avg = (xs: number[]) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : 1)
  const width = (c: number) => {
    const p = opts.from ? opts.from[c] : c
    return (p !== null && p !== undefined ? sizes.columns[p] : undefined) ?? avg(sizes.columns)
  }
  const heights = sizes.bands ?? bandsOf(v).map(() => 1)
  const height = (b: number) => {
    const p = opts.bandsFrom ? opts.bandsFrom[b] : b
    return (p !== null && p !== undefined ? heights[p] : undefined) ?? avg(heights)
  }
  const heightOf = (id: BlockId) => {
    for (const [c, col] of prev.entries()) {
      const r = col.indexOf(id)
      if (r !== -1) return sizes.rows[c]?.[r] ?? 1
    }
    return null
  }
  layout.views[v.id] = {
    columns: next.map((_, c) => width(c)),
    rows: next.map((col) => {
      const known = col.map(heightOf)
      const mean = avg(known.filter((h): h is number => h !== null))
      return known.map((h) => h ?? mean)
    }),
    ...(bands.length > 1 ? { bands: bands.map((_, b) => height(b)) } : {}),
  }
  layout.arrangements[v.id] = next
  if (bands.length > 1) layout.bands[v.id] = bands
  else delete layout.bands[v.id]
  save()
  writeArrangements()
}

/** A layout's blocks and bands without its empty columns, to compare with config.toml's. */
const filled = (cols: BlockId[][], bands: number[] | undefined) => {
  const c = compactBands(cols, fitBands(bands, cols.length))
  return JSON.stringify([c.columns, c.bands])
}
/** What was last written for each view: not written again (a write refused, a block config.toml drops). */
const written = new Map<string, string>()
let writeTimer: ReturnType<typeof setTimeout> | undefined

/**
 * The views whose blocks config.toml does not hold yet: written into its `[[views]]`, in one save,
 * a moment after the first change (snapshots, which call it too, come more often than that).
 */
function writeArrangements() {
  if (writeTimer) return
  writeTimer = setTimeout(async () => {
    writeTimer = undefined
    if (!isDesktop() || !live.snapshot) return
    const pending = Object.fromEntries(
      views()
        .map((v) => [v, layout.arrangements[v.id]] as const)
        .filter(([v, a]) => a && filled(a, layout.bands[v.id]) !== filled(v.columns, v.bands) && written.get(v.id) !== filled(a, layout.bands[v.id]))
        .map(([v, a]) => [v.id, a]),
    )
    if (!Object.keys(pending).length) return
    for (const [id, a] of Object.entries(pending)) written.set(id, filled(a, layout.bands[id]))
    try {
      await saveEdits([{ path: ['views'], value: viewRows(pending, $state.snapshot(layout.bands)) }])
    } catch (e) {
      toast(t('Layout not written to config.toml: {error}', { error: String(e) }))
    }
  }, 300)
}

// Once config.toml holds a view's blocks, they are read from it again; those it does not hold
// yet (a layout from before this version) are written there.
if (isDesktop()) {
  $effect.root(() => {
    $effect(() => {
      if (!live.snapshot || editing.on) return
      const all = views()
      untrack(() => {
        let dropped = false
        for (const v of all) {
          const a = layout.arrangements[v.id]
          if (a && filled(a, layout.bands[v.id]) === filled(v.columns, v.bands)) {
            delete layout.arrangements[v.id]
            delete layout.bands[v.id]
            dropped = true
          }
        }
        if (dropped) save()
        writeArrangements()
      })
    })
  })
}

/** Places `id` in column `column`, before `before` (at the end without `before`). */
export function moveBlock(id: BlockId, column: number, before: BlockId | null) {
  const v = currentView()
  const next = columnsOf(v).map((c) => c.filter((x) => x !== id))
  while (next.length <= column) next.push([])
  const at = before ? next[column].indexOf(before) : -1
  next[column].splice(at === -1 ? next[column].length : at, 0, id)
  arrange(v, next)
}

/** Adds a block to the view, in its least loaded column. */
export function addBlock(id: BlockId) {
  const v = currentView()
  const cols = columnsOf(v)
  if (cols.flat().includes(id)) return
  const target = cols.reduce((best, c, i) => (c.length < cols[best].length ? i : best), 0)
  arrange(v, cols.map((c, i) => (i === target ? [...c, id] : c)))
}

export function removeBlock(id: BlockId) {
  const v = currentView()
  arrange(v, columnsOf(v).map((c) => c.filter((x) => x !== id)))
}

/** Adds an empty column at the end of band `band` (the last one by default). */
export function addColumn(band?: number) {
  const v = currentView()
  const cols = columnsOf(v)
  const bands = bandsOf(v)
  const b = band ?? bands.length - 1
  const at = bandRanges(bands)[b][1]
  const from = cols.map((_, c) => c as number | null)
  from.splice(at, 0, null)
  arrange(v, [...cols.slice(0, at), [], ...cols.slice(at)], { bands: bands.map((n, i) => (i === b ? n + 1 : n)), from })
}

/** Adds a band below the others, with one empty column to drop blocks into. */
export function addBand() {
  const v = currentView()
  const bands = bandsOf(v)
  if (bands.length >= MAX_BANDS) return
  arrange(v, [...columnsOf(v), []], { bands: [...bands, 1], bandsFrom: [...bands.map((_, b) => b), null] })
}

/** Moves band `band` up (`-1`) or down (`1`), with its columns. */
export function moveBand(band: number, dir: -1 | 1) {
  const v = currentView()
  const bands = bandsOf(v)
  const to = band + dir
  if (to < 0 || to >= bands.length) return
  const order = bands.map((_, b) => b)
  ;[order[band], order[to]] = [order[to], order[band]]
  const ranges = bandRanges(bands)
  const from = order.flatMap((b) => Array.from({ length: bands[b] }, (_, i) => ranges[b][0] + i))
  const cols = columnsOf(v)
  arrange(v, from.map((c) => cols[c]), { bands: order.map((b) => bands[b]), from, bandsFrom: order })
}

/** Forgets the remembered layout of these views: they start again from the config's blocks (wizard). */
export function forgetViews(ids: string[]) {
  for (const id of ids) {
    delete layout.arrangements[id]
    delete layout.bands[id]
    delete layout.views[id]
    for (const key of Object.keys(layout.modes)) if (key.startsWith(`${id}:`)) delete layout.modes[key]
  }
  save()
}

/** Goes back to the view's equal proportions and normal blocks; its blocks stay config.toml's. */
export function resetArrangement() {
  const v = currentView()
  layout.views[v.id] = defaultSizes(v, columnsOf(v), bandsOf(v))
  for (const key of Object.keys(layout.modes)) if (key.startsWith(`${v.id}:`)) delete layout.modes[key]
  save()
}

/** Remembered setting of a block, or its default value. */
export function pref<T>(key: string, fallback: T): T {
  return (layout.prefs[key] as T | undefined) ?? fallback
}

export function setPref(key: string, value: unknown) {
  layout.prefs[key] = value
  save()
  if (sharedPref(key) && prefsRead) shareSave('prefs', sharedPrefs)
}

/**
 * The blocks' filters, sorts and groupings, the same on the computer and on a phone: kept in the
 * shared store `prefs` besides each page's own layout. Whoever changes one writes them all; the
 * other page takes them in, and its blocks show them when they open next. Not what belongs to
 * one screen: tabs, folds, widths, panes.
 */
const SHARED_PREFS = new Set([
  'agents.filter', 'agents.tag', 'agents.sortBy', 'agents.groupBy',
  'mrs.filter', 'mrs.repo', 'queue.repo', 'wip.filter', 'ci.scope', 'worktrees.filter', 'sprint.outside',
  'notes.tag', 'notes.repo', 'notes.showDone',
  'refinement.statuses', 'refinement.quarter', 'refinement.hideDone',
  'vulns.hideClean', 'vulns.hideIgnored', 'tokens.tool',
  // Not a filter: comments on an agent's plan or answers, written on one, sent from the other.
  'agents.comments',
  // Nor this: the agents and sessions archived, out of the lists on both.
  'agents.archived',
  // The worktrees archived, likewise.
  'worktrees.archived',
  // Alerts put off (⏾): off on both.
  'todo.snoozed',
])
/** And each ticket list's (`<block>.show`, `.priorities`, `.person`, `.group`, `.sort`, `.desc`), notes' and refinement's sort and grouping. */
const SHARED_SUFFIX = /\.(show|priorities|person|group|sort|desc)$/
export const sharedPref = (key: string) => SHARED_PREFS.has(key) || SHARED_SUFFIX.test(key)
const sharedPrefs = () => Object.fromEntries(Object.entries(layout.prefs).filter(([k]) => sharedPref(k)))

/** The computer starts from its own; a phone writes them only once it has read them (it would wipe the computer's). */
let prefsRead = isDesktop()
onShared('prefs', (value) => {
  prefsRead = true
  if (!value || typeof value !== 'object') return
  const theirs = value as Record<string, unknown>
  for (const k of Object.keys(layout.prefs)) if (sharedPref(k) && !(k in theirs)) delete layout.prefs[k]
  for (const [k, v] of Object.entries(theirs)) if (sharedPref(k)) layout.prefs[k] = v
  save()
})
// Never shared yet (before this version): the computer gives its own.
if (isDesktop()) {
  $effect.root(() => {
    $effect(() => {
      const snap = live.snapshot
      if (snap && !snap.shared_revs?.prefs && Object.keys(sharedPrefs()).length) shareSave('prefs', sharedPrefs)
    })
  })
}

/**
 * Zen mode: the rail and the page header fold away, the blocks get the whole window (the status
 * bar stays). Remembered with the layout; ⌘⇧F or View › Zen Mode. The header comes back while
 * the filter is used (`/`) or the For me panel is open (`i`).
 */
export const zen = $state({ on: pref('zen', false) })

export function toggleZen() {
  zen.on = !zen.on
  setPref('zen', zen.on)
}

/** Smallest size of a block, so that dragging too far does not make it disappear. */
const MIN_PX = 90

/**
 * Moves the boundary between `sizes[i]` and `sizes[i + 1]` by `delta` pixels,
 * `pixels` being the current displayed size of both elements.
 */
export function shift(sizes: number[], i: number, delta: number, pixels: [number, number]) {
  const [pa, pb] = pixels
  const total = pa + pb
  const next = Math.min(total - MIN_PX, Math.max(MIN_PX, pa + delta))
  const sum = sizes[i] + sizes[i + 1]
  sizes[i] = (sum * next) / total
  sizes[i + 1] = sum - sizes[i]
  save()
}
