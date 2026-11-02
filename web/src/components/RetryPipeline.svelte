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
    class="retry"
    class:error={!!task.error}
    {onclick}
    disabled={task.busy}
    title={task.error ? t('Could not retry the pipeline: {error}', { error: task.error }) : t('Retry the {status} jobs of this pipeline', { status: status ?? '' })}
    aria-label={t('Retry the pipeline')}
  ><Icon name="retry" />{#if label || task.busy || task.error}<span>{task.busy ? t('retrying…') : task.error ? tc('mrs', 'failed') : t('Retry pipeline')}</span>{/if}</button>
{/if}

<style>
  .retry { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; height: 18px; padding: 0 5px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-body); line-height: 1; cursor: pointer; }
  .retry span { font-size: var(--fs-meta); }
  .retry:hover { color: var(--info); border-color: color-mix(in srgb, var(--info) 40%, transparent); }
  .retry.error { color: var(--ko); border-color: var(--ko); background: color-mix(in srgb, var(--ko) 12%, transparent); }
  .retry:disabled { opacity: 0.6; cursor: progress; }
</style>
