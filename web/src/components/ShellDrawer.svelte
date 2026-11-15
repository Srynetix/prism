<script lang="ts">
  import { tildePath } from '../platform'
  import { closeShellDrawer, shellDrawer } from '../shell-drawer.svelte'
  import Drawer from './ui/Drawer.svelte'
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
</script>

<Drawer open={!!owner} key="shell" identity={owner?.id ?? ''} crumb={owner?.title ?? ''} label={t('Shells of {name}', { name: owner?.title ?? '' })} size="l" source={pod ? 'var(--src-k8s)' : 'var(--src-gitlab)'} pin="shell" escape={false} onclose={closeShellDrawer}>
  {#snippet head()}
    <Icon name="terminal" />
    <span class="what">{owner?.title}</span>
    {#if pod}<span class="mono">{owner?.folder}</span>{/if}
    <span class="muted path" title={shellDrawer.where}>{pod ? shellDrawer.where : tildePath(shellDrawer.where)}</span>
  {/snippet}
  {#if owner}{#key owner.id}<div class="body"><Shells {owner} /></div>{/key}{/if}
</Drawer>

<style>
  .what { color: var(--text); font-weight: 600; white-space: nowrap; }
  .mono { color: var(--text); font-family: var(--mono); white-space: nowrap; }
  .path { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); }
  .body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
</style>
