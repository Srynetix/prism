import { knownMr, type MrOption } from './linkify'
import { openLink } from './links'
import { live } from './live.svelte'
import { openMr } from './mr-view.svelte'

/**
 * An agent's `!12` (Chat, Terminal) when it works in several repositories with MRs: the choice
 * offered at `x`, `y` (MrChoice), those Prism knows first. With one repository, it opens at once.
 */
export const mrChoice = $state<{ options: MrOption[] | null; x: number; y: number }>({ options: null, x: 0, y: 0 })

/** Its panel when Prism knows it, otherwise its page on the forge. */
export function openMrOption(o: MrOption) {
  if (o.url && !knownMr(o, live.snapshot)) openLink(o.url)
  else openMr(o.ref)
}

/** One MR opens; several are offered at `at` (the link's corner, or the pointer). */
export function openMrOptions(options: MrOption[], at?: { x: number; y: number }) {
  if (options.length === 1) return openMrOption(options[0])
  if (!options.length) return
  const s = live.snapshot
  const sorted = [...options].sort((a, b) => Number(!!knownMr(b, s)) - Number(!!knownMr(a, s)))
  // After the gesture's click: a link opened on mouseup (the Terminal's) would close it at once.
  setTimeout(() => Object.assign(mrChoice, { options: sorted, x: at?.x ?? innerWidth / 2, y: at?.y ?? innerHeight / 2 }))
}

export const closeMrChoice = () => (mrChoice.options = null)
