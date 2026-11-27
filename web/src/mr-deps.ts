import { invoke } from './bridge'
import { t } from './i18n.svelte'
import { mrArgs, mrKey, sameMr, type MrRef } from './mr-ref'
import type { MergeRequest, Snapshot } from './types'
import type { MrDep } from './bindings/MrDep'
import type { DepVia } from './bindings/DepVia'
import type { Held } from './bindings/Held'

/**
 * The dependencies between MRs, as the core computes them (`Snapshot.mr_deps`, Rust's
 * `mr_deps`): stacked branches, dependencies declared on the forge, the tickets' blockers. Here,
 * what the views make of them: an MR's own, its chains, the graph's layout, the list's tree.
 */

export type { MrDep, DepVia, Held }

/** Why an MR waits for another, in words. */
export function viaLabel(v: DepVia): string {
  return v === 'stacked' ? t('stacked branch') : v === 'declared' ? t('declared') : t('via tickets')
}

/** The MR that waits, and the one it waits for. */
export const waiting = (d: MrDep): MrRef => ({ iid: d.iid, repo: d.repo ?? null })
export const waitedFor = (d: MrDep): MrRef => ({ iid: d.on_iid, repo: d.on_repo ?? null })

export const findMr = (s: Snapshot, r: MrRef) => s.merge_requests.find((m) => sameMr(m, r))

const deps = (s: Snapshot) => s.mr_deps ?? []

/** What an MR waits for, the surest first. */
export const dependsOn = (s: Snapshot, r: MrRef) => deps(s).filter((d) => sameMr(waiting(d), r))
/** What waits for an MR. */
export const blocking = (s: Snapshot, r: MrRef) => deps(s).filter((d) => sameMr(waitedFor(d), r))

/** It waits for an MR still open (not merged). */
export const waitsOpen = (s: Snapshot, r: MrRef) => dependsOn(s, r).filter((d) => !d.done)

/** The views' filters: the dependencies via tickets, the merged MRs still waited on. */
export interface DepFilter {
  tickets: boolean
  merged: boolean
}

export interface Chain {
  /** Its MRs, the surest bases first (by depth, then the oldest). */
  mrs: MergeRequest[]
  deps: MrDep[]
}

/**
 * The chains among `shown` (the open MRs a view lists): MRs linked by a dependency, with the
 * MRs they wait for even when not listed (merged ones only with `merged`). The longest first;
 * `alone` the listed MRs in no chain, in their order.
 */
export function chains(s: Snapshot, shown: MergeRequest[], filter: DepFilter): { chains: Chain[]; alone: MergeRequest[] } {
  const listed = new Set(shown.map(mrKey))
  const edges = deps(s).filter((d) => (filter.tickets || d.via !== 'ticket') && (filter.merged || !d.done) && listed.has(mrKey(waiting(d))))
  const parent = new Map<string, string>()
  const root = (k: string): string => {
    let r = k
    while (parent.get(r) !== r) r = parent.get(r)!
    parent.set(k, r)
    return r
  }
  const mrs = new Map<string, MergeRequest>()
  const add = (m: MergeRequest | undefined) => {
    if (!m) return
    const k = mrKey(m)
    mrs.set(k, m)
    if (!parent.has(k)) parent.set(k, k)
  }
  const kept: MrDep[] = []
  for (const d of edges) {
    const a = findMr(s, waiting(d))
    const b = findMr(s, waitedFor(d))
    if (!a || !b) continue
    add(a)
    add(b)
    parent.set(root(mrKey(a)), root(mrKey(b)))
    kept.push(d)
  }
  const groups = new Map<string, Chain>()
  for (const [k, m] of mrs) {
    const r = root(k)
    if (!groups.has(r)) groups.set(r, { mrs: [], deps: [] })
    groups.get(r)!.mrs.push(m)
  }
  for (const d of kept) groups.get(root(mrKey(waiting(d))))!.deps.push(d)
  const list = [...groups.values()].map((c) => {
    const depth = depths(c)
    c.mrs.sort((a, b) => depth.get(mrKey(a))! - depth.get(mrKey(b))! || (a.created_at ?? '').localeCompare(b.created_at ?? '') || a.iid - b.iid)
    return c
  })
  list.sort((a, b) => b.mrs.length - a.mrs.length)
  const chained = new Set(list.flatMap((c) => c.mrs.map(mrKey)))
  return { chains: list, alone: shown.filter((m) => !chained.has(mrKey(m))) }
}

/**
 * How far each MR of a chain is from a base (an MR that waits for none of the chain): the
 * longest path, a column of the graph. A cycle (declared both ways) stops where it loops.
 */
