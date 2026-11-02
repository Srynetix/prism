<script lang="ts">
  import { t } from '../i18n.svelte'
  import { clock } from '../clock.svelte'
  import { forgeLogo, meOn } from '../forge'
  import BlockControls from './BlockControls.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import MrRow from './MrRow.svelte'
  import PipelineBar from './PipelineBar.svelte'
  import type { MergeRequest, Snapshot, Ticket } from '../types'
  import { nextStep } from '../rules'
  import { mrView, openMr } from '../mr-view.svelte'
  import MrTitle from './MrTitle.svelte'
  import { byTarget, mergePriority, mergeQueue, queueCount, queueRepos, type MergeQueue } from '../merge-queue'
  import { repoColor } from '../people'
  import { primaryRepo } from '../repositories'
  import { mrKey, sameMr } from '../mr-ref'

  /**
   * Merge queue: the MRs of one repository waiting to be merged. With a merge queue, those with its
   * label in the order its CI job takes them (by priority label when there are some, the oldest
   * first), then those the forge merges by
   * itself (auto-merge), then the approved ones nothing will merge; split by target branch when they
   * go to several. The repository is remembered (`queue.repo`, shared with the phone).
   */
  let { snap }: { snap: Snapshot } = $props()

  const repos = $derived(queueRepos(snap))
  let chosen = $state<string | null>(pref<string | null>('queue.repo', null))
  const setRepo = (r: string) => setPref('queue.repo', (chosen = r))
  // A repository gone from the list (no queue, nothing waiting): the first one.
  const repo = $derived(chosen && repos.includes(chosen) ? chosen : (repos[0] ?? primaryRepo(snap)?.name ?? ''))
  const queues = $derived(new Map(repos.map((r) => [r, mergeQueue(snap, r)])))
  const q = $derived(queues.get(repo) ?? mergeQueue(snap, repo))
  const queueLabel = $derived(snap.merge_queue_label)
  const prefix = $derived(snap.merge_priority_prefix)
  const branches = $derived(byTarget(q))
  /** Several target branches: one group each, numbered on its own. */
  const split = $derived(branches.length > 1)

  const tickets = $derived(new Map<string, Ticket>([...(snap.sprint?.tickets ?? []), ...(snap.pickup?.boards.flatMap((b) => b.tickets) ?? [])].map((t) => [t.id, t])))
  const now = $derived(clock(1000).now)
  /** In the queue but held up: the job will not merge it as it is. */
  const stuck = (m: MergeRequest) => nextStep(m, meOn(snap, m), queueLabel).tone === 'blocked'
  const stuckCount = $derived(q.queued.filter(stuck).length)
</script>

