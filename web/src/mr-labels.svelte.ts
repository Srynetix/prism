import { hasCore, invoke } from './bridge'
import { mrArgs, type MrRef } from './mr-ref'
import type { ForgeLabel } from './bindings/ForgeLabel'
export type { ForgeLabel }

/**
 * Labels of an MR: the project's labels (read on the first opening of the picker, kept ten minutes
 * on the Rust side), and the picker to open from an MR's ⋯ menu.
 */

/** Per repository (`''`: the primary one): each project has its labels. */
export const projectLabels = $state({ byRepo: {} as Record<string, ForgeLabel[]>, loading: false, error: '' })
export const labelsOf = (repo: string | null | undefined) => projectLabels.byRepo[repo ?? ''] ?? null

export async function loadLabels(repo: string | null | undefined, again = false) {
  if (!hasCore() || projectLabels.loading || (!again && labelsOf(repo))) return
  projectLabels.loading = true
  projectLabels.error = ''
  try {
    projectLabels.byRepo[repo ?? ''] = await invoke<ForgeLabel[]>('forge_labels', { repo: repo ?? null })
  } catch (e) {
    projectLabels.error = String(e)
  } finally {
    projectLabels.loading = false
  }
}

/** The whole list of labels; only those that move go to the forge. The snapshot follows on the Rust side. */
export const setMrLabels = (mr: MrRef, labels: string[]) => invoke<void>('set_mr_labels', { ...mrArgs(mr), labels })

/** What a label may be called: GitLab separates them with commas and trims them. */
export const validLabel = (name: string) => name.trim() !== '' && !name.includes(',')

/** Picker requested by the ⋯ menu: the MR's pane opens it on its line. */
export const labelPick = $state({ ref: null as MrRef | null, open: false })
export function pickLabels(mr: MrRef) {
  labelPick.ref = { iid: mr.iid, repo: mr.repo ?? null }
  labelPick.open = true
}
