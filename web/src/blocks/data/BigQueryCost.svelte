<script lang="ts">
  import { locale } from '../../locale'
  import { t, tn } from '../../i18n.svelte'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import Logo from '../../components/Logo.svelte'
  import type { BlockProps } from '../block'
  import { isCollapsed, isShort } from '../../layout.svelte'
  import DataRefresh from './DataRefresh.svelte'
  import { span } from '../../time'
  import { bqJobUrl, bytes, costStats, dollars, money } from './data'
  import type { GcpData } from './types'

  /**
   * BigQuery cost: an estimate of query cost (bytes billed × on-demand price), not the
   * invoice. Today against the 14-day median, the current month, and the day's most
   * expensive jobs. A person is never named, and the SQL is never read.
   */
  let { state: data, error }: BlockProps<GcpData> = $props()
  const enabled = $derived(data !== undefined)

  const cost = $derived(data?.cost ?? null)
  const stats = $derived(cost ? costStats(cost, data!.thresholds) : null)
  const max = $derived(Math.max(1, ...(cost?.days.map((d) => d.bytes) ?? [])))
  const day = (iso: string) => new Date(`${iso}T12:00:00`).toLocaleDateString(locale(), { weekday: 'short', day: 'numeric', month: 'short' })
</script>

<section class="panel" style="--src: var(--src-k8s)" class:collapsed={isCollapsed('bqcost')}>
  <h2>
    <span class="title"><Logo name="gcp" />{t('BigQuery cost')}</span>
    <span class="head">{#if cost}<small>{t('estimate · {price}/TiB', { price: money(cost.price_per_tib) })}{cost.partial ? ` · ${t('reading history…')}` : ''}</small>{/if}{#if enabled}<DataRefresh />{/if}</span>
    <BlockControls id="bqcost" />
  </h2>
  {#if !enabled}
    <div class="empty">{t('GCP is not read. Turn on Settings › Infra › Data (GCP), and its BigQuery cost switch, then restart Prism.')}</div>
  {:else if !cost || !stats}
    <div class="empty">{error ? t('GCP unavailable: {error}', { error }) : data && !data.cost ? t('BigQuery cost is off ([data] bigquery_cost), or its jobs could not be read.') : t('Reading BigQuery jobs…')}</div>
  {:else if isShort('bqcost')}
    <div class="short-body">
      <div class="srow"><span class="t">{t('Today')}</span><b class:spike={stats.spike}>{money(dollars(stats.today, cost))}</b>{#if stats.spike}<span class="pill warn">{t('{x}× median', { x: stats.spike.toFixed(1) })}</span>{/if}</div>
      <div class="srow"><span class="t">{t('This month')}</span><b>{money(dollars(cost.month_bytes, cost))}</b></div>
    </div>
  {:else}
    <div class="scroll">
      <div class="tiles">
        <div class="tile" class:spike={stats.spike}>
          <span class="cap">{t('Today')}</span><b>{money(dollars(stats.today, cost))}</b>
          <span>{stats.median !== null ? t('median {amount}/day', { amount: money(dollars(stats.median, cost)) }) : t('no history yet')}{#if stats.spike}{' '}· <b class="x">{stats.spike.toFixed(1)}×</b>{/if}</span>
        </div>
        <div class="tile"><span class="cap">{t('This month')}</span><b>{money(dollars(cost.month_bytes, cost))}</b><span>{t('{bytes} billed', { bytes: bytes(cost.month_bytes) })}</span></div>
        <div class="tile"><span class="cap">{t('Billed today')}</span><b>{bytes(stats.today)}</b><span>{tn(cost.days.at(-1)?.jobs ?? 0, '{n} job', '{n} jobs')}</span></div>
        <div class="tile bars" title={t('Last 14 days')}>
          <span class="cap">{t('14 days')}</span>
          <div class="chart">{#each cost.days as d, i (d.day)}<i class:today={i === cost.days.length - 1} class:spike={i === cost.days.length - 1 && stats.spike} style:height="{Math.max(2, (d.bytes / max) * 100)}%" title="{day(d.day)} · {money(dollars(d.bytes, cost))} · {tn(d.jobs, '{n} job', '{n} jobs')}"></i>{/each}</div>
        </div>
      </div>
      <div class="section">{t('Most expensive today')}</div>
      {#each cost.top as j (j.project + j.id)}
        <a class="row line" href={bqJobUrl(j.project, j.location, j.id)} target="_blank" rel="noreferrer" title="{j.id}{j.tables.length ? `\n${t('reads {tables}', { tables: j.tables.join(', ') })}` : ''}">
          <span class="grow"><b class="mono">{j.statement ?? t('job')}</b> <span class="muted">{j.tables[0] ?? j.id}{j.tables.length > 1 ? ` +${j.tables.length - 1}` : ''}</span></span>
          <span class="who muted" title={j.service_account ? t('Service account') : t('Run by a person: never named')}>{j.service_account?.split('@')[0] ?? t('a person')}</span>
          <span class="num muted">{new Date(j.created_at).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })}</span>
          <span class="num muted" title={t('Slot time')}>{span(j.slot_ms)}</span>
          <span class="num">{bytes(j.bytes_billed)}</span>
          <b class="num">{money(dollars(j.bytes_billed, cost))}</b>
          <Icon name="external" />
        </a>
      {:else}
        <div class="empty">{t('No billed query today.')}</div>
      {/each}
      <div class="note">{t('An estimate, not the invoice: on-demand price, no discounts, no storage nor other GCP service. Queries run by people count, unnamed.')}</div>
    </div>
  {/if}
</section>

<style>
  .head { display: flex; gap: 10px; align-items: center; }
  .head small { white-space: nowrap; }
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: 8px; padding: 8px 12px; }
  .tile { display: flex; flex-direction: column; gap: 2px; min-width: 0; padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r8); color: var(--text-muted); font-size: var(--fs-meta); }
  .tile .cap { font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; }
  .tile > b { color: var(--text); font-size: 16px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .tile.spike { border-color: color-mix(in srgb, var(--ko) 45%, var(--border)); }
  .tile.spike > b, .x { color: var(--ko); }
  .chart { display: flex; align-items: flex-end; gap: 2px; height: 34px; margin-top: 2px; }
  .chart i { flex: 1; min-height: 1px; border-radius: var(--r2) var(--r2) 0 0; background: color-mix(in srgb, var(--text-muted) 50%, transparent); }
  .chart i.today { background: var(--src); }
  .chart i.spike { background: var(--ko); }
  .section { padding: 4px 12px; background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); border-top: 1px solid var(--border); }
  .line { color: inherit; text-decoration: none; }
  .line:hover { background: var(--surface-muted); }
  .line b { font-weight: 550; }
  .who { flex-shrink: 1; min-width: 0; max-width: 30%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-meta); }
  .num { flex-shrink: 0; font-variant-numeric: tabular-nums; font-size: var(--fs-meta); }
  .spike { color: var(--ko); }
  .note { padding: 8px 12px; color: var(--text-muted); font-size: var(--fs-tag); }
</style>
