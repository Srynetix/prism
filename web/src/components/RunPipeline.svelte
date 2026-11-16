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
    class="btn sm ghost run"
    class:error={!!task.error}
    class:busy={task.busy}
    {onclick}
    disabled={task.busy}
    title={task.error ? t('Could not run a pipeline: {error}', { error: task.error }) : t('Run a new pipeline for {mr}, with its labels now', { mr: mrNumber(mr.iid, mr.forge) })}
    aria-label={t('Run pipeline')}
  >{#if task.busy}<span class="spinner"></span>{:else}<Icon name="play" />{/if}{#if label || task.busy || task.error}<span>{task.busy ? t('starting…') : task.error ? tc('mrs', 'failed') : t('Run pipeline')}</span>{/if}</button>
{/if}

<style>
  .run { padding: 0 var(--sp-3); }
  .run:hover { color: var(--info); }
  .run.error { color: var(--ko); border-color: var(--ko); }
</style>
