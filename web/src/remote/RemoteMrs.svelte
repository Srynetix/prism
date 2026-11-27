<script lang="ts">
  import { clock } from '../clock.svelte'
  import { t, tc } from '../i18n.svelte'
  import Icon from '../components/Icon.svelte'
  import PipelineBar from '../components/PipelineBar.svelte'
  import { armer } from '../armed.svelte'
  import ArmedLabel from '../components/ArmedLabel.svelte'
  import State from '../components/ui/State.svelte'
  import { toastError } from '../toast.svelte'
  import { changesRequested, mrNeedsMe, nextStep, openMrs } from '../rules'
  import { meOn } from '../forge'
  import { mrTickets } from '../conventions'
  import { primaryAction } from '../mr-actions'
  import MrTitle from '../components/MrTitle.svelte'
  import { mrKey } from '../mr-ref'
  import { openMr } from '../mr-view.svelte'
  import { branchName } from '../pipeline'
  import { forgeAvatar, initialsOf, personColor, repoColor } from '../people'
  import { pref, setPref } from '../layout.svelte'
  import { byTarget, mergePriority, mergeQueue, queueCount, queueRepos, type MergeQueue } from '../merge-queue'
  import { primaryRepo } from '../repositories'
  import type { MergeRequest, Snapshot } from '../types'
  import MrDepChips from '../components/MrDepChips.svelte'
  import { chains, findMr, heldIn, releaseHeld, tree } from '../mr-deps'
  import { setMergeQueue } from '../pipeline-actions'
  import { cleanTitle } from '../mr-actions'
  import type { Held } from '../bindings/Held'

  /**
   * The MRs on a phone: Team / Mine as the page's switch, one scrolled row of filters (repos, then
   * people), the protected branches as a strip; then one list, in the order picked (last update,
   * oldest first, or by repository). A row: who and its state, the title on two lines, its
   * pipeline and context; one that needs you (an MR to review, yours blocked or to act on) is
   * marked in its tone and has its next step as a button. A tap opens it (MrPanel, full screen). Queue: the Merge queue block's MRs of one repository (the
   * same `queue.repo` as the window's), numbered in the order the job takes them.
   */
  let { snap }: { snap: Snapshot } = $props()

  type Tab = 'team' | 'mine' | 'queue'
  let chosenTab = $state<Tab>(pref<Tab>('remote.mrs.tab', 'team'))
  const setTab = (t: Tab) => setPref('remote.mrs.tab', (chosenTab = t))
  const qRepos = $derived(queueRepos(snap))
  // No repository with a queue or an MR waiting to merge: no Queue tab, Team instead.
  const tab = $derived<Tab>(chosenTab === 'queue' && !qRepos.length ? 'team' : chosenTab)
  let repo = $state<string | null>(null)
  let author = $state<string | null>(null)

  const queueLabel = $derived(snap.merge_queue_label)
  const prefix = $derived(snap.merge_priority_prefix)
  const all = $derived(openMrs(snap).sort((a, b) => b.updated_at.localeCompare(a.updated_at)))
  const isMine = (m: MergeRequest) => m.author === meOn(snap, m)
  const mine = $derived(all.filter(isMine))
  const repos = $derived([...new Set(all.map((m) => m.repo ?? ''))].sort())
  const authors = $derived.by(() => {
    const by = new Map<string, { username: string; name: string; count: number }>()
    for (const m of all) {
      const a = by.get(m.author) ?? { username: m.author, name: isMine(m) ? t('You') : (m.author_name || m.author).split(' ')[0], count: 0 }
      a.count++
      by.set(m.author, a)
    }
    return [...by.values()].sort((a, b) => b.count - a.count || a.name.localeCompare(b.name))
  })
  const shown = $derived((tab === 'mine' ? mine : all).filter((m) => (repo === null || (m.repo ?? '') === repo) && (tab === 'mine' || !author || m.author === author)))
  const needsMe = (m: MergeRequest) => mrNeedsMe(m, meOn(snap, m), queueLabel)
  type Sort = 'updated' | 'oldest' | 'repo' | 'chains'
  let sort = $state<Sort>(pref<Sort>('remote.mrs.sort', 'updated'))
  /** By chain (each MR under the one it waits for), offered when the list has a dependency. */
  const linked = $derived(chains(snap, shown, { tickets: true, merged: true }))
  const byChain = $derived(sort === 'chains' && linked.chains.length > 0)
  const setSort = (s: Sort) => setPref('remote.mrs.sort', (sort = s))
  const listed = $derived(
    sort === 'oldest' ? [...shown].reverse() : sort === 'repo' ? [...shown].sort((a, b) => (a.repo ?? '').localeCompare(b.repo ?? '')) : shown,
  )
  const mainName = $derived(snap.repos?.[0]?.name ?? 'main')
  // The primary repository's first, each repository's in its list's order (`pipeline_branches`).
  const branches = $derived([...snap.pipelines].sort((a, b) => +!!a.repo - +!!b.repo || (a.repo ?? '').localeCompare(b.repo ?? '')))

  let qChosen = $state<string | null>(pref<string | null>('queue.repo', null))
  const setQRepo = (r: string) => setPref('queue.repo', (qChosen = r))
  const qRepo = $derived(qChosen && qRepos.includes(qChosen) ? qChosen : (qRepos[0] ?? primaryRepo(snap)?.name ?? ''))
  const queues = $derived(new Map(qRepos.map((r) => [r, mergeQueue(snap, r)])))
  const q = $derived(queues.get(qRepo) ?? mergeQueue(snap, qRepo))
  const qTotal = $derived([...queues.values()].reduce((n, x) => n + queueCount(x), 0))
  const qBranches = $derived(byTarget(q))

  const now = $derived(clock(1000).now)

  /** The queue's repository's MRs held out of it until the MR they wait for is merged. */
  const held = $derived(heldIn(snap, qRepo))
  async function heldAct(h: Held, back: boolean) {
    const ref = { iid: h.iid, repo: h.repo ?? null }
    busy = mrKey(ref)
    try {
      await (back ? setMergeQueue(ref, true) : releaseHeld(ref))
    } catch (err) {
      toastError(String(err))
    } finally {
      busy = null
    }
  }

  /** The row's button: its next step's action, a second tap for one that cannot be undone. */
  let busy = $state<string | null>(null)
  const armed = armer()
  async function act(e: MouseEvent, m: MergeRequest) {
    e.stopPropagation()
    const p = primaryAction(nextStep(m, meOn(snap, m), queueLabel).primary, m, queueLabel)
    if (!p || busy) return
    if ('href' in p) return void window.open(p.href, '_blank', 'noopener')
    if (p.confirm && !armed.confirm(mrKey(m))) return
    armed.disarm()
    busy = mrKey(m)
    try {
      await p.run()
    } catch (err) {
      toastError(String(err))
    } finally {
      busy = null
    }
  }
