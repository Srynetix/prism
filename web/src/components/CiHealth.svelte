<script lang="ts">
  import RepoChip from './RepoChip.svelte'
  import { t, tn } from '../i18n.svelte'
  import BlockControls from './BlockControls.svelte'
  import Icon from './Icon.svelte'
  import Logo from './Logo.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import { claudeClick, flakyAction } from '../claude-actions'
  import { ago } from '../time'
  import type { CiHealth, CiScope } from '../types'
  import { keys } from '../platform'
  import { forgeLogo, forgeName } from '../forge'

  /**
   * CI health: for `main`, `release/*` or MRs, pipeline duration and success,
   * runner wait, slowest jobs and flaky jobs (Analyze with Claude). The
   * figures cover 7 days, compared with the previous 7; the curves, the whole
   * window. Chosen scope remembered under `ci.scope`.
   */
  let { health, error }: { health: CiHealth | null | undefined; error?: string } = $props()

  let key = $state(pref<string>('ci.scope', 'mr'))
  function setScope(k: string) {
    key = k
    setPref('ci.scope', k)
  }
  const scope = $derived(health?.scopes.find((s) => s.key === key) ?? health?.scopes[0])

  /** `18 m 40`, `45 s`, `1 h 05`. */
  function dur(s: number | null | undefined): string {
    if (s == null) return '—'
    const sec = Math.round(s)
    if (sec < 60) return `${sec} s`
    const m = Math.floor(sec / 60)
    if (m < 60) return `${m} m ${String(sec % 60).padStart(2, '0')}`
    return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')}`
  }
  const pct = (v: number | null | undefined) => (v == null ? '—' : `${Math.round(v * 100)} %`)
  /** Relative change from the previous week, in %. */
  const change = (cur: number | null, prev: number | null) => (cur == null || !prev ? null : Math.round(((cur - prev) / prev) * 100))
  const signed = (n: number) => (n > 0 ? `+${n}` : `${n}`)

  /** Curve of a series, without empty days. */
  function line(values: (number | null)[], w = 96, h = 18): string {
    const pts = values.map((v, i) => [i, v] as const).filter((p): p is readonly [number, number] => p[1] != null)
    if (pts.length < 2) return ''
    const ys = pts.map((p) => p[1])
    const [min, max] = [Math.min(...ys), Math.max(...ys)]
    const dx = w / Math.max(1, values.length - 1)
    return pts.map(([i, v]) => `${(i * dx).toFixed(1)},${(h - 2 - ((v - min) / (max - min || 1)) * (h - 4)).toFixed(1)}`).join(' ')
  }

  interface Kpi {
    label: string
    value: string
    sub: string
    /** An increase is bad news (duration, wait). */
    tone: 'bad' | 'good' | ''
    series: (number | null)[]
  }
  function kpis(s: CiScope): Kpi[] {
    const d = change(s.duration, s.duration_prev)
    const succ = s.success != null && s.success_prev != null ? Math.round((s.success - s.success_prev) * 100) : null
    return [
      { label: t('Pipeline p50'), value: dur(s.duration), sub: d == null ? t('no previous week') : t('{change} % vs prev.', { change: signed(d) }), tone: d == null || Math.abs(d) < 5 ? '' : d > 0 ? 'bad' : 'good', series: s.days.map((x) => x.duration) },
      { label: t('Success'), value: pct(s.success), sub: succ == null ? t('no previous week') : t('{change} pts', { change: signed(succ) }), tone: succ == null || Math.abs(succ) < 2 ? '' : succ < 0 ? 'bad' : 'good', series: s.days.map((x) => x.success) },
      { label: t('Wait for runner'), value: dur(s.queue), sub: `p90 ${dur(s.queue_p90)}`, tone: (s.queue_p90 ?? 0) > 300 ? 'bad' : '', series: s.days.map((x) => x.queue) },
      { label: t('Pipelines'), value: `${s.pipelines}`, sub: t('≈ {n} / day', { n: Math.round(s.pipelines / 7) }), tone: '', series: s.days.map((x) => x.pipelines) },
    ]
  }
</script>

