import { flushSync } from 'svelte'
import { featureOn, reducedMotion } from './fun.svelte'

/**
 * Going from a page to another (the rail's pages, then Agents): the content slides in the rail's
 * direction (`down` for one further down) with a fade, through the View Transitions API (styles in
 * app.css: `view-main`, `view-title`). Without it, or with "Reduce motion", the change is immediate.
 * Resolved once the new page is in the DOM.
 */
export function pageTransition(apply: () => void, dir: 'up' | 'down'): Promise<void> {
  if (!document.startViewTransition || reducedMotion() || !featureOn('animations')) {
    apply()
    return Promise.resolve()
  }
  document.documentElement.dataset.nav = dir
  // The DOM must be up to date at the end of the callback: the transition snapshots the after state.
  return document.startViewTransition(() => flushSync(apply)).updateCallbackDone.catch(() => {})
}
