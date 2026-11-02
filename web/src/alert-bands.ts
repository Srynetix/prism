import type { Alert, Severity } from './rules'
import { t } from './i18n.svelte'

/**
 * Alerts by what they hold up rather than by kind: `now` (someone waits on you, a person or an
 * agent, or something is broken), `today` (your own work is blocked), `later` (nothing presses).
 * Each alert's band comes from its rule (`[alerts] band_now`, `band_later`, `band_hidden`); a rule
 * named nowhere goes by its severity: Now when broken, When you can for information, Today otherwise. Today's alerts held for longer than
 * `[alerts] escalate_hours` go up to Now: a review asked yesterday no longer waits politely.
 */
export type Band = 'now' | 'today' | 'later'
export const BANDS: Band[] = ['now', 'today', 'later']

/** What raised an alert (`Alert.rule`), as `[alerts] band_*` names them. */
export type AlertRule =
  | 'question' | 'permission' | 'your-turn' | 'agent-cost' | 'agent-loop' | 'agent-limit'
  | 'review' | 'open-threads' | 'mr-ci' | 'conflicts' | 'rebase-failed' | 'need-rebase' | 'only-ci' | 'no-reviewer' | 'no-ticket'
  | 'sync' | 'queue' | 'branch-ci' | 'infra' | 'argo' | 'source' | 'block' | 'worktree'

/** Every rule, in Settings' order, with its name there. */
export const ALERT_RULES: { id: AlertRule; label: string }[] = [
  { id: 'question', label: t('Agent question or plan') },
  { id: 'permission', label: t('Agent permission') },
  { id: 'your-turn', label: t('Agent turn over') },
  { id: 'agent-loop', label: t('Agent in circles') },
  { id: 'agent-cost', label: t('Agent past its cost limit') },
  { id: 'agent-limit', label: t('Agent at its plan’s limit') },
  { id: 'review', label: t('Review asked of you') },
  { id: 'open-threads', label: t('Unresolved threads') },
  { id: 'mr-ci', label: t('My MR’s CI failed') },
  { id: 'conflicts', label: t('Conflicts') },
  { id: 'rebase-failed', label: t('Rebase failed') },
  { id: 'need-rebase', label: t('To rebase') },
  { id: 'only-ci', label: t('Only waits for its CI') },
  { id: 'no-reviewer', label: t('No reviewer') },
  { id: 'no-ticket', label: t('No ticket id') },
  { id: 'sync', label: t('Ticket status vs MRs') },
  { id: 'queue', label: t('Merge queue') },
  { id: 'branch-ci', label: t('Protected branch CI') },
  { id: 'infra', label: t('Pods and runners') },
  { id: 'argo', label: 'ArgoCD' },
  { id: 'source', label: t('Unreachable source') },
  { id: 'block', label: t('Blocks (Docker, dev servers…)') },
  { id: 'worktree', label: t('Worktrees') },
]

export const BAND_LABEL: Record<Band, string> = { now: t('Now · someone waits'), today: t('Today · it blocks your work'), later: t('When you can') }

/** The `[alerts]` keys bands read. */
export interface BandSettings { band_now: string[]; band_later: string[]; band_hidden: string[]; escalate_hours: number }

/** The band its rule gives, before age; `null`: a rule hidden. */
export function ruleBand(a: Pick<Alert, 'rule' | 'severity'>, s: BandSettings): Band | null {
  const rule = a.rule ?? ''
  if (s.band_hidden.includes(rule)) return null
  if (s.band_now.includes(rule)) return 'now'
  if (s.band_later.includes(rule)) return 'later'
  return a.severity === 'ko' ? 'now' : a.severity === 'info' ? 'later' : 'today'
}

/** Its band, `minutes` after it began: Today goes up to Now past `escalate_hours` (0: never). When you can stays so. */
export function bandOf(a: Pick<Alert, 'rule' | 'severity'>, s: BandSettings, minutes: number | null): Band | null {
  const band = ruleBand(a, s)
  return band === 'today' && s.escalate_hours > 0 && minutes !== null && minutes >= s.escalate_hours * 60 ? 'now' : band
}

/** The object an alert is about (its `open`): alerts of one MR, one ticket, one pipeline go together. */
export function objectKey(a: Pick<Alert, 'open'>): string | null {
  const o = a.open
  if (!o) return null
  switch (o.kind) {
    case 'mr':
      return `mr:${o.repo ?? ''}:${o.iid}`
    case 'ticket':
      return `ticket:${o.id}`
    case 'pipeline':
      return `pipeline:${o.project ?? ''}:${o.pipeline}`
    case 'argo':
      return `argo:${o.cluster}:${o.app}`
  }
}

/** An alert from one snapshot to the next: its rule and its object, its text when it has none (a count changing keeps it). */
export const alertId = (a: Pick<Alert, 'rule' | 'kind' | 'open' | 'text'>) => `${a.rule ?? a.kind}|${objectKey(a) ?? a.text}`

/**
 * Of an object's alerts, the one to act on first, its fix unblocking the others: conflicts before
 * anything (the branch must be redone), the threads before the CI (fixing them pushes, the CI
 * runs again), then the rest by severity.
 */
const LEAD: AlertRule[] = ['conflicts', 'rebase-failed', 'question', 'permission', 'review', 'open-threads', 'mr-ci', 'no-reviewer', 'queue', 'need-rebase', 'only-ci', 'sync', 'no-ticket']
const SEVERITY: Record<Severity, number> = { ko: 0, warn: 1, info: 2 }
const leadRank = (a: Alert) => {
  const i = LEAD.indexOf(a.rule as AlertRule)
  return i < 0 ? LEAD.length : i
}

export interface AlertGroup {
  /** The object (`objectKey`), or the alert's own id when it has none. */
  key: string
  band: Band
  /** Its alerts, the lead first. */
  alerts: Alert[]
  /** Minutes since its oldest alert began, when known. */
  minutes: number | null
}

/**
 * The alerts shown, by band then group: an object's alerts together (`group`), in the most urgent
 * band of theirs; in a band, the most severe groups first, then the oldest. Hidden rules left out.
 * `minutes`: since when each alert holds.
 */
export function arrangeAlerts(list: Alert[], s: BandSettings, minutes: (a: Alert) => number | null, group = true): { band: Band; groups: AlertGroup[] }[] {
  const groups = new Map<string, AlertGroup>()
  list.forEach((a, i) => {
    const m = minutes(a)
    const band = bandOf(a, s, m)
    if (!band) return
    const key = (group && objectKey(a)) || `${alertId(a)}#${i}`
    const g = groups.get(key)
    if (!g) {
      groups.set(key, { key, band, alerts: [a], minutes: m })
      return
    }
    g.alerts.push(a)
    if (BANDS.indexOf(band) < BANDS.indexOf(g.band)) g.band = band
    if (m !== null && (g.minutes === null || m > g.minutes)) g.minutes = m
  })
  for (const g of groups.values()) g.alerts.sort((a, b) => leadRank(a) - leadRank(b) || SEVERITY[a.severity] - SEVERITY[b.severity])
  const worst = (g: AlertGroup) => Math.min(...g.alerts.map((a) => SEVERITY[a.severity]))
  return BANDS.map((band) => ({
    band,
    groups: [...groups.values()].filter((g) => g.band === band).sort((a, b) => worst(a) - worst(b) || (b.minutes ?? -1) - (a.minutes ?? -1)),
  })).filter((b) => b.groups.length)
}
