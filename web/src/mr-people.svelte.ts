import { invoke } from './bridge'
import { keyedRead } from './keyed-read.svelte'
import { mrArgs, type MrRef } from './mr-ref'
import type { Member } from './bindings/Member'
export type { Member }

/**
 * Assignees and reviewers of an MR: the GitLab project's members (read on the first opening of the
 * picker, kept for an hour on the Rust side), and the picker to open from an MR's ⋯ menu.
 */

export type PeopleKind = 'assignees' | 'reviewers'

/** Per repository (`''`: the primary one): each project has its members, read again after an hour. */
const members = keyedRead((repo) => invoke<Member[]>('forge_members', { repo: repo || null }))
export const membersOf = (repo: string | null | undefined) => members.get(repo ?? '')
export const membersError = (repo: string | null | undefined) => members.error(repo ?? '')
export const loadMembers = (repo: string | null | undefined) => members.load(repo ?? '')

/** The whole list of assignees (or reviewers); empty: nobody. The snapshot follows on the Rust side. */
export const setMrPeople = (mr: MrRef, kind: PeopleKind, usernames: string[]) =>
  invoke<void>('set_mr_people', { ...mrArgs(mr), reviewers: kind === 'reviewers', usernames })

/** Picker requested by the ⋯ menu: the MR's pane opens it on its line. */
export const peoplePick = $state({ ref: null as MrRef | null, kind: null as PeopleKind | null })
export function pickPeople(mr: MrRef, kind: PeopleKind) {
  peoplePick.ref = { iid: mr.iid, repo: mr.repo ?? null }
  peoplePick.kind = kind
}
