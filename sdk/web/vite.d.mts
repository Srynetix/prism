import type { UserConfig } from 'vite'

/** What Prism gives a module (`exports.json`). */
export declare const PRISM: {
  contract: number
  svelte: string
  svelteModules: string[]
  specifiers: Record<'@prism/sdk' | '@prism/sdk/words', string[]>
}

/** A module's build: `id`, its folder (`src/defs.ts`, `src/components.ts`, `module.toml`). */
export declare function moduleConfig(id: string, root: string): Promise<UserConfig>
