import { t, tn } from '../../i18n.svelte'
import type { BqCost, DataStream, DataThresholds, GcpData, RunExecution, RunJob, ScheduledQuery, TableFreshness } from './types'

/**
 * Data pipelines on GCP: the state of each job, table, scheduled query and stream, and the
 * estimated BigQuery cost. Shared by the Data and BigQuery cost blocks, the alerts and the rail.
 * Thresholds: `[data] slow_factor`, `stream_lag_minutes`, `cost_spike_factor`, carried by the state.
 */
export type DataTone = 'ko' | 'warn' | 'run' | 'ok' | 'off'
export interface DataState {
  tone: DataTone
  label: string
  /** Why, in one sentence (tooltip, alert). */
  why?: string
}

const H = 3_600_000

/** Duration of a run; while running, up to now. */
export const runDuration = (e: RunExecution, now = Date.now()) =>
  e.started_at ? (e.finished_at ? new Date(e.finished_at).getTime() : now) - new Date(e.started_at).getTime() : null

export function median(values: number[]): number | null {
  if (!values.length) return null
  const s = [...values].sort((a, b) => a - b)
  const m = Math.floor(s.length / 2)
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2
}

/** Median duration of successful runs, excluding the most recent (the one being compared). */
export function medianDuration(j: RunJob): number | null {
  return median(j.executions.slice(1).filter((e) => e.status === 'succeeded').map((e) => runDuration(e)).filter((d): d is number => d !== null))
}

/** Consecutive failures, from the most recent backwards. */
const failStreak = (j: RunJob) => {
  const i = j.executions.findIndex((e) => e.status !== 'failed')
  return i === -1 ? j.executions.length : i
}

export function jobState(j: RunJob, th: DataThresholds, now = Date.now()): DataState {
  const last = j.executions[0]
  if (j.trigger?.error) return { tone: 'ko', label: t('run missed'), why: t('Cloud Scheduler could not start it: {error}', { error: j.trigger.error }) }
  if (!last) return { tone: 'off', label: t('never run') }
  if (last.status === 'failed') {
    const n = failStreak(j)
    const failed = tn(last.tasks, '{failed}/{n} task failed', '{failed}/{n} tasks failed', { failed: last.failed_tasks })
    const why = last.exit_code !== null ? t('{failed}, exit code {code}', { failed, code: last.exit_code }) : failed
    return { tone: 'ko', label: n > 1 ? t('failed ×{n}', { n }) : t('failed'), why }
  }
  const med = medianDuration(j)
  const d = runDuration(last, now)
  const factor = th.slow_factor
  const slow = med && d !== null && d > med * factor ? d / med : null
  if (last.status === 'running') return slow ? { tone: 'warn', label: t('{x}× median', { x: slow.toFixed(1) }), why: t('still running, far longer than usual') } : { tone: 'run', label: t('running') }
  if (last.status === 'cancelled') return { tone: 'warn', label: t('cancelled') }
  if (slow) return { tone: 'warn', label: t('{x}× median', { x: slow.toFixed(1) }), why: t('the last run took far longer than usual') }
  if (j.trigger && j.trigger.state !== 'ENABLED') return { tone: 'off', label: t(j.trigger.state.toLowerCase()) }
  return { tone: 'ok', label: t('ok') }
}

/** Age of a table, in hours. */
export const tableAge = (tb: TableFreshness, now = Date.now()) => (tb.last_modified ? (now - new Date(tb.last_modified).getTime()) / H : null)

export function tableState(table: TableFreshness, jobs: RunJob[], th: DataThresholds, now = Date.now()): DataState {
  if (table.error) return { tone: 'warn', label: t('unreadable'), why: table.error }
  const age = tableAge(table, now)
  if (age === null) return { tone: 'off', label: t('never written') }
  if (age <= table.max_age_hours) return { tone: 'ok', label: t('fresh') }
  const late = t('late {h}h', { h: Math.round(age - table.max_age_hours) })
  const producer = table.fed_by ? jobs.find((j) => j.name === table.fed_by) : undefined
  const broken = producer && jobState(producer, th, now).tone === 'ko'
  return broken
    ? { tone: 'ko', label: late, why: t('its producer {name} failed', { name: producer.name }) }
    : { tone: 'warn', label: late, why: t('not modified for {age}h (expected every {max}h)', { age: Math.round(age), max: table.max_age_hours }) }
}

export function transferState(q: ScheduledQuery): DataState {
  if (q.disabled) return { tone: 'off', label: t('disabled') }
  if (q.state === 'FAILED') return { tone: 'ko', label: t('failed'), why: t('its last run failed') }
  if (q.state === 'RUNNING' || q.state === 'PENDING') return { tone: 'run', label: t(q.state.toLowerCase()) }
  if (q.state === 'CANCELLED') return { tone: 'warn', label: t('cancelled') }
  return { tone: 'ok', label: t('ok') }
}

