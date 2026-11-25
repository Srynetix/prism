<script lang="ts">
  /**
   * A project's local branches (`project-branches.svelte.ts`), below its workstreams: each with its
   * last commit, its worktree, its MR, where it stands against origin and the base, and a verdict
   * (safe to delete, kept and why, never deleted and why). On the computer: a worktree opened on
   * one, an agent there, the clone switched to one, deleted (two clicks; what it would lose only by
   * Force). « Tidy up »: origin pruned, then the safe ones picked from a list deleted, and what was
   * done; on a phone too, with its passkey.
   */
  import { untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import ArmedLabel from './ArmedLabel.svelte'
  import { t, tn } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { isDesktop } from '../bridge'
  import { ago } from '../time'
  import { toast } from '../toast.svelte'
  import { armer } from '../armed.svelte'
  import { tildePath } from '../platform'
  import { mrKey } from '../mr-ref'
  import { openMr } from '../mr-view.svelte'
  import { repoName } from '../repositories'
  import { agentsAvailable, openLauncher } from '../agents.svelte'
  import { branchesOf, branchWorktree, closeTidy, deleteBranch, loadBranches, prepareTidy, runTidy, safe, switchClone, tidy, togglePicked, type LocalBranch } from '../project-branches.svelte'
  import type { MergeRequest } from '../types'

  /** `tidyable`: Tidy up offered (a phone that may act). */
  let { repo, tidyable = true }: { repo: string; tidyable?: boolean } = $props()

  const FEW = 12
  const desk = isDesktop()
  const armed = armer()
  let all = $state(false)
  let busy = $state<string | null>(null)

  $effect(() => {
    const r = repo
    untrack(() => {
      all = false
      void loadBranches(r)
    })
  })

  const b = $derived(branchesOf(repo))
  const list = $derived(b.list ?? [])
  const shown = $derived(all ? list : list.slice(0, FEW))
  const safeCount = $derived(list.filter(safe).length)
  const mine = $derived(tidy.repo === repo)

  /** A branch's MR, as its verdict reads it: its open one, else the latest merged, else the latest. */
  function mrOf(name: string): MergeRequest | undefined {
    const s = live.snapshot
    if (!s) return undefined
    const of = s.merge_requests.filter((m) => repoName(m.repo, s) === repo && m.source_branch === name)
    const latest = (ms: MergeRequest[], at: (m: MergeRequest) => string | null | undefined) => [...ms].sort((a, b) => (at(b) ?? '').localeCompare(at(a) ?? ''))[0]
    return of.find((m) => m.state === 'opened') ?? latest(of.filter((m) => m.merged_at), (m) => m.merged_at) ?? latest(of, (m) => m.updated_at)
  }

  async function act(key: string, f: () => Promise<unknown>, done?: string) {
    if (busy) return
    busy = key
    try {
      await f()
      if (done) toast(done)
    } catch (e) {
      toast(String(e), 8000)
    } finally {
      busy = null
    }
  }

  function remove(x: LocalBranch) {
    const force = !!x.keep
    const key = `${force ? 'force' : 'rm'}:${x.name}`
    if (!armed.confirm(key)) return
    void act(key, () => deleteBranch(repo, x.name, force), t('{branch} deleted', { branch: x.name }))
  }
  const worktree = (x: LocalBranch) => act(`wt:${x.name}`, async () => toast(t('Worktree opened: {path}', { path: tildePath(await branchWorktree(repo, x.name)) })))
  function switchTo(x: LocalBranch) {
    if (!armed.confirm(`sw:${x.name}`)) return
    void act(`sw:${x.name}`, () => switchClone(repo, x.name), t('The clone is on {branch}', { branch: x.name }))
  }
  const agentHere = (x: LocalBranch) => x.worktree && openLauncher({ title: x.name, cwd: x.worktree })
</script>

<section class="branches" aria-label={t('Branches')}>
  <div class="bar">
    <b>{t('Branches')}</b>
    {#if b.list}<span class="muted">{tn(list.length, '{n} local branch', '{n} local branches')}{#if safeCount}{' '}· {tn(safeCount, '{n} branch safe to delete', '{n} branches safe to delete')}{/if}</span>{/if}
    <span class="grow"></span>
    {#if tidyable}<button class="btn sm" onclick={() => void prepareTidy(repo)} disabled={tidy.preparing || tidy.running} title={t('Origin read again and pruned, then the branches done with nothing to lose, to pick and delete')}><Icon name="trash" size={10} />{t('Tidy up')}</button>{/if}
    <button class="btn sm icon ghost" onclick={() => void loadBranches(repo)} disabled={b.loading} title={t('Read the branches again')} aria-label={t('Reload')}><Icon name="refresh" size={11} /></button>
  </div>

  {#if mine}
    <div class="tidy" role="region" aria-label={t('Tidy up')}>
      {#if tidy.preparing}
        <p class="muted">{t('Reading origin again and pruning…')}</p>
      {:else if tidy.error}
        <p class="ko">{tidy.error}</p>
        <div class="acts"><button class="btn sm" onclick={closeTidy}>{t('Close')}</button></div>
      {:else if tidy.report}
        {@const r = tidy.report}
        <p>{tn(r.deleted.length, '{n} branch deleted', '{n} branches deleted')}{#if r.deleted.length}: <code>{r.deleted.join(', ')}</code>{/if}</p>
        {#if r.kept.length}
          <p class="muted">{tn(r.kept.length, '{n} branch kept:', '{n} branches kept:')}</p>
          <ul>{#each r.kept as k (k.branch)}<li><code>{k.branch}</code> · {k.why}</li>{/each}</ul>
        {/if}
        <div class="acts"><button class="btn sm" onclick={closeTidy}>{t('Done')}</button></div>
      {:else if tidy.list}
        {@const candidates = tidy.list.filter(safe)}
        {#if candidates.length}
          <p>{t('Done, with nothing to lose: deleted with their worktree. Nothing is deleted on the forge.')}</p>
          <ul class="pick">
            {#each candidates as x (x.name)}
              <li><label><input type="checkbox" checked={tidy.picked.includes(x.name)} onchange={() => togglePicked(x.name)} /><code>{x.name}</code><span class="muted">{x.done.join(' · ')}{#if x.worktree}{' '}· {t('its worktree too')}{/if}</span></label></li>
            {/each}
          </ul>
          <div class="acts">
            <button class="btn sm danger" onclick={() => void runTidy()} disabled={tidy.running || !tidy.picked.length}><Icon name="trash" size={10} />{tidy.running ? t('Deleting…') : tn(tidy.picked.length, 'Delete {n} branch', 'Delete {n} branches')}</button>
            <button class="btn sm ghost" onclick={closeTidy} disabled={tidy.running}>{t('Cancel')}</button>
          </div>
        {:else}
          <p class="muted">{t('Nothing to tidy: no branch is done with nothing to lose.')}</p>
          <div class="acts"><button class="btn sm" onclick={closeTidy}>{t('Close')}</button></div>
        {/if}
      {/if}
    </div>
  {/if}

  {#if b.error}
    <p class="ko">{b.error}</p>
  {:else if !b.list && b.loading}
    <p class="muted">{t('Reading the branches…')}</p>
  {:else if b.list && !list.length}
    <p class="muted">{t('No local branch.')}</p>
  {:else}
    <ul class="rows">
      {#each shown as x (x.name)}
        {@const mr = mrOf(x.name)}
        <li class:safe={safe(x)} class:locked={!!x.locked}>
          <div class="l1">
            <code class="nm" title={x.name}>{x.name}</code>
            {#if x.current}<span class="pill">{t('the clone')}</span>{:else if x.worktree}<span class="pill" title={x.worktree}><Icon name="folder" size={9} />{t('worktree')}</span>{/if}
            {#if mr}<button class="btn sm key" onclick={() => openMr(mr)} title={mr.title}>{mrKey(mr)}{#if mr.state !== 'opened'} · {mr.state}{/if}</button>{/if}
            {#if x.upstream_gone}<span class="pill ok">{t('gone from origin')}</span>{:else if !x.upstream}<span class="pill">{t('never pushed')}</span>{/if}
            {#if x.to_push}<span class="n" title={t('Commits not on its origin branch yet')}>↑{x.to_push}</span>{/if}
            {#if x.to_pull}<span class="n" title={t('Commits of its origin branch not here')}>↓{x.to_pull}</span>{/if}
            {#if x.ahead !== null && x.behind !== null}<span class="n muted" title={t('Against the base: its own commits, and the base’s it lacks')}>+{x.ahead} −{x.behind}</span>{/if}
            <span class="grow"></span>
            <span class="tm muted">{x.last_commit ? ago(x.last_commit) : ''}</span>
          </div>
          <div class="l2">
            <span class="subj muted" title={`${x.head} · ${x.author}`}>{x.subject}</span>
            <span class="verdict" class:ok={safe(x)} class:warn={!!x.keep}>{x.locked ?? x.keep ?? (x.done.length ? x.done.join(' · ') : t('nothing to lose'))}</span>
          </div>
          {#if desk && !x.locked}
            <div class="acts">
              {#if !x.worktree}
                <button class="btn sm" onclick={() => void worktree(x)} disabled={!!busy} title={t('A worktree on this branch, next to the clone, without an agent')}><Icon name="plus" size={10} />{t('Worktree')}</button>
                <button class="btn sm" class:armed={armed.is(`sw:${x.name}`)} onclick={() => switchTo(x)} disabled={!!busy} title={t('The clone checks this branch out (nothing uncommitted, no agent in it)')}><ArmedLabel label={t('Switch the clone')} armedLabel={t('Click again to switch')} armed={armed.is(`sw:${x.name}`)} /></button>
              {:else}
                <button class="btn sm" onclick={() => agentHere(x)} disabled={!agentsAvailable()} title={t('A new agent in its worktree')}><Icon name="plus" size={10} />{t('Agent here')}</button>
              {/if}
              {#if x.keep}
                <button class="btn sm danger" class:armed={armed.is(`force:${x.name}`)} onclick={() => remove(x)} disabled={!!busy} title={t('Would lose {what}', { what: x.keep })}><Icon name="trash" size={10} /><ArmedLabel label={t('Force')} armedLabel={t('Click again: loses {what}', { what: x.keep })} armed={armed.is(`force:${x.name}`)} /></button>
              {:else}
                <button class="btn sm danger" class:armed={armed.is(`rm:${x.name}`)} onclick={() => remove(x)} disabled={!!busy} title={x.worktree ? t('Deletes the branch and its worktree') : t('Deletes the local branch')}><Icon name="trash" size={10} /><ArmedLabel label={t('Delete')} armedLabel={t('Click again to confirm')} armed={armed.is(`rm:${x.name}`)} /></button>
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
    {#if list.length > FEW}<button class="btn sm ghost more" onclick={() => (all = !all)}>{all ? t('Fewer') : t('All {n} branches', { n: list.length })}</button>{/if}
  {/if}
</section>

<style>
  .branches { display: flex; flex-direction: column; gap: 6px; margin-top: 14px; }
  .bar { display: flex; align-items: center; gap: 8px; }
  .bar .muted { font-size: var(--fs-meta); }
  .grow { flex: 1; }
  .muted { color: var(--text-muted); }
  .ko { color: var(--ko); font-size: var(--fs-meta); margin: 0; }
  p { margin: 0; font-size: var(--fs-meta); }
  .tidy { display: flex; flex-direction: column; gap: 6px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r6); background: color-mix(in srgb, var(--warn) 7%, var(--surface)); }
  .tidy ul { margin: 0; padding-left: 16px; font-size: var(--fs-meta); }
  .tidy .pick { list-style: none; padding: 0; }
  .tidy .pick label { display: flex; align-items: center; gap: 6px; cursor: pointer; }
  .acts { display: flex; flex-wrap: wrap; gap: 6px; }
  .rows { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .rows li { display: flex; flex-direction: column; gap: 3px; padding: 7px 10px; border-left: 3px solid transparent; font-size: var(--fs-meta); }
  .rows li + li { border-top: 1px solid var(--border); }
  .rows li.safe { border-left-color: var(--ok); }
  .rows li.locked { opacity: 0.75; }
  .l1, .l2 { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .nm { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); font-weight: 600; }
  .n { font-family: var(--mono); font-size: var(--fs-tag); white-space: nowrap; }
  .tm { white-space: nowrap; font-size: var(--fs-tag); }
  .subj { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .verdict { white-space: nowrap; font-size: var(--fs-tag); color: var(--text-muted); }
  .verdict.ok { color: var(--ok); }
  .verdict.warn { color: var(--warn); }
  .rows .acts { margin-top: 2px; }
  .armed { background: var(--ko-bg); color: var(--ko); }
  .more { align-self: flex-start; }
  @media (max-width: 719.98px) {
    .l2 { flex-wrap: wrap; }
    .verdict { white-space: normal; }
  }
</style>
