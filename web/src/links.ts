import { isDesktop } from './bridge'
import { openUrl } from '@tauri-apps/plugin-opener'

/**
 * Open a link outside Prism: in the app, the browser (or the `claude://` app) through the opener
 * plugin (`obsidian://` opens Obsidian), `window.open` doing nothing there; in the browser preview, a new tab.
 */
export function openLink(href: string) {
  if (isDesktop()) openUrl(href).catch((err) => console.error('open', href, err))
  else window.open(href, '_blank', 'noopener')
}

/**
 * `onclick` of a link placed on a clickable line: it opens without folding or opening the line.
 * Stopping propagation would deprive the link of the global handler (main.ts): it is opened here.
 */
export function linkOnly(e: MouseEvent) {
  e.stopPropagation()
  const href = (e.currentTarget as HTMLAnchorElement | null)?.getAttribute('href')
  if (!href || !/^(https?|claude|obsidian):/i.test(href)) return
  e.preventDefault()
  openLink(href)
}
