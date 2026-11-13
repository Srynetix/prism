import type { MenuChoice, MenuHead, MenuItem, MenuSection, QuickAction } from './components/ActionMenu.svelte'
import type { Agent, MergeRequest, Person, StatusOption, Ticket } from './types'
import { actorName, cliOf, nameOf } from './agent-cli'
import { closeAgent, interruptAgent, openAgentFolder, openExternal, removeWorktree, restartAgent } from './agents.svelte'
import { hasCore } from './bridge'
import { claudeItem, mrActions, openAgentAction, ticketActions, type ClaudeAction } from './claude-actions'
import { setQuery } from './filter.svelte'
import { namesTicket } from './conventions'
import { forgeName, mrNumber } from './forge'
import { live } from './live.svelte'
import { canShowLogs, openPipeline } from './logs.svelte'
import { timeDescription } from './mr-actions'
import { pickPeople, setMrPeople } from './mr-people.svelte'
import { pickLabels } from './mr-labels.svelte'
import { mrKey, sameMr, type MrRef } from './mr-ref'
import { openMr } from './mr-view.svelte'
import { FILE_MANAGER, keys } from './platform'
import { canCancel, canRetry, canRunPipeline, cancelPipeline, mergeMr, rebaseMr, retryPipeline, runMrPipeline, setAutoMerge, setDraft, setMergePriority, setMergeQueue } from './pipeline-actions'
import { hasMergeQueue } from './repositories'
import { mergePriority, priorityLevels } from './merge-queue'
import { agentStateLabel, canMerge, canQueue, mergeState, mrForSession, rebaseHint, isWorking, statusColor, waitsForYou } from './rules'
import { openThread } from './thread.svelte'
import { mrsOf } from './ticket-step'
import { addToSprint, canPreview, changeStatus, openTicket, setMine, setPriority, type Priority } from './ticket-view.svelte'
import { openTimeDraft } from './timeDraft.svelte'
import { copy, toast } from './toast.svelte'
import { isArchived, setArchived } from './agent-archive.svelte'
import { canMarkRead, markRead } from './session-read'
import { openSwitch } from './agent-switch.svelte'
import { openQuickActionDraft } from './quick-action-draft.svelte'
import { DRAFTER } from './quick-action-source'
import { offers, trackerName } from './tracker'
import { t as tr } from './i18n.svelte'

/**
 * The ⋯ menus of an MR and of a ticket, built once for their menu (MrMenu, TicketMenu) and for
 * the palette's actions (⌘↵): the same actions wherever they are offered. An agent's, below.
 */
export interface Menu {
  head: MenuHead
  quick: QuickAction[]
  sections: MenuSection[]
}

/**
 * An MR's ⋯ menu: the MR and its state at the top; copy the link or the branch, open in
 * GitLab, filter the dashboard in one click; then its actions by theme. Those that cannot
 * be undone (merge, auto-merge, stopping the pipeline) need a second click.
 */
