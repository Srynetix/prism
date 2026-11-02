<script lang="ts">
  import { clock } from '../clock.svelte'
  import { locale } from '../locale'
  import type { ClaudeUsage, UsageWindow } from '../types'
  import { t } from '../i18n.svelte'
  import { month, pace, resets, usageLevel as level, week, weekTitle, type Period } from '../usage'

  /** Plan usage (status bar drawer): 5 h window, week, and the month for Command Code. */
  let { usage, error }: { usage: ClaudeUsage | null; error: string | undefined } = $props()

  const now = $derived(clock(60_000).now)

  const label = (key: string) => (key === 'month' ? 'Month' : key.replace(/^seven_day_/, `${t('week')} · `).replace(/_/g, ' '))
  const rows = $derived(
    usage
      ? ([
          ['Session (5h)', usage.five_hour],
          ['Week', usage.seven_day],
          ...usage.others.map(([k, w]) => [label(k), w] as const),
        ].filter(([, w]) => !!w) as [string, UsageWindow][])
      : [],
  )

  const wk = $derived(week(usage?.seven_day, now))
  const mo = $derived(month(usage?.others.find(([k]) => k === 'month')?.[1], now))
  /** The week and the month are drawn day by day. */
  const days = (name: string): Period | null => (name === 'Week' ? wk : name === 'Month' ? mo : null)
</script>

<div class="usage">
  {#each rows as [name, w]}
    {@const p = days(name)}
    {#if p}
      <div
        class="u week {level(w)}"
        title={weekTitle(w, p, name.toLowerCase())}
      >
        <span class="name">{t(name)}</span>
        <span class="days">
          <span class="bar7" class:many={p.days.length > 7}>
            {#each p.days as d (d.from.getTime())}
              <i class:current={d.current} style="background-image: {d.bg}" title={t('From {when}', { when: d.from.toLocaleDateString(locale(), { weekday: 'long', hour: '2-digit', minute: '2-digit' }) })}>
                {#if d.current}<span class="now" style="left: {d.nowAt}%"></span>{/if}
              </i>
            {/each}
          </span>
          <span class="labels">{#each p.days as d (d.from.getTime())}<span class:current={d.current}>{d.label}</span>{/each}</span>
        </span>
        <span class="pct">{Math.round(w.utilization)}%</span>
        <span class="reset {pace(p.ahead)}">{t('{pct}%/day left', { pct: p.perDay.toFixed(1) })}</span>
      </div>
    {:else}
      <div class="u {level(w)}" title="{t('{pct}% used', { pct: Math.round(w.utilization) })}{w.resets_at ? ` · ${resets(w)}` : ''}">
        <span class="name">{t(name)}</span>
        <span class="bar"><i style="width: {Math.min(100, w.utilization)}%"></i></span>
        <span class="pct">{Math.round(w.utilization)}%</span>
        <span class="reset">{resets(w)}</span>
      </div>
    {/if}
  {:else}
    <div class="u off" title={error ?? ''}>
      <span class="name">{t('Usage')}</span>
      <span class="msg">{error ?? t('loading…')}</span>
    </div>
  {/each}
  {#if rows.length && error}
    <!-- Last measurement kept on display; the endpoint refuses (429…) or fails for now. -->
    <div class="stale" title={error}>{t('Last measured {time}', { time: usage ? new Date(usage.fetched_at).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' }) : '?' })} · {error}</div>
  {/if}
</div>

<style>
  .stale { font-size: var(--fs-tag); color: var(--warn); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* Its CLI's colour (`--fill`), Claude's by default. */
  .usage { --fill: var(--cli-fill, var(--src-claude)); border-top: 1px solid var(--border); padding: 6px 12px; display: flex; flex-direction: column; gap: 4px; background: color-mix(in srgb, var(--fill) 6%, transparent); flex-shrink: 0; }
  .u { display: grid; grid-template-columns: 82px 1fr 34px 92px; gap: 8px; align-items: center; font-size: var(--fs-meta); color: var(--text-muted); }
  .u.off { grid-template-columns: 82px 1fr; }
  .name { color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .bar { height: 6px; border-radius: var(--r3); background: var(--border); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--fill); }
  .u.warn .bar i { background: var(--warn); }
  .u.ko .bar i { background: var(--ko); }
  .pct { font-family: var(--mono); text-align: right; color: var(--text); font-variant-numeric: tabular-nums; }
  .u.warn .pct { color: var(--warn); }
  .u.ko .pct { color: var(--ko); }
  .reset { font-size: var(--fs-tag); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .msg { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  /* Week (7 cells) or month (a cell a day): used (Claude colour), pace overrun (orange), "now" marker. */
  .u.week { align-items: start; }
  .days { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .bar7 { position: relative; display: flex; gap: 2px; height: 8px; }
  .u.week { --claude-fill: var(--fill); --over-fill: var(--warn); }
  .u.week.ko { --over-fill: var(--ko); }
  .bar7 i { position: relative; flex: 1; border-radius: var(--r2); background-color: var(--border); }
  .bar7 i.current { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--text-muted) 60%, transparent); }
  .bar7 .now { position: absolute; top: -2px; bottom: -2px; width: 2px; margin-left: -1px; border-radius: var(--r1); background: var(--text); pointer-events: none; }
  .labels { display: flex; gap: 2px; }
  .bar7.many, .bar7.many + .labels { gap: 1px; }
  .labels span { flex: 1; min-width: 0; overflow: visible; white-space: nowrap; font-size: 9px; line-height: 1; text-align: center; color: var(--text-muted); opacity: 0.8; }
  .labels span.current { color: var(--text); font-weight: 600; opacity: 1; }
  .reset.over { color: var(--warn); }
  .reset.under { color: var(--ok); }
</style>
