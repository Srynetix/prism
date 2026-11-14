import { invoke } from './bridge'
import { live } from './live.svelte'
import { afterLaunch, launchNow, openAgents, openLauncher, type Launch } from './agents.svelte'
import { hasWorktrees } from './repositories'
import { isBlocked } from './tickets'
import { implementAction, runClaude, ticketActions } from './claude-actions'
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
  if (!path) return null
  if ('ticket' in from) {
    // In this repository, whatever repository the ticket names (a tracker without any): the
    // ticket's worktree here, a new one where agents make them, the clone otherwise.
    const a = implementAction(from.ticket, repo, true)
    if (!a.launch) return null
    if (!direct) {
      openLauncher(a.launch, a.href, a.onLaunch, from.ticket.id)
      return null
    }
    const id = await launchNow(a.launch)
    if (id) await afterLaunch(a.onLaunch, tr('Agent started'))
    return id
  }
  const prompt = from.prompt.trim()
  // Its title names the new worktree too.
  const launch: Launch = { title: workstreamTitle(prompt), prompt, ...placeIn({ name: repo, path }, hasWorktrees(repo)) }
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
