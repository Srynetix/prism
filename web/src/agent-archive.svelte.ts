import { live } from './live.svelte'
import { pref, setPref } from './layout.svelte'
import { waitsForYou } from './rules'
import { DRAFTER } from './quick-action-source'
import type { SessionStatus, Snapshot } from './types'

/**
 * Agents and Claude Desktop sessions archived: out of the lists (Agents view, Agents block, the
 * phone's Agents) and their counts, until unarchived — or until they wait for you again after
 * being archived (a turn's end, a permission, a question): then they come back by themselves.
 * An agent drafting a quick action's prompt is always out of them. Archiving changes nothing else: an agent goes on, its terminal stays, Desktop keeps its session.
 * Shared computer ↔ phone (`agents.archived`): its id → its last activity when archived.
 */
const KEY = 'agents.archived'

/** What tells whether it is archived still: an agent or a Desktop session. */
interface Archivable {
  id: string
  status: SessionStatus
  last_activity: string | null
  needs_action?: string | null
  launch?: { from?: string | null } | null
}

const archived = () => pref<Record<string, string>>(KEY, {})

/** It waits for you since it was archived: it comes back. */
const woke = (x: Archivable, at: string) => waitsForYou(x.status) && (x.last_activity ?? '') > at

export function isArchived(x: Archivable): boolean {
  // An agent drafting a quick action's prompt: Prism's, closed once its prompt is read.
  if (x.launch?.from === DRAFTER) return true
  const at = archived()[x.id]
  if (at === undefined) return routineHidden(x)
  return !woke(x, at)
}

/** Opened from "To review" (routines.svelte.ts): a routine's agent in the lists from then on. */
export const ROUTINE_OPENED = 'routines.opened'
/**
 * A routine's agent (`Snapshot.routines`) is archived at once: it comes back when it waits for a
 * permission or an answer, or once opened.
 */
function routineHidden(x: Archivable): boolean {
  if (!live.snapshot?.routines?.runs.some((r) => r.agent === x.id)) return false
  if (pref<string[]>(ROUTINE_OPENED, []).includes(x.id)) return false
  return !(x.status === 'waiting_tool' || x.needs_action === 'Answer its question')
}

export function setArchived(x: Archivable, on: boolean) {
  if (!on && routineHidden(x)) setPref(ROUTINE_OPENED, [...pref<string[]>(ROUTINE_OPENED, []).slice(-199), x.id])
  const { [x.id]: _, ...rest } = archived()
  setPref(KEY, on ? { ...rest, [x.id]: x.last_activity ?? '' } : rest)
}

/** Gone this long without a break (closed, a Desktop session deleted), an archived one is forgotten. */
export const GONE_MS = 10 * 60_000
/** Since when each archived one is seen in neither list (this window only). */
const missing = new Map<string, number>()

/**
 * The archived ids to forget at `now`: those that came back (archived again, they wait for their
 * next call), and those gone from both lists for `GONE_MS` without a break, counted only while
 * Claude Desktop's sessions read (`answered`, no error): a reload of the collectors or a source
 * that fails empties a list for a while, and must not unarchive what comes back.
 */
export function toForget(s: Pick<Snapshot, 'agents' | 'sessions' | 'answered' | 'errors'>, map: Record<string, string>, now: number): string[] {
  const all: Archivable[] = [...(s.agents ?? []), ...(s.sessions ?? [])]
  const byId = new Map(all.map((x) => [x.id, x]))
  const listed = (s.answered ?? []).includes('sessions') && !s.errors?.sessions
  const out: string[] = []
  for (const [id, at] of Object.entries(map)) {
    const x = byId.get(id)
    if (x) {
      missing.delete(id)
      if (woke(x, at)) out.push(id)
    } else if (!listed) {
      missing.delete(id)
    } else {
      const since = missing.get(id) ?? now
      missing.set(id, since)
      if (now - since >= GONE_MS) {
        missing.delete(id)
        out.push(id)
      }
    }
  }
  for (const id of missing.keys()) if (!(id in map)) missing.delete(id)
  return out
}

$effect.root(() => {
  $effect(() => {
    const s = live.snapshot
    const map = archived()
    if (!s || !Object.keys(map).length) return
    const gone = toForget(s, map, Date.now())
    if (gone.length) setPref(KEY, Object.fromEntries(Object.entries(map).filter(([id]) => !gone.includes(id))))
  })
})
