import { invoke, isDesktop } from './bridge'
import { live } from './live.svelte'
import { colorName, viewColor } from './views'
import type { ProfileInfo } from './bindings/ProfileInfo'
export type { ProfileInfo }

/**
 * Profiles (src-tauri/crates/core/src/profile.rs): separate setups of Prism on this machine, each with its
 * config.toml, packs, agents and data. One is active; switching relaunches Prism on another.
 */

export const DEFAULT_PROFILE = 'default'
/**
 * Colors of a profile's own, beyond the pages': mid tones that carry the badge's white letter in
 * both themes, and tell profiles apart at a glance in the rail, the menu bar and the Dock.
 */
const PROFILE_HUES: Record<string, string> = {
  red: '#d64545', amber: '#c98a12', green: '#2e9e5b', teal: '#0f9488', cyan: '#0891b2',
  indigo: '#4f46e5', pink: '#d6338a', brown: '#8b5e3c', slate: '#64748b',
}
/** Colors offered to a profile: those of the pages, then its own. */
export const PROFILE_COLORS = ['accent', 'coral', 'orange', 'purple', 'blue', 'ocean', ...Object.keys(PROFILE_HUES)]

/** A profile's color by name (a former one, `clickup`, included), or as CSS. */
export const profileColor = (c: string | null | undefined) => (c ? (viewColor(c) ?? PROFILE_HUES[c] ?? c) : 'var(--accent)')
/** Its name as the picker knows it. */
export const profileColorName = (c: string) => colorName(c)
/** `Work` → `W`: the badge's letter. */
export const initial = (name: string) => (name.trim()[0] ?? '?').toUpperCase()

export const profiles = $state({
  list: [] as ProfileInfo[],
  /** The profile being switched to: the overlay says where Prism goes. */
  switching: null as ProfileInfo | null,
})

/** The active profile, from the snapshot. */
export const activeProfile = () => live.snapshot?.profile ?? { id: DEFAULT_PROFILE, name: 'Default', color: '', shared: false }

/** `Prism`, or `Prism — Perso` when there are several profiles: the menu bar icon and notifications. */
export function prismName() {
  const p = activeProfile()
  return p.shared ? `Prism — ${p.name}` : 'Prism'
}

export async function loadProfiles() {
  if (!isDesktop()) return
  profiles.list = await invoke<ProfileInfo[]>('list_profiles')
}

/**
 * Prism relaunches on the profile; its agents and the current ones keep running in tmux. Already
 * open in another Prism: that one comes to the front instead.
 */
export async function switchProfile(p: ProfileInfo) {
  if (p.active) return
  if (!p.open) profiles.switching = p
  try {
    const relaunching = await invoke<boolean>('switch_profile', { id: p.id })
    if (!relaunching) profiles.switching = null
  } catch (e) {
    profiles.switching = null
    throw e
  }
}

/** The profile in its own Prism, next to this one (or that Prism to the front if already open). */
export async function openProfile(p: Pick<ProfileInfo, 'id'>) {
  await invoke('open_profile', { id: p.id })
  setTimeout(() => void loadProfiles().catch(() => {}), 2500)
}

export async function switchProfileById(id: string) {
  if (!profiles.list.length) await loadProfiles()
  const p = profiles.list.find((x) => x.id === id)
  if (p) await switchProfile(p)
}

/** A profile's id from its name, as Rust makes it: `Démo client` → `demo-client`. */
export const slugOf = (s: string) => s.toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '').replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 40)

/**
 * First launch: the active profile's config.toml (empty, from a pack, or the demo's), then Prism
 * relaunches on it.
 */
export const setupProfile = (name: string, color: string, pack: string | null, demo = false) => invoke('setup_profile', { name, color, pack, demo })

/** A new profile: empty, from a pack, or the demo (sample data, nothing read). */
export async function createProfile(name: string, color: string, pack: string | null, demo = false) {
  const p = await invoke<ProfileInfo>('create_profile', { name, color, pack, demo })
  await loadProfiles()
  return p
}

export async function renameProfile(id: string, name: string, color: string) {
  await invoke('rename_profile', { id, name, color })
  await loadProfiles()
}

export async function removeProfile(id: string) {
  await invoke('remove_profile', { id })
  await loadProfiles()
}

/** Settings › General › Profiles, with its New profile form open (`create`). */
export const profilesView = $state({ creating: false })