export function mrMenu(mr: MergeRequest, queueLabel: string | null | undefined, me: string | null = null): Menu {


  const queued = !!queueLabel && !!mr.labels?.includes(queueLabel)
  const p = mr.head_pipeline

  const head = ((): MenuHead => {
    const state = mergeState(mr)
    const pills = [{ label: state.label, tone: state.tone }]
    if (mr.pipeline && mr.state === 'opened') pills.push({ label: tr('pipeline {status}', { status: mr.pipeline }), tone: mr.pipeline === 'success' ? 'ok' : mr.pipeline === 'failed' ? 'ko' : 'info' })
    return { id: mrNumber(mr.iid, mr.forge), title: mr.title, color: 'var(--src-gitlab)', pills }
  })()

  const quick: QuickAction[] = [
    { key: 'copy-url', label: tr('Copy link'), icon: 'copy', kbd: 'C', done: tr('Copied'), run: () => copy(mr.url, tr('Link to {name} copied', { name: mrNumber(mr.iid, mr.forge) })) },
    { key: 'copy', label: tr('Branch'), icon: 'branch', kbd: 'B', done: tr('Copied'), run: () => copy(mr.source_branch, tr('Branch {branch} copied', { branch: mr.source_branch })) },
    { key: 'gitlab', label: forgeName(mr.forge), icon: 'external', kbd: 'O', href: mr.url, external: true },
    { key: 'filter', label: tr('Filter'), icon: 'search', kbd: 'F', run: () => setQuery(mrNumber(mr.iid)) },
    { key: 'thread', label: tr('Thread'), icon: 'worktree', kbd: 'H', run: () => openThread({ mr: { repo: mr.repo ?? null, iid: mr.iid } }) },
  ]

  const sections = ((): MenuSection[] => {
    const merge: MenuItem[] = []
    if (mr.state === 'opened') {
      if (canMerge(mr)) merge.push({ key: 'merge', icon: 'merge', label: tr('Merge now'), hint: tr('into {branch}', { branch: mr.target_branch }), confirm: tr('Click again to merge into {branch}', { branch: mr.target_branch }), run: () => mergeMr(mr) })
      if (queueLabel && hasMergeQueue(mr) && (queued || canQueue(mr))) {
        merge.push(queued
          ? { key: 'queue', icon: 'queue', label: tr('Remove from merge queue'), hint: queueLabel, run: () => setMergeQueue(mr, false) }
          : { key: 'queue', icon: 'queue', label: tr('Add to merge queue'), hint: tr('rebase + auto-merge'), run: () => setMergeQueue(mr, true) })
      }
      if (mr.auto_merge) merge.push({ key: 'am', icon: 'automerge', label: tr('Cancel auto-merge'), run: () => setAutoMerge(mr, false) })
      else if (!mr.draft && !mr.conflicts) merge.push({ key: 'am', icon: 'automerge', label: tr('Set auto-merge'), hint: tr('merges when green'), run: () => setAutoMerge(mr, true) })
      if (rebaseHint(mr)) merge.push({ key: 'rebase', icon: 'rebase', label: tr('Rebase onto {branch}', { branch: mr.target_branch }), run: () => rebaseMr(mr) })
      merge.push(mr.draft
        ? { key: 'draft', icon: 'edit', label: tr('Mark as ready'), hint: tr('removes Draft:'), kbd: 'D', run: () => setDraft(mr, false) }
        : { key: 'draft', icon: 'edit', label: tr('Mark as draft'), hint: tr('adds Draft:'), kbd: 'D', run: () => setDraft(mr, true) })
    }
    // The merge queue's order: P0 first, then P1…, then none (`[gitlab] merge_priority_prefix`).
    const s = live.snapshot
    const prefix = s?.merge_priority_prefix
    const priority: MenuChoice[] = []
    if (s && prefix && queueLabel && mr.state === 'opened' && hasCore() && hasMergeQueue(mr)) {
      const current = mergePriority(mr, prefix)
      for (const n of [...priorityLevels(s), null]) {
        priority.push({ key: `mprio-${n ?? 'none'}`, label: n === null ? '—' : `P${n}`, current: n === current, run: () => setMergePriority(mr, n) })
      }
    }
    const pipeline: MenuItem[] = []
    if (p) {
      if (canShowLogs()) pipeline.push({ key: 'logs', icon: 'logs', label: tr('Open logs'), kbd: 'L', run: () => openPipeline(p.id, mrKey(mr), mr.project ?? null) })
      if (canRetry(mr.pipeline)) pipeline.push({ key: 'retry', icon: 'retry', label: tr('Retry failed jobs'), run: () => retryPipeline(p.id) })
      if (canCancel(mr.pipeline)) pipeline.push({ key: 'stop', icon: 'stop', label: tr('Stop pipeline'), confirm: tr('Click again to stop the pipeline'), run: () => cancelPipeline(p.id) })
      pipeline.push({ key: 'pipe-url', icon: 'external', label: tr('Pipeline in {forge}', { forge: forgeName(mr.forge) }), href: p.url, external: true })
    }
    if (canRunPipeline(mr)) pipeline.push({ key: 'run', icon: 'play', label: tr('Run pipeline'), hint: tr('with its labels now'), run: () => runMrPipeline(mr) })
    const claude: MenuItem[] = mrActions(mr).filter((a) => a.launch || a.href).map(claudeItem)
    const other: MenuItem[] = []
    // Assignees, reviewers and labels: the MR drawer opens on the picker.
    if (mr.state === 'opened' && hasCore()) {
      const people = (kind: 'assignees' | 'reviewers') => () => { openMr(mr); pickPeople(mr, kind) }
      if (me && !(mr.assignees ?? []).includes(me)) other.push({ key: 'assign-me', icon: 'user', label: tr('Assign me'), hint: tr('keeps the others'), run: () => setMrPeople(mr, 'assignees', [...(mr.assignees ?? []), me]) })
      other.push({ key: 'assignees', icon: 'user', label: tr('Assignees…'), hint: (mr.assignees ?? []).join(', ') || tr('none'), run: people('assignees') })
      other.push({ key: 'reviewers', icon: 'review', label: tr('Reviewers…'), hint: mr.reviewers.join(', ') || tr('none'), run: people('reviewers') })
      other.push({ key: 'labels', icon: 'label', label: tr('Labels…'), hint: (mr.labels ?? []).join(', ') || tr('none'), run: () => { openMr(mr); pickLabels(mr) } })
    }
    if (mr.ticket && offers('time')) other.push({ key: 'time', icon: 'timer', label: tr('Log time'), hint: mr.ticket, kbd: 'T', run: () => openTimeDraft(mr.ticket!, timeDescription(mr)) })
    return [
      { title: tr('Merge'), items: merge },
      { title: tr('Merge priority'), segment: priority },
      { title: tr('Pipeline'), items: pipeline },
      { title: actorName(), items: claude },
      { title: tr('Other'), items: other },
    ]
  })()
  return { head, quick, sections }
}

