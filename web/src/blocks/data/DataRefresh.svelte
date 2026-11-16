<script lang="ts">
  import { hasCore } from '../../bridge'
  import Icon from '../../components/Icon.svelte'
  import { dataRefresh, refreshData } from './refresh.svelte'
  import { t } from '../../i18n.svelte'

  /** Reload GCP now (Data and BigQuery cost blocks). */
</script>

{#if hasCore()}
  <button
    class="btn sm icon ghost"
    class:busy={dataRefresh.busy}
    class:error={!!dataRefresh.error}
    disabled={dataRefresh.busy}
    onclick={refreshData}
    title={dataRefresh.busy ? t('Reading GCP…') : dataRefresh.error ? t('Refresh failed: {error}', { error: dataRefresh.error }) : t('Read GCP again now (cost included)')}
    aria-label={t('Refresh')}
  ><Icon name="refresh" spin={dataRefresh.busy} /></button>
{/if}

<style>
  .error { color: var(--warn); }
</style>
