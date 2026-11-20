<script lang="ts">
  import Icon from './Icon.svelte'
  import { live } from '../live.svelte'
  import { recreateWorktree, resumeInRepo } from '../agents.svelte'
  import { isArchived, setArchived } from '../agent-archive.svelte'
  import { toast } from '../toast.svelte'
  import { t } from '../i18n.svelte'
  import type { Agent } from '../types'

  /**
   * Its worktree removed under it (a command of its own, a script, by hand): said, and what next —
   * go on in the repository, make the worktree again on its branch (still there), or archive it,
   * first when its branch is merged (never on its own). The same in the Agents view and on the phone.
   */
  let { a }: { a: Agent } = $props()

  const gone = $derived(a.worktree_gone)
  const merged = $derived(!!gone?.branch && !!live.snapshot?.merge_requests.some((m) => m.source_branch === gone.branch && m.state === 'merged'))
  let busy = $state(false)
  async function run(f: () => Promise<unknown>) {
    busy = true
    try {
      await f()
    } catch (e) {
      toast(String(e), 6000, null, 'error')
    } finally {
      busy = false
    }
  }
</script>

{#if gone}
  <div class="banner" role="status">
    <Icon name="warning" />
    <div class="what">
      <b>{t('Its worktree was removed')}</b>
      <code>{gone.path}</code>
      <small>{t('Its conversation stays. Its files, shells and diff are not shown from the repository instead.')}</small>
      {#if merged}<small class="plan">{t('{branch} is merged: archive it if its work is done.', { branch: gone.branch ?? '' })}</small>{/if}
    </div>
    <div class="acts">
      {#if merged && !isArchived(a)}<button class="btn sm primary" disabled={busy} onclick={() => setArchived(a, true)}>{t('Archive')}</button>{/if}
      {#if gone.branch_exists}<button class="btn sm" class:primary={!merged} disabled={busy} onclick={() => run(() => recreateWorktree(a.id))} title={t('Made again on {branch}, its conversation resumed in it', { branch: gone.branch ?? '' })}>{t('Recreate the worktree')}</button>{/if}
      <button class="btn sm" class:primary={!merged && !gone.branch_exists} disabled={busy} onclick={() => run(() => resumeInRepo(a.id))} title={t('Its conversation resumed in the repository, the worktree forgotten')}>{t('Resume in the repository')}</button>
      {#if !merged && !isArchived(a)}<button class="btn sm ghost" disabled={busy} onclick={() => setArchived(a, true)}>{t('Archive')}</button>{/if}
    </div>
  </div>
{/if}

<style>
  .banner { display: flex; align-items: flex-start; gap: 10px; padding: 8px 12px; border-bottom: 1px solid color-mix(in srgb, var(--warn) 35%, var(--border)); background: var(--warn-bg); color: var(--warn); }
  .banner > :global(svg) { margin-top: 3px; flex-shrink: 0; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; font-size: var(--fs-meta); color: var(--text); }
  .what b { color: var(--warn); }
  .what small { color: var(--text-muted); }
  .what code { color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); overflow-wrap: anywhere; }
  .what .plan { color: var(--text); }
  .acts { display: flex; flex-wrap: wrap; gap: 6px; justify-content: flex-end; }
</style>
