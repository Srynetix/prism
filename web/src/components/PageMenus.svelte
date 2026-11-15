<script lang="ts">
  import Icon from './Icon.svelte'
  import ViewIcon from './ViewIcon.svelte'
  import { armer } from '../armed.svelte'
  import { dismissable } from '../popup'
  import { allViews, views, hiddenViews, VIEW_COLORS, VIEW_ICONS } from '../views'
  import { editing, setView, toggleEditing } from '../layout.svelte'
  import { openWizard } from '../wizard.svelte'
  import { openSettings } from '../settings.svelte'
  import { addPage, deletePage, duplicatePage, endStyling, hidePage, movePage, pageUi, resetToPack, startRename, startStyling, templatePages } from '../pages.svelte'
  import { t, tc } from '../i18n.svelte'

  /**
   * The floating parts of managing pages from the rail (`pages.svelte.ts`): a page's menu (right-
   * click, or its ⋯ in Edit layout), its icon and color grid (shown in the rail as they are
   * picked; Esc puts them back, Enter or a click elsewhere keeps them), « + » (a new page) and
   * « Hidden pages ». One at a time, at the point they were opened from.
   */
  const arm = armer()
  const page = $derived(pageUi.menu ? allViews().find((v) => v.id === pageUi.menu!.id) : undefined)
  const at = $derived(page ? views().findIndex((v) => v.id === page.id) : -1)
  const total = $derived(allViews().length)
  const fromTemplates = $derived(templatePages())

  /** Kept inside the window, by its size once shown. */
  function within(el: HTMLElement) {
    const r = el.getBoundingClientRect()
    if (r.right > innerWidth - 8) el.style.left = `${Math.max(8, innerWidth - 8 - r.width)}px`
    if (r.bottom > innerHeight - 8) el.style.top = `${Math.max(8, innerHeight - 8 - r.height)}px`
  }
  const closeMenu = () => {
    pageUi.menu = null
    arm.disarm()
  }
  function editLayout(id: string) {
    closeMenu()
    void setView(id).then(() => {
      if (!editing.on) toggleEditing()
    })
  }
  function styleKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault()
      endStyling(true)
    }
  }
</script>

