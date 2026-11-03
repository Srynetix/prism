import { hasCore, isDesktop } from '../../bridge'
import { pref, setPref } from '../../layout.svelte'
import { read } from '../../persist'
import { onShared, shareSave } from '../../shared.svelte'

/**
 * The pet's state, one for the window and the phones: kept by the app (`pet.json`, shared through
 * the core), formerly in the layout's prefs (moved over at the window's first launch). A phone
 * only writes it once it has read the computer's: a fresh pet of its own never replaces the real one.
 */
const own = read<Record<string, unknown>>('pet')
const former = isDesktop() ? pref<Record<string, unknown> | null>('pet', null) : null

export const petStore = $state<{ value: Record<string, unknown> | null; version: number; ready: boolean }>({
  value: own ?? former,
  version: 0,
  // The window has its file (or its prefs); a phone waits for the computer's.
  ready: !hasCore() || isDesktop(),
})

if (isDesktop() && !own && former) shareSave('pet', () => former)

// Written elsewhere (the window, a phone): the block takes it.
onShared('pet', (saved) => {
  petStore.ready = true
  if (!saved || typeof saved !== 'object') return
  petStore.value = saved as Record<string, unknown>
  petStore.version++
})

/** Keeps the pet's state: the app's store, shared; the layout's prefs in the browser preview. */
export function savePet(value: Record<string, unknown>) {
  if (!hasCore()) return setPref('pet', value)
  if (!petStore.ready) return
  petStore.value = value
  shareSave('pet', () => value)
}

/** The Pet blocks shown now: none, its status bar item stands in (`[pet] status_bar`). */
export const petShown = $state({ n: 0 })
