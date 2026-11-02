import { pref, setPref } from '../layout.svelte'
import type { AgentArtifact } from '../agents.svelte'

/**
 * The artifacts of an agent a phone has seen in its Web tab: the time of the newest when the tab
 * was last open, by agent. Those written since are new (a dot, and the tab's count).
 */
const KEY = 'remote.web.seen'

export const seenAt = (id: string): string => pref<Record<string, string>>(KEY, {})[id] ?? ''

export const isNew = (id: string, a: AgentArtifact) => !!a.modified && a.modified > seenAt(id)

export const unseen = (id: string, list: AgentArtifact[]) => list.filter((a) => isNew(id, a)).length

/** All of them seen, up to the newest. */
export function markSeen(id: string, list: AgentArtifact[]) {
  const newest = list.reduce((m, a) => (a.modified && a.modified > m ? a.modified : m), '')
  if (newest && newest > seenAt(id)) setPref(KEY, { ...pref<Record<string, string>>(KEY, {}), [id]: newest })
}
