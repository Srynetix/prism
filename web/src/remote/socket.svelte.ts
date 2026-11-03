import type { EventCallback, UnlistenFn } from '@tauri-apps/api/event'
import { t } from '../i18n.svelte'
import type { InvokeArgs } from '@tauri-apps/api/core'
import type { Transport } from '../bridge'
import { get } from './passkey'
import { devPreview } from '../dev-preview.svelte'
import { applyChanges } from '../snapshot-changes'

/** The conversation's version this page speaks (`prism_remote::PROTOCOL`). */
export const PROTOCOL = 3

export interface Hello {
  protocol: number
  device: string
  tabs: string[]
  mail: boolean
  /// `read`, then `act` and `critical` as `[remote]` allows them.
  classes: string[]
  /// `[remote] typing`: an agent's terminal can be taken.
  typing: boolean
  /// `[remote] power`: the computer can be restarted or shut down, confirmed by the passkey.
  power?: boolean
  /**
   * With `power`: whether a restart brings the computer back by itself. `password`: FileVault, the
   * Mac's password typed for the restart to skip the unlock at startup; `stuck`: FileVault on a Mac
   * without authenticated restarts, it waits at startup.
   */
  unlock?: 'free' | 'password' | 'stuck' | null
  /** `[remote] settings`: More › Settings, read and the few phone keys changed. */
  settings?: boolean
  /** `[remote] projects`: More › New project, in the computer's working folder, confirmed by the passkey. */
  projects?: boolean
  /** `[remote] preview`: the dev servers open on the phone, through the preview's address. */
  preview?: boolean
  /// `[remote] push`: the computer's VAPID key, to subscribe with.
  push: string | null
}


/**
 * `connected`: the socket is open. `away`: it has been closed for a while (`GRACE`): what the page
 * shows as the computer out of reach, a reconnection of a moment going unseen.
 */
export interface SocketState {
  connected: boolean
  away: boolean
  hello: Hello | null
  ended: boolean
  error: string | null
}

/** The page's own socket's: the profile it shows. */
export const socketState = $state<SocketState>({ connected: false, away: false, hello: null, ended: false, error: null })
const ownState = socketState

/** Closed for this long, the computer is shown out of reach. */
const GRACE = 5000
/** A call made while the socket reconnects waits this long for it. */
const WAIT_OPEN = 12_000
/** The computer's sign of life comes every 10 s: nothing heard for this long, the socket is dead. */
const SILENT = 30_000

/**
 * The remote's socket, as the page's transport (`bridge.useTransport`): commands become calls,
 * the `snapshot` event the snapshot it keeps up to date. It reconnects by itself, at once when
 * the page comes back to the foreground or the network returns, and when it has heard nothing
 * for a while (a socket can die without a word: the phone asleep, a network change); a call made
 * meanwhile waits for it. A session that ended (`4401`) does not: the page asks for the passkey again.
 *
 * `base`: the profile it talks to (`/p/<profile>`, another profile's Prism through the hub,
 * `phone-profiles`); `state`: a socket of its own beside the page's (All: another profile's
 * snapshot), which says nothing to the page.
 */
