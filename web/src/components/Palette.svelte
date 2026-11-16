<script lang="ts">
  import { tick, untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import type { IconName } from '../icons'
  import { KIND_ONE, askFromPalette, choose, closePalette, found, lookUp, palette, paletteKey, runAction, search, showActions, type Kind, type Row } from '../palette.svelte'
  import { keys } from '../platform'
  import { matches } from '../shortcut'
  import { t, tn, tc } from '../i18n.svelte'
  import Modal from './ui/Modal.svelte'
  import State from './ui/State.svelte'

  /**
   * The command palette (palette.svelte.ts): its field, the lines found by kind, and an entry's
   * actions (⌘↵). ↑ ↓ move, ↵ goes or runs (a command waiting for its words gets them typed), ⇥
   * completes a command's name, Escape (the stack of layers, through `Modal`) closes the actions then
   * the palette. `phone`: the remote's.
   */
  let { phone = false }: { phone?: boolean } = $props()

  const ICON: Record<Kind, IconName> = { ask: 'sparkles', agent: 'claude', mr: 'mr', ticket: 'kanban', file: 'files', page: 'web', block: 'blockNormal', pipeline: 'running', env: 'helm', pod: 'kube', shell: 'terminal', closed: 'undo', command: 'terminal', setting: 'settings', help: 'help' }

  let input: HTMLInputElement | undefined = $state()
  let list: HTMLDivElement | undefined = $state()
  const groups = $derived(palette.open ? search(palette.query) : [])
  const rows = $derived(groups.flatMap((g) => g.rows))
  const current = $derived<Row | undefined>(rows[Math.min(palette.selected, rows.length - 1)])

  $effect(() => {
    if (!palette.open) return
    void tick().then(() => input?.focus())
  })
  // The tickets Prism does not show, asked of the tracker once the typing pauses.
  $effect(() => {
    const query = palette.open ? palette.query : ''
    untrack(() => lookUp(query))
  })
  // The chosen line in sight.
  $effect(() => {
    void palette.selected, palette.actions?.selected
    void tick().then(() => list?.querySelector('.on')?.scrollIntoView({ block: 'nearest' }))
  })

  function marked(title: string, at: number[]) {
    const set = new Set(at)
    return [...title].map((c, i) => ({ c, on: set.has(i) }))
  }

  function onkeydown(e: KeyboardEvent) {
    const acts = palette.actions
    // Its shortcut closes it too.
    if (matches(e, paletteKey())) {
      e.preventDefault()
      e.stopPropagation()
      return closePalette()
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const step = e.key === 'ArrowDown' ? 1 : -1
      if (acts) {
        acts.selected = (acts.selected + step + acts.list.length) % acts.list.length
        acts.armed = null
      } else if (rows.length) palette.selected = (Math.min(palette.selected, rows.length - 1) + step + rows.length) % rows.length
    } else if (e.key === 'Enter') {
      e.preventDefault()
      if (e.altKey && !acts && askFromPalette()) return
      if (acts) void runAction(acts.list[acts.selected])
      else if (current && (e.metaKey || e.ctrlKey)) showActions(current)
      else if (current) void choose(current)
    } else if (e.key === 'Tab') {
      e.preventDefault()
      if (current?.entry.kind === 'command' && !current.words) palette.query = `${current.entry.title} `
    }
    // The window's shortcuts do not see what is typed here; Escape goes on to the layers, which close it.
    if (e.key !== 'Escape') e.stopPropagation()
  }
</script>

<!-- Escape or a click on the veil: back from an entry's actions first, then closed. -->
<Modal open={palette.open} key="palette" label={t('Command palette')} width={620} class="pal" onclose={() => (palette.actions ? (palette.actions = null) : closePalette())}>
      <div class="field">
        <Icon name="search" />
        <input bind:this={input} value={palette.query} oninput={(e) => { palette.query = e.currentTarget.value; palette.selected = 0; palette.actions = null }} {onkeydown}
          placeholder={phone ? t('Go to, or do…') : t('Go to anything, or do it…')} autocomplete="off" spellcheck="false" aria-label={t('Search')} role="combobox" aria-expanded="true" aria-controls="palette-list" />
        {#if current?.words}<span class="scope">{current.entry.title}</span>{/if}
        {#if phone}<button class="btn sm icon ghost" onclick={closePalette} aria-label={t('Close')}><Icon name="close" /></button>{:else}<span class="kbd">esc</span>{/if}
      </div>

      <div class="list" id="palette-list" role="listbox" bind:this={list}>
        {#if palette.actions}
          {@const acts = palette.actions}
          <div class="grp">{KIND_ONE[acts.entry.kind]} · {acts.entry.title}</div>
          {#each acts.list as a, i (a.key + i)}
            {#if a.group && a.group !== acts.list[i - 1]?.group}<div class="grp sub">{a.group}</div>{/if}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div class="it" class:on={i === acts.selected} role="option" tabindex="-1" aria-selected={i === acts.selected} onmousemove={() => (acts.selected = i)} onclick={() => runAction(a)}>
              <span class="t">{acts.armed === a.key ? a.confirm : a.label}{#if a.hint && acts.armed !== a.key}<small>{a.hint}</small>{/if}</span>
              {#if a.href}<Icon name="external" />{/if}
            </div>
          {/each}
        {:else}
          {#each groups as g (g.title)}
            <div class="grp">{g.title}</div>
            {#each g.rows as r (r.entry.id + (r.words ?? ''))}
              {@const i = rows.indexOf(r)}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div class="it" class:on={r === current} role="option" tabindex="-1" aria-selected={r === current} onmousemove={() => (palette.selected = i)} onclick={() => choose(r)}>
                <span class="g"><Icon name={ICON[r.entry.kind]} /></span>
                <span class="t">
                  {#if r.words}{r.entry.title} <b>{r.words}</b>{:else}{#each marked(r.entry.title, r.at) as ch, j (j)}{#if ch.on}<mark>{ch.c}</mark>{:else}{ch.c}{/if}{/each}{#if r.entry.usage}<small class="usage">{r.entry.usage}</small>{/if}{/if}
                  {#if r.entry.hint}<small>{r.entry.hint}</small>{/if}
                </span>
                <span class="r">
                  {#if r.entry.pill?.label}<span class="pill {r.entry.pill.tone}">{r.entry.pill.label}</span>{/if}
                  {#if r.entry.keys && !phone}<span class="kbd">{r.entry.keys}</span>{/if}
                  {#if r.entry.actions && r === current && !phone}<span class="kbd" title={t('Its actions')}>{keys('⌘↵')}</span>{/if}
                </span>
              </div>
            {/each}
          {:else}
            {#if !found.busy}<State kind="empty" icon="search" title={t('Nothing in Prism for “{query}”.', { query: palette.query.trim() })} />{/if}
          {/each}
          {#if found.busy}<State kind="loading" inline title={t('Searching the tracker…')} />{/if}
        {/if}
      </div>

      {#if !phone}
        <div class="foot">
          {#if palette.actions}
            <span><b>↵</b> {t('run')}</span><span><b>esc</b> {t('back')}</span>
          {:else}
            <span><b>↵</b> {current?.words ? t('run') : current?.entry.kind === 'command' && current.entry.required ? t('type its words') : current?.entry.kind === 'command' ? t('run') : tc('shell', 'open')}</span>
            <span><b>{keys('⌘↵')}</b> {t('actions')}</span><span><b>⇥</b> {t('complete')}</span>
          {/if}
          <span class="grow"></span>
          <span>{tn(rows.length, '{n} result', '{n} results')}</span>
        </div>
      {:else if current?.entry.actions && !palette.actions}
        <div class="foot"><button class="btn" onclick={() => current && showActions(current)}>{t('Actions on the chosen line')}</button></div>
      {/if}
</Modal>

<style>
  :global(.modal.pal) { max-height: 70vh; }
  :global(html.remote .modal.pal) { max-height: 90vh; }
  .field { display: flex; align-items: center; gap: var(--sp-4); padding: var(--sp-4) var(--sp-5); border-bottom: 1px solid var(--border); color: var(--text-muted); }
  .field input { flex: 1; min-width: 0; border: 0; outline: none; background: transparent; color: var(--text); font: inherit; font-size: var(--fs-head); }
  .scope { padding: 1px 8px; border-radius: var(--r9); background: var(--accent); color: var(--on-accent); font-size: var(--fs-tag); }
  .list { flex: 1; min-height: 0; overflow: auto; overscroll-behavior: contain; padding: 4px 6px 6px; }
  .grp { padding: 8px 8px 3px; color: var(--text-muted); font-size: var(--fs-tag); letter-spacing: 0.04em; text-transform: uppercase; }
  .grp.sub { padding-top: 4px; text-transform: none; letter-spacing: 0; }
  .it { display: flex; align-items: center; gap: 10px; padding: 6px 8px; border-radius: var(--r-ctl); cursor: pointer; }
  .it.on { background: color-mix(in srgb, var(--accent) 12%, var(--surface)); }
  .g { display: grid; flex-shrink: 0; place-items: center; width: 22px; height: 22px; border-radius: var(--r6); background: var(--surface-muted); color: var(--text-muted); }
  .t { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t small { margin-left: 8px; color: var(--text-muted); font-size: var(--fs-meta); }
  .t small.usage { margin-left: 5px; font-family: var(--mono); }
  .t mark { background: none; color: var(--accent); font-weight: 600; }
  .r { display: flex; flex-shrink: 0; align-items: center; gap: 6px; }
  .kbd { padding: 0 5px; border: 1px solid var(--border); border-radius: var(--r4); color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); white-space: nowrap; }
  .foot { display: flex; align-items: center; gap: 14px; padding: 7px 14px; border-top: 1px solid var(--border); color: var(--text-muted); font-size: var(--fs-tag); }
  .foot b { color: var(--text); font-weight: 500; }
  .grow { flex: 1; }
</style>
