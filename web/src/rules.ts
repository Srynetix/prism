import type { Agent, ArchivedSession, ClaudeSession, MergeRequest, Pipeline, PodView, Snapshot, StatusOption, Ticket, Worktree } from './types'
import type { Alert } from './bindings/Alert'
import { minutesSince } from './time'
import { alertSettings, conventions, isDone, isProtected } from './conventions'
import { mrKey } from './mr-ref'
import { hasMergeQueue, repoName } from './repositories'
import { ticketRef } from './tracker'
import { BLOCK_DEFS } from './blocks'
import { stateOf } from './blocks/block'
import { locale } from './locale'
import { t as tr } from './i18n.svelte'
import { baseName } from './platform'

export { isDone, isProtected }

/**
 * The alerts are the core's (`Snapshot.alerts`, src-tauri/crates/core/src/alerts): computed from
 * the snapshot on each change, the window open or not. Their types are the core's too.
 */
export type { Alert } from './bindings/Alert'
export type { Fix } from './bindings/Fix'
export type { OpenTarget } from './bindings/OpenTarget'
export type { Severity } from './bindings/Severity'

/**
 * Tickets Prism knows, with the statuses of their list (to offer a change), by `ticketRef`: an MR
 * finds its ticket with `ticketRef(m.ticket, repoName(m.repo, s))`.
 */
export function ticketIndex(s: Snapshot): Map<string, { t: Ticket; statuses: StatusOption[] }> {
  const index = new Map<string, { t: Ticket; statuses: StatusOption[] }>()
  const add = (tickets: Ticket[] | undefined, statuses: (t: Ticket) => StatusOption[]) => {
    for (const t of tickets ?? []) {
      const key = ticketRef(t.id, t.repo)
      if (!index.has(key)) index.set(key, { t, statuses: statuses(t) })
    }
  }
  // A sprint of several boards (Obsidian): each ticket its own board's lanes.
  add(s.sprint?.tickets, (t) => s.sprint?.boards?.find((b) => b.id === t.list?.id)?.statuses ?? s.sprint?.statuses ?? [])
  for (const b of s.pickup?.boards ?? []) add(b.tickets, () => b.statuses)
  add(s.outside_sprint?.tickets, (t) => (t.list ? (s.outside_sprint?.list_statuses[t.list.id] ?? []) : []))
  return index
}

/** Beyond this, a rebase becomes painful: it is flagged before (`[alerts] behind_commits`). */
export const behindLimit = () => alertSettings().behind_commits

/** Fallback colors of the statuses, when ClickUp did not provide the board's. */
const STATUS_COLORS: Record<string, string> = {
  done: 'var(--ok)', complete: 'var(--ok)', review: 'var(--info)', 'to merge': 'var(--info)', 'to validate': 'var(--violet)',
  validation: 'var(--violet)', doing: 'var(--warn)', 'in progress': 'var(--warn)',
}

/** Color of a status: ClickUp's for this board, otherwise Prism's, otherwise neutral. */
export const statusColor = (status: string, options: { status: string; color: string | null }[] = []) =>
  options.find((s) => s.status === status)?.color ?? STATUS_COLORS[status] ?? 'var(--text-muted)'

/** Rank of a status in the flow (`[clickup] workflow_statuses`); an unknown status after all. */
export const statusRank = (status: string) => {
  const order = conventions().workflow_statuses
  return order.indexOf(status) + 1 || order.length + 1
}
/** In review, not yet at the step after approval (`review`, not `to merge`). */
export const awaitingApproval = (status: string) => conventions().review_statuses.includes(status) && !(conventions().approved_statuses ?? []).includes(status)

export const openMrs = (s: Snapshot) => s.merge_requests.filter((m) => m.state === 'opened')

/** MRs merged on the given day (`YYYY-MM-DD`, local time). */
export function mergedOn(s: Snapshot, day: string): MergeRequest[] {
  return s.merge_requests
    .filter((m) => m.state === 'merged' && m.merged_at && new Date(m.merged_at).toLocaleDateString('en-CA') === day)
    .sort((a, b) => (b.merged_at ?? '').localeCompare(a.merged_at ?? ''))
}

export const failedStage = (p: Pipeline) => p.stages.find((st) => st.failed > 0 || st.status === 'failed')?.name

