import type { ChangeEvent } from '../../bindings/ChangeEvent'
import type { ChangeKind } from '../../bindings/ChangeKind'
import type { ChangeLink } from '../../bindings/ChangeLink'
import type { ChangesState } from '../../bindings/ChangesState'
import type { ChangesClosed } from '../../bindings/ChangesClosed'
import { locale } from '../../locale'
import { t } from '../../i18n.svelte'

/** The journal and its events: Rust's types (`crates/core/src/changes.rs`, `crates/blocks/src/changes`), generated in `bindings/`. */
export type { ChangeEvent, ChangeKind, ChangeLink, ChangesState, ChangesClosed }

const MIN = 60_000
const HOUR = 60 * MIN

/** The periods offered; `custom`: a range picked (dragged on the band, or around a crash). */
export const PERIODS = ['1h', 'since_yesterday', '24h', '7d'] as const
export type Period = (typeof PERIODS)[number] | 'custom'

export interface Range {
  from: number
  to: number
}

/** A period's range, ending now. */
export function rangeOf(period: Period, now: number, custom?: Range | null): Range {
  switch (period) {
    case '1h':
      return { from: now - HOUR, to: now }
    case '24h':
      return { from: now - 24 * HOUR, to: now }
    case '7d':
      return { from: now - 7 * 24 * HOUR, to: now }
    case 'custom':
      return custom ?? { from: now - 24 * HOUR, to: now }
    default: {
      const d = new Date(now)
      d.setDate(d.getDate() - 1)
      d.setHours(0, 0, 0, 0)
      return { from: d.getTime(), to: now }
    }
  }
}

export const periodLabel = (p: Period) =>
  ({ '1h': t('1 h'), since_yesterday: t('Since yesterday'), '24h': t('24 h'), '7d': t('7 d'), custom: t('Custom') })[p]

/** The core's sources first, in this order; a block's after them. */
export const CORE_SOURCES = ['argocd', 'mrs', 'pods', 'alerts', 'vulnerabilities'] as const

export const sourceLabel = (s: string) =>
  ({ argocd: 'Argo CD', mrs: t('MRs'), pods: t('Pods'), alerts: t('Alerts'), vulnerabilities: t('Vulnerabilities') })[s] ?? s

export interface Filter {
  /** null: every environment. */
  env: string | null
  range: Range
  /** The sources hidden. */
  hidden: ReadonlySet<string>
  /** App, author, revision, text. */
  query: string
  /** One application only (around a crash). */
  app?: string | null
}

const at = (e: ChangeEvent) => Date.parse(e.at)

/** An event without an environment is on every one. */
export const onEnv = (e: ChangeEvent, env: string | null) => !env || !e.env || e.env === env

export function matches(e: ChangeEvent, q: string): boolean {
  if (!q) return true
  const hay = [e.title, e.app, e.detail, e.revision, e.mr?.title, e.mr ? `!${e.mr.iid}` : '', e.mr ? `#${e.mr.iid}` : '', ...e.images, ...e.workloads].filter(Boolean).join(' ').toLowerCase()
  return q.toLowerCase().split(/\s+/).filter(Boolean).every((w) => hay.includes(w))
}

const sameApp = (e: ChangeEvent, app: string) => e.app?.toLowerCase() === app.toLowerCase() || e.workloads.includes(app)

/** The events of an environment and a range, before the sources and the search narrow them. */
export const inRange = (events: ChangeEvent[], f: Pick<Filter, 'env' | 'range' | 'app'>) =>
  events.filter((e) => onEnv(e, f.env) && at(e) >= f.range.from && at(e) <= f.range.to && (!f.app || sameApp(e, f.app)))

/** The events shown, oldest first. */
export const visible = (events: ChangeEvent[], f: Filter) => inRange(events, f).filter((e) => !f.hidden.has(e.source) && matches(e, f.query))

/** How many of each source, in the range (the filters' counts). */
export function counts(events: ChangeEvent[], f: Filter, sources: string[]): { source: string; n: number }[] {
  const shown = inRange(events, f).filter((e) => matches(e, f.query))
  const known = [...CORE_SOURCES.filter((s) => sources.includes(s)), ...new Set(shown.map((e) => e.source).filter((s) => !(CORE_SOURCES as readonly string[]).includes(s)))]
  return known.map((source) => ({ source, n: shown.filter((e) => e.source === source).length }))
}

