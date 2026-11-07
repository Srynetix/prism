<script lang="ts">
  import { t as tr, tc, tn } from '../i18n.svelte'
  import { dateFormat } from '../locale'
  import { offers, onObsidian, ticketKey, trackerLogo } from '../tracker'
  import BlockControls from './BlockControls.svelte'
  import RefreshTickets from './RefreshTickets.svelte'
  import TrackerOff from './TrackerOff.svelte'
  import { trackerOn } from '../live.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import TaskType from './TaskType.svelte'
  import { openTicket, previewClick } from '../ticket-view.svelte'
  import Logo from './Logo.svelte'
  import type { OutsideSprint, Person, Snapshot, Sprint, StatusOption, Ticket } from '../types'
  import { isDone, statusColor, statusRank } from '../rules'
  import { isBlocked } from '../tickets'
  import TicketList from './TicketList.svelte'
  import ActionMenu, { type MenuSection } from './ActionMenu.svelte'
  import Icon from './Icon.svelte'
  import NewTicketForm from './NewTicketForm.svelte'
  import { ticketBoards } from '../new-ticket.svelte'
  import { ticketStep } from '../ticket-step'
  let {
    sprint, next = null, outside, me, filtered = false, full,
  }: { sprint: Sprint | null; next?: Sprint | null; outside?: OutsideSprint; me: Person | null; filtered?: boolean; full: Snapshot } = $props()

  /**
   * Current and Next tabs (remembered under `sprint.tab`): the next sprint and all its tickets,
   * to prepare what comes next without leaving the block. Without a next sprint, no tabs.
   */
  let tab = $state(pref<'current' | 'next'>('sprint.tab', 'current'))
  function setTab(t: 'current' | 'next') {
    tab = t
    setPref('sprint.tab', t)
  }
  const onNext = $derived(tab === 'next' && !!next)
  const sprintNo = (s: Sprint) => s.name.replace(/^.*?(Sprint \d+).*$/, '$1')

  /**
   * The block shows the sprint's work: neither EPICs (which group tasks), nor cross-cutting
   * tasks (time-logging entries: meetings, support…), subtasks included.
   */
  const timeTasks = $derived(new Set((full.time_tasks?.tickets ?? []).map((t) => t.task_id)))
  const hidden = (t: Ticket) => t.task_type?.toLowerCase() === 'epic' || timeTasks.has(t.task_id) || (!!t.parent && timeTasks.has(t.parent))
  const tickets = $derived((sprint?.tickets ?? []).filter((t) => !hidden(t)))
  const blocked = $derived(tickets.filter((t) => !isDone(t) && isBlocked(t)).length)
  const nextTickets = $derived((next?.tickets ?? []).filter((t) => !hidden(t)))

  /**
   * Tickets in progress (or further) outside the sprint, shown on demand ("Outside sprint",
   * remembered under `sprint.outside`) in a separate group, with their board and "Add to sprint".
   */
  let showOutside = $state(pref<boolean>('sprint.outside', false))
  function toggleOutside() {
    showOutside = !showOutside
    setPref('sprint.outside', showOutside)
  }
  /** A ticket added to the sprint from here leaves immediately, without waiting for the re-read. */
  const extra = $derived((outside?.tickets ?? []).filter((t) => !hidden(t) && !sprint?.tickets.some((s) => s.task_id === t.task_id)))
  const extraIds = $derived(new Set(extra.map((t) => t.task_id)))
  const listed = $derived(showOutside ? [...tickets, ...extra] : tickets)
  const statusesOf = (t: Ticket): StatusOption[] =>
    extraIds.has(t.task_id)
      ? (outside?.list_statuses[t.list?.id ?? ''] ?? [])
      : (sprint?.boards?.find((b) => b.id === t.list?.id)?.statuses ?? sprint?.statuses ?? [])
  const special = (t: Ticket) => (extraIds.has(t.task_id) ? { key: 'outside', name: tr('outside the sprint'), color: 'var(--violet)' } : null)

  /** Sprint progress: share of High and Urgent tickets done; none on Obsidian, whose sprint is its open cards. */
  const key = $derived(tickets.filter((t) => t.priority === 'urgent' || t.priority === 'high'))
  const progress = $derived(key.length && !onObsidian() ? Math.round((100 * key.filter(isDone).length) / key.length) : null)
  /** Short: the status bar and my open tickets, with their next step. */
  const mineOpen = $derived(tickets.filter((t) => !isDone(t) && !!me && t.assignees.some((a) => a.id === me.id)))
  const rank = statusRank
  const countByStatus = (list: Ticket[]) => {
    const m = new Map<string, number>()
    for (const t of list) m.set(t.status, (m.get(t.status) ?? 0) + 1)
    return [...m.entries()].sort((a, b) => rank(b[0]) - rank(a[0]))
  }
  const groups = $derived(countByStatus(tickets))
  const nextGroups = $derived(countByStatus(nextTickets))
  /** Sprint day (1 = first day) and its length, in calendar days; `null` without dates. */
  const DAY = 86_400_000
  const days = $derived.by(() => {
    if (!sprint?.start || !sprint.end) return null
    const start = new Date(sprint.start).getTime()
    const total = Math.max(1, Math.round((new Date(sprint.end).getTime() - start) / DAY))
    const day = Math.min(total, Math.max(1, Math.floor((Date.now() - start) / DAY) + 1))
    return { day, total }
  })
  /** Days before the next sprint starts; `null` without a date. */
  const startsIn = $derived(next?.start ? Math.max(0, Math.ceil((new Date(next.start).getTime() - Date.now()) / DAY)) : null)
  /** Narrow header ⋯ menu: what no longer fits on the line; on Next, its board. */
  const more = $derived<MenuSection[]>(onNext ? [
    { title: tr('Next sprint'), items: next?.url ? [{ key: 'board', label: tr('Open the board'), icon: 'external' as const, href: next.url, external: true }] : [] },
  ] : [
    {
      title: tr('Sprint'),
      items: [
        ...(sprint?.url ? [{ key: 'board', label: tr('Open the board'), icon: 'external' as const, href: sprint.url, external: true }] : []),
        ...(extra.length || showOutside
          ? [{ key: 'outside', label: showOutside ? tr('Hide tickets outside the sprint') : tr('Show tickets outside the sprint'), hint: `${extra.length}`, run: toggleOutside }]
          : []),
      ],
    },
  ])
  /** A new ticket, from the header's +: the form under it (`NewTicketForm`). */
  const canCreate = $derived(ticketBoards().length > 0)
  let creating = $state(false)

  /** Obsidian: the sprint is the open cards of several boards, one per repository. */
  const boards = $derived(new Set((sprint?.tickets ?? []).map((t) => t.list?.id ?? '')).size)
  const byBoard = { key: 'board', label: tr('Board'), of: (t: Ticket) => ({ name: t.list?.name ?? '', color: 'var(--src-clickup)', rank: 0 }) }
  const short = (d: string | null) => (d ? dateFormat({ day: 'numeric', month: 'numeric' }).format(new Date(d)) : '?')
