<script lang="ts">
  import { t, tc } from '../i18n.svelte'
  import { offers, trackerName } from '../tracker'
  import { hasCore } from '../bridge'
  import { setPriority, type Priority } from '../ticket-view.svelte'

  /**
   * A ticket's priority: a coloured flag and its name, readable at a glance (Urgent and High
   * on a tinted background, Normal and Low more discreet, nothing visible without a priority until
   * hovered). In the app, it is also the picker that changes it in ClickUp.
   */
  let { taskId, priority, label = true }: { taskId: string; priority: Priority | null; label?: boolean } = $props()

  const NAMES: Record<Priority, string> = { urgent: t('Urgent'), high: t('High'), normal: tc('tickets', 'Normal'), low: t('Low') }
  const LEVELS: Priority[] = ['urgent', 'high', 'normal', 'low']

  let saving = $state(false)
  let error = $state<string | null>(null)
  let select: HTMLSelectElement | undefined = $state()

  async function onchange(e: Event & { currentTarget: HTMLSelectElement }) {
    const v = e.currentTarget.value
    const next = v ? (v as Priority) : null
    if (next === priority) return
    saving = true
    error = null
    try {
      await setPriority(taskId, next)
    } catch (err) {
      error = String(err)
      // The picker reverts to the actual priority.
      if (select) select.value = priority ?? ''
    } finally {
      saving = false
    }
  }
  const title = $derived(error ? t('Priority change failed: {error}', { error }) : `${priority ? t('{level} priority', { level: NAMES[priority] }) : t('No priority')}${hasCore() && offers('priority') ? ` · ${t('change it in {tracker}', { tracker: trackerName() })}` : ''}`)
</script>

{#snippet face()}
  <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2.5 11V1.5h7l-1.6 2.6 1.6 2.6h-7" /></svg>
  {#if label && priority}<span>{NAMES[priority]}</span>{/if}
{/snippet}

{#if hasCore() && offers('priority')}
  <label class="prio {priority ?? 'none'}" class:error={!!error} class:saving {title}>
    {@render face()}
    <select class="bare" bind:this={select} value={priority ?? ''} disabled={saving} {onchange} aria-label={t('Priority')}>
      {#each LEVELS as p (p)}<option value={p}>{NAMES[p]}</option>{/each}
      <option value="">{t('No priority')}</option>
    </select>
  </label>
{:else}
  <span class="prio {priority ?? 'none'}" {title}>{@render face()}</span>
{/if}

<style>
  .prio { position: relative; display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; padding: 0 5px 0 3px; border-radius: var(--r4); font-size: var(--fs-tag); font-weight: 600; line-height: 16px; white-space: nowrap; color: var(--c); --c: var(--text-muted); }
  .prio svg { width: 10px; height: 10px; fill: currentColor; stroke: currentColor; stroke-width: 1; stroke-linejoin: round; }
  .urgent { --c: var(--ko); background: var(--ko-bg); }
  .high { --c: var(--warn); background: var(--warn-bg); }
  .normal { --c: var(--info); font-weight: 500; }
  .low { --c: var(--text-muted); font-weight: 500; }
  /* No priority: an empty flag, visible on row hover to set one. */
  .none { padding: 0 2px; }
  .none svg { fill: none; opacity: 0.35; }
  :global(:is(.tk, .card):hover) .none svg, label.none:hover svg { opacity: 0.8; }
  /* The native select, invisible, covers the label: a click opens it. */
  select { position: absolute; inset: 0; width: 100%; opacity: 0; cursor: pointer; font: inherit; }
  label:hover { box-shadow: inset 0 0 0 1px var(--c); }
  .error { box-shadow: inset 0 0 0 1px var(--ko); }
  .saving { opacity: 0.6; }
</style>