export type MergeTone = 'ok' | 'ko' | 'warn' | 'info' | ''

/** Merge state of an MR in one word, in order of what blocks it. */
export function mergeState(m: MergeRequest): { label: string; tone: MergeTone } {
  if (m.state === 'merged') {
    const at = m.merged_at ? new Date(m.merged_at).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' }) : ''
    return { label: tr('merged {at}', { at }).trim(), tone: 'ok' }
  }
  if (m.draft) return { label: tr('draft'), tone: '' }
  if (m.conflicts) return { label: tr('conflicts'), tone: 'ko' }
  if (m.merge_status === 'need_rebase') return { label: tr('rebase'), tone: 'warn' }
  if (m.auto_merge) return { label: m.pipeline === 'success' ? tr('auto-merge') : tr('auto-merge · CI'), tone: m.pipeline === 'failed' ? 'ko' : 'info' }
  if (m.pipeline === 'failed') return { label: tr('blocked by CI'), tone: '' }
  if (changesRequested(m)) return { label: tr('changes requested'), tone: 'warn' }
  if (m.approvals_left > 0) return { label: tr('in review'), tone: '' }
  if (m.open_threads > 0) return { label: tr('open threads'), tone: 'warn' }
  if (m.merge_status === 'mergeable') return { label: tr('ready'), tone: 'ok' }
  return { label: m.merge_status.replaceAll('_', ' ') || '—', tone: '' }
}

/**
 * Next step of an MR, in plain words: what blocks it or what is left to do, and by whom.
 * The tint only says that: blocked, to do by me, waiting for someone else, ready, automatic,
 * or nothing to report. `primary`: the action that unblocks, offered as a button on the line.
 */
export type StepTone = 'blocked' | 'todo' | 'wait' | 'ready' | 'auto' | 'idle'
export type PrimaryAction = 'claude-fix' | 'claude-review' | 'claude-rebase' | 'rebase' | 'queue' | 'merge' | 'automerge' | 'logs'
export interface NextStep {
  label: string
  tone: StepTone
  primary?: PrimaryAction
}

const RUNNING = new Set(['created', 'waiting_for_resource', 'preparing', 'pending', 'running'])

export function nextStep(m: MergeRequest, me: string | null, queueLabel?: string | null): NextStep {
  // The merge queue is a job of the repository's CI (`[[repos]] merge_queue`).
  if (!hasMergeQueue(m)) queueLabel = null
  if (m.state === 'merged') {
    const at = m.merged_at ? new Date(m.merged_at).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' }) : ''
    return { label: tr('Merged {at}', { at }).trim(), tone: 'ready' }
  }
  const mine = !!me && m.author === me
  if (m.draft) return { label: tr('Draft'), tone: 'idle' }
  if (m.conflicts) return { label: tr('Conflicts'), tone: 'blocked', primary: mine ? 'claude-rebase' : undefined }
  // Before the merge queue and auto-merge: the queue's job asks GitLab for the rebase that failed.
  if (rebaseFailed(m)) return { label: tr('Rebase failed'), tone: 'blocked', primary: mine ? 'claude-rebase' : undefined }
  if (m.pipeline === 'failed') return { label: tr('CI failed'), tone: 'blocked', primary: mine ? 'claude-fix' : 'logs' }
  if (queueLabel && m.labels?.includes(queueLabel)) return { label: tr('In merge queue'), tone: 'auto' }
  if (m.auto_merge) return { label: m.pipeline && RUNNING.has(m.pipeline) ? tr('Merges after CI') : tr('Merging'), tone: 'auto' }
  // Before a review of mine: a reviewer is waiting for the author's changes.
  if (changesRequested(m)) return { label: tr('Changes requested'), tone: mine ? 'todo' : 'wait' }
  if (!mine && awaitsMyReview(m, me)) return { label: tr('Review it'), tone: 'todo', primary: 'claude-review' }
  if (m.rebase_in_progress) return { label: tr('Rebasing…'), tone: 'wait' }
  if (m.merge_status === 'need_rebase' || m.needs_rebase) return { label: tr('Rebase needed'), tone: mine ? 'todo' : 'wait', primary: mine ? 'rebase' : undefined }
  // My MR only waiting for its CI: auto-merge rather than coming back once it is green. The
  // team's merge queue, when it has one, takes it once ready.
  if (m.pipeline && RUNNING.has(m.pipeline)) return { label: tr('CI running'), tone: 'wait', ...(mine && !queueLabel && onlyWaitsForCi(m) && { primary: 'automerge' as const }) }
  if (m.approvals_left > 0) return { label: mine ? tr('Waiting for review') : tr('In review'), tone: 'wait' }
  if (m.open_threads > 0) return { label: tr('Open threads'), tone: mine ? 'todo' : 'wait' }
  // My MR ready: the team's merge queue when it has one, otherwise a merge now.
  if (m.merge_status === 'mergeable') return { label: tr('Ready to merge'), tone: 'ready', primary: mine ? (queueLabel ? 'queue' : 'merge') : undefined }
  return { label: m.merge_status.replaceAll('_', ' ') || '—', tone: 'idle' }
}

