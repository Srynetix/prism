import { invoke, isDesktop, listen } from './bridge'
import { t } from './i18n.svelte'

/**
 * Brief messages at the bottom of the window ("Link to ABC-4688 copied"), shown by Toast.svelte: up
 * to three at once, the oldest leaving first, each cleared after a few seconds (not while the
 * pointer is over it), with at most one button (`action`). A kind each:
 * - `ok`, done (a check): the default;
 * - `info`, nothing done or something under way (« No panel closed lately », « Diagnosis started »);
 * - `error`, failed (never a check): `toastError`, with Try again when it can be;
 * - `undo`, done and reversible (« Worktree archived · Undo »): `toastUndo`.
 */
export type ToastKind = 'ok' | 'info' | 'error' | 'undo'

/** A button in the toast, what it does: Open it. */
export interface ToastAction {
  label: string
  run: () => void
}

export interface Toast {
  id: number
  kind: ToastKind
  message: string
  action: ToastAction | null
  ms: number
}

const MAX = 3
let next = 1
export const toasts = $state({ list: [] as Toast[] })
const timers = new Map<number, { timer: ReturnType<typeof setTimeout>; until: number; left: number }>()

export function toast(message: string, ms = 2200, action: ToastAction | null = null, kind: ToastKind = 'ok') {
  // The same message again, neither with an action: the one shown stays in place, its time renewed.
  // With an action (two Undo), each keeps its own.
  const same = action ? undefined : toasts.list.find((x) => x.message === message && x.kind === kind && !x.action)
  if (same) {
    clearTimer(same.id)
    same.ms = ms
    arm(same.id, ms)
    return
  }
  const item: Toast = { id: next++, kind, message, action, ms }
  const list = [...toasts.list, item]
  // Over the limit, the oldest without an action goes first: an Undo or a Try again stays its time.
  while (list.length > MAX) {
    const i = list.findIndex((x) => !x.action && x !== item)
    list.splice(i < 0 ? 0 : i, 1)
  }
  toasts.list = list
  for (const id of timers.keys()) if (!toasts.list.some((x) => x.id === id)) clearTimer(id)
  arm(item.id, ms)
}

/** Something failed: what, and why. Longer, and with Try again when there is `retry`. */
export const toastError = (message: string, retry?: () => void) => toast(message, 6000, retry ? { label: t('Try again'), run: retry } : null, 'error')

/** Something under way or not done, with no check: « No panel closed lately ». */
export const toastInfo = (message: string, ms = 2600, action: ToastAction | null = null) => toast(message, ms, action, 'info')

/** Done, and can be undone for a few seconds. */
export const toastUndo = (message: string, undo: () => void) => toast(message, 6000, { label: t('Undo'), run: undo }, 'undo')

export function dismissToast(id: number) {
  clearTimer(id)
  toasts.list = toasts.list.filter((x) => x.id !== id)
}

function arm(id: number, ms: number) {
  timers.set(id, { timer: setTimeout(() => dismissToast(id), ms), until: Date.now() + ms, left: ms })
}
function clearTimer(id: number) {
  clearTimeout(timers.get(id)?.timer)
  timers.delete(id)
}

/** The pointer over a toast holds it; leaving it, the time that was left runs again. */
export function holdToast(id: number, held: boolean) {
  const t0 = timers.get(id)
  if (!t0) return
  clearTimeout(t0.timer)
  if (held) timers.set(id, { ...t0, left: Math.max(1200, t0.until - Date.now()) })
  else arm(id, t0.left)
}

// What Prism tells of its own work: an agent's chain, a prompt held for it and sent.
if (isDesktop()) void listen<{ text: string; ms: number }>('notice', ({ payload }) => toast(payload.text, payload.ms))

/**
 * Puts text on the clipboard. On the computer, through the core: the webview refuses
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
    toastError(t('Could not copy: {error}', { error: String(e) }))
  }
}
