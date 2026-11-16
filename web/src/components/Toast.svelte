<script lang="ts">
  import { fly } from 'svelte/transition'
  import { flip } from 'svelte/animate'
  import { dismissToast, holdToast, toasts, type ToastKind } from '../toast.svelte'
  import { reducedMotion } from '../fun.svelte'
  import Icon from './Icon.svelte'
  import type { IconName } from '../icons'

  /** The toasts (toast.svelte.ts), above the status bar on the right; on a phone, centred. */
  const ICON: Record<ToastKind, IconName> = { ok: 'check', info: 'info', error: 'warning', undo: 'undo' }
  const dur = (ms: number) => (reducedMotion() ? 0 : ms)
</script>

<div class="toasts" aria-live="polite">
  {#each toasts.list as x (x.id)}
    <div
      class="toast {x.kind}"
      role={x.kind === 'error' ? 'alert' : 'status'}
      onpointerenter={() => holdToast(x.id, true)}
      onpointerleave={() => holdToast(x.id, false)}
      in:fly={{ y: 10, duration: dur(200) }}
      out:fly={{ x: 24, duration: dur(140) }}
      animate:flip={{ duration: dur(160) }}
    >
      <span class="ic"><Icon name={ICON[x.kind]} /></span>
      <span class="msg">{x.message}</span>
      {#if x.action}{@const a = x.action}<button class="btn sm" onclick={() => { dismissToast(x.id); a.run() }}>{a.label}</button>{/if}
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: 16px; bottom: 40px; z-index: var(--z-toast); display: flex; flex-direction: column; align-items: flex-end; gap: var(--sp-3); pointer-events: none; max-width: calc(100vw - 32px); }
  /* A long message wraps within the screen, a path or an address broken too. */
  .toast {
    --k: var(--ok);
    pointer-events: auto; display: flex; align-items: center; gap: var(--sp-4); max-width: 420px; box-sizing: border-box;
    padding: var(--sp-3) var(--sp-3) var(--sp-3) var(--sp-5); border: 1px solid var(--border); border-left: 3px solid var(--k); border-radius: var(--r-card);
    background: var(--surface); color: var(--text); box-shadow: var(--shadow-pop); font-size: var(--fs-meta); line-height: 1.4;
  }
  .toast.info { --k: var(--info); }
  .toast.error { --k: var(--ko); }
  .toast.undo { --k: var(--text-muted); }
  .ic { display: inline-flex; flex: none; color: var(--k); }
  .msg { min-width: 0; overflow-wrap: anywhere; padding-right: var(--sp-2); }
  .toast:not(:has(.btn)) { padding-right: var(--sp-5); }
  :global(html.remote) .toasts { left: 16px; right: 16px; bottom: 24px; align-items: center; }
</style>