export function depths(c: Chain): Map<string, number> {
  const on = new Map<string, string[]>()
  for (const d of c.deps) on.set(mrKey(waiting(d)), [...(on.get(mrKey(waiting(d))) ?? []), mrKey(waitedFor(d))])
  const out = new Map<string, number>()
  const visit = (k: string, seen: Set<string>): number => {
    if (out.has(k)) return out.get(k)!
    if (seen.has(k)) return 0
    seen.add(k)
    const n = Math.max(-1, ...(on.get(k) ?? []).map((x) => visit(x, seen))) + 1
    seen.delete(k)
    out.set(k, n)
    return n
  }
  for (const m of c.mrs) visit(mrKey(m), new Set())
  return out
}

export interface Placed {
  mr: MergeRequest
  col: number
  row: number
}

/**
 * The graph's layout: one column per depth (left: to merge first), each chain a band of rows
 * under the previous one; in a column, the chain's MRs in its order.
 */
export function layout(list: Chain[]): { placed: Placed[]; cols: number; rows: number } {
  const placed: Placed[] = []
  let top = 0
  let cols = 0
  for (const c of list) {
    const depth = depths(c)
    const used = new Map<number, number>()
    for (const m of c.mrs) {
      const col = depth.get(mrKey(m)) ?? 0
      const row = used.get(col) ?? 0
      used.set(col, row + 1)
      placed.push({ mr: m, col, row: top + row })
      cols = Math.max(cols, col + 1)
    }
    top += Math.max(1, ...used.values())
  }
  return { placed, cols, rows: top }
}

/**
 * The list's tree of a chain: each MR under the first MR it waits for, its bases at the root;
 * an MR waiting for several shows once, under the first.
 */
export function tree(c: Chain): { mr: MergeRequest; depth: number; dep?: MrDep }[] {
  const keys = new Set(c.mrs.map(mrKey))
  const first = new Map<string, MrDep>()
  for (const d of c.deps) if (!first.has(mrKey(waiting(d))) && keys.has(mrKey(waitedFor(d)))) first.set(mrKey(waiting(d)), d)
  const children = new Map<string, MergeRequest[]>()
  const roots: MergeRequest[] = []
  for (const m of c.mrs) {
    const d = first.get(mrKey(m))
    if (!d) roots.push(m)
    else children.set(mrKey(waitedFor(d)), [...(children.get(mrKey(waitedFor(d))) ?? []), m])
  }
  const out: { mr: MergeRequest; depth: number; dep?: MrDep }[] = []
  const seen = new Set<string>()
  const walk = (m: MergeRequest, depth: number) => {
    const k = mrKey(m)
    if (seen.has(k)) return
    seen.add(k)
    out.push({ mr: m, depth, dep: first.get(k) })
    for (const ch of children.get(k) ?? []) walk(ch, depth + 1)
  }
  for (const r of roots) walk(r, 0)
  // A cycle has no root: what is left, flat.
  for (const m of c.mrs) walk(m, 0)
  return out
}

/** The MRs linked to one, both ways and further: what lights up when it is pointed at. */
export function lineage(list: MrDep[], from: string): Set<string> {
  const lit = new Set([from])
  const walk = (k: string, up: boolean) => {
    for (const d of list) {
      const [a, b] = up ? [mrKey(waiting(d)), mrKey(waitedFor(d))] : [mrKey(waitedFor(d)), mrKey(waiting(d))]
      if (a === k && !lit.has(b)) {
        lit.add(b)
        walk(b, up)
      }
    }
  }
  walk(from, true)
  walk(from, false)
  return lit
}

/** An arc of the merge queue's train: `from` waits for `to`; `back` when it comes first. */
export interface Arc {
  from: number
  to: number
  back: boolean
  dep: MrDep
}

/**
 * The dependencies between the MRs of a queue (in the order its job takes them), not inferred,
 * not satisfied: an arc from each to the MR it waits for, backwards when it would merge first.
 */
export function arcs(s: Snapshot, queued: MergeRequest[]): Arc[] {
  const at = new Map(queued.map((m, i) => [mrKey(m), i]))
  return deps(s)
    .filter((d) => d.via !== 'ticket' && !d.done)
    .flatMap((d) => {
      const from = at.get(mrKey(waiting(d)))
      const to = at.get(mrKey(waitedFor(d)))
      return from === undefined || to === undefined ? [] : [{ from, to, back: from < to, dep: d }]
    })
}

/** The MRs of a repository held out of its queue (`Snapshot.queue_held`). */
export const heldIn = (s: Snapshot, repo: string | null) => (s.queue_held ?? []).filter((h) => (h.repo ?? null) === repo)

/** Out of the merge queue until the MR it waits for is merged, which puts it back. */
export const holdInQueue = (mr: MrRef, on: MrRef) => invoke<void>('hold_in_queue', { ...mrArgs(mr), on_repo: on.repo ?? null, on_iid: on.iid })

/** No longer held, and left out of the queue. */
export const releaseHeld = (mr: MrRef) => invoke<void>('queue_release', mrArgs(mr))

/** Declares on the forge that an MR waits for another. */
export const declareDependency = (mr: MrRef, on: MrRef) => invoke<void>('declare_dependency', { ...mrArgs(mr), on_repo: on.repo ?? null, on_iid: on.iid })
