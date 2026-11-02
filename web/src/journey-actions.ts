import { agentsTarget, mrActions, runClaude, ticketActions, type ClaudeAction } from './claude-actions'
import { actorName } from './agent-cli'
import { openAgents } from './agents.svelte'
import { primaryAction, type Primary } from './mr-actions'
import { changesRequested, nextStep, waitsForYou } from './rules'
import { meOn } from './forge'
import { mrKey } from './mr-ref'
import { ticketStep } from './ticket-step'
import { t } from './i18n.svelte'
import type { Journey } from './journey'
import type { Snapshot } from './types'

/** An agent action as a button: the launcher in the app, Claude Desktop otherwise. */
function claude(a: ClaudeAction | undefined, title: string): Primary | null {
  if (a?.launch && agentsTarget()) return { label: a.label, title: t('{title} — opens the agent launcher', { title }), run: () => runClaude(a) }
  return a?.href ? { label: a.label, title: t('{title} — opens a Claude Code session', { title }), href: a.href } : null
}

/**
 * The one action that moves a journey on (journey.ts): the review asked of me, the feedback on my
 * MR applied (Review-fix), the agent that waits for me, the ticket validated; otherwise the
 * ticket's next step as its row offers it (take it, implement it, its MR's Fix, Rebase, Merge…).
 */
export function journeyAction(j: Journey, s: Snapshot): Primary | null {
  const m = j.mr
  const queue = s.merge_queue_label
  if (j.role === 'reviewer' && m) return primaryAction('claude-review', m, queue)
  if (m && m.state === 'opened' && j.role === 'author' && (changesRequested(m) || m.open_threads > 0)) {
    return claude(mrActions(m).find((a) => a.id === 'review-fix'), t('{actor} applies the review’s feedback on {mr}', { actor: actorName(m.repo), mr: mrKey(m) }))
  }
  const asking = j.agents.find((a) => a.state === 'running' && waitsForYou(a.status))
  if (asking) return { label: t('Answer'), title: t('Open the agent “{title}”, which waits for you', { title: asking.title }), run: () => openAgents(asking.id) }
  if (j.ticket) {
    if (j.step === 'validate') {
      const v = ticketActions(j.ticket).find((a) => a.id === 'validate')
      if (v) return claude(v, t('{actor} helps you validate {id} locally', { actor: actorName(j.ticket.repo), id: j.ticket.id }))
    }
    return ticketStep(j.ticket, s.tracker_me, j.statuses).primary
  }
  return m && m.state === 'opened' ? primaryAction(nextStep(m, meOn(s, m), queue).primary, m, queue) : null
}
