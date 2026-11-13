/**
 * Rendering of ClickUp Markdown (ticket descriptions) into a data structure, displayed by
 * Svelte elements: never injected HTML, a ticket's content cannot execute anything
 * in the window. Covers what the ClickUp editor produces: headings, (check)lists,
 * quotes, code blocks, tables, bold, italic, strikethrough, code, links and images (as links).
 * In-app Help, three more: callouts (a quote opening with `[!TIP]`, `[!NOTE]` or `[!WARNING]`,
 * as on GitHub), `prism:` links, which act in the app instead of leaving it, and images by a
 * relative path (`images/pages.png`), shipped with the app.
 * On the GitLab side (MR descriptions), collapsible `<details><summary>…</summary>…</details>` blocks,
 * nested or not, whose content is read like the rest.
 */

import type { MrOption } from './linkify'
import { ticketRegex } from './conventions'
import { t } from './i18n.svelte'

import type { IconName } from './icons'

export interface Span {
  text: string
  bold?: boolean
  italic?: boolean
  strike?: boolean
  code?: boolean
  /** http(s) link only. */
  href?: string
  /** `ABC-1234`: opens this ticket's preview. */
  ticket?: string
  /** `!1957` (`#1957` on GitHub) in an agent's words: the MRs it may name (linkify's `withMrs`), opened on a click. */
  mr?: MrOption[]
  /** Image (http(s)): displayed inline, `text` is its alt text. */
  image?: string
  /** `image` is a relative path (`images/x.png`): a file shipped with the app, see Markdown's `images`. */
  local?: boolean
  /** Requested width for the image (GitLab's `{width=900}`), in pixels: its maximum width. */
  width?: number
  /** Icon before the text (image that cannot be displayed). */
  icon?: IconName
  /** A GitLab custom emoji's picture (an https address), `text` its `:name:`; see emoji.svelte.ts. */
  emoji?: string
  /** `prism:` link: what to do in the app (`block/mrs`, `help/agents`…), see help.svelte.ts. */
  action?: string
  /** Someone mentioned (`@ann`, ClickUp's `@Ann Two`), drawn as a chip; `me`: I am. */
  mention?: 'me' | 'other'
}

/**
 * Who a comment's text mentions: a forge's `@username` (`me`: my username), or a tracker's known
 * tokens (`known`: `@Ann Two`, each told whether it is me), the only ones read then.
 */
export interface MentionMarks {
  me?: string | null
  known?: { text: string; mine: boolean }[]
}

