<script lang="ts" module>
  import type { Snippet } from 'svelte'

  /**
   * A tool of the bar: drawn in the bar, or in its ⋯ once there is no room (`menu` true then: a
   * tool may draw itself otherwise there, a list of tabs as a selector).
   */
  export interface Tool {
    id: string
    render: Snippet<[menu: boolean]>
    /** The lowest goes first into the ⋯; `Infinity`: never. Of two alike, the one further right. */
    rank?: number
    /** After the space that pushes to the right end (`end`), or right after the lead. */
    at?: 'start' | 'end'
    /** False: nothing to show now (out of the bar and of its ⋯, never weighed). */
    shown?: boolean
    /**
     * Something of its own is open, asked from elsewhere (a drawer, a popover by a key): folded,
     * the ⋯ opens for it to show there.
     */
    wanted?: boolean
    /** The ⋯ closed while it was wanted there: what it had open is closed too. */
    dismiss?: () => void
  }
</script>

<script lang="ts">
  import { untrack } from 'svelte'
  import { foldOut } from '../fold'
  import { anchored } from '../popup'
  import { holdEscape } from '../layers.svelte'
  import Icon from './Icon.svelte'
  import { t } from '../i18n.svelte'

  /**
   * A bar of tools on one line, whatever its width: squeezed, its secondary tools go into a ⋯
   * (measured, as the status bar's items: no breakpoint per bar), its lead (a title, a filter)
   * shrinks down to its `min-width` first, and its tail (the main action, a menu) always stays.
   * `class` styles the bar as its place wants (paddings, gaps, its background).
   */
  let { tools, lead, tail, label, class: cls = '' }: { tools: Tool[]; lead?: Snippet; tail?: Snippet; label?: string; class?: string } = $props()

  let bar = $state<HTMLElement>()
  let leadEl = $state<HTMLElement>()
  let tailEl = $state<HTMLElement>()
  let moreButton = $state<HTMLElement>()
  let open = $state(false)
  let folded = $state<string[]>([])
  /** Each tool's last width in the bar, its gap included: weighed again from it once folded. */
  const widths = new Map<string, number>()
  const MORE = 30

  const px = (v: string) => parseFloat(v) || 0
  function weigh() {
    if (!bar) return
    const style = getComputedStyle(bar)
    const gap = px(style.columnGap)
    for (const el of bar.querySelectorAll<HTMLElement>(':scope > .tool[data-id]')) widths.set(el.dataset.id!, el.offsetWidth ? el.offsetWidth + gap : 0)
    // The lead at its narrowest (a `.shrink` in it, a title, cut down to 140 px; a filter at its
    // own min-width), the rest for the tools.
    let leadMin = 0
    if (leadEl) {
      leadEl.classList.add('measuring')
      leadMin = leadEl.offsetWidth + gap
      leadEl.classList.remove('measuring')
    }
    const room = bar.clientWidth - px(style.paddingLeft) - px(style.paddingRight) - leadMin - (tailEl ? tailEl.offsetWidth + gap : 0)
    const next = foldOut(
      present.map((x) => x.id),
      widths,
      room,
      MORE + gap,
      (id) => present.find((x) => x.id === id)?.rank ?? 0,
    )
    if (next.join() !== folded.join()) folded = next
    if (!next.length) close()
  }
  let frame = 0
  const soon = () => {
    cancelAnimationFrame(frame)
    frame = requestAnimationFrame(weigh)
  }
  const sizes = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(soon)
  /** The bar, and each tool in it: a tool saying more (a count, a longer label) weighs again. */
  const observe = (el: HTMLElement) => {
    sizes?.observe(el)
    return () => sizes?.unobserve(el)
  }
  $effect(() => {
    void present.map((x) => x.id).join()
    untrack(soon)
  })

  const present = $derived(tools.filter((x) => x.shown !== false))
  const inBar = $derived(present.filter((x) => !folded.includes(x.id)))
  const inMenu = $derived(present.filter((x) => folded.includes(x.id)))

  /* A folded tool wanted (its drawer asked for from elsewhere, or a tool folding while it is): the ⋯ opens. */
  let wantedFolded = new Set<string>()
  $effect(() => {
    const now = new Set(inMenu.filter((x) => x.wanted).map((x) => x.id))
    if ([...now].some((id) => !wantedFolded.has(id))) open = true
    wantedFolded = now
  })
  /** The ⋯ closed: what its tools had open from elsewhere, closed too (or it would come back later). */
  function close() {
    open = false
    for (const x of inMenu) if (x.wanted) x.dismiss?.()
  }

  /**
   * The ⋯ panel: a click outside closes it, but not one in a popup a tool opened from it (a date
   * picker, a menu attached to the page), nor Escape meant for that popup. Not `exclusive`: a
   * tool's own popup is, and opening it would close the ⋯, and the tool with it.
   */
  function panel(el: HTMLElement) {
    const opened = performance.now()
    const click = (e: MouseEvent) => {
      const target = e.target as Element | null
      if (e.timeStamp <= opened || !target?.isConnected || el.contains(target) || moreButton?.contains(target)) return
      if (target.closest('.popover, [role=menu], [role=dialog], [role=listbox]')) return
      close()
    }
    window.addEventListener('click', click)
    const letGo = holdEscape(close)
    return () => {
      letGo()
      window.removeEventListener('click', click)
    }
  }
