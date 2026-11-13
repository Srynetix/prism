import type { Agent, LeftSession, MergeRequest, Pipeline, Repo, RepoInfo, Snapshot, Ticket } from './types'
import { describeWorktrees, isDone, latest, mrForSession, ticketIndex, waitsForYou, type WorktreeInfo } from './rules'
import { agentRepo, repoName } from './repositories'
import { ticketRef } from './tracker'

/**
 * The Projects page (`ProjectsView.svelte`): a project is a repository (`[[repos]]`), a workstream
 * is a branch of work in it, a repository + branch pair: its worktree, or the clone itself (a
 * workstream "on main" when an agent works there, shown while it has an agent or changes), its
 * agents, its ticket, its MR and its pipeline. Pure: the snapshot in, the view's lists out.
 */

/**
 * Where a workstream stands: agents on it (`active`), its MR open (`review`), its MR merged, its
 * branch gone or its ticket done (`cleanup`); otherwise nothing runs there, it waits to be picked up
 * again (`resume`: a worktree made by hand, an agent stopped).
 */
export type Stage = 'active' | 'review' | 'cleanup' | 'resume'
export const STAGES: Stage[] = ['active', 'review', 'cleanup']

export interface Workstream {
  /** `repo:path`: one per folder. */
  key: string
  repo: string
  branch: string | null
  /** Its folder: the worktree's, or the clone's. */
  path: string
  /** In the clone, not a worktree of it. */
  clone: boolean
  /** The worktree as the Worktrees block describes it (verdict, what a removal loses); absent: a clone without its worktrees listed. */
  info: WorktreeInfo | null
  agents: Agent[]
  ticketId: string | null
  ticket: Ticket | undefined
  mr: MergeRequest | undefined
  /** What it is about: its ticket's name, failing that its agent's title, its MR's, its branch. */
  title: string
  stage: Stage
  /** An agent of it waits for me: a question, a permission. */
  waiting: boolean
  /** Last sign of life (ISO). */
  activity: string | null
  ahead: number
  behind: number
  /** Files changed, when counted (a worktree's). */
  changes: number
  /** Local changes, counted or not (a clone whose worktrees are not listed says only that). */
  dirty: boolean
}

const live = (a: Agent) => a.state === 'running' || a.state === 'prepared'
export const waitingAgent = (a: Agent) => a.state === 'running' && waitsForYou(a.status) && !a.read

export function stageOf(o: { agents: Agent[]; mr: MergeRequest | undefined; clone: boolean; upstreamGone?: boolean; ticket?: Ticket }): Stage {
  if (o.agents.some(live)) return 'active'
  if (o.mr?.state === 'opened') return 'review'
  if (!o.clone && (o.mr?.state === 'merged' || o.upstreamGone || (!!o.ticket && isDone(o.ticket)))) return 'cleanup'
  return 'resume'
}

/** A workstream's MR: its worktree's, else the one linked to an agent of it, else its branch's. */
function mrOf(s: Snapshot, repo: string, branch: string | null, agents: Agent[], found?: MergeRequest): MergeRequest | undefined {
  if (found) return found
  for (const a of agents) {
    const linked = a.mr && s.merge_requests.find((m) => m.iid === a.mr!.iid && repoName(m.repo, s) === repoName(a.mr!.repo, s))
    if (linked) return linked
  }
  if (!branch) return undefined
  const mine = (m: MergeRequest) => repoName(m.repo, s) === repo && m.source_branch === branch
  return s.merge_requests.find((m) => mine(m) && m.state === 'opened') ?? s.merge_requests.find(mine) ?? (agents[0] ? mrForSession(s, { worktree: agents[0].worktree, branch }) : undefined)
}

/**
 * The workstreams of repository `repo`: its worktrees (not archived: `hidden`, nor gone), and its
 * clone while an agent works in it or it has changes. The agents of the repository no worktree
 * claims make theirs (a worktree not listed: its own; none: the clone's).
 */
/** The tickets Prism knows, by `ticketRef` and by id: built once for every project. */
export interface TicketLookup {
  byRef: ReturnType<typeof ticketIndex>
  byId: Map<string, Ticket>
}
export function ticketLookup(s: Snapshot): TicketLookup {
  const byRef = ticketIndex(s)
  const byId = new Map<string, Ticket>()
  for (const { t } of byRef.values()) if (!byId.has(t.id)) byId.set(t.id, t)
  return { byRef, byId }
}

