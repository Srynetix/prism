<script lang="ts">
  import { t } from '../i18n.svelte'
  import { locale } from '../locale'
  import { clickedOutside } from '../drawers'
  import { isPinned } from '../drawer-pins'
  import PinButton from './PinButton.svelte'
  import { closeEvent, eventView, openInCalendar, type Attendee } from '../agenda.svelte'
  import Markdown from './Markdown.svelte'
  import Icon from './Icon.svelte'
  import { live } from '../live.svelte'
  import type { IconName } from '../icons'
  import Logo from './Logo.svelte'
  import { claudeClick, meetingAction } from '../claude-actions'
  import { keys } from '../platform'

  /**
   * Detail of an agenda event in a side drawer: time, location, video link,
   * organizer, attendees and their responses, invitation text. Responding is done in
   * Calendar (EventKit does not allow it), or Google Calendar on the web: a button opens it on the
   * event. Prepare hands the meeting to an agent (see `meetingAction`). Escape closes.
   */
  const d = $derived(eventView.details)
  const calendarApp = $derived(live.snapshot?.agenda?.provider === 'google' ? 'Google Calendar' : 'Calendar')
  let opening = $state<string | null>(null)

  const day = (iso: string) => new Date(iso).toLocaleDateString(locale(), { weekday: 'long', day: 'numeric', month: 'long' })
  const hm = (iso: string) => new Date(iso).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
  const ICON: Partial<Record<Attendee['status'], IconName>> = { accepted: 'check', declined: 'fail' }
  const MARK: Partial<Record<Attendee['status'], string>> = { tentative: '?', pending: '·' }
  const LABEL: Record<Attendee['status'], string> = { accepted: t('accepted'), tentative: t('tentative'), declined: t('declined'), pending: t('no answer') }
  const counts = $derived(
    d ? (['accepted', 'tentative', 'declined', 'pending'] as const).map((s) => [s, d.attendees.filter((a) => a.status === s).length] as const).filter(([, n]) => n) : [],
  )
  /** Organizer first, then me, then the others by response. */
  const RANK = { accepted: 0, tentative: 1, pending: 2, declined: 3 }
  const people = $derived(d ? [...d.attendees].sort((a, b) => Number(b.me) - Number(a.me) || RANK[a.status] - RANK[b.status] || a.name.localeCompare(b.name)) : [])

  const prepare = $derived(d && !d.canceled ? meetingAction(d, `${day(d.start)}${d.all_day ? '' : ` ${hm(d.start)}–${hm(d.end)}`}`) : null)

  async function respond() {
    if (!eventView.id) return
    opening = null
    try {
      await openInCalendar(eventView.id)
    } catch (e) {
      opening = String(e)
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !eventView.id) return
    e.stopImmediatePropagation()
    closeEvent()
  }

  let drawerEl: HTMLElement | undefined = $state()
  /** A click outside the drawer closes it (see drawers.ts). */
  function onOutside(e: PointerEvent) {
    if (eventView.id && clickedOutside(e, drawerEl)) {
      closeEvent()
    }
  }
</script>

<svelte:window onkeydowncapture={onKey} onpointerdowncapture={onOutside} />