export function openSocket(onEnded: () => void, opts: { base?: string; state?: SocketState } = {}): Transport {
  const socketState = opts.state ?? ownState
  const side = !!opts.state
  let ws: WebSocket | null = null
  let snap: unknown = null
  let next = 1
  let delay = 1000
  let heard = 0
  let retry: ReturnType<typeof setTimeout> | null = null
  /** Attempts closed in a row without opening. */
  let failed = 0
  let grace: ReturnType<typeof setTimeout> | null = null
  /** Calls waiting for the socket to open. */
  const queued: (() => void)[] = []
  type Pending = { resolve: (v: unknown) => void; reject: (e: unknown) => void; timer: ReturnType<typeof setTimeout>; command: string; args: unknown }
  const pending = new Map<number, Pending>()
  const listeners = new Map<string, Set<EventCallback<unknown>>>()
  const waiters: (() => void)[] = []

  const emit = (event: string, payload: unknown) => {
    for (const h of listeners.get(event) ?? []) h({ event, id: 0, payload } as never)
  }

  /** The socket lost: the calls it carried may not have reached the computer, they fail. */
  const dropped = () => {
    socketState.connected = false
    grace ??= setTimeout(() => (socketState.away = true), GRACE)
    for (const [id, p] of pending) {
      clearTimeout(p.timer)
      p.reject('the connection to the computer dropped: try again')
      pending.delete(id)
    }
  }

  const connect = () => {
    if (retry) clearTimeout(retry)
    retry = null
    const url = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}${opts.base ?? ''}/api/ws`
    const sock = new WebSocket(url)
    ws = sock
    heard = Date.now()
    sock.onopen = () => {
      heard = Date.now()
      socketState.connected = true
      socketState.away = false
      socketState.error = null
      if (grace) clearTimeout(grace)
      grace = null
      delay = 1000
      failed = 0
      queued.splice(0).forEach((q) => q())
    }
    sock.onmessage = (e) => {
      heard = Date.now()
      const m = JSON.parse(e.data as string)
      if (m.t === 'hello') {
        if (m.protocol !== PROTOCOL) {
          // Another Prism answers: its page is the one to run.
          if (!side) location.reload()
          return
        }
        socketState.hello = m
        if (side) return
        devPreview.phone = true
        devPreview.on = !!m.preview
      } else if (m.t === 'snapshot') {
        snap = m.value
        emit('snapshot', snap)
        waiters.splice(0).forEach((w) => w())
      } else if (m.t === 'changes') {
        snap = applyChanges(snap, m.changes)
        emit('snapshot', snap)
      } else if (m.t === 'result') {
        const p = pending.get(m.id)
        if (!p) return
        pending.delete(m.id)
        clearTimeout(p.timer)
        if ('stepUp' in m) withPasskey(p, m.stepUp)
        else if ('error' in m) p.reject(m.error)
        else p.resolve(m.ok)
      }
    }
    sock.onclose = (e) => {
      // A socket given up for a new one (`revive`) says nothing more.
      if (ws !== sock) return
      dropped()
      if (e.code === 4401 || e.code === 1008) {
        socketState.ended = true
        onEnded()
        return
      }
      socketState.error = t('Reconnecting…')
      retry = setTimeout(connect, delay)
      delay = Math.min(delay * 2, 5000)
      // A socket refused before it opens says nothing of why (a 401 reads as any close): after two,
      // the computer is asked whether this page's session still holds. A Prism that came back without
      // it (ended meanwhile, or older than its 12 hours) wants the passkey again, not a retry.
      if (++failed >= 2) void sessionGone().then((gone) => {
        if (!gone || socketState.ended || ws !== sock) return
        if (retry) clearTimeout(retry)
        retry = null
        socketState.ended = true
        onEnded()
      })
    }
  }
  connect()

  /** The computer answers, and this page has no session there (the computer out of reach: not gone). */
  const sessionGone = () =>
    fetch('/api/session', { credentials: 'same-origin', cache: 'no-store' })
      .then((r) => (r.ok ? (r.json() as Promise<{ device: string | null }>) : null))
      .then((s) => !!s && !s.device)
      .catch(() => false)

  /** A new socket, now: the old one may be dead without having said so. */
  const revive = () => {
    if (socketState.ended) return
    const old = ws
    ws = null
    old?.close()
    dropped()
    delay = 1000
    connect()
  }
  /** Dead: open but silent past `quiet`, or still opening after 10 s (no answer, no error either). */
  const dead = (quiet: number) => {
    const since = Date.now() - heard
    return ws?.readyState === WebSocket.OPEN ? since > quiet : ws?.readyState === WebSocket.CONNECTING && since > 10_000
  }
  setInterval(() => {
    if (document.visibilityState === 'visible' && dead(SILENT)) revive()
  }, 5000)
  // Back in the foreground, or the network back: the socket is checked at once; one waiting to
  // reconnect does at once.
  const check = () => {
    if (document.visibilityState !== 'visible' || socketState.ended) return
    if (dead(15_000)) revive()
    else if (retry) connect()
  }
  document.addEventListener('visibilitychange', check)
  addEventListener('online', check)

  /** Sends a call; `assertion`: the passkey's answer to the step-up it asked for. */
  const send = (p: Omit<Pending, 'timer'>, assertion?: unknown) => {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      // Reconnecting: sent once the socket opens, unless it takes too long.
      const later = () => send(p, assertion)
      queued.push(later)
      setTimeout(() => {
        const i = queued.indexOf(later)
        if (i < 0) return
        queued.splice(i, 1)
        p.reject('the computer is out of reach')
      }, WAIT_OPEN)
      return
    }
    const id = next++
    // Long enough for the passkey's prompt.
    const timer = setTimeout(() => {
      pending.delete(id)
      p.reject('no answer from the computer')
    }, 120_000)
    pending.set(id, { ...p, timer })
    ws.send(JSON.stringify({ t: 'call', id, command: p.command, args: p.args ?? {}, ...(assertion ? { assertion } : {}) }))
  }

  /** A critical call: the phone's passkey (biometrics) confirms it, then it is sent again. */
  const withPasskey = async (p: Pending, stepUp: { challenge: string; rpId: string; allow: string[] }) => {
    try {
      send(p, await get(stepUp))
    } catch (e) {
      p.reject(e instanceof Error ? `not confirmed: ${e.message}` : 'not confirmed')
    }
  }

  const call = <T>(command: string, args?: InvokeArgs): Promise<T> => {
    if (command === 'get_snapshot') {
      return snap ? Promise.resolve(snap as T) : new Promise((resolve) => waiters.push(() => resolve(snap as T)))
    }
    return new Promise<T>((resolve, reject) => send({ resolve: resolve as (v: unknown) => void, reject, command, args }))
  }

  return {
    invoke: call,
    listen: async <T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> => {
      const set = listeners.get(event) ?? new Set()
      set.add(handler as EventCallback<unknown>)
      listeners.set(event, set)
      return () => set.delete(handler as EventCallback<unknown>)
    },
  }
}
