import { pref, setPref } from './layout.svelte'
import type { WebOrigin } from './bindings/WebOrigin'
import { IMPORTED } from './web-address'

/**
 * Remarks on a page of an agent's Web view, clicked on its elements (Comment): drafts kept per
 * agent and page until sent, then one prompt quoting each element. The page talks through Prism's
 * annotation script (`annotate.js`, added to every page Prism serves) by `postMessage`; what it
 * says is checked here field by field (`readPick`, `readFound`): the page is not trusted.
 */

/** A remark on an element: its selector, tag and text as the page said them. */
export interface WebNote {
  /** Its own, for its picture (two remarks on one element keep theirs). */
  id: string
  sel: string
  tag: string
  text: string
  note: string
}

/** An element the page says was clicked; `rect` in the frame's points. */
export interface Pick {
  sel: string
  tag: string
  text: string
  rect: { x: number; y: number; w: number; h: number }
}

/** A page's drafts: a remark on the whole page, and those on its elements (numbered from 1). */
export interface WebDraft {
  whole: string
  notes: WebNote[]
}

const KEY = 'agents.web.comments'
const all = $state<Record<string, WebDraft>>(pref<Record<string, WebDraft>>(KEY, {}))
const key = (agent: string, page: string) => `${agent}|${page}`
const save = () => setPref(KEY, $state.snapshot(all))

export const webDraft = (agent: string, page: string): WebDraft => all[key(agent, page)] ?? { whole: '', notes: [] }
export function setWebDraft(agent: string, page: string, draft: WebDraft) {
  const k = key(agent, page)
  if (draft.whole.trim() || draft.notes.length) all[k] = draft
  else delete all[k]
  save()
}
/** The pictures of the remarks' elements (`web_capture`, base64 PNG), by remark: in memory only. */
export const captures = new Map<string, string>()
export const noteId = () => Math.random().toString(36).slice(2, 10)

/** How the agent is told where the page is: an artifact by its name in its scratchpad, an imported page in its folder. */
export const pageName = (page: string) =>
  page.startsWith('@scratchpad/')
    ? `${page.slice('@scratchpad/'.length)} (the artifact in your session's scratchpad)`
    : page.startsWith(`${IMPORTED}/`)
      ? `.prism/web/${page.slice(IMPORTED.length + 1)} (imported into your folder)`
      : page

/** An agent closed: its drafts go. */
export function dropWebNotes(agent: string) {
  for (const k of Object.keys(all)) if (k.startsWith(`${agent}|`)) delete all[k]
  save()
}

const str = (v: unknown, max: number) => (typeof v === 'string' ? v.slice(0, max) : null)
const num = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : null)

/** A message of the annotation script: `{ prism: 'annotate', type }`; anything else, null. */
export function annotateType(data: unknown): string | null {
  if (!data || typeof data !== 'object') return null
  const d = data as Record<string, unknown>
  return d.prism === 'annotate' && typeof d.type === 'string' ? d.type : null
}

/** A pick as the page sent it, kept to what a remark needs, bounded; null when malformed. */
export function readPick(data: unknown): Pick | null {
  if (annotateType(data) !== 'pick') return null
  const d = data as Record<string, unknown>
  const r = (d.rect ?? {}) as Record<string, unknown>
  const sel = str(d.sel, 300)
  const tag = str(d.tag, 30)
  const [x, y, w, h] = [num(r.x), num(r.y), num(r.w), num(r.h)]
  if (!sel || !tag || !/^[a-z][a-z0-9-]*$/.test(tag) || x === null || y === null || w === null || h === null) return null
  return { sel, tag, text: (str(d.text, 80) ?? '').replace(/\s+/g, ' ').trim(), rect: { x, y, w: Math.max(0, w), h: Math.max(0, h) } }
}

/** Which remarks the page found again (its pins), by their number from 1; null when malformed. */
export function readFound(data: unknown): Set<number> | null {
  if (annotateType(data) !== 'found') return null
  const pins = (data as Record<string, unknown>).pins
  if (!Array.isArray(pins)) return null
  return new Set(pins.filter((p) => p && typeof p === 'object' && !(p as Record<string, unknown>).missing).map((p) => Number((p as Record<string, unknown>).n)).filter(Number.isInteger))
}

/** What the page gets to draw the pins: the remarks, numbered from 1 (`hl`: the one pointed at). */
export const pinsOf = (notes: WebNote[], hl: number | null) => notes.map((n, i) => ({ n: i + 1, sel: n.sel, tag: n.tag, text: n.text, hl: hl === i + 1 }))

/** How an element is named in the list and the prompt: `<button> « Relancer »`. */
export const elementLabel = (n: Pick | WebNote) => `<${n.tag}>${n.text ? ` “${n.text}”` : ''}`

/**
 * The prompt: the page, then each remark — the whole page's first, then each element with its
 * selector and, when captured, its picture (`@path`).
 */
export function webPrompt(page: string, draft: WebDraft, captures: Record<number, string> = {}, where?: string, origin?: WebOrigin | null): string {
  const whole = draft.whole.trim()
  const count = draft.notes.length + (whole ? 1 : 0)
  const lines = [`Remarks on the page ${where ?? pageName(page)} (${count} remark${count > 1 ? 's' : ''}, made on its elements in Prism's Web view). Address each, then say what you changed.`]
  if (whole) lines.push('', `- The whole page: ${whole.replace(/\n/g, '\n  ')}`)
  draft.notes.forEach((n, k) => {
    const i = k + 1
    lines.push('', `${i}. ${elementLabel(n)} — ${n.sel}`, `   ${n.note.trim().replace(/\n/g, '\n   ')}`)
    if (captures[i]) lines.push(`   ${/\s/.test(captures[i]) ? `@"${captures[i]}"` : `@${captures[i]}`}`)
  })
  if (origin?.block) lines.push('', `It is the HTML block of the Notion page ${origin.page ?? origin.url}: just edit the file, Prism puts the new version back on that block.`)
  else if (origin) lines.push('', origin.page ? `It was imported from ${origin.url}, attached to ${origin.page}: once done, put the new version back there, in place of that attachment (or tell me how).` : `It was imported from ${origin.url}: once done, say how to put the new version back there.`)
  return lines.join('\n')
}
