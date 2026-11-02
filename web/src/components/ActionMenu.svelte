<script lang="ts" module>
  import type { IconName } from '../icons'
  import { t } from '../i18n.svelte'

  export interface MenuItem {
    key: string
    label: string
    hint?: string
    /** Icon at the start of the row. */
    icon?: IconName
    /** Key that triggers the action while the menu is open. */
    kbd?: string
    /** Second click required, with this label. */
    confirm?: string
    run?: () => Promise<unknown> | unknown
    href?: string
    external?: boolean
  }
  /** Pill (status) or selector box (priority); `current`: the current value. */
  export interface MenuChoice {
    key: string
    label: string
    color?: string
    current?: boolean
    run: () => Promise<unknown> | unknown
  }
  export interface MenuSection {
    title: string
    items?: MenuItem[]
    /** Choices as pills that wrap (statuses). */
    chips?: MenuChoice[]
    /** Choices on a single line (priority). */
    segment?: MenuChoice[]
  }
  /** What the menu shows: identifier, title, state, in the source's color. */
  export interface MenuHead {
    id: string
    title: string
    color: string
    pills?: { label: string; tone?: string }[]
  }
  /** Quick-bar button, always at the top (copy, open, filter). */
  export interface QuickAction {
    key: string
    label: string
    icon: IconName
    kbd?: string
    /** Label shown briefly after the action ("Copied"). */
    done?: string
    run?: () => Promise<unknown> | unknown
    href?: string
    external?: boolean
  }
</script>

