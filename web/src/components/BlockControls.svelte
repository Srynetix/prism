<script lang="ts">
  import BlockHelp from './BlockHelp.svelte'
  import Icon from './Icon.svelte'
  import type { IconName } from '../icons'
  import { editing, modeOf, removeBlock, setMode, toggleZoom, zoom, type BlockMode } from '../layout.svelte'
  import type { BlockId } from '../views'
  import { t } from '../i18n.svelte'

  /**
   * Block controls, at the right edge of the title row: block help (?), block state
   * (▤ normal, ▭ short, – collapsed) and temporary enlargement to the whole view (Escape to return). They
   * only appear on title hover, unless the block is not normal. In "Edit
   * layout", a ⋮⋮ handle at the start of the title to move the block, and × to remove it.
   */
  let { id }: { id: string } = $props()
  const mode = $derived(modeOf(id))
  const zoomed = $derived(zoom.id === id)

  const MODES: { mode: BlockMode; glyph: IconName; label: string }[] = [
    { mode: 'normal', glyph: 'blockNormal', label: t('Normal') },
    { mode: 'short', glyph: 'blockShort', label: t('Short: a few lines') },
    { mode: 'collapsed', glyph: 'none', label: t('Collapsed: title only') },
  ]
</script>

{#if editing.on}<span class="grip" title={t('Drag to move this block')} aria-hidden="true"><Icon name="grip" /></span>{/if}
<span class="controls" class:pinned={mode !== 'normal' || zoomed || editing.on}>
  <BlockHelp {id} />
  {#if !zoomed}
    <span class="seg" role="group" aria-label={t('Block size')}>
      {#each MODES as m (m.mode)}
        <button class:on={mode === m.mode} onclick={() => setMode(id, m.mode)} aria-pressed={mode === m.mode} title={m.label}><Icon name={m.glyph} /></button>
      {/each}
    </span>
  {/if}
  {#if editing.on}
    <button class="x" onclick={() => removeBlock(id as BlockId)} aria-label={t('Remove from this view')} title={t('Remove from this view (+ Block brings it back)')}><Icon name="close" /></button>
  {:else}
    <button onclick={() => toggleZoom(id)} aria-pressed={zoomed} aria-label={zoomed ? t('Restore block') : t('Zoom block')} title={zoomed ? t('Restore (Esc)') : t('Zoom')}><Icon name={zoomed ? 'shrink' : 'expand'} /></button>
  {/if}
</span>

<style>
  .controls { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; padding-left: 8px; border-left: 1px solid var(--border); opacity: 0; transition: opacity 0.1s; }
  :global(.panel > h2:hover) .controls, .controls:focus-within, .controls.pinned { opacity: 1; }
  button { display: inline-flex; align-items: center; justify-content: center; width: 20px; height: 20px; border: none; border-radius: var(--r5); background: none; padding: 0; color: var(--text-muted); font: inherit; font-size: var(--fs-body); line-height: 1; cursor: pointer; }
  button:hover { background: var(--surface-muted); color: var(--text); }
  .seg { display: inline-flex; gap: 1px; padding: 1px; border-radius: var(--r6); background: var(--surface-muted); }
  .seg button { width: 20px; height: 17px; border-radius: var(--r4); font-size: 11px; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12); }
  .x:hover { color: var(--ko); }
  /* Handle: placed at the start of the title by flex order, wherever the component is. */
  .grip { order: -1; display: inline-flex; align-items: center; margin-right: -2px; color: var(--text-muted); font-size: 12px; cursor: grab; }
</style>
