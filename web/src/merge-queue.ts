import { conventions, isRelease } from './conventions'
import { repoName } from './repositories'
import { isApproved, openMrs } from './rules'
import type { MergeRequest, Snapshot } from './types'

/**
 * The Merge queue block's MRs, one repository at a time. With a merge queue (`[[repos]] merge_queue`),
 * its CI job takes the oldest open MR with the label (`[gitlab] merge_queue_label`): the queue is
 * those, oldest first, or by priority first with priority labels (`[gitlab] merge_priority_prefix`,
 * `merge-priority::0` before `merge-priority::1`, both before an MR without one). Then the MRs the forge merges by itself (auto-merge), and the approved ones
 * nothing will merge (no label, no auto-merge). Drafts are left out.
 */
export interface MergeQueue {
  /** In the queue, in the order the job takes them: the first is the next one. */
  queued: MergeRequest[]
  /** Auto-merge on, out of the queue. */
  auto: MergeRequest[]
  /** Approved, neither queued nor auto-merging. */
  ready: MergeRequest[]
  /** The repository has a merge queue (and Prism knows its label). */
  hasQueue: boolean
}

/** Oldest first: the MR's opening, else its number (an older reading has no `created_at`). */
const oldest = (a: MergeRequest, b: MergeRequest) => (a.created_at ?? '').localeCompare(b.created_at ?? '') || a.iid - b.iid

/**
 * The MR's priority in the queue: the number after the prefix (`merge-priority::0` → 0), the lowest
 * if it has several; null without a prefix or such a label.
 */
export function mergePriority(m: MergeRequest, prefix: string | null | undefined): number | null {
  if (!prefix) return null
  const ns = (m.labels ?? []).filter((l) => l.startsWith(prefix)).map((l) => l.slice(prefix.length).trim()).filter((n) => /^-?\d+$/.test(n)).map(Number)
  return ns.length ? Math.min(...ns) : null
}

/** The priorities offered for an MR: 0 to 3, and any other the open MRs carry, lowest first. */
export function priorityLevels(s: Snapshot): number[] {
  const seen = openMrs(s).map((m) => mergePriority(m, s.merge_priority_prefix)).filter((n): n is number => n !== null && n >= 0)
  return [...new Set([0, 1, 2, 3, ...seen])].sort((a, b) => a - b)
}

export function mergeQueue(s: Snapshot, repo: string): MergeQueue {
  const label = s.merge_queue_label
  const prio = (m: MergeRequest) => mergePriority(m, s.merge_priority_prefix) ?? Infinity
  const byPriority = (a: MergeRequest, b: MergeRequest) => prio(a) - prio(b) || oldest(a, b)
  const mrs = openMrs(s).filter((m) => !m.draft && repoName(m.repo, s) === repo).sort(oldest)
  const hasQueue = !!label && !!s.repositories?.find((r) => r.name === repo)?.merge_queue
  const inQueue = (m: MergeRequest) => hasQueue && !!m.labels?.includes(label!)
  return {
    queued: mrs.filter(inQueue).sort(byPriority),
    auto: mrs.filter((m) => !inQueue(m) && m.auto_merge),
    ready: mrs.filter((m) => !inQueue(m) && !m.auto_merge && isApproved(m)),
    hasQueue,
  }
}

/** The block's repositories: those with a merge queue first, then the others with an MR it would show. */
export function queueRepos(s: Snapshot): string[] {
  const queued = (s.repositories ?? []).filter((r) => r.merge_queue && s.merge_queue_label).map((r) => r.name)
  const shown = openMrs(s).filter((m) => !m.draft && (m.auto_merge || isApproved(m))).map((m) => repoName(m.repo, s))
  return [...new Set([...queued, ...[...new Set(shown)].sort()])].filter(Boolean)
}

/** One target branch's share of a repository's queue: each branch is merged into in its own order. */
export interface BranchQueue extends MergeQueue {
  target: string
}

/**
 * The queue split by target branch: each branch has its own order and its own head (▶). The main
 * branch first, then the release branches, then the others, by name.
 */
export function byTarget(q: MergeQueue): BranchQueue[] {
  const all = [...q.queued, ...q.auto, ...q.ready]
  const rank = (b: string) => (b === conventions().main_branch ? 0 : isRelease(b) ? 1 : 2)
  const targets = [...new Set(all.map((m) => m.target_branch))].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b))
  const of = (ms: MergeRequest[], t: string) => ms.filter((m) => m.target_branch === t)
  return targets.map((target) => ({ target, queued: of(q.queued, target), auto: of(q.auto, target), ready: of(q.ready, target), hasQueue: q.hasQueue }))
}

/** How many MRs the block shows for a repository. */
export const queueCount = (q: MergeQueue) => q.queued.length + q.auto.length + q.ready.length
