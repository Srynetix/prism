import { dayKey } from '../../calendar'

/**
 * The world outside its window (`[pet] seasons`): the hour's light, the season, and the few days
 * that are not like the others.
 */

export type Daypart = 'dawn' | 'day' | 'dusk' | 'night'
export type Season = 'winter' | 'spring' | 'summer' | 'autumn'
export type Special = 'birthday' | 'sprint-end' | 'no-deploy' | 'halloween' | 'christmas' | 'new-year'

export function daypart(now: number): Daypart {
  const h = new Date(now).getHours()
  return h >= 6 && h < 9 ? 'dawn' : h >= 9 && h < 18 ? 'day' : h >= 18 && h < 21 ? 'dusk' : 'night'
}

/** The season of the northern hemisphere, by the month. */
export function season(now: number): Season {
  const m = new Date(now).getMonth()
  return m === 11 || m < 2 ? 'winter' : m < 5 ? 'spring' : m < 8 ? 'summer' : 'autumn'
}

/** Today's special days: its birthday (a year since it hatched), the sprint's last day, Friday afternoon, a few holidays. */
export function specials(now: number, born: number, sprintEnd?: string | null): Special[] {
  const d = new Date(now)
  const b = new Date(born)
  const out: Special[] = []
  if (d.getFullYear() > b.getFullYear() && d.getMonth() === b.getMonth() && d.getDate() === b.getDate()) out.push('birthday')
  if (sprintEnd && dayKey(new Date(sprintEnd)) === dayKey(d)) out.push('sprint-end')
  if (d.getDay() === 5 && d.getHours() >= 15) out.push('no-deploy')
  const md = `${d.getMonth() + 1}-${d.getDate()}`
  if (md === '10-31') out.push('halloween')
  if (['12-24', '12-25', '12-26'].includes(md)) out.push('christmas')
  if (md === '1-1') out.push('new-year')
  return out
}
