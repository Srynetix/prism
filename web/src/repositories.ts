import { live } from './live.svelte'
import type { RepoInfo, Snapshot } from './types'

/**
 * The config's repositories (`[[repos]]`, the primary one first) and their switches: worktrees,
 * merge queue, ticket required. An MR or a worktree names its repository; an MR without one is
 * the primary repository's.
 */
const list = (s?: Snapshot | null): RepoInfo[] => (s === undefined ? live.snapshot : s)?.repositories ?? []

export const primaryRepo = (s?: Snapshot | null) => list(s).find((r) => r.primary)

/** The repository of an MR (`repo` absent: the primary one). */
export const repoOf = (repo: string | null | undefined, s?: Snapshot | null) => (repo ? list(s).find((r) => r.name === repo) : primaryRepo(s))

/** The name of an MR's repository, the primary one's when absent. */
export const repoName = (repo: string | null | undefined, s?: Snapshot | null) => repo ?? primaryRepo(s)?.name ?? ''

export const hasMergeQueue = (m: { repo?: string | null }, s?: Snapshot | null) => !!repoOf(m.repo, s)?.merge_queue
export const needsTicket = (m: { repo?: string | null }, s?: Snapshot | null) => !!repoOf(m.repo, s)?.ticket_required
/** Agents create worktrees there, and the Claude actions on its MRs run there. */
export const hasWorktrees = (repo: string | null | undefined, s?: Snapshot | null) => !!repoOf(repo, s)?.worktrees

/** The repositories where agents create worktrees, the primary one first. */
export const worktreeRepos = (s?: Snapshot | null) => list(s).filter((r) => r.worktrees)

/**
 * The repository an agent works in: its worktree's, failing that the clone its folder is in (or the
 * `<clone>.worktrees` folder next to it), the deepest one first. None: a folder outside them all.
 */
export function agentRepo(a: { cwd: string; worktree: string | null }, s?: Snapshot | null): string | null {
  const snap = s === undefined ? live.snapshot : s
  const wt = a.worktree && snap?.worktrees.find((w) => w.path === a.worktree)
  if (wt) return wt.repo
  const dir = a.worktree ?? a.cwd
  const inside = (root: string) => dir === root || dir.startsWith(`${root}/`)
  const clone = [...(snap?.repos ?? [])].sort((x, y) => y.path.length - x.path.length).find((r) => inside(r.path) || inside(`${r.path}.worktrees`))
  return clone?.name ?? null
}
