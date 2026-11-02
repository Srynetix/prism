import type { AgentProvider, HandoffMaterial } from './types'
import { oneLine, quoted } from './untrusted'

/**
 * The relay an agent's next CLI starts from (Continue with…), written in `[agents] prompt_language`
 * like every prompt Prism sends: `prism` from what Prism reads (its task, plan, last turns, its
 * branch's state), `agent` around the summary its former CLI wrote, `task` its task only.
 */
export type Handoff = 'prism' | 'agent' | 'task'
export type Lang = 'fr' | 'en'

/** What Prism's relay recalls, each part off-able in the window. */
export interface Include {
  prompt: boolean
  plan: boolean
  turns: boolean
  files: boolean
  commits: boolean
}
export const INCLUDE_ALL: Include = { prompt: true, plan: true, turns: true, files: true, commits: true }

export interface RelayContext {
  /** The former CLI's name (Claude, Codex…). */
  from: string
  reason: 'limit' | 'choice'
  ticket?: string | null
  mr?: string | null
}

const WORDS = {
  en: {
    title: '# Picking up work in progress',
    intro: (from: string, reason: string) => `You continue the work of another agent (${from}) in this folder${reason}. Do not redo what is done: check where it is with git first, then carry on.`,
    limit: ': it stopped at its usage limit',
    task: '## The task',
    plan: '## The plan',
    where: '## Where it was',
    me: 'me',
    files: '## Its branch',
    commits: 'Commits since its base:',
    uncommitted: 'Not committed:',
    changed: 'Changed since its base:',
    note: '## Its own handoff note',
    ticket: 'Ticket',
    mr: 'Merge request',
    branch: (b: string, base?: string | null) => `Branch ${b}${base ? `, from ${base}` : ''}.`,
    read: (base?: string | null) => `Read \`git status\` and \`git diff${base ? ` ${base}...` : ''}\` to see where it is.`,
    ask: 'Write a handoff note for another agent who will take this work over in this folder: the goal, what is done, what is left, the decisions taken and why, the pitfalls met, the files involved, and how to check the result. Reply with the note only, in Markdown, nothing else.',
  },
  fr: {
    title: '# Reprise d’un travail en cours',
    intro: (from: string, reason: string) => `Tu continues le travail d’un autre agent (${from}) dans ce dossier${reason}. Ne refais pas ce qui est fait : vérifie d’abord où il en est avec git, puis continue.`,
    limit: ' ; il s’est arrêté à sa limite d’usage',
    task: '## La demande',
    plan: '## Le plan',
    where: '## Où il en était',
    me: 'moi',
    files: '## Sa branche',
    commits: 'Commits depuis sa base :',
    uncommitted: 'Non commité :',
    changed: 'Modifié depuis sa base :',
    note: '## Son mot de passation',
    ticket: 'Ticket',
    mr: 'Merge request',
    branch: (b: string, base?: string | null) => `Branche ${b}${base ? `, depuis ${base}` : ''}.`,
    read: (base?: string | null) => `Lis \`git status\` et \`git diff${base ? ` ${base}...` : ''}\` pour voir où il en est.`,
    ask: 'Écris un mot de passation pour un autre agent qui va reprendre ce travail dans ce dossier : le but, ce qui est fait, ce qui reste, les décisions prises et pourquoi, les pièges rencontrés, les fichiers concernés, et comment vérifier le résultat. Réponds avec le mot seul, en Markdown, rien d’autre.',
  },
}

const words = (lang: Lang) => WORDS[lang] ?? WORDS.en

/** The prompt that asks the former agent for its own summary (relay `agent`). */
export const summaryRequest = (lang: Lang) => words(lang).ask

const STEP: Record<string, string> = { completed: '✓', in_progress: '→', pending: '◻' }

/** A block of the branch's text kept as it is (diff stats, status, commits): names from the repository, which can't close it. */
const fenced = (text: string) => quoted(text.trim())

function links(m: HandoffMaterial, ctx: RelayContext, lang: Lang): string[] {
  const w = words(lang)
  const out: string[] = []
  if (ctx.ticket) out.push(`${w.ticket}: ${ctx.ticket}`)
  if (ctx.mr) out.push(`${w.mr}: ${ctx.mr}`)
  // A branch can be an MR author's (a review checks it out): one line, nothing to close.
  if (m.branch) out.push(w.branch(oneLine(m.branch, 200), m.base ? oneLine(m.base, 200) : m.base))
  return out
}

/** The relay as sent: Markdown, its parts in the order the next agent needs them. */
export function relayText(m: HandoffMaterial, handoff: Handoff, ctx: RelayContext, lang: Lang, include: Include = INCLUDE_ALL): string {
  const w = words(lang)
  const parts: string[] = [w.title, w.intro(ctx.from, ctx.reason === 'limit' ? w.limit : '')]
  const task = (m.prompt ?? '').trim() || m.title
  if (handoff === 'task' || include.prompt) parts.push(`${w.task}\n${task}`)
  if (handoff === 'agent') {
    if (m.last_reply?.trim()) parts.push(`${w.note}\n${m.last_reply.trim()}`)
    parts.push([...links(m, ctx, lang), w.read(m.base)].join('\n'))
    return parts.join('\n\n')
  }
  if (handoff === 'task') {
    parts.push([...links(m, ctx, lang), w.read(m.base)].join('\n'))
    return parts.join('\n\n')
  }
  if (include.plan && m.plan.length) parts.push(`${w.plan}\n${m.plan.map((s, i) => `${i + 1}. ${STEP[s.status] ?? '◻'} ${oneLine(s.text, 300)}`).join('\n')}`)
  // The turns stay as written: the conversation the next agent takes up, its own trust.
  if (include.turns && m.turns.length) {
    const lines = m.turns.flatMap((t) => [...(t.prompt.trim() ? [`— ${w.me}: ${t.prompt.trim()}`] : []), ...(t.reply.trim() ? [`— ${ctx.from}: ${t.reply.trim()}`] : [])])
    parts.push(`${w.where}\n${lines.join('\n\n')}`)
  }
  const branch: string[] = links(m, ctx, lang)
  if (include.files && m.diffstat.trim()) branch.push(`${w.changed}\n${fenced(m.diffstat)}`)
  if (include.files && m.status.trim()) branch.push(`${w.uncommitted}\n${fenced(m.status)}`)
  if (include.commits && m.commits.length) branch.push(`${w.commits}\n${fenced(m.commits.join('\n'))}`)
  if (branch.length) parts.push(`${w.files}\n${branch.join('\n')}`)
  return parts.join('\n\n')
}

/** The relays a CLI can be handed over from: Prism's and the agent's own need its conversation read. */
export const handoffs = (readable: boolean): Handoff[] => (readable ? ['prism', 'agent', 'task'] : ['task'])

/** A rough count of tokens (4 characters each), shown beside the relay. */
export const tokensOf = (text: string) => Math.ceil(text.length / 4)

/**
 * The CLI to suggest when the agent's is used up: among those installed, other than its own, the
 * one whose plan is the least used (no reading counts as half used; OpenCode, with no plan, last).
 */
export function suggestTarget(current: AgentProvider, installed: AgentProvider[], use: (p: AgentProvider) => number | null): AgentProvider | null {
  const score = (p: AgentProvider) => (p === 'opencode' ? 99 : (use(p) ?? 50))
  const others = installed.filter((p) => p !== current && score(p) < 99.5)
  return others.sort((a, b) => score(a) - score(b))[0] ?? installed.find((p) => p !== current) ?? null
}
