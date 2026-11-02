<script lang="ts">
  import { anchored, dismissable, exclusive } from '../popup'
  import { tildePath } from '../platform'
  import { worktreeBranches, type BranchChoice } from '../worktrees'
  import Icon from './Icon.svelte'
  import { t } from '../i18n.svelte'

  /**
   * A worktree's "Change ▾" (Worktrees): its branches to switch to, searched as typed — the local
   * ones (most recent first), then origin's without a local one. A branch another worktree is on is
   * shown, not offered. Read when it opens (origin fetched first: a moment).
   */
  let { path, disabled = false, why = '', onpick }: { path: string; disabled?: boolean; why?: string; onpick: (branch: string) => void } = $props()

  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()
  let list = $state<BranchChoice[] | null>(null)
  let error = $state('')
  let query = $state('')

  async function toggle(e: MouseEvent) {
    e.stopPropagation()
    open = !open
    if (!open) return
    query = ''
    list = null
    error = ''
    try {
      list = await worktreeBranches(path)
    } catch (err) {
      error = String(err)
    }
  }
  const found = $derived((list ?? []).filter((b) => b.name.toLowerCase().includes(query.trim().toLowerCase())))
  const local = $derived(found.filter((b) => !b.remote))
  const remote = $derived(found.filter((b) => b.remote))
  /** Enter: the first branch offered. */
  const first = $derived(found.find((b) => !b.current && !b.elsewhere))
  function pick(b: BranchChoice) {
    if (b.current || b.elsewhere) return
    open = false
    onpick(b.name)
  }
</script>

<button bind:this={button} class="chg" class:open onclick={toggle} {disabled} title={disabled ? why : t('Switch this worktree to another branch')} aria-expanded={open}>{t('Change')}<Icon name="chevronDown" /></button>
{#if open}
  <div class="pop" role="dialog" aria-label={t('Switch branch')} {@attach anchored(() => button, { align: 'start', gap: 4 })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button })}>
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={query} placeholder={t('Search a branch…')} autofocus onkeydown={(e) => { if (e.key === 'Enter' && first) pick(first); else if (e.key === 'Escape') open = false }} />
    <div class="list">
      {#if error}<p class="err">{error}</p>
      {:else if !list}<p class="muted">{t('Fetching origin…')}</p>
      {:else if !found.length}<p class="muted">{t('No branch matches.')}</p>
      {:else}
        {#each [[t('Local'), local], [t('On origin'), remote]] as [label, items] (label)}
          {#if (items as BranchChoice[]).length}
            <p class="g">{label}</p>
            {#each items as BranchChoice[] as b (b.name)}
              <button class="it" class:off={b.current || !!b.elsewhere} disabled={b.current || !!b.elsewhere} onclick={() => pick(b)} title={b.elsewhere ? tildePath(b.elsewhere) : b.name}>
                <span class="nm">{b.name}</span>
                {#if b.current}<small>{t('current')}</small>{:else if b.elsewhere}<small>{t('in another worktree')}</small>{:else if b.remote}<small>{t('made local')}</small>{/if}
              </button>
            {/each}
          {/if}
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  button { border: none; background: none; color: inherit; font: inherit; cursor: pointer; }
  .chg { display: inline-flex; align-items: center; gap: 2px; height: 20px; padding: 0 6px; border-radius: var(--r5); color: var(--text-muted); font-size: var(--fs-meta); }
  .chg:hover:not(:disabled), .chg.open { background: var(--surface-muted); color: var(--text); }
  .chg:disabled { opacity: 0.45; cursor: default; }
  .pop { position: fixed; z-index: 45; display: flex; flex-direction: column; width: 320px; max-height: 360px; padding: 6px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); font-size: var(--fs-body); color: var(--text); }
  input { height: 26px; margin-bottom: 4px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; }
  .list { min-height: 0; overflow-y: auto; }
  .g { margin: 6px 6px 2px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .it { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 6px; border-radius: var(--r5); text-align: left; }
  .it:hover:not(:disabled) { background: var(--surface-muted); }
  .it.off { opacity: 0.45; cursor: default; }
  .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: var(--fs-mono); }
  .it small { flex: none; color: var(--text-muted); font-size: var(--fs-meta); }
  p.muted, p.err { margin: 6px; font-size: var(--fs-meta); }
  .muted { color: var(--text-muted); }
  .err { color: var(--ko); white-space: pre-wrap; }
</style>