/**
 * An open MR with nothing left but its running CI (approved, no thread, no conflict nor rebase,
 * no auto-merge yet): the forge could merge it by itself once green. In the merge queue or not.
 */
export const onlyWaitsForCi = (m: MergeRequest) =>
  m.state === 'opened' && !m.draft && !m.auto_merge && !m.conflicts && !rebaseFailed(m) && !m.rebase_in_progress && !m.needs_rebase && m.merge_status !== 'need_rebase'
  && !!m.pipeline && RUNNING.has(m.pipeline) && !changesRequested(m) && m.approvals_left <= 0 && m.open_threads === 0

/** Security label of an MR (`security`, `type::security`, `Security fix`…), if there is one. */
export const securityLabel = (m: MergeRequest) => m.labels?.find((l) => /(^|::|\b)security\b/i.test(l))

/**
 * Rebase to plan, to flag even when another blocker occupies the merge state. Without
 * conflict, GitLab can do it itself: that is what the ↻ button offers.
 */
/** GitLab would merge the MR now (`mergeable`: pipeline, approvals, threads, no draft nor conflict). */
export const canMerge = (m: MergeRequest) => m.state === 'opened' && !m.draft && !m.auto_merge && m.merge_status === 'mergeable'

/**
 * GitLab's last rebase of the MR failed (`Rebase failed: Rebase locally, resolve all conflicts,
 * then push the branch.`), whoever asked for it (the Rebase button, a merge queue's job): a commit
 * in between conflicts although the branches do not as a whole. Whatever its merge status, which
 * GitLab does not always set back to "to rebase"; rebased locally, by an agent, rather than retried.
 */
export const rebaseFailed = (m: MergeRequest) => m.state === 'opened' && !m.conflicts && !m.rebase_in_progress && !!m.merge_error && /rebase/i.test(m.merge_error)

/** GitLab's reason, without its `Rebase failed:` lead. */
/**
 * The MR may go into the merge queue: neither a draft nor in conflict, its last rebase did not
 * fail — the queue's job would only fail the same rebase again — and no required approval is
 * missing: the forge would refuse the merge.
 */
export const canQueue = (m: MergeRequest) => !m.draft && !m.conflicts && !rebaseFailed(m) && m.approvals_left <= 0

export const rebaseFailure = (m: MergeRequest) => (m.merge_error ?? '').replace(/^rebase failed:\s*/i, '').trim()

export const rebaseHint = (m: MergeRequest) =>
  m.state === 'opened' && (m.needs_rebase || m.merge_status === 'need_rebase') && !m.conflicts && !m.rebase_in_progress

export function awaitsMyReview(m: MergeRequest, me: string | null): boolean {
  // Changes requested by me: the author's turn, not mine.
  return !!me && m.state === 'opened' && !m.draft && m.reviewers.includes(me) && !m.approved_by.includes(me) && !(m.changes_requested_by ?? []).includes(me)
}

/** Needs me: to review, or mine and blocked or to act on (not a draft, nor only waiting). */
export function mrNeedsMe(m: MergeRequest, me: string | null, queueLabel?: string | null): boolean {
  if (awaitsMyReview(m, me)) return true
  if (m.author !== me) return false
  const s = nextStep(m, me, queueLabel)
  return s.tone === 'blocked' || s.tone === 'todo' || (s.tone === 'ready' && !!s.primary)
}