{#if eventView.id}
  <aside class="drawer" bind:this={drawerEl} data-drawer="52" data-pinned={isPinned('event')} aria-label={t('Event details')}>
    <header>
      <svg class="cal" viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="2.5" width="13" height="12" rx="2" /><path d="M1.5 6.5h13M5 1v3M11 1v3" /></svg>
      <span class="grow">{d ? d.calendar : t('Event')}</span>
      <PinButton kind="event" />
      <button class="icon" onclick={closeEvent} title={t('Close (Esc)')} aria-label={t('Close')}><Icon name="close" /></button>
    </header>
    <div class="body">
      {#if eventView.error}
        <p class="error">{eventView.error}</p>
      {:else if !d}
        <p class="muted">{t('Loading…')}</p>
      {:else}
        <h2 class:off={d.canceled}>{d.title}</h2>
        <p class="when">
          {day(d.start)}{#if !d.all_day}{' '}· {hm(d.start)} – {hm(d.end)}{:else}{' '}· {t('all day')}{/if}
          {#if d.canceled}<span class="flag">{t('canceled')}</span>{/if}
        </p>
        {#if d.location}<p class="where">{d.location}</p>{/if}

        <div class="actions">
          {#if d.join_url && !d.canceled}<a class="join" href={d.join_url} target="_blank" rel="noreferrer">{t('Join')}</a>{/if}
          {#if prepare}
            {#if prepare.href}<a class="prepare" href={prepare.href} onclick={claudeClick(prepare)} title={`${t('An agent gets you ready: topic, attendees, points to raise')}\n${t('{key}: launch at once', { key: keys('⇧-click') })}`}><Logo name="agent" size={10} />{prepare.label}</a>{:else}<span class="prepare off" title={prepare.why}><Logo name="agent" size={10} />{prepare.label}</span>{/if}
          {/if}
          {#if d.attendees.length}
            <span class="mine">
              {#if d.my_status}{t('Your answer:')} <b class={d.my_status}>{LABEL[d.my_status]}</b>{/if}
            </span>
            <button class="respond" onclick={respond} title={t('Accept, decline or answer tentatively in {app}', { app: calendarApp })}>{t('Respond in {app}', { app: calendarApp })} <Icon name="external" /></button>
          {/if}
        </div>
        {#if opening}<p class="error">{opening}</p>{/if}

        {#if d.organizer || d.attendees.length}
          <section>
            <h3>{t('People')} <small>{counts.map(([s, n]) => `${n} ${LABEL[s]}`).join(' · ')}</small></h3>
            {#if d.organizer}
              <p class="person"><span class="st org"><Icon name="star" /></span><span class="n">{d.organizer.name}</span><span class="r">{t('organizer')}</span></p>
            {/if}
            {#each people.filter((p) => p.email !== d.organizer?.email || !d.organizer) as p, i (`${p.email ?? p.name}-${i}`)}
              <p class="person" class:me={p.me} title={p.email ?? ''}>
                <span class="st {p.status}">{#if ICON[p.status]}<Icon name={ICON[p.status]!} />{:else}{MARK[p.status]}{/if}</span>
                <span class="n">{p.name}{p.me ? ` ${t('(you)')}` : ''}</span>
                <span class="r">{p.optional ? `${t('optional')} · ` : ''}{LABEL[p.status]}</span>
              </p>
            {/each}
          </section>
        {/if}

        {#if d.notes}
          <section>
            <h3>{t('Invitation')}</h3>
            <div class="notes"><Markdown text={d.notes} plain /></div>
          </section>
        {/if}
      {/if}
    </div>
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 52; width: min(520px, 92vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid color-mix(in srgb, var(--src-calendar) 45%, var(--border));
    box-shadow: var(--shadow-drawer);
  }
  header {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; border-bottom: 1px solid var(--border);
    background: var(--surface); box-shadow: inset 0 2px var(--src-calendar);
    font-size: var(--fs-meta); color: var(--text-muted);
  }
  .cal { width: 14px; height: 14px; fill: none; stroke: var(--src-calendar); stroke-width: 1.4; }
  .grow { flex: 1; }
  .icon { width: 24px; height: 24px; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font-size: 15px; cursor: pointer; }
  .icon:hover { background: var(--surface-muted); color: var(--text); }
  .body { flex: 1; min-height: 0; overflow: auto; padding: 12px 16px 24px; }
  h2 { margin: 0 0 4px; font-size: 15px; font-weight: 600; line-height: 1.35; }
  h2.off { text-decoration: line-through; color: var(--text-muted); }
  .when, .where { margin: 0 0 2px; font-size: var(--fs-body); }
  .when::first-letter { text-transform: uppercase; }
  .where { color: var(--text-muted); overflow-wrap: anywhere; }
  .flag { margin-left: 6px; color: var(--warn); }
  .actions { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin: 12px 0 4px; font-size: var(--fs-meta); }
  .join { padding: 2px 12px; border-radius: var(--r5); background: var(--src-calendar); color: #fff; font-weight: 500; }
  .prepare { display: inline-flex; align-items: center; gap: 5px; padding: 2px 10px; border: 1px solid color-mix(in srgb, var(--src-claude) 60%, var(--border)); border-radius: var(--r5); color: var(--text); }
  .prepare:hover { border-color: var(--src-claude); }
  .prepare.off { color: var(--text-muted); cursor: default; }
  .mine { color: var(--text-muted); }
  .mine b { font-weight: 600; }
  .accepted { color: var(--ok); } .tentative { color: var(--warn); } .declined { color: var(--ko); } .pending { color: var(--text-muted); }
  .respond { margin-left: auto; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text); font: inherit; padding: 2px 10px; cursor: pointer; }
  .respond:hover { border-color: var(--src-calendar); }
  section { padding-top: 12px; margin-top: 12px; border-top: 1px solid var(--border); }
  h3 { margin: 0 0 8px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  h3 small { font-weight: 400; text-transform: none; letter-spacing: 0; margin-left: 6px; }
  .person { display: flex; align-items: center; gap: 8px; margin: 0; padding: 2px 0; font-size: var(--fs-body); }
  .person.me .n { font-weight: 600; }
  .st { width: 14px; text-align: center; flex-shrink: 0; font-weight: 600; }
  .st.org { color: var(--src-calendar); }
  .st.org :global(svg) { fill: currentColor; }
  .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .r { flex-shrink: 0; font-size: var(--fs-tag); color: var(--text-muted); }
  .notes { font-size: var(--fs-body); }
  .notes :global(p) { margin: 0; }
  .error { color: var(--ko); font-size: var(--fs-body); white-space: pre-wrap; }
  .muted { color: var(--text-muted); font-size: var(--fs-body); }
</style>
