import { hasCore, invoke, isDesktop } from './bridge'
import { live } from './live.svelte'

/**
 * Stores the window and the phones (the remote) share, through the core: the notes, the retro
 * points, the pet, the blocks' filters and sorts (`prefs`), the agents' chains (the window's alone). Each page writes with `shared_save`, which bumps the store's revision in the
 * snapshot (`shared_revs`); the other pages read it again. The last write wins.
 */
export type SharedName = 'notes' | 'retro' | 'pet' | 'prefs'

/** The revision of each store this page last wrote or read; absent on a phone until it has read it. */
const known = new Map<SharedName, number>()
/** A write waiting (grouped, as typing produces many) or on its way: a reload would lose it. */
const waiting = new Map<SharedName, ReturnType<typeof setTimeout>>()
const inFlight = new Map<SharedName, number>()

/** The window starts from the files it preloaded; a phone, from nothing: it reads each store first. */
const start = () => (isDesktop() ? 0 : -1)

/** Writes `value()` a moment later, grouped with the writes that follow; `failed` on an error. */
export function shareSave(name: SharedName, value: () => unknown, failed?: (e: unknown) => void) {
  const t = waiting.get(name)
  if (t) clearTimeout(t)
  waiting.set(
    name,
    setTimeout(() => {
      waiting.delete(name)
      inFlight.set(name, (inFlight.get(name) ?? 0) + 1)
      invoke<number>('shared_save', { name, value: value() })
        .then((rev) => known.set(name, Math.max(known.get(name) ?? start(), rev)))
        .catch((e) => failed?.(e))
        .finally(() => inFlight.set(name, (inFlight.get(name) ?? 1) - 1))
    }, 300),
  )
}

/**
 * Calls `apply` with the store as written elsewhere (the window, a phone), each time it changes,
 * unless this page has a write of its own on the way; `null`: never written. Once per store, at
 * the module's level.
 */
export function onShared(name: SharedName, apply: (value: unknown) => void, failed?: (e: unknown) => void) {
  $effect.root(() => {
    $effect(() => {
      const rev = live.snapshot?.shared_revs?.[name] ?? 0
      if (!hasCore() || !live.snapshot || rev <= (known.get(name) ?? start()) || waiting.has(name) || inFlight.get(name)) return
      known.set(name, rev)
      invoke<unknown>('shared_load', { name }).then(apply).catch((e) => failed?.(e))
    })
  })
}
