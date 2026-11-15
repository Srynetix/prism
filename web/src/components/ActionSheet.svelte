<script lang="ts">
  import Icon from './Icon.svelte'
  import Sheet from './ui/Sheet.svelte'
  import { t } from '../i18n.svelte'
  import { openLink } from '../links'
  import { armer } from '../armed.svelte'
  import { toastError } from '../toast.svelte'
  import type { FlatAction } from '../menus'

  /**
   * A phone's actions sheet, from the bottom: an object's actions (its ⋯ menu, `flatten`) by group.
   * One that cannot be undone asks for a second tap (« Tap again to confirm », its label under it); a tap outside,
   * or one that runs, closes it.
   */
  let { title, actions, onclose }: { title: string; actions: FlatAction[]; onclose: () => void } = $props()
  const armed = armer()
  let busy = $state<string | null>(null)

  async function run(a: FlatAction) {
    if (busy) return
    if (a.confirm && !armed.confirm(a.key)) return
    if (a.href && !a.run) {
      openLink(a.href)
      return onclose()
    }
    busy = a.key
    try {
      await a.run?.()
      onclose()
    } catch (e) {
      toastError(String(e), () => run(a))
    } finally {
      busy = null
    }
  }
</script>

<Sheet key="actions" label={t('Actions on {title}', { title })} {onclose}>
  <p class="title">{title}</p>
  <div class="list">
    {#each actions as a, i (a.key + i)}
      {#if a.group && a.group !== actions[i - 1]?.group}<p class="g">{a.group}</p>{/if}
      <button class="a" class:armed={armed.is(a.key)} disabled={busy === a.key} onclick={() => run(a)}>
        <span>{armed.is(a.key) ? t('Tap again to confirm') : a.label}{#if armed.is(a.key)}<small>{a.label}</small>{:else if a.hint}<small>{a.hint}</small>{/if}</span>
        {#if busy === a.key}<span class="spinner"></span>{:else if a.href}<Icon name="external" />{/if}
      </button>
    {/each}
  </div>
</Sheet>

<style>
  .title { margin: 0 4px 4px; overflow: hidden; color: var(--text-muted); font-size: var(--fs-meta); text-overflow: ellipsis; white-space: nowrap; }
  .list { overflow-y: auto; overscroll-behavior: contain; }
  .g { margin: 12px 4px 2px; color: var(--text-muted); font-size: var(--fs-micro); letter-spacing: 0.05em; text-transform: uppercase; }
  .a { display: flex; align-items: center; justify-content: space-between; gap: 10px; width: 100%; padding: 12px 6px; border: 0; border-bottom: 1px solid var(--border); background: none; color: var(--text); font: inherit; font-size: var(--fs-title); text-align: left; }
  .a small { margin-left: 8px; color: var(--text-muted); font-size: var(--fs-tag); }
  .a.armed { background: var(--ko-bg); color: var(--ko); font-weight: 600; }
  .a:disabled { opacity: 0.6; }
</style>
