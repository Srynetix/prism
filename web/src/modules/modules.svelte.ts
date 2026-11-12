import { isDesktop, machine } from '../bridge'
import { DICTIONARIES, type Dictionary } from '../i18n'
import { t } from '../i18n.svelte'
import { CLASHES, isSealed, OWNER, RUNTIME_DEFS, takeComponents } from '../blocks/runtime'
import type { BlockComponent, BlockDef } from '../blocks/block'
import type { ModuleInfo } from '../bindings/ModuleInfo'

/**
 * Modules: a team's blocks loaded at run time, without rebuilding Prism (`modules/` next to
 * config.toml, see `prism_core::modules`). A module's web folder holds two ES modules built apart:
 *
 * - `defs.js`, read first: `prism` (the contract it was built for), `svelte` (the version it was
 *   compiled with), `words` (its French), `blocks()` (its `BlockDef`s, a function: called once
 *   its words are in);
 * - `components.js`, read next: `components`, by block id (`<id>:status` for a status-bar item).
 *
 * Both are read before the app's modules, which keep the blocks in constants: a module installed
 * or replaced shows once the window reloads. Their imports of Svelte and of Prism (`@prism/sdk`)
 * read Prism's own instances (`host.ts`).
 */

/** The module contract this window gives (`prism_core::modules::CONTRACT`). */
export const CONTRACT = 1

/** A module's `defs.js`. */
export interface ModuleDefs {
  prism: number
  svelte: string
  words?: Partial<Record<'fr', Dictionary>>
  blocks: () => BlockDef<never>[]
}

export interface ModuleStatus {
  id: string
  version: string | null
  description: string | null
  /** `on`: its blocks are in; `refused`: Prism did not load it; `failed`: its files did not load. */
  state: 'on' | 'refused' | 'failed'
  reason?: string
  blocks: string[]
}

/** The modules of this window, as it loaded them. */
export const modules = $state<{ list: ModuleStatus[]; loaded: boolean }>({ list: [], loaded: false })

const minor = (v: string) => v.split('.').slice(0, 2).join('.')

/** Why a module's declarations are refused: another contract, or another Svelte than Prism's (its runtime is not a public API). */
export function refusal(defs: Partial<ModuleDefs>, svelte: string): string | null {
  if (typeof defs.prism !== 'number') return t('defs.js does not say its contract (prism)')
  if (defs.prism !== CONTRACT) return t('built for module contract {theirs}, this Prism gives {ours}', { theirs: String(defs.prism), ours: CONTRACT })
  if (typeof defs.svelte !== 'string') return t('defs.js does not say its Svelte (svelte)')
  if (minor(defs.svelte) !== minor(svelte)) return t('compiled with Svelte {theirs}, this Prism runs {ours}: build it again', { theirs: defs.svelte, ours: svelte })
  if (typeof defs.blocks !== 'function') return t('defs.js has no blocks()')
  return null
}

/** A module's words, under Prism's: Prism's win. */
export function addWords(dict: Dictionary, words: Dictionary) {
  for (const [en, translated] of Object.entries(words)) if (!(en in dict)) dict[en] = translated
}

interface Source {
  list: () => Promise<ModuleInfo[]>
  base: (id: string) => string
}

/**
 * Where this page reads its modules: a window from the app (`prism-module:`, the modules of the
 * profile it shows), a phone or the browser preview from its server (`/modules/`).
 */
function source(base: string): Source | null {
  if (isDesktop()) {
    // Another computer's window: its modules are not files of this one.
    if (machine()) return null
    base = navigator.userAgent.includes('Windows') ? 'http://prism-module.localhost' : 'prism-module://localhost'
  }
  const root = isDesktop() ? base : `${base}/modules`
  return {
    list: async () => {
      const r = await fetch(`${root}/index.json`, { credentials: 'same-origin', cache: 'no-store' })
      return r.ok ? ((await r.json()) as ModuleInfo[]) : []
    },
    base: (id) => `${root}/${id}/`,
  }
}

let loading: Promise<void> | null = null

/**
 * The modules' blocks loaded, before the app's modules read the blocks (main.ts, remote/main.ts),
 * once per page. `base`: a phone's prefix for the profile it shows (`/p/<id>`). A module that does
 * not load is said in `modules.list`, the others load.
 */
export const loadModules = (base = '') => (loading ??= load(base))

async function load(base: string): Promise<void> {
  const from = source(base)
  let infos: ModuleInfo[] = []
  try {
    infos = from ? await from.list() : []
  } catch (e) {
    console.error('modules:', e)
  }
  const status: ModuleStatus[] = infos.map((m) => ({ id: m.id, version: m.version, description: m.description, state: m.refused ? 'refused' : 'on', reason: m.refused ?? undefined, blocks: [] }))
  const wanted = status.filter((s) => s.state === 'on' && infos.find((m) => m.id === s.id)?.web)
  if (from && wanted.length) {
    if (isSealed()) console.error('modules: the blocks were read before the modules loaded')
    const host = await import('./host')
    host.giveWords(CONTRACT)
    const declared: ModuleStatus[] = []
    for (const s of wanted) {
      try {
        const defs = (await import(/* @vite-ignore */ `${from.base(s.id)}defs.js`)) as Partial<ModuleDefs>
        const why = refusal(defs, host.SVELTE)
        if (why) {
          Object.assign(s, { state: 'refused', reason: why })
          continue
        }
        for (const [lang, words] of Object.entries(defs.words ?? {})) if (lang in DICTIONARIES && words) addWords(DICTIONARIES[lang as 'fr'], words)
        // A block id another module took first stays the first one's.
        const blocks = defs.blocks!().filter((b) => {
          if (!(b.id in OWNER)) return true
          CLASHES.push({ block: b.id, module: s.id })
          return false
        })
        for (const b of blocks) OWNER[b.id] = s.id
        RUNTIME_DEFS.push(...blocks)
        s.blocks = blocks.map((b) => b.id)
        declared.push(s)
      } catch (e) {
        Object.assign(s, { state: 'failed', reason: `defs.js: ${e instanceof Error ? e.message : String(e)}` })
      }
    }
    // The blocks fixed (the SDK reads them anyway): a block whose id is Prism's is out of OWNER
    // before any component comes in.
    await import('../blocks')
    if (declared.length) await host.giveSdk()
    for (const s of declared) {
      try {
        const mod = (await import(/* @vite-ignore */ `${from.base(s.id)}components.js`)) as { components?: Record<string, BlockComponent<never>> }
        takeComponents(s.id, mod.components ?? {})
      } catch (e) {
        // Its declarations stay (labels, help); its blocks show as failed.
        Object.assign(s, { state: 'failed', reason: `components.js: ${e instanceof Error ? e.message : String(e)}` })
      }
    }
  }
  modules.list = status
  modules.loaded = true
}

/** The modules and what became of their blocks (a block whose id was taken is said too). */
export function moduleStates(): (ModuleStatus & { clashes: string[] })[] {
  return modules.list.map((m) => ({ ...m, clashes: CLASHES.filter((c) => c.module === m.id).map((c) => c.block) }))
}

/** Why a module block has no component: its module's reason. */
export const moduleFailure = (block: string): string | undefined => modules.list.find((m) => m.blocks.includes(block) && m.state === 'failed')?.reason
