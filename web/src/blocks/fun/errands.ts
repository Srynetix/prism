import type { MergeRequest, Snapshot } from '../../types'
import { meOn, mrNumber } from '../../forge'
import { awaitsMyReview, nextStep, waitsForYou } from '../../rules'
import { agentRepo } from '../../repositories'
import { t } from '../../i18n.svelte'

/**
 * What it fetches (`[pet] fetch`): the things waiting for you, the most urgent first. An agent
 * waiting for you, then the oldest review asked of you, then my MRs blocked or to act on (red CI,
 * conflicts, changes requested), then those ready to merge.
 */
export interface Errand {
  /** Stays the same while the thing waits: once it is gone, it was done. */
  key: string
  /** What it says, bringing it back. */
  text: string
  /** On the paper in its mouth. */
  tag: string
  open: { agent: string } | { mr: MergeRequest }
}

const since = (at: string | null | undefined, now: number) => {
  const min = Math.max(0, Math.round((now - Date.parse(at ?? '')) / 60_000))
  return Number.isNaN(min) ? '' : min < 60 ? t('{n} min', { n: min }) : min < 48 * 60 ? t('{n} h', { n: Math.round(min / 60) }) : t('{n} days', { n: Math.round(min / 1440) })
}

export function errands(s: Snapshot, now = Date.now()): Errand[] {
  const out: Errand[] = []
  const agents = (s.agents ?? []).filter((a) => a.state === 'running' && waitsForYou(a.status)).sort((a, b) => (a.last_activity ?? '').localeCompare(b.last_activity ?? ''))
  for (const a of agents) {
    const where = agentRepo(a, s) ?? a.title
    out.push({ key: `agent:${a.id}`, tag: where, text: t('{title} waits for you ({since}).', { title: a.title || where, since: since(a.last_activity, now) }), open: { agent: a.id } })
  }
  const open = s.merge_requests.filter((m) => m.state === 'opened')
  const key = (m: MergeRequest) => `mr:${m.repo ?? ''}!${m.iid}`
  const reviews = open.filter((m) => m.author !== meOn(s, m) && awaitsMyReview(m, meOn(s, m))).sort((a, b) => a.updated_at.localeCompare(b.updated_at))
  for (const m of reviews) out.push({ key: key(m), tag: mrNumber(m.iid, m.forge), text: t('{who} waits for your review of {mr} ({since}).', { who: m.author_name || m.author, mr: mrNumber(m.iid, m.forge), since: since(m.updated_at, now) }), open: { mr: m } })
  const mine = open.filter((m) => m.author === meOn(s, m)).map((m) => ({ m, step: nextStep(m, meOn(s, m), s.merge_queue_label) }))
  for (const tone of ['blocked', 'todo', 'ready'] as const) {
    for (const { m, step } of mine.filter((x) => x.step.tone === tone && (tone !== 'ready' || !!x.step.primary))) {
      out.push({ key: `${key(m)}:${step.label}`, tag: mrNumber(m.iid, m.forge), text: t('{mr}: {step}.', { mr: mrNumber(m.iid, m.forge), step: step.label }), open: { mr: m } })
    }
  }
  return out
}
