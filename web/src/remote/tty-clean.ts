/**
 * A terminal's text made readable on a phone: what the agents' CLIs draw around their words left
 * out — a frame's borders (the prompt box), the spinner's line (`✻ Whirring… (34s · esc to
 * interrupt)`), the status bar (`⏵⏵ accept edits on`, `? for shortcuts`) — and repeats collapsed.
 * For the history above the live terminal, and the Chat tab's few lines of the terminal.
 */

import type { Line, Seg } from '../ansi'

const BOX = /[─━═╭╮╰╯┌┐└┘├┤┬┴┼│┃╌┄┈╴╶▔▁]/
const ONLY_BOX = /^[\s─━═╭╮╰╯┌┐└┘├┤┬┴┼│┃╌┄┈╴╶▔▁-]*$/
/** A spinner while it works (Claude Code, Codex, OpenCode all offer Esc), or the status bar. */
const CHROME = /esc to interrupt|^\s*(⏵⏵|⏸|\? for shortcuts)|shift\+tab to cycle/i

/** A line that only draws: a frame's top or bottom, a rule. */
export const isFrame = (text: string) => BOX.test(text) && ONLY_BOX.test(text)
export const isChrome = (text: string) => isFrame(text) || CHROME.test(text)

/** A framed line's text, its side borders off (`│ > the prompt   │`). */
export function unframe(text: string): string {
  return text.replace(/^\s*[│┃] ?/, '').replace(/\s*[│┃]\s*$/, '')
}

/** Lines of text cleaned: chrome off, borders off, repeats and runs of blank lines collapsed. */
export function cleanText(lines: string[]): string[] {
  const out: string[] = []
  for (const raw of lines) {
    if (isChrome(raw)) continue
    const l = unframe(raw).trimEnd()
    const last = out[out.length - 1]
    if (l === last && (l !== '' || out.length === 0)) continue
    if (l === '' && last === '') continue
    out.push(l)
  }
  while (out[0] === '') out.shift()
  while (out[out.length - 1] === '') out.pop()
  return out
}

const textOf = (l: Line) => l.segs.map((s) => s.text).join('')

/** Coloured lines (ansi.ts) cleaned the same way, their colours kept. */
export function cleanLines(lines: Line[]): Line[] {
  const out: Line[] = []
  let last: string | null = null
  for (const l of lines) {
    const text = textOf(l)
    if (isChrome(text)) continue
    const segs = unframeSegs(l.segs)
    const t = segs.map((s) => s.text).join('').trimEnd()
    if (t === last && (t !== '' || out.length === 0)) continue
    if (t === '' && last === '') continue
    out.push({ ...l, segs: t === '' ? [] : segs })
    last = t
  }
  while (out.length && !textOf(out[0]).trim()) out.shift()
  while (out.length && !textOf(out[out.length - 1]).trim()) out.pop()
  return out
}

function unframeSegs(segs: Seg[]): Seg[] {
  const text = segs.map((s) => s.text).join('')
  const head = text.match(/^\s*[│┃] ?/)?.[0].length ?? 0
  const tail = text.slice(head).match(/\s*[│┃]\s*$/)?.[0].length ?? 0
  if (!head && !tail) return segs
  // The characters kept, [head, end), cut from the segments they fall in.
  const end = text.length - tail
  const out: Seg[] = []
  let at = 0
  for (const s of segs) {
    const from = Math.max(head, at)
    const to = Math.min(end, at + s.text.length)
    if (to > from) out.push({ ...s, text: s.text.slice(from - at, to - at) })
    at += s.text.length
  }
  return out
}
