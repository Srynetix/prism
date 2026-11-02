import { invoke } from './bridge'
import type { BranchChoice } from './bindings/BranchChoice'
export type { BranchChoice }

/**
 * Actions of the Worktrees view. The Rust core refuses what would lose work (changes,
 * unshipped commits) or disturb a session (agent running, desktop session not archived).
 * `force`: delete anyway what would be lost, at my request; never under a session.
 */
export const deleteWorktree = (path: string, force = false) => invoke('worktree_remove', { path, force })
export const pruneWorktrees = () => invoke('worktree_prune')
export const openWorktree = (path: string, target: 'editor' | 'terminal' | 'finder') => invoke('open_repo', { path, with: target })

/** Origin fetched first, so this takes a moment. */
export const worktreeBranches = (path: string) => invoke<BranchChoice[]>('worktree_branches', { path })
/** Fast-forward only; refused when the branch and its upstream diverged. Says what it did. */
export const pullWorktree = (path: string) => invoke<string>('worktree_pull', { path })
/** Refused with uncommitted changes, or while an agent runs in it. */
export const switchWorktree = (path: string, branch: string) => invoke<string>('worktree_switch', { path, branch })

/** `1.2 GB`, `340 MB`. */
export function size(bytes: number | null | undefined): string {
  if (!bytes) return '—'
  const mb = bytes / 1048576
  return mb < 1024 ? `${Math.round(mb)} MB` : `${(mb / 1024).toFixed(1)} GB`
}