<script lang="ts">
  import { anchored, exclusive } from '../popup'
  import { actorName } from '../agent-cli'
  import { openLink } from '../links'
  import Icon from './Icon.svelte'
  import ActionSheet from './ActionSheet.svelte'
  import type { FlatAction } from '../menus'

  /**
   * ⋯ menu of a row (MR, ticket, sprint): at the top what it targets (`head`), a bar of quick
   * actions (`quick`), then the actions by theme, spelled out, or as pills (statuses) and
   * as a selector (priority). Those that cannot be undone ask for a second click (`confirm`).
   * Keyboard: arrows to move, the displayed key to run, Escape to close.
   * No fixed max height: the menu only scrolls as a last resort (very short window),
   * with the header and quick bar staying visible. Fixed position, attached to `<body>`.
   */
  let { sections, label, head, quick = [] }: { sections: MenuSection[]; label: string; head?: MenuHead; quick?: QuickAction[] } = $props()

  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()
  let menu: HTMLDivElement | undefined = $state()
  let armed = $state<string | null>(null)
  let busy = $state<string | null>(null)
  let error = $state<string | null>(null)
  let flashed = $state<string | null>(null)

  const shown = $derived(sections.filter((s) => s.items?.length || s.chips?.length || s.segment?.length))

  /**
   * On the phone (the remote): a sheet from the bottom, by group, rather than a popup by the
   * button — taller than the screen, a popup scrolled hard and closed when the address bar moved.
   * As `flatten` (menus.ts) makes them, without its imports.
   */
  const asSheet = document.documentElement.classList.contains('remote')
  const sheetActions = $derived.by((): FlatAction[] => {
    const out: FlatAction[] = quick.map((q) => ({ key: q.key, label: q.href ? t('Open in {place}', { place: q.label }) : q.label, run: q.run, href: q.href }))
    for (const s of sections) {
      for (const i of s.items ?? []) out.push({ key: i.key, label: i.label, hint: i.hint, group: s.title, confirm: i.confirm, run: i.run, href: i.href })
      for (const c of [...(s.chips ?? []), ...(s.segment ?? [])]) if (!c.current) out.push({ key: c.key, label: `${s.title}: ${c.label}`, group: s.title, run: c.run })
    }
    // Filtering the dashboard means nothing on the phone.
    return out.filter((a) => a.key !== 'filter')
  })

  /**
   * The menu is attached to `<body>`: a faded row (merged MR, finished ticket) has its
   * own stacking context, where the menu would slip under the following rows.
   */
  function toBody(node: HTMLElement) {
    document.body.appendChild(node)
    // The first item takes focus: the arrows start from it.
    requestAnimationFrame(() => node.querySelector<HTMLElement>('[data-nav]')?.focus({ preventScroll: true }))
    return () => node.remove()
  }

  function toggle(e: MouseEvent) {
    e.stopPropagation()
    open = !open
    armed = null
    error = null
    flashed = null
  }

  async function exec(key: string, run: () => Promise<unknown> | unknown, opts: { confirm?: string; done?: string } = {}) {
    if (busy) return
    if (opts.confirm && armed !== key) {
      armed = key
      return
    }
    armed = null
    busy = key
    error = null
    try {
      await run()
      if (opts.done) {
        // The button says "Copied" for a moment, then the menu closes.
        flashed = key
        setTimeout(() => (open = false), 450)
      } else open = false
    } catch (err) {
      error = String(err)
    } finally {
      busy = null
    }
  }

  function pick(e: MouseEvent, item: MenuItem) {
    e.stopPropagation()
    if (item.run) exec(item.key, item.run, { confirm: item.confirm })
  }

  const typing = (e: KeyboardEvent) => !!(e.target as HTMLElement | null)?.closest?.('input, textarea, select, [contenteditable]')
  function onKey(e: KeyboardEvent) {
    if (!open) return
    if (e.key === 'Escape') {
      e.stopImmediatePropagation()
      open = false
      return
    }
    if (typing(e) || e.metaKey || e.ctrlKey || e.altKey) return
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      e.stopImmediatePropagation()
      const all = [...(menu?.querySelectorAll<HTMLElement>('[data-nav]:not(:disabled)') ?? [])]
      const i = all.indexOf(document.activeElement as HTMLElement)
      all[(i + (e.key === 'ArrowDown' ? 1 : -1) + all.length) % all.length]?.focus()
      return
    }
    const key = e.key.toLowerCase()
    const q = quick.find((x) => x.kbd?.toLowerCase() === key)
    const item = shown.flatMap((s) => s.items ?? []).find((x) => x.kbd?.toLowerCase() === key)
    const hit = q ?? item
    if (!hit) return
    e.preventDefault()
    e.stopImmediatePropagation()
    if (hit.href) {
      if (/^(https?|claude):/i.test(hit.href)) openLink(hit.href)
      else window.open(hit.href, '_self')
      open = false
    } else if (hit.run) exec(hit.key, hit.run, { confirm: 'confirm' in hit ? hit.confirm : undefined, done: 'done' in hit ? hit.done : undefined })
  }
</script>

<svelte:window onclick={() => (open = false)} onkeydowncapture={onKey} onresize={() => !asSheet && (open = false)} />

