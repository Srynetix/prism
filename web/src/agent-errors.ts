import { invoke } from './bridge'
import { t } from './i18n.svelte'
import { dateFormat } from './locale'
import { planSections } from './usage'
import { toastError } from './toast.svelte'
import type { Agent, AgentFailure, Snapshot, UsageWindow } from './types'

/**
 * An agent stopped by an API error (status `failed`, Claude Code's `StopFailure`): what its kind
 * means and what to do (the Chat's banner, the phone's, the alert), and its resume: now, at the
 * usage limit's reset, or on its own for a passing error (`[agents] retry_on_error`, failures.rs).
 */
export type ErrorAction = 'resume' | 'resume-at' | 'login' | 'account' | 'compact' | 'model'

export interface ErrorInfo {
  /** In a few words, for the card and the list. */
  short: string
  /** What it is and what to do, for the banner. */
  message: string
  action: ErrorAction
}

export function errorInfo(kind: string): ErrorInfo {
  switch (kind) {
    case 'overloaded':
    case 'server_error':
      return { short: t('Service overloaded'), message: t('The service is overloaded or down: it usually passes within minutes.'), action: 'resume' }
    case 'rate_limit':
      return { short: t('Usage limit'), message: t('Usage limit reached: it goes on once the plan resets.'), action: 'resume-at' }
    case 'authentication_failed':
    case 'cloud_credential_error':
    case 'verification_required':
      return { short: t('Login expired'), message: t('The CLI’s login has expired: log in again in its terminal (/login), then resume.'), action: 'login' }
    case 'billing_error':
    case 'account_on_hold':
    case 'oauth_org_not_allowed':
      return { short: t('Account or billing'), message: t('An account or billing matter: nothing to resume until it is settled in the provider’s console.'), action: 'account' }
    case 'invalid_request':
    case 'max_output_tokens':
      return { short: t('Request refused'), message: t('The request was refused, or the answer was too long: compact the conversation then resume, or put it another way.'), action: 'compact' }
    case 'model_not_found':
      return { short: t('Model unavailable'), message: t('This model is not available: pick another one, then resume.'), action: 'model' }
    default:
      return { short: t('API error'), message: t('An API error ended its turn.'), action: 'resume' }
  }
}

/** The kinds a resume may get past (a usage limit, at its reset). */
export const resumable = (kind: string) => ['resume', 'resume-at', 'compact'].includes(errorInfo(kind).action)

/** `14:05`: a time today, with its day otherwise. */
export function atLabel(at: string | Date): string {
  const d = new Date(at)
  const today = d.toDateString() === new Date().toDateString()
  return dateFormat(today ? { hour: '2-digit', minute: '2-digit' } : { weekday: 'short', hour: '2-digit', minute: '2-digit' }).format(d)
}

/**
 * When the agent's plan comes back (a usage limit): the reset of its fullest window, among its
 * CLI's (and account's) plan as Prism reads it; null when unknown.
 */
export function limitResetOf(s: Snapshot | null | undefined, a: Agent): Date | null {
  if (!s) return null
  const section = planSections(s).find((p) => p.provider === a.provider && p.account === (a.account ?? null))
  const windows = [section?.usage?.five_hour, section?.usage?.seven_day, ...(section?.usage?.others ?? []).map(([, w]) => w)].filter((w): w is UsageWindow => !!w?.resets_at)
  if (!windows.length) return null
  const fullest = windows.reduce((a, b) => (b.utilization > a.utilization ? b : a))
  const at = new Date(fullest.resets_at!)
  // A minute past the reset, so the plan has room again.
  return at.getTime() > Date.now() ? new Date(at.getTime() + 60_000) : null
}

/** Resumed now (its resume prompt), or at `at`. */
export async function resumeAgent(id: string, at?: Date | null) {
  try {
    await invoke('agent_error_resume', { id, at: at ? at.toISOString() : null })
  } catch (e) {
    toastError(t('Not resumed: {error}', { error: String(e) }))
  }
}

/** `/compact`, then its resume prompt once compacted (a refused request, an answer too long). */
export async function compactThenResume(id: string) {
  try {
    await invoke('agent_error_compact', { id })
  } catch (e) {
    toastError(t('Not resumed: {error}', { error: String(e) }))
  }
}

/** The resume planned, cancelled (the automatic ones too, for this error). */
export async function cancelResume(id: string) {
  try {
    await invoke('agent_error_resume_cancel', { id })
  } catch (e) {
    toastError(String(e))
  }
}

/** The failure an agent shows, while it is in error. */
export const failureOf = (a: Pick<Agent, 'status' | 'failure'>): AgentFailure | null => (a.status === 'failed' ? (a.failure ?? null) : null)
