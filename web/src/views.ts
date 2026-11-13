import { live } from './live.svelte'
import { BLOCK_DEFS, type PluginBlockId } from './blocks'
import type { ViewConfig } from './types'
import { t } from './i18n.svelte'

/**
 * App views (the rail's pages): each is a list of columns, each column a list
 * of blocks; the columns go in bands, rows of columns from top to bottom (`---` between two in
 * config.toml), one by default. They come from `[[views]]` in config.toml; without any, one empty Dashboard
 * (`EMPTY_VIEW`), and the pages wizard offers its templates. A new block of the core is declared
 * here; a block with a source of its own, in `blocks/`.
 */
export type BlockId = 'todo' | 'sessions' | 'worktrees' | 'notes' | 'mrs' | 'sprint' | 'pods' | 'ci' | 'vulns' | 'envs' | 'renovate' | 'refinement' | 'agenda' | 'pickup' | 'mail' | 'retro' | 'argocd' | 'queue' | 'wip' | 'routines' | 'aside' | PluginBlockId | CustomBlockId

/** A custom block of `[[custom_blocks]]` (`blocks/custom/`): `custom:<id>`, declared in config.toml, not here. */
export type CustomBlockId = `custom:${string}`
export const CUSTOM_PREFIX = 'custom:'
export const isCustom = (id: string): id is CustomBlockId => /^custom:[a-z0-9][a-z0-9_-]{0,39}$/.test(id)
/** The custom blocks the snapshot has, in the config's order. */
export const customBlocks = (): { id: string; label: string }[] =>
  ((live.snapshot?.blocks?.custom_blocks as { blocks?: { id: string; label: string }[] } | undefined)?.blocks ?? []).map((b) => ({ id: b.id, label: b.label }))
/** The custom blocks as page blocks (`custom:<id>`). */
export const customBlockIds = (): CustomBlockId[] => customBlocks().map((b) => `${CUSTOM_PREFIX}${b.id}` as CustomBlockId)
/** A block that can be on a page: one of Prism's, or any `custom:<id>` (it may come with the config, read after the layout). */
export const knownBlock = (id: string): id is BlockId => ALL_BLOCKS.includes(id as BlockId) || isCustom(id)
/** A block's name, translated: a custom block's own label (its id until the snapshot has it). */
export function blockLabel(id: string): string {
  if (isCustom(id)) {
    const own = id.slice(CUSTOM_PREFIX.length)
    return customBlocks().find((b) => b.id === own)?.label ?? own
  }
  return id in BLOCK_LABELS ? t(BLOCK_LABELS[id as BlockId]) : id
}

/** Name of each block, for the "+ Block" menu. */
export const BLOCK_LABELS: Record<BlockId, string> = {
  todo: 'Alerts', sessions: 'Agents', worktrees: 'Worktrees', notes: 'Notes', mrs: 'Merge requests', queue: 'Merge queue', renovate: 'Renovate', sprint: 'Sprint',
  agenda: 'Today', refinement: 'Refinement', pickup: 'To pick', pods: 'Pods', ci: 'CI health', vulns: 'Vulnerabilities', envs: 'Environments', argocd: 'ArgoCD',
  mail: 'Mail', retro: 'My sprint', wip: 'In progress', routines: 'Routines', aside: 'In passing',
  // The blocks of `blocks/`, the modules' included (`BLOCK_DEFS`).
  ...(Object.fromEntries(BLOCK_DEFS.map((b) => [b.id, b.label])) as Record<PluginBlockId, string>),
}
export const ALL_BLOCKS = Object.keys(BLOCK_LABELS) as BlockId[]

