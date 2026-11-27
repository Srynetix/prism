<script lang="ts">
  import { t, tn, tc } from '../i18n.svelte'
  import { clock } from '../clock.svelte'
  import { locale } from '../locale'
  import { forgeLogo, forgeName, meOn, onGithub } from '../forge'
  import { branchName, pipelineProject } from '../pipeline'
  import BlockControls from './BlockControls.svelte'
  import State from './ui/State.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import type { MergeRequest, Snapshot, Ticket } from '../types'
  import { awaitsMyReview, mergedOn, nextStep, openMrs, securityLabel } from '../rules'
  import PipelineBar from './PipelineBar.svelte'
  import CancelPipeline from './CancelPipeline.svelte'
  import RetryPipeline from './RetryPipeline.svelte'
  import MrRow from './MrRow.svelte'
  import MrGraph from './MrGraph.svelte'
  import { chains, tree, viaLabel, type DepFilter } from '../mr-deps'
  import { mrView, openMr } from '../mr-view.svelte'
  import MrTitle from './MrTitle.svelte'
  import { ago } from '../time'
  import { canShowLogs, logsClick } from '../logs.svelte'
  import { selectedDay, todayKey } from '../filter.svelte'
  import { firstName, forgeAvatar, initialsOf, personColor, repoColor } from '../people'
  import { mrKey, sameMr } from '../mr-ref'
  import { keys } from '../platform'

  /**
   * Merge requests: one compact row per MR (title, context, pipeline, next step and
   * the unblocking action), most recently updated first; details open in a drawer. Tabs Team /
   * Mine / To review, main and release pipelines banner, MRs merged on the day viewed.
   */
  let { snap }: { snap: Snapshot } = $props()
  type Filter = 'team' | 'mine' | 'review'
  let filter = $state<Filter>(pref<Filter>('mrs.filter', 'team'))
  function setFilter(f: Filter) {
    filter = f
    setPref('mrs.filter', f)
  }

  const queueLabel = $derived(snap.merge_queue_label)
  const step = (m: MergeRequest) => nextStep(m, meOn(snap, m), queueLabel)
  const isMine = (m: MergeRequest) => m.author === meOn(snap, m)
  /** Most recently updated first; the next step, in colour, says what is urgent. */
  const all = $derived(openMrs(snap).sort((a, b) => b.updated_at.localeCompare(a.updated_at)))
  const mine = $derived(all.filter(isMine))
  const toReview = $derived(all.filter((m) => awaitsMyReview(m, meOn(snap, m))))
  /**
   * Authors of the team's MRs, most active first: a click shows only one author's MRs
   * (open and merged), a second click goes back to all.
   */
  let author = $state<string | null>(null)
  const authors = $derived.by(() => {
    const by = new Map<string, { username: string; name: string; avatar: string | null | undefined; count: number }>()
    for (const m of all) {
      const a = by.get(m.author) ?? { username: m.author, name: m.author_name || m.author, avatar: m.author_avatar, count: 0 }
      a.count++
      by.set(m.author, a)
    }
    return [...by.values()].sort((a, b) => b.count - a.count || a.name.localeCompare(b.name))
  })
  // No open MR left from the filtered author: the filter is lifted.
  $effect(() => {
    if (author && !authors.some((a) => a.username === author)) author = null
  })
  const byAuthor = (m: MergeRequest) => filter !== 'team' || !author || m.author === author
  /**
   * Repos: the primary repository (`''`) and the others whose MRs Prism follows (`[[repos]]
   * merge_requests`). A click shows only one repo's MRs, "All" goes back to all;
   * choice remembered (`mrs.repo`). The row only appears with MRs from another repository.
   */
  let repo = $state<string | null>(pref<string | null>('mrs.repo', null))
  const setRepo = (r: string | null) => setPref('mrs.repo', (repo = r))
  const mainName = $derived(snap.repos?.[0]?.name ?? 'main')
  const listed = $derived((filter === 'team' ? all : filter === 'mine' ? mine : toReview).filter(byAuthor))
  const repoNames = $derived([...new Set(snap.merge_requests.map((m) => m.repo).filter((r): r is string => !!r))].sort())
  // No MR left from the filtered repo: the filter is lifted.
  $effect(() => {
    if (repo && !repoNames.includes(repo)) setRepo(null)
  })
  const byRepo = (m: MergeRequest) => repo === null || (m.repo ?? '') === repo
  const shown = $derived(listed.filter(byRepo))
  /**
   * The dependencies between the MRs listed (`mr_deps`): with at least one, the list may show
   * them as chains (each MR under the one it waits for) or as a graph (`mrs.view`); the graph's
   * filters (`mrs.graph`): the dependencies via tickets, the merged MRs still waited on.
   */
  type View = 'list' | 'chains' | 'graph'
  let view = $state<View>(pref<View>('mrs.view', 'list'))
  const setView = (v: View) => setPref('mrs.view', (view = v))
  let depFilter = $state<DepFilter>(pref<DepFilter>('mrs.graph', { tickets: true, merged: true }))
  const setDepFilter = (f: Partial<DepFilter>) => setPref('mrs.graph', (depFilter = { ...depFilter, ...f }))
  const linked = $derived(chains(snap, shown, depFilter))
  const hasDeps = $derived(chains(snap, shown, { tickets: true, merged: true }).chains.length > 0)
  const shownView = $derived<View>(hasDeps ? view : 'list')

  /** Short: my open MRs and those awaiting my review, one row each. */
  const concerning = $derived(all.filter((m) => isMine(m) || awaitsMyReview(m, meOn(snap, m))))
  const day = $derived(selectedDay())
  const merged = $derived(mergedOn(snap, day).filter((m) => (filter === 'team' || (filter === 'mine' && isMine(m))) && byAuthor(m) && byRepo(m)))
  /** Collapsible merged MRs section, state remembered. */
  let mergedOpen = $state(pref<boolean>('mrs.mergedOpen', true))
  function toggleMerged() {
    mergedOpen = !mergedOpen
    setPref('mrs.mergedOpen', mergedOpen)
  }
  const mergedLabel = $derived(
    day === todayKey() ? t('Merged today') : t('Merged on {date}', { date: new Date(`${day}T12:00`).toLocaleDateString(locale(), { weekday: 'short', day: 'numeric', month: 'short' }) }),
  )
  /** The server only keeps 7 days of merged MRs. */
  const beyondHistory = $derived(Date.now() - new Date(`${day}T00:00`).getTime() > 7 * 86400000)

  const tickets = $derived(new Map<string, Ticket>([...(snap.sprint?.tickets ?? []), ...(snap.pickup?.boards.flatMap((b) => b.tickets) ?? [])].map((t) => [t.id, t])))

  /** Clock for running stages, whose duration grows. */
  const now = $derived(clock(1000).now)

  /** Followed branches (main, release, or `pipeline_branches`): the only MR-less pipelines that matter to the whole team, the primary repository's first, each repository's in its list's order. */
  const branches = $derived([...snap.pipelines].sort((a, b) => +!!a.repo - +!!b.repo || (a.repo ?? '').localeCompare(b.repo ?? '')))
