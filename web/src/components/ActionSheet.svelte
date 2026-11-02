<script lang="ts">
  import Icon from './Icon.svelte'
  import { t } from '../i18n.svelte'
  import { openLink } from '../links'
  import { toast } from '../toast.svelte'
  import type { FlatAction } from '../menus'

  /**
   * A phone's actions sheet, from the bottom: an object's actions (its ⋯ menu, `flatten`) by group.
   * One that cannot be undone asks for a second tap; a tap outside, or one that runs, closes it.
   */
  let { title, actions, onclose }: { title: string; actions: FlatAction[]; onclose: () => void } = $props()
  let armed = $state<string | null>(null)
  let busy = $state<string | null>(null)

  async function run(a: FlatAction) {
    if (busy) return
    if (a.confirm && armed !== a.key) return void (armed = a.key)
    if (a.href && !a.run) {
      openLink(a.href)
      return onclose()
    }
    busy = a.key
    try {
      await a.run?.()
      onclose()
    } catch (e) {
      toast(String(e), 6000)
    } finally {
      busy = null
      armed = null
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions : the sheet's own buttons are the keyboard path -->
<div class="dim" onclick={onclose}></div>
<div class="sheet" role="dialog" aria-modal="true" aria-label={t('Actions on {title}', { title })}>
  <div class="grab" aria-hidden="true"></div>
  <p class="title">{title}</p>
  <div class="list">
    {#each actions as a, i (a.key + i)}
      {#if a.group && a.group !== actions[i - 1]?.group}<p class="g">{a.group}</p>{/if}
      <button class="a" class:armed={armed === a.key} disabled={busy === a.key} onclick={() => run(a)}>
        <span>{armed === a.key ? a.confirm : a.label}{#if a.hint && armed !== a.key}<small>{a.hint}</small>{/if}</span>
        {#if busy === a.key}…{:else if a.href}<Icon name="external" />{/if}
      </button>
    {/each}
  </div>
</div>

<style>
  .dim { position: fixed; inset: 0; z-index: 80; background: rgba(0, 0, 0, 0.4); }
  .sheet { position: fixed; left: 0; right: 0; bottom: 0; z-index: 81; display: flex; flex-direction: column; max-height: 80vh; padding: 8px 12px calc(12px + env(safe-area-inset-bottom)); border-radius: 16px 16px 0 0; background: var(--surface); box-shadow: 0 -10px 40px rgba(0, 0, 0, 0.35); animation: up 0.2s cubic-bezier(0.2, 0.8, 0.2, 1); }
  @keyframes up { from { transform: translateY(100%); } }
  .grab { width: 36px; height: 4px; margin: 0 auto 8px; border-radius: 2px; background: var(--border); }
  .title { margin: 0 4px 4px; overflow: hidden; color: var(--text-muted); font-size: 13px; text-overflow: ellipsis; white-space: nowrap; }
  .list { overflow-y: auto; overscroll-behavior: contain; }
  .g { margin: 12px 4px 2px; color: var(--text-muted); font-size: 11px; letter-spacing: 0.05em; text-transform: uppercase; }
  .a { display: flex; align-items: center; justify-content: space-between; gap: 10px; width: 100%; padding: 12px 6px; border: 0; border-bottom: 1px solid var(--border); background: none; color: var(--text); font: inherit; font-size: 15px; text-align: left; }
  .a small { margin-left: 8px; color: var(--text-muted); font-size: 12px; }
  .a.armed { color: var(--warn); }
  .a:disabled { opacity: 0.6; }
</style>
