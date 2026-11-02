import type { Group } from './agents-list'
import type { LeftSession, Ticket } from './types'

/**
 * The Agents view by project (grouped by repository): every repository of the config, its agents
 * or not, with where it was left (the last agent closed in it, `left`) and its next ticket. Those
 * with agents first, in the groups' order; then those left lately, most recent first; the others
 * rest, folded. Pure: the views give it their groups and the snapshot's parts.
 */
export interface Project {
  name: string
  /** Its group of the list; absent: no agent in it. */
  group: Group | null
  left: LeftSession | null
  next: Ticket | null
}

/** Left this long ago at most, a project without agents stays in sight. */
export const RECENT_DAYS = 14

export function projectsOf(o: { groups: Group[]; repos: string[]; left: LeftSession[]; next: (repo: string) => Ticket | null; now: number }): { active: Project[]; resting: Project[] } {
  const leftOf = (repo: string) => o.left.find((l) => l.repo === repo) ?? null
  const project = (name: string, group: Group | null): Project => ({ name, group, left: group ? null : leftOf(name), next: o.next(name) })
  const active = o.groups.map((g) => project(g.name, g))
  const rest = o.repos.filter((r) => !o.groups.some((g) => g.name === r)).map((r) => project(r, null))
  const recent = (p: Project) => !!p.left && o.now - Date.parse(p.left.closed_at) <= RECENT_DAYS * 86_400_000
  const lately = rest.filter(recent).sort((a, b) => b.left!.closed_at.localeCompare(a.left!.closed_at))
  return { active: [...active, ...lately], resting: rest.filter((p) => !recent(p)) }
}

/**
 * A repository's next ticket: one to pick (`statuses`, in their order: the first status first)
 * of its boards (Obsidian's say their repository), not blocked.
 */
export function nextTicket(tickets: Ticket[], statuses: string[], repo: string, blocked: (t: Ticket) => boolean): Ticket | null {
  const rank = (t: Ticket) => statuses.findIndex((s) => s.toLowerCase() === t.status.toLowerCase())
  return tickets.filter((t) => t.repo === repo && rank(t) >= 0 && !t.closed && !blocked(t)).sort((a, b) => rank(a) - rank(b))[0] ?? null
}
