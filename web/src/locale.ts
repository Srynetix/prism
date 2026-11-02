import { appearance } from './fun.svelte'

/**
 * The format of dates, times and numbers (`[appearance] locale`: `fr-FR`, `en-GB`…), for every
 * `toLocale…String` of the window; `undefined`, the system's: empty, or a tag the webview does not
 * know (which would otherwise throw).
 */
export function locale(): string | undefined {
  const tag = appearance().locale?.trim() ?? ''
  if (tag !== canonical.tag) canonical = { tag, value: canonicalOf(tag) }
  return canonical.value
}
let canonical: { tag: string; value: string | undefined } = { tag: '', value: undefined }
function canonicalOf(tag: string): string | undefined {
  if (!tag) return undefined
  try {
    return Intl.getCanonicalLocales(tag)[0]
  } catch {
    return undefined
  }
}

/**
 * A date's formatter for these options, in the window's locale, made once: each
 * `toLocale…String(locale, options)` builds one (~44 µs in WebKit, 0.4 µs reused, measured).
 */
export function dateFormat(options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const key = `${locale() ?? ''}|${JSON.stringify(options)}`
  let f = formats.get(key)
  if (!f) formats.set(key, (f = new Intl.DateTimeFormat(locale(), options)))
  return f
}
const formats = new Map<string, Intl.DateTimeFormat>()
