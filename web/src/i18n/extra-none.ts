import type { Dictionary } from '.'

/**
 * The words of another repository's blocks (`PRISM_EXTRA_BLOCKS`, see vite.config.ts): none in
 * this build. With it, `$extra-i18n` is that repository's `web/i18n.ts`, of the same shape: by
 * language, a dictionary as Prism's own (`i18n/<lang>/*.ts`). It imports nothing of Prism at run
 * time (types aside): Prism's dictionaries read it first.
 */
export const EXTRA_DICTIONARIES: Partial<Record<'fr', Dictionary>> = {}