/**
 * The current sprint's short name ("Sprint 42") when the ticket can be added to it: the tracker
 * offers it, a sprint is read, and the ticket isn't in it yet. Null otherwise.
 */
export function sprintToAdd(taskId: string): string | null {
  const sprint = live.snapshot?.sprint
  if (!hasCore() || !offers('add_to_sprint') || !sprint?.list_id || sprint.tickets.some((x) => x.task_id === taskId)) return null
  return sprint.name.replace(/^.*?(Sprint \d+).*$/, '$1') || 'sprint'
}

/**
 * A ticket's ⋯ menu: the ticket, its status and its priority at the top; copy the link or the ID,
 * open in ClickUp, filter the dashboard in one click; the status as badges and the priority
 * as a picker; then assign me, log time, its MRs and the Claude actions (`claude`
 * replaces the default ones, Refine and Implement). `toSprint`: offer to add the ticket
 * to the current sprint, in addition to its list ("To pick"), after a second click.
 */
export function ticketMenu(t: Ticket, me: Person | null, statuses: StatusOption[], claude?: ClaudeAction[], toSprint = false): Menu {


  const sprintName = toSprint ? sprintToAdd(t.task_id) : null
  const mine = !!me && t.assignees.some((a) => a.id === me.id)

  const LEVELS: [Priority | null, string, string][] = [
    ['urgent', tr('Urgent'), 'var(--ko)'],
    ['high', tr('High'), 'var(--warn)'],
    ['normal', tr('Normal'), 'var(--info)'],
    ['low', tr('Low'), 'var(--text-muted)'],
    [null, '—', ''],
  ]

  const head = ((): MenuHead => {
    const pills = [{ label: t.status, tone: t.status === 'done' ? 'ok' : 'info' }]
    if (t.priority) pills.push({ label: LEVELS.find(([p]) => p === t.priority)![1], tone: t.priority === 'urgent' ? 'ko' : t.priority === 'high' ? 'warn' : '' })
    return { id: t.id, title: t.name, color: 'var(--src-clickup)', pills }
  })()

  const quick: QuickAction[] = [
    { key: 'copy-url', label: tr('Copy link'), icon: 'copy', kbd: 'C', done: tr('Copied'), run: () => copy(t.url, tr('Link to {name} copied', { name: t.id })) },
    { key: 'copy', label: tr('Copy ID'), icon: 'hash', kbd: 'I', done: tr('Copied'), run: () => copy(t.id, tr('{name} copied', { name: t.id })) },
    { key: 'clickup', label: trackerName(), icon: 'external', kbd: 'O', href: t.url, external: true },
    { key: 'filter', label: tr('Filter'), icon: 'search', kbd: 'F', run: () => setQuery(t.id) },
    { key: 'thread', label: tr('Thread'), icon: 'worktree', kbd: 'H', run: () => openThread({ ticket: t.id, repo: t.repo }) },
  ]

  const sections = ((): MenuSection[] => {
    const app = hasCore()
    const status: MenuChoice[] = app && offers('status')
      ? statuses.map((s) => ({ key: `st-${s.status}`, label: s.status, color: statusColor(s.status, statuses), current: s.status === t.status, run: () => changeStatus(t.task_id, s.status, statuses) }))
      : []
    const priority: MenuChoice[] = app && offers('priority')
      ? LEVELS.map(([p, name, color]) => ({ key: `prio-${p ?? 'none'}`, label: name, color: color || undefined, current: p === t.priority, run: () => setPriority(t.task_id, p) }))
      : []
    const ticket: MenuItem[] = []
    if (app && me && offers('assignees')) ticket.push(mine ? { key: 'mine', icon: 'user', label: tr('Unassign me'), kbd: 'A', run: () => setMine(t.task_id, false) } : { key: 'mine', icon: 'user', label: tr('Assign me'), kbd: 'A', run: () => setMine(t.task_id, true) })
    if (sprintName) {
      ticket.push({
        key: 'sprint',
        icon: 'add',
        label: tr('Add to {sprint}', { sprint: sprintName }),
        hint: tr('keeps its list'),
        confirm: tr('Click again: {id} is added to {sprint} and stays in its list', { id: t.id, sprint: sprintName }),
        run: () => addToSprint(t.task_id),
      })
    }
    if (offers('time')) ticket.push({ key: 'time', icon: 'timer', label: tr('Log time'), kbd: 'T', run: () => openTimeDraft(t.id, '') })
    for (const m of mrsOf(t.id, t.repo).slice(0, 4)) ticket.push({ key: `mr-${mrKey(m)}`, icon: 'mr', label: tr('Open {name}', { name: mrNumber(m.iid, m.forge) }), hint: m.state === 'merged' ? tr('merged') : m.source_branch, run: () => openMr(m) })
    // The ticket's actions (or the block's), then an agent with no defined work, whatever the status.
    const ai: MenuItem[] = [...(claude ?? ticketActions(t)), openAgentAction(t)].filter((a) => a.launch || a.href).map(claudeItem)
    return [
      { title: tr('Status'), chips: status },
      { title: tr('Priority'), segment: priority },
      { title: tr('Ticket'), items: ticket },
      { title: actorName(), items: ai },
    ]
  })()
  return { head, quick, sections }
}

