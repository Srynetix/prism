<script lang="ts">
  import { t } from '../i18n.svelte'
  import Icon from './Icon.svelte'
  import { historyStart, filter, selectedDay, setDate, shiftDate, todayKey } from '../filter.svelte'
  import Calendar from './Calendar.svelte'
  import { dateFormat } from '../locale'
  import { anchored, dismissable, exclusive } from '../popup'

  /**
   * Day viewed by the dashboard: time entries and merged MRs. The day in the window's locale and
   * a calendar of the app's own (the system's date field differs between macOS and Linux), from
   * the history's start to today; ← → ↑ ↓ move in it, Enter picks, Esc closes.
   */
  const day = $derived(selectedDay())
  const isToday = $derived(!filter.date)
  const min = $derived(historyStart())
  const today = $derived(todayKey())
  const at = (key: string) => new Date(`${key}T12:00`)
  const label = $derived(dateFormat(at(day).getFullYear() === new Date().getFullYear() ? { weekday: 'short', day: 'numeric', month: 'short' } : { day: 'numeric', month: 'short', year: 'numeric' }).format(at(day)))

  let trigger: HTMLButtonElement | undefined = $state()
  let open = $state(false)
  const show = () => (open = true)
  const close = () => (open = false)
  function pick(key: string) {
    setDate(key)
    close()
  }
  const focusOnOpen = (el: HTMLElement) => el.querySelector<HTMLElement>('.month')?.focus()
</script>

<div class="dp" class:active={!isToday}>
  <button onclick={() => shiftDate(-1)} disabled={day <= min} aria-label={t('Previous day')} title={t('Previous day')}><Icon name="chevronLeft" /></button>
  <button bind:this={trigger} class="day" class:on={open} onclick={() => (open ? close() : show())} aria-haspopup="dialog" aria-expanded={open} aria-label={t('Day: {label}', { label })} title={t('Pick a day')}>{label}</button>
  <button onclick={() => shiftDate(1)} disabled={isToday} aria-label={t('Next day')} title={t('Next day')}><Icon name="chevronRight" /></button>
  {#if !isToday}<button class="today" onclick={() => setDate(todayKey())}>{t('Today')}</button>{/if}
</div>

{#if open}
  <div class="cal" role="dialog" aria-label={t('Pick a day')} tabindex="-1" {@attach anchored(() => trigger, { align: 'center' })} {@attach exclusive(close)} {@attach dismissable(close, { trigger: () => trigger })} {@attach focusOnOpen}>
    <Calendar value={day} {min} max={today} {today} onpick={pick} />
  </div>
{/if}

<style>
  .dp { display: flex; align-items: center; gap: 2px; height: 28px; padding: 0 4px; border: 1px solid var(--border); border-radius: var(--r6); flex-shrink: 0; }
  .dp.active { border-color: var(--warn); }
  button { border: none; background: none; color: var(--text-muted); cursor: pointer; font: inherit; font-size: 14px; padding: 0 4px; line-height: 1; }
  button:disabled { opacity: 0.3; cursor: default; }
  button:not(:disabled):hover { color: var(--text); }
  .day { min-width: 92px; height: 22px; border-radius: var(--r4); color: var(--text); font-size: var(--fs-body); font-variant-numeric: tabular-nums; }
  .day.on { background: var(--surface-muted); }
  .today { font-size: var(--fs-meta); color: var(--warn); }
  .cal { position: fixed; z-index: 60; width: 232px; padding: 8px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); color: var(--text); outline: none; }
</style>
