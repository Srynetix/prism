import { rememberPanel } from './recent-panels.svelte'

/**
 * The ArgoCD sync drawer: the Applications of a cluster's instances (or all of them), one chosen.
 * Read from the snapshot, which ArgoCD refreshes every 30 s, every 5 s while a sync runs.
 */
export const argoView = $state({
  open: false,
  /** The instances' cluster shown; null: every instance. */
  cluster: null as string | null,
  /** The chosen Application: `<cluster>/<namespace>/<name>`. */
  app: null as string | null,
})

export const argoKey = (cluster: string, namespace: string, name: string) => `${cluster}/${namespace}/${name}`

export function openArgo(cluster: string | null, app: string | null = null) {
  Object.assign(argoView, { open: true, cluster, app })
}

export function closeArgo() {
  const { open, cluster, app } = argoView
  if (open) rememberPanel({ key: `argo:${cluster ?? ''}`, kind: 'argo', title: cluster ? `ArgoCD · ${cluster}` : 'ArgoCD', hint: app?.split('/').pop() ?? '', reopen: () => openArgo(cluster, app) })
  Object.assign(argoView, { open: false, cluster: null, app: null })
}
