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
  <button class="repo" onclick={() => openSettings('repo-blocks')} title={t('The repository it reads: Settings › Repositories')}>{repos.join(', ')}</button>
{/if}

<style>
  .repo { margin-left: 6px; padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r9); background: none; color: var(--text-muted); font: var(--fs-tag) var(--mono); font-weight: 400; line-height: 16px; cursor: pointer; text-transform: none; letter-spacing: 0; }
  .repo:hover { border-color: var(--accent); color: var(--text); }
</style>
