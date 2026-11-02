import { invoke } from './bridge'
import { interruptAgent } from './agents.svelte'
import { ready } from './agent-inbox.svelte'
import { pref, setPref } from './layout.svelte'
import type { MrDiff } from './mr-review.svelte'

/**
 * Review of an agent's diff (its Diff tab): notes on lines or ranges, a draft until sent as one
 * prompt typed in its terminal. Kept per agent with the layout, so a draft survives Prism quitting.
 * Sent notes stay on their lines, "changed since" once the agent has touched them; resolved ones
 * fold, and are forgotten once the worktree commits (`HEAD` moves). Prism never edits the
 * worktree: even Revert is a note, the agent does the work.
 */
export interface ReviewNote {
  id: string
  path: string
  /** `new`: lines of the working tree; `old`: removed lines, numbered in the version compared to. */
  side: 'new' | 'old'
  from: number
  to: number
  /** The lines noted, as they were: quoted in the prompt, and compared to tell "changed since". */
  quote: string[]
  body: string
  status: 'draft' | 'sent' | 'resolved'
  sentAt?: string
  /** `HEAD` when the note was written: a resolved note goes once it moves. */
  head: string
}

/** What the diff view needs to show and edit the notes of an agent's review. */
export interface ReviewHooks {
  notes: (ReviewNote & { changed: boolean })[]
  add: (note: Pick<ReviewNote, 'path' | 'side' | 'from' | 'to' | 'quote' | 'body'>) => void
  edit: (note: string, body: string) => void
  remove: (note: string) => void
  resolve: (note: string) => void
}

interface AgentReview {
  notes: ReviewNote[]
  /** Each file's diff fingerprint when the notes were sent: "Since my notes" keeps what moved. */
  sentDiff?: Record<string, string>
  /** Send asked while a menu of the agent's was open: it goes once the menu is answered. */
  queued?: boolean
  /** Why the last sending failed (the drafts stay). */
  error?: string
}

const KEY = 'agents.review'
const all = $state<Record<string, AgentReview>>(pref<Record<string, AgentReview>>(KEY, {}))
const save = () => setPref(KEY, $state.snapshot(all))
const of = (id: string): AgentReview => (all[id] ??= { notes: [] })

export const notesOf = (id: string): ReviewNote[] => all[id]?.notes ?? []
export const drafts = (id: string) => notesOf(id).filter((n) => n.status === 'draft')
export const isQueued = (id: string) => !!all[id]?.queued
export const sendError = (id: string) => all[id]?.error ?? ''
export const sentDiffOf = (id: string) => all[id]?.sentDiff ?? null

export function addNote(id: string, note: Omit<ReviewNote, 'id' | 'status'>) {
  of(id).notes.push({ ...note, id: crypto.randomUUID(), status: 'draft' })
  save()
}
export function editNote(id: string, note: string, body: string) {
  const n = of(id).notes.find((x) => x.id === note)
  if (n) n.body = body
  save()
}
export function removeNote(id: string, note: string) {
  const r = of(id)
  r.notes = r.notes.filter((x) => x.id !== note)
  save()
}
export function resolveNote(id: string, note: string) {
  const n = of(id).notes.find((x) => x.id === note)
  if (n) n.status = 'resolved'
  save()
}

/** Each file's diff, by a short fingerprint. */
export function fingerprint(text: string) {
  let h = 5381
  for (let i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) | 0
  return (h >>> 0).toString(36)
}
const pathOf = (f: MrDiff['files'][number]) => f.new_path || f.old_path

/**
 * What a fresh diff says about the notes: resolved ones are forgotten once `HEAD` has moved
 * since they were written; for a sent one, whether its lines read differently now.
 */
