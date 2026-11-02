<script lang="ts">
  import { invoke } from '../bridge'
  import { t } from '../i18n.svelte'
  import { cpu, mem } from '../time'
  import { pref, setPref } from '../layout.svelte'
  import { dateFormat } from '../locale'
  import type { UsageHistory, UsagePoint } from '../types'

  /**
   * A pod's CPU and memory over a period, above its log (`pod_usage_history`): two charts, each its
   * own scale, the containers' limit dashed across. From Google Managed Prometheus for a GKE
   * context, otherwise what Prism kept of metrics-server's readings (6 h at most). Read again
   * while shown; hovering one (a finger sliding across, on a phone) shows a moment's values in both.
   */
  let { context, namespace, pod }: { context: string; namespace: string; pod: string } = $props()

  const PERIODS = [
    { minutes: 60, label: '1 h' },
    { minutes: 360, label: '6 h' },
    { minutes: 1440, label: '24 h' },
    { minutes: 10080, label: '7 d' },
  ]
  let minutes = $state(pref<number>('logs.usage_minutes', 60))
  function setPeriod(m: number) {
    minutes = m
    setPref('logs.usage_minutes', m)
  }

  let history = $state<UsageHistory | null>(null)
  let error = $state<string | null>(null)
  let loading = $state(false)

  $effect(() => {
    const key = { context, namespace, pod, minutes }
    let timer: ReturnType<typeof setTimeout> | undefined
    let gone = false
    history = null
    error = null
    async function read() {
      loading = true
      try {
        const h = await invoke<UsageHistory>('pod_usage_history', key)
        if (gone) return
        history = h
        error = null
      } catch (e) {
        if (gone) return
        error = String(e)
      } finally {
        if (!gone) loading = false
      }
      // Prism's readings come every 15 s; Google's are minutes apart at best.
      if (!gone) timer = setTimeout(read, history?.source === 'prism' ? 15_000 : 60_000)
    }
    read()
    return () => {
      gone = true
      clearTimeout(timer)
    }
  })

  const W = 600
  const H = 96
  /** The moment hovered, in either chart: both show it. */
  let hover = $state<number | null>(null)

  const points = $derived(history?.points ?? [])
  const span = $derived.by(() => {
    const end = Math.floor(Date.now() / 1000)
    // Prism's history starts with its first reading: the chart starts there.
    const start = history?.source === 'prism' && points.length ? Math.max(points[0].at, end - minutes * 60) : end - minutes * 60
    return { start, end: Math.max(end, points.at(-1)?.at ?? end) }
  })
  const x = (at: number) => ((at - span.start) / Math.max(1, span.end - span.start)) * W

  interface Series {
    key: 'cpu' | 'mem'
    title: string
    value: (p: UsagePoint) => number | null
    limit: number | null
    fmt: (v: number) => string
  }
  const series = $derived<Series[]>([
    { key: 'cpu', title: t('CPU'), value: (p) => p.cpu_millis, limit: history?.cpu_limit_millis ?? null, fmt: (v) => cpu(Math.round(v)) },
    { key: 'mem', title: t('Memory'), value: (p) => p.mem_bytes, limit: history?.mem_limit_bytes ?? null, fmt: mem },
  ])

  /**
   * The top of a chart: its highest point with room above, its limit included when near (within
   * twice the highest point); a limit far above would flatten the line, its value stays written.
   */
  function top(s: Series): number {
    const max = Math.max(0, ...points.map((p) => s.value(p) ?? 0))
    const limit = s.limit !== null && s.limit <= max * 2 ? s.limit : 0
    return Math.max(max * 1.15, limit * 1.08) || 1
  }

  /** The line, broken where readings are missing (Prism stopped, a pod not yet scraped). */
  function path(s: Series, max: number): { line: string; area: string } {
    const gap = (history?.step ?? 15) * 2.5
    let line = ''
    let area = ''
    let run: [number, number][] = []
    let last: number | null = null
    const flush = () => {
      if (run.length) {
        line += run.map(([px, py], i) => `${i ? 'L' : 'M'}${px.toFixed(1)},${py.toFixed(1)}`).join('')
        area += `M${run[0][0].toFixed(1)},${H}` + run.map(([px, py]) => `L${px.toFixed(1)},${py.toFixed(1)}`).join('') + `L${run.at(-1)![0].toFixed(1)},${H}Z`
      }
      run = []
    }
    for (const p of points) {
      const v = s.value(p)
      if (v === null || (last !== null && p.at - last > gap)) flush()
      if (v !== null) run.push([x(p.at), H - (v / max) * H])
      last = v === null ? null : p.at
    }
    flush()
    return { line, area }
  }

  /** The point nearest the moment hovered. */
  const near = $derived.by(() => {
    if (hover === null || !points.length) return null
    let best = points[0]
    for (const p of points) if (Math.abs(p.at - hover) < Math.abs(best.at - hover)) best = p
    return best
  })
  const latest = $derived(points.at(-1) ?? null)

  function onMove(e: PointerEvent) {
    const box = (e.currentTarget as SVGElement).getBoundingClientRect()
    hover = span.start + ((e.clientX - box.left) / box.width) * (span.end - span.start)
  }

  const when = (at: number) => dateFormat(minutes > 1440 ? { weekday: 'short', hour: '2-digit', minute: '2-digit' } : { hour: '2-digit', minute: '2-digit' }).format(new Date(at * 1000))
  const pct = (v: number, limit: number | null) => (limit ? ` · ${Math.round((v / limit) * 100)} %` : '')
