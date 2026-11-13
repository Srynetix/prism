import type { Snapshot, Ticket } from './types'
import type { ForgeProvider } from './bindings/ForgeProvider'
import { namesTicket, ticketRegex } from './conventions'
import { forgeName, mrNumber, mrSign } from './forge'
import { sameMr, type MrRef } from './mr-ref'
import type { Span } from './markdown'
import { agentRepo } from './repositories'
import { ticketKey } from './tracker'
import { t } from './i18n.svelte'

/** `query`: ticket or MR to filter the dashboard on; absent for a plain URL. */
export type Segment = { text: string } | { text: string; href: string; title?: string; query?: string }

/**
 * `ABC-1234` (prefix from the config), `!1957` (`#1957` on GitHub; `signs`: those of the
 * repositories an agent works in) and http(s) URLs, in this order of priority at equal position.
 */
const token = (signs = mrSign()) =>
  ticketRegex(`linkify${signs}`, (t) => String.raw`\b(${t})\b|(?<![\w/&])${signs ? String.raw`([${signs}])(\d+)` : '(?!)()()'}\b|(https?:\/\/[^\s<>"')\]]+)`, 'gi')

/**
 * A repository an agent works in whose MRs Prism reads: its `!12` are that repository's. `repo`:
 * null for the primary one; `web`: where its MRs are on its forge.
 */
export interface MrPlace {
  repo: string | null
  forge: ForgeProvider | null
  web: string
}

/**
 * One MR a `!12` may name: its panel when Prism knows it (`knownMr`, read at the click: the
 * snapshot changes all the time), otherwise `url`, its page on the forge.
 */
export interface MrOption {
  ref: MrRef
  url?: string
}

/** The MR Prism shows for `o`, if it knows it. */
export const knownMr = (o: MrOption, s: Snapshot | null | undefined) => s?.merge_requests.find((m) => sameMr(m, o.ref))

/** Each agent's last scope: the same array while its content stays, so the Chat is not redrawn for nothing. */
const scopes = new Map<string, { key: string; value: MrPlace[] | null }>()

/**
 * The repositories an agent (or a Claude Desktop session) works in, where its `!12` point: its
 * folder's, then those it was launched in too (`also`), each when Prism reads its MRs. Null in a
 * folder of no repository: the primary one's `!12`, as everywhere else. Empty: in repositories
 * whose MRs Prism does not read, no MR link.
 */
export function mrScopeOf(id: string, s: Snapshot | null): MrPlace[] | null {
  if (!s) return null
  const a = s.agents.find((x) => x.id === id)
  const d = a ? undefined : s.sessions.find((x) => x.id === id)
  const dirs = a ? [{ cwd: a.cwd, worktree: a.worktree }, ...(a.also ?? []).map((p) => ({ cwd: p, worktree: p }))] : []
  // A Desktop session's repository, found by the core: its folder is never sent.
  const found = a ? dirs.map((x) => agentRepo(x, s)) : [d?.repo]
  const names = [...new Set(found.filter((n): n is string => !!n))]
  const repos = names.map((n) => s.repositories.find((r) => r.name === n)).filter((r) => !!r)
  const value = repos.length ? repos.flatMap((r) => (r.mr_web ? [{ repo: r.primary ? null : r.name, forge: r.forge, web: r.mr_web }] : [])) : null
  const key = JSON.stringify(value)
  const last = scopes.get(id)
  if (last?.key === key) return last.value
  scopes.set(id, { key, value })
  return value
}

const signsOf = (scope: MrPlace[] | null | undefined) => (scope ? [...new Set(scope.map((p) => mrSign(p.forge)))].join('') : mrSign())

/**
 * The MRs `sign` + `iid` may name: the primary repository's without a scope; otherwise that of
 * each repository of the scope written with that sign (several: the choice is offered).
 */
export function mrOptions(iid: number, sign: string, scope: MrPlace[] | null | undefined): MrOption[] {
  if (!scope) return [{ ref: { iid, repo: null } }]
  return scope.filter((p) => mrSign(p.forge) === sign).map((p) => ({ ref: { iid, repo: p.repo, ...(p.forge ? { forge: p.forge } : {}) }, url: `${p.web}${iid}` }))
}

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
    const [whole, ticket, , mr, url] = m
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

/**
 * A ticket (its key for the panel) or an MR, named in a text or pointed to by a URL; `mrs`: the
 * MRs an agent's `!12` may name in the repositories it works in (`mrOptions`).
 */
export type Ref = { ticket: string } | { mr: MrRef } | { mrs: MrOption[] }

const knownTickets = (s: Snapshot | null): Ticket[] => [...(s?.sprint?.tickets ?? []), ...(s?.next_sprint?.tickets ?? []), ...(s?.outside_sprint?.tickets ?? []), ...(s?.pickup?.boards.flatMap((b) => b.tickets) ?? []), ...(s?.time_tasks?.tickets ?? [])]

/**
 * The tickets and MRs named in a line of text (`ABC-12`, `!1957`), with their place; those inside a
 * URL are left to the URL. An MR's number is the primary repository's; with a `scope` (an agent's,
 * `mrScopeOf`), that of the repositories it works in.
 */
export function refsIn(text: string, scope?: MrPlace[] | null): { start: number; end: number; text: string; ref: Ref }[] {
  const out: { start: number; end: number; text: string; ref: Ref }[] = []
  for (const m of text.matchAll(token(signsOf(scope)))) {
    const [whole, ticket, sign, mr] = m
    if (!ticket && !mr) continue
    const at = { start: m.index!, end: m.index! + whole.length, text: whole }
    if (ticket) out.push({ ...at, ref: { ticket: ticket.toUpperCase() } })
    else if (!scope) out.push({ ...at, ref: { mr: { iid: Number(mr), repo: null } } })
    else {
      const mrs = mrOptions(Number(mr), sign, scope)
      if (mrs.length) out.push({ ...at, ref: { mrs } })
    }
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
 * An agent's words (its Chat): the MRs they name (`!1957`) as spans of their own (`mr`: the MRs
 * it may be, `mrOptions`), as its Terminal links them; never inside code, a link, a ticket or an
 * image. `scope`: the repositories the agent works in (`mrScopeOf`).
 */
export function withMrs(list: Span[], scope?: MrPlace[] | null): Span[] {
  return list.flatMap((s) => {
    if (s.code || s.href || s.ticket || s.action || s.image || s.mr !== undefined) return [s]
    const out: Span[] = []
    let at = 0
    for (const r of refsIn(s.text, scope)) {
      const mr = 'mr' in r.ref ? [{ ref: r.ref.mr }] : 'mrs' in r.ref ? r.ref.mrs : null
      if (!mr) continue
      if (r.start > at) out.push({ ...s, text: s.text.slice(at, r.start) })
      out.push({ ...s, text: r.text, mr })
      at = r.end
    }
    if (!out.length) return [s]
    if (at < s.text.length) out.push({ ...s, text: s.text.slice(at) })
    return out
  })
}
