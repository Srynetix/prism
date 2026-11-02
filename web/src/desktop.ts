import { invoke, isDesktop } from './bridge'
import { t, tn } from './i18n.svelte'
import { countedSessions, waitingAlert, type Alert } from './rules'
import type { Snapshot } from './types'
import { prismName } from './profiles.svelte'
import { agentName, nameOf } from './agent-cli'
import { route, type DeskEvent } from './digest.svelte'
import { canNotify, notify } from './notify'

/**
 * Desktop integration: a counter in the menu bar / tray, and notifications for what has just
 * appeared. The first pass serves as the baseline: nothing is notified at startup.
 */
let seen: Set<string> | null = null

type Event = DeskEvent

/** What deserves an interruption: broken infra, CI, deployments (ArgoCD) and data pipelines, Claude waiting, review requested. */
function events(s: Snapshot, alerts: Alert[]): Event[] {
  const out: Event[] = []
  for (const a of alerts) {
    if (a.notify === false) continue
    if ((a.kind === 'CI' || a.kind === 'Infra' || a.kind === 'ArgoCD') && a.severity === 'ko') out.push({ key: `alert:${a.text}`, title: a.kind === 'CI' ? t('Pipeline failed') : a.kind === 'ArgoCD' ? t('Deployment') : t('Pods'), body: a.text, kind: a.kind === 'CI' ? 'pipeline' : a.kind === 'ArgoCD' ? 'deploy' : 'infra' })
    if (a.notification && a.severity === 'ko') out.push({ key: a.notification.key, title: a.notification.title, body: a.text, kind: a.block ? 'block' : 'infra' })
    // One notification per transition to the broken state: the key tracks only the object, not the changing detail.
    if (a.kind === 'Data' && a.severity === 'ko') out.push({ key: `data:${a.text.split(/\u00a0?: /)[0]}`, title: t('Data pipeline'), body: a.text, kind: 'data' })
    if (a.kind === 'Review') out.push({ key: `review:${a.text}`, title: t('Review requested'), body: a.text, kind: 'review' })
  }
  for (const c of countedSessions(s).filter(waitingAlert)) {
    // A Prism agent finishing its turn does not notify: only its permissions interrupt.
    if (c.status === 'needs_you' && c.origin !== 'agent') out.push({ key: `session:${c.id}:needs_you`, title: t('Claude is waiting for you'), body: c.needs_action ? `${c.title} — ${c.needs_action}` : c.title, kind: 'agent' })
    // A Prism agent runs its own CLI; a desktop session is Claude's.
    const agent = c.origin === 'agent' ? s.agents?.find((a) => a.id === c.id) : undefined
    if (c.status === 'waiting_tool') out.push({ key: `session:${c.id}:tool`, title: t('{agent} needs a permission', { agent: agent ? nameOf(agent) : 'Claude' }), body: c.title, kind: 'agent' })
  }
  return out
}

/** What the menu bar was last given. */
let lastTray: { count: number; tooltip: string } | null = null

export async function syncDesktop(s: Snapshot, alerts: Alert[]) {
  if (!isDesktop()) return
  const waiting = countedSessions(s).filter(waitingAlert)
  const urgent = alerts.filter((a) => a.severity !== 'info' && a.kind !== 'Claude')
  const lines = [
    waiting.length && tn(waiting.length, '{n} {agent} session waiting', '{n} {agent} sessions waiting', { agent: agentName() }),
    ...urgent.slice(0, 5).map((a) => `${t(a.kind)}: ${a.text}`),
  ].filter(Boolean)
  // Only when it changed: an IPC call and the menu bar item set again at every snapshot otherwise.
  const tray = { count: waiting.length + urgent.length, tooltip: lines.length ? `${prismName()}\n${lines.join('\n')}` : `${prismName()} · ${t('nothing to report')}` }
  if (tray.count !== lastTray?.count || tray.tooltip !== lastTray.tooltip) {
    lastTray = tray
    invoke('set_tray', tray).catch((e) => {
      lastTray = null
      console.error('tray', e)
    })
  }

  const current = events(s, alerts)
  const keys = new Set(current.map((e) => e.key))
  // Held for a digest, or silenced (quiet hours, focus): see digest.svelte.ts.
  const fresh = route(seen ? current.filter((e) => !seen!.has(e.key)) : [], keys)
  seen = keys
  if (fresh.length && (await canNotify())) {
    // Beyond three, a summary rather than an avalanche.
    if (fresh.length > 3) notify({ title: prismName(), body: t('{n} new items need your attention', { n: fresh.length }) })
    else for (const e of fresh) notify({ title: e.title, body: e.body })
  }
}
