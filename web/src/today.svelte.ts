import { dayKey } from './calendar'

/**
 * Today, as the views read it: a state, so that what derives from it follows when a new day begins
 * — midnight passed with Prism open, or the computer woken the next morning. Read again every
 * minute (a timer held during a sleep fires on waking) and when the window comes back.
 */
export const today = $state({ key: dayKey(new Date()) })
export const todayKey = () => today.key
/** The date read again: `today` moves when it changed. */
export function checkDay() {
  const now = dayKey(new Date())
  if (now !== today.key) today.key = now
}
if (typeof window !== 'undefined') {
  // One clock per page: a module loaded again (hot reload, tests) takes the former one's place.
  const page = window as unknown as { prismDayClock?: () => void }
  page.prismDayClock?.()
  const back = () => document.hidden || checkDay()
  const timer = setInterval(checkDay, 60_000)
  window.addEventListener('focus', checkDay)
  window.addEventListener('pageshow', checkDay)
  document.addEventListener('visibilitychange', back)
  page.prismDayClock = () => {
    clearInterval(timer)
    window.removeEventListener('focus', checkDay)
    window.removeEventListener('pageshow', checkDay)
    document.removeEventListener('visibilitychange', back)
  }
}
