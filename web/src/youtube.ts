/**
 * YouTube player for the "YouTube" source: an embed `<iframe>`, kept off screen
 * and shown only in the Audio menu (see `showIn`). It is driven by the embedded player's
 * `postMessage` protocol (`enablejsapi=1`), without loading the YouTube API script:
 * a third-party script in the page would have access to the app's commands.
 *
 * A hidden player departs from the YouTube API terms: a deliberate choice, for personal use.
 *
 * In the app, YouTube refuses the player (error 153): the page, on `tauri://localhost`, sends no
 * referrer. The player is then embedded in the relay's page (`http://127.0.0.1:<port>`, see
 * youtube_relay.rs), which passes the messages between it and this module. In the browser
 * preview, embedded directly.
 */
import { invoke, isDesktop } from './bridge'
import { t } from './i18n.svelte'

const ORIGIN = 'https://www.youtube.com'

/** The relay's origin, asked once; null in the browser preview (no relay needed). */
let relay: Promise<string | null> | null = null
const relayOrigin = () => (relay ??= isDesktop() ? invoke<string>('youtube_relay').catch(() => null) : Promise.resolve(null))
/** Where the frame's messages come from and go: the relay's origin, or YouTube's. */
let frameOrigin = ORIGIN

export interface YtState {
  playing: boolean
  loading: boolean
  ended: boolean
  position: number
  /** 0 for a live. */
  duration: number
  /** Current video (useful in a playlist or a live). */
  title: string | null
  author: string | null
}

/** Error codes of the embedded player. */
const ERRORS: Record<number, string> = {
  2: t('Invalid YouTube link'),
  5: t('YouTube cannot play this here'),
  100: t('Video not found or private'),
  101: t('Embedding disabled by the owner'),
  150: t('Embedding disabled by the owner'),
  153: t('YouTube refused the embed (no referrer)'),
}

let host: HTMLDivElement | null = null
let frame: HTMLIFrameElement | null = null
let handlers: { state(s: Partial<YtState>): void; error(msg: string): void } | null = null
let pendingVolume = 70

/**
 * Reads the URL of a page, a live or a playlist: the corresponding embed URL. `origin`: the page
 * that talks to the player (the relay's, in the app).
 */
export function embedUrl(url: string, origin = location.origin): string | null {
  let u: URL
  try {
    u = new URL(url)
  } catch {
    return null
  }
  const list = u.searchParams.get('list')
  let id = u.searchParams.get('v')
  if (!id && u.hostname === 'youtu.be') id = u.pathname.slice(1)
  if (!id) id = u.pathname.match(/\/(?:live|embed|shorts)\/([\w-]{6,})/)?.[1] ?? null
  const params = new URLSearchParams({ enablejsapi: '1', autoplay: '1', playsinline: '1', rel: '0', origin })
  if (list) params.set('list', list)
  if (id) return `${ORIGIN}/embed/${id}?${params}`
  if (list) return `${ORIGIN}/embed/videoseries?${params}`
  return null
}

function ensureHost(): HTMLDivElement {
  if (host) return host
  host = document.createElement('div')
  host.className = 'yt-host'
  // Off screen while the menu is closed; in its place when it is open.
  Object.assign(host.style, { position: 'fixed', left: '-10000px', top: '0', width: '296px', height: '166px', zIndex: '46', borderRadius: '8px', overflow: 'hidden' })
  document.body.appendChild(host)
  window.addEventListener('message', onMessage)
  return host
}

function send(func: string, args: unknown[] = []) {
  frame?.contentWindow?.postMessage(JSON.stringify({ event: 'command', func, args }), frameOrigin)
}