{#snippet sections(b: MergeQueue)}
  {#if b.hasQueue}
    <div class="section"><Icon name="queue" /> {t('In the queue')} · {b.queued.length}<span class="hint">{prefix ? t('by priority, then oldest first: the order the job takes them') : t('oldest first: the order the job takes them')}</span></div>
    {#each b.queued as m, i (mrKey(m))}{@render row(m, i + 1)}{:else}<div class="empty">{t('No MR with the “{label}” label.', { label: queueLabel ?? '' })}</div>{/each}
  {/if}
  {#if b.auto.length}
    <div class="section"><Icon name="automerge" /> {b.hasQueue ? t('Auto-merge, out of the queue') : t('Auto-merge')} · {b.auto.length}<span class="hint">{t('merged by the forge once green')}</span></div>
    {#each b.auto as m (mrKey(m))}{@render row(m)}{/each}
  {/if}
  {#if b.ready.length}
    <div class="section"><Icon name="check" /> {b.hasQueue ? t('Approved, not in the queue') : t('Approved, auto-merge off')} · {b.ready.length}<span class="hint">{t('nothing will merge them')}</span></div>
    {#each b.ready as m (mrKey(m))}{@render row(m)}{/each}
  {/if}
{/snippet}

{#snippet row(m: MergeRequest, pos?: number)}
  {@const prio = pos ? mergePriority(m, prefix) : null}
  <div class="line">
    <span class="pos" class:head={pos === 1} title={pos === 1 ? (prefix ? t('Next: the one the job takes') : t('Next: the oldest MR with the label, the one the job takes')) : pos ? t('Number {pos} in the queue', { pos }) : undefined}>{pos === 1 ? '▶' : (pos ?? '')}</span>
    {#if prio !== null}<span class="prio" title={t('Merge priority {n} (label {label})', { n: prio, label: `${prefix}${prio}` })}>P{prio}</span>{/if}
    <div class="grow"><MrRow mr={m} me={meOn(snap, m)} {queueLabel} ticket={m.ticket ? tickets.get(m.ticket) : undefined} {now} selected={!!mrView.ref && sameMr(mrView.ref, m)} onopen={() => openMr(m)} /></div>
  </div>
{/snippet}

<section class="panel" style="--src: var(--src-gitlab)" class:collapsed={isCollapsed('queue')}>
  <h2>
    <span class="title"><Logo name={forgeLogo()} />{t('Merge queue')}</span>
    <span class="head">
      {#if q.hasQueue}<small class="pill auto" title={t('MRs with the “{label}” label', { label: queueLabel ?? '' })}>{t('{n} queued', { n: q.queued.length })}</small>{/if}
      {#if stuckCount}<small class="pill ko" title={t('In the queue, but conflicts, a failed rebase or a failed CI hold them up')}>{t('{n} stuck', { n: stuckCount })}</small>{/if}
      {#if q.auto.length}<small class="pill auto" title={t('Auto-merge on: the forge merges them once their checks pass')}>{t('{n} auto-merge', { n: q.auto.length })}</small>{/if}
    </span>
    <BlockControls id="queue" />
  </h2>
  {#if repos.length > 1 && !isShort('queue')}
    <div class="repos" role="group" aria-label={t('Repositories')}>
      {#each repos as r (r)}
        {@const n = queueCount(queues.get(r)!)}
        <button class="rp" class:on={repo === r} class:zero={!n} style:--c={repoColor(r)} onclick={() => setRepo(r)} title={t('The merge queue of {repo}', { repo: r })}><i></i>{r} <b>{n}</b></button>
      {/each}
    </div>
  {/if}
  {#if isShort('queue')}
    <div class="short-body">
      {#each branches.flatMap((b) => (q.queued.length ? b.queued : b.auto).map((m, i) => ({ m, i }))) as { m, i } (mrKey(m))}
        {@const st = nextStep(m, meOn(snap, m), queueLabel)}
        {@const prio = q.queued.length ? mergePriority(m, prefix) : null}
        <div class="srow">
          <span class="pos" class:head={q.queued.length > 0 && i === 0}>{q.queued.length ? (i === 0 ? '▶' : i + 1) : ''}</span>
          {#if prio !== null}<span class="prio" title={t('Merge priority {n} (label {label})', { n: prio, label: `${prefix}${prio}` })}>P{prio}</span>{/if}
          <span class="am" title={m.auto_merge ? t('Auto-merge on: the forge merges it once its checks pass') : undefined}>{#if m.auto_merge}<Icon name="automerge" />{/if}</span>
          {#if split}<span class="target" title={t('Target branch')}>→ {m.target_branch}</span>{/if}
          <button class="iid" onclick={() => openMr(m)}>{mrKey(m)}</button>
          <span class="t" title={m.title}><MrTitle mr={m} /></span>
          <span class="pipe-s"><PipelineBar pipeline={m.head_pipeline} status={m.pipeline} {now} /></span>
          <span class="next {st.tone}">{st.label}</span>
        </div>
      {:else}
        <span class="more">{t('Nothing waiting to merge in {repo}.', { repo })}</span>
      {/each}
    </div>
  {:else}
    <div class="scroll">
      {#if split}
        {#each branches as b (b.target)}
          <div class="branch"><Icon name="branch" /> → {b.target}<b>{queueCount(b)}</b></div>
          {@render sections(b)}
        {/each}
      {:else}
        {@render sections(q)}
      {/if}
      {#if !q.hasQueue && !q.auto.length && !q.ready.length}
        <div class="empty">{repo ? t('Nothing waiting to merge in {repo}.', { repo }) : t('No repository with a merge queue or an MR waiting to merge.')}</div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .head { display: flex; gap: 6px; align-items: baseline; }
  .pill { font-size: var(--fs-tag); font-weight: 500; padding: 0 7px; border-radius: var(--r10); --c: var(--text-muted); background: color-mix(in srgb, var(--c) 15%, transparent); color: color-mix(in srgb, var(--c) 70%, var(--text)); }
  .pill.auto { --c: var(--accent); } .pill.ko { --c: var(--ko); }
  .repos { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; padding: 5px 10px; border-bottom: 1px solid var(--border); }
  .repos .rp { --c: var(--text-muted); display: inline-flex; align-items: center; gap: 5px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); line-height: 19px; cursor: pointer; }
  .repos .rp b { color: var(--text); font-weight: 600; }
  .repos .rp i { width: 7px; height: 7px; border-radius: 50%; background: var(--c); }
  .repos .rp.on { border-color: var(--c); background: color-mix(in srgb, var(--c) 14%, var(--surface)); color: var(--c); }
  .repos .rp.on b { color: var(--c); }
  .repos .rp.zero { opacity: 0.5; }
  .section { display: flex; align-items: center; gap: 5px; padding: 3px 10px; border-bottom: 1px solid var(--border); background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); }
  .section .hint { margin-left: auto; opacity: 0.8; }
  .branch { display: flex; align-items: center; gap: 5px; padding: 4px 10px; border-bottom: 1px solid var(--border); font: 600 var(--fs-meta) var(--mono); color: var(--text); }
  .branch b { margin-left: auto; font-family: inherit; color: var(--text-muted); }
  .target { flex-shrink: 0; font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); }
  .line { display: flex; align-items: stretch; }
  .line .grow { flex: 1; min-width: 0; }
  .pos { flex-shrink: 0; display: inline-flex; align-items: center; justify-content: center; width: 22px; font: 600 var(--fs-meta) var(--mono); color: var(--text-muted); }
  .pos.head { color: var(--accent); }
  .prio { flex-shrink: 0; align-self: center; padding: 0 5px; border-radius: var(--r10); font: 600 var(--fs-tag) var(--mono); line-height: 16px; background: color-mix(in srgb, var(--warn) 15%, transparent); color: color-mix(in srgb, var(--warn) 70%, var(--text)); }
  .srow .pos { width: 16px; }
  .am { flex-shrink: 0; width: 13px; display: inline-flex; color: var(--accent); }
  .iid { border: none; background: none; padding: 0; font-family: var(--mono); font-size: var(--fs-meta); color: var(--info); cursor: pointer; }
  .pipe-s { width: 150px; flex-shrink: 0; overflow: hidden; }
  .next { flex-shrink: 0; padding: 0 7px; border-radius: var(--r10); font-size: var(--fs-tag); font-weight: 500; line-height: 18px; --c: var(--text-muted); background: color-mix(in srgb, var(--c) 15%, transparent); color: color-mix(in srgb, var(--c) 70%, var(--text)); }
  .next.blocked { --c: var(--ko); } .next.todo { --c: var(--warn); } .next.wait { --c: var(--info); } .next.ready { --c: var(--ok); } .next.auto { --c: var(--accent); }
</style>
