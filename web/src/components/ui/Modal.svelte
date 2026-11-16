<script lang="ts">
  import type { Snippet } from 'svelte'
  import { fade, scale } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import { t } from '../../i18n.svelte'
  import { reducedMotion } from '../../fun.svelte'
  import { pushLayer, zIndex } from '../../layers.svelte'
  import Icon from '../Icon.svelte'

  /**
   * A dialog, the one shell of them all (palette, new ticket, feedback, agent switch…): centered
   * near the top over a veil (`--scrim`), in a width of its own (`width`, px). With a `title`, a
   * header with it and a ×; without, the content draws its own (the palette's field). Escape and a
   * click on the veil close it, through the stack of layers (layers.svelte.ts), which also moves
   * the focus in (`autofocus`, else the first field) and back.
   */
  let {
    open,
    key,
    label,
    title,
    width = 620,
    top = '10vh',
    onclose,
    head,
    children,
    footer,
    class: cls = '',
    el = $bindable(),
  }: {
    open: boolean
    key: string
    label: string
    title?: string
    width?: number
    /** Where it sits: 10vh, as all; a taller one asks less. */
    top?: string
    onclose: () => void
    /** Beside the title, before the ×. */
    head?: Snippet
    children: Snippet
    footer?: Snippet
    class?: string
    el?: HTMLElement
  } = $props()

  let wrap: HTMLElement | undefined = $state()
  let id = $state(0)

  $effect(() => {
    if (!open) return
    const layer = pushLayer({ kind: 'modal', key, el: () => wrap, close: () => onclose() })
    id = layer.id
    return () => {
      layer.remove()
      id = 0
    }
  })

  const dur = (ms: number) => (reducedMotion() ? 0 : ms)
</script>

{#if open}
  <div class="veil" role="presentation" bind:this={wrap} style:z-index={id ? zIndex(id) : 'var(--z-modal)'} data-layer onpointerdown={(e) => e.target === e.currentTarget && onclose()} transition:fade={{ duration: dur(140) }}>
    <div class="modal {cls}" bind:this={el} style:--w="{width}px" style:margin-top={top} role="dialog" aria-modal="true" aria-label={label} data-layer tabindex="-1" in:scale={{ start: 0.98, duration: dur(180), easing: cubicOut }}>
      {#if title}
        <header class="mh">
          <h2>{title}</h2>
          {@render head?.()}
          <button class="btn sm icon ghost" onclick={onclose} title={t('Close (Esc)')} aria-label={t('Close')}><Icon name="close" /></button>
        </header>
      {/if}
      {@render children()}
      {#if footer}<footer class="mf">{@render footer()}</footer>{/if}
    </div>
  </div>
{/if}

<style>
  .veil {
    position: fixed; inset: 0; display: flex; justify-content: center; align-items: flex-start;
    background: var(--scrim); backdrop-filter: blur(2px); -webkit-backdrop-filter: blur(2px);
  }
  :global(html[data-reduce-transparency]) .veil { backdrop-filter: none; -webkit-backdrop-filter: none; background: color-mix(in srgb, var(--bg) 70%, transparent); }
  .modal {
    width: min(var(--w), calc(100vw - 32px)); max-height: calc(100vh - 2 * var(--sp-7) - 6vh); box-sizing: border-box;
    display: flex; flex-direction: column; overflow: hidden;
    background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: var(--r-layer);
    box-shadow: var(--shadow-modal); outline: none;
  }
  .mh { display: flex; align-items: center; gap: var(--sp-4); padding: var(--sp-4) var(--sp-4) var(--sp-4) var(--sp-6); border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .mh h2 { flex: 1; min-width: 0; margin: 0; font-size: var(--fs-title); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mf { display: flex; align-items: center; justify-content: flex-end; gap: var(--sp-3); padding: var(--sp-4) var(--sp-6); border-top: 1px solid var(--border); flex-shrink: 0; }
  :global(html.remote) .veil { align-items: flex-end; }
  :global(html.remote) .modal { margin-top: 0 !important; width: 100%; max-height: 90vh; border-radius: var(--r-layer) var(--r-layer) 0 0; }
</style>
