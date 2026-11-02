import { mrKey, type MrRef } from './mr-ref'
import { live } from './live.svelte'
import { rememberPanel } from './recent-panels.svelte'
import { t } from './i18n.svelte'

/** MR open in the detail pane, by (repository, number): the pane reads the up-to-date MR from the snapshot. */
export const mrView = $state({ ref: null as MrRef | null })

export function openMr(ref: MrRef) {
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
