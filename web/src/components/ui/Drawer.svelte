<script lang="ts">
  import type { Snippet } from 'svelte'
  import { fly } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import { t } from '../../i18n.svelte'
  import { reducedMotion } from '../../fun.svelte'
  import { isPinned, type DrawerKind } from '../../drawer-pins'
  import { below, closeAbove, front, isTop, layers, pushLayer, zIndex } from '../../layers.svelte'
  import Icon from '../Icon.svelte'
  import PinButton from '../PinButton.svelte'

  /**
   * A side drawer, the one shell of them all (MR, ticket, logs, event, thread…): on the right, over
   * the page, in one of three widths, S a record (520), M a conversation (640), L a log, a terminal,
   * a wide diff, two panes side by side (960); `xl` the MR's changes (1320), `full` the whole window (the MR's full screen,
   * a phone). Its header carries the crumbs of the drawers under it, the colour of its source on
   * top (`source`), what the panel puts in it (`head`), its pin and its ×. The stack of layers
   * (layers.svelte.ts) closes it on Escape and on a click outside, sets its rank, moves the focus
   * in and back. `key` names what it shows ('mr'); when `identity` changes while open (another MR),
   * it comes on top.
   */
  let {
    open,
    key,
    identity = '',
    crumb = '',
    label,
    size = 's',
    source = 'var(--accent)',
    pin,
    escape = true,
    outside = true,
    header = true,
    onclose,
    head,
    children,
    class: cls = '',
    el = $bindable(),
  }: {
    open: boolean
    key: string
    identity?: string
    /** Its name in the crumbs of a drawer opened over it. */
    crumb?: string
    /** For assistive technologies. */
    label: string
    size?: 's' | 'm' | 'l' | 'xl' | 'full'
    source?: string
    pin?: DrawerKind
    /** Escape closes it: a terminal keeps it. */
    escape?: boolean
    /** A click outside closes it (unless pinned): false for a form a stray click must not lose. */
    outside?: boolean
    /** The standard header (crumbs, head, pin, ×); false when the panel draws its own. */
    header?: boolean
    onclose: () => void
    head?: Snippet
    children: Snippet
    class?: string
    el?: HTMLElement
  } = $props()

  let id = $state(0)

  $effect(() => {
    if (!open) return
    const layer = pushLayer({ kind: 'drawer', key, label: () => crumb, el: () => el, close: () => onclose(), pinned: () => (pin ? isPinned(pin) : false), escape, outside })
    id = layer.id
    return () => {
      layer.remove()
      id = 0
    }
  })

  // Another thing shown in it while it is open, under another drawer: it comes on top.
  let shown = ''
  $effect(() => {
    const now = identity
    if (id && shown && now !== shown) front(id)
    shown = now
  })

  /** Its rank, kept once it left the stack: it slides away still above the page. */
  let rank = $state('var(--z-drawer)')
  $effect.pre(() => {
    if (id) rank = zIndex(id)
  })
  const crumbs = $derived(id ? below(id) : [])
  const top = $derived(id ? isTop(id) && layers.stack.length > 0 : true)
  const motion = (dur: number) => ({ x: 32, duration: reducedMotion() ? 0 : dur, easing: cubicOut, opacity: 0 })
</script>

{#if open}
  <aside
    class="drawer {size} {cls}"
    class:under={!top}
    bind:this={el}
    style:--src={source}
    style:z-index={rank}
    data-drawer={id ? layers.stack.findIndex((l) => l.id === id) + 1 : 1}
    data-pinned={pin ? isPinned(pin) : false}
    aria-label={label}
    tabindex="-1"
    in:fly={motion(200)}
    out:fly={motion(120)}
  >
    {#if header}
      {#if crumbs.length}
        <nav class="crumbs" aria-label={t('Opened from')}>
          {#each crumbs as c (c.id)}
            <button onclick={() => closeAbove(c.id)} title={t('Back to {name}', { name: c.label?.() ?? '' })}><Icon name="chevronLeft" />{c.label?.()}</button>
          {/each}
        </nav>
      {/if}
      <header class="dh">
        {@render head?.()}
        <span class="end">
          {#if pin}<PinButton kind={pin} />{/if}
          <button class="btn sm icon ghost" onclick={onclose} title={t('Close (Esc)')} aria-label={t('Close')}><Icon name="close" /></button>
        </span>
      </header>
    {/if}
    {@render children()}
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; width: min(var(--dw-s), 92vw);
    display: flex; flex-direction: column; background: var(--surface); color: var(--text);
    border-left: 1px solid color-mix(in srgb, var(--src) 45%, var(--border));
    box-shadow: var(--shadow-drawer); outline: none;
    transition: transform var(--t-mid) var(--ease), filter var(--t-mid) var(--ease);
  }
  .drawer.m { width: min(var(--dw-m), 94vw); }
  .drawer.l { width: min(var(--dw-l), 94vw); }
  .drawer.xl { width: min(1320px, 96vw); }
  .drawer.full { left: 0; width: auto; border-left: none; box-shadow: none; }
  /* Narrow (app.css, « Widths »): the whole width, whatever its size; from edge to edge, not
     100vw, which counts a scrollbar's room too. */
  @media (max-width: 719.98px) { .drawer:is(.m, .l, .xl), .drawer { left: 0; width: auto; border-left: none; } }
  /* Under the one on top: it steps back, still readable. */
  .drawer.under { transform: translateX(-24px); filter: brightness(0.92); }
  :global(html[data-reduce-motion]) .drawer { transition: none; }
  :global(html[data-reduce-motion]) .drawer.under { transform: none; }
  .crumbs { display: flex; align-items: center; gap: 2px; flex-wrap: wrap; padding: var(--sp-2) var(--sp-4) 0; flex-shrink: 0; background: var(--surface); }
  .crumbs button { display: inline-flex; align-items: center; gap: 2px; height: 18px; padding: 0 var(--sp-3) 0 2px; border: 0; border-radius: var(--r4); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .crumbs button:hover { background: var(--surface-muted); color: var(--text); }
  .dh {
    display: flex; align-items: center; gap: var(--sp-4); min-height: 40px; padding: var(--sp-3) var(--sp-4) var(--sp-3) var(--sp-5); flex-shrink: 0;
    border-bottom: 1px solid var(--border); background: var(--surface); box-shadow: inset 0 2px var(--src);
    font-size: var(--fs-meta); color: var(--text-muted); box-sizing: border-box;
  }
  .crumbs + .dh { box-shadow: none; }
  .drawer:has(.crumbs) { box-shadow: var(--shadow-drawer), inset 0 2px var(--src); }
  /* On the phone, full screen: below the notch. */
  :global(html.remote) .drawer.full > :first-child { padding-top: calc(var(--sp-3) + env(safe-area-inset-top)); }
  .end { display: inline-flex; align-items: center; gap: var(--sp-1); margin-left: auto; flex-shrink: 0; }
</style>
