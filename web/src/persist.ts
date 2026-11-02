import { invoke, isDesktop } from './bridge'

/**
 * Notes, layout, retro points, the pet and the usage counts (Settings › Cleanup), stored by the app in its data folder (`store_load` /
 * `store_save`). Loaded once before mounting, so that Svelte stores can
 * initialize synchronously.
 */
export type StoreName = 'notes' | 'layout' | 'retro' | 'pet' | 'usage'
const STORES: StoreName[] = ['notes', 'layout', 'retro', 'pet', 'usage']

const cache = new Map<StoreName, unknown>()
const timers = new Map<StoreName, ReturnType<typeof setTimeout>>()

/** Browser preview: `localStorage` replaces the app's data folder. */
const PREVIEW_PREFIX = 'prism.preview.'

export async function preload() {
  if (!isDesktop()) {
    for (const name of STORES) {
      try {
        cache.set(name, JSON.parse(localStorage.getItem(PREVIEW_PREFIX + name) ?? 'null'))
      } catch {
        // Storage unavailable: the preview simply starts empty.
      }
    }
    return
  }
  await Promise.all(
    STORES.map(async (name) => {
      try {
        cache.set(name, await invoke('store_load', { name }))
      } catch (e) {
        console.error(`store ${name}:`, e)
      }
    }),
  )
}

export const read = <T>(name: StoreName): T | null => (cache.get(name) as T | undefined) ?? null

/** Batched writing: resizing with the mouse produces dozens of changes per second. */
export function write(name: StoreName, value: unknown, onError?: (e: unknown) => void) {
  cache.set(name, value)
  clearTimeout(timers.get(name))
  timers.set(
    name,
    setTimeout(() => {
      if (!isDesktop()) {
        try {
          localStorage.setItem(PREVIEW_PREFIX + name, JSON.stringify(value))
        } catch (e) {
          onError?.(e)
        }
        return
      }
      invoke('store_save', { name, value: JSON.parse(JSON.stringify(value)) }).catch((e) => {
        console.error(`store ${name}:`, e)
        onError?.(e)
      })
    }, 300),
  )
}
