import { t } from '../../i18n.svelte'
import type { DiskKind } from '../../bindings/DiskKind'
import type { DiskNode } from '../../bindings/DiskNode'
import type { DiskReclaim } from '../../bindings/DiskReclaim'
import type { DiskReclaimGroup } from '../../bindings/DiskReclaimGroup'
import type { DiskScan } from '../../bindings/DiskScan'
import type { DiskUsageState } from '../../bindings/DiskUsageState'

/** The Disk usage block's state: Rust's types (`crates/blocks/src/disk_usage`), generated in `bindings/`. */
export type { DiskKind, DiskNode, DiskReclaim, DiskReclaimGroup, DiskScan, DiskUsageState }

/** `29.4 GB`, `840 MB`, `12 kB`: binary units, as the file managers count. */
export function size(n: number): string {
  const a = Math.abs(n)
  if (a >= 1 << 30) return t('{n} GB', { n: (n / (1 << 30)).toFixed(a >= 100 * (1 << 30) ? 0 : 1) })
  if (a >= 1 << 20) return t('{n} MB', { n: Math.round(n / (1 << 20)) })
  if (a >= 1 << 10) return t('{n} kB', { n: Math.round(n / (1 << 10)) })
  return t('{n} B', { n })
}

/** `+6.1 GB`, `−1.2 GB`; nothing under 1 MB either way. */
export const deltaText = (d: number | null | undefined) => (d === null || d === undefined || Math.abs(d) < 1 << 20 ? '' : `${d > 0 ? '+' : '−'}${size(Math.abs(d))}`)

/** `12 %`, `4.5 %`. */
export const share = (part: number, whole: number) => {
  const p = whole ? (100 * part) / whole : 0
  return `${p < 10 ? p.toFixed(1) : Math.round(p)} %`
}

/** `1.8 M`, `412 k`, `37`. */
export const count = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)} M` : n >= 1e4 ? `${Math.round(n / 1e3)} k` : String(n))

/** Each kind's hue, and its name. */
export const KINDS: Record<DiskKind, { hue: number; label: string }> = {
  build: { hue: 22, label: t('build') },
  cache: { hue: 48, label: t('cache') },
  vm: { hue: 270, label: t('VMs and images') },
  media: { hue: 330, label: t('media') },
  models: { hue: 172, label: t('models') },
  apps: { hue: 212, label: t('apps') },
  docs: { hue: 125, label: t('documents') },
  archives: { hue: 32, label: t('archives') },
  code: { hue: 195, label: t('code') },
  other: { hue: 40, label: t('other') },
}

/** A hue blended with the theme's surface: readable on light and dark themes alike. */
export const hueColor = (hue: number, sat = 62, depth = 0) => `color-mix(in oklab, hsl(${hue} ${sat}% ${50 + depth * 4}%) ${78 - depth * 6}%, var(--surface))`
export const kindColor = (k: DiskKind) => hueColor(KINDS[k].hue, k === 'other' ? 12 : 62)
/** The hues of the folders when colored by folder: one per top-level one. */
const FOLDER_HUES = [210, 22, 150, 280, 340, 45, 190, 100, 0, 250, 300, 70]
export const folderColor = (i: number, depth: number) => hueColor(FOLDER_HUES[i % FOLDER_HUES.length], 55, Math.min(3, depth))

/** What each reclaim group is, how it is freed, and its button. */
export const RECLAIM: Record<DiskReclaim, { label: string; how: string; act: string }> = {
  docker: { label: t('Docker’s disk image'), how: t('docker system prune -a: unused images, stopped containers, build cache (volumes kept)'), act: t('Prune') },
  rust_target: { label: t('Rust target/'), how: t('removed as cargo clean does, worktrees included; rebuilt at the next build'), act: t('Remove') },
  node_modules: { label: 'node_modules', how: t('installed again by the package manager'), act: t('Remove') },
  xcode: { label: t('Xcode DerivedData'), how: t('rebuilt at the next build'), act: t('Empty') },
  simulators: { label: t('iOS simulators'), how: t('xcrun simctl delete unavailable: those no Xcode can run'), act: t('Clean up') },
  caches: { label: t('Caches'), how: t('the applications’ and tools’ caches, filled again as they are used'), act: t('Empty') },
  python_env: { label: t('Python virtual environments'), how: t('.venv, made again by uv sync or pip install'), act: t('Remove') },
  js_build: { label: t('JavaScript builds'), how: t('.next, .nuxt, .svelte-kit, .turbo…: rebuilt at the next build'), act: t('Remove') },
  terraform: { label: '.terraform', how: t('providers and modules, downloaded again by terraform init'), act: t('Remove') },
  pods: { label: 'CocoaPods', how: t('installed again by pod install'), act: t('Remove') },
  trash: { label: t('Trash'), how: t('emptied for good'), act: t('Empty') },
}

/** A rectangle of the treemap. */
export interface Rect<T> {
  item: T
  x: number
  y: number
  w: number
  h: number
}

/**
 * `items` (biggest first) laid out in the `w`×`h` box at (`x`, `y`), each an area proportional to
 * its `value`, as square as can be (Bruls, Huizing and van Wijk's squarified treemap).
 */
export function squarify<T>(items: T[], value: (item: T) => number, x: number, y: number, w: number, h: number): Rect<T>[] {
  const out: Rect<T>[] = []
  const total = items.reduce((s, i) => s + value(i), 0)
  if (!total || w <= 0 || h <= 0) return out
  const scale = (w * h) / total
  const list = items.map((item) => ({ item, v: value(item) * scale })).filter((o) => o.v > 0)
  const worst = (row: { v: number }[], side: number) => {
    let sum = 0
    let max = 0
    let min = Infinity
    for (const r of row) {
      sum += r.v
      max = Math.max(max, r.v)
      min = Math.min(min, r.v)
    }
    return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min))
  }
  let i = 0
  while (i < list.length) {
    const side = Math.min(w, h)
    let row = [list[i]]
    let best = worst(row, side)
    let j = i + 1
    while (j < list.length) {
      const next = [...row, list[j]]
      const r = worst(next, side)
      if (r > best) break
      row = next
      best = r
      j++
    }
    const sum = row.reduce((s, r) => s + r.v, 0)
    if (w >= h) {
      const cw = sum / h
      let yy = y
      for (const r of row) {
        const rh = r.v / cw
        out.push({ item: r.item, x, y: yy, w: cw, h: rh })
        yy += rh
      }
      x += cw
      w -= cw
    } else {
      const rh = sum / w
      let xx = x
      for (const r of row) {
        const rw = r.v / rh
        out.push({ item: r.item, x: xx, y, w: rw, h: rh })
        xx += rw
      }
      y += rh
      h -= rh
    }
    i = j
  }
  return out
}

/** The nodes from `root` down to `id` (both included), or null when `id` is not under it. */
export function chain(root: DiskNode, id: number): DiskNode[] | null {
  if (root.id === id) return [root]
  for (const c of root.children ?? []) {
    const rest = chain(c, id)
    if (rest) return [root, ...rest]
  }
  return null
}
