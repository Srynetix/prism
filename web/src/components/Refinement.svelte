<script lang="ts">
  import { t as tr, tc } from '../i18n.svelte'
  import { offers, queuesSource, trackerLogo, trackerName, trackerProvider } from '../tracker'
  import BlockControls from './BlockControls.svelte'
  import { live } from '../live.svelte'
  import RefreshTickets from './RefreshTickets.svelte'
  import TrackerOff from './TrackerOff.svelte'
  import { trackerOn } from '../live.svelte'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import TaskType from './TaskType.svelte'
  import RefinementTree from './RefinementTree.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import { previewClick } from '../ticket-view.svelte'
  import type { Person, QuarterBoard, Refinement, Ticket } from '../types'

  /**
   * Refinement queue: tickets of the `refinement` boards — a folder's quarterly lists (current
   * quarter and following ones), or a board's list, which has no quarter and so stays visible
   * whatever quarter is chosen — arranged by EPIC (see RefinementTree). In short, a counter per
   * step and its next ticket: the highest priority, then the one that has waited the
   * longest. Filters by quarter, by folder and by status, remembered under `refinement.quarter`,
   * `refinement.folders` and `refinement.statuses`; the status filter is the full view's (the short
   * one keeps a counter per step).
   */
  let {
    refinement, me, filtered = false,
  }: { refinement: Refinement | undefined; me: Person | null; filtered?: boolean } = $props()

  const COLORS = ['var(--warn)', 'var(--violet)', 'var(--info)']
  const PRIORITIES = ['urgent', 'high', 'normal', 'low', 'none'] as const

  const now = new Date()
  const currentQuarter = `Q${Math.floor(now.getMonth() / 3) + 1} ${now.getFullYear()}`

  let quarter = $state(pref<string>('refinement.quarter', 'all'))
  let folders = $state(pref<string[]>('refinement.folders', []))
  let picked = $state(pref<string[]>('refinement.statuses', []))
  function setQuarter(q: string) {
    quarter = q
    setPref('refinement.quarter', q)
  }
  function toggleFolder(f: string) {
    folders = folders.includes(f) ? folders.filter((x) => x !== f) : [...folders, f]
    setPref('refinement.folders', folders)
  }
  function toggleStatus(st: string) {
    picked = only.includes(st) ? only.filter((x) => x !== st) : [...only, st]
    setPref('refinement.statuses', picked)
  }

  interface Item {
    t: Ticket
    board: QuarterBoard
  }
  const statuses = $derived(refinement?.statuses ?? [])
  const boards = $derived<QuarterBoard[]>(refinement?.boards ?? [])
  /** A ticket whose status was just changed leaves the queue without waiting for the re-read. */
  const all = $derived<Item[]>(boards.flatMap((board) => board.tickets.filter((t) => statuses.includes(t.status)).map((t) => ({ t, board }))))
  const quarters = $derived([...new Set(boards.map((b) => b.quarter).filter(Boolean))])
  const folderNames = $derived([...new Set(boards.map((b) => b.folder))])
  const inQuarter = (i: Item) => quarter === 'all' || !i.board.quarter || i.board.quarter === quarter
  const inFolders = (i: Item) => !folders.length || folders.includes(i.board.folder)
  const shown = $derived(all.filter((i) => inQuarter(i) && inFolders(i)))
  /** The chosen statuses still in the queue's flow (a status renamed or removed is forgotten). */
  const only = $derived(picked.filter((st) => statuses.includes(st)))
  const inStatuses = (i: Item) => !only.length || only.includes(i.t.status)
  const listed = $derived(shown.filter(inStatuses))

  const rank = (t: Ticket) => PRIORITIES.indexOf(t.priority ?? 'none')
  const columns = $derived(
    statuses.map((status, n) => ({
      status,
      // The status's ClickUp colour, the same as on the card badges.
      color: boards.flatMap((b) => b.statuses).find((s) => s.status === status)?.color ?? COLORS[n] ?? 'var(--text-muted)',
      items: shown
        .filter((i) => i.t.status === status)
        .sort((a, b) => rank(a.t) - rank(b.t) || (a.t.updated_at ?? '').localeCompare(b.t.updated_at ?? '')),
    })),
  )
  /** Links to the boards: those of the chosen quarter (or the current one), and Architecture. */
  const links = $derived(
    boards.filter((b) => (!b.quarter || b.quarter === (quarter === 'all' ? currentQuarter : quarter)) && (!folders.length || folders.includes(b.folder))),
  )
</script>

