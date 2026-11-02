import { invoke, isDesktop, listen } from './bridge'
import { t } from './i18n.svelte'

/**
 * Brief confirmation at the bottom of the window ("Link to ABC-4688 copied"): only one at a time,
 * cleared after a few seconds, with at most one button (`action`). Displayed by Toast.svelte.
 */
export const toastState = $state({ message: '', id: 0, action: null as ToastAction | null })

/** A button in the toast, what it does: Open it. */
export interface ToastAction {
  label: string
  run: () => void
}

let timer: ReturnType<typeof setTimeout> | undefined

export function toast(message: string, ms = 2200, action: ToastAction | null = null) {
  clearTimeout(timer)
  toastState.message = message
  toastState.action = action
  toastState.id++
  timer = setTimeout(() => (toastState.message = ''), ms)
}

// What Prism tells of its own work: an agent's chain, a prompt held for it and sent.
if (isDesktop()) void listen<{ text: string; ms: number }>('notice', ({ payload }) => toast(payload.text, payload.ms))

/**
 * Puts text on the clipboard. On the Mac, through the core: the webview refuses
 * `navigator.clipboard` outside a click's own moment (`NotAllowedError`), as for an action of a
 * menu, a shortcut or the palette, or after an await. On a phone, its own clipboard.
 */
export async function writeClipboard(text: string) {
  if (isDesktop()) await invoke('copy_text', { text })
  else await navigator.clipboard.writeText(text)
}

/** Copies to the clipboard, then says so (or why not). */
export async function copy(text: string, message: string) {
  try {
    await writeClipboard(text)
    toast(message)
  } catch (e) {
    toast(t('Could not copy: {error}', { error: String(e) }), 5000)
  }
}
