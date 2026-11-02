<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { showBlock } from '../../layout.svelte'
  import { stateOf, type StatusProps } from '../block'
  import { troubled, type StatusState } from './types'
  import { t } from '../../i18n.svelte'

  /** The status bar's item, while a service has something under way: the worst one, the count of the others. */
  let { snap }: StatusProps = $props()

  const list = $derived(troubled(stateOf<StatusState>({ id: 'status' }, snap)))
  const worst = $derived(list[0])
</script>

{#if worst}
  <button class="sbi svc" class:ko={worst.level === 'major' || worst.level === 'critical'} class:maint={worst.level === 'maintenance'} onclick={() => showBlock('status')} title="{list.map((s) => `${s.name}: ${s.incidents[0]?.name ?? s.summary}`).join('\n')} · {t('open Status pages')}">
    <span class="g"><Icon name="warning" /></span><b>{worst.name}</b>{worst.level === 'maintenance' ? t('maintenance') : t('incident')}
    {#if list.length > 1}<small>+{list.length - 1}</small>{/if}
  </button>
{/if}

<style>
  .svc { color: var(--warn); }
  .svc.ko { color: var(--ko); }
  .svc.maint { color: var(--text-muted); }
  .svc b { margin-right: 4px; }
  .svc small { color: var(--text-muted); margin-left: 4px; }
</style>
