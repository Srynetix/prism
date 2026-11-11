/**
 * « In passing » (`Snapshot.asides`, `[agents.aside]`): the tasks an agent flagged out of its own
 * with `suggest_task` (or a Desktop session with `spawn_task`), one line per subject. Here: which
 * ones an agent saw, the list's filters, the launch prefilled from one, and the user's answers.
 */
import type { Agent, Aside, Snapshot } from './types'
import type { ConversationTool } from './agents.svelte'
import { agentsSettings, openLauncher, type Launch } from './agents.svelte'
import type { LaunchSpec } from './bindings/LaunchSpec'
import { invoke } from './bridge'
import { live } from './live.svelte'
import { t } from './i18n.svelte'

export const asidesOf = (s: Snapshot | null | undefined): Aside[] => s?.asides ?? []

/** Still to deal with: open, or kept for later. */
export const isOpen = (a: Aside) => a.state === 'open' || a.state === 'later'

/** Counted on a badge: open, not kept for later. */
export const badged = (a: Aside) => a.state === 'open'

/** The ones agent `id` saw, open and not kept for later: its chip. */
export const openOfAgent = (s: Snapshot | null | undefined, id: string) => asidesOf(s).filter((a) => badged(a) && a.seen.some((x) => x.agent === id))

/** An open line older than `stale_days`: folded under « Older ». */
export function isStale(a: Aside, now = Date.now()) {
  const days = agentsSettings().aside.stale_days ?? 14
  return isOpen(a) && days > 0 && now - Date.parse(a.at) > days * 86_400_000
}

export type AsideFilter = 'open' | 'launched' | 'ticket' | 'dismissed'
export const FILTERS: [AsideFilter, string][] = [
  ['open', t('Open')],
  ['launched', t('Launched')],
  ['ticket', t('Tickets')],
  ['dismissed', t('Dismissed')],
]
export function inFilter(a: Aside, f: AsideFilter) {
  if (f === 'open') return isOpen(a)
  if (f === 'launched') return a.state === 'launched' || a.state === 'then' || a.state === 'session' || a.state === 'done'
  if (f === 'ticket') return a.state === 'ticket'
  return a.state === 'dismissed'
}

/** The list in the Agents view: shown or not, on one agent's only, which filter. */
export const asideView = $state({ open: false, agent: '', filter: 'open' as AsideFilter })
export function showAsides(agent = '') {
  Object.assign(asideView, { open: true, agent, filter: 'open' })
}

/** By repository (those without one last), the newest first within each. */
export function byRepo(list: Aside[]): [string, Aside[]][] {
  const groups = new Map<string, Aside[]>()
  for (const a of list) groups.set(a.repo ?? '', [...(groups.get(a.repo ?? '') ?? []), a])
  return [...groups].sort(([x], [y]) => (x === '' ? 1 : y === '' ? -1 : x.localeCompare(y)))
}

/** The Prism agent that saw it last and still exists: where it comes from. */
export function originOf(a: Aside, s: Snapshot | null | undefined = live.snapshot): Agent | undefined {
  for (const seen of [...a.seen].reverse()) {
    const agent = seen.agent && s?.agents?.find((x) => x.id === seen.agent)
    if (agent) return agent
  }
  return undefined
}

/** Its agent's branch, which a worktree may start from (`needs_current_branch`). */
export const agentBranch = (a: Aside) => originOf(a)?.branch ?? [...a.seen].reverse().find((x) => x.branch)?.branch ?? null

/** A launch as the core gives it (`LaunchSpec`), as the launcher takes it. */
export function launchOf(l: LaunchSpec): Launch {
  const opt = <T>(v: T | null | undefined) => v ?? undefined
  const w = l.worktree
  return {
    title: l.title,
    prompt: opt(l.prompt),
    category: opt(l.category),
    provider: opt(l.provider),
    model: opt(l.model),
    effort: opt(l.effort),
    from: opt(l.from),
    ...(w ? { worktree: { repo: opt(w.repo), name: opt(w.name), branch: opt(w.branch), base: opt(w.base) } } : l.cwd ? { cwd: l.cwd } : {}),
  }
}

