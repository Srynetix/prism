<script lang="ts">
  import { openSettings } from '../settings.svelte'
  import type { Snippet } from 'svelte'
  import BlockHelpContent from './BlockHelpContent.svelte'
  import { exclusive } from '../popup'
  import { untrack } from 'svelte'
  import { pushLayer } from '../layers.svelte'
  import { addBand, addBlock, addColumn, bandsOf, columnsOf, currentView, editing, resetArrangement, toggleEditing } from '../layout.svelte'
  import { ALL_BLOCKS, MAX_BANDS, MAX_COLUMNS, blockLabel, byCategory, customBlockIds, views, type BlockId } from '../views'
  import { t, tc } from '../i18n.svelte'

  /**
   * "Edit layout" in the header: toggles layout mode (handles, × on blocks,
   * target columns); while editing, "+ Block" adds to the view a block it does not have (including
   * one from another view: a block can appear in several), "+ Column" a column to the last band,
   * "+ Band" a band below (a row of columns: a block alone in one spans the page's width), and
   * "Reset" restores the view's default layout. Escape or "Done" leaves editing.
   * In the "+ Block" menu, the blocks are grouped by what they are about (`byCategory`); the
   * hovered block shows its help and a preview of the block itself, with the current data, shrunk
   * and inert (`preview`: the app's block rendering).
   */
  let { preview }: { preview?: Snippet<[BlockId]> } = $props()
  let open = $state(false)
  let hovered = $state<BlockId | null>(null)
  const view = $derived(currentView())
  const columns = $derived(columnsOf(view))
  const bands = $derived(bandsOf(view))
  const present = $derived(new Set(columns.flat()))
  const missing = $derived([...ALL_BLOCKS, ...customBlockIds()].filter((id) => !present.has(id)))
  const groups = $derived(byCategory(missing))
  const shown = $derived(hovered && missing.includes(hovered) ? hovered : groups[0]?.ids[0])
  /** Views that already contain a block absent from here, to locate it in the menu. */
  const elsewhere = (id: string) => views().filter((v) => v.id !== view.id && columnsOf(v).flat().includes(id as never)).map((v) => v.label).join(', ')

  let picker: HTMLDivElement | undefined = $state()
  // Open, a layer on top: Escape closes the « + Block » menu first, before leaving the editing.
  $effect(() => {
    if (!open) return
    return untrack(() => pushLayer({ kind: 'menu', key: 'layout-menu', el: () => picker, close: () => (open = false), outside: false })).remove
  })

  function onWindowClick(e: MouseEvent) {
    if (open && !(e.target as HTMLElement).closest('.layout-menu')) open = false
  }
</script>

<svelte:window onclick={onWindowClick} />

<span class="layout-menu">
  {#if editing.on}
    <span class="add">
      <button class="btn sm" class:on={open} onclick={() => (open = !open)} aria-expanded={open} disabled={!missing.length} title={missing.length ? t('Add a block to this view') : t('Every block is already here')}>{t('+ Block')}</button>
      {#if open}
        <div class="picker popover" bind:this={picker} {@attach exclusive(() => (open = false))}>
          <div class="menu" role="menu">
            {#each groups as g (g.category)}
              <div class="group" role="group" aria-label={t(g.category)}>
                <span class="hd" aria-hidden="true">{t(g.category)}</span>
                {#each g.ids as id (id)}
                  <button class="mi" role="menuitem" class:on={id === shown} onmouseenter={() => (hovered = id)} onfocus={() => (hovered = id)} onclick={() => { addBlock(id); open = false }}>{blockLabel(id)}{#if elsewhere(id)}<small class="k">{elsewhere(id)}</small>{/if}</button>
                {/each}
              </div>
            {/each}
          </div>
          {#if shown}
            <div class="about">
              <BlockHelpContent id={shown} />
              {#if preview}
                <div class="preview" aria-hidden="true" title={t('Preview with the current data')}>
                  <div class="frame" inert>{@render preview(shown)}</div>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </span>
    <button class="btn sm" onclick={() => addColumn()} disabled={bands[bands.length - 1] >= MAX_COLUMNS} title={bands.length > 1 ? t('Add an empty column to the last band (up to {max})', { max: MAX_COLUMNS }) : t('Add an empty column (up to {max})', { max: MAX_COLUMNS })}>{t('+ Column')}</button>
    <button class="btn sm" onclick={addBand} disabled={bands.length >= MAX_BANDS} title={t('Add a band below: a row of its own columns, a block alone in it across the page (up to {max})', { max: MAX_BANDS })}>{t('+ Band')}</button>
    <button class="btn sm" onclick={resetArrangement} title={t('Back to equal sizes and normal blocks in this view (the blocks stay)')}>{t('Reset')}</button>
    <button class="btn sm" onclick={() => { toggleEditing(); openSettings('views') }} title={t('Add, rename, reorder or remove pages (Settings › Interface › Pages)')}>{t('Pages…')}</button>
  {/if}
  <button class="btn sm" class:primary={editing.on} onclick={toggleEditing} title={editing.on ? t('Leave layout editing (Esc)') : t('Move, add or remove blocks and columns')}>{editing.on ? tc('core', 'Done') : t('Edit layout')}</button>
</span>

<style>
  .layout-menu { display: inline-flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .layout-menu .btn.on { background: var(--surface-muted); }
  .add { position: relative; }
  .picker { position: absolute; top: calc(100% + 4px); left: 0; display: flex; padding: 0; overflow: hidden; }
  .menu { min-width: 190px; max-height: 70vh; overflow: auto; padding: var(--sp-2); border-right: 1px solid var(--border); }
  .about { display: flex; flex-direction: column; gap: 10px; width: 460px; padding: 10px 12px; font-size: var(--fs-body); line-height: 1.45; }
  /* Preview: the block rendered at 920 × 560, scaled to half, with no possible interaction. */
  .preview { position: relative; width: 436px; height: 266px; overflow: hidden; border: 1px solid var(--border); border-radius: var(--r8); background: var(--bg); }
  .frame { position: absolute; top: 0; left: 0; display: flex; width: 920px; height: 560px; padding: 8px; transform: scale(0.474); transform-origin: top left; pointer-events: none; }
  .frame > :global(*) { flex: 1; min-width: 0; }
  .menu .mi { font-size: var(--fs-body); }
  .group + .group { margin-top: 4px; padding-top: 4px; border-top: 1px solid var(--border); }
  .hd { display: block; }
</style>