export function workstreamsOf(s: Snapshot, repo: string, o: { infos?: WorktreeInfo[]; tickets?: TicketLookup; hidden?: (i: WorktreeInfo) => boolean } = {}): Workstream[] {
  const tickets = o.tickets ?? ticketLookup(s)
  const lookup = (id: string | null | undefined) => (id ? (tickets.byRef.get(ticketRef(id, repo))?.t ?? tickets.byId.get(id)) : undefined)
  const clone = s.repos?.find((r) => r.name === repo)
  const infos = (o.infos ?? describeWorktrees(s)).filter((i) => i.w.repo === repo)
  const claimed = new Set(infos.flatMap((i) => i.agents.map((a) => a.id)))
  const loose = (s.agents ?? []).filter((a) => !claimed.has(a.id) && agentRepo(a, s) === repo)
  const out: Workstream[] = []

  const make = (x: Omit<Workstream, 'key' | 'repo' | 'stage' | 'waiting' | 'title' | 'ticket' | 'mr'> & { mr?: MergeRequest; ticket?: Ticket; upstreamGone?: boolean }): Workstream => {
    const ticketId = x.ticketId ?? x.agents.find((a) => a.ticket)?.ticket ?? null
    const ticket = x.ticket ?? lookup(ticketId)
    const mr = x.clone && x.branch && clone && x.branch === clone.base ? undefined : mrOf(s, repo, x.branch, x.agents, x.mr)
    const first = [...x.agents].sort((a, b) => Number(live(b)) - Number(live(a)))[0]
    const { upstreamGone, ...rest } = x
    return {
      ...rest, key: `${repo}:${x.path}`, repo, ticketId: ticket?.id ?? ticketId, ticket, mr,
      title: ticket?.name || first?.title || mr?.title || x.branch || x.path.split('/').pop() || x.path,
      stage: stageOf({ agents: x.agents, mr, clone: x.clone, upstreamGone, ticket }),
      waiting: x.agents.some(waitingAgent),
    }
  }

  for (const i of infos) {
    if (i.w.prunable || o.hidden?.(i)) continue
    const agents = [...i.agents, ...(i.w.is_main ? loose.filter((a) => !a.worktree) : [])]
    if (i.w.is_main && !agents.length && !i.w.changes && !i.w.untracked) continue
    out.push(make({
      branch: i.w.branch, path: i.w.path, clone: i.w.is_main, info: i, agents, ticketId: i.w.ticket, ticket: i.ticket, mr: i.mr,
      activity: latest([i.activity, ...agents.map((a) => a.last_activity ?? a.created_at)]), ahead: i.w.ahead, behind: i.w.behind, changes: i.w.changes + (i.w.untracked ?? 0), dirty: !!(i.w.changes || i.w.untracked), upstreamGone: i.w.upstream_gone,
    }))
  }
  const hasMain = infos.some((i) => i.w.is_main)
  // Agents in a worktree the snapshot does not list: one workstream per folder.
  const byFolder = new Map<string, Agent[]>()
  for (const a of loose) {
    if (!a.worktree && hasMain) continue
    const dir = a.worktree ?? clone?.path ?? a.cwd
    byFolder.set(dir, [...(byFolder.get(dir) ?? []), a])
  }
  const inClone = (dir: string) => !!clone && dir === clone.path
  // The clone, when its worktrees are not listed: shown while an agent works there or it has changes.
  if (clone && !hasMain && !byFolder.has(clone.path) && clone.dirty) byFolder.set(clone.path, [])
  for (const [dir, agents] of byFolder) {
    const isClone = inClone(dir)
    out.push(make({
      branch: isClone ? clone!.branch : (agents.find((a) => a.branch)?.branch ?? null), path: dir, clone: isClone, info: null, agents, ticketId: null,
      activity: latest(agents.map((a) => a.last_activity ?? a.created_at)), ahead: isClone ? clone!.ahead : 0, behind: isClone ? clone!.behind : 0, changes: 0, dirty: isClone && clone!.dirty,
    }))
  }
  return out
}

