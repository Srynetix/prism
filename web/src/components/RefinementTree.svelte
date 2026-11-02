<script lang="ts">
  import { t as tr, tc } from '../i18n.svelte'
  import { ticketKey } from '../tracker'
  import Assignees from './Assignees.svelte'
  import Logo from './Logo.svelte'
  import PrioritySelect from './PrioritySelect.svelte'
  import StatusSelect from './StatusSelect.svelte'
  import TaskType from './TaskType.svelte'
  import Icon from './Icon.svelte'
  import TicketMenu from './TicketMenu.svelte'
  import { pref, setPref } from '../layout.svelte'
  import { previewClick, canPreview, openTicket, ticketView } from '../ticket-view.svelte'
  import { byTaskId, descendantsOf } from '../tickets'
  import { claudeClick, refineAction, type ClaudeAction } from '../claude-actions'
  import { isDone, statusColor } from '../rules'
  import { ago, minutesSince } from '../time'
  import type { Person, QuarterBoard, StatusOption, Ticket } from '../types'
  import { keys } from '../platform'

  /**
   * Refinement queue arranged by EPIC: one row per EPIC (progress of its tasks by
   * status, estimated load, signals, next action), which expands onto its tasks. Tickets
   * without an EPIC stay at the bottom. At the top, at most three priority actions. Sort by status
   * (flow order, EPICs by priority then remaining work) or by last modification
   * (most recent first, an EPIC counting its tasks'). Expanded EPICs, "Hide done"
   * and sort remembered under `refinement.open`, `refinement.hideDone` and `refinement.sort`.
   */
  let {
    items, boards, statuses, only = [], me,
  }: { items: { t: Ticket; board: QuarterBoard }[]; boards: QuarterBoard[]; statuses: string[]; only?: string[]; me: Person | null } = $props()

  const PRIORITIES = ['urgent', 'high', 'normal', 'low', 'none'] as const
  /** Beyond this, a ticket in refinement that has not moved is flagged. */
  const STALE_DAYS = 30

  let open = $state(pref<string[]>('refinement.open', []))
  let hideDone = $state(pref<boolean>('refinement.hideDone', true))
  type Sort = 'status' | 'updated'
  let sort = $state(pref<Sort>('refinement.sort', 'status'))
  function setSort(s: Sort) {
    sort = s
    setPref('refinement.sort', s)
  }
  function toggle(id: string) {
    open = open.includes(id) ? open.filter((x) => x !== id) : [...open, id]
    setPref('refinement.open', open)
  }
  function setOpen(ids: string[]) {
    open = ids
    setPref('refinement.open', open)
  }
  function toggleDone() {
    hideDone = !hideDone
    setPref('refinement.hideDone', hideDone)
  }

  const rank = (t: Ticket) => PRIORITIES.indexOf(t.priority ?? 'none')
  const days = (iso: string | null) => Math.round(minutesSince(iso) / 1440)
  const isEpic = (t: Ticket) => t.task_type?.toLowerCase() === 'epic'
  const inQueue = (t: Ticket) => !t.closed && statuses.includes(t.status)
  const mine = (t: Ticket) => !!me && t.assignees.some((a) => a.id === me.id)
  const label = (status: string) => (status.includes('estimat') ? tr('Estimate') : tr('Refine'))
  const hours = (ms: number) => `${Math.round((ms / 3_600_000) * 10) / 10} h`

  /** Each ticket's board, for its statuses: the first one that contains it. */
  const boardOf = $derived(new Map(boards.flatMap((b) => b.tickets.map((t) => [t.task_id, b] as const)).reverse()))

  /** A ticket's EPIC, walking up its parents: a task's subtask stays attached. */
  function epicOf(t: Ticket): Ticket | undefined {
    const seen = new Set<string>()
    for (let p = byTaskId(t.parent); p && !seen.has(p.task_id); p = byTaskId(p.parent)) {
      if (isEpic(p)) return p
      seen.add(p.task_id)
    }
  }

  /** Rank of a status in the board's flow; closed tickets last. */
  const step = (t: Ticket, opts: StatusOption[]) => (t.closed ? 1000 : (opts.findIndex((s) => s.status === t.status) + 1 || 500))
  const byStep = (opts: StatusOption[]) => (a: Ticket, b: Ticket) =>
    step(a, opts) - step(b, opts) || rank(a) - rank(b) || (a.updated_at ?? '').localeCompare(b.updated_at ?? '')

  interface Group {
    epic: Ticket
    statuses: StatusOption[]
    kids: Ticket[]
    /** Tasks in refinement, the next one to handle first. */
    todo: Ticket[]
    /** Tasks past refinement: ready, in progress or delivered. */
    ready: number
    /** Progress: number of tasks per status, in flow order. */
    bar: { status: string; color: string; n: number }[]
    estimate: number
    unestimated: number
    unassigned: number
    asleep: number
    next: { what: string; action: ClaudeAction } | null
  }

  function group(epic: Ticket): Group {
    const opts = boardOf.get(epic.task_id)?.statuses ?? []
    const kids = descendantsOf(epic.task_id).sort(byStep(opts))
    const lastRefine = Math.max(...statuses.map((s) => opts.findIndex((o) => o.status === s)))
    const live = kids.filter((k) => !isDone(k))
    const todo = kids.filter(inQueue)
    const bar = [...new Set(kids.map((k) => (k.closed ? 'closed' : k.status)))].map((status) => ({
      status,
      color: status === 'closed' ? 'var(--ok)' : statusColor(status, opts),
      n: kids.filter((k) => (k.closed ? 'closed' : k.status) === status).length,
    }))
    const first = todo[0]
    return {
      epic,
      statuses: opts,
      kids,
      todo,
      ready: kids.filter((k) => k.closed || (lastRefine >= 0 && opts.findIndex((o) => o.status === k.status) > lastRefine)).length,
      bar,
      estimate: kids.reduce((sum, k) => sum + (k.time_estimate ?? 0), 0),
      unestimated: live.filter((k) => !k.time_estimate).length,
      unassigned: live.filter((k) => !k.assignees.length).length,
      asleep: todo.filter((k) => days(k.updated_at) > STALE_DAYS).length,
      next: first
        ? { what: `${first.status.replace(/^to /, '')} ${first.id}`, action: refineAction(first, label(first.status)) }
        : kids.length
          ? null
          : { what: tr('split it into tasks'), action: refineAction(epic, tc('tickets', 'Split')) },
    }
  }

  /** The EPICs of the queue's tickets (filters applied), and the tickets that have none. */
  const tree = $derived.by(() => {
    const epics = new Map<string, Ticket>()
    const orphans: Ticket[] = []
    for (const { t } of items) {
      const epic = isEpic(t) ? t : epicOf(t)
      if (epic) epics.set(epic.task_id, epic)
      else orphans.push(t)
    }
    const groups = [...epics.values()].map(group).sort(
      sort === 'updated'
        ? (a, b) => lastUpdate(b).localeCompare(lastUpdate(a))
        : // Priority EPICs first, then those not yet split, then the most behind.
          (a, b) => rank(a.epic) - rank(b.epic) || +!!a.kids.length - +!!b.kids.length || b.todo.length - a.todo.length,
    )
    return { groups, orphans: orphans.sort(sort === 'updated' ? byUpdate : byStep([])) }
  })

  interface Hint {
    tone: string
    why: string
    t: Ticket
    epic?: Ticket
    action: ClaudeAction
  }
  /** At most three things to do: an EPIC to split, the ticket asleep the longest, an EPIC almost ready. */
  const hints = $derived.by(() => {
    const out: Hint[] = []
    const unsplit = tree.groups.find((g) => !g.kids.length)
    if (unsplit) out.push({ tone: 'var(--ko)', why: `${tr('EPIC not split')}${unsplit.epic.priority ? ` · ${unsplit.epic.priority}` : ''}`, t: unsplit.epic, action: refineAction(unsplit.epic, tc('tickets', 'Split')) })
    const asleep = items.map((i) => i.t).filter((t) => !isEpic(t) && days(t.updated_at) > STALE_DAYS).sort((a, b) => (a.updated_at ?? '').localeCompare(b.updated_at ?? ''))[0]
    if (asleep) out.push({ tone: 'var(--warn)', why: `${tr('Asleep for {n} days', { n: days(asleep.updated_at) })} · ${asleep.status}`, t: asleep, epic: epicOf(asleep), action: refineAction(asleep, label(asleep.status)) })
    const last = statuses[statuses.length - 1]
    const almost = tree.groups.find((g) => g.todo.length && g.todo.every((k) => k.status === last))
    if (almost) out.push({ tone: 'var(--info)', why: tr('{n} × {status} left before ready', { n: almost.todo.length, status: last }), t: almost.epic, action: refineAction(almost.todo[0], label(last)) })
    return out
  })

  /** Most recent first. */
  const byUpdate = (a: Ticket, b: Ticket) => (b.updated_at ?? '').localeCompare(a.updated_at ?? '')
  /** An EPIC's last modification: its own, or one of its tasks'. */
  const lastUpdate = (g: Group) => [g.epic, ...g.kids].map((t) => t.updated_at ?? '').sort().pop() ?? ''
  /**
   * The displayed tasks: `kids` stays in flow order, which gives the next action. Statuses chosen
   * (`only`): those tasks alone, the EPIC's bar and counts still telling all of them.
   */
  const shownKids = (g: Group) => {
    const kids = only.length ? g.kids.filter((k) => !k.closed && only.includes(k.status)) : hideDone ? g.kids.filter((k) => !isDone(k)) : g.kids
    return sort === 'updated' ? [...kids].sort(byUpdate) : kids
  }

  /** A click on a task row opens the preview; links, buttons and selects keep their own. */
  function openRow(e: MouseEvent, t: Ticket) {
    if (canPreview() && !(e.target as HTMLElement).closest('a, button, label, select, [role=menu]')) openTicket(ticketKey(t))
  }
  function toggleRow(e: MouseEvent, id: string) {
    if (!(e.target as HTMLElement).closest('a, button, label, select, [role=menu]')) toggle(id)
  }
