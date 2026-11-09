import { hasCore } from './bridge'
import { call } from './blocks/block'
import { alertId } from './alert-bands'
import type { Alert } from './rules'
import type { Snapshot } from './types'

import type { WindowAlert } from './bindings/WindowAlert'

/** An alert as the Changes block keeps it (`WindowAlert`, crates/blocks/src/changes). */
export type FedAlert = WindowAlert

/** The alerts of an environment: Argo CD's and the clusters', those an agent can investigate; one that names none is left out. */
export function infraAlerts(s: Snapshot, alerts: Alert[]): FedAlert[] {
  const out: FedAlert[] = []
  for (const a of alerts) {
    const investigate = a.fixes?.find((f) => f.kind === 'investigate')
    const changes = a.fixes?.find((f) => f.kind === 'changes')
    const argo = a.open?.kind === 'argo' ? a.open : undefined
    if (a.kind !== 'Infra' && a.kind !== 'ArgoCD' && !investigate) continue
    const app = argo ? s.argocd?.instances.flatMap((i) => i.apps).find((x) => argo.app.endsWith(`/${x.name}`)) : undefined
    const env = changes?.env ?? app?.env ?? (investigate && s.environments?.envs.some((e) => e.env === investigate.env) ? investigate.env : undefined)
    // Runners and CI jobs are no environment's.
    if (!env) continue
    const name = changes?.app ?? app?.app ?? investigate?.workload
    out.push({
      // Its object's when it names one (an Argo CD application); otherwise its environment and
      // application, not its text, which changes with a count (restarts) or the language.
      key: a.open ? alertId(a) : `${a.rule ?? a.kind}|${env}|${name ?? alertId(a)}`,
      severity: a.severity,
      text: a.text,
      env,
      ...(name ? { app: name } : {}),
      ...(a.url ? { url: a.url } : {}),
    })
  }
  return out
}

let sent = ''
let timer: ReturnType<typeof setTimeout> | undefined

/**
 * The window computes the alerts; the Changes block notes when one appears and goes. Sent when
 * they change, a few seconds after (a burst of snapshots sends once).
 */
export function feedChanges(s: Snapshot, alerts: Alert[]) {
  // Not before the clusters or Argo CD have been read: their alerts would look gone.
  if (!hasCore() || !s.blocks?.changes || (!s.environments && !s.argocd)) return
  const list = infraAlerts(s, alerts).sort((a, b) => a.key.localeCompare(b.key))
  const text = JSON.stringify(list)
  // Back to what was sent: what was waiting to go is no longer true.
  clearTimeout(timer)
  if (text === sent) return
  timer = setTimeout(() => {
    call('changes', 'alerts', { alerts: list })
      .then(() => (sent = text))
      .catch(() => {})
  }, 3000)
}
