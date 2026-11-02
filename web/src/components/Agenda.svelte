<script lang="ts">
  import { t, tn } from '../i18n.svelte'
  import { clock } from '../clock.svelte'
  import { dateFormat } from '../locale'
  import BlockControls from './BlockControls.svelte'
  import { isCollapsed, isShort } from '../layout.svelte'
  import type { Agenda, CalendarEvent } from '../types'
  import { isBusy as busy, openEvent } from '../agenda.svelte'
  import { hasCore, invoke } from '../bridge'
  import { selectedDay, todayKey } from '../filter.svelte'
  import { live } from '../live.svelte'
  import { openSettings } from '../settings.svelte'

  /**
   * Today's agenda, read from the macOS Calendar or Google Calendar: the ongoing meeting highlighted (with its
   * progress), a countdown to the next one, a Join button for Teams / Zoom /
   * Meet. Declined or cancelled meetings stay visible, struck through.
   */
  let { agenda }: { agenda: Agenda | null | undefined } = $props()

  const now = $derived(clock(30_000).now)

  /**
   * Day shown: the one from the global date picker. Today comes from the snapshot (live);
   * another day is read on demand from Calendar, and kept for the session.
   */
  const day = $derived(selectedDay())
  const isToday = $derived(day === todayKey())
  const other = $state<{ day: string; events: CalendarEvent[] | null; error: string | null }>({ day: '', events: null, error: null })
  const cache = new Map<string, CalendarEvent[]>()
  $effect(() => {
    const d = day
    if (isToday || agenda?.access !== 'granted' || !hasCore()) return
    Object.assign(other, { day: d, events: cache.get(d) ?? null, error: null })
    if (cache.has(d)) return
    invoke<CalendarEvent[]>('calendar_day', { day: d })
      .then((events) => {
        cache.set(d, events)
        if (other.day === d) other.events = events
      })
      .catch((e) => other.day === d && (other.error = String(e)))
  })
  const events = $derived(isToday ? (agenda?.events ?? []) : other.day === day ? (other.events ?? []) : [])
  const loadingOther = $derived(!isToday && other.day === day && !other.events && !other.error)
  const dayLabel = $derived(isToday ? t('Today') : dateFormat({ weekday: 'short', day: 'numeric', month: 'short' }).format(new Date(`${day}T12:00`)))

  const ms = (iso: string) => new Date(iso).getTime()
  const allDay = $derived(events.filter((e) => e.all_day))
  const timed = $derived(events.filter((e) => !e.all_day))
  const current = $derived(timed.filter((e) => busy(e) && ms(e.start) <= now && now < ms(e.end)))
  const next = $derived(timed.find((e) => busy(e) && ms(e.start) > now))
  /** Short: the ongoing meeting and the next one (another day: the first three). */
  const short = $derived(isShort('agenda'))
  const listed = $derived(short ? (isToday ? [...current, ...(next ? [next] : [])] : timed.slice(0, 3)) : timed)
  const total = $derived(timed.filter(busy).reduce((sum, e) => sum + (ms(e.end) - ms(e.start)), 0))
  const left = $derived(timed.filter((e) => busy(e) && ms(e.end) > now).length)

  const hm = (iso: string) => dateFormat({ hour: '2-digit', minute: '2-digit' }).format(new Date(iso))
  function duration(ms: number) {
    const m = Math.round(ms / 60_000)
    return m < 60 ? `${m} min` : `${Math.floor(m / 60)}h${m % 60 ? String(m % 60).padStart(2, '0') : ''}`
  }
  const until = (iso: string) => duration(Math.max(0, ms(iso) - now))
  const progress = (e: CalendarEvent) => Math.min(100, Math.max(0, (100 * (now - ms(e.start))) / (ms(e.end) - ms(e.start))))
  const RESPONSE: Record<string, string> = { tentative: t('tentative'), pending: t('not answered'), declined: t('declined') }
  /** You can join a little before the start, and until the end. */
  const joinable = (e: CalendarEvent) => !!e.join_url && !e.canceled && ms(e.start) - 15 * 60_000 <= now && now < ms(e.end)
