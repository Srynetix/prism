/**
 * GitLab job log (or a terminal's history) → lines of colored segments, displayed by Svelte
 * elements (no injected HTML). Handles SGR colors (the 16, 256 and true colors; backgrounds left
 * out), GitLab section markers, line erasing
 * and progress bar carriage returns (only the last state remains).
 */

export interface Seg {
  text: string
  /** Color class: `c31`…`c37`, `c90`…`c97`. */
  fg?: string
  /** Any other color (256 or true color), as CSS. */
  color?: string
  bold?: boolean
}

export interface Line {
  segs: Seg[]
  /** Title of a GitLab section (`step_script`, `prepare_executor`…) starting here. */
  section?: string
}

const SECTION = /section_(start|end):\d+:([\w.-]+)(\[[^\]]*\])?\r?(\x1b\[0K)?/g
const SGR = /\x1b\[([\d;]*)m/g
/** Other sequences (erasing, cursor): not rendered here. */
const OTHER = /\x1b\[[\d;?]*[A-Za-z]/g

/** One of the 256 colors: the first 16 as the classes, then the 6×6×6 cube and the grays. */
function color256(n: number): { fg?: string; color?: string } {
  if (n < 8) return { fg: `c${30 + n}` }
  if (n < 16) return { fg: `c${90 + n - 8}` }
  if (n < 232) {
    const level = (v: number) => (v ? 55 + v * 40 : 0)
    const i = n - 16
    return { color: `rgb(${level(Math.floor(i / 36))}, ${level(Math.floor(i / 6) % 6)}, ${level(i % 6)})` }
  }
  const g = 8 + (n - 232) * 10
  return { color: `rgb(${g}, ${g}, ${g})` }
}

/** The colors in force at a line's start: the SGR state carries from one line to the next. */
interface State {
  fg?: string
  color?: string
  bold: boolean
}

/** One line of the log, from `state`; `null` when there is nothing to show (a lone section marker). */
function parseLine(raw: string, state: State): { line: Line | null; state: State } {
  let { fg, color, bold } = state
  let section: string | undefined
  raw = raw.replace(SECTION, (_m, kind: string, name: string) => {
    if (kind === 'start') section = name
    return ''
  })
  // Progress bar: the displayed line is the last rewrite.
  const cr = raw.replace(/\r+$/, '').lastIndexOf('\r')
  if (cr !== -1) raw = raw.slice(cr + 1)
  raw = raw.replace(/\r/g, '')

  const segs: Seg[] = []
  let last = 0
  const push = (text: string) => {
    text = text.replace(OTHER, '')
    if (text) segs.push({ text, fg, bold, ...(color ? { color } : {}) })
  }
  for (const m of raw.matchAll(SGR)) {
    push(raw.slice(last, m.index))
    last = m.index! + m[0].length
    const codes = (m[1] || '0').split(';').map(Number)
    for (let i = 0; i < codes.length; i++) {
      const code = codes[i]
      if (code === 0) [fg, color, bold] = [undefined, undefined, false]
      else if (code === 1) bold = true
      else if (code === 22) bold = false
      else if (code === 39) [fg, color] = [undefined, undefined]
      else if ((code >= 30 && code <= 37) || (code >= 90 && code <= 97)) [fg, color] = [`c${code}`, undefined]
      // Extended: `38;5;n` (256 colors), `38;2;r;g;b` (true color); a background's, skipped.
      else if (code === 38 || code === 48) {
        const mode = codes[i + 1]
        const args = mode === 5 ? codes.slice(i + 2, i + 3) : mode === 2 ? codes.slice(i + 2, i + 5) : []
        i += 1 + args.length
        if (code === 48) continue
        if (mode === 5 && args.length === 1) ({ fg, color } = color256(args[0]))
        else if (mode === 2 && args.length === 3) [fg, color] = [undefined, `rgb(${args.join(', ')})`]
      }
    }
  }
  push(raw.slice(last))
  const next = { fg, color, bold }
  // A line emptied by a lone section marker has nothing to show.
  if (!segs.length && section === undefined && raw !== '' && !raw.replace(OTHER, '').trim()) return { line: null, state: next }
  return { line: { segs, section }, state: next }
}

type Each = (raw: string, state: State) => { line: Line | null; state: State }

/** `text`'s lines (split on `\n`, each piece one) after `state`: those shown, and the state after them. */
function run(text: string, state: State, each: Each, out: Line[] = []): { lines: Line[]; state: State } {
  for (const raw of text.split('\n')) {
    const r = each(raw, state)
    state = r.state
    if (r.line) out.push(r.line)
  }
  return { lines: out, state }
}

/** Without the empty lines at the end. */
function trimmed(lines: Line[]): Line[] {
  let n = lines.length
  while (n && !lines[n - 1].segs.length) n--
  return n === lines.length ? lines : lines.slice(0, n)
}

/** `log` line by line; `each` gives a line from its text and the state before it (`LogLines` keeps them). */
function parseWith(log: string, each: Each): Line[] {
  return trimmed(run(log, { bold: false }, each).lines)
}

export function parse(log: string): Line[] {
  return parseWith(log, parseLine)
}

/**
 * A log read again and again as it grows or slides (a job's, a pod's tail): a line read again — the
 * same text, after the same colors — is the same object as before, so its row is not rendered
 * again, and `key(i)` follows it when the log slides (its first lines dropped), where an index
 * would have made every row another one.
 */
export class LogLines {
  private known = new Map<string, { line: Line | null; state: State }>()
  private prev: Line[] = []
  /** How far the log has slid since the first read: what a line's key adds to its index. */
  private base = 0
  /** The last read up to its last line break: its text, its lines and the colors after them. */
  private whole: { text: string; lines: Line[]; state: State } | null = null
  /** The log read last, which `grown` says the next one comes from. */
  private last: string | null = null

  /**
   * `grown`: `log` made from `from` (the log read before), its first `same` characters the same —
   * what the caller knows, which comparing 50 MB each time (25 ms) would only find again.
   */
  parse(log: string, grown?: { from: string; same: number } | null): Line[] {
    const cut = log.lastIndexOf('\n') + 1
    const w = this.whole
    const kept = !!w && !!grown && grown.from === this.last && grown.same >= w.text.length
    this.last = log
    let head: { lines: Line[]; state: State }
    if (w && kept && cut > w.text.length) {
      // A log that only grew (read back to the container's start, then polled): the lines after
      // those read before, alone — 200,000 lines read again were 400 ms a poll.
      head = run(log.slice(w.text.length, cut - 1), w.state, parseLine, w.lines.slice())
      this.known.clear()
    } else if (w && kept && cut === w.text.length) {
      head = w
    } else {
      const used = new Map<string, { line: Line | null; state: State }>()
      head = run(cut ? log.slice(0, cut - 1) : '', { bold: false }, (raw, state) => {
        const k = `${state.fg ?? ''}|${state.color ?? ''}|${state.bold ? 1 : 0}|${raw}`
        const r = this.known.get(k) ?? used.get(k) ?? parseLine(raw, state)
        used.set(k, r)
        return r
      })
      if (!cut) head = { lines: [], state: { bold: false } }
      // Only the lines of this read are kept: what the log dropped goes too.
      this.known = used
    }
    this.whole = { text: log.slice(0, cut), lines: head.lines, state: head.state }
    // The last line, maybe still being written: read each time, never kept.
    const rest = log.slice(cut)
    const next = trimmed(rest ? run(rest, head.state, parseLine, head.lines.slice()).lines : head === w ? head.lines.slice() : head.lines)
    this.base += slid(this.prev, next)
    this.prev = next
    return next
  }

  /** A line's key: its index, plus how far the log slid. */
  key(i: number): number {
    return this.base + i
  }
}

/**
 * How many of `prev`'s first lines `next` dropped: where `next` starts again in it (its first lines
 * checked); all, when nowhere. Negative when `next` added lines above (a log read further back).
 */
function slid(prev: Line[], next: Line[]): number {
  if (!prev.length || !next.length || prev[0] === next[0]) return 0
  const above = next.length > prev.length ? next.length - prev.length : 0
  if (above && next[above] === prev[0] && next[above + 1] === prev[1] && next[above + 2] === prev[2]) return -above
  const check = Math.min(3, next.length)
  for (let k = 1; k < prev.length; k++) {
    let same = true
    for (let j = 0; j < check && k + j < prev.length; j++) if (prev[k + j] !== next[j]) same = false
    if (same) return k
  }
  return prev.length
}
