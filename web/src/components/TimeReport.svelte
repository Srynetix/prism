<script lang="ts">
  import { t, tn } from '../i18n.svelte'
  import { columns, grip } from '../columns.svelte'
  import { locale } from '../locale'
  import Icon from './Icon.svelte'
  import type { TimeEntry } from '../types'
  import { historyStart, todayKey } from '../filter.svelte'
  import { addDays, addMonths, byDay, byTicket, hours, isWorkday, monthOf, monthWeeks, targetOf, tone, weekOf } from '../timesheet'

  /**
   * Week and Month views of the time tracker. Week: a timesheet, one row per ticket, one column per
   * day. Month: a calendar of the days, the week totals, then the time per ticket. ‹ › move within
   * the history read (`historyStart`); a day opens the Day view on it.
   */
  let { entries, mode, day, dayMs, onDay }: { entries: TimeEntry[]; mode: 'week' | 'month'; day: string; dayMs: number; onDay: (k: string) => void } = $props()

  /** A day of the week or month shown, the viewed day at first. */
  let anchor = $state('')
  $effect.pre(() => {
    anchor = day
  })

  const today = $derived(todayKey())
  const days = $derived(byDay(entries))
  const ms = (k: string) => days.get(k) ?? 0

  const week = $derived(weekOf(anchor || day))
  const weeks = $derived(monthWeeks(anchor || day))
  const month = $derived(monthOf(anchor || day))
  const inMonth = $derived(weeks.flat().filter((k) => monthOf(k) === month))
  /** The days shown: the week's, or the month's own. */
  const shown = $derived(mode === 'week' ? week : inMonth)
  const rows = $derived(byTicket(entries, shown))
  const logged = $derived(shown.reduce((n, k) => n + ms(k), 0))
  /** Target of the period, and up to today for the current one. */
  const target = $derived(targetOf(shown, dayMs))
  const targetToDate = $derived(targetOf(shown.filter((k) => k <= today), dayMs))
  const missing = $derived(shown.filter((k) => isWorkday(k) && k < today && ms(k) < dayMs))

  const first = $derived(shown[0])
  const last = $derived(shown.at(-1) ?? first)
  const canBack = $derived(first > historyStart())
  const canNext = $derived(last < today)
  function move(n: number) {
    anchor = mode === 'week' ? addDays(week[0], 7 * n) : addMonths(month, n)
  }

  const dayLabel = (k: string) => new Date(`${k}T12:00`).toLocaleDateString(locale(), { weekday: 'short', day: 'numeric' })
  const weekday = (k: string) => new Date(`${k}T12:00`).toLocaleDateString(locale(), { weekday: 'narrow' })
  const longDay = (k: string) => new Date(`${k}T12:00`).toLocaleDateString(locale(), { weekday: 'long', day: 'numeric', month: 'long' })
  const title = $derived(
    mode === 'week'
      ? `${new Date(`${week[0]}T12:00`).toLocaleDateString(locale(), { day: 'numeric', month: 'short' })} – ${new Date(`${week[6]}T12:00`).toLocaleDateString(locale(), { day: 'numeric', month: 'short', year: 'numeric' })}`
      : new Date(`${month}-01T12:00`).toLocaleDateString(locale(), { month: 'long', year: 'numeric' }),
  )
  const target_ = (v: number) => `${Math.round((v / 3600_000) * 10) / 10}h`
  /** A cell's time, empty when nothing is logged. */
  const cell = (v: number | undefined) => (v ? hours(v) : '')
</script>