</script>

<section class="panel" style="--src: var(--src-calendar)" class:collapsed={isCollapsed('agenda')}>
  <h2>
    <span class="title">
      <svg class="cal" viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="2.5" width="13" height="12" rx="2" /><path d="M1.5 6.5h13M5 1v3M11 1v3" /></svg>
      {dayLabel}
    </span>
    {#if agenda?.access === 'granted'}
      <span class="head">
        <small>{tn(timed.filter(busy).length, '{n} meeting', '{n} meetings')} · {duration(total)}{isToday && left ? ` · ${t('{n} to go', { n: left })}` : ''}</small>
        {#if next && !current.length}<small class="next">{t('next in {time}', { time: until(next.start) })}</small>{/if}
      </span>
    {/if}
    <BlockControls id="agenda" />
  </h2>
  {#if live.snapshot?.calendar === false}
    <div class="empty">{t('The Calendar is off: Prism doesn’t read it.')} <button class="link" onclick={() => openSettings('calendar')}>{t('Turn on Agenda')}</button> {t('in Settings › Work › Calendar (macOS asks for access once).')}</div>
  {:else if !agenda && live.snapshot?.errors?.calendar}
    <div class="empty ko">{live.snapshot.errors.calendar}</div>
  {:else if !agenda}
    <div class="empty">{t('Reading Calendar…')}</div>
  {:else if agenda.access === 'signed_out'}
    <div class="empty">{t('Not signed in to Google.')} <button class="link" onclick={() => openSettings('calendar-google')}>{t('Sign in')}</button> {t('in Settings › Work › Calendar › Google Calendar.')}</div>
  {:else if agenda.access === 'denied' || agenda.access === 'restricted'}
    <div class="empty">
      {agenda.access === 'restricted' ? t('Calendar access is restricted on this Mac.') : t('Calendar access was refused.')} {t('Allow Prism in System Settings → Privacy & Security → Calendars, then reopen Prism.')}
    </div>
  {:else if agenda.access === 'unsupported'}
    <div class="empty">{t('The macOS Calendar is not available on this system.')} <button class="link" onclick={() => openSettings('calendar')}>{t('Read Google Calendar')}</button> {t('instead: Settings › Work › Calendar › Provider.')}</div>
  {:else if !isToday && other.day === day && other.error}
    <div class="empty">{other.error}</div>
  {:else if loadingOther}
    <div class="empty">{t('Reading Calendar…')}</div>
  {:else}
    <div class="list">
      {#if allDay.length && !short}
        <div class="allday">{#each allDay as e (e.id)}{#if hasCore()}<button class="chip" onclick={() => openEvent(e.id)} title="{e.title} · {e.calendar}">{e.title}</button>{:else}<span class="chip" title="{e.title} · {e.calendar}">{e.title}</span>{/if}{/each}</div>
      {/if}
      {#each listed as e (e.id)}
        {@const past = ms(e.end) <= now}
        {@const live = !past && ms(e.start) <= now}
        <div class="ev" class:past class:live class:off={!busy(e)} class:next={e === next}>
          <span class="time mono">{hm(e.start)}<small>{hm(e.end)}</small></span>
          <span class="what">
            {#if hasCore()}<button class="t" onclick={() => openEvent(e.id)} title="{e.title} · {t('details')}">{e.title}</button>{:else}<span class="t" title={e.title}>{e.title}</span>{/if}
            <span class="meta">
              {#if live}<b class="now">{t('now · {time} left', { time: until(e.end) })}</b>{:else if e === next}<b class="soon">{t('in {time}', { time: until(e.start) })}</b>{/if}
              {#if e.canceled}<span class="flag">{t('canceled')}</span>{:else if e.my_status && RESPONSE[e.my_status]}<span class="flag">{RESPONSE[e.my_status]}</span>{/if}
              {#if e.location && !e.location.startsWith('http')}<span title={e.location}>{e.location}</span>{/if}
              {#if e.attendees > 1}<span>{tn(e.attendees, '{n} person', '{n} people')}</span>{/if}
            </span>
            {#if live}<span class="bar"><i style="width: {progress(e)}%"></i></span>{/if}
          </span>
          {#if e.join_url && !e.canceled && !past}
            <a class="join" class:hot={joinable(e)} href={e.join_url} target="_blank" rel="noreferrer" title={t('Join the meeting')}>{t('Join')}</a>
          {/if}
        </div>
      {:else}
        <div class="empty">{short && isToday && timed.length ? t('No more meetings today.') : isToday ? t('Nothing in the calendar today.') : t('Nothing in the calendar that day.')}</div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .ko { color: var(--ko); }
  .link { padding: 0; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; text-underline-offset: 2px; }
  .cal { width: 14px; height: 14px; flex-shrink: 0; fill: none; stroke: var(--src-calendar); stroke-width: 1.4; vertical-align: -2px; }
  .head { display: flex; gap: 10px; align-items: baseline; min-width: 0; overflow: hidden; white-space: nowrap; }
  .head .next { color: var(--src-calendar); font-weight: 500; }
  .mono { font-family: var(--mono); }
  .list { flex: 1; min-height: 0; overflow: auto; padding: 4px 0 8px; }
  .allday { display: flex; flex-wrap: wrap; gap: 4px; padding: 4px 12px 6px; border-bottom: 1px solid var(--border); }
  button.chip { border: none; font: inherit; font-size: var(--fs-tag); color: inherit; cursor: pointer; }
  .chip { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 0 7px; border-radius: var(--r9); font-size: var(--fs-tag); background: color-mix(in srgb, var(--src-calendar) 14%, transparent); }
  .ev { display: flex; align-items: center; gap: 10px; padding: 5px 12px; border-left: 3px solid transparent; }
  .ev.live { border-left-color: var(--src-calendar); background: color-mix(in srgb, var(--src-calendar) 9%, transparent); }
  .ev.next { border-left-color: color-mix(in srgb, var(--src-calendar) 45%, transparent); }
  .ev.past { opacity: 0.45; }
  .ev.off .t { text-decoration: line-through; color: var(--text-muted); }
  .time { display: flex; flex-direction: column; width: 38px; flex-shrink: 0; font-size: var(--fs-mono); line-height: 1.2; }
  .time small { font-size: var(--fs-tag); color: var(--text-muted); }
  /* On the phone the time takes what it needs (09:30 AM on one line). */
  :global(html.remote) .time { width: auto; min-width: 38px; white-space: nowrap; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  button.t { border: none; background: none; padding: 0; font: inherit; color: inherit; text-align: left; cursor: pointer; }
  button.t:hover { color: var(--src-calendar); }
  .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-body); }
  .meta { display: flex; gap: 8px; min-width: 0; overflow: hidden; white-space: nowrap; font-size: var(--fs-tag); color: var(--text-muted); }
  .meta > span { overflow: hidden; text-overflow: ellipsis; }
  .now, .soon { color: var(--src-calendar); font-weight: 600; }
  .flag { color: var(--warn); }
  .bar { height: 3px; margin-top: 3px; border-radius: var(--r2); background: var(--surface-muted); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--src-calendar); }
  .join { flex-shrink: 0; padding: 1px 9px; border-radius: var(--r5); border: 1px solid color-mix(in srgb, var(--src-calendar) 50%, var(--border)); font-size: var(--fs-meta); color: var(--src-calendar); }
  .join.hot { background: var(--src-calendar); border-color: var(--src-calendar); color: #fff; font-weight: 500; }
</style>
