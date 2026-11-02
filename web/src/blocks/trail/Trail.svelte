<script lang="ts">
  import { locale } from '../../locale'
  import { t, tn } from '../../i18n.svelte'
  import { hasCore, invoke } from '../../bridge'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import { call, type BlockProps } from '../block'
  import { isCollapsed, isShort, pref, setPref } from '../../layout.svelte'
  import { selectedDay, todayKey } from '../../filter.svelte'
  import { canPreview, openTicket } from '../../ticket-view.svelte'
  import { openMr } from '../../mr-view.svelte'
  import { copy, toast } from '../../toast.svelte'
  import { byTicket, daily, withoutTicket, dailyText, mrLabel, previousWorkday, timeline, toLog, traces, type Row, type TicketTime, type TraceKind, type TrailState } from './trail'

  /**
   * Day trail: the day picked in the header (today by default) rebuilt from its traces. Timeline: in
   * order, alike traces in a row grouped, stretches without any trace flagged. By ticket: the time
   * Prism estimates against the time logged, one click logs the difference. Daily: the standup's
   * text from the day before's traces and what waits today, to edit and paste.
   */
  let { state: trail, error, snap }: BlockProps<TrailState> = $props()

  type Tab = 'timeline' | 'ticket' | 'daily'
  // The tab chosen last; never chosen: the daily in the morning, the time to log afterwards.
  let tab = $state<Tab>(pref<Tab | null>('trail.tab', null) ?? (new Date().getHours() < 12 ? 'daily' : 'ticket'))
  const pick = (next: Tab) => {
    tab = next
    setPref('trail.tab', next)
  }

  const day = $derived(selectedDay())
  const list = $derived(traces(trail, snap, day))
  const items = $derived(timeline(list, trail?.gap_minutes ?? 60, day))
  const ticketName = (id: string) => snap.sprint?.tickets.find((tk) => tk.id === id)?.name ?? snap.outside_sprint?.tickets.find((tk) => tk.id === id)?.name
  const perTicket = $derived(byTicket(list, ticketName))
  const traced = $derived(perTicket.tickets.reduce((n, g) => n + g.traced, 0) + perTicket.meetings)
  const logged = $derived(perTicket.tickets.reduce((n, g) => n + g.logged, 0))
  const gaps = $derived(items.flatMap((i) => ('gap' in i ? [i.gap] : [])))

  const primary = $derived(snap.repositories?.find((r) => r.primary)?.name)
  const mrRef = (repo: string | undefined, iid: number) => ({ iid, repo: repo && repo !== primary ? repo : null })

  const hm = (minutes: number) => (minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h${String(minutes % 60).padStart(2, '0')}`)
  const clock = (at: number) => new Date(at).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
  const dayLabel = (k: string) => (k === todayKey() ? t('Today') : new Date(`${k}T12:00`).toLocaleDateString(locale(), { weekday: 'short', day: 'numeric', month: 'short' }))

  const ICON: Record<TraceKind, string> = { commit: '●', mr_opened: '⇄', mr_merged: '⇄', mr_approved: '✓', mr_commented: '💬', ticket_moved: '◆', meeting: '◷', agent: '✳', time: '⏱' }
  const TONE: Record<TraceKind, string> = { commit: 'git', mr_opened: 'mr', mr_merged: 'mr', mr_approved: 'mr', mr_commented: 'mr', ticket_moved: 'tk', meeting: 'cal', agent: 'ai', time: 'time' }

  const NOUN: Record<TraceKind, [string, string]> = { commit: ['{n} commit', '{n} commits'], mr_opened: ['{n} MR opened', '{n} MRs opened'], mr_merged: ['{n} MR merged', '{n} MRs merged'], mr_approved: ['{n} approval', '{n} approvals'], mr_commented: ['{n} comment', '{n} comments'], ticket_moved: ['{n} status change', '{n} status changes'], meeting: ['{n} meeting', '{n} meetings'], agent: ['{n} agent', '{n} agents'], time: ['{n} entry logged', '{n} entries logged'] }
  const counted = (kind: TraceKind, n: number) => tn(n, NOUN[kind][0], NOUN[kind][1])

  /** A row's sentence: what was done, then its detail. */
  function sentence(r: Row): { what: string; detail: string } {
    const first = r.items[0]
    const n = r.items.length
    switch (r.kind) {
      case 'commit':
        return { what: n > 1 ? t('{n} commits on {branch}', { n, branch: first.branch ?? first.repo ?? '' }) : first.title, detail: n > 1 ? r.items.map((i) => i.title).join(' · ') : (first.branch ?? first.repo ?? '') }
      case 'mr_opened':
        return { what: t('Opened {mr}', { mr: mrLabel(first.mr) }), detail: first.title }
      case 'mr_merged':
        return { what: t('Merged {mr}', { mr: mrLabel(first.mr) }), detail: first.title }
      case 'mr_approved':
        return { what: t('Approved {mr}', { mr: mrLabel(first.mr) }), detail: first.title }
      case 'mr_commented':
        return { what: `${first.others === false ? t('Commented {mr}', { mr: mrLabel(first.mr) }) : t('Reviewed {mr}', { mr: mrLabel(first.mr) })} · ${tn(n, '{n} comment', '{n} comments')}`, detail: first.title }
      case 'ticket_moved':
        return { what: `${first.ticket} ${first.from} → ${first.to}`, detail: first.title }
      case 'meeting':
        return { what: first.title, detail: hm(first.minutes ?? 0) }
      case 'agent':
        return { what: t('Agent: {title}', { title: first.ticket ? withoutTicket(first.title, first.ticket) : first.title }), detail: first.end && first.end > first.t ? t('ran {time}', { time: hm(Math.round((first.end - first.t) / 60_000)) }) : '' }
      case 'time':
        return { what: first.ticket ? t('Logged {time} on {ticket}', { time: hm(first.minutes ?? 0), ticket: first.ticket }) : t('Logged {time}', { time: hm(first.minutes ?? 0) }), detail: first.title }
    }
  }

  /** A row leads to its MR, or its ticket's preview. */
  function open(r: Row) {
    const first = r.items[0]
    if (first.mr) openMr(mrRef(first.repo, first.mr))
    else if (first.ticket && canPreview()) openTicket(first.ticket)
  }
  const openable = (r: Row) => !!r.items[0].mr || (!!r.items[0].ticket && canPreview())

  /* ---------- Logging ---------- */

  let logging = $state<string | null>(null)
  async function log(g: TicketTime, minutes = toLog(g)) {
    if (!minutes || !hasCore()) return
    logging = g.ticket
    try {
      await invoke('log_time', { entry: { ticket: g.ticket, start: new Date(g.first).toISOString(), minutes, description: '' } })
      toast(t('Logged {time} on {ticket}', { time: hm(minutes), ticket: g.ticket }))
    } catch (e) {
      toast(t('Not logged: {error}', { error: String(e) }), 5000)
    } finally {
      logging = null
    }
  }
  async function logAll() {
    for (const g of perTicket.tickets.filter((g) => toLog(g))) await log(g)
  }
  const leftToLog = $derived(perTicket.tickets.reduce((n, g) => n + toLog(g), 0))

  /* ---------- Daily ---------- */

  const generated = $derived(dailyText(daily(trail, snap, day)))
  /** Your edits, kept for that day until regenerated. */
  let saved = $state(pref<{ day: string; text: string } | null>('trail.daily', null))
  const text = $derived(saved?.day === day ? saved.text : generated)
  const edit = (value: string) => {
    saved = { day, text: value }
    setPref('trail.daily', saved)
  }
  const regenerate = () => {
    saved = null
    setPref('trail.daily', null)
  }

  let refreshing = $state(false)
  async function refresh() {
    refreshing = true
    try {
      await call('trail', 'refresh')
    } finally {
      setTimeout(() => (refreshing = false), 1500)
    }
  }
</script>

<section class="panel" style="--src: var(--accent)" class:collapsed={isCollapsed('trail')}>
  <h2>
    <span class="title"><Icon name="clock" />{t('Day trail')}</span>
    {#if trail}
      <small title={t('{n} traces · {traced} traced · {logged} logged', { n: list.length, traced: hm(traced), logged: hm(logged) })}>{dayLabel(day)} · {tn(list.length, '{n} trace', '{n} traces')}{#if gaps.length && !isShort('trail')}{' '}· <span class="warn">{tn(gaps.length, '{n} gap', '{n} gaps')}</span>{/if}</small>
      {#if !isShort('trail')}
        <span class="seg" role="radiogroup" aria-label={t('Show')}>
          {#each [['timeline', 'Timeline'], ['ticket', 'By ticket'], ['daily', 'Daily']] as const as [id, label] (id)}
            <button role="radio" aria-checked={tab === id} class:on={tab === id} onclick={() => pick(id)}>{t(label)}</button>
          {/each}
        </span>
      {/if}
      <button class="icon" class:spin={refreshing} onclick={refresh} title={t('Read your traces again now')} aria-label={t('Refresh')}><Icon name="refresh" /></button>
    {/if}
    <BlockControls id="trail" />
  </h2>

  {#if !trail}
    <div class="empty">{t('The Day trail is off. Turn it on in Settings › Work › Day trail ([trail] enabled), then restart Prism.')}</div>
  {:else if isShort('trail')}
    <div class="short">
      <span><b>{hm(traced)}</b> {t('traced')}</span><span><b>{hm(logged)}</b> {t('logged')}</span>
      {#if leftToLog}<button class="btn primary" onclick={logAll} disabled={!!logging}>{t('Log {time}', { time: hm(leftToLog) })}</button>{/if}
    </div>
  {:else if tab === 'timeline'}
    <div class="body">
      {#each items as it, i ('gap' in it ? `gap-${i}` : it.row.key)}
        {#if 'gap' in it}
          <div class="gap"><span>{clock(it.gap.from)} → {clock(it.gap.to)} · {t('{time} without trace', { time: hm(Math.round((it.gap.to - it.gap.from) / 60_000)) })}</span></div>
        {:else}
          {@const s = sentence(it.row)}
          {#snippet line()}
            <span class="t mono">{clock(it.row.t)}</span>
            <span class="src {TONE[it.row.kind]}" aria-hidden="true">{ICON[it.row.kind]}</span>
            <span class="what">{s.what}{#if s.detail}<small>{s.detail}</small>{/if}</span>
            {#if it.row.items[0].ticket && it.row.kind !== 'ticket_moved'}<span class="tk mono">{it.row.items[0].ticket}</span>{/if}
          {/snippet}
          {#if openable(it.row)}
            <button class="ev link" class:meeting={it.row.kind === 'meeting'} onclick={() => open(it.row)}>{@render line()}</button>
          {:else}
            <div class="ev" class:meeting={it.row.kind === 'meeting'}>{@render line()}</div>
          {/if}
        {/if}
      {:else}
        <div class="empty">{day === todayKey() ? t('No trace yet today.') : t('No trace that day.')}{#if error}<br /><small>{error}</small>{/if}</div>
      {/each}
    </div>
  {:else if tab === 'ticket'}
    <div class="body">
      {#each perTicket.tickets as g (g.ticket)}
        <div class="grp">
          <div class="grp-h">{#if g.ticket}<span class="tk mono">{g.ticket}</span>{/if}<b>{g.title}</b></div>
          <div class="grp-ev">
            {#each Object.entries(g.traces.reduce((m, tr) => ((m[tr.kind] = (m[tr.kind] ?? 0) + 1), m), {} as Record<string, number>)) as [kind, n] (kind)}
              <span><span class="src {TONE[kind as TraceKind]}" aria-hidden="true">{ICON[kind as TraceKind]}</span>{counted(kind as TraceKind, n)}</span>
            {/each}
          </div>
          <div class="grp-f">
            <span class="muted">{t('traced')}</span><b>{hm(g.traced)}</b><span class="muted">· {t('logged')}</span><span class={g.logged ? 'ok' : 'warn'}>{g.logged ? hm(g.logged) : '—'}</span>
            {#if g.cost >= 0.01}<span class="muted" title={t('Its agents’ conversations, at the API prices')}>· {t('agents ≈ {cost}', { cost: `$${g.cost < 10 ? g.cost.toFixed(2) : Math.round(g.cost)}` })}</span>{/if}
            <span class="grow"></span>
            {#if toLog(g)}<button class="btn primary" onclick={() => log(g)} disabled={!hasCore() || logging === g.ticket} title={t('Log {time} on {ticket}, from {start}', { time: hm(toLog(g)), ticket: g.ticket, start: clock(g.first) })}>{t('Log {time}', { time: hm(toLog(g)) })}</button>{/if}
          </div>
        </div>
      {/each}
      {#if perTicket.meetings}
        <div class="grp"><div class="grp-h"><span class="src cal" aria-hidden="true">◷</span><b>{t('Meetings')}</b></div><div class="grp-f"><span class="muted">{hm(perTicket.meetings)}</span></div></div>
      {/if}
      {#if !perTicket.tickets.length && !perTicket.meetings}<div class="empty">{day === todayKey() ? t('No trace yet today.') : t('No trace that day.')}</div>{/if}
    </div>
    {#if perTicket.tickets.length}
      <div class="foot">
        <span><b>{hm(traced)}</b> {t('traced')} · <b>{hm(logged)}</b> {t('logged')}</span>
        {#if gaps.length}<span class="warn">· {t('{time} without trace', { time: hm(gaps.reduce((n, g) => n + Math.round((g.to - g.from) / 60_000), 0)) })}</span>{/if}
        <span class="grow"></span>
        {#if leftToLog}<button class="btn primary" onclick={logAll} disabled={!hasCore() || !!logging}>{t('Log all · {time}', { time: hm(leftToLog) })}</button>{/if}
      </div>
    {/if}
    <p class="note">{t('An estimate: each trace counts the time since the previous one (45 min at most), an agent its launch and its last activity. Nothing is logged without your click.')}</p>
  {:else}
    <div class="daily">
      <p class="muted">{t('For the standup of {day}: yesterday is {before}.', { day: dayLabel(day), before: dayLabel(previousWorkday(day)) })}</p>
      <textarea value={text} oninput={(e) => edit(e.currentTarget.value)} spellcheck="false" aria-label={t('The daily')}></textarea>
      <div class="acts">
        {#if saved?.day === day}<button class="btn" onclick={regenerate} title={t('Rebuild it from the traces: your edits are lost')}>{t('Regenerate')}</button>{/if}
        <span class="grow"></span>
        <button class="btn primary" onclick={() => copy(text, t('Daily copied'))}>{t('Copy')}</button>
      </div>
    </div>
  {/if}
</section>

<style>
  .body { flex: 1; min-height: 0; overflow: auto; padding: 4px 0; }
  .empty { padding: 14px 16px; color: var(--text-muted); font-size: var(--fs-body); }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .muted { color: var(--text-muted); }
  .warn { color: var(--warn); }
  .ok { color: var(--ok); }
  .grow { flex: 1; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { padding: 0 8px; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); font-weight: 400; cursor: pointer; }
  .seg button.on { background: color-mix(in srgb, var(--src) 18%, transparent); color: var(--text); }
  .icon { display: inline-grid; place-items: center; width: 22px; height: 22px; padding: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover { background: var(--surface-muted); color: var(--text); }
  .icon :global(svg) { width: 13px; height: 13px; }
  .spin :global(svg) { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .btn { padding: 1px 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); white-space: nowrap; cursor: pointer; }
  .btn.primary { border-color: color-mix(in srgb, var(--accent) 60%, var(--border)); color: var(--accent); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .src { display: inline-grid; place-items: center; width: 18px; height: 18px; flex-shrink: 0; border-radius: var(--r5); font-size: 10px; }
  .src.git { background: color-mix(in srgb, var(--text-muted) 22%, transparent); color: var(--text-muted); }
  .src.mr { background: color-mix(in srgb, var(--src-gitlab) 20%, transparent); color: var(--src-gitlab); }
  .src.tk { background: color-mix(in srgb, var(--src-clickup) 22%, transparent); color: var(--src-clickup); }
  .src.ai { background: color-mix(in srgb, var(--src-claude) 22%, transparent); color: var(--src-claude); }
  .src.cal { background: color-mix(in srgb, var(--src-calendar) 22%, transparent); color: var(--src-calendar); }
  .src.time { background: color-mix(in srgb, var(--accent) 22%, transparent); color: var(--accent); }
  .tk { color: var(--info); }

  .ev { display: grid; grid-template-columns: 42px 20px minmax(0, 1fr) auto; gap: 0 8px; align-items: start; padding: 4px 12px; }
  .ev.link { width: 100%; border: none; background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .ev:hover { background: var(--surface-muted); }
  .ev.meeting { background: color-mix(in srgb, var(--src-calendar) 7%, transparent); }
  .ev .t { color: var(--text-muted); padding-top: 1px; }
  .what { min-width: 0; font-size: var(--fs-body); line-height: 1.4; }
  .what small { display: block; overflow: hidden; color: var(--text-muted); font-size: var(--fs-meta); text-overflow: ellipsis; white-space: nowrap; }
  .gap { margin: 2px 12px 2px 62px; padding: 2px 8px; border: 1px dashed color-mix(in srgb, var(--warn) 50%, var(--border)); border-radius: var(--r6); color: var(--warn); font-size: var(--fs-meta); }

  .grp { padding: 7px 12px; border-bottom: 1px solid var(--border); }
  .grp-h { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .grp-h b { overflow: hidden; font-size: var(--fs-body); font-weight: 600; text-overflow: ellipsis; white-space: nowrap; }
  .grp-ev { display: flex; flex-wrap: wrap; gap: 3px 10px; margin: 4px 0 5px; color: var(--text-muted); font-size: var(--fs-meta); }
  .grp-ev > span { display: inline-flex; align-items: center; gap: 4px; }
  .grp-f { display: flex; align-items: center; gap: 6px; font-size: var(--fs-meta); }
  .foot { display: flex; align-items: center; gap: 6px; padding: 6px 12px; border-top: 1px solid var(--border); background: var(--surface-muted); font-size: var(--fs-meta); }
  .note { margin: 0; padding: 4px 12px 6px; color: var(--text-muted); font-size: var(--fs-tag); }
  .short { display: flex; align-items: center; gap: 12px; padding: 6px 12px; font-size: var(--fs-meta); }

  .daily { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 6px; padding: 8px 12px; }
  .daily p { margin: 0; font-size: var(--fs-meta); }
  textarea { flex: 1; min-height: 140px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: var(--fs-body)/1.55 var(--sans); resize: none; outline: none; }
  textarea:focus { border-color: var(--accent); }
  .acts { display: flex; gap: 6px; }
</style>