function onMessage(e: MessageEvent) {
  if (!frame || e.source !== frame.contentWindow || e.origin !== frameOrigin) return
  let data: { event?: string; info?: unknown }
  try {
    data = typeof e.data === 'string' ? JSON.parse(e.data) : e.data
  } catch {
    return
  }
  if (data.event === 'onReady') {
    send('setVolume', [pendingVolume])
    send('playVideo')
  } else if (data.event === 'onError') {
    handlers?.error(ERRORS[Number(data.info)] ?? t('YouTube error {code}', { code: String(data.info) }))
  } else if (data.event === 'infoDelivery' || data.event === 'initialDelivery') {
    const info = (data.info ?? {}) as { currentTime?: number; duration?: number; playerState?: number; videoData?: { title?: string; author?: string } }
    const s: Partial<YtState> = {}
    if (typeof info.currentTime === 'number') s.position = info.currentTime
    if (typeof info.duration === 'number') s.duration = info.duration
    if (typeof info.playerState === 'number') Object.assign(s, stateOf(info.playerState))
    if (info.videoData?.title) {
      s.title = info.videoData.title
      s.author = info.videoData.author ?? null
    }
    handlers?.state(s)
  } else if (data.event === 'onStateChange') {
    handlers?.state(stateOf(Number(data.info)))
  }
}

/** -1 not started, 0 ended, 1 playing, 2 paused, 3 buffering, 5 cued. */
const stateOf = (n: number): Partial<YtState> => ({ playing: n === 1, loading: n === 3 || n === -1, ended: n === 0 })

/** Loads and plays a video, a live or a playlist. False: not a YouTube link. */
export function load(url: string, volume: number, on: NonNullable<typeof handlers>): boolean {
  if (!embedUrl(url)) return false
  handlers = on
  pendingVolume = Math.round(volume * 100)
  const h = ensureHost()
  frame?.remove()
  frame = null
  const ask = ++loads
  void relayOrigin().then((relayAt) => {
    // A later load (or a stop) took over meanwhile.
    if (ask !== loads || handlers !== on) return
    const direct = !relayAt
    const src = embedUrl(url, relayAt ?? location.origin)!
    frameOrigin = relayAt ?? ORIGIN
    frame = document.createElement('iframe')
    frame.src = direct ? src : `${relayAt}/youtube?src=${encodeURIComponent(src)}`
    frame.title = 'YouTube'
    frame.allow = 'autoplay; encrypted-media'
    // Without a referrer, YouTube refuses the embed (error 153).
    frame.referrerPolicy = 'strict-origin-when-cross-origin'
    Object.assign(frame.style, { width: '100%', height: '100%', border: '0' })
    // The player only emits its events once subscribed to (the relay subscribes itself).
    if (direct) frame.addEventListener('load', () => frame?.contentWindow?.postMessage(JSON.stringify({ event: 'listening', id: 'prism', channel: 'widget' }), ORIGIN))
    h.appendChild(frame)
  })
  return true
}

/** Loads asked: the latest one wins. */
let loads = 0

export const play = () => send('playVideo')
export const pause = () => send('pauseVideo')
export const next = () => send('nextVideo')
export const previous = () => send('previousVideo')
export const seek = (s: number) => send('seekTo', [s, true])
export function setVolume(v: number) {
  pendingVolume = Math.round(v * 100)
  send('setVolume', [pendingVolume])
}

/** Stops and unloads the player (switch to another source). */
export function stop() {
  loads++
  frame?.remove()
  frame = null
  handlers = null
}

/** Is it a playlist (next / previous then go through YouTube)? */
export const isPlaylist = (url: string) => {
  try {
    return new URL(url).searchParams.has('list')
  } catch {
    return false
  }
}

/**
 * Shows the player in place of `slot` (the Audio menu's frame) while attached, then
 * sends it back off screen. To use as `{@attach}`.
 */
export function showIn(slot: HTMLElement) {
  const h = ensureHost()
  let raf = 0
  const place = () => {
    const r = slot.getBoundingClientRect()
    Object.assign(h.style, { left: `${r.left}px`, top: `${r.top}px`, width: `${r.width}px`, height: `${r.height}px` })
    raf = requestAnimationFrame(place)
  }
  place()
  return () => {
    cancelAnimationFrame(raf)
    Object.assign(h.style, { left: '-10000px', top: '0' })
  }
}
