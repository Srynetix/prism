/**
 * A turn of an agent's conversation summed up, for the Chat tab's list of turns: the files it
 * edited (lines added and removed), the commands it ran and how many failed, how long it took, and
 * its last words. And the turns a search finds, in your prompts, its words and what its tools named.
 */

import type { ConversationEntry, ConversationTurn } from '../../agents.svelte'
import { baseName } from '../../platform'

export interface TurnSummary {
  files: { path: string; added: number; removed: number }[]
  commands: number
  failed: number
  /** From its prompt to its last entry. */
  ms: number | null
  /** Its last words, their first line. */
  last: string | null
}

export function summarize(turn: ConversationTurn): TurnSummary {
  const files = new Map<string, { path: string; added: number; removed: number }>()
  let commands = 0
  let failed = 0
  let last: string | null = null
  for (const e of turn.entries) {
    if (e.type === 'text') last = e.text
    if (e.type !== 'tool') continue
    if (e.state === 'failed') failed++
    if (e.kind === 'command') commands++
    if (e.kind === 'write' && e.what) {
      const f = files.get(e.what) ?? { path: e.what, added: 0, removed: 0 }
      f.added += e.added
      f.removed += e.removed
      files.set(e.what, f)
    }
  }
  const ms = turn.at && turn.last ? Math.max(0, new Date(turn.last).getTime() - new Date(turn.at).getTime()) : null
  // Its Markdown's marks off: a heading's, a list's, bold, code.
  const line = last?.split('\n').map((l) => l.replace(/^[#>*\-\s]+/, '').replace(/\*\*|__|`/g, '').trim()).find(Boolean) ?? null
  return { files: [...files.values()], commands, failed, ms, last: line }
}

/**
 * Whether a search's words are all in the turn (any case); its reminders' words only when they are
 * shown where they came (`reminders`), never found where nothing shows them.
 */
export function matches(turn: ConversationTurn, query: string, reminders = false): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (!words.length) return true
  const said = (e: ConversationEntry) => (e.type === 'tool' ? `${e.tool} ${e.what ?? ''}${reminders ? `\n${(e.reminders ?? []).join('\n')}` : ''}` : e.type === 'reminder' && !reminders ? '' : e.text)
  const text = [turn.prompt, ...turn.entries.map(said)].join('\n').toLowerCase()
  return words.every((w) => text.includes(w))
}

/**
 * The images a prompt mentions (`@path`, `@"path"`, as Prism types its attachments), and its words
 * without them: the Chat shows those images in its bubble.
 */
export function promptImages(prompt: string): { text: string; images: string[] } {
  const images: string[] = []
  // The character before is looked at, not taken: the space a path ends with starts the next one.
  const text = prompt.replace(/(?<=^|[\s(])@(?:"([^"\n]+)"|([^\s"]+))( ?)/g, (all, quoted?: string, bare?: string, space = '') => {
    // A bare path's trailing punctuation is the sentence's.
    const trail = bare?.match(/[.,;:!?)\]]+$/)?.[0] ?? ''
    const path = quoted ?? bare?.slice(0, bare.length - trail.length) ?? ''
    if (!/\.(png|jpe?g|gif|webp)$/i.test(path)) return all
    images.push(path)
    return trail + (trail && space)
  })
  return { text: images.length ? text.trim() : prompt, images }
}

/** A path's file name. */
export const base = (path: string) => baseName(path) || path

/**
 * A turn's entries cut where its Chat folds them: the runs of steps (its calls and thoughts)
 * between its words, its notes, its questions and its subagents, each run one segment; the rest
 * one by one.
 */
export type Segment = { type: 'steps'; from: number; entries: ConversationEntry[] } | { type: 'one'; at: number; entry: ConversationEntry }

/**
 * A call proposing a change of Prism (`prism_propose_*`) that has a card: waiting for you, or
 * answered (applied, refused, edited). One whose proposal was invalid, or that handed its wait on
 * (`waiting`: `prism_wait_proposal` follows), stays a step.
 */
export const proposalCard = (e: ConversationEntry) => e.type === 'tool' && !!e.propose && (e.state === 'running' || /"status":"(applied|refused|applied_modified)"/.test(e.output ?? ''))

/** Prism's « in passing » tools (`aside`) and its proposals have a card of their own: never folded. */
const isStep = (e: ConversationEntry) => e.type === 'thinking' || (e.type === 'tool' && !e.ask && !e.proposal && !e.aside && !proposalCard(e) && e.kind !== 'agent')

/**
 * A turn's entries as runs of steps and entries of their own. A reminder shown where it came (a
 * hook's, a file changed) goes in the run under way, not counted, rather than cutting it in two.
 */
export function segments(entries: ConversationEntry[]): Segment[] {
  const out: Segment[] = []
  entries.forEach((e, i) => {
    const last = out[out.length - 1]
    if (e.type === 'reminder' && last?.type === 'steps') last.entries.push(e)
    else if (!isStep(e)) out.push({ type: 'one', at: i, entry: e })
    else if (last?.type === 'steps') last.entries.push(e)
    else out.push({ type: 'steps', from: i, entries: [e] })
  })
  return out
}

/** A run of steps summed up, for its folded line. */
export interface StepsSummary {
  /** Its calls and thoughts. */
  steps: number
  read: number
  /** Files written, each once, and their lines. */
  edited: number
  added: number
  removed: number
  commands: number
  /** Its web calls and other tools. */
  others: number
  failed: number
}

export function sumSteps(entries: ConversationEntry[]): StepsSummary {
  const s: StepsSummary = { steps: entries.filter((e) => e.type !== 'reminder').length, read: 0, edited: 0, added: 0, removed: 0, commands: 0, others: 0, failed: 0 }
  const files = new Set<string>()
  for (const e of entries) {
    if (e.type !== 'tool') continue
    if (e.state === 'failed') s.failed++
    if (e.kind === 'read') s.read++
    else if (e.kind === 'command') s.commands++
    else if (e.kind === 'write') {
      files.add(e.what ?? `${files.size}`)
      s.added += e.added
      s.removed += e.removed
    } else s.others++
  }
  s.edited = files.size
  return s
}
