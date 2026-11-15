<script lang="ts">
  import { t, tc } from '../i18n.svelte'
  import { runner } from '../runner.svelte'
  import Icon from './Icon.svelte'
  import { canRetry, retryPipeline } from '../pipeline-actions'

  /**
   * ⟳ on a failed or cancelled pipeline: GitLab retries its failed or cancelled jobs. A single
   * click: nothing is lost, and the retried pipeline can still be stopped.
   */
  let { id, status, label = false }: { id: number; status: string | null; label?: boolean } = $props()

  const task = runner()

  function onclick(e: MouseEvent) {
    e.preventDefault()
    e.stopPropagation()
    void task.run(() => retryPipeline(id))
  }
</script>

{#if canRetry(status) || task.error || task.busy}
  <button
    class="btn sm ghost retry"
    class:error={!!task.error}
    class:busy={task.busy}
    {onclick}
    disabled={task.busy}
    title={task.error ? t('Could not retry the pipeline: {error}', { error: task.error }) : t('Retry the {status} jobs of this pipeline', { status: status ?? '' })}
    aria-label={t('Retry the pipeline')}
  >{#if task.busy}<span class="spinner"></span>{:else}<Icon name="retry" />{/if}{#if label || task.busy || task.error}<span>{task.busy ? t('retrying…') : task.error ? tc('mrs', 'failed') : t('Retry pipeline')}</span>{/if}</button>
{/if}

<style>
  .retry { padding: 0 var(--sp-3); }
  .retry:hover { color: var(--info); }
  .retry.error { color: var(--ko); border-color: var(--ko); }
</style>
