/**
 * Emoji shortcodes of a forge's text (MR titles, descriptions, comments, Markdown files in a diff):
 * `:thumbsup:`, `:+1:`, `:slight_smile:` become their emoji, by GitLab's names and GitHub's
 * (emoji-shortcodes.ts). An unknown name stays as written. The table is large: it is only loaded
 * once a text holds something that looks like a shortcode, the text redrawn when it arrives.
 * On GitLab, a group's custom emoji too (`:party-parrot:`), read once per project, drawn as
 * their picture (CustomEmoji.svelte) in a text and a title (MrTitle.svelte), offered in the comment
 * box; a title as a string (an attribute, `emojiText`) keeps their name.
 */

import { hasCore, invoke } from './bridge'
import type { Span } from './markdown'

/** `:name:`, not glued to a word before it (`a:b:c` is no shortcode). */
const SHORTCODE = /(?<![\w])(:[a-z0-9_+-]+:)/g
const LOOKS = /(?<![\w]):[a-z0-9_+-]+:/

type Tables = { common: Map<string, string>; gitlab: Map<string, string>; github: Map<string, string> }

let tables = $state.raw<Tables | null>(null)
let loading = false

const read = (pairs: string) => new Map(pairs.split(' ').map((p) => [p.slice(0, p.indexOf(':')), p.slice(p.indexOf(':') + 1)] as const))

function load() {
  if (loading) return
  loading = true
  void import('./emoji-shortcodes').then((m) => (tables = { common: read(m.SHORTCODES), gitlab: read(m.GITLAB_ONLY), github: read(m.GITHUB_ONLY) }))
}

/** The project of a forge text (`group/project`), from its web address (Markdown's `forge`). */
export const projectOf = (web: string): string => {
  try {
    return new URL(web).pathname.replace(/^\/|\/$/g, '')
  } catch {
    return ''
  }
}

/** Custom emoji by project (`group/project`): name → picture address; asked once each. */
let customs = $state.raw<Record<string, Map<string, string>>>({})
const asked = new Set<string>()

/**
 * The custom emoji of a GitLab project's groups, null until they are read (the read then starts).
 * Asked once a session: on failure (an instance that has none to give), none.
 */
export function customOf(project: string | null): Map<string, string> | null {
  if (!project || !hasCore()) return null
  const got = customs[project]
  if (got || asked.has(project)) return got ?? null
  asked.add(project)
  const done = (list: { name: string; url: string }[]) => (customs = { ...customs, [project]: new Map(list.map((e) => [e.name, e.url])) })
  invoke<{ name: string; url: string }[]>('forge_custom_emoji', { project }).then(done, () => done([]))
  return null
}

/** `text` with its known shortcodes replaced, by `lookup` (name without colons → emoji). */
export function replaceShortcodes(text: string, lookup: (name: string) => string | undefined): string {
  return text.replace(SHORTCODE, (whole: string) => lookup(whole.slice(1, -1)) ?? whole)
}

/** The emoji of a shortcode's name, on GitHub's names when `github` (only a few differ), GitLab's otherwise; null while the table loads (the load then starts). */
function lookupOn(github: boolean): ((name: string) => string | undefined) | null {
  const t = tables
  if (!t) {
    load()
    return null
  }
  const own = github ? t.github : t.gitlab
  return (name) => own.get(name) ?? t.common.get(name)
}

/**
 * The emoji whose name holds `query` (a comment's `:thu`), each once by its best name: names that
 * start with it first, shorter first. Null while the table loads (the load then starts).
 */
export type EmojiMatch = { name: string; emoji: string; custom?: boolean }

/**
 * The emoji whose name holds `query` (a comment's `:thu`), each once by its best name: names that
 * start with it first, shorter first, a group's custom ones (`custom`: name → picture, `emoji`
 * then the picture) ahead of the standard ones of the same rank. Null while the table loads (the
 * load then starts).
 */
export function emojiMatches(query: string, github: boolean, limit = 40, custom?: Map<string, string> | null): EmojiMatch[] | null {
  const lookup = lookupOn(github)
  const t = tables
  if (!lookup || !t) return null
  const q = query.toLowerCase()
  // At the start of the name or of one of its words (`smile` finds `cat_smile`, `shipit` `big-shipit`).
  const rank = (name: string) => {
    const at = name.indexOf(q)
    return at === 0 ? 0 : at > 0 && /[_-]/.test(name[at - 1]) ? 1 : null
  }
  const found: (EmojiMatch & { rank: number })[] = []
  for (const [name, url] of custom ?? []) {
    const r = rank(name)
    if (r !== null && !lookup(name)) found.push({ name, emoji: url, custom: true, rank: r })
  }
  for (const name of new Set([...t.common.keys(), ...(github ? t.github : t.gitlab).keys()])) {
    const r = rank(name)
    if (r !== null) found.push({ name, emoji: lookup(name)!, rank: r })
  }
  found.sort((a, b) => a.rank - b.rank || Number(!!b.custom) - Number(!!a.custom) || a.name.length - b.name.length || a.name.localeCompare(b.name))
  const seen = new Set<string>()
  const out: EmojiMatch[] = []
  for (const { rank: _, ...f } of found) {
    if (seen.has(f.emoji)) continue
    seen.add(f.emoji)
    out.push(f)
    if (out.length === limit) break
  }
  return out
}

/** `text` with its shortcodes as emoji (an MR's title); before the table is loaded, as it is. */
export function emojiText(text: string, github: boolean): string {
  if (!LOOKS.test(text)) return text
  const lookup = lookupOn(github)
  return lookup ? replaceShortcodes(text, lookup) : text
}

/**
 * `text` cut into its pieces: text with the standard shortcodes replaced, and each custom emoji
 * on its own (`emoji`: its picture, `text` its `:name:`). A standard name wins over a custom one.
 */
export function splitShortcodes(text: string, lookup: ((name: string) => string | undefined) | null, custom: Map<string, string> | null): { text: string; emoji?: string }[] {
  const out: { text: string; emoji?: string }[] = []
  let plain = ''
  let last = 0
  for (const m of text.matchAll(SHORTCODE)) {
    const name = m[0].slice(1, -1)
    const std = lookup?.(name)
    const url = std ? undefined : custom?.get(name)
    if (!std && !url) continue
    plain += text.slice(last, m.index)
    last = m.index! + m[0].length
    if (std) plain += std
    else {
      if (plain) out.push({ text: plain })
      plain = ''
      out.push({ text: m[0], emoji: url })
    }
  }
  plain += text.slice(last)
  if (plain) out.push({ text: plain })
  return out
}

/** `text` in pieces (see `splitShortcodes`), on a GitLab project's text (`project`) its groups' custom emoji too. */
export function emojiPieces(text: string, github: boolean, project: string | null = null): { text: string; emoji?: string }[] {
  if (!LOOKS.test(text)) return [{ text }]
  return splitShortcodes(text, lookupOn(github), github ? null : customOf(project))
}

/**
 * Spans with their shortcodes as emoji (see `emojiPieces`). Code, images, references and bare URLs
 * are left alone.
 */
export function withEmoji(list: Span[], github: boolean, project: string | null = null): Span[] {
  const open = (s: Span) => !s.code && !s.image && !s.ticket && !s.action && s.mr === undefined && s.href !== s.text
  if (!list.some((s) => open(s) && LOOKS.test(s.text))) return list
  return list.flatMap((s) => (open(s) ? emojiPieces(s.text, github, project).map((p) => ({ ...s, ...p })) : [s]))
}
