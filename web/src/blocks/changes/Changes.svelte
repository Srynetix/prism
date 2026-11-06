<script lang="ts">
  import { t } from '../../i18n.svelte'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import { isCollapsed, isShort } from '../../layout.svelte'
  import type { BlockProps } from '../block'
  import ChangesView from './ChangesView.svelte'
  import type { ChangesState } from './changes'

  /** Changes: what touched an environment, on one timeline (`ChangesView`). */
  let { state: journal, error }: BlockProps<ChangesState> = $props()
  const recent = $derived(journal?.events.filter((e) => Date.now() - Date.parse(e.at) < 3600_000).length ?? 0)
</script>

<section class="panel" style="--src: var(--accent)" class:collapsed={isCollapsed('changes')}>
  <h2>
    <span class="title"><Icon name="activity" />{t('Changes')}</span>
    {#if journal && recent}<small>{t('{n} in the last hour', { n: recent })}</small>{/if}
    <BlockControls id="changes" />
  </h2>
  {#if !journal}
    <div class="empty">{t('The Changes block is off. Turn it on in Settings › Infra › Changes ([changes] enabled).')}</div>
  {:else}
    <ChangesView state={journal} {error} compact={isShort('changes')} />
  {/if}
</section>

<style>
  .empty { padding: 14px 16px; color: var(--text-muted); font-size: var(--fs-body); }
</style>