{#if pageUi.menu && page}
  <div class="pop menu" role="menu" style:left="{pageUi.menu.x}px" style:top="{pageUi.menu.y}px" {@attach within} {@attach dismissable(closeMenu)}>
    <div class="head" style:--tint={page.color}>
      <ViewIcon id={page.icon} /><b>{t(page.label)}</b>
      {#if page.origin === 'pack'}<span class="pack" title={t('A page of a pack, kept up to date with it')}>{t('pack')}</span>
      {:else if page.origin === 'copy'}<span class="pack changed" title={t('A page of a pack, changed here: it no longer follows the pack')}>{t('pack, changed')}</span>{/if}
    </div>
    <button role="menuitem" onclick={() => startRename(page.id)}><Icon name="edit" />{t('Rename')}<kbd>F2</kbd></button>
    <button role="menuitem" onclick={() => startStyling(page.id, pageUi.menu!.x, pageUi.menu!.y)}><Icon name="category" />{t('Icon and color…')}</button>
    <button role="menuitem" onclick={() => duplicatePage(page.id)}><Icon name="copy" />{t('Duplicate')}</button>
    {#if !page.hidden}
      <button role="menuitem" disabled={at <= 0} onclick={() => movePage(page.id, at - 1)}><Icon name="up" />{t('Move up')}</button>
      <button role="menuitem" disabled={at < 0 || at >= views().length - 1} onclick={() => movePage(page.id, at + 1)}><Icon name="down" />{t('Move down')}</button>
    {/if}
    <button role="menuitem" onclick={() => editLayout(page.id)}><Icon name="grip" />{t('Edit layout')}</button>
    <hr />
    {#if page.hidden}
      <button role="menuitem" onclick={() => hidePage(page.id, false)}><Icon name="show" />{t('Show in the rail')}</button>
    {:else}
      <button role="menuitem" disabled={views().length <= 1} onclick={() => hidePage(page.id)} title={t('Out of the rail, not deleted: back from Hidden pages, at its bottom')}><Icon name="hide" />{t('Hide')}</button>
    {/if}
    {#if page.origin === 'copy'}
      <button role="menuitem" onclick={() => resetToPack(page.id)} title={t('The pack’s page again, followed from now on: what was changed here goes')}><Icon name="undo" />{t('Back to the pack’s page')}</button>
    {/if}
    <button role="menuitem" class="danger" class:armed={arm.is(page.id)} disabled={total <= 1} title={total <= 1 ? t('The last page cannot be deleted') : t('Undo stays a few seconds')}
      onclick={() => arm.confirm(page.id) && deletePage(page.id)}><Icon name="trash" />{arm.is(page.id) ? t('Click again to delete') : tc('core', 'Delete')}</button>
  </div>
{/if}

{#if pageUi.styling}
  {@const s = pageUi.styling}
  <div class="pop grid" role="dialog" aria-label={t('Icon and color')} style:left="{s.x}px" style:top="{s.y}px" {@attach within} {@attach dismissable(() => endStyling(true), { escape: false })}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div onkeydown={styleKey}>
      <p class="cap">{t('Icon')}</p>
      <div class="icons">
        {#each VIEW_ICONS as id (id)}
          <button class:on={s.icon === id} style:--tint={VIEW_COLORS[s.color] ?? 'var(--accent)'} onclick={() => (s.icon = id)} aria-label={id} aria-pressed={s.icon === id} title={id}><ViewIcon {id} /></button>
        {/each}
      </div>
      <p class="cap">{t('Color')}</p>
      <div class="colors">
        {#each Object.entries(VIEW_COLORS) as [name, css] (name)}
          <button class="sw" class:on={s.color === name} style:--c={css} onclick={() => (s.color = name)} aria-label={name} aria-pressed={s.color === name} title={name}></button>
        {/each}
      </div>
      <div class="foot">
        <small>{t('Seen in the rail as you pick · Esc: as before')}</small>
        <button class="ok" onclick={() => endStyling(true)}>{tc('core', 'Done')}</button>
      </div>
    </div>
  </div>
{/if}
<!-- Before the app's keys: Esc puts the page back as it was, and does nothing else. -->
<svelte:window onkeydowncapture={(e) => { if (e.key === 'Escape' && pageUi.styling) { e.stopPropagation(); endStyling(false) } }} />

{#if pageUi.adding}
  <div class="pop menu" role="menu" style:left="{pageUi.adding.x}px" style:top="{pageUi.adding.y}px" {@attach within} {@attach dismissable(() => (pageUi.adding = null))}>
    <div class="head">{t('New page')}</div>
    <button role="menuitem" onclick={() => addPage('empty')}><Icon name="plus" />{t('Empty page')}</button>
    <button role="menuitem" onclick={() => addPage('copy')}><Icon name="copy" />{t('A copy of this page')}</button>
    {#if fromTemplates.length}
      <hr />
      <p class="cap">{t('From a template')}</p>
      <div class="scroll">
        {#each fromTemplates as x, i (i)}
          <button role="menuitem" onclick={() => addPage(x.page)}><ViewIcon id={x.page.icon ?? 'dashboard'} />{x.page.name}<small>{x.template}</small></button>
        {/each}
      </div>
    {:else}
      <button role="menuitem" onclick={() => { pageUi.adding = null; openSettings('views'); openWizard() }}><Icon name="category" />{t('From a template…')}</button>
    {/if}
  </div>
{/if}

{#if pageUi.hiddenMenu}
  <div class="pop menu" role="menu" style:left="{pageUi.hiddenMenu.x}px" style:top="{pageUi.hiddenMenu.y}px" {@attach within} {@attach dismissable(() => (pageUi.hiddenMenu = null))}>
    <div class="head">{t('Hidden pages')}</div>
    {#each hiddenViews() as v (v.id)}
      <button role="menuitem" style:--tint={v.color} onclick={() => hidePage(v.id, false)} title={t('Show it in the rail again')}><ViewIcon id={v.icon} />{t(v.label)}{#if v.origin === 'pack'}<small>{t('pack')}</small>{/if}</button>
    {/each}
  </div>
{/if}

<style>
  .pop { position: fixed; z-index: var(--z-menu); min-width: 210px; padding: 4px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); box-shadow: var(--shadow-pop); font-size: var(--fs-body); }
  .head { display: flex; align-items: center; gap: 7px; padding: 6px 8px 7px; color: var(--tint, var(--text)); font-weight: 600; }
  .head :global(svg), .menu button :global(svg) { flex: none; width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
  .head b { flex: 1; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pack { padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r8); color: var(--text-muted); font-size: var(--fs-tag); font-weight: 500; }
  .pack.changed { color: var(--warn); }
  .menu button { display: flex; align-items: center; gap: 9px; width: 100%; padding: 5px 8px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .menu button:hover:not(:disabled) { background: var(--surface-muted); }
  .menu button:disabled { opacity: 0.45; cursor: default; }
  .menu button[style] :global(svg) { color: var(--tint); }
  .menu .danger { color: var(--ko); }
  .menu .danger.armed { background: var(--ko-bg); font-weight: 600; }
  kbd, .menu small { margin-left: auto; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); }
  hr { margin: 4px 2px; border: none; border-top: 1px solid var(--border); }
  .cap { margin: 4px 8px 4px; color: var(--text-muted); font-size: var(--fs-tag); font-weight: 600; letter-spacing: 0.4px; text-transform: uppercase; }
  .scroll { max-height: 40vh; overflow: auto; }
  .grid { width: 262px; padding: 6px 8px 8px; }
  .icons { display: grid; grid-template-columns: repeat(7, 1fr); gap: 4px; }
  .icons button { display: grid; place-items: center; aspect-ratio: 1; border: 1px solid transparent; border-radius: var(--r6); background: none; color: var(--text-muted); cursor: pointer; }
  .icons button:hover { background: var(--surface-muted); color: var(--text); }
  .icons button.on { border-color: var(--tint); background: color-mix(in srgb, var(--tint) 14%, transparent); color: var(--tint); }
  .icons :global(svg) { width: 18px; height: 18px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
  .colors { display: flex; flex-wrap: wrap; gap: 6px; padding: 0 2px; }
  .sw { width: 22px; height: 22px; border: 2px solid var(--surface); border-radius: 50%; background: var(--c); box-shadow: 0 0 0 1px var(--border); cursor: pointer; }
  .sw.on { box-shadow: 0 0 0 2px var(--c); }
  .foot { display: flex; align-items: center; gap: 8px; margin-top: 10px; }
  .foot small { flex: 1; color: var(--text-muted); font-size: var(--fs-tag); }
  .ok { padding: 3px 10px; border: 1px solid var(--accent); border-radius: var(--r6); background: var(--accent); color: var(--on-accent); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
</style>
