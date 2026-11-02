import type { InvokeArgs } from '@tauri-apps/api/core'
import type { Transport } from '../bridge'
import type { Snapshot } from '../types'
import { api, get } from './passkey'
import { openSocket, type SocketState } from './socket.svelte'
import { NO_BOOT } from './switched'

/**
 * Several profiles on one phone (`[remote] tunnel = "relay"`, `prism_remote::relay`): the page is
 * the hub's, the profile whose remote has the tunnel; another profile open beside it on the Mac is
 * reached at `/p/<profile>/`. The phone shows one profile at a time (its tabs, its views), or All:
 * the hub's views, with the agents waiting in every profile on Home and the agents of all of them
 * under Agents. The choice is the phone's own, kept in it; switching reloads the page there.
 */
export interface PhoneProfile {
  id: string
  name: string
  /** A color of the app or a CSS color (`profileColor`). */
  color: string
  /** The profile whose remote has the tunnel: this page's. */
  hub: boolean
  /** Its Prism runs on the Mac. */
  open: boolean
  /** Its Prism answers here (tunnel relay). */
  reachable: boolean
  agents: number
  waiting: number
}

/** The choice that shows every profile at once. */
export const ALL = 'all'
const KEY = 'prism.remote.profile'

export const phoneProfiles = $state({
  /** The hub's profile id. */
  own: '',
  list: [] as PhoneProfile[],
  /** `all`, or the id of the profile shown. */
  choice: ALL,
  /** All: the other profiles' snapshots, each through its own socket. */
  sides: {} as Record<string, { snap: Snapshot | null; state: SocketState; transport: Transport }>,
})

/** More than one profile: the switcher shows, and the page is tinted with the one shown. */
export const several = () => phoneProfiles.list.length > 1

/** The profile shown: the hub's in All. */
export const shown = (): PhoneProfile | null => phoneProfiles.list.find((p) => p.id === (phoneProfiles.choice === ALL ? phoneProfiles.own : phoneProfiles.choice)) ?? null

/** Where the profile shown is served: `` for the hub's, `/p/<id>` for a relayed one. */
let base = ''
export const remoteBase = () => base

function read(): string | null {
  try {
    return localStorage.getItem(KEY)
  } catch {
    return null
  }
}

function keep(choice: string) {
  try {
    localStorage.setItem(KEY, choice)
  } catch {
    // A private window: the choice lasts the page.
  }
}

/**
 * The profiles, from the hub (`/api/profiles`), and the choice they allow: the one asked in the
 * address (a notification's), else the one kept, else All; a profile no longer answering falls
 * back to All.
 */
export async function loadProfiles(): Promise<void> {
  const r = await api<{ own: string; profiles: PhoneProfile[] }>('/api/profiles').catch(() => null)
  if (!r) return
  phoneProfiles.own = r.own
  phoneProfiles.list = r.profiles
  // The hub's own agent woke the phone in All: All stays (it shows the hub's agents).
  const kept = read()
  const asked = new URLSearchParams(location.search).get('profile')
  const wanted = (asked === r.own && kept === ALL ? null : asked) ?? kept ?? ALL
  const p = r.profiles.find((x) => x.id === wanted)
  phoneProfiles.choice = wanted === ALL || !p || !p.reachable ? ALL : wanted
  if (asked) keep(phoneProfiles.choice)
  base = p && !p.hub && p.reachable ? `/p/${p.id}` : ''
}

/** Read again (the switcher opened, every while on Home): which answer, their counts. */
export async function refreshProfiles(): Promise<void> {
  const r = await api<{ own: string; profiles: PhoneProfile[] }>('/api/profiles').catch(() => null)
  if (r) phoneProfiles.list = r.profiles
}

/**
 * Shows `choice` (a profile's id, or All): the page starts again there, on `then`'s view and agent
 * (a notification's, a waiting agent tapped on Home).
 */
