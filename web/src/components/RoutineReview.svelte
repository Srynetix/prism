<script lang="ts">
  import { live } from '../live.svelte'
  import { openMr } from '../mr-view.svelte'
  import { openSettings } from '../settings.svelte'
  import { isDesktop } from '../bridge'
  import { ago } from '../time'
  import { t, tn } from '../i18n.svelte'
  import { archiveRuns, nextTime, openRunAgent, rerunRun, reviewPile, routinesOf, runMr, runSpan, STATE_ICON, STATE_LABEL } from '../routines.svelte'
  import type { RoutineRun } from '../bindings/RoutineRun'

  /**
   * "To review": the routines' runs that wait for me, one row each — the routine, its repository,
   * what its agent said in a line, what it produced (its MR, branch, files, cost), and the
   * gestures: Reply (blocked) or Open, the MR, Archive, Rerun (failed). Blocked ones first. Under
   * them, the next times (`next`), each a link to Settings › Routines. `compact`: the Agents list's
   * narrow column. `rerun`: Rerun asked of a phone, its passkey (`onrerun`).
   */
  let { compact = false, next = true, onrerun }: { compact?: boolean; next?: boolean; onrerun?: (run: RoutineRun) => void } = $props()

  const routines = $derived(routinesOf(live.snapshot))
  const pile = $derived(reviewPile(live.snapshot))
  const ready = $derived(pile.filter((r) => r.state === 'ready'))
  const upcoming = $derived(
    routines.routines
      .filter((r) => r.next.length)
      .map((r) => ({ name: r.name, at: r.next[0], digest: live.snapshot?.agents_settings?.routines.find((x) => x.name === r.name)?.deliver === 'digest', repos: r.repos.length }))
      .sort((a, b) => a.at.localeCompare(b.at))
      .slice(0, compact ? 3 : 5),
  )
  const off = $derived(routines.routines.filter((r) => !r.enabled || r.pending || r.error))
  const rerun = (r: RoutineRun) => (onrerun ? onrerun(r) : void rerunRun(r.id))
</script>

