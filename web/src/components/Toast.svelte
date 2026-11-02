<script lang="ts">
  import { toastState } from '../toast.svelte'
  import Icon from './Icon.svelte'

  /** Brief confirmation (toast.svelte.ts), above the status bar. */
</script>

{#if toastState.message}
  {#key toastState.id}
    <div class="toast" class:act={toastState.action} role="status" aria-live="polite"><Icon name="check" /><span class="msg">{toastState.message}</span>{#if toastState.action}{@const a = toastState.action}<button onclick={() => { toastState.message = ''; a.run() }}>{a.label}</button>{/if}</div>
  {/key}
{/if}

<style>
  /* A long message wraps within the screen (16 px aside), a path or an address broken too. */
  .toast { position: fixed; left: 50%; bottom: 40px; z-index: 60; transform: translateX(-50%); display: flex; align-items: flex-start; gap: 6px; width: max-content; max-width: calc(100vw - 32px); box-sizing: border-box; padding: 6px 12px; border-radius: var(--r8); background: var(--text); color: var(--surface); font-size: var(--fs-meta); font-weight: 500; line-height: 1.4; box-shadow: var(--shadow-pop); pointer-events: none; animation: rise 0.18s ease-out; }
  /* On the first line, however many there are. */
  .toast :global(svg) { flex: none; margin-top: 0.2em; }
  .msg { min-width: 0; overflow-wrap: anywhere; }
  /* With a button: it takes the pointer. */
  .toast.act { align-items: center; gap: 8px; pointer-events: auto; }
  .toast button { flex: none; white-space: nowrap; padding: 0 2px; border: none; background: none; color: var(--src-claude); font: inherit; font-weight: 600; cursor: pointer; }
  .toast button:hover { text-decoration: underline; }
  @keyframes rise { from { opacity: 0; transform: translate(-50%, 6px); } }
  :global(html[data-reduce-motion]) .toast { animation: none; }
</style>
