import { rawAppearance } from './fun.svelte'
import { DICTIONARIES, dictionaryLoaded, loadDictionary } from './i18n'

/**
 * Prism's words in the user's language (`[appearance] language`: `en`, `fr`; empty: the system's).
 * The English text is the key: `t('Merge requests')`, `t('Move {ticket} to “{status}”', { ticket, status })`;
 * a language's dictionary (`i18n/<lang>/*.ts`) gives its translation, failing that the English stays.
 *
 * The language is decided once, at the first word: the module constants that hold words are then
 * right, and a change of it reloads the window (`watchLanguage`). It comes from the snapshot when
 * there is one already, otherwise from the last one seen (kept in `localStorage`), so that the
 * reload does not loop.
 */
export type Lang = 'en' | 'fr'
export const LANGUAGES: { id: Lang; name: string }[] = [
  { id: 'en', name: 'English' },
  { id: 'fr', name: 'Français' },
]
const KEPT = 'prism.language'

/** `[appearance] language` resolved: the system's when empty, English when Prism does not speak it. */
export function resolve(setting: string | undefined): Lang {
  const tag = (setting?.trim() || (typeof navigator === 'undefined' ? '' : navigator.language) || 'en').toLowerCase()
  return LANGUAGES.find((l) => tag === l.id || tag.startsWith(`${l.id}-`))?.id ?? 'en'
}

function kept(): string | undefined {
  try {
    return localStorage.getItem(KEPT) ?? undefined
  } catch {
    return undefined
  }
}

let current: Lang | null = null
/** The window's language, fixed at its first word. */
export function lang(): Lang {
  if (!current) {
    const raw = rawAppearance()
    current = resolve(raw ? raw.language : kept())
    if (typeof document !== 'undefined') document.documentElement.lang = current
  }
  return current
}

/** The window's language decided and its dictionary read: awaited before the app's modules run. */
export const loadLanguage = () => loadDictionary(lang())

let warned = false
/** Development: a word asked before the dictionary is read stays in English; said once. */
function early(l: Lang) {
  if (import.meta.env.DEV && !warned && !dictionaryLoaded(l)) {
    warned = true
    console.warn(`i18n: a word asked before the ${l} dictionary was read stays in English (a module constant?)`, new Error().stack)
  }
}

type Vars = Record<string, string | number>
const fill = (text: string, vars?: Vars) => (vars ? text.replace(/\{(\w+)\}/g, (all, k: string) => (k in vars ? String(vars[k]) : all)) : text)

/** `text`, in the window's language; `{name}` is replaced by `vars.name`. */
export function t(text: string, vars?: Vars): string {
  const l = lang()
  if (l !== 'en') early(l)
  return fill(l === 'en' ? text : (DICTIONARIES[l][text] ?? text), vars)
}

/**
 * A word that English writes the same for two meanings (`Open`, a state; `Open`, an action): its
 * dictionary key is `context|text`, and without one the plain `text`'s translation is used.
 */
export function tc(context: string, text: string, vars?: Vars): string {
  const l = lang()
  if (l !== 'en') early(l)
  return fill(l === 'en' ? text : (DICTIONARIES[l][`${context}|${text}`] ?? DICTIONARIES[l][text] ?? text), vars)
}

/**
 * The singular or the plural for `n` (each its own key), `{n}` replaced by it: `tn(n, '{n} review', '{n} reviews')`.
 * French takes the singular for 0 and 1, English for 1 only.
 */
export function tn(n: number, one: string, other: string, vars?: Vars): string {
  const single = lang() === 'fr' ? Math.abs(n) < 2 : n === 1
  return t(single ? one : other, { n, ...vars })
}

/** The language changed in config.toml (Settings, the phone, an edit): the window starts again in it. */
export function watchLanguage() {
  return $effect.root(() => {
    $effect(() => {
      const raw = rawAppearance()
      if (!raw) return
      try {
        localStorage.setItem(KEPT, raw.language ?? '')
      } catch {
        // No storage: a reload reads the snapshot again anyway.
      }
      if (current && resolve(raw.language) !== current) location.reload()
    })
  })
}
