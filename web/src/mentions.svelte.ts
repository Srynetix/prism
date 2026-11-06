import { hasCore, invoke } from './bridge'
import { loadMembers, membersError, membersOf } from './mr-people.svelte'
import { keyedRead } from './keyed-read.svelte'
import { escapeRe } from './markdown'
import { forgeAvatar, initialsOf, ticketAvatar } from './people'
import type { Mention } from './bindings/Mention'
import type { Person } from './types'
export type { Mention }

/**
 * Who can be mentioned in a comment (`@`), and how: on a forge, `@username` in the Markdown (the
 * forge notifies them by itself); on ClickUp, `@Name` in the box, kept as a token whose user id
 * goes along with the comment, sent as a rich comment that notifies them (`Mention`).
 */

/** Someone offered by the `@` menu. */
export interface Mentionable {
  /** Their username on a forge; their user id on a tracker. */
  key: string
  name: string
  /** `@username`, shown under the name (forge). */
  handle: string | null
  initials: string
  color: string | null
  /** Their picture's address, loaded by `picture`. */
  avatar: string | null
  /** The tracker's user id (a mention's). */
  id: number
}

export interface MentionSource {
  /** Everyone; `null` while they are read. */
  people: () => Mentionable[] | null
  /** The reading failed. */
  error: () => string
  /** Reads them (again, an hour later): when the box is focused. */
  load: () => void
  /** Those of this thread (MR author, reviewers, commenters…), by key: offered first. */
  first: string[]
  /** My key: never offered. */
  me: string | null
  /** The text put in the box for a person. */
  token: (p: Mentionable) => string
  /** A tracker's: the tokens picked are sent along with the comment (`Mention`). */
  rich: boolean
  picture: (p: Mentionable) => string | undefined
}

/** The members of a forge repository (`null`: the primary one). */
export function forgeMentions(repo: string | null | undefined, first: string[], me: string | null): MentionSource | undefined {
  if (!hasCore()) return undefined
  return {
    people: () => membersOf(repo)?.map((m) => ({ key: m.username, name: m.name || m.username, handle: m.username, initials: initialsOf(m.name, m.username), color: null, avatar: m.avatar_url, id: m.id })) ?? null,
    error: () => membersError(repo),
    load: () => void loadMembers(repo),
    first,
    me,
    token: (p) => `@${p.handle ?? p.key}`,
    rich: false,
    picture: (p) => forgeAvatar(p.avatar),
  }
}

/** Per tracker list: those who see it. */
const lists = keyedRead((list) => invoke<Person[]>('ticket_members', { list }))

/** The members of a tracker list (ClickUp), `first` and `me` by user id. */
export function ticketMentions(list: string, first: number[], me: number | null): MentionSource | undefined {
  if (!hasCore()) return undefined
  return {
    people: () => lists.get(list)?.map((p) => ({ key: String(p.id), name: p.name, handle: null, initials: p.initials, color: p.color ?? null, avatar: p.avatar ?? null, id: p.id })) ?? null,
    error: () => lists.error(list),
    load: () => void lists.load(list),
    first: first.map(String),
    me: me === null ? null : String(me),
    token: (p) => `@${p.name}`,
    rich: true,
    picture: (p) => ticketAvatar(p.avatar),
  }
}

/** Lowercase, without accents: `Élodie` → `elodie`. */
export const fold = (s: string) => s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase()

/**
 * Those a typed `query` (after `@`) offers, at most `max`: me left out, those of the thread first,
 * then those whose name or username starts with it, then the others it is found in; by name.
 */
export function mentionMatches(src: MentionSource, query: string, max = 30): Mentionable[] {
  const q = fold(query)
  const first = new Set(src.first)
  const scored: { p: Mentionable; rank: number }[] = []
  for (const p of src.people() ?? []) {
    if (p.key === src.me) continue
    const name = fold(p.name)
    const handle = fold(p.handle ?? '')
    const starts = !q || handle.startsWith(q) || name.split(/\s+/).some((w) => w.startsWith(q))
    if (!starts && !name.includes(q) && !handle.includes(q)) continue
    scored.push({ p, rank: (first.has(p.key) ? 0 : 2) + (starts ? 0 : 1) })
  }
  return scored.sort((a, b) => a.rank - b.rank || a.p.name.localeCompare(b.p.name)).slice(0, max).map((s) => s.p)
}

/** A token in a text as a whole: not the start of a longer word (`@Ann` is not in `@Annabelle`). */
export const hasToken = (text: string, token: string) =>
  new RegExp(`${escapeRe(token)}(?![\\p{L}\\p{N}_])`, 'u').test(text)

/** The mentions still in a text: a token edited by hand is no longer one; each once. */
export const mentionsIn = (text: string, picked: Mention[]): Mention[] =>
  picked.filter((m, i) => hasToken(text, m.text) && picked.findIndex((o) => o.id === m.id && o.text === m.text) === i)
