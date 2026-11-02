import { hasCore, invoke, isDesktop, listen } from './bridge'
import { applyChanges, type Change } from './snapshot-changes'
import type { Snapshot } from './types'

/**
 * Snapshot pushed by the app's Rust core on each change (Tauri `snapshot` event). Raw, never a
 * deep proxy: it is replaced whole, never changed in place, and wrapping each new one cost every
 * read in every block a proxy trap (a third of a refresh, measured), and the phone's unchanged
 * parts (shared by `applyChanges`) their identity.
 */
class Live {
  snapshot = $state.raw<Snapshot | null>(null)
  connected = $state(false)
}
export const live = new Live()

/**
 * Outside Tauri (preview in a browser, development only): snapshot written by
 * the app with `PRISM_DUMP_SNAPSHOT=web/public/dev-snapshot.json`, re-read every 5 s.
 */
const PREVIEW_SNAPSHOT = '/dev-snapshot.json'

export async function connect() {
  if (!hasCore()) {
    const refresh = async () => {
      try {
        live.snapshot = await (await fetch(PREVIEW_SNAPSHOT, { cache: 'no-store' })).json()
        live.connected = true
      } catch {
        live.connected = false
      }
    }
    await refresh()
    setInterval(refresh, 5000)
    return
  }
  // A phone: its socket gives the snapshot whole (`snapshot`), already put together from its changes.
  if (!isDesktop()) {
    // Subscribe before reading the initial state: no change can slip in between.
    await listen<Snapshot>('snapshot', (e) => {
      live.snapshot = e.payload
      live.connected = true
    })
    live.snapshot = await invoke<Snapshot>('get_snapshot')
    live.connected = true
    return
  }
  // The window: the changes to the revision it has; one missed (or none yet), the whole again.
  await listen<Changes>('snapshot-changes', (e) => {
    if (rev === null) return void waiting.push(e.payload)
    take(e.payload)
  })
  await sync()
}

interface Changes { base: number; rev: number; changes: Change[] }
/** Those that came while the whole was asked: applied after it, from its revision on. */
let waiting: Changes[] = []
function take(c: Changes) {
  if (rev === null || c.base < rev) return
  if (c.base !== rev) return void sync()
  live.snapshot = applyChanges(live.snapshot, c.changes) as Snapshot
  rev = c.rev
  live.connected = true
}

/** The revision of the snapshot the window has; `null` while it is asked again. */
let rev: number | null = null
let syncing: Promise<void> | null = null
function sync(): Promise<void> {
  rev = null
  syncing ??= invoke<{ rev: number; snapshot: Snapshot }>('snapshot_sync')
    .then((s) => {
      live.snapshot = s.snapshot
      rev = s.rev
      live.connected = true
      for (const c of waiting.splice(0)) take(c)
    })
    .catch((e) => {
      console.error('snapshot:', e)
      // Asked again in a moment; what came meanwhile is in the whole then.
      waiting = []
      setTimeout(() => void sync(), 2000)
    })
    .finally(() => (syncing = null))
  return syncing
}

/** A tracker is read (`[clickup] enabled`); before the first snapshot, assumed so. */
export const trackerOn = () => live.snapshot?.tracker !== null
