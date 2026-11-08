/**
 * What differs between macOS, Linux and Windows on the frontend side: the shortcut modifier
 * (⌘ on macOS, Ctrl elsewhere), how keys are written, the name of the file manager.
 * Texts are written the macOS way (`⌘S`, `⌘↵`, Finder); `keys()` and `FILE_MANAGER` adapt them.
 */

const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent
export const isMac = /Macintosh|Mac OS X/.test(ua)
export const isWindows = /Windows/.test(ua)

/** Name of the system's file manager, for "Reveal in …" buttons. */
export const FILE_MANAGER = isMac ? 'Finder' : isWindows ? 'Explorer' : 'Files'

/** Typing in an agent's terminal: its keys go to the program. */
export const inTerminal = (e: Event) => !!(e.target as HTMLElement | null)?.closest?.('.xterm')

/**
 * An app shortcut chord: ⌘ on macOS. Elsewhere Ctrl, and Ctrl+Shift inside an agent's
 * terminal, where Ctrl belongs to the program (Ctrl+C, Ctrl+B…).
 */
export function chord(e: KeyboardEvent): boolean {
  if (isMac) return e.metaKey
  return e.ctrlKey && !e.metaKey && (e.shiftKey || !inTerminal(e))
}

/** Shift added only to leave the terminal (Linux, Windows): it does not count as a modifier. */
export const extraShift = (e: KeyboardEvent) => !isMac && e.shiftKey && inTerminal(e)

const CODES: Record<string, string> = { BracketLeft: '[', BracketRight: ']', Period: '.', Comma: ',', Slash: '?', Equal: '=', Minus: '-' }

/**
 * The key of a chord, lower case. When Shift was only added to leave the terminal, the key
 * comes from the physical key: Ctrl+Shift+[ gives "{", which would match nothing.
 */
export function chordKey(e: KeyboardEvent): string {
  if (!extraShift(e)) return e.key.toLowerCase()
  if (CODES[e.code]) return CODES[e.code]
  const m = /^(?:Key|Digit)(.)$/.exec(e.code)
  return m ? m[1].toLowerCase() : e.key.toLowerCase()
}

/**
 * The digit of a page shortcut (⌘1…⌘9, Ctrl+1… elsewhere), or `''`. Read from the physical key, so
 * ⌘1 works on AZERTY too (unshifted, it types `&`); with ⌘ alone: ⌘⇧3/4/5 (screenshots), ⌃1…
 * (switch desktop) and ⌥⌘1… (Profile menu) stay the system's and the menu's.
 */
export function pageDigit(e: KeyboardEvent): string {
  if (!chord(e) || e.altKey || (isMac ? e.ctrlKey : e.metaKey) || (e.shiftKey && !extraShift(e))) return ''
  return /^Digit([1-9])$/.exec(e.code)?.[1] ?? ''
}

/**
 * A text written with macOS key symbols, for this system: `⌘S` → `Ctrl+S`, `⌘↵` → `Ctrl+Enter`,
 * `⌥` → `Alt+`, `⇧` → `Shift+`, `⌃` → `Ctrl+`, "the Finder" → "the file manager". Unchanged on macOS.
 */
export function keys(text: string): string {
  if (isMac) return text
  return text
    // `-clic` in French.
    .replace(/⌘-clic(k?)/g, 'Ctrl-clic$1')
    .replace(/⇧-clic(k?)/g, 'Shift-clic$1')
    .replace(/⌥-clic(k?)/g, 'Alt-clic$1')
    .replace(/↩|↵/g, 'Enter')
    // A modifier named alone ("hold ⌥") takes no "+".
    .replace(/[⌘⌃](?=[\s)`]|$)/g, 'Ctrl')
    .replace(/⌥(?=[\s)`]|$)/g, 'Alt')
    .replace(/⇧(?=[\s)`]|$)/g, 'Shift')
    .replace(/⌘|⌃/g, 'Ctrl+')
    .replace(/⌥/g, 'Alt+')
    .replace(/⇧/g, 'Shift+')
    .replace(/\bCmd\+/g, 'Ctrl+')
    .replace(/\b([Tt])he Finder\b/g, '$1he file manager')
    .replace(/\b([Ll])e Finder\b/g, '$1e gestionnaire de fichiers')
    .replace(/\bFinder\b/g, FILE_MANAGER)
}

/** A path under the home folder, with `~` for it (`C:\Users\me` on Windows). */
export const tildePath = (path: string) =>
  path
    .replace(/^\/Users\/[^/]+/, '~')
    .replace(/^\/home\/[^/]+/, '~')
    .replace(/^[A-Za-z]:[\\/]Users[\\/][^\\/]+/i, '~')

/** The last name of a path, `/` or `\` (Windows) between its parts, a trailing one ignored. */
export const baseName = (path: string) => path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? ''

/** A path's folder, without its last name; empty without a folder. */
export const dirName = (path: string) => {
  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
  return cut < 0 ? '' : path.slice(0, cut)
}
