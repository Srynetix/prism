import { invoke, isDesktop, listen } from './bridge'
import { pref, setPref } from './layout.svelte'
import * as yt from './youtube'
import type { IconName } from './icons'
import { t } from './i18n.svelte'
import type { Episode } from './bindings/Episode'
import type { External as NowPlaying } from './bindings/External'

/**
 * "Audio" block of the status bar: two playback engines (the WebView's `<audio>`, and the
 * hidden YouTube player of youtube.ts) and providers that give them tracks. A
 * provider only lists; playback, resuming and the macOS media keys
 * (Media Session) are shared.
 *
 * Providers: `radio` (live streams from the config), `youtube` (videos, lives, playlists from the
 * config) and `podcast` (episodes of the RSS feeds from the config, read by the Rust core,
 * musicForProgramming by default). To add one: an
 * `AudioProvider` object in `PROVIDERS`, and its groups appear in the menu.
 *
 * Apart, `external`: what macOS shows in "Now Playing" (Spotify in the browser,
 * YouTube, Music…), relayed by the Rust core (`now-playing` event) and remote-controlled.
 * Starting one pauses the other; the bar shows the last of the two to have played.
 */
export interface Track {
  provider: ProviderId
  /** URL of the file or stream: identifies the track. */
  url: string
  title: string
  /** Originating station or podcast. */
  source: string
  /** Live stream: no position, no duration, no resume. */
  live: boolean
}

/** A menu group: a list of tracks under a title ("Radio", "musicForProgramming"). */
export interface TrackGroup {
  key: string
  label: string
  tracks: Track[]
}

export type ProviderId = 'radio' | 'youtube' | 'podcast'

export interface AudioProvider {
  id: ProviderId
  label: string
  /** Badge icon, on `color`. */
  icon: IconName
  color: string
  /** Groups to display; called when the menu opens, the provider keeps its cache. */
  groups(): Promise<TrackGroup[]>
}

interface Source {
  label: string
  url: string
}

let sources: Promise<{ stations: Source[]; feeds: Source[]; youtube: Source[] }> | null = null
const loadSources = () => (sources ??= isDesktop() ? invoke('audio_sources') : Promise.resolve({ stations: [], feeds: [], youtube: [] }))

const radio: AudioProvider = {
  id: 'radio',
  label: 'Radio',
  icon: 'radio',
  color: 'var(--accent)',
  async groups() {
    const { stations } = await loadSources()
    return [{ key: 'radio', label: 'Radio', tracks: stations.map((s) => ({ provider: 'radio', url: s.url, title: s.label, source: s.label, live: true })) }]
  },
}

const youtube: AudioProvider = {
  id: 'youtube',
  label: 'YouTube',
  icon: 'video',
  color: '#e62117',
  async groups() {
    const { youtube } = await loadSources()
    return [{ key: 'youtube', label: 'YouTube', tracks: youtube.map((s) => ({ provider: 'youtube', url: s.url, title: s.label, source: 'YouTube', live: false })) }]
  },
}

const episodes = new Map<number, Promise<Track[]>>()
const podcast: AudioProvider = {
  id: 'podcast',
  label: 'Podcasts',
  icon: 'podcast',
  color: 'var(--src-claude)',
  async groups() {
    const { feeds } = await loadSources()
    return Promise.all(
      feeds.map(async (f, index) => {
        if (!episodes.has(index)) {
          const load = invoke<Episode[]>('audio_feed', { index }).then((list) =>
            list.map((e): Track => ({ provider: 'podcast', url: e.url, title: e.title, source: f.label, live: false })),
          )
          // A failure is not cached: retried at the next opening.
          load.catch(() => episodes.delete(index))
          episodes.set(index, load)
        }
        return { key: `feed:${index}`, label: f.label, tracks: await episodes.get(index)! }
      }),
    )
  },
}

export const PROVIDERS: AudioProvider[] = [radio, youtube, podcast]
export const providerOf = (id: ProviderId) => PROVIDERS.find((p) => p.id === id)!

/** Player outside the app, as seen by macOS "Now Playing" (`audio/now_playing.rs`). */
export interface External extends NowPlaying {
  /** Reception of this state (ms): the position advances by itself from `elapsed`. */
  at: number
}

/** Remembered with the layout: last track, volume, position of each episode. */
interface Saved {
  track: Track | null
  volume: number
  positions: Record<string, number>
}

export const audio = $state({
  track: null as Track | null,
  playing: false,
  /** Buffering (start of a stream, seek within an episode). */
  loading: false,
  error: null as string | null,
  position: 0,
  duration: 0,
  volume: 0.7,
  /** YouTube: the current video (title, channel), that of a live or a playlist. */
  detail: null as { title: string; author: string | null } | null,
  /** YouTube: a live (its "duration" is that of the stream since its start, months). */
  ytLive: false,
  /** Groups from the last menu opening: also used for "next / previous". */
  groups: [] as TrackGroup[],
  external: null as External | null,
  /** The last to have played, our player or the "Now Playing" one: the one shown. */
  active: 'own' as 'own' | 'external',
})

