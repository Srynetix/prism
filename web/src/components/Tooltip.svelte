<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { appearance } from '../fun.svelte'

  /**
   * Prism's tooltips, in place of the system's: the same `title` attributes, taken over while the
   * pointer is on their element (the attribute is set aside, so the system shows nothing, and put
   * back when it leaves: the DOM keeps its titles, a changed one is followed). The look is the
   * app's (theme, font), the delay `[appearance] tooltip_delay`, and a tooltip says more:
   * - its lines (`\n`) are kept;
   * - a shortcut at its end, `Settings (⌘,)`, `only mine (m)`, shows as keys;
   * - it shows under the pointer, where it is when the tooltip appears (as the system's), above it
   *   near the bottom of the window: a wide element (the status bar) keeps it next to the pointer;
   * - `data-tip` gives a tooltip without a `title` (nothing native, ever); `data-tip-side` places
   *   it against its element instead (`below`, `above`, `right`, `left`), flipped when there is no room.
   * `[appearance] tooltips` off: the system's, as before. Moving from one tooltip to the next, the
   * next one shows at once, as on macOS.
   */
  type Side = 'below' | 'above' | 'right' | 'left'
  const GAP = 6
  const MARGIN = 6
  /** Under the pointer: below its arrow (about this tall), or this far above it. */
  const BELOW_POINTER = 20
  const ABOVE_POINTER = 8
  /** After a tooltip hides, the next one within this time shows without waiting. */
  const WARM_MS = 500

  let box = $state<HTMLDivElement>()
  let text = $state('')
  let shown = $state(false)
  let pos = $state({ x: -9999, y: -9999, side: 'below' as Side })

  let current: HTMLElement | null = null
  /** The `title` set aside while the pointer is on `current`. */
  let saved: string | null = null
  /**
   * The titles of `current`'s ancestors, set aside as well: the system, finding none on `current`,
   * would show the nearest of theirs (a row's "Double-click to edit") over Prism's tooltip.
   */
  let above = new Map<Element, string>()
  let observer: MutationObserver | null = null
  let timer: ReturnType<typeof setTimeout> | undefined
  let watch: ReturnType<typeof setInterval> | undefined
  let hiddenAt = 0
  /** Where the pointer is, for a tooltip placed against it. */
  let pointer = { x: 0, y: 0 }

  const enabled = () => appearance().tooltips !== false
  const delay = () => Math.max(0, appearance().tooltip_delay ?? 400)

  /** A shortcut at the end, between parentheses: keys rather than words. */
  const KEYS = /^(?:.*[⌘⌃⌥⇧].*|Esc|Enter|Tab|Space|F\d{1,2}|\S)$/
  const parts = $derived.by(() => {
    const m = /\s*\(([^()]{1,24})\)$/.exec(text)
    if (!m || !m[1].split(/ or | \/ |, /).every((k) => KEYS.test(k.trim()))) return { body: text, keys: [] as string[] }
    return { body: text.slice(0, m.index), keys: m[1].split(/ or | \/ |, /).map((k) => k.trim()) }
  })

  /**
   * The element whose tooltip `target` shows: the nearest with a `title` or a `data-tip`, `current`
   * included, its title set aside, and the ancestors whose titles are set aside. Without it, the
   * pointer moving onto a child of `current` (its icon, its count) would pass over it to a titled
   * ancestor: the status bar's tooltip, instead of its item's. Never a frame: the pointer inside it
   * no longer reaches Prism, which would not see it leave and would keep the tooltip on screen.
   */
  function owner(target: EventTarget | null): HTMLElement | null {
    if (target instanceof HTMLIFrameElement) return null
    for (let n = target instanceof Element ? target : null; n; n = n.parentElement) {
      if (n === current || above.has(n) || n.hasAttribute('title') || n.hasAttribute('data-tip')) return n as HTMLElement
    }
    return null
  }

  function textOf(el: HTMLElement) {
    return (el.getAttribute('data-tip') ?? el.getAttribute('title') ?? saved ?? '').trim()
  }

  /** An ancestor's title set aside (again, when the app sets it back or gives it one). */
  function setAbove(n: Element) {
    const title = n.getAttribute('title')
    if (title === null) return
    above.set(n, title)
    n.removeAttribute('title')
  }

  function enter(el: HTMLElement) {
    current = el
    saved = el.getAttribute('title')
    if (saved !== null) el.removeAttribute('title')
    const ancestors: Element[] = []
    for (let n = el.parentElement; n; n = n.parentElement) ancestors.push(n)
    ancestors.forEach(setAbove)
    // The app changes the title while it is set aside (a count, a state): followed, and set aside again.
    observer = new MutationObserver((records) => {
      for (const r of records) if (r.target !== current && r.target instanceof Element) setAbove(r.target)
      if (!current?.hasAttribute('title')) return
      saved = current.getAttribute('title')
      current.removeAttribute('title')
      text = textOf(current)
      if (!text) hide()
      else if (shown) void place()
    })
    observer.observe(el, { attributes: true, attributeFilter: ['title', 'data-tip'] })
    // Every ancestor, not only the titled ones: one given a title during the hover is set aside too.
    for (const n of ancestors) observer.observe(n, { attributes: true, attributeFilter: ['title'] })
    clearInterval(watch)
    // Its element gone (a list re-rendered under the pointer): no tooltip left behind, the titles
    // put back, even once the tooltip is hidden by a click (which may be what removed it).
    watch = setInterval(() => {
      if (!current?.isConnected) leave()
    }, 250)
    text = textOf(el)
    if (!text) return
    clearTimeout(timer)
    timer = setTimeout(show, Date.now() - hiddenAt < WARM_MS ? 0 : delay())
  }

  function leave() {
    clearTimeout(timer)
    clearInterval(watch)
    observer?.disconnect()
    observer = null
    if (current && saved !== null && !current.hasAttribute('title')) current.setAttribute('title', saved)
    for (const [n, title] of above) if (!n.hasAttribute('title')) n.setAttribute('title', title)
    above = new Map()
    current = null
    saved = null
    hide()
  }

  async function show() {
    if (!current?.isConnected || !text) return
    shown = true
    await place()
  }

  function hide() {
    if (shown) hiddenAt = Date.now()
    shown = false
  }

  /**
   * Under the pointer, from its tip, kept in the window, above it when it does not fit below; with
   * `data-tip-side`, against the element (centred), on the other side when it does not fit.
   */
  async function place() {
    await tick()
    if (!current || !box) return
    const w = box.offsetWidth
    const h = box.offsetHeight
    const vw = window.innerWidth
    const vh = window.innerHeight
    const clampX = (x: number) => Math.min(Math.max(MARGIN, x), vw - MARGIN - w)
    const clampY = (y: number) => Math.min(Math.max(MARGIN, y), vh - MARGIN - h)
    const asked = current.dataset.tipSide as Side | undefined
    if (!asked) {
      const below = pointer.y + BELOW_POINTER + h <= vh - MARGIN
      pos = { x: clampX(pointer.x), y: below ? pointer.y + BELOW_POINTER : clampY(pointer.y - ABOVE_POINTER - h), side: below ? 'below' : 'above' }
      return
    }
    const r = current.getBoundingClientRect()
    let side = asked
    if (side === 'below' && r.bottom + GAP + h > vh - MARGIN && r.top - GAP - h >= MARGIN) side = 'above'
    else if (side === 'above' && r.top - GAP - h < MARGIN) side = 'below'
    else if (side === 'right' && r.right + GAP + w > vw - MARGIN) side = 'left'
    else if (side === 'left' && r.left - GAP - w < MARGIN) side = 'right'
    pos =
      side === 'below' ? { x: clampX(r.left + r.width / 2 - w / 2), y: r.bottom + GAP, side }
      : side === 'above' ? { x: clampX(r.left + r.width / 2 - w / 2), y: r.top - GAP - h, side }
      : side === 'right' ? { x: r.right + GAP, y: clampY(r.top + r.height / 2 - h / 2), side }
      : { x: r.left - GAP - w, y: clampY(r.top + r.height / 2 - h / 2), side }
  }

  onMount(() => {
    const move = (e: PointerEvent) => {
      pointer = { x: e.clientX, y: e.clientY }
    }
    const over = (e: PointerEvent) => {
      pointer = { x: e.clientX, y: e.clientY }
      if (!enabled() || e.pointerType === 'touch') return
      const el = owner(e.target)
      if (el === current) return
      if (current) leave()
      if (el) enter(el)
    }
    const out = (e: PointerEvent) => {
      if (current && !(e.relatedTarget instanceof Node && current.contains(e.relatedTarget))) leave()
    }
    // A click, a key, a scroll: the tooltip goes (its title stays aside until the pointer leaves).
    const away = () => {
      clearTimeout(timer)
      hide()
    }
    document.addEventListener('pointermove', move, { passive: true })
    document.addEventListener('pointerover', over)
    document.addEventListener('pointerout', out)
    document.addEventListener('pointerdown', away, true)
    document.addEventListener('keydown', away, true)
    document.addEventListener('wheel', away, { capture: true, passive: true })
    window.addEventListener('blur', leave)
    return () => {
      leave()
      document.removeEventListener('pointermove', move)
      document.removeEventListener('pointerover', over)
      document.removeEventListener('pointerout', out)
      document.removeEventListener('pointerdown', away, true)
      document.removeEventListener('keydown', away, true)
      document.removeEventListener('wheel', away, true)
      window.removeEventListener('blur', leave)
    }
  })
  // Turned off while one is set aside: the title goes back at once.
  $effect(() => {
    if (!enabled()) leave()
  })
