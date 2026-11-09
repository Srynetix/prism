import { read, write } from './persist'
import { hasCore, invoke, isLocal } from './bridge'
import { live } from './live.svelte'
import { pref, setPref } from './layout.svelte'
import { onShared, shareSave } from './shared.svelte'
import { t } from './i18n.svelte'

/**
 * Points for the sprint retro, written in the My sprint block, apart from the free notes: by
 * sprint (its name), each one to keep, to change, or an idea. Stored by the app in its data folder
 * (`retro.json`), shared with the phones (`shared.svelte.ts`). The retro often happens once the next sprint has begun: the block shows any
 * sprint's points, the current one's first. The same store keeps the sprints' figures (`history`),
 * written by the core alone (`retro_keep`): a write of the points leaves them as they are.
 */
export type PointKind = 'keep' | 'change' | 'idea'
export const POINT_KINDS: [PointKind, string, string][] = [
  ['keep', '▲', t('To keep')],
  ['change', '▼', t('To change')],
  ['idea', '✦', t('Idea')],
]
export const POINT_LABEL: Record<PointKind, string> = Object.fromEntries(POINT_KINDS.map(([k, , l]) => [k, l])) as Record<PointKind, string>

export interface RetroPoint {
  id: string
  text: string
  kind: PointKind
  created_at: string
}

function load(): Record<string, RetroPoint[]> {
  return normalize(read('retro'))
}

function normalize(raw: unknown): Record<string, RetroPoint[]> {
  const saved = raw as { sprints?: Record<string, RetroPoint[]> } | null
  const out: Record<string, RetroPoint[]> = {}
  for (const [sprint, list] of Object.entries(saved?.sprints ?? {})) {
    if (!Array.isArray(list)) continue
    out[sprint] = list
      .filter((p) => typeof p?.id === 'string' && typeof p?.text === 'string')
      .map((p) => ({ ...p, kind: POINT_KINDS.some(([k]) => k === p.kind) ? p.kind : 'idea' }))
  }
  return out
}

/** By sprint name, in the order written. */
export const retroPoints = $state<Record<string, RetroPoint[]>>(load())

/** True if the last write failed: the block says so. */
export const pointsStorage = $state({ failed: false })

function save() {
  pointsStorage.failed = false
  // The browser preview: its own storage.
  if (!hasCore()) return write('retro', { sprints: retroPoints }, () => (pointsStorage.failed = true))
  shareSave('retro', () => ({ sprints: $state.snapshot(retroPoints) }), () => (pointsStorage.failed = true))
}

// Written elsewhere (the window, a phone): read again.
onShared('retro', (saved) => {
  const next = normalize(saved)
  for (const k of Object.keys(retroPoints)) if (!(k in next)) delete retroPoints[k]
  Object.assign(retroPoints, next)
}, () => (pointsStorage.failed = true))

export const pointsOf = (sprint: string): RetroPoint[] => retroPoints[sprint] ?? []

/** The sprints with points, the most recently written first. */
export function sprintsWithPoints(): string[] {
  const last = (s: string) => pointsOf(s).reduce((m, p) => (p.created_at > m ? p.created_at : m), '')
  return Object.keys(retroPoints).filter((s) => pointsOf(s).length).sort((a, b) => last(b).localeCompare(last(a)))
}

export function addPoint(sprint: string, text: string, kind: PointKind) {
  const t = text.trim()
  if (!t) return
  const point = { id: crypto.randomUUID(), text: t, kind, created_at: new Date().toISOString() }
  // Through the state's own array: `??=` would push into the plain one it was given, not into its proxy.
  retroPoints[sprint] = [...pointsOf(sprint), point]
  save()
}

export function updatePoint(sprint: string, id: string, change: Partial<Pick<RetroPoint, 'text' | 'kind'>>) {
  const p = pointsOf(sprint).find((x) => x.id === id)
  if (!p) return
  if (change.text !== undefined) {
    const t = change.text.trim()
    if (!t) return removePoint(sprint, id)
    p.text = t
  }
  if (change.kind) p.kind = change.kind
  save()
}

export function removePoint(sprint: string, id: string) {
  const list = pointsOf(sprint).filter((p) => p.id !== id)
  if (list.length) retroPoints[sprint] = list
  else delete retroPoints[sprint]
  save()
}

/** On click on a point's mark: to keep → to change → idea → to keep. */
export const nextKind = (k: PointKind): PointKind => POINT_KINDS[(POINT_KINDS.findIndex(([x]) => x === k) + 1) % POINT_KINDS.length][0]

/** Where the window kept the sprints' figures before the core did: moved into the store once. */
const OLD_HISTORY = 'retro.history'

/**
 * The current sprint's figures kept by the core in the store's history (My sprint shown), the
 * window's former pref `retro.history` moved there with them when the store has none.
 */
export function keepFigures(): Promise<void> {
  const old = pref<unknown[] | undefined>(OLD_HISTORY, undefined)
  return invoke('retro_keep', { history: Array.isArray(old) && old.length ? old : null }).then(() => {
    if (old !== undefined) setPref(OLD_HISTORY, undefined)
  })
}

// At the window's first snapshot: the former pref moved, whether My sprint shows or not.
$effect.root(() => {
  let done = false
  $effect(() => {
    if (done || !live.snapshot || !isLocal() || pref(OLD_HISTORY, undefined) === undefined) return
    done = true
    keepFigures().catch((e) => console.error('retro_keep', e))
  })
})
