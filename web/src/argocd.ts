import type { ArgoApp, ArgoCd, ArgoInstance, ArgoResource } from './types'
import { stageOf, type Stage } from './stages'
import { t } from './i18n.svelte'

/** ArgoCD: attach the Applications to the Deployments of the Environments block, and qualify their state. */

export interface ArgoRef {
  app: ArgoApp
  instance: ArgoInstance
}

/** Is the instance authoritative for this environment (`prod`, `demo`, `staging`)? */
export const authoritative = (instance: ArgoInstance, env: string | null) => !instance.envs || (!!env && instance.envs.includes(env))

/** Key of a Deployment: environment, namespace, name. */
export const deploymentKey = (env: string, namespace: string, name: string) => `${env}|${namespace}|${name}`

/** The Deployments managed by ArgoCD, excluding customer instances (absent from the Environments block). */
export function byDeployment(argo: ArgoCd | null | undefined): Map<string, ArgoRef> {
  const index = new Map<string, ArgoRef>()
  for (const instance of argo?.instances ?? []) {
    for (const app of instance.apps) {
      if (app.customer || !app.env || !app.namespace || !authoritative(instance, app.env)) continue
      for (const d of app.deployments) index.set(deploymentKey(app.env, app.namespace, d), { app, instance })
    }
  }
  return index
}

/** `ko`: sync failed, or degraded health; `warn`: out of sync; `info`: in progress; `ok`. */
export function argoTone(a: ArgoApp): 'ok' | 'warn' | 'ko' | 'info' {
  if (a.operation === 'Failed' || a.operation === 'Error' || a.health === 'Degraded' || a.health === 'Missing') return 'ko'
  if (a.operation === 'Running' || a.health === 'Progressing') return 'info'
  if (a.sync === 'OutOfSync' || a.health === 'Unknown' || a.sync === 'Unknown') return 'warn'
  return 'ok'
}

/** Short label: the health if it is not good, otherwise the sync, otherwise "Synced"; in English (a prompt quotes it), `t()` where shown. */
export function argoLabel(a: ArgoApp): string {
  if (a.operation === 'Failed' || a.operation === 'Error') return 'Sync failed'
  if (a.operation === 'Running') return 'Syncing'
  if (a.health !== 'Healthy') return a.health
  return a.sync === 'Synced' ? 'Synced' : a.sync
}


/**
 * ArgoCD Application of a mono component for a stage's environment, excluding customer
 * instances. The component's name is the one in the Helm path (`apps/<component>/…`).
 */
export function findApp(argo: ArgoCd | null | undefined, component: string, env: Stage['role']): ArgoRef | null {
  // The stage's environment name: the one in ArgoCD's Helm paths.
  const want = stageOf(env)?.env ?? env
  for (const instance of argo?.instances ?? []) {
    if (!authoritative(instance, want)) continue
    const app = instance.apps.find((a) => !a.customer && a.env === want && a.app === component)
    if (app) return { app, instance }
  }
  return null
}

/**
 * Is the expected tag running? `live`: the deployed image is that tag and the Application is healthy;
 * otherwise the ArgoCD state (sync in progress, out of sync, degraded…). Without an expected tag, the state alone.
 * `label` is translated: test `live`, not the label.
 */
export function rollout(r: ArgoRef, expected: string | null): { tone: 'ok' | 'warn' | 'ko' | 'info'; label: string; live: boolean } {
  const tone = argoTone(r.app)
  if (tone === 'ko') return { tone, label: t(argoLabel(r.app)), live: false }
  // Staging: the image tag contains the commit (`release-q4-2026-<sha>`), expected by its prefix.
  const running = !!expected && r.app.images.some((i) => i === expected || i.endsWith(expected) || i.includes(expected))
  if (running && tone === 'ok') return { tone: 'ok', label: t('live'), live: true }
  if (expected && !running) return { tone: tone === 'ok' ? 'warn' : tone, label: tone === 'ok' ? t('not live') : t(argoLabel(r.app)), live: false }
  return { tone, label: t(argoLabel(r.app)), live: false }
}

/** Page of the Application in the ArgoCD UI, if its URL is configured. */
export const argoUrl = (r: ArgoRef) => (r.instance.url ? `${r.instance.url.replace(/\/$/, '')}/applications/${r.app.name}` : null)

/** A resource's outcome: ko for a failed sync or hook, or a degraded or missing health. */
export const resourceTone = (r: ArgoResource): 'ok' | 'warn' | 'ko' | 'info' =>
  r.status === 'SyncFailed' || r.hook === 'Failed' || r.hook === 'Error' || r.health === 'Degraded' || r.health === 'Missing'
    ? 'ko'
    : r.hook === 'Running' || r.health === 'Progressing'
      ? 'info'
      : (r.status && r.status !== 'Synced') || (r.health && r.health !== 'Healthy')
        ? 'warn'
        : 'ok'
/** `hook Failed · Degraded`, `OutOfSync · Healthy`. */
export const resourceOutcome = (r: ArgoResource) => [r.hook ? `hook ${r.hook}` : r.status, r.health].filter(Boolean).join(' · ')