export function switchProfile(choice: string, then: { view?: string; agent?: string } = {}) {
  keep(choice)
  const q = new URLSearchParams()
  if (then.view) q.set('view', then.view)
  if (then.agent) q.set('agent', then.agent)
  const preview = import.meta.env.DEV && new URLSearchParams(location.search).has('preview')
  // The page starts again on the other profile, not the app: no boot screen there.
  try {
    sessionStorage.setItem(NO_BOOT, '1')
  } catch {
    /* private mode: the boot screen shows, tap to skip */
  }
  location.replace(preview ? `${location.pathname}?preview&profiles${q.size ? `&${q}` : ''}` : `/${q.size ? `?${q}` : ''}`)
}

/**
 * Opens a closed profile in its own Prism beside the hub's (`[remote] actions`): the passkey
 * confirms it, every time.
 */
export async function openProfile(id: string): Promise<void> {
  const r = await api<{ stepUp?: { challenge: string; rpId: string; allow: string[] } }>('/api/profiles/open', { id })
  if (r.stepUp) await api('/api/profiles/open', { id, assertion: await get(r.stepUp) })
}

/** All: each other profile answering, its snapshot through a socket of its own. */
export function openSides() {
  if (phoneProfiles.choice !== ALL) return
  for (const p of phoneProfiles.list) {
    if (p.hub || !p.reachable || phoneProfiles.sides[p.id]) continue
    const state = $state<SocketState>({ connected: false, away: false, hello: null, ended: false, error: null })
    const transport = openSocket(() => {}, { base: `/p/${p.id}`, state })
    phoneProfiles.sides[p.id] = { snap: null, state, transport }
    void transport.listen<Snapshot>('snapshot', (e) => {
      const side = phoneProfiles.sides[p.id]
      if (side) side.snap = e.payload
    })
  }
}

/** The profile an agent runs in, among the other profiles shown in All; none: the page's own. */
export function sideOf(agent: string): string | null {
  for (const [id, side] of Object.entries(phoneProfiles.sides)) {
    if (side.snap?.agents?.some((a) => a.id === agent)) return id
  }
  return null
}

/**
 * The page's transport in All: a call about another profile's agent (its quick answers on Home or
 * Agents, by its `id` or `agent`) goes to that profile; any other to the page's own.
 */
export function routed(own: Transport): Transport {
  const target = (args?: InvokeArgs): Transport => {
    const a = args as Record<string, unknown> | undefined
    const agent = typeof a?.id === 'string' ? a.id : typeof a?.agent === 'string' ? a.agent : null
    const side = agent ? sideOf(agent) : null
    return side ? phoneProfiles.sides[side].transport : own
  }
  return {
    invoke: <T>(command: string, args?: InvokeArgs) => target(args).invoke<T>(command, args),
    listen: own.listen,
  }
}

/**
 * Development only (`remote.html?preview`): two more profiles to look at the switcher and All
 * with, the second one's agents a copy of the preview's.
 */
export function previewProfiles(snap: Snapshot | null) {
  phoneProfiles.own = 'default'
  phoneProfiles.list = [
    { id: 'default', name: 'Work', color: 'amber', hub: true, open: true, reachable: true, agents: snap?.agents?.length ?? 0, waiting: 1 },
    { id: 'perso', name: 'Perso', color: 'pink', hub: false, open: true, reachable: true, agents: 2, waiting: 1 },
    { id: 'lab', name: 'Lab', color: 'teal', hub: false, open: false, reachable: false, agents: 0, waiting: 0 },
  ]
  const wanted = read() ?? ALL
  phoneProfiles.choice = phoneProfiles.list.some((p) => p.id === wanted && p.reachable) ? wanted : ALL
  if (phoneProfiles.choice !== ALL || !snap) return
  const state = $state<SocketState>({ connected: true, away: false, hello: null, ended: false, error: null })
  const agents = (snap.agents ?? []).slice(0, 2).map((a, i) => ({ ...a, id: `perso-${a.id}`, title: ['Odin Synth · Filtre résonant', 'poc-car-game · Support manette'][i] ?? a.title, status: i === 0 ? ('needs_you' as const) : ('running' as const), needs_action: i === 0 ? 'Auto-oscillation au-delà de 0.95, ou borner ?' : null }))
  const none: Transport = { invoke: () => Promise.reject(new Error('preview')), listen: async () => () => {} }
  phoneProfiles.sides.perso = { snap: { ...snap, agents }, state, transport: none }
}