<section class="panel" style="--src: var(--src-gitlab)" class:collapsed={isCollapsed('ci')}>
  <h2>
    <span class="title"><Logo name={forgeLogo()} />{t('CI health')}<RepoChip id="ci_health" /></span>
    <small>{health ? t('last 7 days vs previous · {days} d trend', { days: health.window_days }) : t('{forge} pipelines', { forge: forgeName() })}</small>
    {#if health}
      <span class="tabs" role="group" aria-label={t('Pipelines')}>
        {#each health.scopes as s (s.key)}<button class:on={s.key === scope?.key} onclick={() => setScope(s.key)}>{s.label}</button>{/each}
      </span>
    {/if}
    <BlockControls id="ci" />
  </h2>
  {#if !health || !scope}
    <div class="empty">{error ? t('Unavailable: {error}', { error }) : t('Reading two weeks of pipelines…')}</div>
  {:else if isShort('ci')}
    <div class="short-body">
      {#each health.scopes as s (s.key)}
        <div class="srow">
          <span class="t">{s.label}</span>
          <span class="mono">{dur(s.duration)}</span>
          <span class="mono">{pct(s.success)}</span>
          {#if s.flaky.length}<span class="pill warn">{t('{n} flaky', { n: s.flaky.length })}</span>{/if}
        </div>
      {/each}
    </div>
  {:else}
    <div class="kpis">
      {#each kpis(scope) as k (k.label)}
        <div class="kpi">
          <span class="l">{k.label}</span>
          <b>{k.value}</b>
          <small class={k.tone}>{k.sub}</small>
          <svg class="spark" viewBox="0 0 96 18" preserveAspectRatio="none" aria-hidden="true"><polyline points={line(k.series)} /></svg>
        </div>
      {/each}
    </div>
    <div class="scroll">
      <h5>{t('Slowest jobs')} <span>{t('median of successful runs · {days} d', { days: health.window_days })}</span></h5>
      {#each scope.slow_jobs as j (j.name)}
        {@const c = change(j.duration, j.duration_prev)}
        <div class="job">
          <span class="name" title={tn(j.runs, '{name} · {n} run this week', '{name} · {n} runs this week', { name: j.name })}>{j.name}</span>
          <span class="mono">{dur(j.duration)}</span>
          <svg class="spark" viewBox="0 0 96 18" preserveAspectRatio="none" aria-hidden="true"><polyline points={line(j.days)} /></svg>
          {#if c == null}<span class="delta">{t('new')}</span>{:else}<span class="delta" class:bad={c >= 10} class:good={c <= -10}>{signed(c)} %</span>{/if}
        </div>
      {:else}
        <p class="none">{t('Not enough successful runs this week.')}</p>
      {/each}
      <h5>{t('Flaky jobs')} {#if scope.flaky.length}<b>{scope.flaky.length}</b>{/if}<span>{t('failed, then passed on retry in the same pipeline')}</span></h5>
      {#each scope.flaky as f (f.name)}
        {@const a = flakyAction(f.name, f.urls)}
        <div class="job flaky">
          <span class="name" title={f.name}>{f.name}</span>
          <span class="muted">{t('{n}× · {pct} % of runs', { n: f.flakes, pct: Math.round((f.flakes / Math.max(1, f.runs)) * 100) })}</span>
          <span class="muted">{t('last {ago}', { ago: ago(f.last_at) })}</span>
          {#if f.urls[0]}<a class="log" href={f.urls[0]} target="_blank" rel="noreferrer" title={t('Last failed attempt in {forge}', { forge: forgeName() })}>{t('log')} <Icon name="external" /></a>{:else}<span></span>{/if}
          {#if a.href}<a class="act" href={a.href} onclick={claudeClick(a)} title={`${a.prompt}\n${t('{key}: launch at once', { key: keys('⇧-click') })}`}><Logo name="agent" size={10} />{a.label}</a>{:else}<span class="act off" title={a.why}><Logo name="agent" size={10} />{a.label}</span>{/if}
        </div>
      {:else}
        <p class="none">{t('No job needed a retry to pass.')}</p>
      {/each}
    </div>
  {/if}
</section>

<style>
  section { container-type: inline-size; }
  .tabs { display: inline-flex; gap: 2px; margin-left: auto; padding: 2px; border-radius: var(--r6); background: var(--surface-muted); }
  .tabs button { padding: 0 8px; border: none; border-radius: var(--r4); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; }
  .tabs button.on { background: var(--surface); color: var(--text); font-weight: 500; }
  .kpis { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 1px; background: var(--border); border-bottom: 1px solid var(--border); }
  .kpi { display: flex; flex-direction: column; gap: 1px; min-width: 0; padding: 6px 10px; background: var(--surface); }
  .kpi .l { font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .kpi b { font-size: 16px; font-weight: 600; white-space: nowrap; }
  .kpi small { font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .bad { color: var(--ko) !important; }
  .good { color: var(--ok) !important; }
  .spark { display: block; width: 100%; max-width: 120px; height: 18px; }
  .spark polyline { fill: none; stroke: var(--src-gitlab); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
  h5 { display: flex; align-items: baseline; gap: 6px; margin: 0; padding: 5px 12px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); background: var(--bg); border-bottom: 1px solid var(--border); }
  h5 b { color: var(--warn); }
  h5 span { margin-left: auto; font-weight: 400; text-transform: none; letter-spacing: 0; }
  .job { display: grid; grid-template-columns: minmax(0, 1fr) 70px 96px 54px; align-items: center; gap: 10px; padding: 4px 12px; border-bottom: 1px solid var(--border); font-size: var(--fs-body); }
  .job.flaky { grid-template-columns: minmax(0, 1fr) auto auto auto auto; }
  .job .spark polyline { stroke: var(--text-muted); }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: var(--fs-mono); }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); white-space: nowrap; }
  .muted { color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .delta { justify-self: start; padding: 0 6px; border-radius: var(--r9); background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .delta.bad { background: var(--ko-bg); }
  .delta.good { background: var(--ok-bg); }
  .log { font-size: var(--fs-tag); white-space: nowrap; }
  .act { display: inline-flex; flex-shrink: 0; align-items: center; gap: 4px; padding: 0 8px; border: 1px solid color-mix(in srgb, var(--src-claude) 60%, var(--border)); border-radius: var(--r5); background: var(--surface); color: var(--text); font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; }
  .act:hover { background: color-mix(in srgb, var(--src-claude) 12%, var(--surface)); text-decoration: none; }
  .act.off { opacity: 0.5; cursor: not-allowed; }
  .none { margin: 6px 12px; color: var(--text-muted); font-size: var(--fs-meta); }
  /* Narrow block: two indicators per row, and neither curves nor dates in the lists. */
  @container (max-width: 520px) {
    .kpis { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .job { grid-template-columns: minmax(0, 1fr) 64px 48px; }
    .job .spark { display: none; }
    .job.flaky { grid-template-columns: minmax(0, 1fr) auto auto; }
    .job.flaky .muted:nth-of-type(2), .log { display: none; }
  }
</style>