/** A reviewer asked for changes, and has not approved since: the MR is not ready to merge. */
export const changesRequested = (m: MergeRequest) => (m.changes_requested_by ?? []).length > 0

/** Sessions whose priority order reflects what they expect from me. */
export const SESSION_RANK: Record<ClaudeSession['status'], number> = {
  needs_you: 0, waiting_tool: 1, review_ready: 2, running: 3, background: 4, idle: 5, completed: 6,
}
export const SESSION_LABEL: Record<ClaudeSession['status'], string> = {
  needs_you: tr('Needs your answer'),
  waiting_tool: tr('Tool or permission'),
  review_ready: tr('Ready to review'),
  running: tr('Running'),
  background: tr('In the background'),
  idle: tr('Idle'),
  completed: tr('Completed'),
}
/** A session's status in words: one marked read (`session-read.ts`) shows as stopped. */
export const sessionLabel = (x: { status: ClaudeSession['status']; read?: boolean }) => (x.read ? tr('Stopped') : SESSION_LABEL[x.status])
/** A status waiting for an answer or a permission. */
export const waitsForYou = (s: ClaudeSession['status']) => s === 'needs_you' || s === 'waiting_tool'
/** Working: on its turn, or its turn over while its subagents or background tasks go on. */
export const isWorking = (s: ClaudeSession['status']) => s === 'running' || s === 'background'
/** An agent that does not run, in words: the same on the computer and the phone. */
export const AGENT_STATE: Record<Agent['state'], string> = { running: '', exited: tr('Exited'), gone: tr('Stopped'), prepared: tr('Prepared') }
/** An agent's state in words: its CLI's while it runs, otherwise why it stopped; `short` in the phone's header. */
export const agentStateLabel = (a: Agent, short = false) =>
  (a.state === 'running' && a.gate ? (short ? tr('Your trust') : tr('Waits for your trust')) : '') ||
  (a.state === 'running' ? (a.status === 'waiting_tool' ? (short ? tr('Permission') : tr('Permission requested')) : sessionLabel(a)) : AGENT_STATE[a.state]) + (a.state === 'exited' && a.exit_code ? ` (${a.exit_code})` : '')
/**
 * The Claude sessions counted in Prism's figures (header, status bar, menu bar,
 * Alerts, notifications): Prism's agents, and Claude Desktop's with
 * `[agents.claude] count_desktop`, when they are shown. The Worktrees view shows them all.
 */
export const countedSessions = (s: Snapshot) => s.sessions.filter((c) => c.origin === 'agent' || (!!s.agents_settings?.claude?.count_desktop && desktopShown(s.agents_settings)))

/**
 * Claude Desktop sessions are listed in the Agents view (`[agents.claude] show_desktop`): only with
 * Claude Code agents, never with Command Code, OpenCode or Codex ones, whose work they are not part of.
 */
export const desktopShown = (a: Snapshot['agents_settings'] | undefined) => a?.claude?.show_desktop !== false && (a?.provider ?? 'claude') === 'claude'

/** Session waiting for an answer or a permission. */
export const isWaiting = (c: ClaudeSession) => waitsForYou(c.status)
/** Beyond this, a waiting session is considered set aside: no more alert. */
export const waitingAlertHours = () => alertSettings().waiting_session_hours
/**
 * A Prism agent's turn over, asking nothing (Your turn): stopping there is often the point, so it is flagged
 * a short while only (`[alerts] your_turn_minutes`, 0: never), unlike a question, a permission or a plan.
 */
export const yourTurnOnly = (c: Pick<ClaudeSession, 'status' | 'needs_action' | 'origin'>) => c.origin === 'agent' && c.status === 'needs_you' && !c.needs_action
export const waitingAlert = (c: ClaudeSession) => isWaiting(c) && minutesSince(c.last_activity) < (yourTurnOnly(c) ? alertSettings().your_turn_minutes : waitingAlertHours() * 60)

/** A session's worktree (an agent's too: the same fields). */
export const worktreeOf = (s: Snapshot, c: Pick<ClaudeSession, 'worktree' | 'branch'>) =>
  s.worktrees.find((w) => (c.worktree && w.path === c.worktree) || (!!c.branch && w.branch === c.branch))

