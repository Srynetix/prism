<script lang="ts">
  import { tick, untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import type { IconName } from '../icons'
  import { KIND_ONE, askFromPalette, choose, closePalette, found, lookUp, palette, paletteKey, runAction, search, showActions, type Kind, type Row } from '../palette.svelte'
  import { keys } from '../platform'
  import { matches } from '../shortcut'
  import { t, tn, tc } from '../i18n.svelte'

  /**
   * The command palette (palette.svelte.ts): its field, the lines found by kind, and an entry's
   * actions (⌘↵). ↑ ↓ move, ↵ goes or runs (a command waiting for its words gets them typed), ⇥
   * completes a command's name, Escape closes the actions then the palette. `phone`: the remote's,
   * the whole screen.
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
    if (e.key === 'Escape') {
      e.preventDefault()
      if (acts) palette.actions = null
      else closePalette()
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
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
    // The window's shortcuts do not see what is typed here.
    e.stopPropagation()
  }
</script>

{#if palette.open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions : Escape closes from the keyboard -->
  <div class="scrim" class:phone onclick={(e) => e.target === e.currentTarget && closePalette()}>
    <div class="pal" role="dialog" aria-modal="true" aria-label={t('Command palette')}>
      <div class="field">
        <Icon name="search" />
        <input bind:this={input} value={palette.query} oninput={(e) => { palette.query = e.currentTarget.value; palette.selected = 0; palette.actions = null }} {onkeydown}
          placeholder={phone ? t('Go to, or do…') : t('Go to anything, or do it…')} autocomplete="off" spellcheck="false" aria-label={t('Search')} role="combobox" aria-expanded="true" aria-controls="palette-list" />
        {#if current?.words}<span class="scope">{current.entry.title}</span>{/if}
        {#if phone}<button class="x" onclick={closePalette} aria-label={t('Close')}><Icon name="close" /></button>{:else}<span class="kbd">esc</span>{/if}
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
            {#if !found.busy}<p class="empty">{t('Nothing in Prism for “{query}”.', { query: palette.query.trim() })}</p>{/if}
          {/each}
          {#if found.busy}<p class="busy">{t('Searching the tracker…')}</p>{/if}
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
    </div>
  </div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; z-index: 72; display: flex; justify-content: center; align-items: flex-start; padding: 10vh 16px 16px; background: color-mix(in srgb, var(--bg) 40%, transparent); backdrop-filter: blur(2px); }
  .scrim.phone { padding: 0; align-items: stretch; }
  .pal { width: min(640px, 100%); max-height: 70vh; display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--r12); background: var(--surface); box-shadow: var(--shadow-modal); overflow: hidden; }
  .phone .pal { max-height: none; border-radius: 0; }
  .field { display: flex; align-items: center; gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--border); color: var(--text-muted); }
  .field input { flex: 1; min-width: 0; border: 0; outline: none; background: transparent; color: var(--text); font: inherit; font-size: 15px; }
  .scope { padding: 1px 8px; border-radius: 9px; background: var(--accent); color: var(--on-accent); font-size: var(--fs-tag); }
  .x { display: inline-flex; padding: 4px; border: none; background: none; color: var(--text-muted); }
  .list { flex: 1; min-height: 0; overflow: auto; overscroll-behavior: contain; padding: 4px 6px 6px; }
  .grp { padding: 8px 8px 3px; color: var(--text-muted); font-size: var(--fs-tag); letter-spacing: 0.04em; text-transform: uppercase; }
  .grp.sub { padding-top: 4px; text-transform: none; letter-spacing: 0; }
  .it { display: flex; align-items: center; gap: 10px; padding: 6px 8px; border-radius: var(--r7); cursor: pointer; }
  .it.on { background: color-mix(in srgb, var(--accent) 12%, var(--surface)); }
  .g { display: grid; flex-shrink: 0; place-items: center; width: 22px; height: 22px; border-radius: var(--r6); background: var(--surface-muted); color: var(--text-muted); }
  .t { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t small { margin-left: 8px; color: var(--text-muted); font-size: var(--fs-meta); }
  .t small.usage { margin-left: 5px; font-family: var(--mono); }
  .t mark { background: none; color: var(--accent); font-weight: 600; }
  .r { display: flex; flex-shrink: 0; align-items: center; gap: 6px; }
  .pill { padding: 1px 7px; border-radius: 9px; background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .pill.warn { background: var(--warn-bg); color: var(--warn); }
  .pill.info { background: var(--info-bg); color: var(--info); }
  .pill.ko { background: var(--ko-bg); color: var(--ko); }
  .pill.ok { background: var(--ok-bg); color: var(--ok); }
  .kbd { padding: 0 5px; border: 1px solid var(--border); border-radius: var(--r4); color: var(--text-muted); font-family: var(--mono); font-size: 10px; white-space: nowrap; }
  .empty { margin: 0; padding: 16px 10px; color: var(--text-muted); }
  .busy { margin: 0; padding: 8px 10px; color: var(--text-muted); font-size: var(--fs-meta); }
  .foot { display: flex; align-items: center; gap: 14px; padding: 7px 14px; border-top: 1px solid var(--border); color: var(--text-muted); font-size: var(--fs-tag); }
  .foot b { color: var(--text); font-weight: 500; }
  .grow { flex: 1; }
  .btn { padding: 6px 12px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; }
</style>