<section class="panel" style="--src: var(--src-clickup)" class:collapsed={isCollapsed('refinement')}>
  <h2>
    <span class="title"><Logo name={trackerLogo()} />{tr('Refinement')}</span>
    <span class="head">
      <small>{tr('{n} in the queue', { n: isShort('refinement') ? shown.length : listed.length })}{filtered ? ` ${tr('(filtered)')}` : ''}</small>
      {#each links as b (b.url)}<a href={b.url} target="_blank" rel="noreferrer">{b.folder}{b.quarter ? ` ${b.quarter.split(' ')[0]}` : ''} <Icon name="external" /></a>{/each}
    </span>
    <RefreshTickets />
    <BlockControls id="refinement" />
  </h2>
  {#if !trackerOn()}
    <TrackerOff />
  {:else if !offers('refinement')}
    <div class="empty">{tr('{tracker} has no refinement queue: its open cards are in the Sprint block.', { tracker: trackerName() })}</div>
  {:else if !live.snapshot?.answered?.includes(queuesSource())}
    <div class="empty">{tr('Loading the refinement queue…')}</div>
  {:else if !boards.length && trackerProvider() === 'jira'}
    <div class="empty">{tr('No refinement queue: add a')} <code>[[jira.queues]]</code> {tr('with')} <code>uses = ["refinement"]</code> {tr('(Settings › Work › Tracker › Jira).')}</div>
  {:else if !boards.length}
    <div class="empty">{tr('No refinement board found: add a')} <code>[[clickup.boards]]</code> {tr('with')} <code>uses = ["refinement"]</code> {tr('(Settings › Work › Tracker › ClickUp), a folder’s quarterly lists or a list.')}</div>
  {:else if isShort('refinement')}
    <!-- Short: a counter per step and its next ticket (quarter and board filters applied). -->
    <div class="short-body">
      {#each columns as col (col.status)}
        {@const first = col.items[0]}
        <div class="srow" style="--c: {col.color}">
          <span class="step">{col.status} <b>{col.items.length}</b></span>
          {#if first}
            <a class="id" href={first.t.url} target="_blank" rel="noreferrer" onclick={previewClick(first.t.id)}>{first.t.id}</a>
            <span class="t" title={first.t.name}><TaskType type={first.t.task_type} glyph={first.t.task_type_icon} icon />{first.t.name}</span>
          {:else}<span class="t muted-txt">—</span>{/if}
        </div>
      {/each}
    </div>
  {:else}
    <div class="bar">
      <span class="label">{tr('Quarter')}</span>
      <button class="chip" class:on={quarter === 'all'} onclick={() => setQuarter('all')}>{tc('tickets', 'All')} <b>{all.filter((i) => inFolders(i) && inStatuses(i)).length}</b></button>
      {#each quarters as q (q)}
        <button class="chip" class:on={quarter === q} onclick={() => setQuarter(q)}>{q}{q === currentQuarter ? ` · ${tc('tickets', 'current')}` : ''} <b>{all.filter((i) => i.board.quarter === q && inFolders(i) && inStatuses(i)).length}</b></button>
      {/each}
      <span class="sep"></span>
      <span class="label">{tr('Board')}</span>
      {#each folderNames as f (f)}
        <button class="chip" class:on={folders.includes(f)} onclick={() => toggleFolder(f)}>{f} <b>{all.filter((i) => i.board.folder === f && inQuarter(i) && inStatuses(i)).length}</b></button>
      {/each}
      <span class="sep"></span>
      <span class="label">{tr('Status')}</span>
      {#each columns as col (col.status)}
        <button class="chip st" style="--c: {col.color}" class:on={only.includes(col.status)} onclick={() => toggleStatus(col.status)}>{col.status} <b>{col.items.length}</b></button>
      {/each}
    </div>
    <RefinementTree items={listed} {boards} {statuses} {only} {me} />
  {/if}
</section>

<style>
  .head { display: flex; gap: 10px; align-items: baseline; min-width: 0; overflow: hidden; white-space: nowrap; }
  .head a { font-size: var(--fs-meta); font-weight: 400; }
  .empty code { font-family: var(--mono); font-size: var(--fs-mono); }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; padding: 8px 12px 6px; }
  .label { font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.4px; margin-right: 2px; }
  .sep { width: 1px; height: 14px; background: var(--border); margin: 0 4px; }
  .step { display: inline-flex; align-items: center; gap: 5px; width: 112px; flex-shrink: 0; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.3px; color: var(--text-muted); }
  .step::before { content: ''; width: 8px; height: 8px; border-radius: var(--r2); background: var(--c); }
  .step b { color: var(--text); }
  .muted-txt { color: var(--text-muted); }
  .id { font-family: var(--mono); font-size: var(--fs-mono); }
  .chip { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border: 1px solid transparent; border-radius: var(--r10); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .chip:hover { color: var(--text); }
  .chip.on { border-color: var(--accent); color: var(--text); background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .chip b { font-weight: 600; }
  .chip.st::before { content: ''; width: 7px; height: 7px; border-radius: var(--r2); background: var(--c); }
</style>
