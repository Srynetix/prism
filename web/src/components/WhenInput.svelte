<script lang="ts">
  import { t } from '../i18n.svelte'
  import { presets } from '../notes.svelte'
  import { parseWhen, whenLabel } from '../when'
  import { dayKey } from '../calendar'
  import { anchored, dismissable } from '../popup'
  import Calendar from './Calendar.svelte'
  import Icon from './Icon.svelte'

  /**
   * A reminder's due time: proposed times in one click, a field that reads plain words
   * (`14:30`, `demain 9h`, `+20m`, `lun 10h`, `12/10 15h`) with what it understood right
   * below, and a calendar of the app's own as a last resort (a day, then a time). `at`: the chosen time (null: none yet);
   * `onenter`: Enter in the field, with a time understood.
   */
  let { at = $bindable(null), now, onenter, autofocus = false, empty = '' }: { at?: Date | null; now: number; onenter?: (at: Date) => void; autofocus?: boolean; empty?: string } = $props()

  let text = $state('')
  let calButton: HTMLButtonElement | undefined = $state()
  const parsed = $derived(text.trim() ? parseWhen(text, new Date(now)) : null)

  function type(value: string) {
    text = value
    const d = value.trim() ? parseWhen(value, new Date(now)) : null
    if (d) at = d
  }
  function pick(d: Date) {
    at = d
    text = ''
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Enter') return
    e.preventDefault()
    e.stopPropagation()
    if (parsed) onenter?.(parsed)
  }
  /** The calendar: the day picked in it and the time typed (`9:30`, `9h30`, `930`, `9`). */
  let calendar = $state<{ day: string; time: string } | null>(null)
  const two = (n: number) => String(n).padStart(2, '0')
  function openCalendar() {
    const from = at ?? new Date(now)
    calendar = { day: dayKey(from), time: at ? `${two(at.getHours())}:${two(at.getMinutes())}` : '09:00' }
  }
  const closeCalendar = () => (calendar = null)
  function chosenOf(c: { day: string; time: string }): Date | null {
    const m = /^(\d{1,2})(?:\s*[:h.]?\s*(\d{2}))?$/i.exec(c.time.trim())
    if (!m || +m[1] > 23 || +(m[2] ?? 0) > 59) return null
    const d = new Date(`${c.day}T00:00`)
    d.setHours(+m[1], +(m[2] ?? 0), 0, 0)
    return d
  }
  const chosen = $derived(calendar ? chosenOf(calendar) : null)
  const future = $derived(!!chosen && chosen.getTime() > now)
  function apply() {
    if (!chosen || !future) return
    pick(chosen)
    closeCalendar()
    calButton?.focus()
  }
  /** Esc closes the calendar only, not the popup it is in. */
  function calendarKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation()
      closeCalendar()
      calButton?.focus()
    }
  }
  const focusGrid = (el: HTMLElement) => el.querySelector<HTMLElement>('.month')?.focus()

  const focus = (el: HTMLInputElement) => {
    if (autofocus) el.focus()
  }
</script>

<div class="when-in">
  <div class="chips">
    {#each presets(new Date(now)) as p (p.label)}
      <button type="button" class:on={!!at && !text && at.getTime() === p.at.getTime()} onclick={() => pick(p.at)}>{p.label}</button>
    {/each}
  </div>
  <div class="line">
    <input class="txt" value={text} oninput={(e) => type(e.currentTarget.value)} {onkeydown} use:focus placeholder="14:30 · demain 9h · +20m · lun 10h · 12/10 15h" spellcheck="false" aria-label={t('When')} />
    <button bind:this={calButton} type="button" class="cal" class:on={!!calendar} onclick={() => (calendar ? closeCalendar() : openCalendar())} title={t('Pick a date and time')} aria-haspopup="dialog" aria-expanded={!!calendar}><Icon name="calendar" /></button>
  </div>
  <small class="read" class:bad={!!text.trim() && !parsed}>
    {#if text.trim() && !parsed}{t('Not understood: try 14:30, demain 9h, +20m, lun 10h')}{:else if at}→ {whenLabel(at, new Date(now))}{:else if empty}{empty}{:else}&nbsp;{/if}
  </small>
  {#if calendar}
    {@const c = calendar}
    <div class="when-cal" role="dialog" aria-label={t('Pick a date and time')} tabindex="-1" onkeydown={calendarKey} {@attach anchored(() => calButton, { align: 'end' })} {@attach dismissable(closeCalendar, { trigger: () => calButton })} {@attach focusGrid}>
      <Calendar value={c.day} min={dayKey(new Date(now))} today={dayKey(new Date(now))} onpick={(key) => (c.day = key)} />
      <div class="time">
        <input value={c.time} oninput={(e) => (c.time = e.currentTarget.value)} onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); apply() } }} placeholder="09:00" spellcheck="false" aria-label={t('Time')} />
        <button type="button" class="ok" disabled={!future} onclick={apply} title={chosen ? whenLabel(chosen, new Date(now)) : t('Not understood: try 14:30, demain 9h, +20m, lun 10h')}>OK</button>
      </div>
      <small class="read" class:bad={!future}>{#if !chosen}{t('Not understood: try 14:30, demain 9h, +20m, lun 10h')}{:else if !future}{t('Already past')}{:else}→ {whenLabel(chosen, new Date(now))}{/if}</small>
    </div>
  {/if}
</div>

<style>
  .when-in { display: flex; flex-direction: column; gap: 4px; font-size: var(--fs-tag); }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chips button { padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font: inherit; line-height: 18px; cursor: pointer; }
  .chips button:hover, .chips button.on { color: var(--text); border-color: var(--src-alert); }
  .line { position: relative; display: flex; align-items: center; gap: 4px; }
  .txt { flex: 1; min-width: 0; padding: 2px 6px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-meta); outline: none; }
  .txt:focus { border-color: var(--src-alert); }
  .cal { display: inline-flex; padding: 2px 5px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text-muted); cursor: pointer; }
  .cal:hover, .cal.on { color: var(--text); }
  .when-cal { position: fixed; z-index: 70; display: flex; flex-direction: column; gap: 6px; width: 232px; padding: 8px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); color: var(--text); outline: none; }
  .time { display: flex; gap: 6px; }
  .time input { flex: 1; min-width: 0; padding: 3px 6px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-body); font-variant-numeric: tabular-nums; outline: none; }
  .time input:focus { border-color: var(--src-alert); }
  .ok { padding: 3px 12px; border: none; border-radius: var(--r5); background: var(--accent); color: var(--on-accent); font: inherit; font-size: var(--fs-meta); font-weight: 600; cursor: pointer; }
  .ok:disabled { opacity: 0.4; cursor: default; }
  .read { color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .read.bad { color: var(--ko); }
</style>