</script>

{#snippet board(t: Ticket)}
  {#if extraIds.has(t.task_id) && t.list}<span class="board" title={tr('Not in the sprint: {board}', { board: t.list.name })}>{t.list.name}</span>
  {:else if t.repo && t.list && boards > 1}<span class="board" title={tr('Board: {board}', { board: t.list.name })}>{t.list.name}</span>{/if}
{/snippet}

<section class="panel" style="--src: var(--src-clickup)" class:collapsed={isCollapsed('sprint')}>
  <!-- Header: one line when wide; when narrow, title and actions, then a line of info (see the container queries). -->
  <h2>
    <span class="title"><Logo name={trackerLogo()} /><span class="name">{onNext && next ? sprintNo(next) : sprint ? sprintNo(sprint) : tr('Sprint')}</span></span>
    {#if next}
      <span class="tabs" role="tablist" aria-label={tr('Sprint')}>
        <button role="tab" aria-selected={!onNext} class:on={!onNext} onclick={() => setTab('current')}><span class="long">{tc('tickets', 'Current')}</span><span class="short">{tr('Now')}</span></button>
        <button role="tab" aria-selected={onNext} class:on={onNext} onclick={() => setTab('next')} title="{next.name} · {tn(nextTickets.length, '{n} ticket', '{n} tickets')}">{tr('Next')}</button>
      </span>
    {/if}
    {#if onNext && next}
      <small class="info wide">{short(next.start)} → {short(next.end)}{#if startsIn !== null}{' '}· {startsIn ? tn(startsIn, 'starts in {n} day', 'starts in {n} days') : tr('starts today')}{/if} · {tn(nextTickets.length, '{n} ticket', '{n} tickets')}{filtered ? ` ${tr('(filtered)')}` : ''}</small>
      <span class="grow"></span>
      {#if next.url}<a class="wide" href={next.url} target="_blank" rel="noreferrer">{tr('Board')} <Icon name="external" /></a>{/if}
    {:else if sprint}
      {#if progress !== null}<b class="progress" title={tr('{done} of {total} high and urgent tickets done', { done: key.filter(isDone).length, total: key.length })}>{progress}%</b>{/if}
      <small class="info wide">{#if sprint.start || sprint.end}{short(sprint.start)} → {short(sprint.end)} ·{' '}{/if}{#if days}{tr('day {day}/{days}', { day: days.day, days: days.total })} ·{' '}{/if}{tn(tickets.length, '{n} ticket', '{n} tickets')}{filtered ? ` ${tr('(filtered)')}` : ''}</small>
      {#if blocked}<span class="blocked-pill" title={tn(blocked, '{n} ticket waiting on unfinished ones', '{n} tickets waiting on unfinished ones')}><Icon name="lock" />{blocked > 1 ? tc('plural', '{n} blocked', { n: blocked }) : tr('{n} blocked', { n: blocked })}</span>{/if}
      <span class="grow"></span>
      {#if extra.length || showOutside}
        <button class="outside wide" class:on={showOutside} onclick={toggleOutside} aria-pressed={showOutside} title={tr('Tickets {statuses} that are not in this sprint', { statuses: outside?.statuses.join(', ') ?? '' })}>
          <span class="switch" aria-hidden="true"><i></i></span>{tr('Outside sprint')} <b>{extra.length}</b>
        </button>
      {/if}
      {#if sprint.url}<a class="wide" href={sprint.url} target="_blank" rel="noreferrer">{tr('Board')} <Icon name="external" /></a>{/if}
    {/if}
    {#if canCreate && !onNext}<button class="new" class:on={creating} onclick={() => (creating = !creating)} title={tr('New ticket')} aria-label={tr('New ticket')} aria-expanded={creating}><Icon name="plus" /></button>{/if}
    <RefreshTickets />
    {#if (onNext ? next?.url : sprint)}<span class="narrow"><ActionMenu sections={more} label={onNext ? tr('More on the next sprint') : tr('More on this sprint')} /></span>{/if}
    <BlockControls id="sprint" />
  </h2>
  {#if creating && canCreate && !onNext}<NewTicketForm onclose={() => (creating = false)} />{/if}
  {#if onNext && next}
    <!-- Next tab: the next sprint, its dates and all its tickets. -->
    <div class="sub narrow">
      <span class="t">{short(next.start)} → {short(next.end)}{#if startsIn !== null}{' '}· {tr('in')} <b>{startsIn}</b> {tr('d')}{/if} · <b>{nextTickets.length}</b> {tr('tickets')}{filtered ? ` ${tr('(filtered)')}` : ''}</span>
    </div>
    {#if isShort('sprint')}
      <div class="short-body">
        <div class="stack">{#each nextGroups as [status, n]}<i style="flex: {n}; background: {statusColor(status, next.statuses)}" title="{status} {n}"></i>{/each}</div>
        <span class="more">{startsIn !== null ? tr('{n} tickets planned, starting in {days} days.', { n: nextTickets.length, days: startsIn }) : tr('{n} tickets planned.', { n: nextTickets.length })}</span>
      </div>
    {:else}
      <TicketList id="sprint.next" tickets={nextTickets} statusesOf={() => next.statuses} {me} people />
    {/if}
  {:else}
  {#if sprint && !isShort('sprint')}
    <!-- Progress (High and Urgent tickets done) and sprint day, at a glance. -->
    <div class="meter" title="{tr('{n}% done', { n: progress ?? 0 })}{days ? ` · ${tr('day {day} of {days}', { day: days.day, days: days.total })}` : ''}">
      <span class="done" style="width: {progress ?? 0}%"></span>
      {#if days}<span class="today" style="left: {(100 * days.day) / days.total}%"></span>{/if}
    </div>
    <div class="sub narrow">
      <span class="t">{#if sprint.start || sprint.end}{short(sprint.start)} → {short(sprint.end)} ·{' '}{/if}{#if days}{tr('day')} <b>{days.day}</b>/{days.total} ·{' '}{/if}<b>{tickets.length}</b> {tr('tickets')}{filtered ? ` ${tr('(filtered)')}` : ''}{#if progress !== null}<span class="pct" title={tr('{done} of {total} high and urgent tickets done', { done: key.filter(isDone).length, total: key.length })}>{' '}· <b>{progress}%</b> {tc('tickets', 'done')}</span>{/if}</span>
      <span class="grow"></span>
      {#if showOutside}<button class="pill-off" onclick={toggleOutside} title={tr('Hide the tickets outside the sprint')}>+{extra.length} {tr('outside')} <Icon name="close" /></button>{/if}
    </div>
  {/if}
  {#if sprint && isShort('sprint')}
    <div class="short-body">
      <div class="stack">{#each groups as [status, n]}<i style="flex: {n}; background: {statusColor(status, sprint.statuses)}" title="{status} {n}"></i>{/each}</div>
      {#each mineOpen as t (t.task_id)}
        {@const step = ticketStep(t, me, sprint.statuses)}
        <div class="srow">
          <a class="mono" href={t.url} target="_blank" rel="noreferrer" onclick={previewClick(ticketKey(t))}>{t.id}</a>
          <span class="t" title={t.name}><TaskType type={t.task_type} glyph={t.task_type_icon} icon />{t.name}</span>
          <span class="step {step.tone}" title={step.label}>{step.label}</span>
        </div>
      {:else}
        <span class="more">{tr('No open ticket assigned to me.')}</span>
      {/each}
    </div>
  {:else if sprint}
    <TicketList id="sprint" tickets={listed} {statusesOf} {me} {special} tag={board} grouping={boards > 1 ? byBoard : undefined} toSprint={offers('add_to_sprint')} people />
  {:else if !trackerOn()}
    <TrackerOff />
  {:else}
    <div class="empty">{onObsidian() ? tr('Boards not read yet (Settings › Work › Tracker › Obsidian).') : tr('Current sprint not found (ClickUp folder, dates, or missing token).')}</div>
  {/if}
  {/if}
</section>

<style>
  .blocked-pill { display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; padding: 0 7px; border-radius: var(--r9); background: color-mix(in srgb, var(--ko) 16%, transparent); color: var(--ko); font-size: var(--fs-tag); font-weight: 500; line-height: 17px; white-space: nowrap; }
  .new { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 20px; height: 20px; padding: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font-size: 14px; cursor: pointer; }
  .new:hover, .new.on { background: var(--surface-muted); color: var(--text); }
  section { container-type: inline-size; }
  .grow { flex: 1; }
  .tabs { display: inline-flex; flex-shrink: 0; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .tabs button { padding: 0 8px; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); font-weight: 400; cursor: pointer; }
  .tabs button.on { background: color-mix(in srgb, var(--src) 18%, transparent); color: var(--text); }
  .progress { font-size: var(--fs-meta); font-weight: 600; color: var(--ok); font-variant-numeric: tabular-nums; }
  .info { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .narrow { display: none; }
  .short, .pct { display: none; }
  .pct b { color: var(--ok); }
  /* On the phone: Now / Next a thumb can hit. */
  :global(html.remote) .tabs button { min-height: 30px; padding: 0 12px; }
  /* Narrow block: title, progress and actions on the line; info below, the rest in ⋯. The block's
     controls take ~120px: below 660px, Outside sprint and Board no longer fit beside them. */
  @container (max-width: 660px) {
    .wide { display: none !important; }
    .narrow { display: flex; }
  }
  /* Very narrow: shorter tabs, tighter gaps, the progress (in the line below) leaves the header,
     and the title gives way before the block's controls do. */
  @container (max-width: 430px) {
    .long, .progress { display: none; }
    .short, .pct { display: inline; }
    h2 { gap: 5px; }
    h2 .title { min-width: 0; flex-shrink: 1; }
    h2 .name { overflow: hidden; text-overflow: ellipsis; }
    h2 > :global(*:not(.title)) { flex-shrink: 0; }
  }
  .meter { position: relative; flex-shrink: 0; height: 4px; background: var(--surface-muted); }
  .meter .done { position: absolute; inset: 0 auto 0 0; background: var(--ok); }
  .meter .today { position: absolute; top: -2px; bottom: -2px; width: 2px; margin-left: -1px; background: var(--text); opacity: 0.55; }
  .sub { align-items: center; gap: 8px; padding: 3px 12px; border-bottom: 1px solid var(--border); font-size: var(--fs-meta); color: var(--text-muted); white-space: nowrap; min-width: 0; }
  .sub .t { overflow: hidden; text-overflow: ellipsis; }
  .sub b { color: var(--text); font-weight: 600; }
  .pill-off { flex-shrink: 0; padding: 0 6px; border: none; border-radius: var(--r9); background: color-mix(in srgb, var(--violet) 18%, transparent); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .stack { display: flex; height: 8px; border-radius: var(--r4); overflow: hidden; margin-bottom: 2px; flex-shrink: 0; }
  .stack i { display: block; }
  h2 a { font-size: var(--fs-meta); font-weight: 400; white-space: nowrap; }
  .outside { display: inline-flex; align-items: center; gap: 5px; padding: 0 2px; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); font-weight: 400; cursor: pointer; white-space: nowrap; }
  .outside:hover { color: var(--text); }
  .outside b { font-weight: 600; }
  .switch { position: relative; width: 22px; height: 12px; border-radius: var(--r6); background: var(--surface-muted); border: 1px solid var(--border); transition: background 0.15s; }
  .switch i { position: absolute; top: 1px; left: 1px; width: 8px; height: 8px; border-radius: 50%; background: var(--text-muted); transition: transform 0.15s, background 0.15s; }
  .outside.on { color: var(--text); }
  .outside.on .switch { background: color-mix(in srgb, var(--violet) 35%, var(--surface)); border-color: var(--violet); }
  .outside.on .switch i { transform: translateX(10px); background: var(--violet); }
  .board { padding: 0 5px; border-radius: var(--r4); background: color-mix(in srgb, var(--violet) 18%, transparent); color: var(--text); font-size: var(--fs-tag); white-space: nowrap; }
  /* Next step: the same colour code as the block's rows. */
  .step { flex-shrink: 0; max-width: 45%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-tag); font-weight: 500; color: var(--text-muted); }
  .step.blocked { color: var(--ko); } .step.todo { color: var(--warn); } .step.wait { color: var(--info); } .step.ready { color: var(--ok); } .step.auto { color: var(--accent); }
</style>
