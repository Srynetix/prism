import type { AgentProvider, ClaudeUsage, Snapshot, UsageWindow } from './types'
import { dateFormat } from './locale'
import { t } from './i18n.svelte'

/** Plan usage (Claude Code or Command Code): computations shared by the Claude block (Usage.svelte) and the status bar. */

/** A reading measured over 30 min ago (Codex dates its own, from its last request): shown as out of date. */
const OUT_OF_DATE = 30 * 60_000

/** When its reading was measured, if that is out of date: today's time, another day's with its day. */
export function outdated(u: ClaudeUsage | null | undefined, now: number): string | null {
  if (!u) return null
  const at = new Date(u.fetched_at)
  if (now - at.getTime() < OUT_OF_DATE) return null
  const today = at.toDateString() === new Date(now).toDateString()
  return dateFormat(today ? { hour: '2-digit', minute: '2-digit' } : { weekday: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(at)
}

/** `ko` above 90 %, `warn` above 70 %. */
export const usageLevel = (w: UsageWindow) => (w.utilization >= 90 ? 'ko' : w.utilization >= 70 ? 'warn' : '')

export function resets(w: UsageWindow): string {
  if (!w.resets_at) return ''
  const at = new Date(w.resets_at)
  const sameDay = at.toDateString() === new Date().toDateString()
  return t('resets {when}', { when: `${sameDay ? '' : dateFormat({ weekday: 'short' }).format(at) + ' '}${dateFormat({ hour: '2-digit', minute: '2-digit' }).format(at)}` })
}

const DAY = 86_400_000

/**
 * A plan window split into days, aligned on its reset: each cell is worth an equal share of the
 * plan. The fill is what has been consumed; the "now" marker is the elapsed share of the period,
 * i.e. what steady consumption would have already used. Beyond the marker, I am consuming
 * the following days' share; below it, I still have some margin.
 */
function period(w: UsageWindow, now: number, start: number, end: number, label: (from: Date, i: number) => string) {
  const length = end - start
  const count = Math.max(1, Math.round(length / DAY))
  const cell = length / count
  const elapsed = Math.min(100, Math.max(0, ((now - start) / length) * 100))
  const share = 100 / count
  const within = (v: number, i: number) => Math.min(100, Math.max(0, ((v - i * share) / share) * 100))
  const days = Array.from({ length: count }, (_, i) => {
    const from = new Date(start + i * cell)
    const current = start + i * cell <= now && now < start + (i + 1) * cell
    // Cell fill: consumed within the pace (up to the marker), then beyond the marker.
    const used = within(w.utilization, i)
    const paced = Math.min(used, within(elapsed, i))
    return {
      from,
      label: label(from, i) || (current ? String(from.getDate()) : ''),
      current,
      nowAt: ((now - (start + i * cell)) / cell) * 100,
      bg: `linear-gradient(to right, var(--claude-fill) 0 ${paced}%, var(--over-fill) ${paced}% ${used}%, transparent ${used}%)`,
    }
  })
  const left = Math.max(0, 100 - w.utilization)
  const daysLeft = Math.max(0, (end - now) / DAY)
  return { used: w.utilization, elapsed, days, perDay: daysLeft > 0 ? left / daysLeft : left, daysLeft, ahead: w.utilization - elapsed }
}

export type Period = ReturnType<typeof period>

/** The week in 7 day cells, named after their weekday. */
export function week(w: UsageWindow | null | undefined, now: number): Period | null {
  if (!w?.resets_at) return null
  const end = new Date(w.resets_at).getTime()
  return period(w, now, end - 7 * DAY, end, (from) => dateFormat({ weekday: 'short' }).format(from))
}

/** The billing month (Command Code) in day cells, from the previous renewal; a date every 7 days. */
export function month(w: UsageWindow | null | undefined, now: number): Period | null {
  if (!w?.resets_at) return null
  const end = new Date(w.resets_at)
  const start = new Date(end)
  start.setMonth(start.getMonth() - 1)
  return period(w, now, start.getTime(), end.getTime(), (from, i) => (i % 7 === 0 ? String(from.getDate()) : ''))
}

/** More than 5 points ahead of a steady pace: `over`; more than 5 points behind: `under`. */
export const pace = (ahead: number) => (ahead > 5 ? 'over' : ahead < -5 ? 'under' : 'on')

/** `noun`: `week` or `month`, the period drawn. */
export function weekTitle(w: UsageWindow, wk: Period, noun = 'week') {
  const elapsed = Math.round(wk.elapsed)
  const pace = wk.ahead >= 0 ? t('{pts} pts ahead of an even pace', { pts: wk.ahead.toFixed(1) }) : t('{pts} pts under an even pace', { pts: (-wk.ahead).toFixed(1) })
  const left = t('{pct}% left over {days} days = {perDay}%/day', { pct: (100 - w.utilization).toFixed(0), days: wk.daysLeft.toFixed(1), perDay: wk.perDay.toFixed(1) })
  return `${t('{pct}% used', { pct: Math.round(w.utilization) })} · ${noun === 'month' ? t('{pct}% of the month elapsed', { pct: elapsed }) : t('{pct}% of the week elapsed', { pct: elapsed })} (${pace}) · ${left} · ${resets(w)}`
}

/** A CLI's plan usage, as the status bar's popup and the phone show it. */
export interface PlanSection {
  provider: AgentProvider
  /** Its account (`[[agents.accounts]]`), each its own plan; null: the CLI's usual folder. */
  account: string | null
  usage: ClaudeUsage | null
  error: string | undefined
  /** The CLI of `[agents] provider`. */
  configured: boolean
  /** Its agents running. */
  agents: number
}

/**
 * The plan usage of each CLI in use: `[agents] provider`'s first, then those other agents run on, in
 * the order the backend reads them; OpenCode, which has no plan, only while one of its agents runs.
 */
export function planSections(s: Snapshot): PlanSection[] {
  const configured = s.agents_settings?.provider ?? 'claude'
  const mine = s.usage_account ?? null
  const running = (p: AgentProvider, account: string | null) => (s.agents ?? []).filter((a) => a.state === 'running' && a.provider === p && (a.account ?? null) === account).length
  const out: PlanSection[] = [{ provider: configured, account: mine, usage: s.usage ?? null, error: s.errors.usage, configured: true, agents: running(configured, mine) }]
  for (const o of s.other_usage ?? []) out.push({ provider: o.provider, account: o.account ?? null, usage: o.usage, error: o.error ?? undefined, configured: false, agents: running(o.provider, o.account ?? null) })
  if (configured !== 'opencode' && running('opencode', null)) out.push({ provider: 'opencode', account: null, usage: null, error: undefined, configured: false, agents: running('opencode', null) })
  // A configured CLI without a plan, with no agent: nothing to say about it.
  return out.filter((p) => p.usage?.five_hour || p.usage?.seven_day || p.agents || p.error)
}

/** A section's key: its CLI and account (two accounts of a CLI, two plans). */
export const sectionKey = (p: PlanSection) => `${p.provider}:${p.account ?? ''}`

/** A section's name: its account's (`Claude pro`), else its CLI's. */
export const sectionName = (p: PlanSection, cliName: string) => p.account ?? cliName

/** A section with windows to draw (the others have no plan, or no reading yet). */
export const hasWindows = (p: PlanSection) => !!(p.usage?.five_hour || p.usage?.seven_day)

/** The highest use of a section's windows, 0 without any. */
export const peak = (p: PlanSection) => Math.max(0, ...[p.usage?.five_hour, p.usage?.seven_day, ...(p.usage?.others ?? []).map(([, w]) => w)].filter((w): w is UsageWindow => !!w).map((w) => w.utilization))

/** How much of `p`'s plan is used now (its fullest window), when Prism reads it. */
export function planUseOf(s: Snapshot, p: AgentProvider, account?: string | null): number | null {
  // An account's own plan; without one asked, the CLI's first section (as before accounts).
  const section = planSections(s).find((x) => x.provider === p && (account === undefined || x.account === (account ?? null)))
  return section?.usage ? peak(section) : null
}

/** A plan used up: its CLI refuses to go on until it resets. */
export const USED_UP = 99.5
