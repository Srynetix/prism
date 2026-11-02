import { locale } from './locale'

/** A day's key, `YYYY-MM-DD`, in local time. */
export const dayKey = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`

/** A day of a month's grid: its key (`YYYY-MM-DD`), its number, and whether it belongs to the month shown. */
export interface GridDay {
  key: string
  day: number
  inMonth: boolean
}

/**
 * First day of the week in the window's locale: 1 Monday … 7 Sunday (`Intl.Locale` week info,
 * a getter or a method depending on the engine); Monday when the engine does not say.
 */
export function weekStart(tag = locale()): number {
  try {
    const l = new Intl.Locale(tag ?? new Intl.DateTimeFormat().resolvedOptions().locale) as Intl.Locale & { getWeekInfo?: () => { firstDay: number }; weekInfo?: { firstDay: number } }
    return l.getWeekInfo?.().firstDay ?? l.weekInfo?.firstDay ?? 1
  } catch {
    return 1
  }
}

/**
 * The weeks of a month (`month` 0–11), whole: the days before its 1st and after its last day
 * come from the months around it, so that every row has seven days.
 */
export function monthGrid(year: number, month: number, firstDay = 1): GridDay[][] {
  const first = new Date(year, month, 1, 12)
  // Days of the previous month before the 1st: 0 to 6, from the week's first day.
  const lead = (first.getDay() - (firstDay % 7) + 7) % 7
  const start = new Date(year, month, 1 - lead, 12)
  const weeks: GridDay[][] = []
  const d = new Date(start)
  do {
    const week: GridDay[] = []
    for (let i = 0; i < 7; i++) {
      week.push({ key: dayKey(d), day: d.getDate(), inMonth: d.getMonth() === month })
      d.setDate(d.getDate() + 1)
    }
    weeks.push(week)
  } while (d.getMonth() === month)
  return weeks
}

/** The key `days` days after (or before) `key`. */
export function addDays(key: string, days: number): string {
  const d = new Date(`${key}T12:00`)
  d.setDate(d.getDate() + days)
  return dayKey(d)
}
