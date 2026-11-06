import { branchName } from './pipeline'
import type { Agent, ArchivedSession, ClaudeSession, MergeRequest, Pipeline, PodView, Snapshot, StatusOption, Ticket, Worktree } from './types'
import type { ClaudeActionId } from './claude-actions'
import type { AlertRule } from './alert-bands'
import { minutesSince } from './time'
import { alertSettings, conventions, hasTickets, isDone, isProtected, mrTickets, ticketPrefix } from './conventions'
import { mrKey } from './mr-ref'
import { forgeName, meOn } from './forge'
import { hasMergeQueue, hasWorktrees, needsTicket, repoName } from './repositories'
import { ticketKey, ticketRef } from './tracker'
import { BLOCK_DEFS } from './blocks'
import { stateOf } from './blocks/block'
import { locale } from './locale'
import { argoKey } from './argo-view.svelte'
import { isHidden } from './worktree-archive.svelte'
import { USED_UP, planUseOf } from './usage'
import { t as tr, tn } from './i18n.svelte'
import { alertedOverlaps, overlapId, sideName } from './overlaps'

export { isDone, isProtected }

export type Severity = 'ko' | 'warn' | 'info'

export interface Alert {
  /** A block's alerts (`blocks/`) take its label. */
  kind: 'Source' | 'Infra' | 'CI' | 'Sync' | 'Review' | 'MR' | 'Claude' | 'ArgoCD' | 'Worktrees' | 'Data' | (string & {})
  /** The block with a source of its own that raised it (`blocks/`): always one of mine. */
  block?: string
  /** A macOS notification when broken (`ko`): one per `key`, the object, not its changing detail. */
  notification?: { key: string; title: string }
  severity: Severity
  text: string
  url?: string
  /** What "Open" shows in the app (a drawer) instead of `url`, which ⌘-click keeps. */
  open?: OpenTarget
  /** `false`: shown in Alerts, but without a macOS notification (pending jobs…). */
  notify?: boolean
  /** Fixes offered in one click in Alerts (Alerts.svelte executes them). */
  fixes?: Fix[]
  /** The agent the alert waits on: its quick answers in the row. */
  answer?: string
  /** The rule that raised it: its band (`[alerts] band_now`, `band_later`, `band_hidden`). */
  rule?: AlertRule
  /** Since when it holds, when the data says it; otherwise Prism counts from when it first saw it. */
  since?: string
  /** Its few words beside the others of its object (the same MR, the same ticket), grouped. */
  short?: string
}

/** An alert's object, opened in the app: a ticket's preview, an MR's pane, a pipeline's logs, the ArgoCD drawer. */
export type OpenTarget =
  | { kind: 'ticket'; id: string }
  | { kind: 'mr'; iid: number; repo?: string | null }
  | { kind: 'pipeline'; pipeline: number; label: string; project?: string | null }
  | { kind: 'argo'; cluster: string; app: string }
const ticketOpen = (t: Ticket): OpenTarget => ({ kind: 'ticket', id: ticketKey(t) })
const mrOpen = (m: MergeRequest): OpenTarget => ({ kind: 'mr', iid: m.iid, repo: m.repo })

/**
 * Fix of an alert, as data: this module stays free of side effects. `status`: change the
 * status of a ticket; `rebase`: rebase by GitLab; `queue`: into the merge queue;
 * `retry` / `logs`: retry the failed jobs or open a pipeline's logs; `mr`: open
 * an MR's pane; `claude`: the Claude action of that name for the MR; `fix-branch`: an agent
 * fixing a protected branch's pipeline; `investigate`: an agent diagnosing a workload;
 * `dev-server`: the server's own agent, told why it stopped; `switch`: another CLI to continue an
 * agent stopped at its plan's limit (Continue with…); `block`: a command of a block's Rust
 * side (`blocks/block.ts` `call`).
 */
export type Fix =
  | { kind: 'status'; task_id: string; ticket: string; status: string; statuses: StatusOption[] }
  | { kind: 'rebase' | 'mr' | 'queue' | 'automerge'; iid: number; repo?: string | null }
  | { kind: 'retry' | 'logs'; pipeline: number; label: string; project?: string | null }
  | { kind: 'claude'; iid: number; repo?: string | null; action: ClaudeActionId }
  | { kind: 'podlogs'; context: string; namespace: string; deployment: string; label: string }
  | { kind: 'settings'; section: string; label: string }
  | { kind: 'fix-branch'; pipeline: number }
  | { kind: 'investigate'; workload: string; namespace: string; context: string; env: string; issue: string }
  | { kind: 'dev-server'; agent: string; command: string; where: string; tail: string }
  | { kind: 'switch'; agent: string }
  | { kind: 'overlap'; id: string; side: 'a' | 'b' }
  | { kind: 'block'; block: string; command: string; args: Record<string, unknown>; label: string; title: string }

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

