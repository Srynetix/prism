<script lang="ts">
  import { t as tr, tc, tn } from '../i18n.svelte'
  import { isBlocked } from '../tickets'
  import { offers, onObsidian, queuesSource, ticketKey, trackerLogo, trackerName, trackerProvider } from '../tracker'
  import BlockControls from './BlockControls.svelte'
  import { live } from '../live.svelte'
  import RefreshTickets from './RefreshTickets.svelte'
  import TrackerOff from './TrackerOff.svelte'
  import { trackerOn } from '../live.svelte'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import TicketList, { type Grouping } from './TicketList.svelte'
  import TaskType from './TaskType.svelte'
  import Assignees from './Assignees.svelte'
  import { isCollapsed, isShort } from '../layout.svelte'
  import { previewClick } from '../ticket-view.svelte'
  import type { PickBoard, Person, Pickup, Ticket } from '../types'

  /**
   * Tickets to pick: the `pick` boards of the config (`[[clickup.boards]]`) and the next sprint,
   * in the "to pick" statuses (`pick_statuses` in the config). The table is the sprint's (next
   * step, assign me, Claude Implement, ⋯ menu), grouped by board in the config's order — the red
   * and orange ones before the next sprint —, with the board as a label on each row.
   */
  let {
    pickup, me, filtered = false,
  }: { pickup: Pickup | undefined; me: Person | null; filtered?: boolean } = $props()

  const statuses = $derived(pickup?.statuses ?? [])
  /** The boards shown here (a board read only for its tickets' Sync is not). */
  const sources = $derived<PickBoard[]>((pickup?.boards ?? []).filter((b) => b.pick))
  /** Each ticket only once, attached to its first source; only the "to pick" statuses. */
  const origin = $derived.by(() => {
    const m = new Map<string, PickBoard>()
    for (const s of sources) for (const t of s.tickets) if (statuses.includes(t.status) && !m.has(t.task_id)) m.set(t.task_id, s)
    return m
  })
  const all = $derived(sources.flatMap((s) => s.tickets.filter((t) => origin.get(t.task_id) === s)))
  const blocked = $derived(all.filter(isBlocked).length)

  const rankOf = (t: Ticket) => sources.indexOf(origin.get(t.task_id)!)
  const statusesOf = (t: Ticket) => origin.get(t.task_id)?.statuses ?? []

  const TONES = ['ko', 'warn', 'info', 'violet', 'ok']
  const toneOf = (s: PickBoard) => (TONES.includes(s.tone) ? s.tone : 'info')
  const sourceName = (s: PickBoard) => (s.kind === 'next_sprint' ? s.name.replace(/^.*?(Sprint \d+).*$/, '$1') : s.name)
  /** Groups by board, in the config's order. */
  const BOARD: Grouping = {
    key: 'board',
    label: tr('board'),
    of: (t) => {
      const s = origin.get(t.task_id)
      return { name: s ? sourceName(s) : '', color: `var(--${s ? toneOf(s) : 'info'})`, rank: rankOf(t) }
    },
  }

  /** Short: a counter per board and the three highest-priority unassigned ones. */
  const PRIORITIES = ['urgent', 'high', 'normal', 'low', null]
  const top = $derived(
    all
      .filter((t) => !t.assignees.length)
      .sort((a, b) => PRIORITIES.indexOf(a.priority) - PRIORITIES.indexOf(b.priority) || rankOf(a) - rankOf(b))
      .slice(0, 3),
  )
</script>

