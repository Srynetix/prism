<script lang="ts">
  import { anchored, dismissable } from '../popup'

  /**
   * A select's list of options, the app's in place of the system's menu (Aqua on the Mac, GTK on
   * Linux): for every select of the window (one choice, not a list box), opened by a click on it,
   * or from the keyboard by Space, Enter, ↑ or ↓. ↑ ↓ Home End move, a letter goes to the next
   * option starting with it, Enter or Space picks, Esc or Tab closes; the select keeps the focus.
   * A pick sets the select and sends it `input` then `change`, as the system's menu did, so
   * `bind:value` and `onchange` see it. The phone keeps its own pickers. Not exclusive: a popup
   * the select is in stays open.
   */
  interface Item {
    index: number
    label: string
    disabled: boolean
    /** The first option of an `<optgroup>`: its label, shown above it. */
    group?: string
  }
  let open = $state<{ select: HTMLSelectElement; items: Item[]; cursor: number } | null>(null)
  let list: HTMLDivElement | undefined = $state()
  const close = () => (open = null)

  const eligible = (el: EventTarget | null): el is HTMLSelectElement =>
    el instanceof HTMLSelectElement && !el.multiple && el.size <= 1 && !el.disabled && !document.documentElement.classList.contains('remote')

  function items(select: HTMLSelectElement): Item[] {
    let group: HTMLOptGroupElement | null = null
    return [...select.options]
      .filter((o) => !o.hidden)
      .map((o) => {
        const parent = o.parentElement instanceof HTMLOptGroupElement ? o.parentElement : null
        const first = parent && parent !== group
        group = parent
        return { index: o.index, label: o.label || o.text, disabled: o.disabled || !!parent?.disabled, group: first ? parent.label : undefined }
      })
  }
  function show(select: HTMLSelectElement) {
    const all = items(select)
    if (!all.length) return
    open = { select, items: all, cursor: Math.max(0, all.findIndex((i) => i.index === select.selectedIndex)) }
  }
  function pick(item: Item) {
    if (!open || item.disabled) return
    const select = open.select
    close()
    if (select.selectedIndex === item.index) return
    select.selectedIndex = item.index
    select.dispatchEvent(new Event('input', { bubbles: true }))
    select.dispatchEvent(new Event('change', { bubbles: true }))
  }
  /** The next option that can be picked from `from`, `step` by step (none past either end). */
  function next(from: number, step: number): number {
    if (!open) return from
    for (let i = from + step; i >= 0 && i < open.items.length; i += step) if (!open.items[i].disabled) return i
    return from
  }

  function mousedown(e: MouseEvent) {
    if (e.button !== 0 || !eligible(e.target)) return
    e.preventDefault()
    const select = e.target
    select.focus()
    if (open?.select === select) close()
    else show(select)
  }
  let typed = ''
  let typedAt = 0
  function keydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.isComposing) return
    if (!open) {
      if (eligible(e.target) && [' ', 'Enter', 'ArrowDown', 'ArrowUp'].includes(e.key)) {
        e.preventDefault()
        show(e.target)
      }
      return
    }
    const o = open
    const take = () => {
      e.preventDefault()
      e.stopImmediatePropagation()
    }
    switch (e.key) {
      case 'ArrowDown': take(); o.cursor = next(o.cursor, 1); break
      case 'ArrowUp': take(); o.cursor = next(o.cursor, -1); break
      case 'Home': take(); o.cursor = next(-1, 1); break
      case 'End': take(); o.cursor = next(o.items.length, -1); break
      case 'Enter':
      case ' ': take(); pick(o.items[o.cursor]); break
      // Closes the list only, not the panel the select is in.
      case 'Escape': take(); close(); break
      case 'Tab': close(); break
      default:
        if (e.key.length !== 1 || e.altKey) return
        take()
        // Letters typed in a row make a word; a pause starts a new one.
        typed = e.timeStamp - typedAt < 800 ? typed + e.key.toLowerCase() : e.key.toLowerCase()
        typedAt = e.timeStamp
        const found = [...o.items.keys()].map((k) => (o.cursor + (typed.length > 1 ? 0 : 1) + k) % o.items.length).find((k) => !o.items[k].disabled && o.items[k].label.toLowerCase().startsWith(typed))
        if (found !== undefined) o.cursor = found
    }
  }
  const blurred = (e: FocusEvent) => {
    if (open && e.target === open.select) close()
  }

  $effect(() => {
    document.addEventListener('mousedown', mousedown, true)
    document.addEventListener('keydown', keydown, true)
    document.addEventListener('focusout', blurred, true)
    return () => {
      document.removeEventListener('mousedown', mousedown, true)
      document.removeEventListener('keydown', keydown, true)
      document.removeEventListener('focusout', blurred, true)
    }
  })
  $effect(() => {
    if (open) list?.querySelector('.on')?.scrollIntoView({ block: 'nearest' })
  })
</script>

{#if open}
  {@const o = open}
  <!-- The select keeps its focus while a choice is clicked. -->
  <div
    bind:this={list}
    class="selectmenu"
    role="listbox"
    tabindex="-1"
    style:min-width="{o.select.offsetWidth}px"
    onmousedown={(e) => e.preventDefault()}
    {@attach anchored(() => o.select, { align: 'start', gap: 2 })}
    {@attach dismissable(close, { trigger: () => o.select, resize: true })}
  >
    {#each o.items as item, k (item.index)}
      {#if item.group}<p class="group">{item.group}</p>{/if}
      <div
        class="opt"
        class:on={k === o.cursor}
        class:picked={item.index === o.select.selectedIndex}
        class:off={item.disabled}
        class:inner={o.items.some((i) => i.group)}
        role="option"
        tabindex="-1"
        aria-selected={item.index === o.select.selectedIndex}
        aria-disabled={item.disabled}
        onmousemove={() => !item.disabled && (o.cursor = k)}
        onclick={() => pick(item)}
        onkeydown={() => {}}
      >{item.label}</div>
    {/each}
  </div>
{/if}

<style>
  .selectmenu { position: fixed; z-index: 200; max-height: 320px; overflow-y: auto; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); color: var(--text); font-size: var(--fs-body); outline: none; }
  .opt { position: relative; padding: 4px 10px 4px 22px; border-radius: var(--r5); white-space: nowrap; cursor: pointer; }
  .opt.on { background: color-mix(in srgb, var(--info) 18%, transparent); }
  .opt.picked::before { content: ''; position: absolute; left: 8px; top: 50%; width: 6px; height: 3px; margin-top: -3px; border-left: 1.5px solid currentColor; border-bottom: 1.5px solid currentColor; transform: rotate(-45deg); }
  .opt.off { color: var(--text-muted); opacity: 0.6; cursor: default; }
  .opt.inner { padding-left: 30px; }
  .opt.inner.picked::before { left: 14px; }
  .group { margin: 6px 10px 2px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .group:first-child { margin-top: 2px; }
</style>