/** Lines: one event, or the same thing repeated (a pod restarting 6 times in 4 min). */
export interface Row {
  key: string
  first: ChangeEvent
  items: ChangeEvent[]
  /** Its newest event: where the list places it. */
  t: number
}

/** Repeats of the same kind on the same application, each within this of the previous, fold. */
const FOLD_GAP = 10 * MIN
const FOLDS: ReadonlySet<ChangeKind> = new Set(['restart', 'crash_loop', 'oom_killed', 'waiting', 'kube_event', 'health'])

/** Rows, newest first. */
export function rows(events: ChangeEvent[]): Row[] {
  const out: Row[] = []
  const open = new Map<string, Row>()
  for (const e of [...events].sort((a, b) => at(a) - at(b))) {
    const k = `${e.env ?? ''}|${e.kind}|${e.app ?? e.title}`
    const r = FOLDS.has(e.kind) ? open.get(k) : undefined
    if (r && at(e) - r.t <= FOLD_GAP) {
      r.items.push(e)
      r.t = at(e)
      continue
    }
    const row = { key: e.id, first: e, items: [e], t: at(e) }
    open.set(k, row)
    out.push(row)
  }
  return out.sort((a, b) => b.t - a.t)
}

/** Rows grouped by hour, newest first. */
export function byHour(list: Row[]): { key: string; label: string; rows: Row[] }[] {
  const groups = new Map<string, { key: string; label: string; rows: Row[] }>()
  const today = new Date().toDateString()
  for (const r of list) {
    const d = new Date(r.t)
    d.setMinutes(0, 0, 0)
    const key = String(d.getTime())
    let g = groups.get(key)
    if (!g) {
      const hour = d.toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
      const day = d.toDateString() === today ? '' : `${d.toLocaleDateString(locale(), { weekday: 'short', day: 'numeric', month: 'short' })} · `
      g = { key, label: `${day}${hour}`, rows: [] }
      groups.set(key, g)
    }
    g.rows.push(r)
  }
  return [...groups.values()]
}

/** The axis' ticks of a range: at most about 8, on round steps. */
export function ticksOf(range: Range): number[] {
  const span = range.to - range.from
  const step = [15, 30, 60, 120, 180, 360, 720, 1440, 2880].map((m) => m * MIN).find((s) => span / s <= 8) ?? 2880 * MIN
  const out: number[] = []
  for (let ms = Math.ceil(range.from / step) * step; ms <= range.to; ms += step) out.push(ms)
  return out
}

/** A tick's label: the day at midnight, the time otherwise. */
export const tickLabel = (ms: number) => {
  const d = new Date(ms)
  return d.getHours() === 0 && d.getMinutes() === 0 ? d.toLocaleDateString(locale(), { weekday: 'short', day: 'numeric' }) : clock(ms)
}

/** The ranges Prism was closed within `range`. */
export const closedIn = (closed: ChangesClosed[], range: Range): Range[] =>
  closed.map((c) => ({ from: Math.max(Date.parse(c.from), range.from), to: Math.min(Date.parse(c.to), range.to) })).filter((c) => c.to > c.from)

const mrRef = (e: ChangeEvent) => (e.mr ? `${e.mr.repo ? `${e.mr.repo}` : ''}!${e.mr.iid}` : '')