/**
 * Lancer: the launcher, prefilled by the core (`aside_launch`): its prompt headed by where it comes
 * from, a new worktree from the base or its agent's branch, its agent's CLI, model and effort, its
 * kind; `from` names it, so that the launch marks it launched. Enter launches it as it is.
 */
export async function launchAside(a: Aside, fromBranch: boolean) {
  const spec = await invoke<LaunchSpec>('aside_launch', { id: a.id, fromBranch })
  openLauncher(launchOf(spec), null, null, t('In passing'))
}

const act = (id: number, action: string, extra: { why?: string; target?: string } = {}) => invoke('aside_action', { id, action, why: extra.why ?? null, target: extra.target ?? null })
export const asideLater = (a: Aside) => act(a.id, 'later')
export const asideReopen = (a: Aside) => act(a.id, 'reopen')
export const asideDismiss = (a: Aside, why = '') => act(a.id, 'dismiss', { why: why.trim() || undefined })
export const asideForget = (a: Aside) => act(a.id, 'forget')
/** After this agent: a "Then" step of its chain, from its branch as it is then, or the base. */
export const asideThen = (a: Aside, fromBranch: boolean) => invoke('aside_then', { id: a.id, fromBranch })
/** In this session: back to the agent that saw it, done once its current turn is over (resumed first if it exited). */
export const asideSession = (a: Aside) => invoke('aside_session', { id: a.id })
/** Done in its session, its agent not having said so. */
export const asideDone = (a: Aside) => act(a.id, 'done')
/** A subject of mine, noted from agent `agent`'s Chat: a line in passing, sent to its session. Its id. */
export const asideNote = (agent: string, text: string) => invoke<number>('aside_note', { agent, text })
/** Its session can take it: the agent that saw it is there, running or to resume. */
export const sessionOf = (a: Aside, s: Snapshot | null | undefined = live.snapshot): Agent | undefined => {
  const o = originOf(a, s)
  return o && (o.state === 'running' || o.state === 'exited') ? o : undefined
}

/** A ticket of the tracker; its key. */
export const asideTicket = (a: Aside) => invoke<string>('aside_ticket', { id: a.id })

/** Of Prism's « in passing » tools (`ConversationTool.aside`), the one that proposes. */
export const isSuggest = (tool: ConversationTool) => !!tool.aside && tool.tool.endsWith('suggest_task')
/** The one that says a task sent back to its session done. */
export const isDoneCall = (tool: ConversationTool) => !!tool.aside && tool.tool.endsWith('done_task')

/** The line a `suggest_task` call made (its answer gives its `task_id`), or withdrew. */
export function asideOfCall(tool: ConversationTool, s: Snapshot | null | undefined = live.snapshot): Aside | undefined {
  const id = Number(tool.output?.match(/"task_id"\s*:\s*"?(\d+)/)?.[1] ?? tool.what?.match(/^#(\d+)$/)?.[1])
  return Number.isFinite(id) ? asidesOf(s).find((a) => a.id === id) : undefined
}

/** Its main button: In this session while its agent is there; else `[agents.aside] default_action`, Then and Ticket only where they apply. */
export function mainAction(a: Aside, canTicket: boolean): 'session' | 'launch' | 'then' | 'ticket' {
  if (sessionOf(a)) return 'session'
  const d = agentsSettings().aside.default_action ?? 'launch'
  if (d === 'then' && originOf(a) && originOf(a)!.state === 'running') return 'then'
  if (d === 'ticket' && canTicket) return 'ticket'
  return 'launch'
}

export const STATE_LABEL: Record<Aside['state'], string> = {
  open: t('open'),
  later: t('later'),
  then: t('after its agent'),
  launched: t('launched'),
  ticket: t('ticket'),
  dismissed: t('dismissed'),
  session: t('in its session'),
  done: t('done'),
}
