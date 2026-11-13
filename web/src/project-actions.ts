import { invoke } from './bridge'
import { live } from './live.svelte'
import { afterLaunch, launchNow, openAgents, openLauncher, type Launch } from './agents.svelte'
import { hasWorktrees } from './repositories'
import { isBlocked } from './tickets'
import { runClaude, ticketActions } from './claude-actions'
import { nextTicket, placeIn } from './projects'
import { t as tr } from './i18n.svelte'
import type { Ticket } from './types'

/**
 * What a project of the Agents view does (`projects.ts`), on the computer and the phone: a new agent in
 * it, the agent left in it brought back, its next ticket implemented.
 */

/** The short launcher on this repository: a new worktree where agents make them, the checkout otherwise. */
export function newAgentIn(repo: string) {
  const path = live.snapshot?.repos?.find((r) => r.name === repo)?.path
  openLauncher({ title: '', ...placeIn({ name: repo, path: path ?? '' }, hasWorktrees(repo) || !path) }, null, null, repo, true)
}

/** The last agent closed in it back in the list, resumed, and shown. */
export async function reopenLeft(repo: string) {
  const id = await invoke<string>('agent_reopen', { repo })
  openAgents(id)
}

/** Its next ticket to pick, on the boards the tracker reads (Obsidian's name their repository). */
export function nextOf(repo: string): Ticket | null {
  const pickup = live.snapshot?.pickup
  if (!pickup) return null
  return nextTicket(pickup.boards.flatMap((b) => b.tickets), pickup.statuses, repo, isBlocked)
}

/** Implement the ticket, as from its panel: the launcher, prefilled. */
export function implement(t: Ticket) {
  const a = ticketActions(t).find((x) => x.id === 'implement')
  if (a) void runClaude(a)
}

/**
 * A new workstream in `repo` (the Projects page's « New workstream », and whoever proposes one: the
 * assistant): an agent on a prompt, or on a ticket (its Implement, which takes the ticket), in a
 * new worktree of the repository where agents make them, its clone otherwise. `direct`: launched at
 * once; otherwise the launcher opens, prefilled. Its agent's id once launched, `null` otherwise.
 */
export async function startWorkstream(repo: string, from: { prompt: string } | { ticket: Ticket }, direct = true): Promise<string | null> {
  const path = live.snapshot?.repos?.find((r) => r.name === repo)?.path ?? ''
  const worktree = hasWorktrees(repo) || !path
  if ('ticket' in from) {
    const a = ticketActions(from.ticket).find((x) => x.id === 'implement')
    if (!a?.launch) return null
    // Its place in this repository, whatever repository the ticket names (a tracker without any).
    // The ticket's worktree when it has one already; otherwise a new one here, or the clone.
    const own = a.launch.cwd && a.launch.cwd !== path ? a.launch.cwd : null
    const place = own ? { cwd: own } : worktree ? { worktree: { ...(a.launch.worktree ?? {}), repo } } : { cwd: path }
    const launch: Launch = { ...a.launch, cwd: undefined, worktree: undefined, ...place }
    if (!direct) {
      openLauncher(launch, a.href, a.onLaunch, from.ticket.id)
      return null
    }
    const id = await launchNow(launch)
    if (id) await afterLaunch(a.onLaunch, tr('Agent started'))
    return id
  }
  const prompt = from.prompt.trim()
  const title = workstreamTitle(prompt)
  const launch: Launch = { title, prompt, ...(worktree ? { worktree: { repo, name: title } } : placeIn({ name: repo, path }, false)) }
  if (!direct) {
    openLauncher(launch, null, null, repo, true)
    return null
  }
  return launchNow(launch)
}

/** A prompt's first line, cut at a word under 60 characters: the workstream's (and its agent's) title. */
export function workstreamTitle(prompt: string): string {
  const line = prompt.split('\n').find((l) => l.trim())?.trim() ?? ''
  if (line.length <= 60) return line
  const cut = line.slice(0, 60)
  return `${cut.slice(0, cut.lastIndexOf(' ') > 30 ? cut.lastIndexOf(' ') : 60).trimEnd()}…`
}