/** A row's sentence: what happened, then its detail. */
export function sentence(r: Row): { what: string; detail: string } {
  const e = r.first
  const n = r.items.length
  const span = Math.max(1, Math.round((r.t - at(e)) / MIN))
  const many = (one: string, several: string) => (n > 1 ? t(several, { n, m: span }) : one)
  const subject = e.app ?? e.title
  switch (e.kind) {
    case 'sync':
      return { what: t('{app} synced {revision}', { app: e.title, revision: e.revision ?? '' }).trim(), detail: [e.mr ? `${mrRef(e)} ${e.mr.title}` : '', e.images.join(', '), e.detail === 'manual' ? t('manual') : ''].filter(Boolean).join(' · ') }
    case 'sync_failed':
      return { what: t('{app}: sync failed', { app: e.title }), detail: e.detail ?? '' }
    case 'health':
      return { what: many(`${e.title}: ${e.detail ?? ''}`, '{n} health changes in {m} min'), detail: n > 1 ? r.items.map((i) => i.detail).join(' · ') : '' }
    case 'drift':
      return { what: t('{app}: out of sync', { app: e.title }), detail: e.detail ?? '' }
    case 'merged':
      return { what: t('Merged {mr}', { mr: mrRef(e) }), detail: [e.title, e.detail].filter(Boolean).join(' · ') }
    case 'rollout':
      return { what: t('{app}: new ReplicaSet', { app: e.title }), detail: [e.images.join(', '), e.detail].filter(Boolean).join(' · ') }
    case 'restart':
      return { what: n > 1 ? t('{app}: restarted {n} times in {m} min', { app: subject, n, m: span }) : t('{app}: restarted', { app: subject }), detail: e.detail ?? '' }
    case 'crash_loop':
      return { what: `${subject}: CrashLoopBackOff${n > 1 ? ` ×${n}` : ''}`, detail: e.detail ?? '' }
    case 'oom_killed':
      return { what: `${subject}: OOMKilled${n > 1 ? ` ×${n}` : ''}`, detail: e.detail ?? '' }
    case 'waiting':
      return { what: `${subject}: ${e.detail ?? ''}`, detail: '' }
    case 'kube_event':
      return { what: n > 1 ? `${e.title} ×${n}` : e.title, detail: e.detail ?? '' }
    case 'alert_on':
      return { what: t('Alert: {text}', { text: e.title }), detail: '' }
    case 'alert_off':
      return { what: t('Alert over: {text}', { text: e.title }), detail: '' }
    case 'vuln_new':
      return { what: t('New vulnerability {title}', { title: e.title }), detail: [e.app, e.detail].filter(Boolean).join(' · ') }
    case 'vuln_fixed':
      return { what: t('Vulnerability gone {title}', { title: e.title }), detail: e.app ?? '' }
    default:
      return { what: e.title, detail: e.detail ?? '' }
  }
}

export const isCrash = (e: ChangeEvent) => e.kind === 'crash_loop' || e.kind === 'oom_killed' || e.kind === 'restart' || (e.kind === 'health' && e.severity === 'ko')

/**
 * Around a crash of `app` on `env`: from 2 h before its first crash (the oldest of the crashes
 * that follow each other within 30 min, the newest last) to 15 min after its last, the change
 * linked to it highlighted. No crash in the journal: the last 2 h.
 */
export function around(events: ChangeEvent[], env: string | null, app: string, now: number): { range: Range; highlight: string | null; crash: string | null } {
  const crashes = events.filter((e) => onEnv(e, env) && sameApp(e, app) && isCrash(e)).sort((a, b) => at(b) - at(a))
  if (!crashes.length) return { range: { from: now - 2 * HOUR, to: now }, highlight: null, crash: null }
  let first = crashes[0]
  for (const c of crashes.slice(1)) {
    if (at(first) - at(c) > 30 * MIN) break
    first = c
  }
  const linked = crashes.find((c) => at(c) >= at(first) && c.related)
  return { range: { from: at(first) - 2 * HOUR, to: Math.min(now, at(crashes[0]) + 15 * MIN) }, highlight: linked?.related ?? null, crash: first.id }
}

/** A time: `09:41`. */
export const hm = (ms: number) => new Date(ms).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
const clock = hm

/** A time, its day before it when not today: `09:41`, `Sat 1 16:05`. */
export const when = (ms: number) => {
  const d = new Date(ms)
  return d.toDateString() === new Date().toDateString() ? hm(ms) : `${d.toLocaleDateString(locale(), { weekday: 'short', day: 'numeric' })} ${hm(ms)}`
}

/** The rows as Markdown, oldest first: for a post-mortem or a message. */
export function markdown(list: Row[], env: string | null, range: Range): string {
  const day = (ms: number) => new Date(ms).toLocaleDateString(locale(), { day: 'numeric', month: 'short' })
  const head = `### ${t('Changes')}${env ? ` · ${env}` : ''} · ${day(range.from)} ${clock(range.from)} → ${day(range.to)} ${clock(range.to)}`
  const byId = new Map(list.flatMap((r) => r.items.map((i) => [i.id, r] as const)))
  const lines = [...list].reverse().map((r) => {
    const s = sentence(r)
    const seen = r.first.seen ? ` _(${t('seen at')})_` : ''
    const linked = r.first.related && byId.get(r.first.related) ? ` — ${t('probably linked to {what}', { what: sentence(byId.get(r.first.related)!).what })}` : ''
    return `- **${clock(r.t)}**${r.first.env && !env ? ` [${r.first.env}]` : ''} ${s.what}${s.detail ? ` — ${s.detail}` : ''}${seen}${linked}`
  })
  return [head, '', ...lines].join('\n')
}

