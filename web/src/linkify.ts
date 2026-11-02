import type { Snapshot, Ticket } from './types'
import { namesTicket, ticketRegex } from './conventions'
import { forgeName, mrNumber, mrSign } from './forge'
import type { MrRef } from './mr-ref'
import type { Span } from './markdown'
import { ticketKey } from './tracker'
import { t } from './i18n.svelte'

/** `query`: ticket or MR to filter the dashboard on; absent for a plain URL. */
export type Segment = { text: string } | { text: string; href: string; title?: string; query?: string }

/** `ABC-1234` (prefix from the config), `!1957` (`#1957` on GitHub) and http(s) URLs, in this order of priority at equal position. */
const token = () => ticketRegex(`linkify${mrSign()}`, (t) => String.raw`\b(${t})\b|(?<![\w/&])${mrSign()}(\d+)\b|(https?:\/\/[^\s<>"')\]]+)`, 'gi')

/** Base of the project's MRs (`https://gitlab.com/<group>/<project>/-/merge_requests/`, `https://github.com/<owner>/<repo>/pull/`), deduced from a known MR. */
function mrBase(s: Snapshot): string | null {
  const url = s.merge_requests[0]?.url
  return url ? url.replace(/\d+$/, '') : null
}

/** Splits free text into pieces, known references becoming links. */
export function linkify(text: string, s: Snapshot | null): Segment[] {
  const out: Segment[] = []
  let last = 0
  for (const m of text.matchAll(token())) {
    const [whole, ticket, mr, url] = m
    const link = ticket ? ticketLink(ticket.toUpperCase(), s) : mr ? mrLink(Number(mr), s) : { href: url! }
    if (!link) continue
    if (m.index! > last) out.push({ text: text.slice(last, m.index) })
    out.push({ text: whole, ...link, query: ticket ? ticket.toUpperCase() : mr ? mrNumber(mr) : undefined })
    last = m.index! + whole.length
  }
  if (last < text.length) out.push({ text: text.slice(last) })
  return out
}

/** A ticket id's link (the ticket known, else the tracker's address) and its tooltip, or null. */
export function ticketLink(id: string, s: Snapshot | null) {
  const known = [...(s?.sprint?.tickets ?? []), ...(s?.pickup?.boards.flatMap((b) => b.tickets) ?? []), ...(s?.time_tasks?.tickets ?? [])].find((t) => t.id === id)
  if (known) return { href: known.url, title: `${id} · ${known.name} (${known.status})` }
  const mr = s?.merge_requests.find((m) => namesTicket(m, id))
  if (s?.ticket_base) return { href: `${s.ticket_base}${id}`, title: mr ? `${id} · ${mr.title}` : t('Open {id}', { id }) }
  if (s?.clickup_team) return { href: `https://app.clickup.com/t/${s.clickup_team}/${id}`, title: mr ? `${id} · ${mr.title}` : t('Open {id} in ClickUp', { id }) }
  return null
}

function mrLink(iid: number, s: Snapshot | null) {
  const known = s?.merge_requests.find((m) => !m.repo && m.iid === iid)
  if (known) return { href: known.url, title: `${mrNumber(iid)} · ${known.title}` }
  const base = s ? mrBase(s) : null
  return base ? { href: `${base}${iid}`, title: t('Open {mr} in {forge}', { mr: mrNumber(iid), forge: forgeName() }) } : null
}

/** A ticket (its key for the panel) or an MR, named in a text or pointed to by a URL. */
export type Ref = { ticket: string } | { mr: MrRef }

const knownTickets = (s: Snapshot | null): Ticket[] => [...(s?.sprint?.tickets ?? []), ...(s?.next_sprint?.tickets ?? []), ...(s?.outside_sprint?.tickets ?? []), ...(s?.pickup?.boards.flatMap((b) => b.tickets) ?? []), ...(s?.time_tasks?.tickets ?? [])]

/**
 * The tickets and MRs named in a line of text (`ABC-12`, `!1957`), with their place; those inside a
 * URL are left to the URL. An MR's number is the primary repository's.
 */
export function refsIn(text: string): { start: number; end: number; text: string; ref: Ref }[] {
  const out: { start: number; end: number; text: string; ref: Ref }[] = []
  for (const m of text.matchAll(token())) {
    const [whole, ticket, mr] = m
    if (!ticket && !mr) continue
    out.push({ start: m.index!, end: m.index! + whole.length, text: whole, ref: ticket ? { ticket: ticket.toUpperCase() } : { mr: { iid: Number(mr), repo: null } } })
  }
  return out
}

/**
 * The ticket or MR a URL points to, when Prism knows where they live: an MR of a project it reads
 * (whatever its tab, `/diffs`…), a ticket it shows, or one under the tracker's base. Otherwise null.
 */
export function refOfUrl(url: string, s: Snapshot | null): Ref | null {
  if (!s) return null
  const clean = url.replace(/[?#].*$/, '').replace(/\/+$/, '')
  for (const m of s.merge_requests) {
    const base = m.url.replace(/\d+$/, '')
    const n = clean.startsWith(base) ? /^(\d+)(?:\/|$)/.exec(clean.slice(base.length)) : null
    if (n) return { mr: { iid: Number(n[1]), repo: m.repo ?? null } }
  }
  const known = knownTickets(s).find((t) => t.url.replace(/[?#].*$/, '').replace(/\/+$/, '') === clean)
  if (known) return { ticket: ticketKey(known) }
  const id = ticketRegex('ref-url', (t) => String.raw`(?:^|/)(${t})$`, 'i').exec(clean)?.[1]?.toUpperCase()
  if (!id) return null
  const tracker = (s.ticket_base && clean.startsWith(s.ticket_base)) || /^https:\/\/app\.clickup\.com\/t\//.test(clean)
  return tracker ? { ticket: id } : null
}

/**
 * An agent's words (its Chat): the MRs they name (`!1957`) as spans of their own (`mr`), as its
 * Terminal links them; never inside code, a link, a ticket or an image.
 */
export function withMrs(list: Span[]): Span[] {
  return list.flatMap((s) => {
    if (s.code || s.href || s.ticket || s.action || s.image || s.mr !== undefined) return [s]
    const out: Span[] = []
    let at = 0
    for (const r of refsIn(s.text)) {
      if (!('mr' in r.ref)) continue
      if (r.start > at) out.push({ ...s, text: s.text.slice(at, r.start) })
      out.push({ ...s, text: r.text, mr: r.ref.mr.iid })
      at = r.end
    }
    if (!out.length) return [s]
    if (at < s.text.length) out.push({ ...s, text: s.text.slice(at) })
    return out
  })
}
