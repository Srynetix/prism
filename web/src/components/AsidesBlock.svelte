<script lang="ts">
  import { live } from '../live.svelte'
  import { isCollapsed } from '../layout.svelte'
  import { t } from '../i18n.svelte'
  import { asidesOf, badged } from '../asides.svelte'
  import BlockControls from './BlockControls.svelte'
  import AsideList from './AsideList.svelte'

  /** The « In passing » block: the Agents view's list, placed on a page. */
  const n = $derived(asidesOf(live.snapshot).filter(badged).length)
</script>

<section class="panel" style="--src: var(--src-claude)" class:collapsed={isCollapsed('aside')}>
  <h2>
    <span class="title">{t('In passing')}</span>
    {#if n}<small>{t('{n} open', { n })}</small>{/if}
    <BlockControls id="aside" />
  </h2>
  <div class="scroll"><AsideList block /></div>
</section>

<style>
  .scroll { overflow: auto; }
</style>
