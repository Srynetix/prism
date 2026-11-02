<script lang="ts">
  import Icon from './Icon.svelte'
  import AgentLauncher from './AgentLauncher.svelte'
  import { closeLauncherDrawer, kindNames, launcherDrawer } from '../agents.svelte'
  import { desktopShown } from '../rules'
  import { live } from '../live.svelte'
  import { t } from '../i18n.svelte'

  /**
   * The New agent launcher as a drawer, over the panel a Claude action came from (a ticket, an MR,
   * a worktree…): that panel stays behind, set back a little, and the view stays where it is; the
   * launch closes it, a toast leading to the agent. ←, ✕, Cancel or Escape go back to the panel.
   * Mounted before the other drawers, its Escape is heard first. A click elsewhere does not close
   * it: a prompt being written is not lost to a stray click.
   */
  const withDesktop = $derived(desktopShown(live.snapshot?.agents_settings))
  const categories = $derived(kindNames())

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !launcherDrawer.open) return
    // Before the panel behind and the window shortcuts.
    e.stopImmediatePropagation()
    closeLauncherDrawer()
  }
  // The panel behind is set back while the drawer is open.
  $effect(() => {
    document.documentElement.classList.toggle('launching', launcherDrawer.open)
    return () => document.documentElement.classList.remove('launching')
  })
</script>

<svelte:window onkeydowncapture={onKey} />

{#if launcherDrawer.open}
  <aside class="drawer ldrawer" data-drawer="53" aria-label={t('New agent')}>
    <header>
      {#if launcherDrawer.origin}<button class="back" onclick={closeLauncherDrawer} title={t('Back to {origin} (Esc)', { origin: launcherDrawer.origin })}><Icon name="back" />{launcherDrawer.origin}</button>{/if}
      <b><Icon name="terminal" />{t('New agent')}</b>
      <span class="grow"></span>
      <button class="x" onclick={closeLauncherDrawer} aria-label={t('Close (Esc)')}><Icon name="close" /></button>
    </header>
    <div class="body">
      <AgentLauncher kinds={categories} {withDesktop} canCancel slot={launcherDrawer} drawer onclose={closeLauncherDrawer} />
    </div>
  </aside>
{/if}

<style>
  .drawer { position: fixed; top: 0; right: 0; bottom: 0; z-index: 53; width: min(520px, 92vw); display: flex; flex-direction: column; background: var(--surface); border-left: 1px solid color-mix(in srgb, var(--src-claude) 45%, var(--border)); box-shadow: var(--shadow-drawer); animation: slide 0.16s ease-out; }
  @keyframes slide { from { transform: translateX(24px); opacity: 0; } }
  :global(html[data-reduce-motion]) .drawer { animation: none; }
  header { display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; border-bottom: 1px solid var(--border); box-shadow: inset 0 2px var(--src-claude); }
  header b { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; }
  header b :global(svg) { color: var(--src-claude); }
  .grow { flex: 1; }
  .back { display: inline-flex; align-items: center; gap: 4px; padding: 1px 6px; border: none; border-radius: var(--r6); background: none; color: var(--info); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .back:hover { background: var(--surface-muted); }
  .x { display: inline-flex; padding: 3px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .body { flex: 1; min-height: 0; overflow: auto; }
  /* On the phone (the remote): the whole screen, above its tabs; the panel behind stays as it is. */
  :global(html.remote) .drawer { left: 0; width: auto; z-index: 60; border-left: none; box-shadow: none; animation: none; font-size: 14px; }
  :global(html.remote) header { padding: calc(8px + env(safe-area-inset-top)) 12px 8px; }
  :global(html.remote) header b { font-size: 15px; }
  :global(html.remote) .x { padding: 8px; }
  :global(html.remote) .body { padding-bottom: env(safe-area-inset-bottom); }
  :global(html.remote.launching aside.drawer:not(.ldrawer)) { transform: none; filter: none; }
  /* The panel it came from, set back behind it. */
  :global(html.launching aside.drawer:not(.ldrawer)) { transform: translateX(-36px); filter: brightness(0.82); transition: transform 0.16s ease-out, filter 0.16s; }
</style>