/** Move the ticket to the first of these statuses its list knows; none, no fix. */
function moveTo(entry: { t: Ticket; statuses: StatusOption[] } | undefined, wanted: string[]): Fix[] {
  if (!entry) return []
  const status = wanted.find((w) => w !== entry.t.status && entry.statuses.some((o) => o.status === w))
  return status ? [{ kind: 'status', task_id: entry.t.task_id, ticket: entry.t.id, status, statuses: entry.statuses }] : []
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
/** Before development, and after merge (the ticket goes to validation: this is not a desync). */
const beforeDev = (status: string) => conventions().before_dev_statuses.includes(status)
const afterMerge = (status: string) => conventions().after_merge_statuses.includes(status)
const inProgress = (status: string) => conventions().in_progress_statuses.includes(status)
/** In review, not yet at the step after approval (`review`, not `to merge`). */
export const awaitingApproval = (status: string) => conventions().review_statuses.includes(status) && !(conventions().approved_statuses ?? []).includes(status)
/** Blocked ticket (`[clickup] blocked_statuses`): the Sync rules leave it alone. */
const blocked = (status: string) => (conventions().blocked_statuses ?? []).includes(status)

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
/** A status waiting for an answer or a permission. */
export const waitsForYou = (s: ClaudeSession['status']) => s === 'needs_you' || s === 'waiting_tool'
/** Working: on its turn, or its turn over while its subagents or background tasks go on. */
export const isWorking = (s: ClaudeSession['status']) => s === 'running' || s === 'background'
/** An agent that does not run, in words: the same on the computer and the phone. */
export const AGENT_STATE: Record<Agent['state'], string> = { running: '', exited: tr('Exited'), gone: tr('Stopped'), prepared: tr('Prepared') }
/** An agent's state in words: its CLI's while it runs, otherwise why it stopped; `short` in the phone's header. */
export const agentStateLabel = (a: Agent, short = false) =>
  (a.state === 'running' ? (a.status === 'waiting_tool' ? (short ? tr('Permission') : tr('Permission requested')) : SESSION_LABEL[a.status]) : AGENT_STATE[a.state]) + (a.state === 'exited' && a.exit_code ? ` (${a.exit_code})` : '')
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

export function alerts(s: Snapshot): Alert[] {
  return [...ruled('source', sourceAlerts(s)), ...ruled('infra', infraAlerts(s)), ...ruled('block', blockAlerts(s)), ...ruled('argo', argoAlerts(s)), ...ruled('branch-ci', ciAlerts(s)), ...ruled('sync', [...syncAlerts(s), ...draftAlerts(s), ...inReviewAlerts(s), ...approvedAlerts(s)]), ...ruled('queue', queueAlerts(s)), ...sessionAlerts(s), ...ruled('overlap', overlapAlerts(s)), ...ruled('worktree', worktreeAlerts(s)), ...ruled('review', reviewAlerts(s)), ...myMrAlerts(s)]
}

/** A producer's rule, on those of its alerts that name none of their own. */
const ruled = (rule: AlertRule, list: Alert[]) => list.map((a) => (a.rule ? a : { ...a, rule }))

/** The oldest of these dates (ISO), for an alert standing for several. */
const oldest = (dates: (string | null | undefined)[]) => dates.filter((d): d is string => !!d).sort()[0]

function sessionAlerts(s: Snapshot): Alert[] {
  const out: Alert[] = []
  const recent = countedSessions(s).filter(waitingAlert)
  const waiting = recent.filter((c) => c.status === 'needs_you')
  for (const c of recent.filter((c) => c.status === 'waiting_tool')) {
    out.push({ kind: 'Claude', rule: 'permission', since: c.last_activity ?? undefined, severity: 'warn', text: tr('“{title}” is stuck on a tool or a permission prompt', { title: c.title }) })
  }
  if (waiting.length > 0) {
    // One agent alone waits: its quick answers in the row.
    const agents = (s.agents ?? []).filter((a) => a.state === 'running' && a.status === 'needs_you')
    const answer = waiting.length === 1 && agents.length === 1 ? agents[0].id : undefined
    const text = answer ? tr('“{title}” is waiting for your answer', { title: agents[0].title }) : tn(waiting.length, '{n} session is waiting for your answer', '{n} sessions are waiting for your answer')
    // A question, a plan to approve: someone waits on you. Only turns just over: nothing presses.
    const rule = waiting.every(yourTurnOnly) ? 'your-turn' : 'question'
    out.push({ kind: 'Claude', rule, since: oldest(waiting.map((c) => c.last_activity)), severity: 'info', text, answer })
  }
  // A Claude Code agent costing more than `[agents.claude] cost_alert` (its conversation, at the API prices).
  const limit = s.agents_settings?.claude?.cost_alert ?? 0
  for (const a of (s.agents ?? []).filter((a) => a.provider === 'claude' && limit > 0 && (a.usage?.cost ?? 0) >= limit)) {
    out.push({ kind: 'Claude', rule: 'agent-cost', severity: 'warn', text: tr('“{title}” has cost about ${cost} (beyond ${limit})', { title: a.title, cost: a.usage!.cost!.toFixed(2), limit }) })
  }
  // An agent whose CLI's plan is used up, waiting: another CLI can take it on (`[agents] suggest_switch`).
  if (s.agents_settings?.suggest_switch ?? true) {
    for (const a of (s.agents ?? []).filter((a) => a.state !== 'prepared' && !isWorking(a.status) && (planUseOf(s, a.provider) ?? 0) >= USED_UP)) {
      out.push({ kind: 'Claude', rule: 'agent-limit', severity: 'warn', text: tr('“{title}” stopped at its plan’s limit: another CLI can take it on', { title: a.title }), fixes: [{ kind: 'switch', agent: a.id }] })
    }
  }
  // An agent running the same failing command again and again (5 times: past a first warning in its row).
  for (const a of (s.agents ?? []).filter((a) => (a.looping?.count ?? 0) >= 5)) {
    out.push({ kind: 'Claude', rule: 'agent-loop', severity: 'warn', text: tr('“{title}” is going in circles: {command} failed {count} times in a row', { title: a.title, command: a.looping!.command, count: a.looping!.count }) })
  }
  return out
}

/**
 * Two agents (or worktrees) on the same lines (or the same file, `[agents.overlaps] alert`): one
 * alert per pair, updated, gone with the overlap. After a merge, the one left has to rebase.
 */
function overlapAlerts(s: Snapshot): Alert[] {
  return alertedOverlaps(s).map((o) => {
    const [a, b] = [sideName(s, o, 'a'), sideName(s, o, 'b')]
    const files = o.files.filter((f) => f.level === o.level)
    const file = files.length === 1 ? files[0].path : tn(files.length, '{n} file', '{n} files')
    const text = o.merged
      ? tr('“{b}” has to rebase on “{a}” (merged): they change the same lines of {file}', { a, b, file })
      : o.level === 'lines'
        ? tr('“{a}” and “{b}” change the same lines of {file}', { a, b, file })
        : tr('“{a}” and “{b}” change the same file: {file}', { a, b, file })
    return { kind: 'Claude', rule: 'overlap', short: tr('overlap'), severity: o.level === 'lines' ? 'ko' : 'warn', text, fixes: [{ kind: 'overlap', id: overlapId(o), side: 'b' }] } satisfies Alert
  })
}

/** Claude usage has its own display, under the sessions: no duplicate alert. */
const QUIET_SOURCES = new Set(['usage'])
/** Setup warnings rather than an unreachable source: the source answers, the config points at nothing. */
const SETUP_SOURCES: Record<string, string> = { 'clickup-setup': tr('ClickUp config') }

/** The Settings section where a source's access is set: its token, or the forge's login. */
const ACCESS_SECTIONS: Record<string, { section: string; name: string }> = {
  clickup: { section: 'clickup', name: 'ClickUp' },
  'clickup queues': { section: 'clickup', name: 'ClickUp' },
  jira: { section: 'jira', name: 'Jira' },
  'jira queues': { section: 'jira', name: 'Jira' },
  gitlab: { section: 'forge', name: 'GitLab' },
  github: { section: 'forge', name: 'GitHub' },
}

/**
 * A source's access missing or refused, in words, with the Settings section that fixes it: a
 * tracker's token not in the keychain or refused (401, 403), the forge CLI not signed in.
 */
export function accessAlert(source: string, err: string): Alert | null {
  const access = ACCESS_SECTIONS[source]
  if (!access) return null
  const fix: Fix = { kind: 'settings', section: access.section, label: tr('Settings › {name}', { name: access.section === 'forge' ? tr('Forge') : access.name }) }
  if (err.includes('missing token')) return { kind: 'Source', severity: 'warn', text: tr('{name}: no token yet — paste it in Settings, then restart', { name: access.name }), fixes: [fix] }
  if (/HTTP 40[13]\b/.test(err)) return { kind: 'Source', severity: 'ko', text: tr('{name} refused the token (revoked or regenerated?): paste a new one in Settings', { name: access.name }), fixes: [fix] }
  if (access.section === 'forge' && /auth login|not logged|401 Unauthorized|HTTP 401|authentication/i.test(err)) return { kind: 'Source', severity: 'ko', text: tr('{name}: {cli} is not signed in — {err}', { name: access.name, cli: access.name === 'GitHub' ? 'gh' : 'glab', err }), fixes: [fix] }
  return null
}

function sourceAlerts(s: Snapshot): Alert[] {
  return Object.entries(s.errors)
    .filter(([source]) => !QUIET_SOURCES.has(source))
    .map(([source, err]) => accessAlert(source, err) ?? (source === 'config' ? { kind: 'Source', severity: err.startsWith('no config yet') ? 'warn' : 'ko', text: tr('Config: {err}', { err }) } : SETUP_SOURCES[source] ? { kind: 'Source', severity: 'warn', text: `${SETUP_SOURCES[source]}: ${err}` } : { kind: 'Source', severity: 'ko', text: tr('{source} unreachable: {err}', { source, err }) }))
}

/**
 * Pod really down (crash, image not found…): notified, and flagged by the badge
 * of the Infra view. A `Pending` pod is only waiting for room on the cluster.
 */
export const podDown = (p: PodView) => !p.healthy && p.status !== 'Pending'

/** A pod's key in `pod_usage`. */
export const usageKey = (p: PodView) => `${p.target}/${p.namespace}/${p.name}`

/** The alerts of the blocks with a source of their own, from their state. */
function blockAlerts(s: Snapshot): Alert[] {
  return BLOCK_DEFS.flatMap((b) => {
    const state = stateOf(b, s)
    return state !== undefined && b.alerts ? b.alerts(state as never).map((a) => ({ ...a, kind: b.label, block: b.id })) : []
  })
}

function infraAlerts(s: Snapshot): Alert[] {
  const out: Alert[] = []
  const broken = s.pods.filter((p) => !p.healthy)
  const stuck = s.pending_jobs.filter((j) => minutesSince(j.queued_since) > alertSettings().pending_job_minutes)

  // Correlation: jobs stuck with a runner down are a single incident.
  if (stuck.length > 0) {
    const cause = broken.find((p) => p.role === 'runner')
    const oldest = Math.round(Math.max(...stuck.map((j) => minutesSince(j.queued_since))))
    out.push({
      kind: 'Infra',
      severity: 'ko',
      text: tn(stuck.length, '{n} job queued for {min} min', '{n} jobs queued for {min} min', { min: oldest }) + (cause ? tr(' — runner {name} is {status}', { name: cause.name, status: cause.status }) : ''),
      // Pending jobs: to see in Alerts, nothing worth interrupting for (the runner down, for its part, notifies).
      notify: false,
    })
  }
  for (const p of broken) {
    out.push({
      kind: 'Infra',
      severity: 'ko',
      text: tr('{what} is {status}', { what: p.role === 'job' ? tr('Job {name}', { name: p.job_name ?? p.name }) : p.role === 'runner' ? tr('Runner {name}', { name: p.name }) : `Pod ${p.namespace}/${p.name} (${p.target})`, status: p.status }) + (p.restarts ? tn(p.restarts, ' ({n} restart)', ' ({n} restarts)') : ''),
      // A pod waiting for room: visible in Alerts, without notification.
      notify: podDown(p),
      fixes: p.role === 'job' ? [] : [{ kind: 'investigate', workload: p.workload ?? p.name, namespace: p.namespace, context: p.context, env: p.target, issue: `pod ${p.name} is ${p.status}${p.restarts ? ` (${p.restarts} restarts)` : ''}` }],
    })
  }
  // CI jobs with no runner left to take them, target by target.
  for (const target of new Set(s.pods.filter((p) => p.role === 'job').map((p) => p.target))) {
    if (!s.pods.some((p) => p.target === target && p.role === 'runner')) out.push({ kind: 'Infra', severity: 'ko', text: tr('No GitLab runner pod left in {target}, only its CI jobs', { target }) })
  }
  return out
}

/** Out of sync beyond this delay: it is no longer a deployment in progress. */

/**
 * ArgoCD: last sync failed, application degraded or missing, or out of sync that lasts.
 * Fixes: the pods' logs (when the Environments block knows the namespace), and the link
 * to the ArgoCD UI (`url`).
 */
export function argoAlerts(s: Snapshot): Alert[] {
  const out: Alert[] = []
  for (const instance of s.argocd?.instances ?? []) {
    for (const a of instance.apps) {
      // Instance replaced for this environment (`[[argocd]] envs`): its states no longer mean anything.
      if (instance.envs && !(a.env && instance.envs.includes(a.env))) continue
      const url = instance.url ? `${instance.url.replace(/\/$/, '')}/applications/${a.name}` : undefined
      const open: OpenTarget = { kind: 'argo', cluster: instance.cluster, app: argoKey(instance.cluster, instance.namespace, a.name) }
      const env = s.environments?.envs.find((e) => e.env === a.env && e.namespace === a.namespace)
      const fixes: Fix[] =
        env && a.deployments[0] ? [{ kind: 'podlogs', context: env.context, namespace: env.namespace, deployment: a.deployments[0], label: `${a.app ?? a.name} · ${env.env}` }] : []
      const why = a.message ? `: ${a.message}` : ''
      if (env && a.health === 'Degraded') fixes.push({ kind: 'investigate', workload: a.deployments[0] ?? a.app ?? a.name, namespace: env.namespace, context: env.context, env: env.env, issue: `ArgoCD application ${a.name} is Degraded${why}` })
      if (a.operation === 'Failed' || a.operation === 'Error') {
        out.push({ kind: 'ArgoCD', severity: 'ko', text: tr('{name}: last sync failed', { name: a.name }) + why, url, open, fixes })
      } else if (a.health === 'Degraded' || a.health === 'Missing') {
        out.push({ kind: 'ArgoCD', severity: 'ko', text: tr('{name} is {health}', { name: a.name, health: a.health }) + why, url, open, fixes })
      } else if (a.sync === 'OutOfSync' && a.operation !== 'Running' && (!a.synced_at || minutesSince(a.synced_at) > alertSettings().argo_out_of_sync_minutes)) {
        out.push({ kind: 'ArgoCD', severity: 'warn', text: a.synced_at ? tr('{name} out of sync with Git since {min} min', { name: a.name, min: Math.round(minutesSince(a.synced_at)) }) : tr('{name} out of sync with Git', { name: a.name }), url, open, fixes, notify: false })
      }
    }
  }
  return out
}

function ciAlerts(s: Snapshot): Alert[] {
  // Only protected branches' pipelines are collected (every repository with `[[repos]] pipelines`).
  return s.pipelines
    .filter((p) => p.status === 'failed')
    .map((p) => ({
      kind: 'CI',
      severity: 'ko',
      text: (failedStage(p) ? tr('{branch} failed at stage {stage}', { branch: branchName(p), stage: failedStage(p)! }) : tr('{branch} failed', { branch: branchName(p) })) + (p.failed_jobs.length ? tr(': {jobs}', { jobs: p.failed_jobs.slice(0, 3).join(', ') }) : ''),
      url: p.url,
      open: { kind: 'pipeline', pipeline: p.id, label: branchName(p), project: p.project },
      fixes: [
        { kind: 'logs', pipeline: p.id, label: branchName(p), project: p.project },
        { kind: 'retry', pipeline: p.id, label: branchName(p) },
        { kind: 'fix-branch', pipeline: p.id },
      ] satisfies Fix[],
    }))
}

/** The heart of Prism: confront the state of the MRs with that of the tickets, both ways. */
function syncAlerts(s: Snapshot): Alert[] {
  const out: Alert[] = []
  // `me` is only known after the first GitLab collection: before, the absence of an MR proves nothing.
  if (!s.me) return out
  const tickets = new Map<string, Ticket>()
  // The sprint, and the boards read whole to check their tickets (`sync`).
  for (const t of [...(s.sprint?.tickets ?? []), ...(s.pickup?.boards ?? []).filter((b) => b.sync).flatMap((b) => b.tickets)]) tickets.set(ticketRef(t.id, t.repo), t)
  if (tickets.size === 0) return out

  const byTicket = new Map<string, MergeRequest[]>()
  for (const m of s.merge_requests) {
    for (const id of mrTickets(m)) {
      const key = ticketRef(id, repoName(m.repo, s))
      byTicket.set(key, [...(byTicket.get(key) ?? []), m])
    }
  }
  const branches = new Set(s.worktrees.filter((w) => w.ticket).map((w) => ticketRef(w.ticket!, w.repo)))
  const index = ticketIndex(s)

  for (const [key, mrs] of byTicket) {
    const t = tickets.get(key)
    if (!t || blocked(t.status)) continue
    const id = t.id
    const opened = mrs.filter((m) => m.state === 'opened')
    const merged = mrs.filter((m) => m.state === 'merged')

    // MR merged, ticket not yet to validate: to flag (`to merge`, `review`…), except in progress
    // (`doing`): other MRs may follow on the same ticket.
    if (opened.length === 0 && merged.length > 0 && !t.closed && !afterMerge(t.status) && !inProgress(t.status)) {
      out.push({ kind: 'Sync', severity: 'warn', text: tr('{mr} merged — {id} still “{status}”', { mr: mrKey(merged[0]), id, status: t.status }), url: t.url, open: ticketOpen(t), fixes: moveTo(index.get(key), conventions().after_merge_statuses) })
    }
    if (opened.some((m) => !m.draft) && beforeDev(t.status)) {
      const m = opened.find((m) => !m.draft)!
      out.push({ kind: 'Sync', severity: 'warn', text: tr('{mr} open — {id} still “{status}”', { mr: mrKey(m), id, status: t.status }), url: t.url, open: ticketOpen(t), fixes: moveTo(index.get(key), [...conventions().review_statuses, ...conventions().in_progress_statuses]) })
    }
    if (opened.length > 0 && t.closed) {
      out.push({ kind: 'Sync', severity: 'warn', text: tr('{id} closed while {mr} is still open', { id, mr: mrKey(opened[0]) }), url: opened[0].url, open: mrOpen(opened[0]), fixes: [{ kind: 'mr', iid: opened[0].iid, repo: opened[0].repo }] })
    }
  }

  // A parent (EPIC) whose children are in the sprint: the work, and the branches, live with them.
  const parents = new Set((s.sprint?.tickets ?? []).map((t) => t.parent).filter(Boolean))
  for (const t of s.sprint?.tickets ?? []) {
    if (blocked(t.status)) continue
    const ref = ticketRef(t.id, t.repo)
    const mrs = byTicket.get(ref) ?? []
    if (conventions().review_statuses.includes(t.status) && !mrs.some((m) => m.state === 'opened')) {
      if (!mrs.some((m) => m.state === 'merged')) {
        out.push({ kind: 'Sync', severity: 'warn', text: tr('{id} is “{status}” with no open MR', { id: t.id, status: t.status }), url: t.url, open: ticketOpen(t) })
      }
    }
    const idleDays = alertSettings().doing_idle_days
    if (inProgress(t.status) && mrs.length === 0 && !branches.has(ref) && !parents.has(t.task_id) && minutesSince(t.updated_at) > idleDays * 1440) {
      out.push({ kind: 'Sync', severity: 'info', text: tr('{id} “{status}” for over {days} days with no branch or MR', { id: t.id, status: t.status, days: idleDays }), url: t.url, open: ticketOpen(t) })
    }
  }
  return out
}

/**
 * One of my MRs is a draft: I am working on it, its ticket should be "doing". Tickets
 * are looked up wherever Prism reads them (sprint, Architecture, in progress outside sprint, boards to
 * pick): a bug or hotfix MR counts too.
 */
export function draftAlerts(s: Snapshot): Alert[] {
  if (!s.me) return []
  const index = ticketIndex(s)
  return openMrs(s)
    .filter((m) => m.draft && m.author === meOn(s, m))
    .flatMap((m) => mrTickets(m).map((id) => [m, id] as const))
    .flatMap(([m, id]) => {
      const entry = index.get(ticketRef(id, repoName(m.repo, s)))
      const t = entry?.t
      if (!t || t.closed || inProgress(t.status)) return []
      const wanted = conventions().in_progress_statuses
      return [{ kind: 'Sync', severity: 'warn', text: tr('{mr} is a draft — {id} is “{status}”, not “{wanted}”', { mr: mrKey(m), id: t.id, status: t.status, wanted: wanted[0] ?? tr('in progress') }), url: t.url, open: ticketOpen(t), fixes: moveTo(entry, wanted) } satisfies Alert]
    })
}

/**
 * A ticket still in progress ("doing") whose open MRs are all out of draft: the work is in
 * review, the ticket should say so (`[clickup] review_statuses`, the first one), or directly
 * "to merge" when they are all approved. A draft MR on the same ticket keeps it in progress,
 * rightly. Tickets looked up wherever Prism reads them.
 */
export function inReviewAlerts(s: Snapshot): Alert[] {
  if (!s.me) return []
  const review = conventions().review_statuses
  if (!review.length) return []
  const index = ticketIndex(s)
  const byTicket = new Map<string, MergeRequest[]>()
  for (const m of openMrs(s)) {
    for (const id of mrTickets(m)) {
      const key = ticketRef(id, repoName(m.repo, s))
      byTicket.set(key, [...(byTicket.get(key) ?? []), m])
    }
  }
  return [...byTicket].flatMap(([key, mrs]) => {
    const entry = index.get(key)
    const t = entry?.t
    const id = t?.id
    if (!t || t.closed || blocked(t.status) || !inProgress(t.status) || mrs.some((m) => m.draft)) return []
    const approved = conventions().approved_statuses ?? []
    const wanted = approved.length && mrs.every(isApproved) ? approved : review
    const m = mrs[0]
    return [{ kind: 'Sync', severity: 'warn', text: tr('{mr} in review — {id} still “{status}”, not “{wanted}”', { mr: mrKey(m), id: id!, status: t.status, wanted: wanted[0] }), url: t.url, open: ticketOpen(t), fixes: moveTo(entry, wanted) } satisfies Alert]
  })
}

/**
 * A ticket "to merge" (`[tracker] merge_queue_statuses`) whose open MR, in a repository with a
 * merge queue, is not in it: nothing will merge it. Already merging (auto-merge) or a draft, it is
 * left alone; in conflict, after a failed rebase or still short of a required approval, it is flagged without the fix.
 */
export function queueAlerts(s: Snapshot): Alert[] {
  const statuses = conventions().merge_queue_statuses ?? []
  const label = s.merge_queue_label
  if (!statuses.length || !label) return []
  const index = ticketIndex(s)
  return openMrs(s).flatMap((m) => {
    if (m.draft || m.auto_merge || m.labels?.includes(label) || !hasMergeQueue(m, s)) return []
    // Of the tickets it names, the first one waiting to be merged.
    const t = mrTickets(m)
      .map((id) => index.get(ticketRef(id, repoName(m.repo, s)))?.t)
      .find((t) => t && !t.closed && !blocked(t.status) && statuses.includes(t.status))
    if (!t) return []
    const fixes: Fix[] = [...(canQueue(m) ? [{ kind: 'queue' as const, iid: m.iid, repo: m.repo }] : []), { kind: 'mr', iid: m.iid, repo: m.repo }]
    const first = m.conflicts ? tr(' (conflicts first)') : rebaseFailed(m) ? tr(' (failed rebase first)') : m.approvals_left > 0 ? tr(' (approval first)') : ''
    return [{ kind: 'Sync', severity: 'warn', text: tr('{id} “{status}” — {mr} not in the merge queue', { id: t.id, status: t.status, mr: mrKey(m) }) + first, url: m.url, open: mrOpen(m), fixes } satisfies Alert]
  })
}

/** An MR that has at least one approval, and no request for changes: one that may go "to merge". */
export const isApproved = (m: MergeRequest) => m.approved_by.length > 0 && !changesRequested(m)

/**
 * A ticket's open MRs are all approved (and none is a draft), but it is still
 * in review: it should be "to merge" (`[clickup] approved_statuses`). An MR not yet
 * approved on the same ticket, or with changes requested, keeps it in review, rightly. Tickets looked up wherever Prism reads them.
 */
export function approvedAlerts(s: Snapshot): Alert[] {
  if (!s.me) return []
  const wanted = conventions().approved_statuses ?? []
  if (!wanted.length) return []
  const index = ticketIndex(s)
  const byTicket = new Map<string, MergeRequest[]>()
  for (const m of openMrs(s)) {
    for (const id of mrTickets(m)) {
      const key = ticketRef(id, repoName(m.repo, s))
      byTicket.set(key, [...(byTicket.get(key) ?? []), m])
    }
  }
  return [...byTicket].flatMap(([key, mrs]) => {
    const entry = index.get(key)
    const t = entry?.t
    const id = t?.id
    if (!t || t.closed || blocked(t.status) || !awaitingApproval(t.status) || !mrs.every((m) => !m.draft && isApproved(m))) return []
    const m = mrs[0]
    return [{ kind: 'Sync', severity: 'warn', text: tr('{mr} approved — {id} still “{status}”, not “{wanted}”', { mr: mrKey(m), id: id!, status: t.status, wanted: wanted[0] }), url: t.url, open: ticketOpen(t), fixes: moveTo(entry, wanted) } satisfies Alert]
  })
}

function reviewAlerts(s: Snapshot): Alert[] {
  return openMrs(s)
    .filter((m) => awaitsMyReview(m, meOn(s, m)) && minutesSince(m.updated_at) > alertSettings().review_wait_hours * 60)
    .map((m) => ({
      kind: 'Review',
      severity: 'info',
      since: m.updated_at,
      short: tr('your review'),
      text: tr('{mr} by {author} awaits your review', { mr: mrKey(m), author: m.author_name || m.author }),
      url: m.url, open: mrOpen(m),
      fixes: [{ kind: 'mr', iid: m.iid, repo: m.repo }, ...(hasWorktrees(m.repo, s) ? [{ kind: 'claude' as const, iid: m.iid, repo: m.repo, action: 'review' as const }] : [])] satisfies Fix[],
    }))
}

/**
 * Out of draft with nobody to review it: someone was forgotten, `[alerts] no_reviewer_minutes`
 * after it opened (the time to add them; 0: never). Reviewed already counts, even by someone not asked.
 */
export function lacksReviewer(m: MergeRequest): boolean {
  const grace = alertSettings().no_reviewer_minutes
  return !!grace && !m.draft && !m.reviewers.length && !m.approved_by.length && !m.changes_requested_by.length && !!m.created_at && minutesSince(m.created_at) > grace
}

/** Review threads left unresolved on my MR: feedback to apply or answer (`[alerts] open_threads`). */
export const hasOpenThreads = (m: MergeRequest) => alertSettings().open_threads && m.state === 'opened' && m.open_threads > 0

function myMrAlerts(s: Snapshot): Alert[] {
  const out: Alert[] = []
  for (const m of openMrs(s).filter((m) => m.author === meOn(s, m))) {
    if (m.pipeline === 'failed') {
      const p = m.head_pipeline
      const fixes: Fix[] = p ? [{ kind: 'logs', pipeline: p.id, label: mrKey(m), project: m.project }, { kind: 'retry', pipeline: p.id, label: mrKey(m) }] : []
      out.push({ kind: 'MR', rule: 'mr-ci', short: tr('CI failed'), severity: 'warn', text: tr('{mr}: CI failed', { mr: mrKey(m) }), url: m.url, open: mrOpen(m), fixes: [...fixes, ...(hasWorktrees(m.repo, s) ? [{ kind: 'claude' as const, iid: m.iid, repo: m.repo, action: 'fix-pipeline' as const }] : [])] })
    }
    // In conflict: an agent rebases it and resolves the conflicts, where the branch has a worktree to go to.
    if (m.conflicts) out.push({ kind: 'MR', rule: 'conflicts', short: tr('conflicts'), severity: 'warn', text: tr('{mr}: conflicts with {target}', { mr: mrKey(m), target: m.target_branch }), url: m.url, open: mrOpen(m), fixes: [...(hasWorktrees(m.repo, s) ? [{ kind: 'claude' as const, iid: m.iid, repo: m.repo, action: 'rebase' as const }] : []), { kind: 'mr', iid: m.iid, repo: m.repo }] })
    // The forge could not rebase it: locally, by an agent, where the conflicting commit shows.
    else if (rebaseFailed(m))
      out.push({ kind: 'MR', rule: 'rebase-failed', short: tr('rebase failed'), severity: 'warn', text: tr('{mr}: {forge} could not rebase it onto {target} ({reason})', { mr: mrKey(m), forge: forgeName(), target: m.target_branch, reason: rebaseFailure(m) }), url: m.url, open: mrOpen(m), fixes: [...(hasWorktrees(m.repo, s) ? [{ kind: 'claude' as const, iid: m.iid, repo: m.repo, action: 'rebase' as const }] : []), { kind: 'mr', iid: m.iid, repo: m.repo }] })
    // Only when the rebase is the last blocker: in review, a branch behind is normal.
    else if (m.merge_status === 'need_rebase' && !m.rebase_in_progress) out.push({ kind: 'MR', rule: 'need-rebase', short: tr('to rebase'), severity: 'info', text: tr('{mr} is ready, rebase it onto {target}', { mr: mrKey(m), target: m.target_branch }), url: m.url, open: mrOpen(m), fixes: [{ kind: 'rebase', iid: m.iid, repo: m.repo }] })
    // Only its CI left: auto-merge, rather than coming back once it is green. In the merge queue
    // too: the queue's job may not have come to it yet.
    if (onlyWaitsForCi(m)) out.push({ kind: 'MR', rule: 'only-ci', short: tr('only waits for its CI'), severity: 'info', text: tr('{mr} only waits for its CI: set auto-merge', { mr: mrKey(m) }), url: m.url, open: mrOpen(m), fixes: [{ kind: 'automerge', iid: m.iid, repo: m.repo }] })
    if (lacksReviewer(m))
      out.push({ kind: 'MR', rule: 'no-reviewer', short: tr('no reviewer'), since: m.created_at ?? undefined, severity: 'warn', text: tr('{mr} has no reviewer', { mr: mrKey(m) }), url: m.url, open: mrOpen(m), fixes: [{ kind: 'mr', iid: m.iid, repo: m.repo }] })
    if (hasOpenThreads(m))
      out.push({ kind: 'MR', rule: 'open-threads', short: tn(m.open_threads, '{n} thread', '{n} threads'), severity: 'warn', text: tn(m.open_threads, '{mr}: {n} unresolved thread', '{mr}: {n} unresolved threads', { mr: mrKey(m) }), url: m.url, open: mrOpen(m), fixes: [{ kind: 'mr', iid: m.iid, repo: m.repo }, ...(hasWorktrees(m.repo, s) ? [{ kind: 'claude' as const, iid: m.iid, repo: m.repo, action: 'review-fix' as const }] : [])] })
    // Not every repository (infra, helm…) has tickets: only those with `[[repos]] ticket_required`.
    if (!m.ticket && needsTicket(m, s) && hasTickets()) out.push({ kind: 'MR', rule: 'no-ticket', short: tr('no ticket id'), severity: 'info', text: tr('{mr} has no {prefix}- id in its title or branch', { mr: mrKey(m), prefix: ticketPrefix() }), url: m.url, open: mrOpen(m) })
  }
  return out
}

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
      w, name: w.path.split('/').pop() ?? w.path, origin, ticket, mr, sessions, agents, archived, activity, state, reasons, busy, keep,
      removable: !w.is_main && !w.prunable && !w.locked && !busy && !keep,
      forceable: !w.is_main && !w.prunable && !w.locked && !busy && !!keep,
    }
  })
}

function worktreeAlerts(s: Snapshot): Alert[] {
  // Archived ones are left alone; a folder gone still says so.
  const infos = describeWorktrees(s).filter((i) => i.state === 'gone' || !isHidden(i))
  const safe = infos.filter((i) => i.state === 'stale' && i.removable).length
  const gone = infos.filter((i) => i.state === 'gone').length
  const out: Alert[] = []
  if (safe) out.push({ kind: 'Worktrees', severity: 'info', notify: false, text: tn(safe, '{n} stale worktree can be removed safely', '{n} stale worktrees can be removed safely') })
  if (gone) out.push({ kind: 'Worktrees', severity: 'info', notify: false, text: tn(gone, '{n} worktree whose folder is gone (prune)', '{n} worktrees whose folder is gone (prune)') })
  return out
}