{#snippet board(t: Ticket)}
  {@const s = origin.get(t.task_id)}
  {#if s}<span class="src {toneOf(s)}" title={s.name}>{sourceName(s)}</span>{/if}
{/snippet}

<section class="panel" style="--src: var(--src-clickup)" class:collapsed={isCollapsed('pickup')}>
  <h2>
    <span class="title"><Logo name={trackerLogo()} />{tr('To pick')}</span>
    <span class="head">
      <small>{tr('{n} ready', { n: all.length })} · {statuses.join(', ')}{filtered ? ` ${tr('(filtered)')}` : ''}</small>
      {#if blocked}<span class="blocked-pill" title={tn(blocked, '{n} ticket waiting on unfinished ones', '{n} tickets waiting on unfinished ones')}><Icon name="lock" />{blocked > 1 ? tc('plural', '{n} blocked', { n: blocked }) : tr('{n} blocked', { n: blocked })}</span>{/if}
      {#each sources as s (s.url)}<a href={s.url} target="_blank" rel="noreferrer">{s.kind === 'next_sprint' ? tr('Next sprint') : s.name} <Icon name="external" /></a>{/each}
    </span>
    <RefreshTickets />
    <BlockControls id="pickup" />
  </h2>
  {#if !trackerOn()}
    <TrackerOff />
  {:else if !offers('pickup')}
    <div class="empty">{tr('{tracker} has no boards to pick from: its open cards, backlog included, are in the Sprint block.', { tracker: trackerName() })}</div>
  {:else if !live.snapshot?.answered?.includes(queuesSource())}
    <div class="empty">{tr('Loading the tickets to pick…')}</div>
  {:else if !sources.length && onObsidian()}
    <div class="empty">{tr('No card ready to pick: none in a lane of the Ready statuses ({statuses}, Settings › Work › Tracker).', { statuses: live.snapshot?.conventions?.pick_statuses?.join(', ') || tr('none set') })}</div>
  {:else if !sources.length && trackerProvider() === 'jira'}
    <div class="empty">{tr('Nothing to pick from: no next sprint, and no')} <code>[[jira.queues]]</code> {tr('with')} <code>uses = ["pick"]</code> {tr('(Settings › Work › Tracker › Jira).')}</div>
  {:else if !sources.length}
    <div class="empty">{tr('No board to pick from: no next sprint, and no')} <code>[[clickup.boards]]</code> {tr('with')} <code>uses = ["pick"]</code> {tr('found (Settings › Work › Tracker › ClickUp).')}</div>
  {:else if isShort('pickup')}
    <div class="short-body">
      <div class="srow counts">
        {#each sources as s (s.url)}<span class="src {toneOf(s)}">{s.kind === 'next_sprint' ? tr('Next sprint') : s.name} <b>{all.filter((t) => origin.get(t.task_id) === s).length}</b></span>{/each}
      </div>
      {#each top as t (t.task_id)}
        <div class="srow">
          <a class="mono" href={t.url} target="_blank" rel="noreferrer" onclick={previewClick(ticketKey(t))}>{t.id}</a>
          <span class="t" title={t.name}><TaskType type={t.task_type} glyph={t.task_type_icon} icon />{t.name}</span>
          {@render board(t)}
          <Assignees ticket={t} {me} />
        </div>
      {:else}
        <span class="more">{tr('Nothing unassigned to pick.')}</span>
      {/each}
    </div>
  {:else}
    <TicketList id="pickup" tickets={all} {statusesOf} {me} grouping={BOARD} tag={board} toSprint={offers('add_to_sprint')} />
  {/if}
</section>

<style>
  .blocked-pill { display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; padding: 0 7px; border-radius: var(--r9); background: color-mix(in srgb, var(--ko) 16%, transparent); color: var(--ko); font-size: var(--fs-tag); font-weight: 500; line-height: 17px; white-space: nowrap; }
  .head { display: flex; gap: 10px; align-items: baseline; min-width: 0; overflow: hidden; white-space: nowrap; }
  .head a { font-size: var(--fs-meta); font-weight: 400; }
  /* On the phone: the links go to the next line rather than off the edge. */
  :global(html.remote) .head { flex-wrap: wrap; row-gap: 2px; }
  .empty code { font-family: var(--mono); font-size: var(--fs-mono); }
  /* Colour per board: its `tone` (the next sprint blue). */
  .ko { --c: var(--ko); } .warn { --c: var(--warn); } .info { --c: var(--info); } .violet { --c: var(--violet); } .ok { --c: var(--ok); }
  .src { display: inline-block; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 0 6px; border-radius: var(--r4); font-size: var(--fs-tag); background: color-mix(in srgb, var(--c) 16%, transparent); color: color-mix(in srgb, var(--c) 60%, var(--text)); vertical-align: middle; }
  .counts { gap: 8px; flex-wrap: wrap; }
  .counts b { font-weight: 600; }
</style>
