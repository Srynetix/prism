<script lang="ts">
  import { clickedOutside } from '../../drawers'
  import { t } from '../../i18n.svelte'
  import { live } from '../../live.svelte'
  import Icon from '../../components/Icon.svelte'
  import { changesView, closeChanges } from '../../changes-view.svelte'
  import ChangesView from './ChangesView.svelte'
  import type { ChangesState } from './changes'

  /** "What changed": the Changes timeline in a drawer, around a crash or on an environment. */
  let { phone = false }: { phone?: boolean } = $props()
  const journal = $derived(live.snapshot?.blocks?.changes as ChangesState | undefined)
  const error = $derived(live.snapshot?.errors?.['changes'] as string | undefined)
  let drawerEl = $state<HTMLElement>()

  function onKey(e: KeyboardEvent) {
    if (changesView.open && e.key === 'Escape' && !e.defaultPrevented) {
      e.preventDefault()
      closeChanges()
    }
  }
  function onOutside(e: PointerEvent) {
    if (changesView.open && clickedOutside(e, drawerEl)) closeChanges()
  }
</script>

<svelte:window onkeydowncapture={onKey} onpointerdowncapture={onOutside} />

{#if changesView.open}
  <aside class="drawer" class:phone bind:this={drawerEl} data-drawer="51" aria-label={t('What changed')}>
    <header>
      <Icon name="activity" />
      <span class="what">{t('What changed')}</span>
      {#if changesView.focus?.env}<span class="muted">{changesView.focus.env}{changesView.focus.app ? ` · ${changesView.focus.app}` : ''}</span>{/if}
      <span class="grow"></span>
      <button class="icon" onclick={closeChanges} aria-label={t('Close')} title={t('Close (Esc)')}><Icon name="close" /></button>
    </header>
    {#if journal}
      <ChangesView state={journal} {error} focus={changesView.focus} />
    {:else}
      <p class="pad muted">{t('The Changes block is off. Turn it on in Settings › Infra › Changes ([changes] enabled).')}</p>
    {/if}
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 51; width: min(920px, 96vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid var(--border); box-shadow: var(--shadow-drawer);
  }
  .drawer.phone { width: 100vw; }
  header {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; white-space: nowrap;
    border-bottom: 1px solid var(--border); box-shadow: inset 0 2px var(--accent);
    font-size: var(--fs-meta); color: var(--text-muted);
  }
  header :global(svg) { width: 13px; height: 13px; }
  .what { color: var(--text); font-weight: 600; }
  .grow { flex: 1; }
  .muted { color: var(--text-muted); }
  .pad { padding: 16px; }
  .icon { width: 24px; height: 24px; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover { background: var(--surface-muted); color: var(--text); }
</style>
