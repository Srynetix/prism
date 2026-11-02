import { pref, setPref } from './layout.svelte'
import type { Alert } from './rules'

/**
 * Alerts items marked as seen: acknowledged without necessarily acting. An item is recognized
 * by its type and its text; if the text changes (new count, new state), it becomes new
 * again. A mark expires after a few days, so that a recurring problem does not
 * stay hidden indefinitely.
 */
const TTL_DAYS = 7

const key = (a: Alert) => `${a.kind}|${a.text}`

const loaded = Date.now()
const marks = $state<Record<string, number>>(
  Object.fromEntries(Object.entries(pref<Record<string, number>>('todo.seen', {})).filter(([, at]) => loaded - at < TTL_DAYS * 86_400_000)),
)

export const isSeen = (a: Alert) => key(a) in marks

export function setSeen(a: Alert, seen: boolean) {
  if (seen) marks[key(a)] = Date.now()
  else delete marks[key(a)]
  setPref('todo.seen', { ...marks })
}