/** The groups of the "+ Block" menu, in this order; a block of `blocks/` names its own (`BlockDef.category`). */
export const BLOCK_CATEGORIES = ['Overview', 'Tickets', 'Code & agents', 'CI & delivery', 'Security', 'Infra', 'Data', 'Custom', 'Fun'] as const
const CORE_CATEGORY: Record<string, string> = {
  todo: 'Overview', notes: 'Overview', agenda: 'Overview', mail: 'Overview',
  sprint: 'Tickets', refinement: 'Tickets', pickup: 'Tickets', retro: 'Tickets', wip: 'Tickets',
  mrs: 'Code & agents', queue: 'Code & agents', worktrees: 'Code & agents', sessions: 'Code & agents', routines: 'Code & agents', aside: 'Code & agents',
  ci: 'CI & delivery', renovate: 'CI & delivery',
  vulns: 'Security',
  pods: 'Infra', envs: 'Infra', argocd: 'Infra',
}
/** A block's group in the "+ Block" menu: Prism's own, a block's declaration, else Other. */
export const blockCategory = (id: string): string => (isCustom(id) ? 'Custom' : (CORE_CATEGORY[id] ?? BLOCK_DEFS.find((b) => b.id === id)?.category ?? 'Other'))
/** Blocks by group: the known groups in their order, then the others by name, Other last. */
export function byCategory(ids: BlockId[]): { category: string; ids: BlockId[] }[] {
  const groups = new Map<string, BlockId[]>()
  for (const id of ids) groups.set(blockCategory(id), [...(groups.get(blockCategory(id)) ?? []), id])
  const rank = (c: string) => (c === 'Other' ? 1e3 : (BLOCK_CATEGORIES as readonly string[]).indexOf(c) + 1 || 500)
  return [...groups].sort(([a], [b]) => rank(a) - rank(b) || a.localeCompare(b)).map(([category, ids]) => ({ category, ids }))
}

/** Merged or renamed blocks: a saved layout replaces them with their successor. */
export const MERGED_BLOCKS: Record<string, BlockId> = {
  runners: 'pods',
  // The blocks' former ids (`BlockDef.aliases`).
  ...Object.fromEntries(BLOCK_DEFS.flatMap((b) => (b.aliases ?? []).map((a) => [a, b.id as BlockId]))),
}

/** Maximum columns of a band, in free layout. */
export const MAX_COLUMNS = 4
/** Maximum bands of a page. */
export const MAX_BANDS = 4
/** In a page's `columns`, what starts a new band below: `---`. */
export const BAND_BREAK = '---'
export const isBreak = (s: string) => /^-{3,}$/.test(s.trim())

export interface View {
  id: string
  label: string
  /** Rail icon: one of `VIEW_ICONS`. */
  icon: string
  /** Color of the view (rail, header): that of the source it mostly shows. */
  color: string
  /** Blocks per column, top to bottom: the default layout, which can be modified. */
  columns: BlockId[][]
  /** Columns of each band, top to bottom; absent, one band. */
  bands?: number[]
  /** Default proportions: width of each column, height of each block (and of each band). */
  sizes: { columns: number[]; rows: number[][]; bands?: number[] }
}

/**
 * Columns of each band, made to fit `n` columns: a band beyond them goes, the columns left over
 * go to the last one. Absent: one band.
 */
export function fitBands(bands: number[] | undefined, n: number): number[] {
  const out: number[] = []
  let left = n
  for (const b of bands ?? []) {
    const take = Math.min(b, left)
    if (take > 0) out.push(take)
    left -= take
  }
  if (left > 0) {
    if (out.length) out[out.length - 1] += left
    else out.push(left)
  }
  return out.length ? out : [n]
}

/** The `[start, end)` columns of each band. */
export function bandRanges(bands: number[]): [number, number][] {
  let at = 0
  return bands.map((b) => [at, (at += b)])
}

/** Without the empty columns, and the bands that had only those: what is kept (`keep`, the former indices) and how. */
export function compactBands(columns: BlockId[][], bands: number[]): { columns: BlockId[][]; bands: number[]; keep: number[] } {
  const keep: number[] = []
  const next: number[] = []
  for (const [start, end] of bandRanges(fitBands(bands, columns.length))) {
    const kept = Array.from({ length: end - start }, (_, i) => start + i).filter((c) => columns[c].length)
    keep.push(...kept)
    if (kept.length) next.push(kept.length)
  }
  return { columns: keep.map((c) => columns[c]), bands: next, keep }
}


/**
 * The full set of pages: where each block lived, for the layout's migrations and the icon and color
 * of a configured page that gives none.
 */
