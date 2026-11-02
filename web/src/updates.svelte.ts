import { invoke, isDesktop } from './bridge'
import { restartApp } from './settings.svelte'
import type { UpdateInfo } from './bindings/UpdateInfo'
import type { PackCheck } from './bindings/PackCheck'
export type { UpdateInfo, PackCheck }

/**
 * Prism's own updates (updates.rs): the newest release of `[updates] source`, checked at launch
 * and every few hours while `[updates] enabled`; Settings › General › Updates checks on demand
 * and installs, the status bar says when one is there. The packs the same way (`[updates] packs`):
 * read again from their sources, a changed one shows in the status bar, Settings › Packs updates it.
 */

const EVERY = 6 * 3600_000

export const updates = $state<{ info: UpdateInfo | null; checking: boolean; installing: boolean; error: string; done: string }>({ info: null, checking: false, installing: false, error: '', done: '' })

export const packUpdates = $state<{ list: PackCheck[]; checking: boolean; error: string }>({ list: [], checking: false, error: '' })

/** The status bar's item: packs changed at their source, that this Prism can read. */
export const packsReady = () => packUpdates.list.filter((p) => p.changed && !p.too_new && !p.error)
export const packCheck = (name: string) => packUpdates.list.find((p) => p.name === name)

/** `auto`: the periodic check, which reads nothing with `[updates] enabled` or `packs` off. */
export async function checkPacks(auto = false) {
  if (!isDesktop() || packUpdates.checking) return
  packUpdates.checking = true
  try {
    packUpdates.list = await invoke<PackCheck[]>('pack_check', { auto })
    packUpdates.error = ''
  } catch (e) {
    packUpdates.error = String(e)
  } finally {
    packUpdates.checking = false
  }
}

/** The status bar's item: a newer release that installs here. */
export const updateReady = () => !!updates.info?.enabled && updates.info.available && !!updates.info.install

/** `auto`: the periodic check, which reads nothing with the switch off. */
export async function checkUpdates(auto = false) {
  if (!isDesktop() || updates.checking) return
  updates.checking = true
  try {
    updates.info = await invoke<UpdateInfo>('update_check', { auto })
    updates.error = ''
  } catch (e) {
    updates.error = String(e)
  } finally {
    updates.checking = false
  }
}

export async function installUpdate() {
  const tag = updates.info?.latest?.tag
  if (!tag || updates.installing) return
  updates.installing = true
  updates.error = ''
  try {
    const [restart, message] = await invoke<[boolean, string]>('update_install', { tag })
    updates.done = message
    if (restart) await restartApp()
  } catch (e) {
    updates.error = String(e)
  } finally {
    updates.installing = false
  }
}

let started = false
/** At launch, then every 6 hours; `update_check` and `pack_check` read the config each time (their switches included). */
export function watchUpdates() {
  if (started || !isDesktop()) return
  started = true
  const check = () => {
    void checkUpdates(true)
    void checkPacks(true)
  }
  check()
  setInterval(check, EVERY)
}
