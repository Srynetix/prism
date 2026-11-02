import { live } from './live.svelte'
import type { AlertsSettings, Conventions, Ticket } from './types'

/**
 * Team and repository conventions (`[clickup]`, `[gitlab]`) and alert thresholds
 * (`[alerts]`), read from the snapshot; their default values until it has carried them.
 */
const DEFAULTS: Conventions = {
  ticket_prefix: '',
  ticket_patterns: [],
  forge: 'gitlab',
  gitlab_host: 'gitlab.com',
  main_branch: 'main',
  release_prefix: '',
  workflow_statuses: ['to do', 'in progress', 'review', 'complete'],
  before_dev_statuses: ['to do'],
  in_progress_statuses: ['in progress'],
  review_statuses: ['review'],
  approved_statuses: [],
  merge_queue_statuses: [],
  after_merge_statuses: ['complete'],
  done_statuses: ['complete'],
  blocked_statuses: ['blocked'],
  validation_statuses: [],
  branch_aliases: [],
  pick_statuses: ['to do'],
  time_history_days: 30,
  day_hours: 8,
  estimate_builtin: true,
  estimate_fields: [],
  avatars: true,
  forge_avatars: true,
  actuals: true,
  next_action: true,
  actuals_logged_only: false,
  ticket_type: '',
}
const ALERTS: AlertsSettings = {
  behind_commits: 20,
  pending_job_minutes: 5,
  review_wait_hours: 4,
  doing_idle_days: 3,
  waiting_session_hours: 2,
  your_turn_minutes: 15,
  argo_out_of_sync_minutes: 15,
  meeting_lead_minutes: 5,
  worktree_in_use_hours: 24,
  large_mr_lines: 400,
  no_reviewer_minutes: 10,
  open_threads: true,
  band_now: ['question', 'permission', 'review', 'branch-ci', 'infra'],
  band_later: ['your-turn', 'need-rebase', 'only-ci', 'no-ticket', 'agent-cost', 'worktree'],
  band_hidden: [],
  escalate_hours: 24,
  group: true,
  digest: false,
  digest_times: ['09:00', '12:30', '17:30'],
  digest_urgent: ['infra', 'deploy', 'agent'],
  quiet_hours: '',
  focus_events: 'Focus',
}

export const conventions = (): Conventions => live.snapshot?.conventions ?? DEFAULTS
/** A snapshot from an older Prism (the phone's, a preview) lacks the keys added since: their defaults. */
export const alertSettings = (): AlertsSettings => (live.snapshot?.alerts ? { ...ALERTS, ...live.snapshot.alerts } : ALERTS)

/* ---------- Tickets ---------- */

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

export const ticketPrefix = () => conventions().ticket_prefix
/** Every prefix as a pattern: `[tracker] ticket_prefix`, then the Obsidian boards' (`[ABCLM]`). */
const ticketPatterns = (): string[] => conventions().ticket_patterns ?? (ticketPrefix() ? [escape(ticketPrefix())] : [])
/** No prefix at all: the team has no ticket identifiers. */
export const hasTickets = () => ticketPatterns().length > 0
/** An example identifier: `ABC-1234`. */
export const ticketExample = (n = 1234) => `${ticketPrefix() || 'ABC'}-${n}`

const compiled = new Map<string, RegExp>()
/**
 * Expression of a ticket identifier, compiled once per prefix: `build` receives the pattern
 * (`ABC-\d+`) and returns the source of the expression that uses it.
 */
export function ticketRegex(key: string, build: (ticket: string) => string, flags = 'i'): RegExp {
  const patterns = ticketPatterns()
  const id = `${key}|${patterns.join('|')}|${flags}`
  let re = compiled.get(id)
  if (!re) {
    // Without a prefix, the identifier never matches; the rest of the expression still does
    // (Markdown's bold, code and links, linkify's MRs and URLs). So does a board's pattern that
    // this engine refuses (the app checks them with Rust's).
    const ticket = patterns.length ? `(?:${patterns.join('|')})-\\d+` : '(?!)'
    try {
      re = new RegExp(build(ticket), flags)
    } catch {
      re = new RegExp(build(patterns.length && ticketPrefix() ? `${escape(ticketPrefix())}-\\d+` : '(?!)'), flags)
    }
    compiled.set(id, re)
  }
  return re
}
/** `ABC-1234` (case-insensitive): the normalized identifier, otherwise null. */
export const asTicket = (text: string) => (ticketRegex('whole', (t) => `^${t}$`).test(text.trim()) ? text.trim().toUpperCase() : null)

/**
 * Every ticket an MR names in its title or branch (`tickets`), its first one (`ticket`) in a
 * snapshot that predates them: an MR may close several tickets.
 */
export const mrTickets = (m: { ticket: string | null; tickets?: string[] }): string[] => m.tickets ?? (m.ticket ? [m.ticket] : [])
/** The MR names that ticket, among others maybe. */
export const namesTicket = (m: { ticket: string | null; tickets?: string[] }, id: string) => mrTickets(m).includes(id)

/** Finished ticket: closed, or in an end status (`[clickup] done_statuses`). */
export const isDone = (t: Ticket) => t.closed || conventions().done_statuses.includes(t.status)

/* ---------- Branches and GitLab ---------- */

export const isRelease = (branch: string) => !!conventions().release_prefix && branch.startsWith(conventions().release_prefix)
/** Protected branch: the main one, or a release branch. */
export const isProtected = (branch: string) => branch === conventions().main_branch || isRelease(branch)
/** Link to the GitLab instance. */
export const gitlabUrl = (path: string) => `https://${conventions().gitlab_host}/${path.replace(/^\//, '')}`
