<script lang="ts">
  import { clickedOutside } from '../drawers'
  import { isPinned } from '../drawer-pins'
  import PinButton from './PinButton.svelte'
  import { tildePath } from '../platform'
  import { closeShellDrawer, shellDrawer } from '../shell-drawer.svelte'
  import Icon from './Icon.svelte'
  import Shells from './Shells.svelte'
  import { t } from '../i18n.svelte'

  /**
   * A worktree's shells (over the Worktrees view) or a pod's (from its logs pane), in a drawer: no
   * agent needed. Closing it (×, a click outside) only detaches the terminal; tmux keeps the
   * shells for the next time. Escape belongs to the shell, so it does not close the drawer.
   */
  const owner = $derived(shellDrawer.owner)
  const pod = $derived(shellDrawer.kind === 'pod')
  let drawerEl: HTMLElement | undefined = $state()
  function onOutside(e: PointerEvent) {
    if (shellDrawer.owner && clickedOutside(e, drawerEl)) {
      closeShellDrawer()
    }
  }
</script>

<svelte:window onpointerdowncapture={onOutside} />

{#if owner}
  <aside class="drawer" class:pod bind:this={drawerEl} data-drawer="51" data-pinned={isPinned('shell')} aria-label={t('Shells of {name}', { name: owner.title })}>
    <header>
      <Icon name="terminal" />
      <span class="what">{owner.title}</span>
      {#if pod}<span class="mono">{owner.folder}</span>{/if}
      <span class="muted path" title={shellDrawer.where}>{pod ? shellDrawer.where : tildePath(shellDrawer.where)}</span>
      <span class="grow"></span>
      <PinButton kind="shell" />
      <button class="icon" onclick={closeShellDrawer} title={t('Close (the shells keep running)')} aria-label={t('Close')}><Icon name="close" /></button>
    </header>
    {#key owner.id}<div class="body"><Shells {owner} /></div>{/key}
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 51; width: min(960px, 94vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid color-mix(in srgb, var(--src-gitlab) 45%, var(--border));
    box-shadow: var(--shadow-drawer);
  }
  header {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; white-space: nowrap;
    border-bottom: 1px solid var(--border); box-shadow: inset 0 2px var(--src-gitlab);
    font-size: var(--fs-meta); color: var(--text-muted);
  }
  .drawer.pod { border-left-color: color-mix(in srgb, var(--src-k8s) 45%, var(--border)); }
  .pod header { box-shadow: inset 0 2px var(--src-k8s); }
  .what { color: var(--text); font-weight: 600; }
  .mono { color: var(--text); font-family: var(--mono); }
  .path { min-width: 0; overflow: hidden; text-overflow: ellipsis; font-family: var(--mono); }
  .grow { flex: 1; }
  .icon { width: 24px; height: 24px; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover { background: var(--surface-muted); color: var(--text); }
  .body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
</style>
