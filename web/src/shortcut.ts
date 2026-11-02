import { extraShift, isMac } from './platform'

/**
 * Keyboard shortcut written in plain text in the config: a character (`` ` ``, `²`, `@`), a key
 * name (`F1`, `Space`, `Backquote`, `IntlBackslash`) or a combination (`Cmd+K`, `Ctrl+Space`,
 * `Alt+T`). The key is compared to the produced character (`e.key`) then to the physical key
 * (`e.code`), ignoring case: `Backquote` works whatever the layout.
 * `Cmd` (`Command`, `⌘`, `Mod`, `CmdOrCtrl`) is ⌘ on macOS and Ctrl on Linux and Windows, so a
 * config written on a Mac works elsewhere; `Meta` (`Super`, `Win`) is always the meta key.
 */
export interface Shortcut {
  key: string
  meta: boolean
  ctrl: boolean
  alt: boolean
  shift: boolean
}

const MOD = isMac ? 'meta' : 'ctrl'
const MODIFIERS: Record<string, keyof Omit<Shortcut, 'key'>> = {
  cmd: MOD, command: MOD, '⌘': MOD, mod: MOD, cmdorctrl: MOD,
  meta: 'meta', super: 'meta', win: 'meta',
  ctrl: 'ctrl', control: 'ctrl', '⌃': 'ctrl',
  alt: 'alt', option: 'alt', opt: 'alt', '⌥': 'alt',
  shift: 'shift', '⇧': 'shift',
}

export function parseShortcut(text: string): Shortcut {
  const sc: Shortcut = { key: '', meta: false, ctrl: false, alt: false, shift: false }
  // "+" alone is a key, not a separator.
  const parts = text.trim() === '+' ? ['+'] : text.split('+').map((p) => p.trim()).filter(Boolean)
  for (const p of parts) {
    const m = MODIFIERS[p.toLowerCase()]
    if (m && parts.length > 1) sc[m] = true
    else sc.key = p
  }
  return sc
}

const norm = (k: string) => (k.toLowerCase() === 'space' ? ' ' : k.toLowerCase())

export function matches(e: KeyboardEvent, sc: Shortcut): boolean {
  if (!sc.key || e.metaKey !== sc.meta || e.ctrlKey !== sc.ctrl || e.altKey !== sc.alt) return false
  // Shift only counts if the shortcut asks for it with a modifier: `@` may require Shift depending on the layout.
  // Shift added only to leave an agent's terminal (Linux, Windows) is tolerated.
  if ((sc.meta || sc.ctrl || sc.alt) && e.shiftKey !== sc.shift && !(extraShift(e) && !sc.shift)) return false
  const k = norm(sc.key)
  // The physical key too: ⌥T gives "†", but stays KeyT.
  const code = e.code.toLowerCase()
  return norm(e.key) === k || code === k || code === `key${k}` || code === `digit${k}` || (k === ' ' && e.code === 'Space') || (k === '`' && e.code === 'Backquote')
}

/** With ⌘ or ⌃, the shortcut also works while typing; without, it would be a typed character. */
export const worksWhileTyping = (sc: Shortcut) => sc.meta || sc.ctrl

/** Label: macOS-style on macOS (`⌘K`, `⌃Space`, `` ` ``), `Ctrl+Alt+Shift+K` elsewhere. */
export function shortcutLabel(sc: Shortcut): string {
  if (!isMac) return [sc.ctrl && 'Ctrl', sc.alt && 'Alt', sc.shift && 'Shift', sc.meta && 'Super', sc.key.length === 1 ? sc.key.toUpperCase() : sc.key].filter(Boolean).join('+')
  return `${sc.ctrl ? '⌃' : ''}${sc.alt ? '⌥' : ''}${sc.shift ? '⇧' : ''}${sc.meta ? '⌘' : ''}${sc.key.length === 1 ? sc.key.toUpperCase() : sc.key}`
}