/** Those waiting for me first, then the most recently active. */
export const byActivity = (a: Workstream, b: Workstream) => Number(b.waiting) - Number(a.waiting) || (b.activity ?? '').localeCompare(a.activity ?? '')

/** A project of the list: its repository, its working folder, its signals. */
export interface ProjectSummary {
  name: string
  repo: Repo | undefined
  info: RepoInfo | undefined
  /** Its working folder (`[[workspaces]] name`), absent: outside them. */
  workspace: string | null
  workstreams: Workstream[]
  /** Workstreams with an agent waiting for me. */
  waiting: number
  /** Its open MRs (everyone's). */
  mrs: MergeRequest[]
  /** The last pipeline of its default branch, when it failed. */
  mainFailed: Pipeline | undefined
  mainPipeline: Pipeline | undefined
  /** The last agent closed in it. */
  left: LeftSession | null
  /** Nothing in progress: shown dimmed. */
  resting: boolean
}

/** The base branch's last pipeline (`main`), of those Prism reads. */
export function mainPipelineOf(s: Snapshot, repo: string, base: string | undefined): Pipeline | undefined {
  return s.pipelines.filter((p) => repoName(p.repo, s) === repo && (!base || p.target === base)).sort((a, b) => b.updated_at.localeCompare(a.updated_at))[0]
}

/**
 * Every project of the config, its workstreams counted, grouped by working folder (in the
 * config's order, those outside them last), and in each those with someone waiting first, then
 * those with work in progress, then those at rest.
 */
export function projectSummaries(s: Snapshot, o: { hidden?: (i: WorktreeInfo) => boolean } = {}): ProjectSummary[] {
  const infos = describeWorktrees(s)
  const tickets = ticketLookup(s)
  const names = [...new Set([...(s.repositories ?? []).map((r) => r.name), ...(s.repos ?? []).map((r) => r.name)])]
  const list = names.map((name): ProjectSummary => {
    const repo = s.repos?.find((r) => r.name === name)
    const info = s.repositories?.find((r) => r.name === name)
    const workstreams = workstreamsOf(s, name, { infos, tickets, hidden: o.hidden })
    const mainPipeline = mainPipelineOf(s, name, repo?.base)
    const mrs = s.merge_requests.filter((m) => m.state === 'opened' && repoName(m.repo, s) === name)
    return {
      name, repo, info, workspace: info?.workspace ?? null, workstreams,
      waiting: workstreams.filter((w) => w.waiting).length, mrs,
      mainPipeline, mainFailed: mainPipeline?.status === 'failed' ? mainPipeline : undefined,
      left: (s.left ?? []).find((l) => l.repo === name) ?? null,
      resting: !workstreams.some((w) => w.stage !== 'resume') && !mrs.length,
    }
  })
  const folders = [...new Set(list.map((p) => p.workspace).filter((w): w is string => !!w))]
  const folderRank = (p: ProjectSummary) => (p.workspace ? folders.indexOf(p.workspace) : folders.length)
  const rank = (p: ProjectSummary) => (p.waiting ? 0 : p.resting ? 2 : 1)
  return list.map((p, i) => ({ p, i })).sort((a, b) => folderRank(a.p) - folderRank(b.p) || rank(a.p) - rank(b.p) || a.i - b.i).map((x) => x.p)
}

/**
 * The tickets a new workstream of `repo` can start from: those of its boards (a ticket naming no
 * repository goes with the primary one), not done, not already worked on in a workstream; the
 * next one to pick (`next`) first.
 */
export function ticketsFor(s: Snapshot, repo: string, taken: Workstream[], next?: Ticket | null): Ticket[] {
  const primary = s.repositories?.find((r) => r.primary)?.name
  const busy = new Set(taken.map((w) => w.ticketId).filter(Boolean))
  const list = [...ticketIndex(s).values()].map((x) => x.t).filter((t) => (t.repo ? t.repo === repo : repo === primary) && !t.closed && !isDone(t) && !busy.has(t.id))
  return next ? [...list.filter((t) => t.id === next.id), ...list.filter((t) => t.id !== next.id)] : list
}
