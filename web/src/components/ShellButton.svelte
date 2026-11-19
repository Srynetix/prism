<script lang="ts">
  import { hasCore } from '../bridge'
  import { openFolderShell } from '../shell-drawer.svelte'
  import { t } from '../i18n.svelte'
  import Icon from './Icon.svelte'

  /**
   * A shell in Prism in `path` (a worktree, a clone, an agent's folder): the drawer of the
   * Worktrees block, the same shells wherever it is opened from. Where the core runs only.
   * `label`: « Shell » written beside the icon; otherwise the icon alone.
   */
  let { path, label = false }: { path: string; label?: boolean } = $props()
</script>

{#if hasCore()}
  {#if label}
    <button class="btn sm" onclick={() => openFolderShell(path)} title={t('A shell in Prism, in this folder: tmux keeps it after closing')}><Icon name="terminal" size={10} />{t('Shell')}</button>
  {:else}
    <button class="btn sm icon ghost" onclick={() => openFolderShell(path)} title={t('A shell in Prism, in this folder: tmux keeps it after closing')} aria-label={t('Shell')}><Icon name="terminal" /></button>
  {/if}
{/if}
