<script lang="ts">
  import { t } from '../i18n.svelte'
  import { runner } from '../runner.svelte'
  import { live } from '../live.svelte'
  import { hasCore } from '../bridge'
  import { setMergePriority } from '../pipeline-actions'
  import { mergePriority, priorityLevels } from '../merge-queue'
  import type { MergeRequest } from '../types'

  /**
   * An MR's merge priority (`<prefix>N` label), as a segment: P0 first in the queue, then P1…, then
   * none (—). A click sets it, the MR's other priority label goes. Without the app, only the one set.
   */
  let { mr, prefix }: { mr: MergeRequest; prefix: string } = $props()

  const current = $derived(mergePriority(mr, prefix))
  const levels = $derived(live.snapshot ? priorityLevels(live.snapshot) : [0, 1, 2, 3])
  const task = runner()
  /** The level being set, shown at once. */
  let pending = $state<number | null | undefined>(undefined)

  function pick(n: number | null) {
    pending = n
    void task.run(() => setMergePriority(mr, n)).then(() => (pending = undefined))
  }
  const title = (n: number | null) =>
    n === null ? t('No merge priority: after the MRs with one, the oldest first') : t('Merge priority {n} (label {label})', { n, label: `${prefix}${n}` })
</script>

{#if hasCore()}
<span class="prio" class:error={!!task.error} title={task.error ? t('Could not change the label: {error}', { error: task.error }) : undefined}>
  {t('Priority')}
  <span class="seg" role="radiogroup" aria-label={t('Merge priority')}>
    {#each [...levels, null] as n (n ?? 'none')}
      {@const on = (pending !== undefined ? pending : current) === n}
      <button role="radio" aria-checked={on} class:on disabled={task.busy || on} title={title(n)} onclick={() => pick(n)}>{n === null ? '—' : `P${n}`}</button>
    {/each}
  </span>
</span>
{:else if current !== null}
  <span class="prio" title={title(current)}>{t('Priority')} <span class="seg"><button class="on" disabled>P{current}</button></span></span>
{/if}

<style>
  .prio { display: inline-flex; align-items: center; gap: 6px; color: var(--text-muted); }
  .prio.error { color: var(--ko); }
  .prio .seg > button { padding: 0 7px; font: 600 var(--fs-tag) var(--mono); }
  .prio .seg > button.on { color: color-mix(in srgb, var(--warn) 70%, var(--text)); cursor: default; }
  .prio .seg > button:disabled:not(.on) { cursor: progress; }
</style>
