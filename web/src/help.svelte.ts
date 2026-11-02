import { closeHelp, helpView } from './help-view.svelte'
import { closeSettings, openSettings } from './settings.svelte'
import { closeAgents, openAgents } from './agents.svelte'
import { showBlock, pref, setPref } from './layout.svelte'
import type { BlockId } from './views'
import { keys } from './keys.svelte'
import { toggleConsole } from './console.svelte'
import { wizard } from './wizard.svelte'

export function openHelp(guide?: string, query?: string) {
  closeSettings()
  closeAgents()
  // An unknown one (a link gone stale): the Help view shows its first guide.
  if (guide) helpView.guide = guide
  if (query !== undefined) helpView.query = query
  helpView.open = true
  seeTour()
}
export const toggleHelp = () => (helpView.open ? closeHelp() : openHelp())

/** A `prism:` link of a guide: `block/<id>`, `help/<id>`, `settings/<section>`, `agents`, `wizard`, `keys`, `console`. */
export function runAction(target: string) {
  const [what, arg] = target.split('/')
  if (what === 'help') return openHelp(arg)
  closeHelp()
  if (what === 'block') showBlock(arg as BlockId)
  else if (what === 'settings') openSettings(arg)
  else if (what === 'agents') openAgents()
  else if (what === 'wizard') wizard.open = true
  else if (what === 'keys') keys.help = true
  else if (what === 'console') toggleConsole(true)
}

/** First-launch banner, "New to Prism?": shown until Help is opened or the banner dismissed. */
export const tour = $state({ seen: pref<boolean>('help.tour', false) })
export function seeTour() {
  if (tour.seen) return
  tour.seen = true
  setPref('help.tour', true)
}