<div class="report">
  <div class="nav">
    <button onclick={() => move(-1)} disabled={!canBack} aria-label={mode === 'week' ? t('Previous week') : t('Previous month')} title={canBack ? (mode === 'week' ? t('Previous week') : t('Previous month')) : t('Older entries are not read (Settings › Work › Tracker, history)')}><Icon name="chevronLeft" /></button>
    <b>{title}</b>
    <button onclick={() => move(1)} disabled={!canNext} aria-label={mode === 'week' ? t('Next week') : t('Next month')} title={mode === 'week' ? t('Next week') : t('Next month')}><Icon name="chevronRight" /></button>
    <span class="sum" class:ok={logged >= target} class:warn={logged < targetToDate && missing.length > 0}>
      <b>{hours(logged)}</b>/{target_(target)}
      {#if targetToDate < target}<span class="muted"> · {t('{time} to date', { time: target_(targetToDate) })}</span>{/if}
    </span>
  </div>
  {#if missing.length}
    <div class="missing">
      {tn(missing.length, '{n} incomplete day:', '{n} incomplete days:')}
      {#each missing as k, i (k)}{#if i}{', '}{/if}<button class="link" onclick={() => onDay(k)} title="{longDay(k)} · {hours(ms(k))}">{dayLabel(k)}</button>{/each}
    </div>
  {/if}

  {#if mode === 'week'}
    <div class="sheet">
      <table {@attach columns('timesheet')}>
        <thead>
          <tr>
            <th class="name" {@attach grip}></th>
            {#each week as k (k)}
              <th class:off={!isWorkday(k)} {@attach grip}><button class="day" onclick={() => onDay(k)} disabled={k > today} title={t('Open {name}', { name: longDay(k) })}>{dayLabel(k)}</button></th>
            {/each}
            <th>{t('Total')}</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as r (r.key)}
            <tr>
              <td class="name" title="{r.ticket ? `${r.ticket} · ` : ''}{r.name}">
                {#if r.ticket}<a class="mono" href={r.url ?? '#'} target="_blank" rel="noreferrer">{r.ticket}</a> {/if}<span class="muted">{r.name}</span>
              </td>
              {#each week as k (k)}<td class="num" class:off={!isWorkday(k)}>{cell(r.days.get(k))}</td>{/each}
              <td class="num strong">{hours(r.total)}</td>
            </tr>
          {:else}
            <tr><td class="empty" colspan="9">{t('No time logged this week.')}</td></tr>
          {/each}
        </tbody>
        <tfoot>
          <tr>
            <td class="name">{t('Day')}</td>
            {#each week as k (k)}<td class="num {tone(k, ms(k), dayMs, today)}" class:off={!isWorkday(k)}>{cell(ms(k)) || (k <= today && isWorkday(k) ? '—' : '')}</td>{/each}
            <td class="num strong">{hours(logged)}</td>
          </tr>
        </tfoot>
      </table>
    </div>
  {:else}
    <div class="cal">
      {#each weeks[0] as k (k)}<span class="wh">{weekday(k)}</span>{/each}
      <span class="wh">{t('Week')}</span>
      {#each weeks as w (w[0])}
        {#each w as k (k)}
          {@const v = ms(k)}
          {#if monthOf(k) === month}
            <button class="c {tone(k, v, dayMs, today)}" class:off={!isWorkday(k)} class:today={k === today} disabled={k > today} onclick={() => onDay(k)} title="{longDay(k)} · {hours(v)}">
              <span class="d">{Number(k.slice(8))}</span>
              <span class="h mono">{cell(v)}</span>
              <span class="bar"><i style="width: {Math.min(100, (v / dayMs) * 100)}%"></i></span>
            </button>
          {:else}
            <span class="c out"></span>
          {/if}
        {/each}
        {@const own = w.filter((k) => monthOf(k) === month)}
        {@const wv = own.reduce((n, k) => n + ms(k), 0)}
        <span class="c wk mono" class:ok={wv >= targetOf(own, dayMs) && targetOf(own, dayMs) > 0} title={t('Week of {day}', { day: longDay(w[0]) })}>{cell(wv)}</span>
      {/each}
    </div>
    <div class="tickets">
      {#each rows as r (r.key)}
        <div class="t" title="{r.ticket ? `${r.ticket} · ` : ''}{r.name}">
          {#if r.ticket}<a class="mono" href={r.url ?? '#'} target="_blank" rel="noreferrer">{r.ticket}</a>{/if}
          <span class="grow muted">{r.name}</span>
          <span class="share"><i style="width: {logged ? (r.total / logged) * 100 : 0}%"></i></span>
          <span class="mono">{hours(r.total)}</span>
        </div>
      {:else}
        <div class="empty">{t('No time logged this month.')}</div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .report { display: flex; flex-direction: column; min-height: 0; overflow: auto; }
  .nav { display: flex; align-items: center; gap: 6px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--surface-muted); font-size: var(--fs-body); }
  .nav button { display: inline-flex; align-items: center; padding: 2px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .nav button:hover:not(:disabled) { color: var(--text); border-color: var(--border); }
  .nav button:disabled { opacity: 0.35; cursor: default; }
  .nav > b { font-weight: 500; text-transform: capitalize; }
  .sum { margin-left: auto; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .sum b { color: var(--text); font-weight: 500; }
  .sum.ok b { color: var(--ok); }
  .sum.warn b { color: var(--warn); }
  .missing { padding: 6px 12px; border-bottom: 1px solid var(--border); font-size: var(--fs-meta); color: var(--warn); }
  .link { padding: 0; border: none; background: none; color: inherit; font: inherit; text-decoration: underline dotted; cursor: pointer; }
  .link:hover { color: var(--text); }
  .muted { color: var(--text-muted); }
  .mono { font-family: var(--font-mono); }
  a.mono { color: var(--info); text-decoration: none; }
  td.name a.mono { margin-right: 6px; }
  a.mono:hover { text-decoration: underline; }
  .empty { padding: 10px 12px; color: var(--text-muted); font-size: var(--fs-body); }

  .sheet { overflow: auto; }
  table { width: 100%; border-collapse: collapse; font-size: var(--fs-meta); font-variant-numeric: tabular-nums; }
  th, td { padding: 4px 5px; border-bottom: 1px solid var(--border); white-space: nowrap; }
  th { font-weight: 500; color: var(--text-muted); text-align: right; }
  th.name, td.name { text-align: left; padding-left: 12px; }
  td.name { max-width: 170px; overflow: hidden; text-overflow: ellipsis; }
  th:last-child, td:last-child { padding-right: 12px; }
  .num { text-align: right; font-family: var(--font-mono); }
  .strong { color: var(--text); font-weight: 500; }
  .off { background: color-mix(in srgb, var(--surface-muted) 60%, transparent); }
  .day { padding: 0; border: none; background: none; color: inherit; font: inherit; cursor: pointer; }
  .day:hover:not(:disabled) { color: var(--info); }
  .day:disabled { cursor: default; opacity: 0.5; }
  tfoot td { border-bottom: none; color: var(--text-muted); }
  tfoot .ok { color: var(--ok); }
  tfoot .warn { color: var(--warn); }

  .cal { display: grid; grid-template-columns: repeat(7, 1fr) 0.9fr; gap: 3px; padding: 8px 12px; border-bottom: 1px solid var(--border); }
  .wh { font-size: var(--fs-tag); color: var(--text-muted); text-align: center; }
  .c { position: relative; display: flex; flex-direction: column; align-items: flex-start; gap: 1px; min-height: 40px; padding: 3px 5px 5px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; overflow: hidden; }
  .c:hover:not(:disabled) { border-color: var(--info); }
  .c:disabled { opacity: 0.35; cursor: default; }
  .c.off { background: var(--surface-muted); }
  .c.today { border-color: var(--accent); }
  .c.out { border-color: transparent; background: none; cursor: default; }
  .c .d { color: var(--text); }
  .c .h { font-size: var(--fs-tag); }
  .c.ok .h { color: var(--ok); }
  .c.warn .h { color: var(--warn); }
  .c.warn { border-color: color-mix(in srgb, var(--warn) 45%, var(--border)); }
  .c .bar { position: absolute; left: 0; right: 0; bottom: 0; height: 2px; }
  .c .bar i { display: block; height: 100%; background: var(--text-muted); }
  .c.ok .bar i { background: var(--ok); }
  .c.warn .bar i { background: var(--warn); }
  .c.wk { align-items: flex-end; justify-content: center; border-color: transparent; background: none; color: var(--text-muted); cursor: default; }
  .c.wk.ok { color: var(--ok); }

  .tickets { display: flex; flex-direction: column; }
  .t { display: flex; align-items: center; gap: 8px; padding: 4px 12px; border-bottom: 1px solid var(--border); font-size: var(--fs-body); }
  .t .grow { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .share { width: 60px; height: 4px; border-radius: var(--r3); background: var(--border); overflow: hidden; flex-shrink: 0; }
  .share i { display: block; height: 100%; background: var(--accent); }
  .t .mono:last-child { width: 44px; text-align: right; font-size: var(--fs-meta); }
</style>