</script>

{#snippet row(m: MergeRequest)}
  <MrRow mr={m} me={meOn(snap, m)} {queueLabel} ticket={m.ticket ? tickets.get(m.ticket) : undefined} {now} selected={!!mrView.ref && sameMr(mrView.ref, m)} onopen={() => openMr(m)} />
{/snippet}

<section class="panel" style="--src: var(--src-gitlab)" class:collapsed={isCollapsed('mrs')}>
  <h2>
    <span class="title"><Logo name={forgeLogo()} />{onGithub() ? t('Pull requests') : t('Merge requests')}</span>
    <span class="seg">
      <button class:on={filter === 'team'} onclick={() => setFilter('team')}>{t('Team')} {all.length}</button>
      <button class:on={filter === 'mine'} onclick={() => setFilter('mine')}>{tc('mrs', 'Mine')} {mine.length}</button>
      <button class:on={filter === 'review'} onclick={() => setFilter('review')}>{t('To review')} {toReview.length}</button>
    </span>
    {#if hasDeps && !isShort('mrs')}
      <span class="seg views" role="group" aria-label={t('Dependencies')}>
        <button class:on={shownView === 'list'} onclick={() => setView('list')} title={t('One row per MR')}>{t('List')}</button>
        <button class:on={shownView === 'chains'} onclick={() => setView('chains')} title={t('Each MR under the one it waits for')}>{t('Chains')} {linked.chains.length}</button>
        <button class:on={shownView === 'graph'} onclick={() => setView('graph')} title={t('The dependencies between MRs, in the order they can merge')}>{t('Graph')}</button>
      </span>
    {/if}
    <BlockControls id="mrs" />
  </h2>
  {#if branches.length}
    <div class="branches">
      {#each branches as p (p.id)}
        <span class="branch">
          <a href={p.url} target="_blank" rel="noreferrer" onclick={logsClick(p.url, branchName(p), pipelineProject(p))} title={canShowLogs() ? keys(t('{branch} pipeline · click: logs, ⌘-click: {forge}', { branch: branchName(p), forge: forgeName() })) : undefined}>
            <span class="mono">{branchName(p)}</span>
            <PipelineBar pipeline={p} {now} />
            <span class="mono muted">{ago(p.updated_at)}</span>
          </a>
          <CancelPipeline id={p.id} status={p.status} />
          <RetryPipeline id={p.id} status={p.status} />
        </span>
      {/each}
    </div>
  {/if}
  {#if filter === 'team' && authors.length > 1 && !isShort('mrs')}
    <div class="authors" role="group" aria-label={t('Authors')}>
      {#each authors as a (a.username)}
        {@const pic = forgeAvatar(a.avatar)}
        <button class="who" class:on={author === a.username} class:dim={!!author && author !== a.username} onclick={() => (author = author === a.username ? null : a.username)} title="{a.name} · @{a.username} · {tn(a.count, '{n} open MR', '{n} open MRs')} · {author === a.username ? t('click: show everyone') : t('click: only theirs')}">
          <span class="av" class:pic={!!pic} style:--c={personColor(a.username)}>{#if pic}<img src={pic} alt="" />{:else}{initialsOf(a.name, a.username)}{/if}</span>
          <span class="nm">{a.username === snap.me || a.username === snap.me_elsewhere ? t('You') : firstName(a.name, a.username)}</span><b>{a.count}</b>
        </button>
      {/each}
    </div>
  {/if}
  {#if repoNames.length && !isShort('mrs')}
    <div class="repos" role="group" aria-label={t('Repositories')}>
      <span class="lab">{t('Repos')}</span>
      <button class="rp" class:on={repo === null} onclick={() => setRepo(null)}>{tc('mrs', 'All')} <b>{listed.length}</b></button>
      {#each [{ key: '', name: mainName }, ...repoNames.map((r) => ({ key: r, name: r }))] as r (r.key)}
        {@const n = listed.filter((m) => (m.repo ?? '') === r.key).length}
        <button class="rp" class:on={repo === r.key} class:zero={!n} style:--c={r.key ? repoColor(r.key) : 'var(--src-gitlab)'} onclick={() => setRepo(repo === r.key ? null : r.key)} title={repo === r.key ? t('Click: every repository') : t('Only the MRs of {repo}', { repo: r.name })}><i></i>{r.name} <b>{n}</b></button>
      {/each}
    </div>
  {/if}
  {#if isShort('mrs')}
    <div class="short-body">
      {#each concerning as m (mrKey(m))}
        {@const st = step(m)}
        <div class="srow">
          <button class="iid" onclick={() => openMr(m)}>{mrKey(m)}</button>
          <span class="t" title={m.title}>{#if securityLabel(m)}<span class="sec" title={t('Security MR')}><Icon name="security" /></span>{/if}<MrTitle mr={m} /></span>
          <span class="pipe-s"><PipelineBar pipeline={m.head_pipeline} status={m.pipeline} {now} /></span>
          <span class="next {st.tone}">{st.label}</span>
        </div>
      {:else}
        <span class="more">{t('Nothing of mine and no review waiting.')}</span>
      {/each}
    </div>
  {:else}
    <div class="scroll">
      {#if shownView === 'graph'}
        <div class="graph-bar">
          <label><input type="checkbox" checked={depFilter.merged} onchange={(e) => setDepFilter({ merged: e.currentTarget.checked })} /> {t('recently merged')}</label>
          <label><input type="checkbox" checked={depFilter.tickets} onchange={(e) => setDepFilter({ tickets: e.currentTarget.checked })} /> {t('via tickets')}</label>
          <span class="legend"><i class="ln stacked"></i>{viaLabel('stacked')} <i class="ln declared"></i>{viaLabel('declared')} <i class="ln ticket"></i>{viaLabel('ticket')}</span>
        </div>
        <MrGraph mrs={shown} filter={depFilter} />
        {#if linked.alone.length}
          <div class="section">{t('No dependency')} · {linked.alone.length}</div>
          {#each linked.alone as m (mrKey(m))}{@render row(m)}{/each}
        {/if}
      {:else if shownView === 'chains'}
        {#each linked.chains as c, i (i)}
          <div class="section">{t('Chain')} · {tn(c.mrs.length, '{n} MR', '{n} MRs')}</div>
          {#each tree(c) as r (mrKey(r.mr))}
            <div class="nest" style:--d={r.depth} class:child={r.depth > 0} title={r.dep ? viaLabel(r.dep.via) : undefined}>{@render row(r.mr)}</div>
          {/each}
        {/each}
        {#if linked.alone.length}
          <div class="section">{t('No dependency')} · {linked.alone.length}</div>
          {#each linked.alone as m (mrKey(m))}{@render row(m)}{/each}
        {/if}
      {:else}
        {#each shown as m (mrKey(m))}
          {@render row(m)}
        {:else}
          <State kind="empty" icon="mr" title={t('No merge request here.')} />
        {/each}
      {/if}
      {#if beyondHistory}
        <div class="section">{mergedLabel} · {t('merged MRs are only kept for the last 7 days')}</div>
      {:else if merged.length}
        <button class="section fold" onclick={toggleMerged} aria-expanded={mergedOpen} title={mergedOpen ? t('Collapse merged MRs') : t('Show merged MRs')}>
          <span class="chev" class:down={mergedOpen}>›</span>{mergedLabel} · {merged.length}
        </button>
        {#if mergedOpen}
          {#each merged as m (mrKey(m))}{@render row(m)}{/each}
        {/if}
      {/if}
    </div>
  {/if}
</section>

<style>
  /* Team authors: who opened what, and a one-click filter. */
  .repos { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; padding: 5px 10px; border-bottom: 1px solid var(--border); }
  .repos .lab { margin-right: 2px; font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.3px; }
  .repos .rp { --c: var(--text-muted); display: inline-flex; align-items: center; gap: 5px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); line-height: 19px; cursor: pointer; }
  .repos .rp b { color: var(--text); font-weight: 600; }
  .repos .rp i { width: 7px; height: 7px; border-radius: 50%; background: var(--c); }
  .repos .rp.on { border-color: var(--c); background: color-mix(in srgb, var(--c) 14%, var(--surface)); color: var(--c); }
  .repos .rp.on b { color: var(--c); }
  .repos .rp.zero { opacity: 0.5; }
  .authors { display: flex; flex-wrap: wrap; gap: 4px 6px; padding: 5px 10px; border-bottom: 1px solid var(--border); }
  .who { display: inline-flex; align-items: center; gap: 4px; padding: 1px 7px 1px 2px; border: 1px solid var(--border); border-radius: var(--r12); background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .who:hover { border-color: color-mix(in srgb, var(--src-gitlab) 50%, var(--border)); }
  .who.on { border-color: var(--src-gitlab); background: color-mix(in srgb, var(--src-gitlab) 12%, transparent); }
  .who.dim { opacity: 0.5; }
  .who b { color: var(--text-muted); font-weight: 600; font-variant-numeric: tabular-nums; }
  .who .av { display: inline-flex; align-items: center; justify-content: center; width: 18px; height: 18px; border-radius: var(--r-avatar); background: var(--c); color: #fff; font-size: var(--fs-micro); font-weight: 700; }
  .who .av.pic { overflow: hidden; }
  .who .av.pic img { width: 100%; height: 100%; object-fit: cover; }
  .muted { color: var(--text-muted); }
  .mono { font-family: var(--mono); }
  .branches { display: flex; gap: 6px 18px; flex-wrap: wrap; padding: 5px 12px; border-bottom: 1px solid var(--border); background: var(--surface-muted); }
  .branch { display: flex; align-items: center; gap: 4px; font-size: var(--fs-body); min-width: 0; max-width: 100%; }
  .branch > a { display: flex; align-items: center; gap: 8px; min-width: 0; color: var(--text); }
  .branch > a:hover { text-decoration: none; opacity: 0.85; }
  .section { display: flex; align-items: center; gap: 5px; width: 100%; padding: 3px 10px; border: none; border-bottom: 1px solid var(--border); background: var(--surface-muted); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); text-align: left; }
  .fold { cursor: pointer; }
  .fold:hover { color: var(--text); }
  .chev { display: inline-flex; align-items: center; justify-content: center; width: 10px; height: 10px; line-height: 1; transition: transform 0.15s; }
  .chev.down { transform: rotate(90deg); }
  /* Chains: each MR under the one it waits for, a guide on its left. */
  .nest.child { padding-left: calc(var(--d) * 16px); background: linear-gradient(90deg, transparent calc(var(--d) * 16px - 9px), var(--border) calc(var(--d) * 16px - 9px), var(--border) calc(var(--d) * 16px - 8px), transparent calc(var(--d) * 16px - 8px)); }
  .graph-bar { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 14px; padding: 5px 10px; border-bottom: 1px solid var(--border); font-size: var(--fs-meta); color: var(--text-muted); }
  .graph-bar label { display: inline-flex; align-items: center; gap: 4px; cursor: pointer; }
  .legend { display: inline-flex; align-items: center; gap: 5px; margin-left: auto; }
  .ln { display: inline-block; width: 22px; height: 0; border-top: 2px solid var(--text); margin-left: 6px; }
  .ln.declared { border-top: 2px dashed var(--info); }
  .ln.ticket { border-top: 2px dotted var(--text-muted); }
  /* Short: one row per MR, the step on the right. */
  .iid { border: none; background: none; padding: 0; font-family: var(--mono); font-size: var(--fs-meta); color: var(--info); cursor: pointer; }
  .sec { color: var(--ko); margin-right: 4px; }
  .pipe-s { width: 150px; flex-shrink: 0; overflow: hidden; }
  .next { flex-shrink: 0; padding: 0 7px; border-radius: var(--r10); font-size: var(--fs-tag); font-weight: 500; line-height: 18px; --c: var(--text-muted); background: color-mix(in srgb, var(--c) 15%, transparent); color: color-mix(in srgb, var(--c) 70%, var(--text)); }
  .next.blocked { --c: var(--ko); } .next.todo { --c: var(--warn); } .next.wait { --c: var(--info); } .next.ready { --c: var(--ok); } .next.auto { --c: var(--accent); }
</style>
