import { t } from '../../i18n.svelte'
import type { Pressure } from '../../bindings/Pressure'
import type { SystemState } from '../../bindings/SystemState'
import type { DiskLimits } from '../../bindings/DiskLimits'

/** The System block's state: Rust's types (`crates/blocks/src/system`), generated in `bindings/`. */
export type { Pressure, SystemState, DiskLimits }

/** CPU at 90 % or more this long, in seconds, is an alert; more than this % of handshakes lost, too. */
export const HOT_SECONDS = 5 * 60
export const LOSS = 5

export type Tone = 'ok' | 'warn' | 'ko'

/** Prism's defaults, for a state published before the limits were. */
const DISK_LIMITS: DiskLimits = { disk_warn_gb: 25, disk_warn_percent: 10, disk_alert_gb: 10, disk_alert_percent: 5 }

type Disk = SystemState['disks'][number]
/** A disk's free space: red under `[system] disk_alert_percent` or `disk_alert_gb` (5 %, 10 GB), orange under `disk_warn_…` (10 %, 25 GB). */
export function diskTone(d: Disk, limits: DiskLimits = DISK_LIMITS): Tone {
  const pct = d.total ? (d.available / d.total) * 100 : 100
  const under = (gb: number, percent: number) => (percent > 0 && pct < percent) || (gb > 0 && d.available < gb * 2 ** 30)
  if (under(limits.disk_alert_gb, limits.disk_alert_percent)) return 'ko'
  if (under(limits.disk_warn_gb, limits.disk_warn_percent)) return 'warn'
  return 'ok'
}

/** The colour of each figure: CPU by its use, memory by macOS's pressure (by its use elsewhere), the network by loss and latency. */
export function tones(s: SystemState): { cpu: Tone; memory: Tone; network: Tone; disk: Tone } {
  const used = s.memory.total ? s.memory.used / s.memory.total : 0
  const memory: Tone = s.memory.pressure === 'critical' ? 'ko' : s.memory.pressure === 'warn' ? 'warn' : s.memory.pressure === 'normal' ? 'ok' : used > 0.92 ? 'ko' : used > 0.8 ? 'warn' : 'ok'
  const cpu: Tone = s.cpu.hot_seconds >= HOT_SECONDS ? 'ko' : s.cpu.usage >= 80 ? 'warn' : 'ok'
  const ms = s.network.latency_ms
  const network: Tone = s.network.loss > LOSS || (ms === null && s.network.hosts.length > 0) ? 'ko' : s.network.loss > 0 || (ms ?? 0) > 150 ? 'warn' : 'ok'
  const disk: Tone = s.disks?.[0] ? diskTone(s.disks[0], s.disk_limits) : 'ok'
  return { cpu, memory, network, disk }
}

/** `29.4 GB`, `840 MB`. */
export function bytes(n: number): string {
  if (n >= 1 << 30) return t('{n} GB', { n: (n / (1 << 30)).toFixed(n >= 100 * (1 << 30) ? 0 : 1) })
  return t('{n} MB', { n: Math.round(n / (1 << 20)) })
}

/** `3.1 MB/s`, `410 kB/s`. */
export function rate(n: number): string {
  return n >= 1e6 ? t('{n} MB/s', { n: (n / 1e6).toFixed(1) }) : t('{n} kB/s', { n: Math.round(n / 1e3) })
}

/** The points of a sparkline in a `w`×`h` box, `max` at the top (absent values skipped). */
export function spark(values: (number | null)[], w: number, h: number, max: number): string {
  if (values.length < 2) return ''
  const step = w / (values.length - 1)
  return values
    .map((v, i) => (v === null ? null : `${(i * step).toFixed(1)},${(h - (Math.min(v, max) / max) * (h - 2) - 1).toFixed(1)}`))
    .filter(Boolean)
    .join(' ')
}
