import * as svelte from 'svelte'
// Svelte's runtime: not a public API, so without declarations; a module's compiled code calls it.
// @ts-expect-error no declarations
import * as internal from 'svelte/internal/client'
// @ts-expect-error no declarations
import * as disclose from 'svelte/internal/disclose-version'
import sveltePackage from 'svelte/package.json'
import * as words from './sdk-words'

/**
 * What Prism gives a module's code, by the specifier it imports: a module is built with these
 * left out (`import { t } from '@prism/sdk/words'`), each read at run time from
 * `globalThis.__prismHost` — Prism's own instance, not a copy: one Svelte runtime, one snapshot,
 * one set of preferences.
 *
 * Two steps, as the module's two files: its declarations may only use the words (given before
 * anything reads the blocks), its components the whole SDK (`sdk.ts`, which reads them).
 */
export const SVELTE: string = sveltePackage.version

interface Host {
  /** The module contract (`prism_core::modules::CONTRACT`). */
  contract: number
  svelte: string
  get(spec: string): unknown
}

declare global {
  var __prismHost: Host | undefined
}

const given = new Map<string, unknown>()

/** The specifiers a module may import; another one fails its load, said with its name. */
export const SPECIFIERS = ['svelte', 'svelte/internal/client', 'svelte/internal/disclose-version', '@prism/sdk/words', '@prism/sdk'] as const

/** Svelte and the words: what a module's declarations may use. */
export function giveWords(contract: number) {
  given.set('svelte', svelte)
  given.set('svelte/internal/client', internal)
  given.set('svelte/internal/disclose-version', disclose)
  given.set('@prism/sdk/words', words)
  globalThis.__prismHost = {
    contract,
    svelte: SVELTE,
    get(spec) {
      if (!given.has(spec)) throw new Error(`Prism gives no ${spec} to modules (only ${SPECIFIERS.join(', ')})`)
      return given.get(spec)
    },
  }
}

/** The whole SDK: for the components, once the declarations are in. */
export async function giveSdk() {
  given.set('@prism/sdk', await import('./sdk'))
}
