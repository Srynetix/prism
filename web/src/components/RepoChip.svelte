<script lang="ts">
  import { live } from '../live.svelte'
  import { openSettings } from '../settings.svelte'
  import { t } from '../i18n.svelte'

  /**
   * In a block's title, the repositories it reads (`Snapshot.repo_blocks`): shown once the config
   * has several, a click on it leads to where they are chosen.
   */
  let { id }: { id: string } = $props()
  const repos = $derived(live.snapshot?.repo_blocks?.find((b) => b.id === id)?.repos ?? [])
  const several = $derived((live.snapshot?.repositories.length ?? 0) > 1)
</script>

{#if several && repos.length}
  <button class="pill outline mono repo" onclick={() => openSettings('repo-blocks')} title={t('The repository it reads: Settings › Repositories')}>{repos.join(', ')}</button>
{/if}

<style>
  .repo { margin-left: 6px; background: none; font-weight: 400; text-transform: none; letter-spacing: 0; }
  .repo:hover { color: var(--text); box-shadow: inset 0 0 0 1px var(--accent); }
</style>
