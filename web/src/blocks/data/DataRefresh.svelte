<script lang="ts">
  import { hasCore } from '../../bridge'
  import Icon from '../../components/Icon.svelte'
  import { dataRefresh, refreshData } from './refresh.svelte'
  import { t } from '../../i18n.svelte'

  /** Reload GCP now (Data and BigQuery cost blocks). */
</script>

{#if hasCore()}
  <button
    class="refresh"
    class:busy={dataRefresh.busy}
    class:error={!!dataRefresh.error}
    disabled={dataRefresh.busy}
    onclick={refreshData}
    title={dataRefresh.busy ? t('Reading GCP…') : dataRefresh.error ? t('Refresh failed: {error}', { error: dataRefresh.error }) : t('Read GCP again now (cost included)')}
    aria-label={t('Refresh')}
  ><Icon name="refresh" spin={dataRefresh.busy} /></button>
{/if}

<style>
  .refresh { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 20px; height: 20px; padding: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: 14px; line-height: 1; cursor: pointer; }
  .refresh:hover:not(:disabled) { background: var(--surface-muted); color: var(--text); }
  .refresh.busy { cursor: progress; }
  .refresh.error { color: var(--warn); }
</style>
