import { invoke, isDesktop } from './bridge'
import { rawAppearance } from './fun.svelte'

/**
 * App fonts (`[appearance] font`, `font_mono` and `font_terminal`): the name of a family installed
 * on the Mac, placed in front of the default stack of `--sans` and `--mono` (app.css), which stays the
 * fallback. Empty: Inter (bundled) for text, Recursive Mono (bundled) for code, and
 * RecMonoCasual Nerd Font Mono (bundled) for the terminals (`--mono-term`).
 */
export const SANS_STACK = "'Inter', 'Twemoji', 'Prism Symbols', system-ui, sans-serif"
export const MONO_STACK = "'Recursive Mono', 'Twemoji', 'Prism Symbols', ui-monospace, 'SF Mono', Menlo, monospace"
/** The terminals' default: a Nerd Font, for the prompts' icons, bundled in the app. */
export const TERMINAL_FONT = 'RecMonoCasual Nerd Font Mono'
/** The fonts the app carries (app.css): loaded from it, never looked for on the Mac. */
const BUNDLED = ['Inter', 'Nunito', 'Source Serif 4', 'Jost', 'Recursive Mono', 'Twemoji', 'Prism Symbols', TERMINAL_FONT]

/** Suggestions: macOS's, then the most common among developers. */
export const SANS_FONTS = ['Helvetica Neue', 'Avenir Next', 'Geist', 'IBM Plex Sans', 'Roboto', 'Open Sans', 'Source Sans 3', 'Lato', 'Noto Sans', 'Atkinson Hyperlegible', 'Verdana']
export const MONO_FONTS = ['SF Mono', 'Menlo', 'Monaco', 'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'IBM Plex Mono', 'Source Code Pro', 'Geist Mono', 'Hack', 'Iosevka', 'Victor Mono', 'Ubuntu Mono', 'Courier New']

/** SF Mono is not visible by its name in the web view: `ui-monospace` designates it. */
const ALIASES: Record<string, string> = { 'SF Mono': 'ui-monospace', 'SF Pro': 'system-ui' }

const family = (name: string) => ALIASES[name] ?? `'${name.replace(/'/g, '')}'`
/** A font chosen in Settings first; otherwise the theme's (`--theme-sans`, a rounded or monospace one), then Inter. */
export const sansStack = (name?: string) => (name?.trim() ? `${family(name.trim())}, ${SANS_STACK}` : `var(--theme-sans, ${SANS_STACK})`)
export const monoStack = (name?: string) => (name?.trim() ? `${family(name.trim())}, ${MONO_STACK}` : MONO_STACK)
/** The terminals': their own font (the bundled Nerd Font by default), then the mono one's stack. */
export const terminalStack = (name?: string, mono?: string) => `${family(name?.trim() || TERMINAL_FONT)}, ${monoStack(mono)}`

/**
 * The fonts the user installed: macOS's WebKit hides them from the page, only the system's are
 * visible. Prism finds a family's files and loads them here (`FontFace`), once per family.
 */
const loaded = $state<Record<string, 'loading' | 'loaded' | 'missing'>>({})
const loading = new Map<string, Promise<boolean>>()
interface Face {
  index: number
  weight: number
  italic: boolean
}
/** Makes the family usable by the page: visible already, or loaded from its files. False: not found. */
export function ensureFont(name: string): Promise<boolean> {
  name = name.trim()
  // A bundled font is there once loaded: before, a terminal would measure its fallback.
  if (BUNDLED.includes(name)) return Promise.all([`16px '${name}'`, `bold 16px '${name}'`].map((f) => document.fonts.load(f))).then(() => true, () => false)
  if (!name || visible(name)) return Promise.resolve(true)
  let p = loading.get(name)
  if (!p) {
    loaded[name] = 'loading'
    p = loadFaces(name).then((ok) => {
      loaded[name] = ok ? 'loaded' : 'missing'
      return ok
    })
    loading.set(name, p)
  }
  return p
}
async function loadFaces(name: string): Promise<boolean> {
  if (!isDesktop()) return false
  try {
    const faces = await invoke<Face[]>('font_faces', { family: name })
    const done = await Promise.allSettled(
      faces.map(async (f) => {
        const face = new FontFace(name, await invoke<ArrayBuffer>('font_face', { family: name, index: f.index }), { weight: String(f.weight), style: f.italic ? 'italic' : 'normal' })
        document.fonts.add(await face.load())
      }),
    )
    return done.some((r) => r.status === 'fulfilled')
  } catch {
    return false
  }
}

/** The variables are set once their fonts are loaded: a terminal sees the change, and measures the new font. */
let applying = 0
async function apply(sans?: string, mono?: string, terminal?: string) {
  const run = ++applying
  // A bundled terminal font is left to load with the first terminal (2.1 MB, read at start for
  // nothing until one opens), which measures it again once there; one the user installed is
  // looked for now, the page not seeing it otherwise.
  const term = terminal?.trim() || TERMINAL_FONT
  await Promise.all([sans, mono, BUNDLED.includes(term) ? '' : term].map((n) => ensureFont(n ?? '')))
  if (run !== applying) return
  const root = document.documentElement.style
  root.setProperty('--sans', sansStack(sans))
  root.setProperty('--mono', monoStack(mono))
  root.setProperty('--mono-term', terminalStack(terminal, mono))
}

/** The fonts from config.toml, applied on save. */
export function watchFonts() {
  return $effect.root(() => {
    $effect(() => {
      const a = rawAppearance()
      apply(a?.font, a?.font_mono, a?.font_terminal)
    })
  })
}

/** Is the font installed: visible to the page, or loaded from its files? */
export function installed(name: string): boolean {
  if (BUNDLED.includes(name.trim())) return true
  const state = loaded[name.trim()]
  return state === 'loaded' || state === 'loading' || visible(name)
}

/**
 * Does the page see the font? A missing family falls back: its measured width is
 * then the fallback's, for two fallbacks of different widths.
 */
function visible(name: string): boolean {
  if (ALIASES[name]) return true
  const ctx = document.createElement('canvas').getContext('2d')
  if (!ctx) return true
  const text = 'mmmmmmmmmlli10WQ@'
  return ['monospace', 'serif'].some((fallback) => {
    ctx.font = `48px ${fallback}`
    const base = ctx.measureText(text).width
    ctx.font = `48px ${family(name)}, ${fallback}`
    return ctx.measureText(text).width !== base
  })
}