</script>

<div class="toolbar {cls}" bind:this={bar} {@attach observe} role="toolbar" aria-label={label}>
  {#if lead}<div class="lead" bind:this={leadEl} {@attach observe}>{@render lead()}</div>{/if}
  {#each inBar.filter((x) => x.at !== 'end') as x (x.id)}<span class="tool" data-id={x.id} {@attach observe}>{@render x.render(false)}</span>{/each}
  <span class="grow"></span>
  {#each inBar.filter((x) => x.at === 'end') as x (x.id)}<span class="tool" data-id={x.id} {@attach observe}>{@render x.render(false)}</span>{/each}
  {#if inMenu.length}
    <button bind:this={moreButton} class="more" class:open onclick={(e) => { e.stopPropagation(); if (open) close(); else open = true }} aria-expanded={open} aria-label={t('More tools')} title={t('What has no room in the bar')}><Icon name="more" /></button>
  {/if}
  {#if tail}<div class="tail" bind:this={tailEl} {@attach observe}>{@render tail()}</div>{/if}
</div>

{#if open && inMenu.length}
  <div class="folded popover" role="dialog" aria-label={t('What has no room in the bar')} {@attach anchored(() => moreButton, { align: 'end' })} {@attach panel}>
    {#each inMenu as x (x.id)}<div class="row">{@render x.render(true)}</div>{/each}
  </div>
{/if}

<style>
  .toolbar { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .lead { display: flex; align-items: center; gap: inherit; flex: 0 1 auto; min-width: 0; }
  .lead.measuring { flex: none; width: min-content; }
  .lead.measuring :global(.shrink) { max-width: 140px; }
  .lead :global(.shrink) { min-width: min(140px, max-content); }
  .tool { display: inline-flex; align-items: center; gap: inherit; flex: none; }
  .tool:empty { display: none; }
  .grow { flex: 1; min-width: 0; }
  .tail { display: flex; align-items: center; gap: inherit; flex: none; }
  .more { display: inline-grid; place-items: center; flex: none; width: 26px; height: 26px; padding: 0; border: 1px solid transparent; border-radius: var(--r6); background: none; color: var(--text-muted); cursor: pointer; }
  .more:hover, .more.open { border-color: var(--border); background: var(--surface-muted); color: var(--text); }
  .folded { position: fixed; display: flex; flex-direction: column; align-items: stretch; gap: 6px; max-width: calc(100vw - 16px); }
  .folded .row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .folded .row:not(:has(> *)) { display: none; }
</style>
