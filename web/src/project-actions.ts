import { invoke } from './bridge'
import { live } from './live.svelte'
import { openAgents, openLauncher } from './agents.svelte'
import { hasWorktrees } from './repositories'
import { isBlocked } from './tickets'
import { runClaude, ticketActions } from './claude-actions'
import { nextTicket, placeIn } from './projects'
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
