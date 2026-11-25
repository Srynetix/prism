/**
 * A project's local branches (Projects › Branches, `ProjectBranches.svelte`): read on demand, by
 * project; deleted one by one, a worktree opened on one, the clone switched to one (computer only);
 * and a tidy: origin pruned, then the safe ones picked from a list deleted (on a phone too, with its
 * passkey). Local branches only: nothing is deleted on the forge.
 */
import { invoke } from './bridge'
import type { LocalBranch } from './bindings/LocalBranch'
import type { TidyReport } from './bindings/TidyReport'

export type { LocalBranch }

interface Branches {
  list: LocalBranch[] | null
  loading: boolean
  error: string | null
}

export const projectBranches = $state<Record<string, Branches>>({})

/** A tidy under way, of one project: its list (prepared), the branches picked, then its report. */
export const tidy = $state({
  repo: null as string | null,
  preparing: false,
  running: false,
  list: null as LocalBranch[] | null,
  picked: [] as string[],
  report: null as TidyReport | null,
  error: null as string | null,
})

/** What a tidy deletes: done, nothing to lose, not locked. */
export const safe = (b: LocalBranch) => !b.locked && !b.keep && b.done.length > 0

export const branchesOf = (repo: string): Branches => projectBranches[repo] ?? { list: null, loading: false, error: null }

/** Incremented on each read of a project: an older one answering late is ignored. */
const seq: Record<string, number> = {}

export async function loadBranches(repo: string) {
  const mine = (seq[repo] = (seq[repo] ?? 0) + 1)
  projectBranches[repo] = { ...branchesOf(repo), loading: true, error: null }
  try {
    const list = await invoke<LocalBranch[]>('project_branches', { repo })
    if (seq[repo] === mine) projectBranches[repo] = { list, loading: false, error: null }
  } catch (e) {
    if (seq[repo] === mine) projectBranches[repo] = { ...branchesOf(repo), loading: false, error: String(e) }
  }
}

/** A branch deleted, its worktree with it; `force`: what it would lose too. */
export async function deleteBranch(repo: string, branch: string, force: boolean) {
  try {
    await invoke('project_branch_delete', { repo, branch, force })
  } finally {
    void loadBranches(repo)
  }
}

/** A worktree opened on a branch, without an agent: its path. */
export async function branchWorktree(repo: string, branch: string): Promise<string> {
  try {
    return await invoke<string>('project_branch_worktree', { repo, branch })
  } finally {
    void loadBranches(repo)
  }
}

/** The project's clone switched to a branch. */
export async function switchClone(repo: string, branch: string): Promise<string> {
  try {
    return await invoke<string>('project_switch', { repo, branch })
  } finally {
    void loadBranches(repo)
  }
}

/** A tidy's first step: origin pruned, its list read; the safe branches picked. */
export async function prepareTidy(repo: string) {
  // A plain read still under way would answer with the state from before the prune: ignored.
  seq[repo] = (seq[repo] ?? 0) + 1
  Object.assign(tidy, { repo, preparing: true, running: false, list: null, picked: [], report: null, error: null })
  try {
    const list = await invoke<LocalBranch[]>('project_tidy_prepare', { repo })
    if (tidy.repo !== repo) return
    Object.assign(tidy, { preparing: false, list, picked: list.filter(safe).map((b) => b.name) })
    seq[repo] = (seq[repo] ?? 0) + 1
    projectBranches[repo] = { list, loading: false, error: null }
  } catch (e) {
    if (tidy.repo === repo) Object.assign(tidy, { preparing: false, error: String(e) })
  }
}

export function togglePicked(name: string) {
  tidy.picked = tidy.picked.includes(name) ? tidy.picked.filter((n) => n !== name) : [...tidy.picked, name]
}

/** The branches picked deleted, each checked again; what was done. */
export async function runTidy() {
  const repo = tidy.repo
  if (!repo || tidy.running || !tidy.picked.length) return
  tidy.running = true
  try {
    const report = await invoke<TidyReport>('project_tidy', { repo, branches: tidy.picked })
    if (tidy.repo === repo) Object.assign(tidy, { running: false, report })
  } catch (e) {
    if (tidy.repo === repo) Object.assign(tidy, { running: false, error: String(e) })
  } finally {
    void loadBranches(repo)
  }
}

export function closeTidy() {
  Object.assign(tidy, { repo: null, preparing: false, running: false, list: null, picked: [], report: null, error: null })
}
