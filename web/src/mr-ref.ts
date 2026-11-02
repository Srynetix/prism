import { mrForge, mrNumber } from './forge'
import type { ForgeProvider } from './bindings/ForgeProvider'
/**
 * An MR is designated by its repository and its number: the primary repository (`repo` absent) and
 * each repository of `[[repos]] merge_requests` have their own numbering (`!214` exists in both).
 * `!1621` for the primary repository, `helm!214` for another (`#1621`, `helm#214` on GitHub, each
 * repository as its own forge writes it): the key, and what the interface shows.
 */
export interface MrRef {
  iid: number
  repo?: string | null
  /** Its forge when not the profile's; absent: its repository's. */
  forge?: ForgeProvider | null
}

export const mrKey = (r: MrRef) => `${r.repo ?? ''}${mrNumber(r.iid, mrForge(r))}`
export const sameMr = (a: MrRef, b: MrRef) => a.iid === b.iid && (a.repo ?? null) === (b.repo ?? null)
/** The arguments of a Tauri command on an MR. */
export const mrArgs = (r: MrRef) => ({ iid: r.iid, repo: r.repo ?? null })