/** A menu's action as a flat list's line (the palette's ⌘↵, the phone's Actions sheet). */
export interface FlatAction {
  key: string
  label: string
  hint?: string
  group?: string
  /** A second tap / ↵ is needed, with this message. */
  confirm?: string
  run?: () => unknown
  href?: string
}

/** A menu as a list: its quick bar (said in full: “Branch” copies it), then its items, statuses and priorities. */
export function flatten(menu: Menu): FlatAction[] {
  const label = (q: QuickAction) => (q.href ? tr('Open in {name}', { name: q.label }) : q.done === tr('Copied') && !q.label.toLowerCase().startsWith(tr('Copy').toLowerCase()) ? tr('Copy {what}', { what: q.label.toLowerCase() }) : q.label)
  const out: FlatAction[] = menu.quick.map((q) => ({ key: q.key, label: label(q), run: q.run, href: q.href }))
  for (const s of menu.sections) {
    for (const i of s.items ?? []) out.push({ key: i.key, label: i.label, hint: i.hint, group: s.title, confirm: i.confirm, run: i.run, href: i.href })
    for (const c of [...(s.chips ?? []), ...(s.segment ?? [])]) if (!c.current) out.push({ key: c.key, label: tr('{section}: {choice}', { section: s.title, choice: c.label }), group: s.title, run: c.run })
  }
  return out
}

/** An agent's ⋯ menu head: its ticket, title and state. */
export const agentHead = (a: Agent): MenuHead => ({ id: a.ticket ?? tr('Agent'), title: a.title, color: 'var(--src-claude)', pills: [{ label: agentStateLabel(a), tone: waitsForYou(a.status) ? 'warn' : isWorking(a.status) ? 'info' : undefined }] })

/**
 * The open MR an agent works on: its branch's (in its worktree's repository), failing that the
 * only open MR of its ticket. None once merged or closed: the header shows only what is to do.
 */
export function openMrOf(a: Agent): MergeRequest | MrRef | undefined {
  const s = live.snapshot
  if (!s) return undefined
  // Linked by hand: whatever its state, even beyond the MRs Prism reads.
  if (a.mr) return s.merge_requests.find((m) => sameMr(m, a.mr!)) ?? a.mr
  const own = mrForSession(s, a)
  if (own) return own.state === 'opened' ? own : undefined
  const ofTicket = a.ticket ? s.merge_requests.filter((m) => m.state === 'opened' && namesTicket(m, a.ticket!)) : []
  return ofTicket.length === 1 ? ofTicket[0] : undefined
}

/**
 * The ⋯ menu of an agent: what is occasional or risky. Opening its folder elsewhere, attaching an
 * external terminal; renaming, restarting (two clicks while Claude works), interrupting, removing a
 * stopped agent's worktree; closing the session (two clicks while Claude runs). No `confirm()`:
 * Tauri's webview does not show it and always answers no.
 */
