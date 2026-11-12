import type { BlockDef } from './block'
import { admitted, seal } from './runtime'
import { changes } from './changes'
import { customBlocks } from './custom'
import { bqcost, data } from './data'
import { devServers } from './dev_servers'
import { diskUsage } from './disk_usage'
import { docker } from './docker'
import { pet, quotes, shipit } from './fun'
import { sound } from './sound'
import { status } from './status'
import { system } from './system'
import { tokens } from './tokens'
import { trail } from './trail'

/**
 * The blocks with a source of their own compiled into this Prism (see `block.ts`): their
 * declarations, without their components (`components.ts`, read by the grid only).
 */
export const BLOCKS = [trail, changes, devServers, docker, data, bqcost, status, system, diskUsage, sound, tokens, customBlocks, shipit, quotes, pet] as const

/**
 * Prism's own; the modules' are not in the type: the core never names them, it reads them in
 * `BLOCK_DEFS`.
 */
export type PluginBlockId = (typeof BLOCKS)[number]['id']

/**
 * Prism's own, then the modules' loaded at run time (`modules/load.ts`, before this is read), as
 * the core reads them (a state it does not know).
 */
export const BLOCK_DEFS: readonly BlockDef<never>[] = [...BLOCKS, ...admitted(BLOCKS)]
seal()