<button bind:this={button} class="more" class:open onclick={toggle} title={t('All actions')} aria-label={label} aria-expanded={open}><Icon name="more" /></button>
{#if open && asSheet}
  <!-- Its taps stay its own: the window's click would close it before a second tap confirms. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div onclick={(e) => e.stopPropagation()}><ActionSheet title={head ? `${head.id} ${head.title}` : label} actions={sheetActions} onclose={() => (open = false)} /></div>
{:else if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <div bind:this={menu} class="menu" class:wide={!!head} role="menu" tabindex="-1" style:--src={head?.color} onclick={(e) => e.stopPropagation()} {@attach toBody} {@attach anchored(() => button)} {@attach exclusive(() => (open = false))}>
    {#if head}
      <div class="hd">
        <span class="id"><b>{head.id}</b>{#each head.pills ?? [] as p (p.label)}<span class="pill {p.tone ?? ''}">{p.label}</span>{/each}</span>
        <span class="t" title={head.title}>{head.title}</span>
      </div>
    {/if}
    {#if quick.length}
      <div class="qb" style:grid-template-columns="repeat({quick.length}, 1fr)">
        {#each quick as q (q.key)}
          {#if q.href}
            <a data-nav role="menuitem" href={q.href} target={q.external ? '_blank' : undefined} rel="noreferrer" title={q.kbd ? `${q.label} (${q.kbd})` : q.label} onclick={() => (open = false)}><i><Icon name={q.icon} /></i>{q.label}</a>
          {:else}
            <button data-nav role="menuitem" class:done={flashed === q.key} disabled={!!busy} title={q.kbd ? `${q.label} (${q.kbd})` : q.label} onclick={(e) => { e.stopPropagation(); if (q.run) exec(q.key, q.run, { done: q.done }) }}><i><Icon name={flashed === q.key ? 'check' : q.icon} /></i>{flashed === q.key ? q.done : q.label}</button>
          {/if}
        {/each}
      </div>
    {/if}
    <div class="body">
      {#each shown as s (s.title)}
        <p class:claude={s.title === actorName()}>{s.title}</p>
        {#if s.chips?.length}
          <div class="chips">
            {#each s.chips as c (c.key)}
              <button data-nav role="menuitemradio" aria-checked={!!c.current} class="chip" class:cur={c.current} style:--c={c.color ?? 'var(--text-muted)'} disabled={!!busy || c.current} onclick={(e) => { e.stopPropagation(); exec(c.key, c.run) }}><i></i>{busy === c.key ? `${c.label}…` : c.label}</button>
            {/each}
          </div>
        {/if}
        {#if s.segment?.length}
          <div class="seg" role="radiogroup" aria-label={s.title}>
            {#each s.segment as c (c.key)}
              <button data-nav role="radio" aria-checked={!!c.current} class:on={c.current} style:--c={c.color} disabled={!!busy || c.current} onclick={(e) => { e.stopPropagation(); exec(c.key, c.run) }} title={c.label}>{#if c.color}<b></b>{/if}{c.label}</button>
            {/each}
          </div>
        {/if}
        {#each s.items ?? [] as item (item.key)}
          {#if item.href}
            <!-- Links: opened by the document handler (browser, or the Claude app). -->
            <a data-nav role="menuitem" class="it" href={item.href} target={item.external ? '_blank' : undefined} rel="noreferrer" onclick={() => (open = false)}>{#if item.icon}<span class="ic"><Icon name={item.icon} /></span>{/if}{item.label}{#if item.hint}<small>{item.hint}</small>{/if}{#if item.kbd}<kbd>{item.kbd}</kbd>{/if}</a>
          {:else}
            <button data-nav role="menuitem" class="it" class:armed={armed === item.key} disabled={!!busy} onclick={(e) => pick(e, item)}>
              {#if item.icon}<span class="ic"><Icon name={item.icon} /></span>{/if}{armed === item.key ? item.confirm : busy === item.key ? `${item.label}…` : item.label}{#if item.hint && armed !== item.key}<small>{item.hint}</small>{/if}{#if item.kbd && armed !== item.key}<kbd>{item.kbd}</kbd>{/if}
            </button>
          {/if}
        {/each}
      {/each}
      {#if error}<p class="error">{error}</p>{/if}
    </div>
  </div>
{/if}

<style>
  .more { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 20px; height: 20px; padding: 0; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: 14px; line-height: 1; cursor: pointer; }
  .more:hover, .more.open { border-color: var(--border); color: var(--text); background: var(--surface); }
  .menu { position: fixed; z-index: 45; width: 260px; max-height: calc(100vh - 16px); display: flex; flex-direction: column; overflow: hidden; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r10); box-shadow: var(--shadow-pop); text-align: left; font-size: var(--fs-body); }
  .menu.wide { width: 300px; }
  .menu:focus { outline: none; }
  /* Header: the menu's subject, in its source's color. */
  .hd { display: flex; flex-direction: column; gap: 2px; flex-shrink: 0; padding: 9px 11px 8px; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--src) 7%, var(--surface)); box-shadow: inset 0 2px var(--src); }
  .id { display: flex; align-items: center; gap: 6px; font-family: var(--mono); font-size: var(--fs-meta); color: var(--text-muted); }
  .id b { color: var(--src); font-weight: 600; }
  .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; font-size: var(--fs-title); color: var(--text); }
  .pill { padding: 0 6px; border-radius: var(--r9); background: var(--surface-muted); color: var(--text-muted); font-family: var(--sans); font-size: var(--fs-tag); white-space: nowrap; }
  .pill.ok { background: var(--ok-bg); color: var(--ok); }
  .pill.ko { background: var(--ko-bg); color: var(--ko); }
  .pill.warn { background: var(--warn-bg); color: var(--warn); }
  .pill.info { background: var(--info-bg); color: var(--info); }
  /* Quick bar: copy, open, filter, in one click, always at the top. */
  .qb { display: grid; gap: 4px; flex-shrink: 0; padding: 6px; border-bottom: 1px solid var(--border); }
  .qb a, .qb button { display: flex; flex-direction: column; align-items: center; gap: 2px; min-width: 0; padding: 6px 2px 5px; border: none; border-radius: var(--r7); background: none; color: var(--text); font: inherit; font-size: var(--fs-tag); line-height: 1.2; text-align: center; text-decoration: none; cursor: pointer; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .qb i { font-style: normal; font-size: 14px; color: var(--text-muted); }
  .qb a:hover, .qb button:hover:not(:disabled), .qb a:focus-visible, .qb button:focus-visible { background: var(--surface-muted); outline: none; }
  .qb button.done { background: var(--ok-bg); color: var(--ok); }
  .qb button.done i { color: var(--ok); }
  .body { overflow: auto; padding: 4px; }
  .body p { margin: 6px 8px 2px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .body p.claude { color: var(--src-claude); }
  .it { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 8px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; text-decoration: none; cursor: pointer; white-space: nowrap; }
  .it:hover:not(:disabled), .it:focus-visible { background: var(--surface-muted); outline: none; }
  .it:disabled { cursor: progress; opacity: 0.6; }
  .it.armed { background: var(--warn-bg); color: var(--warn); white-space: normal; }
  .ic { display: inline-flex; justify-content: center; width: 16px; flex-shrink: 0; color: var(--text-muted); }
  .it small { margin-left: auto; max-width: 120px; overflow: hidden; text-overflow: ellipsis; color: var(--text-muted); font-size: var(--fs-tag); }
  kbd { margin-left: auto; padding: 0 4px; border: 1px solid var(--border); border-bottom-width: 2px; border-radius: var(--r4); color: var(--text-muted); font: 10px var(--mono); }
  .it small + kbd { margin-left: 6px; }
  /* Statuses as pills; the current value highlighted. */
  .chips { display: flex; flex-wrap: wrap; gap: 4px; padding: 2px 8px 6px; }
  .chip { display: inline-flex; align-items: center; gap: 5px; padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); white-space: nowrap; cursor: pointer; }
  .chip i { width: 7px; height: 7px; border-radius: 50%; background: var(--c); }
  .chip:hover:not(:disabled), .chip:focus-visible { border-color: var(--c); outline: none; }
  .chip.cur { background: color-mix(in srgb, var(--c) 16%, var(--surface)); border-color: var(--c); font-weight: 600; cursor: default; }
  .chip:disabled:not(.cur) { cursor: progress; opacity: 0.6; }
  /* Single-line selector (priority). */
  .seg { display: flex; gap: 2px; margin: 2px 8px 6px; padding: 2px; border-radius: var(--r7); background: var(--surface-muted); }
  .seg button { flex: 1; display: flex; align-items: center; justify-content: center; gap: 4px; min-width: 0; padding: 2px 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); white-space: nowrap; cursor: pointer; }
  .seg button:hover:not(:disabled), .seg button:focus-visible { color: var(--text); outline: none; background: color-mix(in srgb, var(--surface) 60%, transparent); }
  .seg button.on { background: var(--surface); color: var(--text); font-weight: 600; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08); cursor: default; }
  .seg b { width: 7px; height: 7px; border-radius: var(--r2); background: var(--c); }
  .error { margin: 4px 8px; color: var(--ko) !important; text-transform: none !important; letter-spacing: 0 !important; white-space: pre-wrap; }
</style>
