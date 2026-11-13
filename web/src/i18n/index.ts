/**
 * The dictionaries of the languages Prism speaks besides English: each file of `i18n/<lang>/`
 * gives its default export, `{ 'English text': 'translation' }`, one file per part of the app.
 * A key with a context (`tc`) is written `context|English text`. A module brings its own words
 * (`defs.js` `words`); Prism's win over theirs.
 */
export type Dictionary = Record<string, string>

/**
 * Filled by `loadDictionary`, before the app's modules are read (main.ts, remote/main.ts): a
 * window in English reads none of it (French is 140 KB gzipped, a phone's first load).
 */
export const DICTIONARIES: Record<'fr', Dictionary> = { fr: {} }

const LOADERS: Record<'fr', () => Promise<{ default: Dictionary }>> = { fr: () => import('./fr-all') }
const loaded = new Set<string>()

/** `lang`'s dictionary read, once; English has none. */
export async function loadDictionary(lang: string): Promise<void> {
  if (lang === 'en' || loaded.has(lang) || !(lang in LOADERS)) return
  const own = (await LOADERS[lang as 'fr']()).default
  Object.assign(DICTIONARIES[lang as 'fr'], own)
  loaded.add(lang)
}

/** Whether `lang`'s words are there (English always). */
export const dictionaryLoaded = (lang: string) => lang === 'en' || loaded.has(lang)
