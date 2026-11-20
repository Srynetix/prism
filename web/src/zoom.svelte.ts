import { isDesktop, localInvoke, localListen, machine } from './bridge'
import { appearance } from './fun.svelte'
import { saveEdits } from './settings.svelte'
import { toastError } from './toast.svelte'
import { t } from './i18n.svelte'

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
/**
 * A step taken here, shown at once while config.toml is saved (a moment after the last key, as
 * the terminal's text size): held keys or quick presses step from it, not from the file.
 */
const local = $state({ zoom: null as number | null })

/** What config.toml says this window's zoom is (a machine's: the main window's). */
const saved = (): number => (machine() ? main.zoom : clampZoom(appearance().zoom))
/** The zoom this window shows at. */
export const currentZoom = (): number => local.zoom ?? saved()

/** The zoom the webview was last given (100 % at first: nothing to ask then). */
let shown = 1
/** Once the zoom applied: the canvases, terminals and popovers measure the window again. */
const remeasure = () => requestAnimationFrame(() => dispatchEvent(new Event('resize')))
function apply(zoom: number) {
  if (zoom === shown) return
  shown = zoom
  void localInvoke('ui_zoom', { zoom }).then(remeasure, () => {})
}

let saving: ReturnType<typeof setTimeout> | undefined
/**
 * Shown at once, saved a moment after the last step: this computer's config.toml for a machine's
 * window (the main window then gives it to every machine's). Written even at 100 %, so a pack's
 * value does not come back.
 */
export function setZoom(z: number) {
  const zoom = Math.round(clampZoom(z) * 100) / 100
  local.zoom = zoom
  if (isDesktop()) apply(zoom)
  clearTimeout(saving)
  saving = setTimeout(() => void save(zoom), 400)
}
async function save(zoom: number) {
  const edits = [{ path: ['appearance', 'zoom'], value: zoom }]
  try {
    if (machine()) {
      await localInvoke('save_config', { edits })
      main.zoom = zoom
    } else await saveEdits(edits)
  } catch (e) {
    toastError(t('The zoom was not saved: {error}', { error: String(e) }))
    local.zoom = null
    if (isDesktop()) apply(saved())
  }
}

/** ⌘+ (`1`), ⌘− (`-1`), ⌘0 (`0`: 100 %). */
export const zoomBy = (dir: 1 | -1 | 0) => setZoom(dir === 0 ? 1 : stepZoom(currentZoom(), dir))

/** The window follows its zoom (desktop only: the phone keeps its pinch). */
export function watchZoom() {
  if (!isDesktop()) return
  if (machine()) {
    // The app gives this window the main one's (at once, then at each change): here, what it is.
    void localInvoke<number>('ui_zoom_main').then((z) => {
      main.zoom = z
      apply(z)
    }, () => {})
    void localListen<number>('ui-zoom', (e) => {
      main.zoom = e.payload
      shown = e.payload
      if (local.zoom === e.payload) local.zoom = null
      remeasure()
    })
    return
  }
  return $effect.root(() => {
    $effect(() => {
      // The file caught up with the step taken here: it speaks again.
      if (local.zoom !== null && saved() === local.zoom) local.zoom = null
      apply(currentZoom())
    })
  })
}
