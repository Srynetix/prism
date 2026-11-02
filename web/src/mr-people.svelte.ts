import { hasCore, invoke } from './bridge'
import { mrArgs, type MrRef } from './mr-ref'
import type { Member } from './bindings/Member'
export type { Member }

/**
 * Assignees and reviewers of an MR: the GitLab project's members (read on the first opening of the
 * picker, kept for an hour on the Rust side), and the picker to open from an MR's ⋯ menu.
 */

export type PeopleKind = 'assignees' | 'reviewers'

/** Per repository (`''`: the primary one): each GitLab project has its members. */
export const members = $state({ byRepo: {} as Record<string, Member[]>, loading: false, error: '' })
export const membersOf = (repo: string | null | undefined) => members.byRepo[repo ?? ''] ?? null

export async function loadMembers(repo: string | null | undefined) {
  if (!hasCore() || members.loading || membersOf(repo)) return
  members.loading = true
  members.error = ''
  try {
    members.byRepo[repo ?? ''] = await invoke<Member[]>('forge_members', { repo: repo ?? null })
  } catch (e) {
    members.error = String(e)
  } finally {
    members.loading = false
  }
}

/** The whole list of assignees (or reviewers); empty: nobody. The snapshot follows on the Rust side. */
export const setMrPeople = (mr: MrRef, kind: PeopleKind, usernames: string[]) =>
  invoke<void>('set_mr_people', { ...mrArgs(mr), reviewers: kind === 'reviewers', usernames })

/** Picker requested by the ⋯ menu: the MR's pane opens it on its line. */
export const peoplePick = $state({ ref: null as MrRef | null, kind: null as PeopleKind | null })
export function pickPeople(mr: MrRef, kind: PeopleKind) {
  peoplePick.ref = { iid: mr.iid, repo: mr.repo ?? null }
  peoplePick.kind = kind
}
