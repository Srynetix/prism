import { parse, type Block } from './markdown'

/**
 * Comments on an agent's text: its plan (plan mode's `ExitPlanMode`), or a long answer of its. The
 * text is cut into passages — a heading, a paragraph, each item of a list, a code block, a table —
 * each commented on, alone or with the ones after it; the comments go to the agent as one message, each passage quoted (it
 * sees no numbers nor colours). A plan's comments go back through its menu ("Tell Claude what to
 * change"), the others as a prompt.
 */
export interface Passage {
  /** Its place in the text, from 0. */
  at: number
  /** Its Markdown, as written: what a comment is anchored to. */
  source: string
  /** Rendered alone (a list's item keeps its number). */
  block: Block
}

export interface TextComment {
  id: string
  /** The passage's Markdown, as written when commented on. */
  quote: string
  /** On several passages in a row: the last one's (`quote` is the first). */
  end?: string
  body: string
  /** `sent` with the plan `call` answered; `addressed`: its passage changed in the next plan. */
  status: 'draft' | 'sent' | 'addressed'
  call?: string
}

const ITEM = /^(\s*)([-*+]|\d+[.)])\s+/

/** The text's passages, in order. */
export function passages(md: string): Passage[] {
  const lines = md.split('\n')
  const out: Passage[] = []
  const add = (source: string, block: Block) => out.push({ at: out.length, source: source.trim(), block })
  for (const b of parse(md)) {
    const [from, to] = b.lines ?? [1, lines.length]
    const own = lines.slice(from - 1, to)
    if (b.kind !== 'list') {
      add(own.join('\n'), b)
      continue
    }
    // Each item, with its continuation lines; a nested item is an item of its own.
    const starts = own.flatMap((l, i) => (ITEM.test(l) ? [i] : []))
    b.items.forEach((item, k) => {
      const src = own.slice(starts[k] ?? 0, starts[k + 1] ?? own.length).join('\n')
      add(src, { kind: 'list', ordered: b.ordered, items: [{ ...item, depth: 0 }] })
    })
  }
  return out
}

/** The passages a comment is on, as their first and last places; `null` when they are not in this text any more. */
export function spanOf(ps: Passage[], c: Pick<TextComment, 'quote' | 'end'>): [number, number] | null {
  const a = ps.findIndex((p) => p.source === c.quote)
  const b = c.end ? ps.findIndex((p, i) => i >= a && p.source === c.end) : a
  return a < 0 || b < 0 ? null : [a, b]
}

/** A passage as quoted to the agent: its words, without their Markdown marks, cut. */
export function quoteOf(source: string, max = 90): string {
  const words = source
    .replace(/^\s*(#{1,6}|[-*+]|\d+[.)])\s+/, '')
    .replace(/\*\*|__|`/g, '')
    .replace(/\s+/g, ' ')
    .trim()
  return words.length > max ? `${words.slice(0, max - 1).trimEnd()}…` : words
}

/**
 * The message the agent reads: each comment under its passage quoted, in the text's order, then
 * your general words. `revise`: a plan sent back, asked again.
 */
export function compose(what: 'plan' | 'answer', comments: Pick<TextComment, 'quote' | 'end' | 'body'>[], general: string, order: Passage[] = [], revise = false): string {
  const rank = (c: Pick<TextComment, 'quote'>) => {
    const i = order.findIndex((p) => p.source === c.quote)
    return i < 0 ? order.length : i
  }
  const list = [...comments].sort((a, b) => rank(a) - rank(b))
  const on = (c: Pick<TextComment, 'quote' | 'end'>) => (c.end ? `From “${quoteOf(c.quote, 60)}” to “${quoteOf(c.end, 60)}”` : `On “${quoteOf(c.quote)}”`)
  const parts = list.length ? [`Comments on your ${what}:`, ...list.map((c, i) => `${i + 1}. ${on(c)}:\n   ${c.body.trim().replace(/\n/g, '\n   ')}`)] : []
  if (general.trim()) parts.push(list.length ? `Also: ${general.trim()}` : general.trim())
  if (revise && parts.length) parts.push('Revise the plan.')
  return parts.join('\n\n')
}

/**
 * Comments carried to the next version of a plan (`call`): one sent whose passage reads the same
 * there was not taken into account — a draft again; one whose passage changed is addressed.
 */
export function carry(comments: TextComment[], next: Passage[], call: string): TextComment[] {
  const kept = new Set(next.map((p) => p.source))
  const same = (c: TextComment) => kept.has(c.quote) && (!c.end || kept.has(c.end))
  return comments.map((c) => (c.status !== 'sent' || c.call === call ? c : same(c) ? { ...c, status: 'draft', call: undefined } : { ...c, status: 'addressed', call }))
}

export type DiffLine = { kind: 'same' | 'add' | 'del'; text: string }

/** Line by line, what changed from `a` to `b` (their longest common lines kept). */
export function lineDiff(a: string, b: string): DiffLine[] {
  const x = a.split('\n')
  const y = b.split('\n')
  const n = x.length
  const m = y.length
  const lcs = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0))
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) lcs[i][j] = x[i] === y[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1])
  const out: DiffLine[] = []
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (x[i] === y[j]) {
      out.push({ kind: 'same', text: x[i] })
      i++
      j++
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) out.push({ kind: 'del', text: x[i++] })
    else out.push({ kind: 'add', text: y[j++] })
  }
  while (i < n) out.push({ kind: 'del', text: x[i++] })
  while (j < m) out.push({ kind: 'add', text: y[j++] })
  return out
}

/** A text long enough to be commented on (`[agents] comment_answers`): several passages, or long. */
export const commentable = (text: string) => text.length >= 600 || passages(text).length >= 4
