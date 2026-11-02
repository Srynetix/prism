<script lang="ts">
  import { t, tc } from '../i18n.svelte'
  import { runner } from '../runner.svelte'
  import { mrNumber } from '../forge'
  import Icon from './Icon.svelte'
  import { canRunPipeline, runMrPipeline } from '../pipeline-actions'
  import type { MergeRequest } from '../types'

  /**
   * ▶ on an open GitLab MR: a new merge request pipeline, whose rules see the MR as it is now (a
   * pipeline that only runs once a label is set). A single click, as a retry: it can be stopped.
   */
  let { mr, label = false }: { mr: MergeRequest; label?: boolean } = $props()

  const task = runner()

  function onclick(e: MouseEvent) {
    e.preventDefault()
    e.stopPropagation()
    void task.run(() => runMrPipeline(mr))
  }
</script>

{#if canRunPipeline(mr) || task.error || task.busy}
  <button
    class="run"
    class:error={!!task.error}
    {onclick}
    disabled={task.busy}
    title={task.error ? t('Could not run a pipeline: {error}', { error: task.error }) : t('Run a new pipeline for {mr}, with its labels now', { mr: mrNumber(mr.iid, mr.forge) })}
    aria-label={t('Run pipeline')}
  ><Icon name="play" />{#if label || task.busy || task.error}<span>{task.busy ? t('starting…') : task.error ? tc('mrs', 'failed') : t('Run pipeline')}</span>{/if}</button>
{/if}

<style>
  .run { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; height: 18px; padding: 0 5px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-body); line-height: 1; cursor: pointer; }
  .run span { font-size: var(--fs-meta); }
  .run:hover { color: var(--info); border-color: color-mix(in srgb, var(--info) 40%, transparent); }
  .run.error { color: var(--ko); border-color: var(--ko); background: color-mix(in srgb, var(--ko) 12%, transparent); }
  .run:disabled { opacity: 0.6; cursor: progress; }
</style>