export const OPS_VIEWS: View[] = [
  {
    id: 'dashboard',
    icon: 'dashboard',
    color: 'var(--accent)',
    label: 'Dashboard',
    columns: [['todo', 'sessions', 'notes'], ['mrs', 'renovate'], ['sprint', 'agenda']],
    sizes: { columns: [3.4, 6.2, 2.8], rows: [[1, 2.2, 0.9], [2.4, 1], [1.6, 0.8]] },
  },
  {
    // Tickets to prepare (by EPIC) and those ready to be picked, side by side.
    id: 'tickets',
    icon: 'tickets',
    color: 'var(--src-clickup)',
    label: 'Tickets',
    columns: [['refinement'], ['pickup']],
    sizes: { columns: [1.3, 1], rows: [[1], [1]] },
  },
  {
    // CI health and pods (the runners'); a team's Delivery block joins it from its pages.
    id: 'ci',
    icon: 'ci',
    color: 'var(--src-gitlab)',
    label: 'CI',
    columns: [['ci', 'pods']],
    sizes: { columns: [1], rows: [[1.3, 1]] },
  },
  {
    // What runs: state of the environments, and vulnerabilities in dependencies and images.
    id: 'infra',
    icon: 'infra',
    color: 'var(--src-k8s)',
    label: 'Infra',
    columns: [['envs'], ['vulns']],
    sizes: { columns: [1.2, 1], rows: [[1], [1]] },
  },
  {
    // The primary repository's worktrees: what they are for, who works in them, which to clean up.
    // Out of the rail now (the Projects page's Cleanup tab); kept for the layouts saved with it.
    id: 'worktrees',
    icon: 'worktrees',
    color: 'var(--src-gitlab)',
    label: 'Worktrees',
    columns: [['worktrees']],
    sizes: { columns: [1], rows: [[1]] },
  },
]

/**
 * Merged or split views: a layout saved before `LAYOUT_VERSION` is carried over
 * into the view that takes its blocks (Refinement and To pick → Tickets; the old Infra
 * split between CI and Infra, reset to default).
 */
export const LAYOUT_VERSION = 2
export const RENAMED_VIEWS: Record<string, string> = { refinement: 'tickets', pickup: 'tickets' }
export const RESET_VIEWS = ['infra']

/** Rail icons offered for a view (drawn in App.svelte). */
export const VIEW_ICONS = ['dashboard', 'tickets', 'ci', 'infra', 'worktrees', 'agents', 'notes', 'chart', 'star', 'calendar', 'shield', 'music', 'data']

/** Offered colors, following the theme: its accent, and the hues its sources are drawn in. */
export const VIEW_COLORS: Record<string, string> = {
  accent: 'var(--accent)', orange: 'var(--src-gitlab)', purple: 'var(--src-clickup)', blue: 'var(--src-k8s)',
  coral: 'var(--src-claude)', gold: 'var(--src-alert)', ocean: 'var(--src-calendar)',
}
/** Their former names, after the sources: still read in config.toml and the packs. */
const FORMER_COLORS: Record<string, string> = { gitlab: 'orange', clickup: 'purple', k8s: 'blue', claude: 'coral', alert: 'gold', calendar: 'ocean' }
/** `clickup` → `purple`: a color's current name. */
export const colorName = (c: string) => FORMER_COLORS[c] ?? c
/** A color of VIEW_COLORS by name, a former one included; otherwise none. */
export const viewColor = (c: string | null | undefined) => (c ? VIEW_COLORS[colorName(c)] : undefined)

/** `ABC board` → `abc-board`: the identifier of a view without `id`. */
export const viewSlug = (name: string) =>
  name.toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '').replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'view'

/** A configured page's identifier: its `id`, otherwise its name's; `-2`… when a page before it (`taken`) has it. */
function viewId(c: ViewConfig, taken: Set<string>): string {
  let id = c.id?.trim() || viewSlug(c.name)
  for (let n = 2; taken.has(id); n++) id = `${c.id?.trim() || viewSlug(c.name)}-${n}`
  taken.add(id)
  return id
}

/**
 * A view from config.toml: unknown or duplicate blocks ignored, equal proportions. `---` starts
 * a band; an empty band is skipped.
 */