</script>

{#snippet act(a: ClaudeAction)}
  {#if a.href}<a class="act" href={a.href} onclick={claudeClick(a)} title={`${a.prompt}\n${keys('⇧-click')}: ${tr('launch at once')}`}><Logo name="agent" size={10} />{a.label}</a>
  {:else}<span class="act off" title={a.why}><Logo name="agent" size={10} />{a.label}</span>{/if}
{/snippet}

{#snippet row(t: Ticket, opts: StatusOption[], next: boolean, epic?: Ticket)}
  {@const st = boardOf.get(t.task_id)?.statuses ?? opts}
  {@const p = epic && t.parent !== epic.task_id ? byTaskId(t.parent) : undefined}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="kid" class:done={isDone(t)} class:next class:selected={ticketView.id === ticketKey(t)} onclick={(e) => openRow(e, t)}>
    <span class="branch"></span>
    <PrioritySelect taskId={t.task_id} priority={t.priority} label={false} />
    <a class="id" href={t.url} target="_blank" rel="noreferrer" onclick={previewClick(ticketKey(t))} title={canPreview() ? keys(tr('Preview {id} · ⌘-click opens ClickUp', { id: t.id })) : tr('Open {id} in ClickUp', { id: t.id })}>{t.id}</a>
    <span class="name" title={t.name}><TaskType type={t.task_type} glyph={t.task_type_icon} icon />{#if p}<span class="via">{p.name} ›</span>{/if}{t.name}</span>
    <StatusSelect ticket={t} statuses={st} highlight={mine(t)} />
    <Assignees ticket={t} {me} max={2} />
    <span class="upd" class:stale={inQueue(t) && days(t.updated_at) > STALE_DAYS} title={tr('Last update in ClickUp')}>{ago(t.updated_at)}</span>
    <span class="end">
      {#if inQueue(t)}<span class="hover" class:shown={next}>{@render act(refineAction(t, label(t.status)))}</span>
      {:else if t.time_estimate}<span class="est-kid">{hours(t.time_estimate)}</span>{/if}
      <TicketMenu ticket={t} {me} statuses={st} claude={[refineAction(t, label(t.status))]} />
    </span>
  </div>
{/snippet}

<div class="wrap">
  <div class="tools">
    <span class="label">{tr('Sort')}</span>
    <span class="seg" role="group" aria-label={tr('Sort')}>
      <button class:on={sort === 'status'} onclick={() => setSort('status')} title={tr('Tasks in the order of the flow; EPICs by priority, then by work left')}>{tr('Status')}</button>
      <button class:on={sort === 'updated'} onclick={() => setSort('updated')} title={tr('Most recently updated first; an EPIC counts the updates of its tasks')}>{tr('Last update')}</button>
    </span>
    <span class="grow"></span>
    <button class="chip" class:on={hideDone} onclick={toggleDone}>{tr('Hide done')}</button>
    <button class="chip" onclick={() => setOpen(tree.groups.map((g) => g.epic.task_id))}>{tr('Expand all')}</button>
    <button class="chip" onclick={() => setOpen([])}>{tr('Collapse')}</button>
  </div>
  {#if hints.length}
    <div class="hints">
      {#each hints as h (h.t.task_id + h.why)}
        <div class="hint" style="--c: {h.tone}">
          <span class="why">{h.why}</span>
          <a class="what" href={h.t.url} target="_blank" rel="noreferrer" onclick={previewClick(h.t.id)} title={h.t.name}><TaskType type={h.t.task_type} glyph={h.t.task_type_icon} icon />{h.t.name}</a>
          <span class="line"><span class="id">{h.t.id}</span>{#if h.epic}<span class="in" title={h.epic.name}>{tr('in {name}', { name: h.epic.name })}</span>{/if}<span class="grow"></span>{@render act(h.action)}</span>
        </div>
      {/each}
    </div>
  {/if}
  <div class="tree">
    {#each tree.groups as g (g.epic.task_id)}
      {@const isOpen = open.includes(g.epic.task_id)}
      {@const kids = shownKids(g)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="epic" class:open={isOpen} class:selected={ticketView.id === g.epic.id} onclick={(e) => toggleRow(e, g.epic.task_id)}>
        <button class="caret" aria-expanded={isOpen} aria-label={isOpen ? tr('Collapse') : tr('Expand')} onclick={() => toggle(g.epic.task_id)}><Icon name="chevronRight" /></button>
        <div class="title">
          <div class="l1">
            <PrioritySelect taskId={g.epic.task_id} priority={g.epic.priority} label={false} />
            <TaskType type={g.epic.task_type} glyph={g.epic.task_type_icon} icon />
            <a class="id" href={g.epic.url} target="_blank" rel="noreferrer" onclick={previewClick(g.epic.id)}>{g.epic.id}</a>
            <a class="ename" href={g.epic.url} target="_blank" rel="noreferrer" onclick={previewClick(g.epic.id)} title={g.epic.name}>{g.epic.name}</a>
          </div>
          <div class="l2">
            {#if boardOf.get(g.epic.task_id)}{@const b = boardOf.get(g.epic.task_id)!}<span class="folder" class:archi={!b.quarter}>{b.folder}</span>{#if b.quarter}<span class="q">{b.quarter}</span>{/if}{/if}
            <StatusSelect ticket={g.epic} statuses={g.statuses} highlight={mine(g.epic)} />
            {#if !g.kids.length}<span class="flag warn">{tr('not split')}</span>{/if}
            {#if g.unassigned}<span class="flag warn">{tr('{n} unassigned', { n: g.unassigned })}</span>{/if}
            {#if g.asleep}<span class="flag warn">{tr('{n} asleep > {days}d', { n: g.asleep, days: STALE_DAYS })}</span>{/if}
            {#if g.kids.length && !g.todo.length}<span class="flag ok">{tr('refined')}</span>{/if}
          </div>
        </div>
        <div class="progress">
          {#if g.kids.length}
            <div class="stack">{#each g.bar as s (s.status)}<i style="flex: {s.n}; background: {s.color}" title="{s.n} {s.status}"></i>{/each}</div>
            <small><b>{g.ready}</b>/{g.kids.length} {tc('tickets', 'ready')} · <b>{g.todo.length}</b> {tr('to refine')}</small>
          {:else}<small>{tr('no task yet')}</small>{/if}
        </div>
        <div class="estimate">
          {#if g.kids.length}
            {#if g.estimate}<span><b>{hours(g.estimate)}</b> {tr('estimated')}</span>{/if}
            {#if g.unestimated}<span class="stale">{tr('{n} not estimated', { n: g.unestimated })}</span>{:else}<span>{tr('all estimated')}</span>{/if}
          {/if}
        </div>
        <div class="nxt">
          {#if g.next}<span title={tr('Next to work on')}>{tr('next: {what}', { what: g.next.what })}</span>{@render act(g.next.action)}{:else}<span>{tr('nothing to refine')}</span>{/if}
        </div>
      </div>
      {#if isOpen}
        <div class="kids">
          {#each kids as k (k.task_id)}{@render row(k, g.statuses, k === g.todo[0], g.epic)}{/each}
          {#if g.kids.length > kids.length}<p class="foot">{only.length ? tr('{n} in other statuses hidden', { n: g.kids.length - kids.length }) : tr('{n} done hidden', { n: g.kids.length - kids.length })}</p>{/if}
        </div>
      {/if}
    {/each}
    {#if tree.orphans.length}
      <h5>{tr('Without EPIC')} <b>{tree.orphans.length}</b></h5>
      <div class="kids">{#each tree.orphans as t (t.task_id)}{@render row(t, boardOf.get(t.task_id)?.statuses ?? [], false)}{/each}</div>
    {/if}
    {#if !tree.groups.length && !tree.orphans.length}<p class="none">{tr('Nothing to refine.')}</p>{/if}
  </div>
</div>

<style>
  .wrap { flex: 1; min-height: 0; display: flex; flex-direction: column; container-type: inline-size; }
  .tools { display: flex; align-items: center; gap: 4px 6px; padding: 0 12px 6px; }
  .label { font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.4px; }
  .seg { display: inline-flex; gap: 2px; padding: 2px; border-radius: var(--r6); background: var(--surface-muted); }
  .seg button { padding: 0 8px; border: none; border-radius: var(--r4); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); font-weight: 500; }
  .grow { flex: 1; }
  .chip { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border: 1px solid transparent; border-radius: var(--r10); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .chip:hover { color: var(--text); }
  .chip.on { border-color: var(--accent); color: var(--text); background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .id { font-family: var(--mono); font-size: var(--fs-mono); white-space: nowrap; }
  .act { display: inline-flex; flex-shrink: 0; align-items: center; gap: 4px; padding: 0 8px; border: 1px solid color-mix(in srgb, var(--src-claude) 60%, var(--border)); border-radius: var(--r5); background: var(--surface); color: var(--text); font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; }
  .act:hover { background: color-mix(in srgb, var(--src-claude) 12%, var(--surface)); text-decoration: none; }
  .act.off { opacity: 0.5; cursor: not-allowed; }

  .hints { display: flex; gap: 8px; padding: 0 12px 8px; overflow-x: auto; }
  .hint { flex: 1; min-width: 220px; display: flex; flex-direction: column; gap: 2px; padding: 6px 9px; border: 1px solid var(--border); border-radius: var(--r7); background: color-mix(in srgb, var(--accent) 4%, var(--surface)); box-shadow: inset 3px 0 0 var(--c); }
  .why { font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.3px; }
  .what { color: var(--text); font-size: var(--fs-body); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .what:hover { color: var(--info); }
  .line { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .in { font-size: var(--fs-tag); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .tree { flex: 1; min-height: 0; overflow: auto; border-top: 1px solid var(--border); }
  .epic { display: grid; grid-template-columns: 16px minmax(0, 1fr) 180px 110px minmax(150px, 220px); align-items: center; gap: 10px; padding: 6px 12px 6px 8px; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--src-clickup) 3%, var(--surface)); cursor: pointer; }
  .epic:hover { background: color-mix(in srgb, var(--src-clickup) 8%, var(--surface)); }
  .epic.selected { box-shadow: inset 3px 0 0 var(--accent); }
  .caret { display: inline-flex; align-items: center; justify-content: center; border: none; background: none; padding: 0; color: var(--text-muted); font-size: 12px; cursor: pointer; transition: transform 0.12s; }
  .epic.open .caret { transform: rotate(90deg); }
  .title { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .l1, .l2 { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .l2 { flex-wrap: wrap; font-size: var(--fs-tag); }
  .l2 :global(.pill) { max-width: 120px; }
  .ename { color: var(--text); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ename:hover { color: var(--info); }
  .folder { padding: 0 5px; border-radius: var(--r4); background: var(--surface-muted); color: var(--text-muted); white-space: nowrap; }
  .folder.archi { background: color-mix(in srgb, var(--violet) 18%, transparent); color: var(--text); }
  .q { color: var(--text-muted); font-family: var(--mono); white-space: nowrap; }
  .flag { padding: 0 6px; border-radius: var(--r9); white-space: nowrap; }
  .flag.warn { background: var(--warn-bg); color: var(--warn); }
  .flag.ok { background: var(--ok-bg); color: var(--ok); }
  .progress { display: flex; flex-direction: column; gap: 3px; }
  .stack { display: flex; height: 7px; border-radius: var(--r4); overflow: hidden; background: var(--surface-muted); }
  .stack i { display: block; }
  .progress small, .estimate { font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; }
  .progress b, .estimate b { color: var(--text); font-weight: 600; }
  .estimate { display: flex; flex-direction: column; line-height: 1.35; }
  .nxt { display: flex; align-items: center; justify-content: flex-end; gap: 6px; min-width: 0; font-size: var(--fs-meta); color: var(--text-muted); }
  .nxt span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .kid { display: grid; grid-template-columns: 22px auto 64px minmax(0, 1fr) auto auto 52px auto; align-items: center; gap: 8px; padding: 2px 12px 2px 8px; border-bottom: 1px solid var(--border); font-size: var(--fs-body); cursor: pointer; }
  .kid:hover { background: color-mix(in srgb, var(--accent) 5%, var(--surface)); }
  .kid.selected { background: color-mix(in srgb, var(--src-clickup) 9%, var(--surface)); }
  .kid.next { box-shadow: inset 3px 0 0 var(--accent); }
  .kid :global(.pill) { max-width: 120px; }
  .branch { position: relative; align-self: stretch; }
  .branch::before { content: ''; position: absolute; left: 10px; top: 0; bottom: 0; border-left: 1px solid var(--border); }
  .branch::after { content: ''; position: absolute; left: 10px; top: 50%; width: 9px; border-top: 1px solid var(--border); }
  .kid:last-child .branch::before { bottom: 50%; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); }
  .kid.done .name { color: var(--text-muted); text-decoration: line-through; text-decoration-color: color-mix(in srgb, var(--text-muted) 50%, transparent); }
  .via { color: var(--text-muted); margin-right: 4px; }
  .upd { font-size: var(--fs-tag); color: var(--text-muted); text-align: right; white-space: nowrap; }
  .stale { color: var(--warn); }
  .end { display: flex; align-items: center; justify-content: flex-end; gap: 6px; min-width: 96px; }
  .hover { visibility: hidden; }
  .kid:hover .hover, .hover.shown { visibility: visible; }
  .est-kid { font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); }
  .foot { margin: 0; padding: 3px 12px 5px 38px; font-size: var(--fs-tag); color: var(--text-muted); border-bottom: 1px solid var(--border); }
  h5 { margin: 0; padding: 5px 12px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); background: var(--bg); border-bottom: 1px solid var(--border); }
  h5 b { color: var(--text); }
  .none { margin: 8px 12px; color: var(--text-muted); font-size: var(--fs-meta); }

  /* Narrow block: the EPIC keeps its title and next action, the task its status. */
  @container (max-width: 760px) {
    .epic { grid-template-columns: 16px minmax(0, 1fr) 120px; }
    .estimate, .nxt { display: none; }
    .kid { grid-template-columns: 22px auto 64px minmax(0, 1fr) auto auto auto; }
    .end { min-width: 0; }
    .upd, .end > .hover, .est-kid { display: none; }
  }
  /* On the phone: an EPIC's name has the width, its progress under it. */
  :global(html.remote) .epic { grid-template-columns: 16px minmax(0, 1fr); row-gap: 6px; }
  :global(html.remote) .epic .progress { grid-column: 2; }
</style>
