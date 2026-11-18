<script lang="ts">
  import State from '../../components/ui/State.svelte'
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
    {#each state?.errors ?? [] as e (e)}<div class="fail"><State kind="error" inline title={e} /></div>{/each}
    {#each blocks as b (b.id)}
      <CustomBlock block={b} pageId="custom_blocks" embedded />
    {:else}
      <State kind="empty" icon="code" title={t('No custom block yet: a command or a script that prints JSON, declared in [[custom_blocks]].')} action={isDesktop() ? { label: t('Settings › Interface › Custom blocks'), run: () => openSettings('custom-blocks') } : undefined} />
    {/each}
  </div>
</section>

<style>
  .fail { padding: 6px 12px; background: color-mix(in srgb, var(--ko-bg) 50%, transparent); font-size: var(--fs-meta); }
</style>
