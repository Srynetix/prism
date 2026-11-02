import { getContext, setContext } from 'svelte'

/**
 * The polls of the agent panels (Chat, Plan, Activity, Diff and Web tabs, shells) wait while what holds
 * them is hidden: the Agents view stays mounted under another view, Settings or Help (its
 * terminals stay attached), and the window or the phone's page may be hidden. They read again
 * at once when shown. Outside a host that says (`setPollsShown`), they run whenever the page shows.
 */
const page = $state({ hidden: typeof document !== 'undefined' && document.hidden })
if (typeof document !== 'undefined') document.addEventListener('visibilitychange', () => (page.hidden = document.hidden))

const HOST = Symbol('polls-host')

/** For a host's children: whether it shows (read reactively). */
export function setPollsShown(shown: () => boolean) {
  setContext(HOST, shown)
}

/** In a component's setup: a reactive test of whether its polls run now. */
export function pollsShown(): () => boolean {
  const host = getContext<(() => boolean) | undefined>(HOST)
  return () => !page.hidden && (host?.() ?? true)
}