{#if pile.length || (next && (upcoming.length || off.length))}
  <div class="review" class:compact aria-label={t('To review')}>
    <div class="hd">
      <b>{t('To review')}</b>
      <span class="muted">{t('routines')}{#if pile.length} · {pile.length}{/if}</span>
      <span class="grow"></span>
      {#if ready.length > 1}<button class="q" onclick={() => archiveRuns(ready.map((r) => r.id))} title={t('Out of the pile; their agents stay in the archive')}>{t('Archive the ready ones')}</button>{/if}
    </div>
    {#each pile as r (r.id)}
      {@const mr = runMr(live.snapshot, r)}
      {@const agentThere = !!r.agent && !!live.snapshot?.agents?.some((a) => a.id === r.agent)}
      <div class="row {r.state}">
        <span class="ic" aria-hidden="true">{STATE_ICON[r.state]}</span>
        <div class="body">
          <div class="l1">
            <b class="name">{r.routine}</b>
            <span class="repo">{r.repo ?? t('no repository')}</span>
            <span class="st {r.state}">{STATE_LABEL[r.state]}</span>
            {#if r.caught_up}<span class="tag" title={t('Its time passed while the computer slept: run once on waking')}>{t('caught up')}</span>{/if}
          </div>
          {#if r.said || r.note}<div class="said" title={r.said ?? r.note}>{#if r.state === 'blocked' && r.said}« {r.said} »{:else}{r.note && r.state !== 'ready' ? r.note : ''}{r.note && r.said && r.state !== 'ready' ? ' · ' : ''}{r.said ?? ''}{/if}</div>{/if}
          {#if !compact}
            <div class="made">
              {#if mr}<button class="chip lnk" onclick={() => openMr(mr)} title={mr.title}>!{mr.iid}{#if mr.draft} {t('draft')}{/if}</button>{/if}
              {#if r.branch}<span class="chip mono">{r.branch}</span>{/if}
              {#if r.repo}<span class="chip">{r.files ? tn(r.files, '{n} file', '{n} files') : t('no file changed')}</span>{/if}
              {#if r.cost}<span class="chip">{r.cost.toFixed(2)} $</span>{/if}
            </div>
          {/if}
          <div class="l3">
            <span class="when">{r.state === 'blocked' ? t('{time} · waits for {ago}', { time: runSpan(r), ago: ago(r.started ?? r.due) }) : runSpan(r)}</span>
            <span class="grow"></span>
            {#if r.state === 'failed' || r.state === 'too_long'}<button class="q" onclick={() => rerun(r)} title={t('The routine again on this repository, now')}>{t('Rerun')}</button>{/if}
            {#if agentThere}<button class="q go" onclick={() => openRunAgent(r)} title={r.state === 'blocked' ? t('Its Chat, on its question') : t('Its agent, out of the archive')}>{r.state === 'blocked' ? t('Reply') : t('Open')}</button>{/if}
            {#if mr && !compact}<button class="q" onclick={() => openMr(mr)}>{t('Open MR')}</button>{/if}
            <button class="q" onclick={() => archiveRuns([r.id])} title={r.state === 'blocked' ? t('Out of the pile: its agent is stopped (it stays in the archive), the routine runs again at its next time') : t('Out of the pile; its agent stays in the archive')}>{t('Archive')}</button>
          </div>
        </div>
      </div>
    {/each}
    {#if next && (upcoming.length || off.length)}
      <div class="next">
        {#each upcoming as u (u.name)}
          <button class="nx" onclick={() => isDesktop() && openSettings('routines')} title={t('Settings › Routines')}><b>{u.name}</b> {nextTime(u.at)}{#if u.repos > 1} · {tn(u.repos, '{n} repository', '{n} repositories')}{/if}{#if u.digest} → {t('Digest')}{/if}</button>
        {/each}
        {#each off as o (o.name)}
          <button class="nx off" onclick={() => isDesktop() && openSettings('routines')} title={o.error ?? ''}><b>{o.name}</b> {o.error ? t('in error') : o.pending ? t('waits for your trust') : t('disabled')}</button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .review { display: flex; flex-direction: column; gap: 6px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); font-size: var(--fs-body); }
  .review.compact { margin: 4px 6px 8px; padding: 6px 8px; }
  .hd { display: flex; align-items: baseline; gap: 6px; }
  .grow { flex: 1; }
  .muted { color: var(--text-muted); font-size: var(--fs-meta); }
  .row { display: flex; gap: 8px; padding: 6px 0; border-top: 1px solid var(--border); }
  .ic { flex: 0 0 18px; height: 18px; border-radius: 50%; display: grid; place-items: center; font-size: 11px; font-weight: 600; color: #fff; background: var(--text-muted); }
  .row.ready .ic { background: var(--ok); }
  .row.blocked .ic { background: var(--warn); }
  .row.failed .ic, .row.too_long .ic { background: var(--ko); }
  .body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .l1, .l3, .made { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .name { font-weight: 600; }
  .repo { color: var(--text-muted); font-size: var(--fs-meta); }
  .st { padding: 0 6px; border-radius: var(--r9); font-size: var(--fs-tag); border: 1px solid var(--border); color: var(--text-muted); }
  .st.ready { color: var(--ok); border-color: color-mix(in srgb, var(--ok) 40%, var(--border)); }
  .st.blocked { color: var(--warn); border-color: var(--warn); }
  .st.failed, .st.too_long { color: var(--ko); border-color: color-mix(in srgb, var(--ko) 50%, var(--border)); }
  .tag { font-size: var(--fs-tag); color: var(--info); }
  .said { color: var(--text); font-size: var(--fs-meta); overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .compact .said { -webkit-line-clamp: 1; line-clamp: 1; }
  .chip { padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r-chip); font-size: var(--fs-tag); color: var(--text-muted); }
  .chip.mono { font-family: var(--mono); }
  .chip.lnk { background: none; cursor: pointer; color: var(--info); font: inherit; font-size: var(--fs-tag); }
  .when { color: var(--text-muted); font-size: var(--fs-tag); }
  .q { padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .q.go { border-color: transparent; background: color-mix(in srgb, var(--src-claude) 85%, #000); color: #fff; }
  :global(html.remote) .q { min-height: 32px; padding: 4px 12px; }
  .next { display: flex; flex-wrap: wrap; gap: 4px 10px; padding-top: 6px; border-top: 1px solid var(--border); }
  .nx { padding: 0; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; text-align: left; }
  .nx b { color: var(--text); font-weight: 500; }
  .nx.off { opacity: 0.7; }
</style>