/** A forge's `@username`: after a space or a bracket (not an address's `a@b`), not ending on a dot. */
const FORGE_MENTION = /(?<![\w@/.`-])@([A-Za-z0-9_](?:[\w.-]*[\w])?)/g
export const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** The mentions of plain spans cut out as chips; code, links and images are left as they are. */
export function withMentions(spans: Span[], marks: MentionMarks): Span[] {
  const known = marks.known?.filter((k) => k.text.length > 1).sort((a, b) => b.text.length - a.text.length)
  if (marks.known && !known?.length) return spans
  // A known token as a whole: not the start of a longer word (`@Ann` is not in `@Annabelle`).
  const re = known ? new RegExp(`(?:${known.map((k) => escapeRe(k.text)).join('|')})(?![\\p{L}\\p{N}_])`, 'gu') : FORGE_MENTION
  const me = marks.me?.toLowerCase()
  const out: Span[] = []
  for (const s of spans) {
    if (s.code || s.href || s.image || s.emoji || s.action || s.ticket || s.mr !== undefined || s.local) {
      out.push(s)
      continue
    }
    let last = 0
    for (const m of s.text.matchAll(re)) {
      if (m.index > last) out.push({ ...s, text: s.text.slice(last, m.index) })
      const mine = known ? !!known.find((k) => k.text === m[0])?.mine : !!me && m[1].toLowerCase() === me
      out.push({ ...s, text: m[0], mention: mine ? 'me' : 'other' })
      last = m.index + m[0].length
    }
    if (last === 0) out.push(s)
    else if (last < s.text.length) out.push({ ...s, text: s.text.slice(last) })
  }
  return out
}

export interface ListItem {
  spans: Span[]
  depth: number
  /** A numbered item's number: the author's (`3.`), or the one after its sibling's (`1.` `1.` `1.`). */
  number?: number
  /** Checkbox: checked or not; `null` for an ordinary item. */
  checked: boolean | null
  /** What the item holds under its line, indented: a code block, and the paragraphs after it. */
  blocks?: Block[]
}

export type Block = (
  | { kind: 'heading'; level: number; spans: Span[] }
  | { kind: 'paragraph'; spans: Span[] }
  | { kind: 'list'; ordered: boolean; items: ListItem[] }
  /** `blocks`: a body holding a code block, re-read as Markdown (`spans` then empty). */
  | { kind: 'quote'; spans: Span[]; blocks?: Block[] }
  | { kind: 'callout'; tone: 'tip' | 'note' | 'warning'; spans: Span[]; blocks?: Block[] }
  | { kind: 'code'; text: string }
  | { kind: 'table'; rows: Span[][][] }
  | { kind: 'rule' }
  | { kind: 'details'; summary: Span[]; open: boolean; blocks: Block[] }
) & {
  /** Source lines of the block, 1-based and inclusive: `[first, last]`. */
  lines?: [number, number]
}

/**
 * Marks of a Markdown file's preview in a diff (MrDiffView): each top-level block changed by the
 * diff, the lines removed between two blocks, and the block or removal lit from the diff.
 */
export interface MdMarks {
  /** `add`: a block only made of added lines; `mod`: one the diff touches. */
  of: (block: number) => 'add' | 'mod' | null
  /** Lines removed just before block `i` (`i` = number of blocks: at the end). */
  gone: Map<number, { count: number; text: string }>
  lit: MdTarget | null
  onhover: (target: MdTarget | null) => void
  onpick: (target: MdTarget) => void
}
export type MdTarget = { block: number } | { gone: number }

const HEADING = /^(#{1,6})\s+(.*)$/
const ITEM = /^(\s*)([-*+]|\d+[.)])\s+(?:\[([ xX])\]\s+)?(.*)$/
/** A code fence's opening: three backticks or tildes at least, then the language (no backtick after backticks). */
const FENCE = /^(\s*)(`{3,}(?=[^`]*$)|~{3,})(.*)$/
/** Whether `line` closes the fence `open`: the same character, as many at least, nothing after. */
const closes = (line: string, open: string) => {
  const m = /^\s*(`{3,}|~{3,})\s*$/.exec(line)
  return !!m && m[1][0] === open[0] && m[1].length >= open.length
}

/** The fenced code block opened at line `start` (`fence`: its FENCE match), up to its closing line (or the end). */
function fenced(lines: string[], start: number, fence: RegExpExecArray): { text: string; end: number } {
  const indent = fence[1].replace(/\t/g, '  ').length
  const body: string[] = []
  let i = start
  while (++i < lines.length && !closes(lines[i], fence[2])) body.push(lines[i])
  // Inside a list item, the code is indented as the fence: drop that indentation, no more.
  const text = body.map((l) => l.replace(/\t/g, '  ').replace(new RegExp(`^ {0,${indent}}`), '')).join('\n')
  return { text, end: i }
}
const RULE = /^\s*([-*_])(\s*\1){2,}\s*$/
const TABLE_SEP = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/
const DETAILS_OPEN = /<details(\s[^>]*)?>/gi
const DETAILS_CLOSE = /<\/details\s*>/gi

/**
 * `<details>` block starting at line `start`: up to its `</details>` (nested `<details>`
 * counted), its `<summary>`, and the rest re-read as Markdown.
 */
function details(lines: string[], start: number): { block: Block; end: number } {
  let depth = 0
  let end = start
  for (; end < lines.length; end++) {
    depth += (lines[end].match(DETAILS_OPEN)?.length ?? 0) - (lines[end].match(DETAILS_CLOSE)?.length ?? 0)
    if (depth <= 0) break
  }
  end = Math.min(end, lines.length - 1)
  const raw = lines.slice(start, end + 1).join('\n')
  const opening = /<details(\s[^>]*)?>/i.exec(raw)!
  const open = /\bopen\b/i.test(opening[1] ?? '')
  let body = raw.slice(opening.index + opening[0].length)
  const last = body.search(/<\/details\s*>\s*$/i)
  if (last >= 0) body = body.slice(0, last)
  let summary = t('Details')
  const sm = /^\s*<summary[^>]*>([\s\S]*?)<\/summary\s*>/i.exec(body)
  if (sm) {
    summary = sm[1]
    body = body.slice(sm[0].length)
  }
  return { block: { kind: 'details', summary: inline(plainHtml(summary).trim() || t('Details')), open, blocks: parse(body) }, end }
}

/** Simple HTML markup of a summary: `<code>` becomes code, the rest is removed. */
const plainHtml = (html: string) =>
  html
    .replace(/<code>([\s\S]*?)<\/code>/gi, '`$1`')
    .replace(/<(b|strong)>([\s\S]*?)<\/\1>/gi, '**$2**')
    .replace(/<(i|em)>([\s\S]*?)<\/\1>/gi, '*$2*')
    .replace(/<[^>]+>/g, '')
    .replace(/\s+/g, ' ')

export function parse(md: string): Block[] {
  const lines = md.replace(/\r\n?/g, '\n').split('\n')
  const out: Block[] = []
  /** A block, with its source lines (`from` and `to`: indices in `lines`). */
  const push = (b: Block, from: number, to: number) => out.push({ ...b, lines: [from + 1, Math.min(to, lines.length - 1) + 1] })
  let para: string[] = []
  let paraStart = 0
  const flush = () => {
    if (para.length) push({ kind: 'paragraph', spans: inline(para.join('\n')) }, paraStart, paraStart + para.length - 1)
    para = []
  }
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    const start = i
    const fence = FENCE.exec(line)
    if (fence) {
      flush()
      const f = fenced(lines, i, fence)
      push({ kind: 'code', text: f.text }, start, f.end)
      i = f.end
      continue
    }
    if (!line.trim()) {
      flush()
      continue
    }
    if (/^\s*<details(\s[^>]*)?>/i.test(line)) {
      flush()
      const d = details(lines, i)
      push(d.block, start, d.end)
      i = d.end
      continue
    }
    const heading = HEADING.exec(line)
    if (heading) {
      flush()
      push({ kind: 'heading', level: heading[1].length, spans: inline(heading[2].replace(/\s#+\s*$/, '')) }, start, start)
      continue
    }
    if (RULE.test(line)) {
      flush()
      push({ kind: 'rule' }, start, start)
      continue
    }
    if (line.trimStart().startsWith('>')) {
      flush()
      const body: string[] = []
      for (; i < lines.length && lines[i].trimStart().startsWith('>'); i++) body.push(lines[i].trimStart().replace(/^>\s?/, ''))
      i--
      const alert = /^\[!(TIP|NOTE|WARNING)\]\s*$/i.exec(body[0] ?? '')
      const text = (alert ? body.slice(1) : body).join('\n')
      // A code block in it: the body read as blocks, the code kept whole; otherwise one run of text.
      const content = text.split('\n').some((l) => FENCE.test(l)) ? { spans: [], blocks: parse(text) } : { spans: inline(text) }
      if (alert) push({ kind: 'callout', tone: alert[1].toLowerCase() as 'tip' | 'note' | 'warning', ...content }, start, i)
      else push({ kind: 'quote', ...content }, start, i)
      continue
    }
    if (line.includes('|') && TABLE_SEP.test(lines[i + 1] ?? '')) {
      flush()
      const rows = [cells(line)]
      for (i += 2; i < lines.length && lines[i].includes('|') && lines[i].trim(); i++) rows.push(cells(lines[i]))
      i--
      push({ kind: 'table', rows }, start, i)
      continue
    }
    const item = ITEM.exec(line)
    if (item) {
      flush()
      const ordered = /\d/.test(item[2])
      const items: ListItem[] = []
      /** The last number at each depth: `1.` `1.` `1.` count on, `3.` stays 3. */
      const last: number[] = []
      for (; i < lines.length; i++) {
        const f = items.length && /^\s+/.test(lines[i]) ? FENCE.exec(lines[i]) : null
        if (f) {
          // A code block indented under an item: in it, read whole (its lines are no items).
          const code = fenced(lines, i, f)
          ;(items[items.length - 1].blocks ??= []).push({ kind: 'code', text: code.text })
          i = code.end
          continue
        }
        const m = ITEM.exec(lines[i])
        // A numbered list following a bulleted list (or the reverse) is a separate one.
        if (m && !m[1] && /\d/.test(m[2]) !== ordered) break
        if (m) {
          const depth = Math.floor(m[1].replace(/\t/g, '  ').length / 2)
          last.length = depth + 1
          const written = /\d/.test(m[2]) ? parseInt(m[2], 10) : undefined
          const number = written === undefined ? undefined : last[depth] !== undefined && written <= last[depth] ? last[depth] + 1 : written
          if (number !== undefined) last[depth] = number
          items.push({ spans: inline(m[4]), depth, ...(number === undefined ? {} : { number }), checked: m[3] === undefined ? null : m[3] !== ' ' })
        } else if (lines[i].trim() && /^\s+/.test(lines[i]) && items.length) {
          // Continuation of an item on the next line, indented; after its code block, a paragraph under it.
          const it = items[items.length - 1]
          const after = it.blocks?.at(-1)
          if (!it.blocks) it.spans.push({ text: ' ' }, ...inline(lines[i].trim()))
          else if (after?.kind === 'paragraph' && lines[i - 1].trim()) after.spans.push({ text: ' ' }, ...inline(lines[i].trim()))
          else it.blocks.push({ kind: 'paragraph', spans: inline(lines[i].trim()) })
        } else if (!lines[i].trim() && items.length) {
          // A blank line between items (a loose list, as agents write them) or before an item's
          // indented paragraph: the list goes on; before anything else, it ends.
          const next = lines.slice(i + 1).find((l) => l.trim())
          const n = next === undefined ? null : ITEM.exec(next)
          const goesOn = next !== undefined && (n ? !!n[1] || /\d/.test(n[2]) === ordered : /^\s+/.test(next))
          if (!goesOn) break
        } else break
      }
      i--
      push({ kind: 'list', ordered, items }, start, i)
      continue
    }
    if (!para.length) paraStart = i
    para.push(line)
  }
  flush()
  return out
}

/** A row cut on its pipes, but not on an escaped one (`\|`, read as `|`, in code too) nor one in code. */
function cells(row: string): Span[][] {
  const s = row.trim().replace(/^\|/, '').replace(/(?<!\\)\|$/, '')
  const out: string[] = []
  let cell = ''
  for (let i = 0; i < s.length; i++) {
    if (s[i] === '\\' && s[i + 1] === '|') {
      cell += '|'
      i++
    } else if (s[i] === '`') {
      let n = 1
      while (s[i + n] === '`') n++
      const run = '`'.repeat(n)
      // The closing run of as many backticks, not part of a longer one; none: the backticks are text.
      let end = s.indexOf(run, i + n)
      while (end >= 0 && (s[end + n] === '`' || s[end - 1] === '`')) end = s.indexOf(run, end + 1)
      if (end < 0) {
        cell += run
        i += n - 1
      } else {
        cell += s.slice(i, end + n).replace(/\\\|/g, '|')
        i = end + n - 1
      }
    } else if (s[i] === '|') {
      out.push(cell)
      cell = ''
    } else cell += s[i]
  }
  out.push(cell)
  return out.map((c) => inline(c.trim()))
}

/**
 * Code, image, link, bold, strikethrough, italic, bare URL and ticket identifier, in this order of
 * priority at equal position. Bold, italic and strikethrough nest (their content is re-read). Code
 * between runs of as many backticks (``a`b``), the backticks inside it kept.
 */
const INLINE_BASE =
  /(?<!`)(`+)(?!`)(.+?)(?<!`)\1(?!`)|!\[([^\]]*)\]\(([^)\s]+)[^)]*\)(?:\{([^}\n]*)\})?|\[([^\]]+)\]\(([^)\s]+)[^)]*\)|\*\*(.+?)\*\*|__(.+?)__|~~(.+?)~~|(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?!\*)|(?<!\w)_(?!\s)(.+?)(?<!\s)_(?!\w)|(https?:\/\/[^\s<>"')\]]+)/
/** The same, with the ticket identifiers of the config's prefix. */
const inlineRe = () => ticketRegex('markdown', (ids) => `${INLINE_BASE.source}|\\b(${ids})\\b`, 'gi')

const safeHref = (url: string) => (/^https?:\/\//i.test(url) ? url : undefined)
/** File uploaded to a GitLab project (`/uploads/<secret>/…`): relative, resolved by the component. */
const uploadPath = (url: string) => (/^\/uploads\/[0-9a-f]{32}\/[^/]+$/.test(url) ? url : undefined)
/** Image by a relative path (`images/x.png`, `./images/x.png`): a file shipped with the app (Help). */
const localPath = (url: string) => (/^(\.\/)?[\w-]+(\/[\w.-]+)*\.(png|jpe?g|gif|svg|webp)$/i.test(url) ? url.replace(/^\.\//, '') : undefined)

/** Id of a heading, from its text: `Reviewing an MR` → `reviewing-an-mr`. */
export const slug = (spans: Span[]) =>
  spans
    .map((s) => s.text)
    .join('')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[^\w\s-]/g, '')
    .trim()
    .replace(/\s+/g, '-')

export function inline(text: string, style: Omit<Span, 'text'> = {}): Span[] {
  const out: Span[] = []
  let last = 0
  for (const m of text.matchAll(inlineRe())) {
    if (m.index! > last) out.push({ text: text.slice(last, m.index), ...style })
    const [whole, , code, alt, src, attrs, label, href, bold1, bold2, strike, it1, it2, url, ticket] = m
    // One space on each side lets the code start or end with a backtick (`` `a` ``): it is not the code's.
    if (code !== undefined) out.push({ text: /^ .*[^ ].* $/.test(code) ? code.slice(1, -1) : code, ...style, code: true })
    else if (src !== undefined) {
      const image = safeHref(src) ?? uploadPath(src)
      // GitLab image attributes (`{width=900 height=465}`): only the width is used, in pixels.
      const width = Number(/\bwidth\s*=\s*"?(\d+)(?:px)?"?(?![\d%])/i.exec(attrs ?? '')?.[1]) || undefined
      const local = image ? undefined : localPath(src)
      out.push(image ? { text: alt || 'image', image, width } : local ? { text: alt || 'image', image: local, local: true, width } : { text: alt || 'image', ...style, icon: 'image' })
    } else if (label !== undefined) {
      const link = safeHref(href) ?? uploadPath(href)
      const act = /^prism:([\w-]+(?:\/[\w-]+)?)$/.exec(href)?.[1]
      out.push(...inline(label, link ? { ...style, href: link } : act ? { ...style, action: act } : style))
    } else if (bold1 ?? bold2) out.push(...inline((bold1 ?? bold2)!, { ...style, bold: true }))
    else if (strike !== undefined) out.push(...inline(strike, { ...style, strike: true }))
    else if (it1 ?? it2) out.push(...inline((it1 ?? it2)!, { ...style, italic: true }))
    else if (url !== undefined) out.push({ text: url, ...style, href: url })
    else if (ticket !== undefined) out.push({ text: whole, ...style, ...(style.href ? {} : { ticket: ticket.toUpperCase() }) })
    last = m.index! + whole.length
  }
  if (last < text.length) out.push({ text: text.slice(last), ...style })
  // ClickUp escapes Markdown punctuation (`\_`, `\*`): render it as is.
  return out.map((s) => (s.code ? s : { ...s, text: s.text.replace(/\\([\\`*_{}[\]()#+\-.!|~>])/g, '$1') }))
}
