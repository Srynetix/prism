import type { MrOption } from './linkify'
import { openLink } from './links'
import { openMr } from './mr-view.svelte'

/**
 * An agent's `!12` (Chat, Terminal) when it works in several repositories with such an MR: the
 * choice offered at the pointer (MrChoice). With one MR, it opens at once.
 */
export const mrChoice = $state<{ options: MrOption[] | null; x: number; y: number }>({ options: null, x: 0, y: 0 })

/** Its panel when Prism knows it, otherwise its page on the forge. */
export function openMrOption(o: MrOption) {
  if (o.url) openLink(o.url)
  else openMr(o.ref)
}

/** One MR opens; several are offered where `at` was clicked. */
export function openMrOptions(options: MrOption[], at?: { clientX: number; clientY: number }) {
  if (options.length === 1) return openMrOption(options[0])
  if (!options.length) return
  Object.assign(mrChoice, { options, x: at?.clientX ?? innerWidth / 2, y: at?.clientY ?? innerHeight / 2 })
}

export const closeMrChoice = () => (mrChoice.options = null)