</script>

{#snippet row(m: MergeRequest, pos?: number)}
  {@const s = nextStep(m, meOn(snap, m), queueLabel)}
  {@const p = primaryAction(s.primary, m, queueLabel)}
  {@const k = mrKey(m)}
  {@const prio = pos ? mergePriority(m, prefix) : null}
  {@const pic = forgeAvatar(m.author_avatar)}
  {@const me = tab !== 'queue' && needsMe(m)}
  <div class="item {s.tone}" class:me role="button" tabindex="0" onclick={() => openMr(m)} onkeydown={(e) => e.key === 'Enter' && openMr(m)}>
    {#if pos}<span class="pos" class:head={pos === 1} title={pos === 1 ? (prefix ? t('Next: the one the job takes') : t('Next: the oldest MR with the label, the one the job takes')) : t('Number {n} in the queue', { n: pos })}>{pos === 1 ? '▶' : pos}</span>{/if}
    <span class="av" class:pic={!!pic} style:--c={personColor(m.author)}>{#if pic}<img src={pic} alt="" />{:else}{initialsOf(m.author_name, m.author)}{/if}</span>
    <div class="body">
      <div class="h">
        {#if m.repo}<span class="repo" style:--c={repoColor(m.repo)}>{m.repo}</span>{/if}
        <span class="iid">{k}</span>
        {#if prio !== null}<span class="prio" title={t('Merge priority {n} (label {label})', { n: prio, label: `${prefix}${prio}` })}>P{prio}</span>{/if}
        <span class="who">{isMine(m) ? t('you') : m.author_name || m.author}</span>
        <span class="grow"></span>
        <span class="step">{s.label}</span>
      </div>
      <div class="title"><MrTitle mr={m} /></div>
      <div class="meta">
        <span class="pipe"><PipelineBar pipeline={m.head_pipeline} status={m.pipeline} {now} /></span>
        {#if m.ticket}<span class="tk">{mrTickets(m).join(' ')}</span>{/if}
        {#if !m.draft}<span class:ok={m.approvals_left <= 0 && !changesRequested(m)}><Icon name="check" /> {m.approved_by.length}/{m.approved_by.length + Math.max(0, m.approvals_left)}</span>{/if}
        {#if m.open_threads}<span><Icon name="comment" /> {m.open_threads}</span>{/if}
        <MrDepChips mr={m} />
      </div>
    </div>
    {#if p && (tab === 'queue' || me)}
      <button class="btn sm act" class:armed={armed.is(k)} class:busy={busy === k} disabled={busy === k} onclick={(e) => act(e, m)} title={p.title}>{#if busy === k}<span class="spinner"></span>{/if}<ArmedLabel label={p.label} armedLabel={t('Tap again to confirm')} armed={armed.is(k)} /></button>
    {/if}
  </div>
{/snippet}

{#snippet sections(b: MergeQueue)}
  {#if b.hasQueue}
    <div class="grp"><Icon name="queue" /> {t('In the queue')} · {b.queued.length}<span class="hint">{prefix ? t('by priority, then oldest first') : t('oldest first')}</span></div>
    {#each b.queued as m, i (mrKey(m))}{@render row(m, i + 1)}{:else}<State kind="empty" title={t('No MR with the “{label}” label.', { label: queueLabel ?? '' })} />{/each}
  {/if}
  {#if b.auto.length}
    <div class="grp"><Icon name="automerge" /> {b.hasQueue ? t('Auto-merge, out of the queue') : t('Auto-merge')} · {b.auto.length}</div>
    {#each b.auto as m (mrKey(m))}{@render row(m)}{/each}
  {/if}
  {#if b.ready.length}
    <div class="grp"><Icon name="check" /> {b.hasQueue ? t('Approved, not in the queue') : t('Approved, auto-merge off')} · {b.ready.length}</div>
    {#each b.ready as m (mrKey(m))}{@render row(m)}{/each}
  {/if}
{/snippet}

<div class="mrs">
  <div class="seg" role="group" aria-label={t('Merge requests shown')}>
    <button aria-pressed={tab === 'team'} onclick={() => setTab('team')}>{t('Team')} · {all.length}</button>
    <button aria-pressed={tab === 'mine'} onclick={() => setTab('mine')}>{tc('infra', 'Mine')} · {mine.length}</button>
    {#if qRepos.length}<button aria-pressed={tab === 'queue'} onclick={() => setTab('queue')}>{t('Queue')} · {qTotal}</button>{/if}
  </div>
  {#if tab === 'queue'}
    {#if qRepos.length > 1}
      <div class="filters">
        {#each qRepos as r (r)}<button class="btn sm" aria-pressed={qRepo === r} onclick={() => setQRepo(r)}>{r} {queueCount(queues.get(r)!)}</button>{/each}
      </div>
    {/if}
    {#if qBranches.length > 1}
      {#each qBranches as b (b.target)}
        <div class="branch"><Icon name="branch" /> → {b.target}<b>{queueCount(b)}</b></div>
        {@render sections(b)}
      {/each}
    {:else}
      {@render sections(q)}
    {/if}
    {#if held.length}
      <div class="grp"><Icon name="pause" /> {t('Held by Prism')} · {held.length}<span class="hint">{t('back in the queue once the MR they wait for is merged')}</span></div>
      {#each held as h (mrKey({ iid: h.iid, repo: h.repo }))}
        {@const k = mrKey({ iid: h.iid, repo: h.repo })}
        {@const m = findMr(snap, { iid: h.iid, repo: h.repo ?? null })}
        <div class="held">
          <div class="body">
            <div class="h"><span class="iid">{k}</span><span class="grow"></span><span class="step">{t('back at the merge of {mr}', { mr: mrKey({ iid: h.on_iid, repo: h.on_repo }) })}</span></div>
            <div class="title">{m ? cleanTitle(m) : ''}</div>
            <div class="acts">
              <button class="btn sm" disabled={busy === k} onclick={() => heldAct(h, true)}>{t('Put back now')}</button>
              <button class="btn sm ghost" disabled={busy === k} onclick={() => heldAct(h, false)}>{t('Forget')}</button>
            </div>
          </div>
        </div>
      {/each}
    {/if}
    {#if !queueCount(q) && !q.hasQueue}<State kind="empty" title={t('Nothing waiting to merge in {repo}.', { repo: qRepo ?? '' })} />{/if}
  {:else}
  {#if repos.length > 1 || (tab === 'team' && authors.length > 1)}
    <div class="filters">
      {#if repos.length > 1}
        <button class="btn sm" aria-pressed={repo === null} onclick={() => (repo = null)}>{t('All repos')}</button>
        {#each repos as r (r)}<button class="btn sm" aria-pressed={repo === r} onclick={() => (repo = repo === r ? null : r)}>{r || mainName} {all.filter((m) => (m.repo ?? '') === r).length}</button>{/each}
      {/if}
      {#if tab === 'team' && authors.length > 1}
        {#if repos.length > 1}<span class="sep"></span>{/if}
        {#each authors as a (a.username)}<button class="btn sm" aria-pressed={author === a.username} onclick={() => (author = author === a.username ? null : a.username)}>{a.name} {a.count}</button>{/each}
      {/if}
    </div>
  {/if}
  {#if branches.length}
    <div class="strip">
      {#each branches as b (b.id)}<div class="br"><span class="mono">{branchName(b)}</span><PipelineBar pipeline={b} status={b.status} {now} /></div>{/each}
    </div>
  {/if}
  {#if shown.length > 1}
    <label class="sort">{t('Sorted by')}
      <select value={sort} onchange={(e) => setSort(e.currentTarget.value as Sort)}>
        <option value="updated">{t('Last update')}</option>
        <option value="oldest">{t('Oldest first')}</option>
        <option value="repo">{t('Repository')}</option>
        {#if linked.chains.length}<option value="chains">{t('Chains')}</option>{/if}
      </select>
    </label>
  {/if}
  {#if byChain}
    {#each linked.chains as c, i (i)}
      <div class="grp">{t('Chain')} · {c.mrs.length}</div>
      {#each tree(c) as r (mrKey(r.mr))}<div class="nest" style:--d={r.depth}>{@render row(r.mr)}</div>{/each}
    {/each}
    {#if linked.alone.length}
      <div class="grp">{t('No dependency')} · {linked.alone.length}</div>
      {#each linked.alone as m (mrKey(m))}{@render row(m)}{/each}
    {/if}
  {:else}
    {#each listed as m (mrKey(m))}{@render row(m)}{/each}
  {/if}
  {#if !shown.length}<State kind="empty" title={tab === 'mine' ? t('No open MR of yours.') : t('No open MR.')} />{/if}
  {/if}
</div>

<style>
  .mrs { padding-bottom: 12px; }
  .mrs > .seg { display: flex; margin: 10px 14px 6px; }
  .mrs > .seg > button { flex: 1; justify-content: center; height: var(--h-md); font-size: var(--fs-body); }
  .filters { display: flex; gap: 6px; padding: 4px 14px 8px; overflow-x: auto; scrollbar-width: none; }
  /* Filters: round, the one picked in the accent. */
  .filters .btn { border-radius: 999px; background: none; color: var(--text-muted); font-weight: 400; }
  .filters .btn[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); }
  .filters .sep { flex: none; width: 1px; margin: 4px 2px; background: var(--border); }
  .strip { display: flex; gap: 8px; margin: 0 14px 4px; overflow-x: auto; scrollbar-width: none; }
  .br { flex: none; display: flex; align-items: center; gap: 8px; min-width: 200px; padding: 6px 10px; border-radius: var(--r8); background: var(--surface); font-size: var(--fs-tag); }
  .br .mono { font-family: var(--mono); white-space: nowrap; }
  .br :global(> :last-child) { flex: 1; min-width: 0; }
  .grp { display: flex; align-items: center; gap: 5px; padding: 14px 14px 6px; color: var(--text-muted); font-size: var(--fs-micro); letter-spacing: 0.05em; text-transform: uppercase; }
  .grp .hint { margin-left: auto; letter-spacing: 0; text-transform: none; opacity: 0.8; }
  .branch { display: flex; align-items: center; gap: 5px; margin-top: 10px; padding: 8px 14px; border-top: 1px solid var(--border); font: 600 13px var(--mono); }
  .branch b { margin-left: auto; color: var(--text-muted); font-family: inherit; }
  .pos { flex: none; align-self: center; width: 16px; color: var(--text-muted); font: 600 13px var(--mono); text-align: center; }
  .pos.head { color: var(--accent); }
  .sort { display: flex; align-items: center; justify-content: flex-end; gap: 6px; padding: 8px 14px 4px; color: var(--text-muted); font-size: var(--fs-tag); }
  .sort select { padding: 2px 9px; border: 0; border-radius: var(--r10); background: var(--surface-muted); color: var(--text); font: inherit; }
  .item { position: relative; display: flex; gap: 10px; padding: 11px 14px; border-bottom: 1px solid var(--border); --c: var(--text-muted); }
  .item:active { background: var(--surface); }
  .item.me::before { position: absolute; top: 8px; bottom: 8px; left: 0; width: 3px; border-radius: 0 var(--r3) var(--r3) 0; background: var(--tone, var(--accent)); content: ''; }
  .item.me .act:not(.armed) { background: color-mix(in srgb, var(--tone, var(--accent)) 22%, transparent); }
  .item.blocked { --tone: var(--ko); } .item.todo { --tone: var(--warn); } .item.wait { --tone: var(--info); } .item.ready { --tone: var(--ok); } .item.auto { --tone: var(--accent); }
  .av { display: grid; flex: none; place-items: center; width: 28px; height: 28px; border-radius: var(--r-avatar); background: var(--c); color: #fff; font-size: var(--fs-micro); font-weight: 700; }
  .av.pic { overflow: hidden; }
  .av.pic img { width: 100%; height: 100%; object-fit: cover; }
  .body { flex: 1; min-width: 0; }
  .h { display: flex; align-items: center; gap: 6px; min-width: 0; color: var(--text-muted); font-size: var(--fs-tag); }
  .h .iid { flex: none; color: var(--info); font-family: var(--mono); }
  .prio { flex: none; padding: 0 5px; border-radius: var(--r4); background: color-mix(in srgb, var(--warn) 16%, transparent); color: color-mix(in srgb, var(--warn) 70%, var(--text)); font: 600 11px var(--mono); }
  .h .who { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grow { flex: 1; }
  .repo { flex: none; padding: 0 5px; border-radius: var(--r4); background: color-mix(in srgb, var(--c) 16%, transparent); color: var(--c); font-size: var(--fs-micro); font-weight: 600; }
  .step { flex: none; padding: 0 8px; border-radius: var(--r10); background: color-mix(in srgb, var(--tone, var(--text-muted)) 15%, transparent); color: color-mix(in srgb, var(--tone, var(--text-muted)) 70%, var(--text)); font-size: var(--fs-micro); font-weight: 500; line-height: 19px; white-space: nowrap; }
  .title { display: -webkit-box; margin: 3px 0 6px; overflow: hidden; font-size: var(--fs-title); font-weight: 550; line-height: 1.3; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; }
  .meta { display: flex; align-items: center; gap: 10px; min-width: 0; overflow: hidden; color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .meta > span { display: inline-flex; flex: none; align-items: center; gap: 3px; }
  .meta .pipe { flex: 0 1 140px; min-width: 70px; }
  .meta .tk { color: var(--info); font-family: var(--mono); }
  .meta .ok { color: var(--ok); }
  .nest { padding-left: calc(var(--d) * 14px); }
  .held { display: flex; padding: 11px 14px; border-bottom: 1px solid var(--border); }
  .held .acts { display: flex; gap: 8px; }
  .act { align-self: center; height: var(--h-md); border-radius: 999px; }
  .act:not(.armed) { border-color: color-mix(in srgb, var(--tone, var(--accent)) 70%, var(--border)); background: none; }
</style>