function fromConfig(c: ViewConfig, taken: Set<string>): View {
  const id = viewId(c, taken)
  const seen = new Set<BlockId>()
  const columns: BlockId[][] = []
  const bands: number[] = []
  let n = 0
  for (const s of [...(c.columns ?? []), BAND_BREAK]) {
    if (isBreak(s)) {
      if (n) bands.push(n)
      n = 0
      continue
    }
    const col = s.split(/[\s,]+/).map((b) => (MERGED_BLOCKS[b] ?? b) as BlockId).filter((b) => knownBlock(b) && !seen.has(b) && !!seen.add(b))
    if (!col.length || n >= MAX_COLUMNS || bands.length >= MAX_BANDS) continue
    columns.push(col)
    n++
  }
  // A page without blocks yet: one empty column, where "+ Block" and drag and drop put them.
  if (!columns.length) {
    columns.push([])
    bands.push(1)
  }
  const builtIn = OPS_VIEWS.find((v) => v.id === id)
  return {
    id,
    label: c.name.trim() || id,
    icon: c.icon && VIEW_ICONS.includes(c.icon) ? c.icon : (builtIn?.icon ?? 'dashboard'),
    color: viewColor(c.color) ?? builtIn?.color ?? 'var(--accent)',
    columns,
    ...(bands.length > 1 ? { bands } : {}),
    sizes: { columns: columns.map(() => 1), rows: columns.map((col) => col.map(() => 1)), ...(bands.length > 1 ? { bands: bands.map(() => 1) } : {}) },
  }
}

/** Without `[[views]]`: one empty Dashboard, to fill with "+ Block" or a template. */
export const EMPTY_VIEW: View = { id: 'dashboard', label: 'Dashboard', icon: 'dashboard', color: 'var(--accent)', columns: [[]], sizes: { columns: [1], rows: [[]] } }

/** The views from config.toml, otherwise one empty Dashboard. */
export function views(): View[] {
  const configured = live.snapshot?.views ?? []
  if (!configured.length) return [EMPTY_VIEW]
  const taken = new Set<string>()
  const all = configured.filter((c) => c.name?.trim()).map((c) => fromConfig(c, taken))
  // A page of the Worktrees block alone left the rail: the Projects page's Cleanup tab is it now
  // (shown once a repository is configured; without one, the page stays).
  const kept = live.snapshot?.repositories?.length ? all.filter((v) => !onlyWorktrees(v)) : all
  return kept.length ? kept : all
}

/** A page holding the Worktrees block and nothing else. */
const onlyWorktrees = (v: View) => v.columns.flat().length === 1 && v.columns.flat()[0] === 'worktrees'

/**
 * `[[views]]` as "Edit layout" writes it back: the pages of `blocks` get those blocks, in their
 * `bands` (one without), empty columns and bands left out; every page gets its identifier written
 * in, so that renaming or moving it keeps its proportions and block states. Without any page: the
 * empty Dashboard, written.
 */
export function viewRows(blocks: Record<string, BlockId[][]>, bands: Record<string, number[]> = {}): ViewConfig[] {
  const configured = live.snapshot?.views?.length ? live.snapshot.views : [{ name: EMPTY_VIEW.label, id: EMPTY_VIEW.id, icon: EMPTY_VIEW.icon, columns: [] }]
  const taken = new Set<string>()
  return configured.map((c) => {
    if (!c.name?.trim()) return { ...c }
    const id = viewId(c, taken)
    const cols = blocks[id]
    return { ...c, id, columns: cols ? columnRows(cols, bands[id]) : c.columns }
  })
}

/** A page's `columns` in config.toml: one string per column, `---` between two bands. */
function columnRows(columns: BlockId[][], bands: number[] | undefined): string[] {
  const c = compactBands(columns, bands ?? [columns.length])
  return bandRanges(c.bands).flatMap(([start, end], b) => [...(b ? [BAND_BREAK] : []), ...c.columns.slice(start, end).map((col) => col.join(' '))])
}

export const viewById = (id: string) => { const all = views(); return all.find((v) => v.id === id) ?? all[0] ?? EMPTY_VIEW }
