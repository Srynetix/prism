import { invoke, isLocal, isMain } from './bridge'
import { t, tn } from './i18n.svelte'
import { countedSessions, waitingAlert, type Alert } from './rules'
import type { Snapshot } from './types'
import { prismName } from './profiles.svelte'
import { agentName, nameOf } from './agent-cli'
import { route, type DeskEvent } from './digest.svelte'
import { canNotify, notify } from './notify'
import { routineEvents } from './routines.svelte'

/**
 * Desktop integration: a counter in the menu bar / tray, and notifications for what has just
 * appeared. The first pass serves as the baseline: nothing is notified at startup.
 */
let seen: Set<string> | null = null

type Event = DeskEvent

/**
 * What deserves an interruption: broken infra, CI, deployments (ArgoCD) and data pipelines, Claude waiting, review requested.
 * `main`: the app's main window, which also tells of the agents waiting in the other profiles and on the other computers;
 * another profile's window leaves its agents to it (no notification twice).
 */
function events(s: Snapshot, alerts: Alert[], main: boolean): Event[] {
  const out: Event[] = []
  for (const a of alerts) {
    if (a.notify === false) continue
    if ((a.kind === 'CI' || a.kind === 'Infra' || a.kind === 'ArgoCD') && a.severity === 'ko') out.push({ key: `alert:${a.text}`, title: a.kind === 'CI' ? t('Pipeline failed') : a.kind === 'ArgoCD' ? t('Deployment') : t('Pods'), body: a.text, kind: a.kind === 'CI' ? 'pipeline' : a.kind === 'ArgoCD' ? 'deploy' : 'infra' })
    if (a.notification && a.severity === 'ko') out.push({ key: a.notification.key, title: a.notification.title, body: a.text, kind: a.block ? 'block' : 'infra' })
    // One notification per transition to the broken state: the key tracks only the object, not the changing detail.
    if (a.kind === 'Data' && a.severity === 'ko') out.push({ key: `data:${a.text.split(/\u00a0?: /)[0]}`, title: t('Data pipeline'), body: a.text, kind: 'data' })
    if (a.kind === 'Review') out.push({ key: `review:${a.text}`, title: t('Review requested'), body: a.text, kind: 'review' })
  }
  for (const c of main ? countedSessions(s).filter(waitingAlert) : []) {
    // A Prism agent finishing its turn does not notify: only its permissions interrupt.
    if (c.status === 'needs_you' && c.origin !== 'agent') out.push({ key: `session:${c.id}:needs_you`, title: t('Claude is waiting for you'), body: c.needs_action ? `${c.title} — ${c.needs_action}` : c.title, kind: 'agent' })
    // A Prism agent runs its own CLI; a desktop session is Claude's.
    const agent = c.origin === 'agent' ? s.agents?.find((a) => a.id === c.id) : undefined
    if (c.status === 'waiting_tool') out.push({ key: `session:${c.id}:tool`, title: t('{agent} needs a permission', { agent: agent ? nameOf(agent) : 'Claude' }), body: c.title, kind: 'agent' })
  }
  // A routine delivered to the Digest (`deliver = "digest"`): its summary, or at once with the digest off.
  out.push(...routineEvents(s))
  if (!main) return out
  // This computer's other profiles' agents waiting for you, whether their window is open or not.
  for (const p of s.other_profiles ?? []) {
    for (const w of p.waiting_agents ?? []) {
      const at = `profile:${p.id}:${w.id}`
      if (w.status === 'waiting_tool') out.push({ key: `${at}:tool`, title: t('{profile}: an agent needs a permission', { profile: p.name }), body: w.title, kind: 'agent' })
      else if (w.status === 'needs_you') out.push({ key: `${at}:needs_you`, title: t('{profile}: an agent waits for you', { profile: p.name }), body: w.title, kind: 'agent' })
    }
  }
  // Another computer's agents waiting for you (`[[machines]]`), whether its window is open or not.
  for (const m of s.machines ?? []) {
    for (const w of m.waiting) {
      // On one of its other profiles: named too.
      const profile = w.profile ? m.profiles.find((p) => p.id === w.profile)?.name ?? w.profile : null
      const machine = profile ? `${m.name} · ${profile}` : m.name
      const at = `machine:${m.name}:${w.profile ?? ''}:${w.id}`
      if (w.status === 'waiting_tool') out.push({ key: `${at}:tool`, title: t('{machine}: an agent needs a permission', { machine }), body: w.title, kind: 'agent' })
      else if (w.status === 'needs_you') out.push({ key: `${at}:needs_you`, title: t('{machine}: an agent waits for you', { machine }), body: w.title, kind: 'agent' })
    }
  }
  return out
}

/** What the menu bar was last given. */
let lastTray: { count: number; tooltip: string } | null = null

export async function syncDesktop(s: Snapshot, alerts: Alert[]) {
  // This computer's window: a machine's shows that one's agents, which this one's notifies already
  // (`Snapshot.machines`), and leaves the menu bar to it.
  if (!isLocal()) return
  // Another profile's window: its alerts notified (named), the menu bar and its agents the main window's.
  if (!isMain()) return notifyFresh(events(s, alerts, false).map((e) => ({ ...e, title: `${prismName()} · ${e.title}` })))
  const waiting = countedSessions(s).filter(waitingAlert)
  const urgent = alerts.filter((a) => a.severity !== 'info' && a.kind !== 'Claude')
  // As they notify: a permission or your turn, not a review ready.
  const away = (s.machines ?? []).reduce((n, m) => n + m.waiting.filter((w) => w.status === 'waiting_tool' || w.status === 'needs_you').length, 0)
  const elsewhere = (s.other_profiles ?? []).reduce((n, p) => n + (p.waiting_agents ?? []).filter((w) => w.status === 'waiting_tool' || w.status === 'needs_you').length, 0)
  const lines = [
    waiting.length && tn(waiting.length, '{n} {agent} session waiting', '{n} {agent} sessions waiting', { agent: agentName() }),
    elsewhere && tn(elsewhere, '{n} agent waiting in another profile', '{n} agents waiting in other profiles'),
    away && tn(away, '{n} agent waiting on another computer', '{n} agents waiting on other computers'),
    ...urgent.slice(0, 5).map((a) => `${t(a.kind)}: ${a.text}`),
  ].filter(Boolean)
  // Only when it changed: an IPC call and the menu bar item set again at every snapshot otherwise.
  const tray = { count: waiting.length + elsewhere + away + urgent.length, tooltip: lines.length ? `${prismName()}\n${lines.join('\n')}` : `${prismName()} · ${t('nothing to report')}` }
  if (tray.count !== lastTray?.count || tray.tooltip !== lastTray.tooltip) {
    lastTray = tray
    invoke('set_tray', tray).catch((e) => {
      lastTray = null
      console.error('tray', e)
    })
  }

  await notifyFresh(events(s, alerts, true))
}

/** The events not seen at the last pass, notified (the first pass is the baseline). */
async function notifyFresh(current: Event[]) {
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