/** "Now Playing" is shown when it played last, or when we have nothing. */
export const showsExternal = () => !!audio.external && !audio.playing && !audio.loading && (audio.active === 'external' || !audio.track)

let el: HTMLAudioElement | null = null
let positions: Record<string, number> = {}

/** After the layout loads: the last track, paused. */
export function initAudio() {
  const saved = pref<Saved | null>('audio', null)
  audio.track = saved?.track ?? null
  audio.volume = saved?.volume ?? 0.7
  positions = saved?.positions ?? {}
  if (audio.track && !audio.track.live) audio.position = positions[audio.track.url] ?? 0
  setupMediaSession()
  if (isDesktop()) {
    listen<Omit<External, 'at'> | null>('now-playing', (e) => setExternal(e.payload))
    invoke<Omit<External, 'at'> | null>('now_playing').then(setExternal, () => {})
  }
}

function setExternal(n: Omit<External, 'at'> | null) {
  // Our own player also appears in "Now Playing": it is not another player.
  if (n && audio.track && (n.title === audio.track.title || n.title === audio.detail?.title)) n = null
  const started = !!n?.playing && !audio.external?.playing
  audio.external = n ? { ...n, at: Date.now() } : null
  if (started) {
    audio.active = 'external'
    if (audio.playing) pauseOwn()
  }
}

const isYt = (t: Track | null) => t?.provider === 'youtube'
/** Has the YouTube player loaded the current track (not just remembered it)? */
let ytLoaded = false

function pauseOwn() {
  if (isYt(audio.track)) yt.pause()
  else el?.pause()
}

function playYoutube(track: Track) {
  el?.pause()
  audio.detail = null
  ytLoaded = yt.load(track.url, audio.volume, {
    state(s) {
      if (s.playing !== undefined) {
        audio.playing = s.playing
        if (s.playing) audio.error = null
      }
      if (s.loading !== undefined) audio.loading = s.loading
      if (s.position !== undefined) audio.position = s.position
      if (s.duration !== undefined) {
        audio.ytLive = s.duration > 86_400
        audio.duration = audio.ytLive ? 0 : s.duration
      }
      if (s.title) audio.detail = { title: s.title, author: s.author ?? null }
      // A single video moves on to the next one in the menu; for a playlist, YouTube handles it.
      if (s.ended && !yt.isPlaylist(track.url)) next()
    },
    error(msg) {
      audio.playing = false
      audio.loading = false
      audio.error = msg
    },
  })
  if (!ytLoaded) {
    audio.loading = false
    audio.error = t('Not a YouTube link')
  }
}

/** Command to the "Now Playing" player; the state follows through the event. */
export async function externalCommand(cmd: 'toggle' | 'play' | 'pause' | 'next' | 'previous') {
  if (cmd === 'toggle' || cmd === 'play') audio.active = 'external'
  try {
    await invoke('now_playing_command', { cmd })
  } catch (e) {
    audio.error = String(e)
  }
}

/** Current position of the external player, extrapolated from the last received state. */
export function externalPosition(x: External, now = Date.now()) {
  if (x.elapsed === null) return null
  const t = x.elapsed + (x.playing ? (now - x.at) / 1000 : 0)
  return x.duration ? Math.min(t, x.duration) : t
}

function save() {
  setPref('audio', { track: audio.track, volume: audio.volume, positions } satisfies Saved)
}

/** Position of an episode: saved every 15 s of playback, and on each pause. */
let lastSaved = 0
function remember(force = false) {
  const t = audio.track
  if (!t || t.live || isYt(t) || !el) return
  if (!force && Math.abs(el.currentTime - lastSaved) < 15) return
  lastSaved = el.currentTime
  // An episode listened to the end starts again from the beginning.
  if (el.duration && el.duration - el.currentTime < 30) delete positions[t.url]
  else positions[t.url] = Math.floor(el.currentTime)
  save()
}

function player(): HTMLAudioElement {
  if (el) return el
  el = new Audio()
  el.preload = 'none'
  el.volume = audio.volume
  el.addEventListener('playing', () => { audio.playing = true; audio.loading = false; audio.error = null })
  el.addEventListener('pause', () => { audio.playing = false; remember(true) })
  el.addEventListener('waiting', () => (audio.loading = true))
  el.addEventListener('timeupdate', () => { audio.position = el!.currentTime; remember() })
  el.addEventListener('durationchange', () => (audio.duration = Number.isFinite(el!.duration) ? el!.duration : 0))
  el.addEventListener('ended', () => { remember(true); next() })
  el.addEventListener('error', () => {
    // Emptied by `clearAudio`: nothing to read, no error.
    if (!audio.track) return
    audio.playing = false
    audio.loading = false
    audio.error = audio.track?.live ? t('Stream unavailable') : t('Cannot read this episode')
  })
  return el
}

