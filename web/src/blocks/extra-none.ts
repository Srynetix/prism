import type { BlockComponent, BlockDef } from './block'

/**
 * The blocks of another repository (`PRISM_EXTRA_BLOCKS`, see vite.config.ts): none in this
 * build. With it, `$extra-blocks` is that repository's `web/index.ts` (`EXTRA_BLOCKS`, the
 * declarations: they import nothing of Prism at run time but `t()`) and `$extra-components` its
 * `web/components.ts` (`EXTRA_COMPONENTS`), of the same shape. Their words: `$extra-i18n`
 * (`i18n/extra-none.ts`).
 */
export const EXTRA_BLOCKS: readonly BlockDef<never>[] = []
export const EXTRA_COMPONENTS: Record<string, BlockComponent<never>> = {}
