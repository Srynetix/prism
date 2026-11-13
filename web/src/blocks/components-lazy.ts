import type { BlockComponent } from './block'
import { RUNTIME_COMPONENTS } from './runtime'

/**
 * The components of `BLOCK_DEFS`' blocks for the phone, each loaded the first time its view
 * opens (`components.ts` imports them all, for the window's grid). The modules' come as they
 * were loaded.
 */
const OWN: Record<string, () => Promise<{ default: unknown }>> = {
  trail: () => import('./trail/Trail.svelte'),
  changes: () => import('./changes/Changes.svelte'),
  dev_servers: () => import('./dev_servers/DevServers.svelte'),
  docker: () => import('./docker/Docker.svelte'),
  data: () => import('./data/DataPipelines.svelte'),
  bqcost: () => import('./data/BigQueryCost.svelte'),
  shipit: () => import('./fun/ShipIt.svelte'),
  quotes: () => import('./fun/Quotes.svelte'),
  pet: () => import('./fun/Pet.svelte'),
  status: () => import('./status/StatusPages.svelte'),
  tokens: () => import('./tokens/Tokens.svelte'),
  system: () => import('./system/System.svelte'),
  sound: () => import('./sound/Sound.svelte'),
  disk_usage: () => import('./disk_usage/DiskUsage.svelte'),
  custom_blocks: () => import('./custom/CustomBlocks.svelte'),
}

const loaded = new Map<string, Promise<BlockComponent<never> | undefined>>()

/** A block's component; `undefined` for an id without one. */
export function blockComponent(id: string): Promise<BlockComponent<never> | undefined> {
  let p = loaded.get(id)
  if (!p) {
    const own = OWN[id]
    // A block of Prism's missing from `OWN`: all of `components.ts`, rather than nothing.
    p = own ? own().then((m) => m.default as BlockComponent<never>) : id in RUNTIME_COMPONENTS ? Promise.resolve(RUNTIME_COMPONENTS[id]) : import('./components').then((m) => m.BLOCK_COMPONENTS[id])
    loaded.set(id, p)
  }
  return p
}
