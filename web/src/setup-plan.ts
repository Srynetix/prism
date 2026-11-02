import type { SetupDetection } from './bindings/SetupDetection'
import type { RepoFound } from './bindings/RepoFound'
import type { AgentProvider, TrackerProvider, ViewConfig } from './types'

/**
 * The pages wizard's choices, pre-filled from what the machine has (`detect_setup`, detect.rs):
 * the forge signed in, the agent CLIs found, the repositories of the usual folders, the tracker
 * within reach. Then the pages it writes: a template's, without the blocks whose source is missing,
 * so that no block is empty and in error on the first day. Pure, for the tests.
 */

export type ForgeChoice = 'gitlab' | 'github' | 'none'
export type TrackerChoice = TrackerProvider | 'none'

export interface SetupChoices {
  forge: ForgeChoice
  /** The chosen forge's CLI is signed in. */
  signedIn: boolean
  /** The agent CLIs ticked, the first one the default (`[agents] provider`). */
  agents: AgentProvider[]
  /** tmux, which hosts the agents, is installed. */
  tmux: boolean
  /** Repositories the config will have (found and ticked, or already there). */
  repos: number
  tracker: TrackerChoice
  /** The macOS Calendar is read. */
  calendar: boolean
  /** kubectl has a context. */
  kube: boolean
}

/** Why a block is left out: the source it reads is missing. */
export type Missing = 'no-forge' | 'not-signed-in' | 'no-repo' | 'no-agent' | 'no-tmux' | 'no-tracker' | 'no-calendar' | 'no-kube'

/** The source each core block reads; a block not named reads none of these (Notes, Alerts, a pack's own). */
const NEEDS: Record<string, 'forge' | 'repos' | 'agent' | 'tracker' | 'calendar' | 'kube'> = {
  mrs: 'forge', queue: 'forge', ci: 'forge', renovate: 'forge', vulns: 'forge',
  worktrees: 'repos',
  sessions: 'agent',
  sprint: 'tracker', refinement: 'tracker', pickup: 'tracker', retro: 'tracker',
  agenda: 'calendar',
  pods: 'kube', envs: 'kube', argocd: 'kube',
}

/** What `block` misses with these choices; `null`: it has its source. */
export function missingFor(block: string, c: SetupChoices): Missing | null {
  switch (NEEDS[block]) {
    case 'forge':
      return c.forge === 'none' ? 'no-forge' : c.signedIn ? null : 'not-signed-in'
    case 'repos':
      return c.repos ? null : 'no-repo'
    case 'agent':
      return !c.agents.length ? 'no-agent' : c.tmux ? null : 'no-tmux'
    case 'tracker':
      return c.tracker === 'none' ? 'no-tracker' : null
    case 'calendar':
      return c.calendar ? null : 'no-calendar'
    case 'kube':
      return c.kube ? null : 'no-kube'
    default:
      return null
  }
}

const isBreak = (s: string) => /^-{3,}$/.test(s.trim())

/**
 * A template's pages with these choices: each block whose source is missing left out, an emptied
 * column, band or page with it; `blocks`, every block of the template once, in order, with what it misses.
 */
export function planPages(views: ViewConfig[], c: SetupChoices): { pages: ViewConfig[]; blocks: { id: string; missing: Missing | null }[] } {
  const blocks: { id: string; missing: Missing | null }[] = []
  const pages = views
    .map((v) => {
      const columns = (v.columns ?? [])
        .map((col) => {
          if (isBreak(col)) return col
          const ids = col.split(/\s+/).filter(Boolean)
          for (const id of ids) if (!blocks.some((b) => b.id === id)) blocks.push({ id, missing: missingFor(id, c) })
          return ids.filter((id) => !missingFor(id, c)).join(' ')
        })
        .filter(Boolean)
      // A band left without columns goes with them: no `---` first, last, nor twice in a row.
      const kept = columns.filter((col, i) => !isBreak(col) || (i > 0 && i < columns.length - 1 && !isBreak(columns[i + 1])))
      return { ...v, columns: kept }
    })
    .filter((v) => v.columns.some((col) => !isBreak(col)))
  return { pages, blocks }
}

/* ---------- Pre-filled from the detection ---------- */

/** The forge to start from: the one signed in (the one most repositories are on, if both), else the one they are on with its CLI installed, else none. */
export function pickForge(d: SetupDetection): ForgeChoice {
  const on = (p: 'gitlab' | 'github') => d.repos.filter((r) => r.forge === p).length
  const signed = d.forges.filter((f) => f.login).map((f) => f.provider)
  if (signed.length) return [...signed].sort((a, b) => on(b) - on(a))[0]
  const installed = d.forges.filter((f) => f.installed && on(f.provider)).map((f) => f.provider)
  return installed.length ? [...installed].sort((a, b) => on(b) - on(a))[0] : 'none'
}

/** The agent CLIs found, in Prism's order (Claude Code first): the first is the default. */
export const pickAgents = (d: SetupDetection): AgentProvider[] => d.agents.filter((a) => a.path).map((a) => a.provider)

/** At most this many repositories ticked from the start: the most recently touched. */
export const REPOS_TICKED = 8

/**
 * The repositories ticked from the start: those on the chosen forge (any, without a forge), the
 * most recently touched first, but those the config has already.
 */
export function pickRepos(d: SetupDetection, forge: ForgeChoice, known: string[] = []): string[] {
  return d.repos
    .filter((r) => !known.includes(r.path) && (forge === 'none' || r.forge === forge))
    .slice(0, REPOS_TICKED)
    .map((r) => r.path)
}

/** The tracker within reach: a token in the environment (ClickUp, then Jira), else an Obsidian vault, else none. */
export function pickTracker(d: SetupDetection): TrackerChoice {
  if (d.trackers.clickup_env) return 'clickup'
  if (d.trackers.jira_env) return 'jira'
  return d.trackers.obsidian_vaults.length ? 'obsidian' : 'none'
}

/**
 * The `[[repos]]` rows of the ticked repositories: their MRs read and their worktrees listed, named
 * after their folder (`app`, then `app-2` when a name is taken). One on another forge than the
 * profile's says which (`forge`); its project is kept for links either way.
 */
export function repoRows(found: RepoFound[], forge: ForgeChoice, taken: string[]): Record<string, unknown>[] {
  const names = new Set(taken.map((n) => n.toLowerCase()))
  return found.map((r) => {
    const base = r.name.toLowerCase().replace(/[^a-z0-9._-]+/g, '-').replace(/^-|-$/g, '') || 'repo'
    let name = base
    for (let i = 2; names.has(name); i++) name = `${base}-${i}`
    names.add(name)
    const other = forge !== 'none' && r.forge && r.forge !== forge
    return {
      name,
      path: r.path,
      ...(r.project ? { project: r.project, merge_requests: forge !== 'none' && !!r.forge } : {}),
      worktrees: true,
      ...(other ? { forge: r.forge } : {}),
    }
  })
}

/** The working folders to add (`[[workspaces]] dir`) when the config has none: those holding the ticked repositories. */
export const workspaceDirs = (found: RepoFound[]): string[] => [...new Set(found.map((r) => r.root))]
