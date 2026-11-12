import type { BlockComponent, BlockDef } from './block'

/**
 * The blocks of the modules loaded at run time (`modules/`, see `modules/load.ts`): filled before
 * `blocks/index.ts` is read, merged there into `BLOCK_DEFS` and in `components.ts` into
 * `BLOCK_COMPONENTS`. A leaf: it imports nothing at run time, so the loader fills it before
 * anything else of Prism reads the blocks (several modules keep them in constants).
 */
export const RUNTIME_DEFS: BlockDef<never>[] = []
export const RUNTIME_COMPONENTS: Record<string, BlockComponent<never>> = {}

let sealed = false
/** `blocks/index.ts` read: the blocks are fixed from then on, until the window reloads. */
export const seal = () => (sealed = true)
/** A module's blocks would come too late: some of Prism already read them. */
export const isSealed = () => sealed
/** A block of a module: its component runs inside a boundary (`ModuleBoundary.svelte`). */
export const isModuleBlock = (id: string) => id in RUNTIME_COMPONENTS

/**
 * The core's blocks (`views.ts`, `BLOCK_LABELS`): a module may not take their ids. Kept here, a
 * leaf, for the check to run before `views.ts` is read (`runtime.test.ts` keeps it in step).
 */
export const CORE_BLOCK_IDS = ['todo', 'sessions', 'worktrees', 'notes', 'mrs', 'queue', 'renovate', 'sprint', 'agenda', 'refinement', 'pickup', 'pods', 'ci', 'vulns', 'envs', 'argocd', 'mail', 'retro', 'wip', 'routines', 'aside'] as const

/** A block of a module not taken in: its id was already Prism's, or another module's. */
export const CLASHES: { block: string; module: string }[] = []
/** The module each module block comes from. */
export const OWNER: Record<string, string> = {}

/**
 * The modules' blocks `BLOCK_DEFS` takes in: those whose id is free (not the core's, not a block
 * of Prism's or of its build, not one taken by a module before). The others go to `CLASHES`.
 */
export function admitted(taken: readonly { id: string }[]): BlockDef<never>[] {
  const ids = new Set<string>([...CORE_BLOCK_IDS, ...taken.map((b) => b.id)])
  const out: BlockDef<never>[] = []
  for (const b of RUNTIME_DEFS) {
    if (ids.has(b.id) || !/^[a-z][a-z0-9_]{0,39}$/.test(b.id)) {
      CLASHES.push({ block: b.id, module: OWNER[b.id] ?? '?' })
      // Not a module's block any more: Prism's own keeps its component, outside any boundary.
      if (!out.some((o) => o.id === b.id)) {
        delete OWNER[b.id]
        delete RUNTIME_COMPONENTS[b.id]
        delete RUNTIME_COMPONENTS[`${b.id}:status`]
      }
      continue
    }
    ids.add(b.id)
    out.push(b)
  }
  return out
}

/**
 * A module's components taken in: those of its own blocks still admitted (`OWNER`), a block or a
 * block's status-bar item (`<id>:status`). Whichever is read first, `admitted()` or this, a block
 * whose id is Prism's never gets the module's component.
 */
export function takeComponents(module: string, components: Record<string, BlockComponent<never>>) {
  for (const [id, c] of Object.entries(components)) if (OWNER[id.split(':')[0]] === module) RUNTIME_COMPONENTS[id] = c
}