/** `18 min`, `42 s`, `2 h 05`. */
export function lag(seconds: number): string {
  if (seconds < 90) return `${Math.round(seconds)} s`
  const m = Math.round(seconds / 60)
  return m < 90 ? `${m} min` : `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')}`
}

export function streamState(s: DataStream, th: DataThresholds): DataState {
  if (s.state.startsWith('FAILED')) return { tone: 'ko', label: s.state, why: s.errors[0] ?? t('the stream stopped on an error') }
  if (s.state === 'PAUSED' || s.state === 'NOT_STARTED') return { tone: 'off', label: s.state }
  const limit = th.stream_lag_minutes * 60
  if (s.freshness_seconds !== null && s.freshness_seconds > limit) return { tone: 'warn', label: t('lag {lag}', { lag: lag(s.freshness_seconds) }), why: t('{lag} behind (threshold {limit})', { lag: lag(s.freshness_seconds), limit: lag(limit) }) }
  if (s.errors.length) return { tone: 'warn', label: s.state, why: s.errors.join(', ') }
  if (s.state !== 'RUNNING') return { tone: 'run', label: s.state }
  return { tone: 'ok', label: s.state }
}

/* ---------- Cost ---------- */

const TIB = 2 ** 40

export const dollars = (bytes: number, c: BqCost) => (bytes / TIB) * c.price_per_tib
export const money = (v: number) => (v >= 100 ? `$${Math.round(v)}` : `$${v.toFixed(2)}`)

export function bytes(b: number): string {
  if (b >= TIB) return `${(b / TIB).toFixed(1)} TiB`
  if (b >= 2 ** 30) return `${Math.round(b / 2 ** 30)} GiB`
  if (b >= 2 ** 20) return `${Math.round(b / 2 ** 20)} MiB`
  return b ? `${Math.round(b / 1024)} KiB` : '0'
}

export interface CostStats {
  today: number
  /** Median of the previous days that had jobs; null without history. */
  median: number | null
  /** Today / median, when it is a spike. */
  spike: number | null
}

export function costStats(c: BqCost, th: DataThresholds): CostStats {
  const today = c.days.at(-1)?.bytes ?? 0
  const med = median(c.days.slice(0, -1).filter((d) => d.jobs > 0).map((d) => d.bytes))
  const ratio = med ? today / med : null
  return { today, median: med, spike: ratio !== null && ratio > th.cost_spike_factor ? ratio : null }
}

/* ---------- Overall ---------- */

export interface DataProblem {
  kind: 'job' | 'table' | 'query' | 'stream' | 'cost'
  name: string
  state: DataState
  url?: string
}

/** What is wrong, most severe first. */
export function dataProblems(d: GcpData | null | undefined, now = Date.now()): DataProblem[] {
  if (!d) return []
  const out: DataProblem[] = []
  const push = (kind: DataProblem['kind'], name: string, state: DataState, url?: string) => {
    if (state.tone === 'ko' || state.tone === 'warn') out.push({ kind, name, state, url })
  }
  for (const j of d.jobs) push('job', j.name, jobState(j, d.thresholds, now), jobUrl(j))
  for (const tb of d.tables) push('table', tb.table, tableState(tb, d.jobs, d.thresholds, now), tableUrl(tb.table))
  for (const q of d.transfers) push('query', q.name, transferState(q), `https://console.cloud.google.com/bigquery/scheduled-queries?project=${q.project}`)
  for (const s of d.streams) push('stream', s.display_name, streamState(s, d.thresholds), streamUrl(s))
  const c = d.cost && costStats(d.cost, d.thresholds)
  if (d.cost && c?.spike) push('cost', t('BigQuery cost'), { tone: 'warn', label: t('{x}× median', { x: c.spike.toFixed(1) }), why: t('{today} today, median {median}', { today: money(dollars(c.today, d.cost)), median: money(dollars(c.median ?? 0, d.cost)) }) })
  return out.sort((a, b) => (a.state.tone === b.state.tone ? 0 : a.state.tone === 'ko' ? -1 : 1))
}

/* ---------- Links to the console ---------- */

const CONSOLE = 'https://console.cloud.google.com'
export const jobUrl = (j: RunJob) => `${CONSOLE}/run/jobs/details/${j.region}/${j.name}/executions?project=${j.project}`
export const jobLogsUrl = (j: RunJob) => `${CONSOLE}/run/jobs/details/${j.region}/${j.name}/logs?project=${j.project}`
export function tableUrl(table: string): string {
  const [p, d, tb] = table.split('.')
  return `${CONSOLE}/bigquery?project=${p}&ws=!1m5!1m4!4m3!1s${p}!2s${d}!3s${tb}`
}
export const streamUrl = (s: DataStream) => `${CONSOLE}/datastream/streams/locations/${s.region}/instances/${s.name}?project=${s.project}`
export const bqJobUrl = (project: string, location: string | null, id: string) =>
  `${CONSOLE}/bigquery?project=${project}&j=bq:${location ?? 'EU'}:${id}&page=queryresults`