export function mrForSession(s: Snapshot, c: Pick<ClaudeSession, 'worktree' | 'branch'>): MergeRequest | undefined {
  if (!c.branch) return undefined
  // The MRs of the repository the session works in: that of its worktree, otherwise the primary one.
  const repo = worktreeOf(s, c)?.repo ?? repoName(null, s)
  const mine = (m: MergeRequest) => repoName(m.repo, s) === repo && m.source_branch === c.branch
  return s.merge_requests.find((m) => mine(m) && m.state === 'opened') ?? s.merge_requests.find(mine)
}

/**
 * The alerts shown: the core's (`Snapshot.alerts`), and those of the modules' blocks
 * that say theirs in TypeScript only (`BlockDef.alerts`: Prism's own blocks say theirs in Rust,
 * `Block::alerts`). A snapshot from an older Prism (`alerts` its thresholds then) has none.
 */
export function alerts(s: Snapshot): Alert[] {
  return [...(Array.isArray(s.alerts) ? s.alerts : []), ...blockAlerts(s)]
}

/**
 * Pod really down (crash, image not found…): notified, and flagged by the badge
 * of the Infra view. A `Pending` pod is only waiting for room on the cluster.
 */
export const podDown = (p: PodView) => !p.healthy && p.status !== 'Pending'

/** A pod's key in `pod_usage`. */
export const usageKey = (p: PodView) => `${p.target}/${p.namespace}/${p.name}`

/** Modules' blocks that say their alerts in TypeScript: without them, none to tell the core. */
export const hasExternalAlerts = (): boolean => BLOCK_DEFS.some((b) => b.alerts)

/**
 * The alerts of the modules' blocks that define them in TypeScript, from their state:
 * the window tells the core (`alerts_external`) for the routines and the Changes block.
 */
export function blockAlerts(s: Snapshot): Alert[] {
  return BLOCK_DEFS.flatMap((b) => {
    const state = stateOf(b, s)
    return state !== undefined && b.alerts ? b.alerts(state as never).map((a) => ({ ...a, kind: b.label, block: b.id, rule: a.rule ?? 'block' })) : []
  })
}

/** An MR that has at least one approval, and no request for changes: one that may go "to merge". */
export const isApproved = (m: MergeRequest) => m.approved_by.length > 0 && !changesRequested(m)

/*
 * Worktrees: what each one is linked to (ticket, MR, Claude sessions), whether it is "stale", and whether it
 * can be deleted without losing anything. The Rust core re-checks everything before deleting.
 */

/** Origin: the main repository, Claude Desktop (`.claude/worktrees/`), a Prism agent, or by hand. */
export type WorktreeOrigin = 'main' | 'desktop' | 'agent' | 'manual'
export type WorktreeState = 'in_use' | 'stale' | 'idle' | 'gone'

export interface WorktreeInfo {
  w: Worktree
  name: string
  origin: WorktreeOrigin
  ticket: Ticket | undefined
  mr: MergeRequest | undefined
  /** Desktop sessions (not archived), Prism agents, archived sessions. */
  sessions: ClaudeSession[]
  agents: Agent[]
  archived: ArchivedSession[]
  /** Last sign of life: commit, session, agent. */
  activity: string | null
  state: WorktreeState
  /** Why it is stale. */
  reasons: string[]
  /** Who still works in it: prevents deletion. */
  busy: string | null
  /** What a deletion would lose. */
  keep: string | null
  removable: boolean
  /** Deletable only by accepting to lose `keep`. */
  forceable: boolean
}

/** Idle for less time than this, a worktree is in use. */
const DEFAULT_STALE_DAYS = 14
const LIVE_STATUSES = new Set<ClaudeSession['status']>(['running', 'background', 'needs_you', 'waiting_tool', 'review_ready'])

const latest = (dates: (string | null | undefined)[]) => dates.filter((d): d is string => !!d).sort().at(-1) ?? null

/** The worktree containing `dir`: the deepest, the main repository containing `.claude/worktrees/`. */
function ownerOf(dir: string | null | undefined, worktrees: Worktree[]): string | undefined {
  if (!dir) return undefined
  return worktrees
    .filter((w) => dir === w.path || dir.startsWith(`${w.path}/`))
    .sort((a, b) => b.path.length - a.path.length)[0]?.path
}

