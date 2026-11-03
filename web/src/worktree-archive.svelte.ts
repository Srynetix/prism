import { live } from './live.svelte'
import { pref, setPref } from './layout.svelte'
import { isArchived as agentArchived, setArchived as setAgentArchived } from './agent-archive.svelte'
import type { WorktreeInfo } from './rules'
import type { Worktree } from './types'

/**
 * Worktrees archived: out of the Worktrees block, its counts, the stale badge and alert, until
 * unarchived — or until something happens in them after being archived (a new commit, files
 * changed): then they come back by themselves. Archiving changes nothing on disk: the folder, its
 * branch and its agents stay. Shared computer ↔ phone (`worktrees.archived`): its path → its state
 * when archived. A repository's main checkout too (a project folder an agent works in), for focus:
 * archiving never removes anything.
 *
 * It also follows its agents and Desktop sessions (`isHidden`): once every one working in it is
 * archived, it is hidden with them whatever happens in it (an archived agent goes on committing),
 * and back as soon as one of them is.
 */
const KEY = 'worktrees.archived'

type Archivable = Pick<Worktree, 'path' | 'head' | 'changes' | 'prunable'>

const archived = () => pref<Record<string, string>>(KEY, {})

/** What a change in it alters: its commit and its uncommitted files. */
const stateOf = (w: Archivable) => `${w.head ?? ''}|${w.changes}`

/** Any folder still there: a worktree, or a repository's main checkout. */
export const canArchive = (w: Pick<Worktree, 'prunable'>) => !w.prunable

export function isArchived(w: Archivable): boolean {
  const at = archived()[w.path]
  return at !== undefined && at === stateOf(w)
}

export function setArchived(w: Archivable, on: boolean) {
  setArchivedMany([w], on)
}

export function setArchivedMany(list: Archivable[], on: boolean) {
  const paths = new Set(list.map((w) => w.path))
  const rest = Object.fromEntries(Object.entries(archived()).filter(([p]) => !paths.has(p)))
  setPref(KEY, on ? { ...rest, ...Object.fromEntries(list.filter(canArchive).map((w) => [w.path, stateOf(w)])) } : rest)
}

/** Who works in it, as the Worktrees block links them: its agents (by folder too) and Desktop sessions. */
const workers = (i: Pick<WorktreeInfo, 'agents' | 'sessions'>) => [...i.agents, ...i.sessions]

/** Out of the list: archived itself, or every agent and session working in it archived. */
export function isHidden(i: Pick<WorktreeInfo, 'w' | 'agents' | 'sessions'>): boolean {
  if (isArchived(i.w)) return true
  const who = workers(i)
  return canArchive(i.w) && who.length > 0 && who.every(agentArchived)
}

/** Hides `list`, or brings it back: then the agents and sessions archived in it come back too. */
export function setHidden(list: Pick<WorktreeInfo, 'w' | 'agents' | 'sessions'>[], on: boolean) {
  setArchivedMany(list.map((i) => i.w), on)
  if (!on) for (const x of list.flatMap(workers)) if (agentArchived(x)) setAgentArchived(x, false)
}

/** Those that came back, or whose folder is gone, are forgotten. */
$effect.root(() => {
  $effect(() => {
    const s = live.snapshot
    const map = archived()
    if (!s?.worktrees?.length || !Object.keys(map).length) return
    const keep = Object.entries(map).filter(([p, at]) => s.worktrees.some((w) => w.path === p && stateOf(w) === at))
    if (keep.length !== Object.keys(map).length) setPref(KEY, Object.fromEntries(keep))
  })
})
