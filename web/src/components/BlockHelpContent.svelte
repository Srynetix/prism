<script lang="ts">
  import { BLOCK_HELP } from '../block-help'
  import { blockLabel, isCustom, type BlockId } from '../views'
  import { openHelp } from '../help.svelte'
  import type { Guide } from '../help-guides'
  import { t } from '../i18n.svelte'

  /**
   * Content of a block's help: shown by the "?" bubble and the "+ Block" menu. Its guide's link
   * reads the guides when shown: every block imports this, the phone's too, which never shows it.
   */
  let { id }: { id: BlockId } = $props()
  // A custom block: the help of custom blocks, under its own name.
  const help = $derived(BLOCK_HELP[isCustom(id) ? 'custom_blocks' : id])
  let guide = $state<Guide | undefined>()
  $effect(() => {
    const block = id
    void import('../help-guides').then((m) => (guide = m.guideFor(isCustom(block) ? 'custom_blocks' : block)))
  })
</script>

{#if help}
  <p class="purpose"><b>{blockLabel(id)}</b> · {t(help.purpose)}</p>
  <ul>{#each help.points as p, i (i)}<li>{t(p)}</li>{/each}</ul>
  <p class="source"><span>{t('Data: {source}', { source: t(help.source) })}</span>{#if guide}{@const g = guide}<button onclick={() => openHelp(g.id)} title={t('Help › {title}', { title: g.title })}>{t('Read the guide →')}</button>{/if}</p>
{/if}

<style>
  .purpose { margin: 0 0 6px; }
  ul { margin: 0 0 6px; padding-left: 16px; color: var(--text-muted); }
  li + li { margin-top: 3px; }
  .source { display: flex; align-items: baseline; gap: 10px; }
  .source span { flex: 1; }
  .source button { flex-shrink: 0; border: none; background: none; padding: 0; color: var(--accent); font: inherit; font-weight: 600; cursor: pointer; }
  .source { margin: 0; padding-top: 6px; border-top: 1px solid var(--border); color: var(--text-muted); font-size: var(--fs-meta); }
</style>