</script>

{#if shown}
  <div bind:this={box} class="tip {pos.side}" role="tooltip" style:left="{pos.x}px" style:top="{pos.y}px">
    {#if parts.body}<span class="body">{parts.body}</span>{/if}
    {#if parts.keys.length}<span class="keys">{#each parts.keys as k, i (i)}{#if i}<i>or</i>{/if}<kbd>{k}</kbd>{/each}</span>{/if}
  </div>
{/if}

<style>
  .tip { position: fixed; z-index: 1000; display: flex; align-items: baseline; gap: 8px; max-width: min(340px, calc(100vw - 12px)); padding: 4px 8px; border-radius: var(--r6); background: color-mix(in srgb, var(--text) 90%, var(--bg)); color: var(--bg); box-shadow: var(--shadow-pop); font-size: var(--fs-meta); line-height: 1.4; pointer-events: none; animation: tip-in 0.09s ease-out; }
  .body { white-space: pre-line; overflow-wrap: anywhere; }
  .keys { display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; }
  .keys i { font-style: normal; opacity: 0.6; }
  kbd { padding: 0 4px; border-radius: var(--r3); background: color-mix(in srgb, var(--bg) 18%, transparent); font: 600 10.5px var(--mono); line-height: 15px; }
  @keyframes tip-in { from { opacity: 0; transform: translateY(var(--tip-from, 2px)); } }
  .tip.above { --tip-from: -2px; }
  .tip.right, .tip.left { --tip-from: 0; }
  :global(html[data-animations='off']) .tip { animation: none; }
</style>