export function syncWithDiff(id: string, diff: MrDiff & { head?: string }): Set<string> {
  const r = all[id]
  const changed = new Set<string>()
  if (!r) return changed
  if (diff.head) {
    const before = r.notes.length
    r.notes = r.notes.filter((n) => !(n.status === 'resolved' && n.head && n.head !== diff.head))
    if (r.notes.length !== before) save()
  }
  for (const n of r.notes) {
    if (n.status !== 'sent') continue
    const f = diff.files.find((x) => pathOf(x) === n.path)
    if (!f || !sameLines(f.diff, n)) changed.add(n.id)
  }
  return changed
}
/** The note's lines, read again from the file's diff: still there, and the same. */
function sameLines(diff: string, n: ReviewNote): boolean {
  const lines = new Map<number, string>()
  let o = 0
  let w = 0
  for (const raw of diff.split('\n')) {
    const h = /^@@ -(\d+)(?:,\d+)? \+(\d+)/.exec(raw)
    if (h) {
      o = Number(h[1])
      w = Number(h[2])
      continue
    }
    const c = raw[0]
    if (c === '+') {
      if (n.side === 'new') lines.set(w, raw.slice(1))
      w++
    } else if (c === '-') {
      if (n.side === 'old') lines.set(o, raw.slice(1))
      o++
    } else if (c === ' ') {
      lines.set(n.side === 'new' ? w : o, raw.slice(1))
      o++
      w++
    }
  }
  return n.quote.every((q, i) => lines.get(n.from + i) === q)
}

/** The prompt the agent reads: the drafts in diff order, each with its place and the code quoted. */
export function composePrompt(notes: ReviewNote[], order: string[]): string {
  const rank = (p: string) => (order.indexOf(p) + 1 || order.length + 1)
  const list = [...notes].sort((a, b) => rank(a.path) - rank(b.path) || a.from - b.from)
  const quote = (q: string[]) => (q.length <= 6 ? q : [...q.slice(0, 2), '…', ...q.slice(-2)]).map((l) => `   > ${l}`).join('\n')
  const where = (n: ReviewNote) => `${n.path}, ${n.from === n.to ? 'line' : 'lines'} ${n.from === n.to ? n.from : `${n.from}–${n.to}`}${n.side === 'old' ? ' (removed)' : ''}`
  const head = `Review of your changes (${list.length} note${list.length > 1 ? 's' : ''}). Address each, then summarize what you changed.`
  return [head, ...list.map((n, i) => `${i + 1}. ${where(n)}:\n${quote(n.quote)}\n   ${n.body.replace(/\n/g, '\n   ')}`)].join('\n\n')
}

/**
 * Sends the drafts: `insert` types them without Enter; otherwise submitted at once, as the inbox
 * does (`agent-inbox`'s `ready`): working, its CLI reads them once its turn is over; queued only
 * while a menu of its is on its screen (`interrupt`: its turn stopped first, then sent).
 */
export async function sendReview(id: string, how: 'send' | 'insert' | 'interrupt', diff: MrDiff) {
  const r = of(id)
  if (!r.notes.some((n) => n.status === 'draft')) return
  r.error = undefined
  if (how === 'interrupt') {
    await interruptAgent(id)
    await new Promise((res) => setTimeout(res, 700))
  } else if (how === 'send' && !ready(id)) {
    r.queued = true
    r.sentDiff = fingerprints(diff)
    save()
    return
  }
  await deliver(id, how !== 'insert', fingerprints(diff), diff.files.map(pathOf))
}
export function cancelQueue(id: string) {
  if (all[id]) all[id].queued = false
  save()
}
const fingerprints = (diff: MrDiff) => Object.fromEntries(diff.files.map((f) => [pathOf(f), fingerprint(f.diff)]))

async function deliver(id: string, enter: boolean, sentDiff: Record<string, string>, order: string[]) {
  const r = of(id)
  const notes = r.notes.filter((n) => n.status === 'draft')
  await invoke('agent_prompt', { id, text: composePrompt(notes, order), enter })
  const at = new Date().toISOString()
  for (const n of notes) {
    n.status = 'sent'
    n.sentAt = at
  }
  r.sentDiff = sentDiff
  r.queued = false
  r.error = undefined
  save()
}

// A queued review goes once its agent's menu is answered (the Diff tab may be closed by then).
$effect.root(() => {
  $effect(() => {
    for (const [id, r] of Object.entries(all)) {
      if (!r.queued || !ready(id)) continue
      r.queued = false
      const order = Object.keys(r.sentDiff ?? {})
      deliver(id, true, r.sentDiff ?? {}, order).catch((e) => {
        r.error = String(e)
        save()
      })
    }
  })
})
