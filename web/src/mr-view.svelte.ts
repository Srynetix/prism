import { mrKey, type MrRef } from './mr-ref'
import { live } from './live.svelte'
import { rememberPanel } from './recent-panels.svelte'
import { t } from './i18n.svelte'
import { reveal } from './layers.svelte'

/** MR open in the detail pane, by (repository, number): the pane reads the up-to-date MR from the snapshot. */
export const mrView = $state({ ref: null as MrRef | null })

/**
 * Opens an MR in its drawer. The same one already open, under other drawers perhaps: back to it.
 * Another one: its content changes, and the drawer comes on top (Drawer's `identity`).
 */
export function openMr(ref: MrRef) {
  const cur = mrView.ref
  if (cur && cur.iid === ref.iid && (cur.repo ?? null) === (ref.repo ?? null) && reveal('mr')) return
  mrView.ref = { iid: ref.iid, repo: ref.repo ?? null }
}

export function closeMr() {
  const ref = mrView.ref
  if (ref) {
    const mr = live.snapshot?.merge_requests.find((m) => m.iid === ref.iid && (m.repo ?? null) === (ref.repo ?? null))
    rememberPanel({ key: `mr:${mrKey(ref)}`, kind: 'mr', title: mr ? `${mrKey(ref)} ${mr.title}` : mrKey(ref), hint: t('Merge request'), reopen: () => openMr(ref) })
  }
  mrView.ref = null
}
