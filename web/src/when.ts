import { locale } from './locale'
import { t } from './i18n.svelte'
/**
 * A due time typed in plain words, English or French, for the reminders:
 * - a delay: `+20m`, `20 min`, `in 2h`, `dans 1h30`, `+1d`;
 * - a time: `14:30`, `14h30`, `14h`, `9`, `2pm`, `9:15am`;
 * - a day, then optionally a time: `today`, `tomorrow`, `demain`, `aujourd'hui`, a weekday
 *   (`mon`, `lundi`, `fri`…, the next one), a date `12/10` or `12/10/2026` (day first);
 *   without a time, 9:00.
 * A time alone that has already passed today is for tomorrow. Returns null when nothing is understood.
 */
export function parseWhen(input: string, now = new Date()): Date | null {
  let s = input.trim().toLowerCase().replace(/\s+/g, ' ')
  if (!s) return null

  // A delay: +20m, 20 min, in 2h, dans 1h30, +1d.
  const delay = /^(?:\+|in |dans )?\s*(?:(\d+)\s*(?:d|j|days?|jours?)\s*)?(?:(\d+)\s*(?:h|hours?|heures?)\s*)?(?:(\d+)\s*(?:m|min|mins|minutes?)?)?$/.exec(s)
  // Without "+", "in" or "dans", hours alone are a time ("14h"): only minutes or days make a delay.
  const prefixed = /^(\+|in |dans )/.test(s)
  if (delay && (prefixed || /\d\s*(?:m|min|mins|minutes?|d|j|days?|jours?)$/.test(s)) && (delay[1] || delay[2] || delay[3])) {
    const [d, h, m] = [delay[1], delay[2], delay[3]].map((x) => Number(x ?? 0))
    // "+1h30": the trailing number is minutes; "+20" alone too.
    const ms = ((d * 24 + h) * 60 + m) * 60_000
    if (ms > 0) return new Date(now.getTime() + ms)
  }

  // The day.
  const at = new Date(now)
  let dayGiven = false
  const take = (re: RegExp): RegExpExecArray | null => {
    const m = re.exec(s)
    if (m) s = (s.slice(0, m.index) + s.slice(m.index + m[0].length)).trim()
    return m
  }
  if (take(/\b(?:today|aujourd'hui|auj)\b/)) dayGiven = true
  else if (take(/\b(?:tomorrow|tmrw|demain)\b/)) {
    at.setDate(at.getDate() + 1)
    dayGiven = true
  } else {
    const days = [['sun', 'dim'], ['mon', 'lun'], ['tue', 'mar'], ['wed', 'mer'], ['thu', 'jeu'], ['fri', 'ven'], ['sat', 'sam']]
    const wd = take(/\b(sun|mon|tue|wed|thu|fri|sat|dim|lun|mar|mer|jeu|ven|sam)[a-z]*\.?(?=\s|$)/)
    if (wd) {
      const target = days.findIndex((names) => names.includes(wd[1]))
      at.setDate(at.getDate() + (((target - at.getDay() + 7) % 7) || 7))
      dayGiven = true
    } else {
      const date = take(/\b(\d{1,2})\/(\d{1,2})(?:\/(\d{2,4}))?\b/)
      if (date) {
        const year = date[3] ? (date[3].length === 2 ? 2000 + Number(date[3]) : Number(date[3])) : at.getFullYear()
        at.setFullYear(year, Number(date[2]) - 1, Number(date[1]))
        if (at.getDate() !== Number(date[1])) return null
        // A date already past this year, without a year: next year's.
        if (!date[3] && at.getTime() < now.getTime() - 86_400_000) at.setFullYear(year + 1)
        dayGiven = true
      }
    }
  }
  s = s.replace(/^(?:at|à|a)\s+/, '').trim()

  // The time: 14:30, 14h30, 14h, 9, 2pm, 9:15am.
  if (!s) {
    if (!dayGiven) return null
    at.setHours(9, 0, 0, 0)
    return at
  }
  const t = /^(\d{1,2})(?:[:h.](\d{2})?)?\s*(am|pm)?$/.exec(s)
  if (!t) return null
  let hour = Number(t[1])
  const minute = Number(t[2] ?? 0)
  if (t[3] === 'pm' && hour < 12) hour += 12
  if (t[3] === 'am' && hour === 12) hour = 0
  if (hour > 23 || minute > 59) return null
  at.setHours(hour, minute, 0, 0)
  if (!dayGiven && at.getTime() <= now.getTime()) at.setDate(at.getDate() + 1)
  return at
}

/** "Tue 14:30", "today 14:30", "Tue 14 Oct 9:00" beyond a week. */
export function whenLabel(at: Date, now = new Date()): string {
  const time = at.toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
  if (at.toDateString() === now.toDateString()) return t('today {time}', { time })
  const tomorrow = new Date(now)
  tomorrow.setDate(now.getDate() + 1)
  if (at.toDateString() === tomorrow.toDateString()) return t('tomorrow {time}', { time })
  const far = at.getTime() - now.getTime() > 6 * 86_400_000
  return `${at.toLocaleDateString(locale(), { weekday: 'short', day: far ? 'numeric' : undefined, month: far ? 'short' : undefined })} ${time}`
}
