<script lang="ts">
  import { t } from '../i18n.svelte'
  import { runner } from '../runner.svelte'
  import { forgeName, mrNumber } from '../forge'
  import { hasCore } from '../bridge'
  import { setAutoMerge } from '../pipeline-actions'
  import type { MergeRequest } from '../types'
  import Icon from './Icon.svelte'

  /**
   * Auto-merge of an MR from the Merge column, one click to enable it as to turn it off:
   * GitLab merges as soon as everything is green, so right away if that is already the case.
   */
  let { mr }: { mr: MergeRequest } = $props()

  const task = runner()

  function onclick(e: MouseEvent) {
    e.stopPropagation()
    void task.run(() => setAutoMerge(mr, !mr.auto_merge))
  }

  const title = $derived(
    task.error
      ? t('Auto-merge failed: {error}', { error: task.error })
      : mr.auto_merge
        ? t('Cancel auto-merge of {mr}', { mr: mrNumber(mr.iid, mr.forge) })
        : t('Auto-merge: {forge} merges {mr} into {branch} as soon as pipeline, approvals and threads allow it (right away if they already do)', { forge: forgeName(mr.forge), mr: mrNumber(mr.iid, mr.forge), branch: mr.target_branch }),
  )
</script>

{#if hasCore()}
  <button class="am" class:on={mr.auto_merge} class:error={!!task.error} disabled={task.busy} {onclick} {title} aria-label={mr.auto_merge ? t('Cancel auto-merge of {mr}', { mr: mrNumber(mr.iid, mr.forge) }) : t('Auto-merge {mr}', { mr: mrNumber(mr.iid, mr.forge) })}>
    {#if task.busy}…{:else if mr.auto_merge}<Icon name="close" />{:else}<Icon name="automerge" />{/if}
  </button>
{/if}

<style>
  .am { display: inline-flex; align-items: center; gap: 3px; margin-left: 4px; padding: 0 4px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--info); font: inherit; font-size: var(--fs-body); line-height: 1.3; cursor: pointer; vertical-align: middle; opacity: 0; transition: opacity 0.15s; }
  :global(tr:hover) .am, .am:focus-visible, .am.error, .am:disabled { opacity: 1; }
  .am:hover { border-color: color-mix(in srgb, var(--info) 50%, transparent); }
  .am.on { color: var(--text-muted); }
  .am.error { color: var(--ko); border-color: var(--ko); }
  .am:disabled { cursor: progress; }
</style>
