<script lang="ts">
  import { t } from '../i18n.svelte'
  import { runner } from '../runner.svelte'
  import { hasCore } from '../bridge'
  import { setMergeQueue } from '../pipeline-actions'
  import { canQueue } from '../rules'
  import type { MergeRequest } from '../types'
  import Icon from './Icon.svelte'

  /**
   * An MR's merge-queue label: shown when set ("queued"), offered on
   * row hover otherwise. A click sets or removes it; the CI job then rebases the
   * MR and puts it on auto-merge.
   */
  let { mr, label, wide = false }: { mr: MergeRequest; label: string; wide?: boolean } = $props()

  const on = $derived(!!mr.labels?.includes(label))
  const task = runner()

  function onclick(e: MouseEvent) {
    e.stopPropagation()
    void task.run(() => setMergeQueue(mr, !on))
  }
  const title = $derived(
    task.error
      ? t('Could not change the label: {error}', { error: task.error })
      : on
        ? t('In the merge queue (label “{label}”): the CI job rebases it and sets auto-merge · click to remove', { label })
        : t('Add the “{label}” label: the CI job rebases this MR and sets auto-merge', { label }),
  )
</script>

{#if hasCore() && (on || canQueue(mr))}
  <button class="mq" class:on class:wide class:error={!!task.error} disabled={task.busy} {onclick} {title} aria-pressed={on}>
    {#if task.busy}…{:else if on}<Icon name="queue" /> {t('queued')}{:else}<Icon name="queue" />{#if wide}{' '}{t('add to merge queue')}{/if}{/if}
  </button>
{:else if on}
  <span class="mq on" {title}><Icon name="queue" /> {t('queued')}</span>
{/if}

<style>
  .mq { display: inline-flex; align-items: center; gap: 3px; margin-left: 4px; padding: 0 5px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--accent); font: inherit; font-size: var(--fs-tag); line-height: 1.5; white-space: nowrap; cursor: pointer; vertical-align: middle; opacity: 0; transition: opacity 0.15s; }
  :global(tr:hover) .mq, .mq:focus-visible, .mq.on, .mq.wide, .mq.error, .mq:disabled { opacity: 1; }
  .mq:hover { border-color: color-mix(in srgb, var(--accent) 50%, transparent); }
  .mq.on { background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--text); }
  .mq.on:hover { text-decoration: line-through; }
  .mq.wide { border-color: var(--border); }
  .mq.error { color: var(--ko); border-color: var(--ko); }
  .mq:disabled { cursor: progress; }
</style>
