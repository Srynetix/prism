import { hasCore } from './bridge'
import { openLink } from './links'
import { refOfUrl, type Ref } from './linkify'
import { live } from './live.svelte'
import { openMr } from './mr-view.svelte'
import { openMrOptions } from './mr-choice.svelte'
import { canPreview, openTicket } from './ticket-view.svelte'

/**
 * Links in an agent's Terminal and Chat: an MR or a ticket opens in its panel in Prism (`refOfUrl`:
 * an MR of a project it reads, a ticket it shows or under the tracker's base); any other link — a
 * local server (`http://portfolio.localhost:3000`), docs… — in the browser.
 */

/**
 * Its panel, if Prism can show it there; an agent's `!12` that may name several MRs, their choice
 * where `at` was clicked.
 */
export function openRef(ref: Ref, at?: MouseEvent): boolean {
  if ('ticket' in ref) {
    if (!canPreview()) return false
    openTicket(ref.ticket)
    return true
  }
  if (!hasCore()) return false
  if ('mrs' in ref) openMrOptions(ref.mrs, at && { x: at.clientX, y: at.clientY })
  else openMr(ref.mr)
  return true
}

/** `url` in Prism when it is an MR or a ticket it shows; true when it did. */
export function openInPrism(url: string): boolean {
  const ref = refOfUrl(url, live.snapshot)
  return !!ref && openRef(ref)
}

/** `url` in Prism (an MR, a ticket), otherwise in the browser; `browser`: in the browser whatever it is. */
export function openUrlOrRef(url: string, browser = false) {
  if (browser || !openInPrism(url)) openLink(url)
}