</script>

<section class="usage" aria-label={t('Usage over time')}>
  <div class="bar">
    <span class="seg" role="group" aria-label={t('Period')}>
      {#each PERIODS as p (p.minutes)}
        <button class:on={minutes === p.minutes} onclick={() => setPeriod(p.minutes)} disabled={history?.source === 'prism' && p.minutes > 360} title={history?.source === 'prism' && p.minutes > 360 ? t('Prism keeps 6 hours of a pod’s usage') : undefined}>{t(p.label)}</button>
      {/each}
    </span>
    <span class="src">
      {#if history?.source === 'gcp'}{t('Google Managed Prometheus')}{:else if history?.source === 'prism' && points.length}{t('Kept by Prism since {time}', { time: when(points[0].at) })}{:else if history?.source === 'prism'}{t('Kept by Prism: no reading yet')}{/if}
      {#if loading}<span class="dim">· {t('reading…')}</span>{/if}
    </span>
  </div>
  {#if history?.error}<p class="warn" title={history.error}>{t('Google Managed Prometheus unreadable, Prism’s readings instead:')} {history.error}</p>{/if}
  {#if error}
    <p class="err">{error}</p>
  {:else if history && !points.length}
    <p class="dim pad">{t('No reading for this pod over this period.')}</p>
  {:else if history}
    <div class="charts">
      {#each series as s (s.key)}
        {@const max = top(s)}
        {@const d = path(s, max)}
        {@const shown = near ?? latest}
        {@const v = shown ? s.value(shown) : null}
        <figure class="chart {s.key}">
          <figcaption>
            <b>{s.title}</b>
            <span class="v">{v === null ? '—' : s.fmt(v)}{#if s.limit !== null}<small>{' '}/ {s.fmt(s.limit)}{v === null ? '' : pct(v, s.limit)}</small>{/if}</span>
            {#if near}<span class="at">{when(near.at)}</span>{/if}
          </figcaption>
          <div class="plot">
          <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" onpointermove={onMove} onpointerleave={() => (hover = null)} role="img" aria-label={t('{what} over the period, up to {max}', { what: s.title, max: s.fmt(max) })}>
            <line class="grid" x1="0" x2={W} y1={H / 2} y2={H / 2} />
            <path class="area" d={d.area} />
            <path class="line" d={d.line} />
            {#if s.limit !== null && s.limit <= max}<line class="limit" x1="0" x2={W} y1={H - (s.limit / max) * H} y2={H - (s.limit / max) * H} />{/if}
            {#if near && s.value(near) !== null}
              <line class="cross" x1={x(near.at)} x2={x(near.at)} y1="0" y2={H} />
            {/if}
          </svg>
          {#if near && s.value(near) !== null}
            <i class="dot" style:left="{(x(near.at) / W) * 100}%" style:top="{(1 - (s.value(near) ?? 0) / max) * 100}%"></i>
          {/if}
          </div>
          <div class="axis"><span>{when(span.start)}</span>{#if s.limit !== null}<span class="lim">{t('limit {value}', { value: s.fmt(s.limit) })}</span>{/if}<span>{t('now')}</span></div>
        </figure>
      {/each}
    </div>
  {:else}
    <p class="dim pad">{t('Reading the usage…')}</p>
  {/if}
</section>

<style>
  .usage { flex-shrink: 0; padding: 8px 12px 10px; border-bottom: 1px solid var(--border); background: var(--surface); font-size: var(--fs-meta); }
  .bar { display: flex; align-items: center; gap: 10px; margin-bottom: 6px; }
  .src { color: var(--text-muted); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dim { color: var(--text-muted); }
  .pad { margin: 6px 0; }
  .err { margin: 6px 0; color: var(--ko); white-space: pre-wrap; }
  .warn { margin: 0 0 6px; color: var(--warn); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .seg { display: inline-flex; gap: 1px; padding: 2px; border-radius: var(--r6); background: var(--surface-muted); }
  .seg button { padding: 0 8px; white-space: nowrap; border: none; border-radius: var(--r4); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); font-weight: 500; }
  .seg button:disabled { opacity: 0.4; cursor: default; }
  .charts { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
  .chart { margin: 0; min-width: 0; }
  .plot { position: relative; height: 72px; }
  figcaption { display: flex; flex-wrap: wrap; align-items: baseline; column-gap: 8px; margin-bottom: 4px; }
  figcaption b { font-weight: 600; }
  .v { white-space: nowrap; font-family: var(--mono); font-size: var(--fs-mono); font-variant-numeric: tabular-nums; }
  .v small { color: var(--text-muted); }
  .at { margin-left: auto; color: var(--text-muted); font-size: var(--fs-tag); }
  svg { display: block; width: 100%; height: 72px; overflow: visible; cursor: crosshair; touch-action: pan-y; }
  .grid { stroke: var(--border); stroke-width: 1; vector-effect: non-scaling-stroke; }
  .line { fill: none; stroke: var(--src-k8s); stroke-width: 2; stroke-linejoin: round; vector-effect: non-scaling-stroke; }
  .area { fill: color-mix(in srgb, var(--src-k8s) 14%, transparent); stroke: none; }
  .limit { stroke: var(--text-muted); stroke-width: 1; stroke-dasharray: 4 3; vector-effect: non-scaling-stroke; }
  .cross { stroke: var(--text-muted); stroke-width: 1; vector-effect: non-scaling-stroke; }
  /* Drawn outside the stretched SVG: round whatever the chart's width. */
  .dot { position: absolute; width: 8px; height: 8px; margin: -4px 0 0 -4px; border-radius: 50%; background: var(--src-k8s); box-shadow: 0 0 0 2px var(--surface); pointer-events: none; }
  .axis { white-space: nowrap; display: flex; justify-content: space-between; gap: 8px; margin-top: 2px; color: var(--text-muted); font-size: var(--fs-tag); font-variant-numeric: tabular-nums; }
  .lim { opacity: 0.85; }
  @media (max-width: 640px) {
    .charts { grid-template-columns: 1fr; }
  }
</style>