export function describeWorktrees(s: Snapshot): WorktreeInfo[] {
  const index = ticketIndex(s)
  const staleDays = s.worktree_stale_days ?? DEFAULT_STALE_DAYS
  const all = s.worktrees
  // Each folder's owner looked up once a call, not once a worktree (quadratic: 87 % of a snapshot
  // change with 30 worktrees and their agents and sessions).
  const owners = new Map<string, string | undefined>()
  const owner = (dir: string | null | undefined) => {
    if (!dir) return undefined
    if (!owners.has(dir)) owners.set(dir, ownerOf(dir, all))
    return owners.get(dir)
  }
  return all.map((w) => {
    const mine = (dir: string | null | undefined) => owner(dir) === w.path
    const sessions = s.sessions.filter((c) => c.origin === 'desktop' && mine(c.worktree))
    // An agent is linked to its worktree (or its folder), and to the worktrees its commands created.
    const agents = (s.agents ?? []).filter((a) => mine(a.worktree ?? a.cwd) || !!a.adopted?.some((p) => p === w.path) || !!a.also?.some((p) => p === w.path))
    const archived = (s.archived_sessions ?? []).filter((c) => mine(c.worktree))
    const byBranch = w.branch ? s.merge_requests.filter((m) => repoName(m.repo, s) === w.repo && m.source_branch === w.branch) : []
    const opened = byBranch.find((m) => m.state === 'opened')
    const merged = byBranch.filter((m) => m.state === 'merged').sort((a, b) => (b.merged_at ?? '').localeCompare(a.merged_at ?? ''))[0]
    const mr = opened ?? merged
    const ticket = w.ticket ? index.get(ticketRef(w.ticket, w.repo))?.t : undefined
    const origin: WorktreeOrigin = w.is_main ? 'main' : w.path.includes('/.claude/worktrees/') ? 'desktop' : agents.length ? 'agent' : 'manual'
    const activity = latest([w.last_commit, ...sessions.map((c) => c.last_activity), ...agents.map((a) => a.last_activity ?? a.created_at), ...archived.map((c) => c.last_activity)])

    const running = agents.find((a) => a.state === 'running')
    const busy = running ? tr('agent “{title}” is running', { title: running.title }) : sessions[0] ? tr('Claude Desktop session “{title}”', { title: sessions[0].title }) : null
    const live = !!running || sessions.some((c) => LIVE_STATUSES.has(c.status)) || (!!activity && minutesSince(activity) < alertSettings().worktree_in_use_hours * 60)

    const reasons: string[] = []
    if (!w.is_main && !w.prunable) {
      if (mr && mr === merged) reasons.push(tr('{mr} merged', { mr: mrKey(mr) }))
      if (w.upstream_gone) reasons.push(tr('branch gone on origin'))
      if (ticket && isDone(ticket)) reasons.push(`${ticket.id} ${ticket.status}`)
      if (archived.length && !sessions.length && !agents.length) reasons.push(tr('session archived'))
      const idle = activity ? Math.floor(minutesSince(activity) / 1440) : null
      if (idle !== null && idle >= staleDays) reasons.push(tr('idle {days}d', { days: idle }))
    }

    // Commits from before the merge of a squashed MR are shipped, even if absent from the remote.
    const delivered = !!merged && !opened && !!w.last_commit && !!merged.merged_at && w.last_commit <= merged.merged_at
    const lost = [
      w.changes ? tr('{n} uncommitted', { n: w.changes }) : '',
      w.untracked ? tr('{n} untracked', { n: w.untracked }) : '',
      w.unpushed && !delivered ? tr('{n} unpushed', { n: w.unpushed }) : '',
    ].filter(Boolean)
    const keep = lost.length ? lost.join(', ') : null

    const state: WorktreeState = w.prunable ? 'gone' : live ? 'in_use' : reasons.length ? 'stale' : 'idle'
    return {
      w, name: baseName(w.path) || w.path, origin, ticket, mr, sessions, agents, archived, activity, state, reasons, busy, keep,
      removable: !w.is_main && !w.prunable && !w.locked && !busy && !keep,
      forceable: !w.is_main && !w.prunable && !w.locked && !busy && !!keep,
    }
  })
}
