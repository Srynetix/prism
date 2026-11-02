import { BLOCK_LABELS, isBreak, type BlockId } from './views'
import { live } from './live.svelte'
import type { TemplateConfig, ViewConfig } from './types'

/**
 * Templates of the pages wizard: each one writes its pages (`[[views]]`) and its access
 * (`[forge] enabled`, `[clusters] enabled`, `[agents] enabled`, `[data] enabled`) into config.toml.
 * These are not settings: once applied, everything is modified in Settings › Interface › Pages
 * and Edit layout. They all come from the packs (`[[templates]]`): a team's, for its roles, or
 * Prism's starter pack (`prism:starter`, the Developer template); Prism itself has none.
 */
export type Template = TemplateConfig

/** The templates offered: those of the packs. */
export const templates = (): Template[] => live.snapshot?.templates ?? []

/** The pack built into Prism that the wizard offers when no pack brings templates. */
export const STARTER_PACK = 'prism:starter'

/** The blocks of a template page, by column, for the preview (its bands one after the other). */
export const columnsOfConfig = (v: ViewConfig): BlockId[][] =>
  (v.columns ?? []).filter((c) => !isBreak(c)).map((c) => c.split(/\s+/).filter((b): b is BlockId => b in BLOCK_LABELS))
