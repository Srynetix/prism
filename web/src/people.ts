import { SvelteMap } from 'svelte/reactivity'
import { hasCore, invoke } from './bridge'
import { conventions } from './conventions'
import { live } from './live.svelte'

/**
 * Badge of a GitLab person (author of an MR): their initials on a color of their
 * own. The authors of the displayed MRs, in alphabetical order, take the successive
 * hues of the golden angle, which spread as far apart as possible: two authors never
 * have neighboring colors, and each keeps theirs as long as the list does not change.
 */
const hueOf = (slot: number) => Math.round((20 + slot * 137.508) % 360)

function hash(username: string): number {
  let h = 2166136261
  for (const c of username) h = Math.imul(h ^ c.charCodeAt(0), 16777619) >>> 0
  return h
}

let cache: { key: string; order: string[] } | null = null
export function personColor(username: string): string {
  const order = [...new Set((live.snapshot?.merge_requests ?? []).map((m) => m.author))].sort()
  const key = order.join(',')
  if (cache?.key !== key) cache = { key, order }
  const i = cache.order.indexOf(username)
  // Outside the displayed authors: a hue drawn from the name, after the authors' ones.
  const hue = hueOf(i >= 0 ? i : cache.order.length + (hash(username) % 8))
  // Yellows and greens are light by themselves: darker, so that white reads on them.
  return `hsl(${hue} 50% ${hue >= 45 && hue <= 175 ? 34 : 45}%)`
}

/** Hue of a non-primary repository (badge of its MRs): drawn from its name, stable from one launch to the next. */
export function repoColor(name: string): string {
  const hue = hueOf(hash(name) % 12)
  return `hsl(${hue} 45% ${hue >= 45 && hue <= 175 ? 32 : 46}%)`
}

/** `Ann Two` → `AT`; a single-word name, or empty: the first two letters of the username. */
export function initialsOf(name: string, username: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean)
  if (words.length >= 2) return (words[0][0] + words[words.length - 1][0]).toUpperCase()
  return (words[0] ?? username).replace(/[^\p{L}\p{N}]/gu, '').slice(0, 2).toUpperCase() || '?'
}

/** The first name, for a short label; failing that, the username. */
export const firstName = (name: string, username: string) => name.trim().split(/\s+/)[0] || username

/**
 * A forge person's profile picture (MR author, member) as a `data:` URL, once loaded (the
 * window's CSP allows no external image): each read once per session by the Rust core, one that
 * fails is not asked again and the initials stay. `[forge] avatars` off: none.
 */
const pictures = new SvelteMap<string, string>()
const asked = new Set<string>()
export function forgeAvatar(url: string | null | undefined): string | undefined {
  if (!url || !hasCore() || !conventions().forge_avatars) return undefined
  if (!asked.has(url)) {
    asked.add(url)
    invoke<string>('forge_avatar', { url }).then((src) => pictures.set(url, src)).catch(() => {})
  }
  return pictures.get(url)
}
