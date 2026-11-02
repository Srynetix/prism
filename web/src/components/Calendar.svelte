<script lang="ts">
  import { t } from '../i18n.svelte'
  import Icon from './Icon.svelte'
  import { addDays, monthGrid, weekStart } from '../calendar'
  import { dateFormat } from '../locale'

  /**
   * A month of the app's own, in the window's locale (the system's date fields differ between
   * macOS and Linux): `value` the day chosen (`YYYY-MM-DD`), the days from `min` to `max`
   * (included) can be picked. ← → ↑ ↓ move, Enter picks (`onpick`), the focus kept on the grid.
   * The header's day picker and a reminder's due time.
   */
  let { value, min = '', max = '', onpick, today }: { value: string; min?: string; max?: string; onpick: (key: string) => void; today: string } = $props()

  const at = (key: string) => new Date(`${key}T12:00`)
  const allowed = (key: string) => (!min || key >= min) && (!max || key <= max)
  /** The month shown (its 1st) and the day under the keyboard, from the day chosen. */
  let cursor = $state('')
  let month = $state('')
  $effect.pre(() => {
    cursor = value
    month = `${value.slice(0, 8)}01`
  })
  function shiftMonth(n: number) {
    const d = at(month)
    d.setMonth(d.getMonth() + n)
    month = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-01`
  }
  const first = $derived(weekStart())
  const weeks = $derived(month ? monthGrid(at(month).getFullYear(), at(month).getMonth(), first) : [])
  const heading = $derived(month ? dateFormat({ month: 'long', year: 'numeric' }).format(at(month)) : '')
  const weekdays = $derived(weeks[0]?.map((d) => dateFormat({ weekday: 'narrow' }).format(at(d.key))) ?? [])
  const canBack = $derived(!min || month.slice(0, 7) > min.slice(0, 7))
  const canNext = $derived(!max || month.slice(0, 7) < max.slice(0, 7))

  const STEP: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }
  function onKey(e: KeyboardEvent) {
    if (e.key in STEP) {
      e.preventDefault()
      const next = addDays(cursor, STEP[e.key])
      if (!allowed(next)) return
      cursor = next
      month = `${next.slice(0, 8)}01`
    } else if (e.key === 'Enter') {
      e.preventDefault()
      e.stopPropagation()
      if (allowed(cursor)) onpick(cursor)
    }
  }
</script>

<div class="month" role="grid" tabindex="0" onkeydown={onKey} aria-label={heading}>
  <div class="head">
    <button type="button" onclick={() => shiftMonth(-1)} disabled={!canBack} aria-label={t('Previous month')}><Icon name="chevronLeft" /></button>
    <b>{heading}</b>
    <button type="button" onclick={() => shiftMonth(1)} disabled={!canNext} aria-label={t('Next month')}><Icon name="chevronRight" /></button>
  </div>
  <div class="grid">
    {#each weekdays as w, i (i)}<span class="wd">{w}</span>{/each}
    {#each weeks.flat() as d (d.key)}
      <button type="button" class="d" class:out={!d.inMonth} class:sel={d.key === value} class:now={d.key === today} class:cur={d.key === cursor} disabled={!allowed(d.key)} onclick={() => onpick(d.key)} aria-label={dateFormat({ weekday: 'long', day: 'numeric', month: 'long' }).format(at(d.key))} aria-current={d.key === value ? 'date' : undefined}>{d.day}</button>
    {/each}
  </div>
</div>

<style>
  .month { outline: none; }
  button { border: none; background: none; color: var(--text-muted); cursor: pointer; font: inherit; font-size: 14px; padding: 0 4px; line-height: 1; }
  button:disabled { opacity: 0.3; cursor: default; }
  button:not(:disabled):hover { color: var(--text); }
  .head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 6px; }
  .head b { font-size: var(--fs-body); font-weight: 600; }
  .head button { height: 22px; }
  .grid { display: grid; grid-template-columns: repeat(7, 1fr); gap: 2px; }
  .wd { padding: 2px 0 4px; text-align: center; font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; }
  .d { height: 28px; padding: 0; border-radius: var(--r5); color: var(--text); font-size: var(--fs-body); font-variant-numeric: tabular-nums; }
  .d.out { color: var(--text-muted); }
  .d:not(:disabled):hover { background: var(--surface-muted); }
  .d.now { box-shadow: inset 0 0 0 1px var(--border); }
  .d.cur { box-shadow: inset 0 0 0 1px var(--accent); }
  .d.sel { background: var(--accent); color: var(--bg); font-weight: 600; }
  .d:disabled { opacity: 0.25; }
</style>
