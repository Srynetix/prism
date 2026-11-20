import { isDesktop, localInvoke, localListen, machine } from './bridge'
import { appearance } from './fun.svelte'
import { saveNow } from './settings.svelte'

/**
 * The whole window's zoom (`[appearance] zoom`), the webview's own as a browser's (`zoom.rs`):
 * text, spaces, icons, pictures and terminals all scale, and the page sees a narrower or wider
 * window, so its breakpoints follow. ⌘+ / ⌘− / ⌘0 and the View menu step it by `ZOOMS`, each step
 * saved. A profile's window has its profile's; a machine's window the main one's (this computer's
 * profile, not the machine's), which its keys change too.
 */
export const ZOOMS = [0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2]

/** From 50 % to 200 %; anything else (not a number) is 100 %. */
export const clampZoom = (z: unknown): number => (typeof z === 'number' && Number.isFinite(z) && z > 0 ? Math.min(2, Math.max(0.5, z)) : 1)

/** The next step up (`1`) or down (`-1`) from `z`, a value between two steps going to the nearer one that way. */
export function stepZoom(z: number, dir: 1 | -1): number {
  const eps = 0.001
  if (dir > 0) return ZOOMS.find((s) => s > z + eps) ?? ZOOMS[ZOOMS.length - 1]
  return [...ZOOMS].reverse().find((s) => s < z - eps) ?? ZOOMS[0]
}

/** `125%`: a zoom as said in the status bar and the menus. */
export const zoomLabel = (z: number) => `${Math.round(z * 100)}%`

/** A machine's window: the main window's zoom, as its app last said. */
const main = $state({ zoom: 1 })

/** The zoom this window shows at. */
export const currentZoom = (): number => (machine() ? main.zoom : clampZoom(appearance().zoom))

/** Saved in config.toml, this computer's for a machine's window; 100 % removes the key. */
export async function setZoom(z: number) {
  const zoom = Math.round(clampZoom(z) * 100) / 100
  const value = zoom === 1 ? null : zoom
  if (machine()) {
    main.zoom = zoom
    await localInvoke('save_config', { edits: [{ path: ['appearance', 'zoom'], value }] }).catch(() => {})
  } else await saveNow(['appearance', 'zoom'], value)
}

/** ⌘+ (`1`), ⌘− (`-1`), ⌘0 (`0`: 100 %). */
export const zoomBy = (dir: 1 | -1 | 0) => setZoom(dir === 0 ? 1 : stepZoom(currentZoom(), dir))

/** Once the zoom applied: the canvases, terminals and popovers measure the window again. */
const remeasure = () => requestAnimationFrame(() => dispatchEvent(new Event('resize')))

/** The window follows its zoom (desktop only: the phone keeps its pinch). */
export function watchZoom() {
  if (!isDesktop()) return
  if (machine()) {
    // The app applies the main window's to this one: here, only what it is, for the keys and the badge.
    void localInvoke<number>('ui_zoom_main').then((z) => {
      main.zoom = z
      return localInvoke('ui_zoom', { zoom: z })
    }).then(remeasure, () => {})
    void localListen<number>('ui-zoom', (e) => {
      main.zoom = e.payload
      remeasure()
    })
    return
  }
  return $effect.root(() => {
    let shown = 1
    $effect(() => {
      const zoom = currentZoom()
      if (zoom === shown && zoom === 1) return
      shown = zoom
      void localInvoke('ui_zoom', { zoom }).then(remeasure, () => {})
    })
  })
}
