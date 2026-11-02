<script lang="ts">
  import { clock } from '../../clock.svelte'
  import { locale } from '../../locale'
  import { t, tn, tc } from '../../i18n.svelte'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import Logo from '../../components/Logo.svelte'
  import type { BlockProps } from '../block'
  import { isCollapsed, isShort } from '../../layout.svelte'
  import DataRefresh from './DataRefresh.svelte'
  import { ago, span } from '../../time'
  import {
    dataProblems, jobLogsUrl, jobState, jobUrl, lag, medianDuration, runDuration, streamState, streamUrl, tableAge, tableState, tableUrl, transferState,
    type DataState,
  } from './data'
  import type { GcpData, RunJob } from './types'

  /**
   * Data: Cloud Run jobs (and their trigger), freshness of the declared BigQuery tables, scheduled
   * queries and Datastream streams. By default, only what is wrong; "All" shows the rest.
   * A row expands into its detail. Metadata only: no logs, no SQL, no rows.
   */
  let { state: data, error }: BlockProps<GcpData> = $props()
  const enabled = $derived(data !== undefined)

  let all = $state(false)
  let open = $state<string | null>(null)
  const toggle = (key: string) => (open = open === key ? null : key)

  const now = $derived(clock(30_000).now)

  const shown = (s: DataState) => all || s.tone === 'ko' || s.tone === 'warn'
  const jobs = $derived((data?.jobs ?? []).map((j) => ({ j, s: jobState(j, data!.thresholds, now) })))
  const tables = $derived((data?.tables ?? []).map((tb) => ({ tb, s: tableState(tb, data?.jobs ?? [], data!.thresholds, now) })))
  const queries = $derived((data?.transfers ?? []).map((q) => ({ q, s: transferState(q) })))
  const streams = $derived((data?.streams ?? []).map((st) => ({ st, s: streamState(st, data!.thresholds) })))
  const problems = $derived(dataProblems(data, now).filter((p) => p.kind !== 'cost'))
  const ko = $derived(problems.filter((p) => p.state.tone === 'ko').length)
  const healthy = $derived(jobs.length + tables.length + queries.length + streams.length - problems.length)

  const dur = (ms: number | null) => (ms === null ? '—' : span(ms))
  const execTitle = (e: RunJob['executions'][number]) =>
    `${e.name} · ${new Date(e.created_at).toLocaleString(locale())} · ${t(e.status)}${e.exit_code !== null ? ` (${tc('blocks', 'exit {code}', { code: e.exit_code })})` : ''}`
  /**
   * Without a recent point, Datastream had nothing to read: no lag. The last point (often the
   * catch-up) stays in the curve, not in the current value.
   */
  const freshnessTitle = (st: GcpData['streams'][number]) =>
    st.freshness_seconds !== null
      ? t('Replication lag: how far Datastream is behind the source')
      : st.freshness_at
        ? t('Nothing to read since {time}: no lag', { time: new Date(st.freshness_at).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' }) })
        : t('No freshness metric in the last 6 hours')
  /** Polyline of a stream's lag, over 6 h. */
  function sparkPath(points: number[], w = 220, h = 36): string {
    if (points.length < 2) return ''
    const max = Math.max(...points, 1)
    return points.map((p, i) => `${i ? 'L' : 'M'}${((i / (points.length - 1)) * w).toFixed(1)},${(h - (p / max) * (h - 2) - 1).toFixed(1)}`).join(' ')
  }
</script>

{#snippet pill(s: DataState)}<span class="pill {s.tone === 'run' ? 'info' : s.tone === 'off' ? '' : s.tone}" title={s.why}>{s.label}</span>{/snippet}
{#snippet dot(s: DataState)}<span class="dot {s.tone}"></span>{/snippet}

<section class="panel" style="--src: var(--src-k8s)" class:collapsed={isCollapsed('data')}>
  <h2>
    <span class="title"><Logo name="gcp" />{t('Data')}</span>
    <span class="head">
      {#if data}<small>{problems.length ? `${ko ? `${tn(ko, '{n} broken', '{n} broken')} · ` : ''}${problems.length - ko ? `${tn(problems.length - ko, '{n} drifting', '{n} drifting')} · ` : ''}` : ''}{tn(healthy, '{n} fine', '{n} fine')}</small>{/if}
      <span class="seg" role="radiogroup" aria-label={t('Show')}>
        <button role="radio" aria-checked={!all} class:on={!all} onclick={() => (all = false)}>{t('Problems')}</button>
        <button role="radio" aria-checked={all} class:on={all} onclick={() => (all = true)}>{t('All')}</button>
      </span>
      {#if enabled}<DataRefresh />{/if}
    </span>
    <BlockControls id="data" />
  </h2>
  {#if !enabled}
    <div class="empty">{t('GCP is not read. Turn on Settings › Infra › Data (GCP), or use the Data template of the pages wizard, then restart Prism.')}</div>
  {:else if !data}
    <div class="empty">{error ? t('GCP unavailable: {error}', { error }) : t('Reading GCP…')}</div>
  {:else if isShort('data')}
    <div class="short-body">
      {#each problems as p (p.kind + p.name)}
        <div class="srow">{@render dot(p.state)}<span class="t">{p.name}</span>{@render pill(p.state)}</div>
      {:else}
        <div class="srow more">{t('All {n} jobs, tables and streams are fine.', { n: healthy })}</div>
      {/each}
    </div>
  {:else}
    <div class="scroll">
      {#if jobs.length}
        <div class="section">{t('Cloud Run jobs')} · {jobs.length}</div>
        {#each jobs.filter((x) => shown(x.s)) as { j, s } (j.project + j.name)}
          {@const key = `job:${j.project}/${j.name}`}
          {@const med = medianDuration(j)}
          {@const last = j.executions[0]}
          <button class="row line" class:open={open === key} onclick={() => toggle(key)}>
            {@render dot(s)}
            <span class="grow"><b>{j.name}</b> <span class="muted mono">{j.trigger?.schedule ?? t('no schedule')} · {j.project}</span></span>
            <span class="hist" aria-hidden="true">{#each [...j.executions].reverse() as e (e.name)}<i class={e.status} title={execTitle(e)}></i>{/each}</span>
            <span class="num muted" title={med ? t('median {time}', { time: span(med) }) : t('no median yet')}>{last ? dur(runDuration(last, now)) : '—'}</span>
            {@render pill(s)}
          </button>
          {#if open === key}
            <div class="detail">
              <div class="kv">
                {#if s.why}<span>{t('Why')}</span><span class="why {s.tone}">{s.why}</span>{/if}
                <span>{t('Last run')}</span><span>{last ? `${new Date(last.created_at).toLocaleString(locale())} · ${t(last.status)}` : t('never')}</span>
                <span>{t('Duration')}</span><span>{last ? dur(runDuration(last, now)) : '—'}{med ? ` · ${t('median {time}', { time: span(med) })}` : ''}</span>
                {#if j.trigger}
                  <span>{t('Trigger')}</span><span class="mono">{j.trigger.schedule}{j.trigger.time_zone ? ` (${j.trigger.time_zone})` : ''} · {t(j.trigger.state.toLowerCase())}{j.trigger.next_run ? ` · ${t('next {time}', { time: new Date(j.trigger.next_run).toLocaleString(locale()) })}` : ''}</span>
                {/if}
                <span>{t('Deployed')}</span><span>{j.image ? `${j.image} · ` : ''}{j.updated_at ? t('{time} ago', { time: ago(j.updated_at) }) : '—'}</span>
              </div>
              <div class="links">
                <a href={jobUrl(j)} target="_blank" rel="noreferrer">{t('Executions')} <Icon name="external" /></a>
                <a href={jobLogsUrl(j)} target="_blank" rel="noreferrer">{t('Logs')} <Icon name="external" /></a>
              </div>
            </div>
          {/if}
        {/each}
      {/if}

      {#if tables.length}
        <div class="section">{t('BigQuery freshness')} · {tables.length}</div>
        {#each tables.filter((x) => shown(x.s)) as { tb, s } (tb.table)}
          {@const age = tableAge(tb, now)}
          <a class="row line" href={tableUrl(tb.table)} target="_blank" rel="noreferrer" title={s.why}>
            {@render dot(s)}
            <span class="grow"><b class="mono">{tb.table}</b>{#if tb.fed_by}<span class="muted fed" title={t('Fed by the Cloud Run job {job}', { job: tb.fed_by })}>← {tb.fed_by}</span>{/if}</span>
            <span class="num muted" title={t('Last modified {time}', { time: tb.last_modified ? new Date(tb.last_modified).toLocaleString(locale()) : t('never') })}>{age === null ? '—' : `${Math.round(age)}h`} / {tb.max_age_hours}h</span>
            {#if tb.rows !== null}<span class="num muted">{tn(tb.rows, '{n} row', '{n} rows', { n: tb.rows.toLocaleString(locale()) })}</span>{/if}
            {@render pill(s)}
          </a>
        {/each}
      {/if}

      {#if queries.some((x) => shown(x.s))}
        <div class="section">{t('Scheduled queries')} · {queries.length}</div>
        {#each queries.filter((x) => shown(x.s)) as { q, s } (q.project + q.name)}
          <a class="row line" href="https://console.cloud.google.com/bigquery/scheduled-queries?project={q.project}" target="_blank" rel="noreferrer" title={s.why}>
            {@render dot(s)}
            <span class="grow"><b>{q.name}</b> <span class="muted mono">{q.schedule ?? ''} · {q.project}</span></span>
            <span class="num muted">{q.updated_at ? ago(q.updated_at) : '—'}</span>
            {@render pill(s)}
          </a>
        {/each}
      {/if}

      {#if streams.length}
        <div class="section">Datastream · {streams.length}</div>
        {#each streams.filter((x) => shown(x.s)) as { st, s } (st.project + st.name)}
          {@const key = `stream:${st.project}/${st.name}`}
          <button class="row line" class:open={open === key} onclick={() => toggle(key)}>
            {@render dot(s)}
            <span class="grow"><b>{st.display_name}</b> <span class="muted">{st.source ?? '?'} → {st.destination ?? '?'} · {st.project}</span></span>
            <span class="num muted" title={freshnessTitle(st)}>{st.freshness_seconds !== null ? lag(st.freshness_seconds) : st.freshness_at ? t('idle') : '—'}</span>
            {@render pill(s)}
          </button>
          {#if open === key}
            <div class="detail">
              {#if st.freshness_history.length > 1}
                <svg class="spark" viewBox="0 0 220 36" preserveAspectRatio="none" aria-label={t('Freshness over 6 hours')}><path d={sparkPath(st.freshness_history)} /></svg>
                <div class="axis"><span>−6 h</span><span>{t('now')}</span></div>
              {/if}
              <div class="kv">
                <span>{t('State')}</span><span>{st.state}</span>
                {#if st.errors.length}<span>{t('Errors')}</span><span class="mono">{st.errors.join(', ')}</span>{/if}
              </div>
              <div class="links"><a href={streamUrl(st)} target="_blank" rel="noreferrer">{t('Datastream console')} <Icon name="external" /></a></div>
            </div>
          {/if}
        {/each}
      {/if}

      {#if !all && !problems.length}
        <div class="empty">{t('Nothing broken: {n} jobs, tables and streams are fine.', { n: healthy })}</div>
      {/if}
      {#each data.errors as e (e)}<div class="row err" title={e}><Icon name="warning" /> <span class="grow">{e}</span></div>{/each}
    </div>
  {/if}
</section>

<style>
  .head { display: flex; gap: 10px; align-items: center; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { padding: 0 8px; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); font-weight: 400; cursor: pointer; }
  .seg button.on { background: color-mix(in srgb, var(--src) 18%, transparent); color: var(--text); }
  .section { padding: 4px 12px; background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); border-top: 1px solid var(--border); }
  .section:first-child { border-top: none; }
  .line { width: 100%; border: none; border-top: 1px solid var(--border); background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; text-decoration: none; }
  .line:hover, .line.open { background: var(--surface-muted); }
  .line b { font-weight: 550; }
  .num { flex-shrink: 0; font-variant-numeric: tabular-nums; font-size: var(--fs-meta); }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; background: var(--text-muted); opacity: 0.5; }
  .dot.ok { background: var(--ok); opacity: 1; }
  .dot.ko { background: var(--ko); opacity: 1; }
  .dot.warn { background: var(--warn); opacity: 1; }
  .dot.run { background: var(--info); opacity: 1; animation: blink 1.4s infinite; }
  .dot.off { background: none; border: 1.5px solid var(--text-muted); }
  @keyframes blink { 50% { opacity: 0.35; } }
  /* History: one cell per run, the most recent on the right. */
  .hist { display: inline-flex; gap: 2px; flex-shrink: 0; }
  .hist i { width: 5px; height: 11px; border-radius: calc(1.5px * var(--round)); background: var(--ok); opacity: 0.8; }
  .hist i.failed { background: var(--ko); opacity: 1; }
  .hist i.cancelled { background: var(--warn); }
  .hist i.running { background: var(--info); animation: blink 1.4s infinite; }
  .detail { display: flex; flex-direction: column; gap: 6px; padding: 6px 12px 10px 28px; background: var(--surface-muted); font-size: var(--fs-meta); }
  .kv { display: grid; grid-template-columns: 76px minmax(0, 1fr); gap: 3px 8px; }
  .kv > span:nth-child(odd) { color: var(--text-muted); }
  .why.ko { color: var(--ko); } .why.warn { color: var(--warn); }
  .links { display: flex; gap: 12px; }
  .spark { width: 100%; height: 36px; }
  .spark path { fill: none; stroke: var(--info); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
  .axis { display: flex; justify-content: space-between; color: var(--text-muted); font-size: var(--fs-tag); }
  .fed { margin-left: 6px; }
  .err { color: var(--warn); font-size: var(--fs-meta); }
</style>
