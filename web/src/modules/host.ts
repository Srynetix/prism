import * as svelte from 'svelte'
// Svelte's runtime: not a public API, so without declarations; a module's compiled code calls it.
// @ts-expect-error no declarations
import * as internal from 'svelte/internal/client'
// @ts-expect-error no declarations
import * as disclose from 'svelte/internal/disclose-version'
import * as animate from 'svelte/animate'
import * as attachments from 'svelte/attachments'
import * as easing from 'svelte/easing'
import * as events from 'svelte/events'
import * as motion from 'svelte/motion'
import * as reactivity from 'svelte/reactivity'
import * as reactivityWindow from 'svelte/reactivity/window'
import * as store from 'svelte/store'
import * as transition from 'svelte/transition'
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

/** Svelte's modules a module may import, Prism's instances (the SDK lists them, `web/scripts/module-sdk.mjs`, and its build refuses the others). */
const SVELTE_MODULES: Record<string, unknown> = {
  svelte,
  'svelte/internal/client': internal,
  'svelte/internal/disclose-version': disclose,
  'svelte/animate': animate,
  'svelte/attachments': attachments,
  'svelte/easing': easing,
  'svelte/events': events,
  'svelte/motion': motion,
  'svelte/reactivity': reactivity,
  'svelte/reactivity/window': reactivityWindow,
  'svelte/store': store,
  'svelte/transition': transition,
}

/** The specifiers a module may import; another one fails its load, said with its name. */
export const SPECIFIERS = [...Object.keys(SVELTE_MODULES), '@prism/sdk/words', '@prism/sdk']

/** Svelte and the words: what a module's declarations may use. */
export function giveWords(contract: number) {
  for (const [spec, m] of Object.entries(SVELTE_MODULES)) given.set(spec, m)
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
