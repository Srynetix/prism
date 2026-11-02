import { t } from './i18n.svelte'

export const minutesSince = (iso: string | null | undefined) =>
  iso ? (Date.now() - new Date(iso).getTime()) / 60000 : 0

export function ago(iso: string | null | undefined): string {
  if (!iso) return '—'
  const min = Math.round(minutesSince(iso))
  if (min < 1) return t('now')
  if (min < 60) return t('{n}m', { n: min })
  if (min < 1440) return t('{n}h', { n: Math.round(min / 60) })
  return t('{n}d', { n: Math.round(min / 1440) })
}

/** Short duration: `45s`, `3m`, `1h05`. */
export function span(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000))
  if (s < 60) return `${s}s`
  const m = Math.round(s / 60)
  if (m < 60) return t('{n}m', { n: m })
  return `${Math.floor(m / 60)}h${String(m % 60).padStart(2, '0')}`
}

/** Time spent between two instants; as long as `end` is missing, up to now. */
export function elapsed(start: string | null, end: string | null, now = Date.now()): number | null {
  if (!start) return null
  return (end ? new Date(end).getTime() : now) - new Date(start).getTime()
}

export function cpu(millis: number): string {
  return millis < 1000 ? `${millis}m` : `${(millis / 1000).toFixed(millis < 10000 ? 1 : 0)}`
}

export function mem(bytes: number): string {
  const mi = bytes / 1048576
  return mi < 1024 ? `${Math.round(mi)}Mi` : `${(mi / 1024).toFixed(1)}Gi`
}

/** `1h30`, `1h`, `45m`, `45`, `1.5` (hours if decimal) → minutes. */
export function parseDuration(v: string): number | null {
  const t = v.trim().toLowerCase().replace(',', '.')
  let m = t.match(/^(\d+)h(?:(\d{1,2})m?)?$/)
  if (m) return Number(m[1]) * 60 + Number(m[2] ?? 0)
  m = t.match(/^(\d+)m(?:in)?$/)
  if (m) return Number(m[1])
  if (/^\d+\.\d+$/.test(t)) return Math.round(Number(t) * 60)
  if (/^\d+$/.test(t)) return Number(t)
  return null
}
