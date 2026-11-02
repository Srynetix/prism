import { live } from './live.svelte'
import type { EnvironmentStage as Stage } from './bindings/EnvironmentStage'

/**
 * The stages a release goes through, under the team's names (`Snapshot.stages`, src-tauri/crates/core/src/stages.rs):
 * declared by a block (`[clusters] stages`, a team's delivery model), otherwise the environments
 * of `[[environments]]`. Environments, ArgoCD and Settings read them.
 */
export type { EnvironmentStage as Stage } from './bindings/EnvironmentStage'

/** Before the first snapshot (browser preview): a staging, then production. */
const DEFAULT: Stage[] = [
  { role: 'staging', name: 'staging', env: 'staging' },
  { role: 'production', name: 'production', env: 'production' },
]

/** In their order: branch builds first, production last. */
export const stages = (): Stage[] => live.snapshot?.stages ?? DEFAULT
export const stageOf = (role: Stage['role']) => (role ? stages().find((s) => s.role === role) : undefined)
/** Their environments, production first. */
export const stageEnvs = () => stages().map((s) => s.env).reverse()
