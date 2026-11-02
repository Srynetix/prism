import { pref, setPref } from './layout.svelte'
import { carry, type Passage, type TextComment } from './comments'

/**
 * Comments on an agent's texts (comments.ts), per agent and per text: `plan` (its plans, one
 * version after the other) or `answer:<hash>` (a long answer). Drafts until sent; kept with the
 * layout, shared with the phone (`agents.comments`, a shared pref), so a comment written on one
 * is sent from the other.
 */
interface Drafts {
  comments: TextComment[]
  /** Your words on the whole text. */
  general: string
}

const KEY = 'agents.comments'
const all = $state<Record<string, Record<string, Drafts>>>(pref<Record<string, Record<string, Drafts>>>(KEY, {}))
const save = () => setPref(KEY, $state.snapshot(all))
const of = (id: string, target: string): Drafts => ((all[id] ??= {})[target] ??= { comments: [], general: '' })

/** A long answer's target: its text, by a short fingerprint. */
export function answerTarget(text: string) {
  let h = 5381
  for (let i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) | 0
  return `answer:${(h >>> 0).toString(36)}`
}

export const commentsOf = (id: string, target: string): TextComment[] => all[id]?.[target]?.comments ?? []
export const draftsOf = (id: string, target: string) => commentsOf(id, target).filter((c) => c.status === 'draft')
export const generalOf = (id: string, target: string) => all[id]?.[target]?.general ?? ''

/** On one passage, or on several in a row (`end`, the last one's). */
export function addComment(id: string, target: string, quote: string, body: string, end?: string) {
  of(id, target).comments.push({ id: crypto.randomUUID(), quote, ...(end && end !== quote && { end }), body, status: 'draft' })
  save()
}
export function editComment(id: string, target: string, comment: string, body: string) {
  const c = of(id, target).comments.find((x) => x.id === comment)
  if (c) c.body = body
  save()
}
export function removeComment(id: string, target: string, comment: string) {
  const d = of(id, target)
  d.comments = d.comments.filter((x) => x.id !== comment)
  save()
}
export function setGeneral(id: string, target: string, text: string) {
  of(id, target).general = text
  save()
}

/** Sent (with the plan `call` answered): the drafts are sent, the general words gone. */
export function markSent(id: string, target: string, call?: string) {
  const d = of(id, target)
  d.comments = d.comments.map((c) => (c.status === 'draft' ? { ...c, status: 'sent', call } : c))
  d.general = ''
  save()
}

/** A new version of its plan (`call`): what was sent carried over (comments.ts's `carry`). */
export function carryTo(id: string, call: string, next: Passage[]) {
  const d = all[id]?.plan
  if (!d) return
  const carried = carry(d.comments, next, call)
  if (carried.some((c, i) => c !== d.comments[i])) {
    d.comments = carried
    save()
  }
}

/** Its comments forgotten: the plan approved, or the addressed ones put away. */
export function forget(id: string, target: string, only?: TextComment['status']) {
  const d = all[id]?.[target]
  if (!d) return
  d.comments = only ? d.comments.filter((c) => c.status !== only) : []
  if (!only) d.general = ''
  save()
}
