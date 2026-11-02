<script lang="ts">
  import { t as tr, tc } from '../i18n.svelte'
  import { hoursOf, offers, ticketKey, trackerName } from '../tracker'
  import { mrNumber } from '../forge'
  import type { Snippet } from 'svelte'
  import Assignees from './Assignees.svelte'
  import Logo from './Logo.svelte'
  import PipelineBar from './PipelineBar.svelte'
  import StatusSelect from './StatusSelect.svelte'
  import TaskType from './TaskType.svelte'
  import Icon from './Icon.svelte'
  import EstimateInput from './EstimateInput.svelte'
  import { conventions } from '../conventions'
  import TicketMenu from './TicketMenu.svelte'
  import PrioritySelect from './PrioritySelect.svelte'
  import type { ClaudeAction } from '../claude-actions'
  import { isDone, statusColor } from '../rules'
  import { mrsOf, ticketStep } from '../ticket-step'
  import { byTaskId, dependencyDone, openBlockers } from '../tickets'
  import { canPreview, openTicket, previewClick, ticketView } from '../ticket-view.svelte'
  import { openMr } from '../mr-view.svelte'
  import { ago, minutesSince } from '../time'
  import type { Dependency, Person, StatusOption, Ticket } from '../types'
  import { keys } from '../platform'
  import { pref, setPref } from '../layout.svelte'
  import { threadAttr } from '../thread.svelte'
  import { appearance } from '../fun.svelte'
  import ThreadLinks from './ThreadLinks.svelte'

  /**
   * A ticket on two lines of text: the full title, and below it the context (ID, assignees,
   * parent, block-specific label, age). On the right, its MR and pipeline, its editable
   * status, the next step in plain words, the unblocking action and the ⋯ menu (MR and step hidden when
   * the block is narrow). A click opens the preview; ⌘-click on the title opens ClickUp.
   * Waiting on other tickets (ClickUp's dependencies): a third line names them, their status and
   * name, the done ones struck; the tickets it blocks are on the second line.
   */
  let {
    ticket: t, me, statuses, depth = 0, now, tag, claude, toSprint = false, statusInGroup = false,
  }: {
    ticket: Ticket
    me: Person | null
    statuses: StatusOption[]
    /** Subtask placed under its parent. */
    depth?: number
    now: number
    /** Block-specific label on the context line (board of origin…). */
    tag?: Snippet<[Ticket]>
    /** Claude actions for the menu, instead of Refine and Implement. */
    claude?: ClaudeAction[]
    /** Offer "Add to sprint" in the menu ("To pick"). */
    toSprint?: boolean
    /** Its group is its status: on the phone, the status pill is left out (the ⋯ menu changes it). */
    statusInGroup?: boolean
  } = $props()

  /** On the phone (the remote): no status repeating the group, no empty estimate (set in the preview). */
  const phone = document.documentElement.classList.contains('remote')

  /** Beyond this, an open ticket that has not moved is flagged in orange. */
  const STALE_DAYS = 14

  const step = $derived(ticketStep(t, me, statuses))
  const mr = $derived(mrsOf(t.id, t.repo)[0])
  const mine = $derived(!!me && t.assignees.some((a) => a.id === me.id))
  const done = $derived(isDone(t))
  const parent = $derived(t.parent ? byTaskId(t.parent) : undefined)
  const stale = $derived(!done && minutesSince(t.updated_at) > STALE_DAYS * 1440)
  /** Its blockers, shown while it is open; `waiting`: those not finished. */
  const blockers = $derived(done ? [] : (t.waiting_on ?? []))
  const waiting = $derived(openBlockers(t))
  /** The open tickets it blocks: worth finishing first. */
  const blocks = $derived(done ? [] : (t.blocking ?? []).filter((d) => !dependencyDone(d)))
  const PRIORITY: Record<string, string> = { urgent: tr('Urgent'), high: tr('High'), normal: tc('tickets', 'Normal'), low: tr('Low') }
  const depLabel = (d: Dependency) => d.id || tr('a ticket')
  /** Every row's blockers folded into a chip on the second line (remembered), or on their own line. */
  const folded = $derived(pref<boolean>('tickets.depsFolded', false))
  function fold(e: MouseEvent, on: boolean) {
    e.stopPropagation()
    setPref('tickets.depsFolded', on)
  }
  const depsTitle = $derived(blockers.map((d) => `${depLabel(d)}${d.status ? ` (${d.status})` : ''}${d.name ? ` ${d.name}` : ''}${dependencyDone(d) ? ` · ${tc('tickets', 'done')}` : ''}`).join('\n'))
  /** The other ticket's preview; its page when it is not readable. */
  function openDep(e: MouseEvent, d: Dependency) {
    e.stopPropagation()
    if (d.id && canPreview()) {
      e.preventDefault()
      openTicket(d.id)
    }
  }

  let busy = $state(false)
  let error = $state<string | null>(null)
  async function run(e: MouseEvent) {
    e.stopPropagation()
    const p = step.primary
    if (!p || !('run' in p) || busy) return
    busy = true
    error = null
    try {
      await p.run()
    } catch (err) {
      error = String(err)
    } finally {
      busy = false
    }
  }

  function onclick(e: MouseEvent) {
    // Links, buttons, selects and menus keep their own click.
    if ((e.target as HTMLElement).closest('a, button, label, select, [role=menu]')) return
    if (canPreview()) openTicket(ticketKey(t))
  }
  function onkeydown(e: KeyboardEvent) {
    if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget && canPreview()) {
      e.preventDefault()
      openTicket(ticketKey(t))
    }
  }
