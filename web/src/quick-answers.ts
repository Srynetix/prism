import type { Agent } from './types'
import type { QuickAnswer } from './bindings/QuickAnswer'
import type { QuickAnswersConfig } from './bindings/QuickAnswersConfig'
import { mrKey } from './mr-ref'
import { hasCore, invoke, isDesktop } from './bridge'

/**
 * Quick answers (`[agents.quick_answers]`): one click sends an agent at its prompt a text. Its
 * own list, or Prism's in `[agents] prompt_language`; a text may name the agent's ticket, branch,
 * MR, title or folder (`{ticket}`…): an answer whose variable the agent lacks is not offered to it.
 */
/** The same as `QuickAnswersConfig::list` (Rust): a phone's answer is checked against it there. */
export const DEFAULT_ANSWERS: Record<'fr' | 'en', QuickAnswer[]> = {
  en: [
    { label: 'OK', text: 'OK', submit: true },
    { label: 'Do it', text: 'Go ahead, do it.', submit: true },
    { label: 'Continue', text: 'Continue.', submit: true },
    { label: 'No, wait', text: 'No, wait: ', submit: false },
    { label: 'Commit', text: 'Commit with a Conventional message.', submit: true },
  ],
  fr: [
    { label: 'OK', text: 'OK', submit: true },
    { label: 'Fais-le', text: 'Vas-y, fais-le.', submit: true },
    { label: 'Continue', text: 'Continue.', submit: true },
    { label: 'Non, attends', text: 'Non, attends : ', submit: false },
    { label: 'Commit', text: 'Commit avec un message Conventional.', submit: true },
  ],
}

/** At its prompt, free to read an answer: not working, not asking for a permission. */
export const atPrompt = (a: Agent) => a.state === 'running' && ['needs_you', 'review_ready', 'completed', 'idle', 'background'].includes(a.status)

/** A value a phone's answer may carry: a name, a ref, an id (the same as quick.rs's `plain`). */
const plain = (v: string | undefined) => (v && /^[A-Za-z0-9._/#!:@+-]{1,120}$/.test(v) ? v : undefined)

/**
 * The values an agent gives the variables; absent: it has none. From a phone only names: it
 * renames an agent freely, its words would go out as the config's; its title, words, never.
 */
function values(a: Agent): Record<string, string | undefined> {
  const folder = a.cwd.split('/').filter(Boolean).pop()
  const v = { ticket: a.ticket ?? undefined, branch: a.branch ?? undefined, mr: a.mr ? mrKey(a.mr) : undefined, title: a.title || undefined, folder }
  return onPhone() ? { ticket: plain(v.ticket), branch: plain(v.branch), mr: plain(v.mr), title: undefined, folder: plain(v.folder) } : v
}

/** `text` with the agent's values; null when it names one the agent lacks. An unknown `{name}` stays as is. */
export function fill(text: string, a: Agent): string | null {
  const v = values(a)
  let missing = false
  const out = text.replace(/\{(\w+)\}/g, (all, name: string) => {
    if (!(name in v)) return all
    const value = v[name]
    if (!value) missing = true
    return value ?? ''
  })
  return missing ? null : out
}

/** The answers offered to `a` now: none while it works or asks for a permission, or when off. */
export function answersFor(cfg: QuickAnswersConfig | undefined, language: string, a: Agent): (QuickAnswer & { sent: string })[] {
  if (cfg && !cfg.enabled) return []
  if (!atPrompt(a)) return []
  const list = cfg?.answers.length ? cfg.answers : (DEFAULT_ANSWERS[language as 'fr' | 'en'] ?? DEFAULT_ANSWERS.en)
  return list.filter((q) => q.label.trim() && q.text.trim()).flatMap((q) => {
    const sent = fill(q.text, a)
    return sent === null ? [] : [{ ...q, sent }]
  })
}

/**
 * From a phone: the Mac sends the answer `label`, filled there and checked against `sent` (what the
 * phone shows); no free text leaves the phone this way, so no passkey. Typed (`submit = false`) or
 * sent as the answer says.
 */
export const onPhone = () => hasCore() && !isDesktop()
export const sendFromPhone = (a: Agent, q: QuickAnswer & { sent: string }) => invoke<void>('agent_quick_answer', { id: a.id, label: q.label, text: q.sent })

/** The prompt the agent suggests at its prompt (Claude Code's dimmed text), when there is one. */
export const suggestionOf = (a: Agent) => (atPrompt(a) ? (a.suggestion?.trim() || null) : null)

/** Sends the agent its own suggestion: the Mac reads its screen again and sends it only if it is still this text. */
export const sendSuggestion = (a: Agent, text: string) => invoke<void>('agent_send_suggestion', { id: a.id, text })
