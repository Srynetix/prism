import { live } from './live.svelte'
import type { Ticket, TrackerFeatures, TrackerProvider } from './types'

/** On Obsidian Kanban boards (`[tracker] provider`) rather than ClickUp. */
export const onObsidian = (): boolean => live.snapshot?.tracker === 'obsidian'

/** The tracker's provider; before the first snapshot, ClickUp. */
export const trackerProvider = (): TrackerProvider => live.snapshot?.tracker ?? 'clickup'

const NAMES: Record<TrackerProvider, string> = { clickup: 'ClickUp', obsidian: 'Obsidian', jira: 'Jira' }

/** The tracker's name, for labels. */
export const trackerName = (): string => NAMES[trackerProvider()]

/** The tracker's logo (`Logo` name). */
export const trackerLogo = (): TrackerProvider => trackerProvider()

/** The source of the tracker's slow queues (To pick, Refinement), answered once they are read. */
export const queuesSource = (): string => ({ clickup: 'clickup queues', obsidian: 'obsidian', jira: 'jira queues' })[trackerProvider()]

/** What the tracker offers; before the first snapshot (or from an older one), everything. */
export const offers = (feature: keyof TrackerFeatures): boolean => live.snapshot?.tracker_features?.[feature] !== false

/**
 * The key a ticket opens by: its id, unless ids are only unique per repository (Obsidian boards),
 * where its own key tells two `T-12` apart.
 */
export const ticketKey = (t: Pick<Ticket, 'id' | 'task_id' | 'repo'>): string => (t.repo || !t.id ? t.task_id : t.id)

/** The primary repository's name. */
export const primaryRepo = (): string | undefined => live.snapshot?.repositories?.find((r) => r.primary)?.name

/**
 * The key the rules look a ticket up by: its id; on Obsidian, qualified by its repository (absent:
 * the primary one), so that an MR, a branch or a worktree only meets its own repository's ticket.
 */
export const ticketRef = (id: string, repo?: string | null): string => (onObsidian() ? `${repo || primaryRepo() || ''}|${id}` : id)

/** The hours an estimate field holds (`7`, `3.5`, `3,5`); null when empty or not a number. */
export function hoursOf(f: { values: { label: string }[] }): number | null {
  const n = Number(f.values[0]?.label.trim().replace(',', '.'))
  return f.values.length && Number.isFinite(n) ? n : null
}