export function agentMenu(a: Agent, rename: () => void): MenuSection[] {
  const apps = live.snapshot?.open_with
  const terminal = apps?.terminal ?? 'Terminal'
  const open: MenuItem[] = [
    { key: 'finder', icon: 'folder', label: FILE_MANAGER === 'Finder' ? tr('In the Finder') : tr('In {app}', { app: FILE_MANAGER }), run: () => openAgentFolder(a.id, 'finder') },
    { key: 'shell', icon: 'terminal', label: tr('A shell'), hint: terminal, run: () => openAgentFolder(a.id, 'terminal') },
    ...(apps?.editor ? [{ key: 'editor', icon: 'code', label: tr('In the editor'), hint: apps.editor, run: () => openAgentFolder(a.id, 'editor') } satisfies MenuItem] : []),
    ...(a.state === 'running' ? [{ key: 'attach', icon: 'external', label: tr('Attach in {terminal} too', { terminal }), run: () => openExternal(a.id) } satisfies MenuItem] : []),
  ]
  const working = a.state === 'running' && a.status === 'running'
  const agent: MenuItem[] = [
    { key: 'rename', icon: 'edit', label: tr('Rename'), hint: tr('double-click the title'), run: rename },
    ...(a.state === 'running'
      ? [
          { key: 'restart', icon: 'retry', label: tr('Restart'), hint: cliOf(a.provider).resume, confirm: working ? tr('{name} is working: click again to restart', { name: nameOf(a) }) : undefined, run: () => restartAgent(a.id) } satisfies MenuItem,
          ...(working ? [{ key: 'interrupt', icon: 'pause', label: tr('Interrupt'), hint: keys('⌘.'), run: () => interruptAgent(a.id) } satisfies MenuItem] : []),
        ]
      : []),
    ...(a.state !== 'prepared' ? [{ key: 'switch', icon: 'retry', label: tr('Continue with…'), hint: tr('another CLI, from a relay'), run: () => openSwitch(a) } satisfies MenuItem] : []),
    ...(a.worktree && a.state !== 'running' ? [{ key: 'rm-wt', icon: 'close', label: tr('Remove worktree'), hint: tr('if nothing would be lost'), run: () => removeWorktree(a.id) } satisfies MenuItem] : []),
  ]
  const hidden = isArchived(a)
  // A session that went well, kept as a quick action (not one still prepared, nor a draft's own agent).
  const reusable = a.state !== 'prepared' && a.launch?.from !== DRAFTER
  const session: MenuItem[] = [
    ...(a.state === 'running' && canMarkRead(a) ? [{ key: 'read', icon: 'check', label: tr('Mark as read'), hint: tr('it waits for you no more, until its next turn'), run: () => markRead(a.id) } satisfies MenuItem] : []),
    ...(reusable ? [{ key: 'quick-action', icon: 'quick', label: tr('Create a quick action…'), hint: tr('from this session'), run: () => openQuickActionDraft({ agent: a.id }) } satisfies MenuItem] : []),
    hidden
      ? { key: 'unarchive', icon: 'show', label: tr('Unarchive'), hint: tr('back in the list'), run: () => setArchived(a, false) }
      : { key: 'archive', icon: 'hide', label: tr('Archive'), hint: tr('out of the list until it waits for you'), run: () => setArchived(a, true) },
    { key: 'close', icon: 'close', label: tr('Close'), hint: a.worktree ? tr('stops it, removes its worktree if nothing would be lost') : tr('stops it; the conversation stays'), confirm: a.state === 'running' ? tr('{name} is still running: click again to stop and close', { name: nameOf(a) }) : undefined, run: async () => { const kept = await closeAgent(a.id); if (kept) toast(kept, 5000) } },
  ]
  const mr = openMrOf(a)
  const show: MenuItem[] = [
    ...(a.ticket && canPreview() ? [{ key: 'ticket', icon: 'kanban', label: tr('Ticket {id}', { id: a.ticket }), hint: tr('its preview'), run: () => openTicket(a.ticket!) } satisfies MenuItem] : []),
    ...(mr ? [{ key: 'mr', icon: 'mr', label: `${mrKey(mr)}`, hint: tr('its panel'), run: () => openMr(mr) } satisfies MenuItem] : []),
  ]
  return [...(show.length ? [{ title: tr('Show'), items: show }] : []), { title: tr('Open its folder'), items: open }, { title: tr('Agent'), items: agent }, { title: tr('Session'), items: session }]
}
