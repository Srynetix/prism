<script lang="ts">
  import { t } from '../i18n.svelte'
  import { runner } from '../runner.svelte'
  import { forgeName, mrNumber } from '../forge'
  import { hasCore } from '../bridge'
  import { rebaseMr } from '../pipeline-actions'
  import type { MergeRequest } from '../types'
  import Icon from './Icon.svelte'

  /** ↻ on an MR behind its target: rebase by GitLab in one click (no Claude, no local worktree). */
  let { mr }: { mr: MergeRequest } = $props()

  const task = runner()

  function onclick(e: MouseEvent) {
    e.stopPropagation()
    void task.run(() => rebaseMr(mr))
  }
</script>

{#if hasCore()}
  <button
    class="rebase"
    class:error={!!task.error}
    disabled={task.busy}
    {onclick}
    title={task.error ? t('Rebase failed: {error}', { error: task.error }) : t('Behind {branch}: {forge} rebases {mr} onto it', { branch: mr.target_branch, forge: forgeName(mr.forge), mr: mrNumber(mr.iid, mr.forge) })}
    aria-label={t('Rebase {mr}', { mr: mrNumber(mr.iid, mr.forge) })}
  ><Icon name="rebase" />{#if task.busy}<span>{t('rebasing…')}</span>{/if}</button>
{:else}
  <span class="rebase" title={t('Behind {branch}: rebase needed before merging', { branch: mr.target_branch })}><Icon name="rebase" /></span>
{/if}

<style>
  .rebase { display: inline-flex; align-items: center; gap: 3px; margin-left: 4px; padding: 0 4px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--warn); font: inherit; font-size: var(--fs-body); line-height: 1.3; cursor: pointer; vertical-align: middle; }
  .rebase span { font-size: var(--fs-tag); }
  button.rebase:hover { border-color: color-mix(in srgb, var(--warn) 50%, transparent); }
  .rebase.error { color: var(--ko); border-color: var(--ko); }
  .rebase:disabled { cursor: progress; }
</style>