/** Plays a track; the current track is simply resumed or paused. */
export async function play(track: Track): Promise<void> {
  const p = player()
  if (audio.track?.url === track.url && (isYt(track) ? ytLoaded : p.src)) return toggle()
  remember(true)
  if (isYt(audio.track)) {
    yt.stop()
    ytLoaded = false
  }
  audio.track = track
  audio.active = 'own'
  if (audio.external?.playing) externalCommand('pause')
  audio.error = null
  audio.loading = true
  audio.position = track.live ? 0 : (positions[track.url] ?? 0)
  audio.duration = 0
  audio.detail = null
  audio.ytLive = false
  if (isYt(track)) {
    save()
    return playYoutube(track)
  }
  p.src = track.url
  if (!track.live && audio.position) p.currentTime = audio.position
  save()
  updateMediaSession()
  try {
    await p.play()
  } catch (e) {
    // A deliberate interruption (pause, track change) interrupts `play()`: not an error.
    if ((e as DOMException).name !== 'AbortError') {
      audio.loading = false
      audio.error = t('Cannot play this source')
    }
  }
}

/** Stops and deselects: no track any more, the bar back to "Audio" (an episode keeps its position).
 * "Now Playing" is let go too, until it plays again; its app is not paused. */
export function clearAudio() {
  remember(true)
  if (isYt(audio.track)) {
    yt.stop()
    ytLoaded = false
  }
  if (el) {
    el.pause()
    el.removeAttribute('src')
    el.load()
  }
  Object.assign(audio, { track: null, playing: false, loading: false, error: null, position: 0, duration: 0, detail: null, ytLive: false, active: 'own' })
  if (!audio.external?.playing) audio.external = null
  if ('mediaSession' in navigator) navigator.mediaSession.metadata = null
  save()
}

export async function toggle(): Promise<void> {
  if (!audio.track) return
  if (isYt(audio.track)) {
    if (!ytLoaded) return play({ ...audio.track })
    if (audio.playing) return yt.pause()
    audio.active = 'own'
    if (audio.external?.playing) externalCommand('pause')
    return yt.play()
  }
  const p = player()
  if (!p.src) return play({ ...audio.track })
  if (p.paused) {
    audio.active = 'own'
    if (audio.external?.playing) externalCommand('pause')
    // A live stream resumed after a pause restarts from live, not from the buffer.
    if (audio.track.live) p.src = audio.track.url
    audio.loading = true
    await p.play().catch(() => (audio.loading = false))
  } else p.pause()
}

/** Neighboring station or episode, in menu order. */
function neighbour(step: 1 | -1): Track | null {
  const t = audio.track
  if (!t) return null
  const group = audio.groups.find((g) => g.tracks.some((x) => x.url === t.url))
  if (!group) return null
  const i = group.tracks.findIndex((x) => x.url === t.url)
  return group.tracks[(i + step + group.tracks.length) % group.tracks.length] ?? null
}

/** Groups are only loaded when the menu opens: before that, they are loaded here. */
async function step(dir: 1 | -1) {
  if (!audio.groups.length) await loadGroups()
  const t = neighbour(dir)
  if (t) play(t)
}
const inPlaylist = () => isYt(audio.track) && ytLoaded && yt.isPlaylist(audio.track!.url)
export const next = () => (inPlaylist() ? yt.next() : void step(1))
export const previous = () => (inPlaylist() ? yt.previous() : void step(-1))

export function seek(seconds: number) {
  if (isYt(audio.track)) return yt.seek(seconds)
  if (!el || audio.track?.live) return
  el.currentTime = Math.max(0, Math.min(seconds, el.duration || seconds))
  remember(true)
}

export function setVolume(v: number) {
  audio.volume = Math.max(0, Math.min(1, v))
  if (el) el.volume = audio.volume
  yt.setVolume(audio.volume)
  save()
}

/** Loads the groups of all providers (when the menu opens). */
export async function loadGroups() {
  const all = await Promise.allSettled(PROVIDERS.map((p) => p.groups()))
  audio.groups = all.flatMap((r) => (r.status === 'fulfilled' ? r.value : [])).filter((g) => g.tracks.length)
  return all.filter((r) => r.status === 'rejected').map((r) => String((r as PromiseRejectedResult).reason))
}

/** Keyboard media keys and macOS "Now Playing". */
function setupMediaSession() {
  const ms = navigator.mediaSession
  if (!ms) return
  ms.setActionHandler('play', () => void toggle())
  ms.setActionHandler('pause', () => void toggle())
  ms.setActionHandler('nexttrack', next)
  ms.setActionHandler('previoustrack', previous)
}

function updateMediaSession() {
  const ms = navigator.mediaSession
  if (!ms || !audio.track || typeof MediaMetadata === 'undefined') return
  ms.metadata = new MediaMetadata({ title: audio.track.title, artist: audio.track.live ? t('Live') : audio.track.source, album: 'Prism' })
}

export const clock = (s: number) => {
  const t = Math.floor(s)
  const h = Math.floor(t / 3600)
  const m = Math.floor((t % 3600) / 60)
  const sec = String(t % 60).padStart(2, '0')
  return h ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`
}