</script>

<div
  class="tk {step.tone}"
  class:deps={blockers.length > 0 && !folded}
  class:waiting={waiting.length > 0}
  class:selected={ticketView.id === ticketKey(t)}
  class:done
  class:mine
  data-thread={threadAttr({ ticket: t.id, repo: t.repo })}
  role="button"
  tabindex="0"
  {onclick}
  {onkeydown}
>
  <span class="flag p-{t.priority ?? 'none'}" title={t.priority ? tr('{level} priority', { level: PRIORITY[t.priority] }) : tr('No priority')}></span>
  <div class="l1" style="padding-left: {depth * 14}px">
    {#if depth}<span class="sub"><Icon name="subtask" /></span>{/if}
    <PrioritySelect taskId={t.task_id} priority={t.priority} label={t.priority === 'urgent' || t.priority === 'high'} />
    <TaskType type={t.task_type} glyph={t.task_type_icon} icon />
    <a class="t" href={t.url} target="_blank" rel="noreferrer" onclick={previewClick(ticketKey(t))} title="{t.name} · {canPreview() ? keys(tr('click: preview, ⌘-click: {tracker}', { tracker: trackerName() })) : tr('open in {app}', { app: trackerName() })}">{t.name}</a>
  </div>
  <div class="l2" style="padding-left: {depth * 14}px">
    <span class="mono">{t.id}</span>
    {#if appearance().ticket_threads}<ThreadLinks at={{ ticket: t.id, repo: t.repo }} />{/if}
    {#if offers('assignees')}<Assignees ticket={t} {me} max={2} />{/if}
    {#if t.assignees.length}<span class="names">{t.assignees.map((a) => (a.id === me?.id ? tr('you') : a.name.split(' ')[0])).join(', ')}</span>{/if}
    {#if tag}{@render tag(t)}{/if}
    {#if t.parent && !depth}<span class="sep">·</span><button class="parent" onclick={() => openTicket(parent?.id ?? t.parent!)} title={tr('Subtask of {parent}', { parent: parent ? `${parent.id} · ${parent.name}` : tr('another ticket') })}><Icon name="parent" /> {parent?.id ?? tr('parent')}</button>{/if}
    <span class="sep">·</span>
    <span class:stale title={tr('Last update in {tracker}', { tracker: trackerName() })}>{ago(t.updated_at)}</span>
    {#if blockers.length && folded}
      <span class="sep">·</span>
      <button class="depchip" class:free={!waiting.length} onclick={(e) => fold(e, false)} title={`${waiting.length ? tr('Waiting on {n} of {total}', { n: waiting.length, total: blockers.length }) : tr('Unblocked: its blockers are done')}\n${depsTitle}\n${tr('Click: show the blockers of every ticket')}`}><Icon name={waiting.length ? 'lock' : 'unlock'} />{waiting.length ? tr('waiting on {n}', { n: waiting.length }) : tr('unblocked')}</button>
    {/if}
    {#if blocks.length}
      <span class="sep">·</span>
      <span class="blocks" title={tr('Waiting on this ticket: {list}', { list: blocks.map((d) => `${depLabel(d)} ${d.name}`).join(', ') })}>{tr('blocks')} {#each blocks.slice(0, 3) as d, i (d.task_id)}{#if i},{' '}{/if}<a href={d.url || undefined} onclick={(e) => openDep(e, d)}>{depLabel(d)}</a>{/each}{#if blocks.length > 3}{' '}+{blocks.length - 3}{/if}</span>
    {/if}
  </div>
  {#if blockers.length && !folded}
    <div class="l3" class:free={!waiting.length}>
      <button class="lbl" onclick={(e) => fold(e, true)} title={tr('Fold the blockers of every ticket into a chip on its second line')}><Icon name={waiting.length ? 'lock' : 'unlock'} />{waiting.length ? tr('waiting on') : tr('unblocked')}<Icon name="chevronDown" /></button>
      {#each blockers as d (d.task_id)}
        {@const over = dependencyDone(d)}
        <span class="dep" class:over>
          <a class="mono" href={d.url || undefined} onclick={(e) => openDep(e, d)} title={d.id ? `${d.id} · ${d.name}` : tr('Not read from ClickUp (deleted, or no access)')}>{depLabel(d)}</a>
          {#if d.status}<span class="ds" style:--sc={statusColor(d.status, statuses)}>{d.status}</span>{/if}
          {#if d.name}<span class="dn">{d.name}</span>{/if}
        </span>
      {/each}
    </div>
  {/if}
  <div class="mr">
    {#if mr}
      <button class="iid" onclick={() => openMr(mr)} title="{tr('Open {name}', { name: mrNumber(mr.iid) })} · {mr.title}">{mrNumber(mr.iid)}</button>
      {#if mr.state === 'merged'}<span class="merged">{tr('merged')}</span>{:else}<PipelineBar pipeline={mr.head_pipeline} status={mr.pipeline} {now} />{/if}
    {:else if !done}
      <span class="none">{tr('no MR')}</span>
    {/if}
  </div>
  <div class="st">
    {#if !(phone && statusInGroup)}<StatusSelect ticket={t} {statuses} highlight={mine && !done} />{/if}
    {#if t.fields?.length || conventions().estimate_builtin}
      <!-- The custom fields of [clickup] fields, under the status: a pill per value, in its option's color;
           an estimate field (and the tracker's own estimate, when entered from Prism) as a field to type in. -->
      <span class="fields">{#each t.fields ?? [] as f (f.name)}{#if f.estimate}{#if !phone || hoursOf(f) !== null}<EstimateInput compact taskId={t.task_id} field={f.name} hours={hoursOf(f)} label={f.name} />{/if}{:else}{#each f.values as v (v.label)}<span class="field" style:--c={v.color} title="{f.name}: {v.label}">{v.label}</span>{/each}{/if}{/each}{#if conventions().estimate_builtin && (!phone || t.time_estimate)}<EstimateInput compact taskId={t.task_id} field={null} hours={t.time_estimate ? t.time_estimate / 3_600_000 : null} label={tc('noun', 'Estimate')} />{/if}</span>
    {/if}
  </div>
  <div class="right">
    {#if !done}
      <span class="next" title={step.label}>{step.label}</span>
      {#if step.primary}
        {#if 'href' in step.primary}
          <a class="act" href={step.primary.href} title={step.primary.title}><Logo name="agent" size={10} />{step.primary.label}</a>
        {:else}
          <button class="act" class:error={!!error} disabled={busy} onclick={run} title={error ? tr('Failed: {error}', { error }) : step.primary.title}>{busy ? '…' : step.primary.label}</button>
        {/if}
      {/if}
    {/if}
    <TicketMenu ticket={t} {me} {statuses} {claude} {toSprint} />
  </div>
</div>

<style>
  .tk {
    display: grid; grid-template-columns: 4px minmax(0, 1fr) 150px 108px 250px; gap: 0 10px; align-items: center;
    grid-template-areas: 'flag l1 mr st right' 'flag l2 mr st right';
    padding: calc(3px * var(--space)) 10px calc(3px * var(--space)) 8px; border-bottom: 1px solid var(--border); line-height: 1.35; cursor: pointer;
    --c: var(--text-muted);
  }
  .tk:hover { background: rgba(127, 127, 127, 0.06); }
  .tk.selected { background: color-mix(in srgb, var(--src-clickup) 9%, transparent); }
  .tk:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  /* Same colour code as the MRs: red blocked, orange to do, blue waiting, green ready, purple automatic. */
  .tk.blocked { --c: var(--ko); } .tk.todo { --c: var(--warn); } .tk.wait { --c: var(--info); } .tk.ready { --c: var(--ok); } .tk.auto { --c: var(--accent); }
  .tk.done { opacity: 0.55; }
  /* Waiting on others: a third line, under the title and the context, across the row. */
  .tk.deps { grid-template-areas: 'flag l1 mr st right' 'flag l2 mr st right' 'flag l3 l3 l3 l3'; }
  .tk.waiting { background: linear-gradient(90deg, color-mix(in srgb, var(--ko) 6%, transparent), transparent 55%); }
  .l3 { grid-area: l3; display: flex; flex-wrap: wrap; align-items: center; gap: 2px 12px; min-width: 0; margin: 2px 0 1px; padding-top: 3px; border-top: 1px dashed color-mix(in srgb, var(--ko) 28%, transparent); font-size: var(--fs-tag); }
  .l3.free { border-top-color: color-mix(in srgb, var(--ok) 30%, transparent); }
  .l3 .lbl { display: inline-flex; align-items: center; gap: 4px; padding: 0 3px 0 0; border: none; border-radius: var(--r4); background: none; color: var(--ko); font: inherit; font-weight: 600; text-transform: uppercase; letter-spacing: 0.3px; cursor: pointer; }
  .l3 .lbl:hover { background: color-mix(in srgb, currentColor 12%, transparent); }
  .depchip { display: inline-flex; align-items: center; gap: 3px; padding: 0 6px 0 4px; border: none; border-radius: var(--r9); background: color-mix(in srgb, var(--ko) 15%, transparent); color: var(--ko); font: inherit; font-weight: 500; line-height: 16px; cursor: pointer; }
  .depchip.free { background: color-mix(in srgb, var(--ok) 15%, transparent); color: var(--ok); }
  .depchip:hover { filter: brightness(1.15); }
  .l3.free .lbl { color: var(--ok); }
  .dep { display: inline-flex; align-items: center; gap: 5px; min-width: 0; max-width: 100%; }
  .dep a { color: var(--text); font-weight: 600; }
  .dep a:hover { color: var(--info); text-decoration: none; }
  .dep .ds { flex-shrink: 0; padding: 0 5px; border-radius: var(--r4); background: color-mix(in srgb, var(--sc) 20%, transparent); color: color-mix(in srgb, var(--sc) 75%, var(--text)); font-weight: 600; text-transform: uppercase; font-size: 9.5px; line-height: 15px; }
  .dep .dn { min-width: 0; max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); }
  .dep.over { opacity: 0.55; }
  .dep.over .dn, .dep.over a { text-decoration: line-through; }
  .blocks { color: var(--ko); }
  .blocks a { color: inherit; font-family: var(--mono); }
  .blocks a:hover { text-decoration: underline; }
  .flag { grid-area: flag; align-self: stretch; margin: 2px 0; border-radius: var(--r2); background: transparent; }
  .p-urgent { background: var(--ko); } .p-high { background: var(--warn); } .p-normal { background: var(--info); } .p-low { background: var(--text-muted); }
  .l1 { grid-area: l1; display: flex; align-items: center; gap: 5px; min-width: 0; font-size: var(--fs-body); }
  .sub { color: var(--text-muted); }
  .t { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; color: var(--text); }
  .t:hover { color: var(--info); }
  .tk.mine .t { font-weight: 600; }
  .l2 { grid-area: l2; display: flex; align-items: center; gap: 5px; min-width: 0; overflow: hidden; white-space: nowrap; font-size: var(--fs-tag); color: var(--text-muted); }
  .l2 > * { flex-shrink: 0; }
  .l2 .names { flex-shrink: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .l2 .sep { opacity: 0.45; }
  /* A custom field's value, in its option's color when it has one. */

  .mono { font-family: var(--mono); }
  .parent { border: none; background: none; padding: 0; font: inherit; color: var(--text-muted); cursor: pointer; }
  .parent:hover { color: var(--info); }
  .stale { color: var(--warn); }
  .mr { grid-area: mr; display: flex; align-items: center; gap: 6px; min-width: 0; font-size: var(--fs-tag); }
  .mr :global(.pipe) { min-width: 0; }
  .iid { flex-shrink: 0; border: none; background: none; padding: 0; font-family: var(--mono); font-size: var(--fs-meta); color: var(--info); cursor: pointer; }
  .iid:hover { text-decoration: underline; }
  .merged { color: var(--ok); }
  .none { color: var(--text-muted); opacity: 0.6; }
  .st { grid-area: st; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .st .fields { display: flex; gap: 3px; min-width: 0; overflow: hidden; }
  .st .field { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 0 6px; border-radius: var(--r8); background: color-mix(in srgb, var(--c, var(--text-muted)) 16%, transparent); color: var(--c, var(--text)); font-size: var(--fs-tag); line-height: 1.5; }
  .right { grid-area: right; display: flex; align-items: center; justify-content: flex-end; gap: 6px; min-width: 0; }
  .next { min-width: 0; overflow: hidden; text-overflow: ellipsis; padding: 0 7px; border-radius: var(--r10); font-size: var(--fs-tag); font-weight: 500; line-height: 18px; white-space: nowrap; background: color-mix(in srgb, var(--c) 15%, transparent); color: color-mix(in srgb, var(--c) 70%, var(--text)); }
  .tk.idle .next { background: var(--surface-muted); color: var(--text-muted); }
  .act { display: inline-flex; flex-shrink: 0; align-items: center; gap: 4px; padding: 0 8px; border: 1px solid color-mix(in srgb, var(--c) 60%, var(--border)); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; cursor: pointer; }
  .act:hover { background: color-mix(in srgb, var(--c) 12%, var(--surface)); text-decoration: none; }
  .act.error { border-color: var(--ko); color: var(--ko); }
  .act:disabled { cursor: progress; opacity: 0.6; }
  /* Narrow block: title and status only; the MR and the next step stay in the preview and the menu. */
  @container tickets (max-width: 620px) {
    .tk { grid-template-columns: 4px minmax(0, 1fr) 108px auto; grid-template-areas: 'flag l1 st right' 'flag l2 st right'; }
    .tk.deps { grid-template-areas: 'flag l1 st right' 'flag l2 st right' 'flag l3 l3 l3'; }
    .mr, .next, .act { display: none; }
  }
  /* On the phone: the title has the whole first line (⋯ aside); the status and fields go on the second, after the id. */
  :global(html.remote) .tk { grid-template-columns: 4px minmax(0, 1fr) auto auto; grid-template-areas: 'flag l1 l1 right' 'flag l2 st st'; }
  :global(html.remote) .tk.deps { grid-template-areas: 'flag l1 l1 right' 'flag l2 st st' 'flag l3 l3 l3'; }
  :global(html.remote) .st { flex-direction: row; align-items: center; gap: 4px; }
  :global(html.remote) .right :global(.more) { width: 32px; height: 32px; }
  /* A title read whole on the phone, on two lines at most, rather than cut at its first. */
  :global(html.remote) .l1 { align-items: flex-start; }
  :global(html.remote) .t { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; white-space: normal; line-height: 1.3; }
</style>
