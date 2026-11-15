<script lang="ts">
  import type { Snippet } from 'svelte'
  import { pushLayer, zIndex } from '../../layers.svelte'

  /**
   * The phone's bottom sheet, the one shell of them all (an object's actions, an agent's ⋯ and
   * tools, a new session, the profiles…): a scrim, the sheet risen from the bottom with its
   * grabber, the home bar's room under it. A layer of the stack (kind 'modal', layers.svelte.ts):
   * Escape closes it, and so does Android's Back (RemoteApp counts the open ones in its history);
   * a tap on the scrim too, unless `dismiss` is false (a prompt being written is not lost to a
   * stray thumb). Its content draws its own header and lists; `fill` lets it take the height and
   * scroll inside (a form with a footer), otherwise it is as tall as what it holds, at most 85 %.
   */
  let {
    key,
    label,
    onclose,
    dismiss = true,
    fill = false,
    class: cls = '',
    children,
  }: {
    key: string
    label: string
    onclose: () => void
    dismiss?: boolean
    fill?: boolean
    class?: string
    children: Snippet
  } = $props()

  let el: HTMLElement | undefined = $state()
  let id = $state(0)

  $effect(() => {
    const layer = pushLayer({ kind: 'modal', key, el: () => el, close: () => onclose() })
    id = layer.id
    return () => {
      layer.remove()
      id = 0
    }
  })
</script>

<!-- svelte-ignore a11y_click_events_have_key_events : Escape and Back close it too -->
<div class="scrim" role="presentation" style:z-index={id ? zIndex(id) : 'var(--z-modal)'} onclick={() => dismiss && onclose()}></div>
<div class="sheet {cls}" class:fill bind:this={el} role="dialog" aria-modal="true" aria-label={label} tabindex="-1" data-layer style:z-index={id ? zIndex(id) : 'var(--z-modal)'}>
  <div class="grab" aria-hidden="true"></div>
  {@render children()}
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgb(0 0 0 / 0.4); }
  .sheet {
    position: fixed; left: 0; right: 0; bottom: 0; box-sizing: border-box;
    display: flex; flex-direction: column; max-height: calc(100dvh - 24px - env(safe-area-inset-top));
    padding: var(--sp-3) var(--sp-6) calc(var(--sp-6) + env(safe-area-inset-bottom));
    border-radius: var(--r-layer) var(--r-layer) 0 0; background: var(--surface); color: var(--text);
    box-shadow: 0 -10px 40px rgb(0 0 0 / 0.3); outline: none;
  }
  /* A form: its own scroll and footer, to the edges. */
  .sheet.fill { padding: 0; background: var(--bg); }
  .grab { flex: none; width: 38px; height: 5px; margin: var(--sp-1) auto var(--sp-4); border-radius: var(--r3); background: var(--border); }
  .fill .grab { margin-top: var(--sp-4); margin-bottom: 0; }
  :global(html:not([data-reduce-motion])) .scrim { animation: fade var(--t-mid) ease-out; }
  :global(html:not([data-reduce-motion])) .sheet { animation: rise var(--t-mid) var(--ease); }
  @keyframes rise { from { transform: translateY(100%); } }
  @keyframes fade { from { opacity: 0; } }
</style>
