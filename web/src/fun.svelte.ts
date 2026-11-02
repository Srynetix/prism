import { invoke, isDesktop } from './bridge'
import { live } from './live.svelte'
import type { Appearance } from './types'

/**
 * Prism's playful touches, each with its switch
 * in `[appearance]` of config.toml (and in Settings → Appearance): boot, animations,
 * celebrations, easter eggs, Friday guard. macOS "Reduce motion" cuts the motion, and
 * "Reduce transparency" a theme's glass and overlays, unless `ignore_reduce`. Mirrored on `<html>`
 * for the CSS: `data-animations`, `data-theme-motion`, `data-reduce-motion`, `data-reduce-transparency`, `data-density`.
 */
export const DEFAULT_APPEARANCE: Appearance = { theme: 'system', theme_preview: true, density: 'compact', ignore_reduce: false, font: '', font_mono: '', font_terminal: '', language: '', locale: '', boot: true, animations: true, theme_motion: true, celebrations: true, easter_eggs: true, friday_guard: true, screensaver: true, screensaver_seconds: 600, screensaver_mode: 'spectrum', tooltips: true, tooltip_delay: 400, ticket_threads: false, page_swipe: true, pin_drawers: false, statusbar: [], statusbar_hidden: [] }
/** Read through a derived: a reader (each date formatted, `featureOn`) depends on the appearance
 * only, not on the whole snapshot, so it does not run again at each change of it (a change keeps
 * `appearance` as it was unless it changed). */
class AppearanceSource {
  v = $derived(live.snapshot?.appearance)
}
const source = new AppearanceSource()
/** The snapshot's `appearance` as it is (undefined before the first one), read that way. */
export const rawAppearance = () => source.v
/** Merged once per snapshot's `appearance`, not at each of the many calls of a refresh. */
let merged: { from: Appearance | undefined; value: Appearance } | null = null
export const appearance = (): Appearance => {
  const from = source.v
  if (!merged || merged.from !== from) merged = { from, value: { ...DEFAULT_APPEARANCE, ...from } }
  return merged.value
}
export type Feature = 'boot' | 'animations' | 'theme_motion' | 'celebrations' | 'easter_eggs' | 'friday_guard'
export const featureOn = (f: Feature) => appearance()[f]

/** What macOS asks: less motion (the webview knows), less transparency (asked of the app, WebKit does not say). */
const reduce = typeof matchMedia === 'function' ? matchMedia('(prefers-reduced-motion: reduce)') : null
export const system = $state({ reduceMotion: reduce?.matches ?? false, reduceTransparency: false })
reduce?.addEventListener('change', (e) => (system.reduceMotion = e.matches))
async function readTransparency() {
  if (isDesktop()) system.reduceTransparency = await invoke<boolean>('reduce_transparency').catch(() => false)
}
void readTransparency()
// Changed in System Settings while Prism is in the background: read again on coming back.
if (typeof window !== 'undefined') addEventListener('focus', () => void readTransparency())

/** Motion is reduced: macOS asks it, and `[appearance] ignore_reduce` does not override it. */
export const reducedMotion = () => system.reduceMotion && !appearance().ignore_reduce
export const reducedTransparency = () => system.reduceTransparency && !appearance().ignore_reduce

/** A decorative animation is allowed: its feature is on, and motion is not reduced. */
export const moves = (f: Feature = 'animations') => featureOn(f) && !reducedMotion()

/** `<html data-animations | data-reduce-motion | data-reduce-transparency | data-density>`, for the CSS (app.css). */
export function watchAnimations() {
  return $effect.root(() => {
    $effect(() => {
      const html = document.documentElement
      html.dataset.animations = featureOn('animations') ? 'on' : 'off'
      html.dataset.themeMotion = moves('theme_motion') ? 'on' : 'off'
      html.toggleAttribute('data-reduce-motion', reducedMotion())
      html.toggleAttribute('data-reduce-transparency', reducedTransparency())
      html.dataset.density = appearance().density === 'comfortable' ? 'comfortable' : 'compact'
    })
  })
}

