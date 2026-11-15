<script lang="ts">
  import Icon from './Icon.svelte'
  import Drawer from './ui/Drawer.svelte'
  import AgentLauncher from './AgentLauncher.svelte'
  import { closeLauncherDrawer, kindNames, launcherDrawer } from '../agents.svelte'
  import { desktopShown } from '../rules'
  import { live } from '../live.svelte'
  import { layers } from '../layers.svelte'
  import { t } from '../i18n.svelte'

  /**
   * The New agent launcher as a drawer, over the panel a Claude action came from (a ticket, an MR,
   * a worktree…): that panel stays behind, set back a little (the stack's crumbs lead back to it),
   * and the view stays where it is; the launch closes it, a toast leading to the agent. ←, ✕,
   * Cancel or Escape go back to the panel. A click elsewhere does not close it: a prompt being
   * written is not lost to a stray click.
   */
  const withDesktop = $derived(desktopShown(live.snapshot?.agents_settings))
  const categories = $derived(kindNames())
  const phone = document.documentElement.classList.contains('remote')
  // Over another drawer, its crumb leads back; over a view, the origin's own Back.
  const overDrawer = $derived(layers.stack.filter((l) => l.kind === 'drawer' && l.key !== 'launcher').length > 0)
</script>

<Drawer open={launcherDrawer.open} key="launcher" crumb={t('New agent')} label={t('New agent')} size={phone ? 'full' : 's'} source="var(--src-claude)" outside={false} onclose={closeLauncherDrawer}>
  {#snippet head()}
    {#if launcherDrawer.origin && !overDrawer}<button class="btn sm ghost" onclick={closeLauncherDrawer} title={t('Back to {origin} (Esc)', { origin: launcherDrawer.origin })}><Icon name="back" />{launcherDrawer.origin}</button>{/if}
    <b><Icon name="terminal" />{t('New agent')}</b>
  {/snippet}
  <div class="body">
    <AgentLauncher kinds={categories} {withDesktop} canCancel slot={launcherDrawer} drawer onclose={closeLauncherDrawer} />
  </div>
</Drawer>

<style>
  b { display: inline-flex; align-items: center; gap: var(--sp-2); font-size: var(--fs-title); color: var(--text); }
  b :global(svg) { color: var(--src-claude); }
  .body { flex: 1; min-height: 0; overflow: auto; }
  :global(html.remote) .body { padding-bottom: env(safe-area-inset-bottom); }
</style>
