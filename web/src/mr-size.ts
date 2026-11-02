import { t, tn } from './i18n.svelte'
import { alertSettings } from './conventions'
import type { MrSize } from './types'

/** A short line count: 840, 1.2k, 12k. */
export function lines(n: number): string {
  if (n < 1000) return String(n)
  return n < 10_000 ? `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k` : `${Math.round(n / 1000)}k`
}

/** Five GitHub-style cells: the share of additions in green, of deletions in red. */
export function blocks(s: MrSize): ('add' | 'del' | 'none')[] {
  const total = s.additions + s.deletions
  if (!total) return Array(5).fill('none')
  const add = Math.round((s.additions / total) * 5)
  return Array.from({ length: 5 }, (_, i) => (i < add ? 'add' : 'del'))
}

/** More changed lines than `[alerts] large_mr_lines`: a large MR. */
export const isLarge = (s: MrSize) => s.additions + s.deletions > alertSettings().large_mr_lines

export const sizeTitle = (s: MrSize) =>
  `+${s.additions} −${s.deletions} · ${tn(s.files, '{n} file', '{n} files')}${isLarge(s) ? ` · ${t('large (over {n} lines)', { n: alertSettings().large_mr_lines })}` : ''}`