/* ---------- Confetti ---------- */

export interface Burst {
  id: number
  x: number
  y: number
  count: number
  /** Spread of the burst, in radians, centered upwards. */
  spread: number
  colors?: string[]
}
/** Pending bursts, which Confetti.svelte draws then removes. */
export const bursts = $state<Burst[]>([])
let next = 0

/** A burst of confetti from (x, y), in window coordinates. */
export function celebrate(x: number, y: number, opts: { count?: number; spread?: number; colors?: string[] } = {}) {
  if (!moves('celebrations')) return
  bursts.push({ id: next++, x, y, count: opts.count ?? 90, spread: opts.spread ?? Math.PI / 1.6, colors: opts.colors })
}

/** From an element (its middle, at the top), otherwise from the bottom of the window. */
export function celebrateFrom(el: Element | null | undefined, opts: { count?: number; spread?: number; colors?: string[] } = {}) {
  const r = el?.getBoundingClientRect()
  const visible = r && r.bottom > 0 && r.top < innerHeight && r.width > 0
  celebrate(visible ? r.left + r.width / 2 : innerWidth / 2, visible ? r.top : innerHeight - 40, opts)
}

/** Fireworks: three bursts spread over the width, one after the other. */
export function fireworks() {
  ;[0.25, 0.5, 0.75].forEach((f, i) => setTimeout(() => celebrate(innerWidth * f, innerHeight * 0.85, { count: 70, spread: Math.PI / 2.4 }), i * 250))
}

/* ---------- Startup ---------- */

/** Replay the boot screen (`boot` command), whatever the setting. */
export const boot = $state({ replay: 0 })
export const replayBoot = () => boot.replay++

/* ---------- Screensaver ---------- */

export const SAVER_MODES = ['spectrum', 'bounce', 'matrix', 'stars', 'life'] as const
export type SaverMode = (typeof SAVER_MODES)[number]
/** Launched on demand (`screensaver` command), never on its own. */
export const saver = $state({ on: false, mode: 'spectrum' as SaverMode })
/** The one from the settings (`[appearance] screensaver_mode`), or `random`. */
export function defaultSaver(): SaverMode | 'random' {
  const m = appearance().screensaver_mode ?? 'spectrum'
  return m === 'random' || (SAVER_MODES as readonly string[]).includes(m) ? (m as SaverMode | 'random') : 'spectrum'
}
/** `random`: one at random; without argument, the one from the settings. */
export function startSaver(mode: SaverMode | 'random' = defaultSaver()) {
  saver.mode = mode === 'random' ? SAVER_MODES[(Math.random() * SAVER_MODES.length) | 0] : mode
  saver.on = true
}

/**
 * Starts on its own (`[appearance] screensaver`): after `screensaver_seconds` without key, click,
 * movement or wheel, window visible. Waking (a key, a click) resets the count.
 */
export function watchIdle() {
  let last = Date.now()
  const active = () => (last = Date.now())
  const events = ['keydown', 'pointerdown', 'pointermove', 'wheel'] as const
  for (const e of events) addEventListener(e, active, { capture: true, passive: true })
  const timer = setInterval(() => {
    const a = appearance()
    if (!a.screensaver || saver.on || document.visibilityState !== 'visible') return
    if (Date.now() - last >= Math.max(5, a.screensaver_seconds ?? 600) * 1000) startSaver()
  }, 2_000)
  return () => {
    clearInterval(timer)
    for (const e of events) removeEventListener(e, active, { capture: true })
  }
}
/** ← / →: the previous or next screensaver, without waking. */
export function cycleSaver(delta: number) {
  const i = SAVER_MODES.indexOf(saver.mode)
  saver.mode = SAVER_MODES[(i + delta + SAVER_MODES.length) % SAVER_MODES.length]
}
