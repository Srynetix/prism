<script lang="ts">
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import { isCollapsed } from '../../layout.svelte'
  import { isDesktop } from '../../bridge'
  import { openSettings } from '../../settings.svelte'
  import { t } from '../../i18n.svelte'
  import type { BlockProps } from '../block'
  import CustomBlock from './CustomBlock.svelte'
  import type { CustomBlocksState } from './types'

  /** Every custom block, one after the other: the phone's view, or one block for them all on a page. */
  let { state }: BlockProps<CustomBlocksState> = $props()
  const blocks = $derived(state?.blocks ?? [])
</script>

<section class="panel" style="--src: var(--violet)" class:collapsed={isCollapsed('custom_blocks')}>
  <h2>
    <span class="title"><Icon name="code" />{t('Custom blocks')}</span>
    {#if blocks.length}<small>{blocks.length}</small>{/if}
    <BlockControls id="custom_blocks" />
  </h2>
  <div class="scroll">
    {#each state?.errors ?? [] as e (e)}<div class="err">{e}</div>{/each}
    {#each blocks as b (b.id)}
      <CustomBlock block={b} pageId="custom_blocks" embedded />
    {:else}
      <div class="empty">
        {t('No custom block yet: a command or a script that prints JSON, declared in [[custom_blocks]].')}
        {#if isDesktop()}<button class="lnk" onclick={() => openSettings('custom-blocks')}>{t('Settings › Custom blocks')}</button>{/if}
      </div>
    {/each}
  </div>
</section>

<style>
  .empty { padding: 12px 14px; color: var(--text-muted); font-size: var(--fs-body); }
  .lnk { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; }
  .err { padding: 6px 12px; background: var(--ko-bg); color: var(--ko); font-size: var(--fs-meta); }
</style>
