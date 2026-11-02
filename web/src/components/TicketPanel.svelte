<script lang="ts">
  import { t as tr, tc, tn } from '../i18n.svelte'
  import { untrack } from 'svelte'
  import { locale } from '../locale'
  import { offers, onObsidian, trackerName } from '../tracker'
  import { linkOnly } from '../links'
  import { asTicket, conventions } from '../conventions'
  import { hoursOf } from '../tracker'
  import EstimateInput from './EstimateInput.svelte'
  import TicketActual from './TicketActual.svelte'
  import { estimateOf } from '../actuals'
  import { clickedOutside } from '../drawers'
  import { isPinned } from '../drawer-pins'
  import PinButton from './PinButton.svelte'
  import { addComment, addToSprint, backTicket, closeTicket, editComment, openTicket, refreshTicket, setStatus, ticketView } from '../ticket-view.svelte'
  import { live } from '../live.svelte'
  import { hasCore, invoke } from '../bridge'
  import CommentBox from './CommentBox.svelte'
  import PrioritySelect from './PrioritySelect.svelte'
  import { setQuery } from '../filter.svelte'
  import { ago, span } from '../time'
  import Markdown from './Markdown.svelte'
  import RemoteImage from './RemoteImage.svelte'
  import TaskType from './TaskType.svelte'
  import Icon from './Icon.svelte'
  import { keys } from '../platform'
  import { byTaskId, dependencyDone } from '../tickets'
  import { statusColor } from '../rules'
  import { claudeClick, openAgentAction, ticketActions } from '../claude-actions'
  import { actorName } from '../agent-cli'
  import type { Ticket } from '../types'
  import ActionSheet from './ActionSheet.svelte'
  import { flatten, sprintToAdd, ticketMenu } from '../menus'
  import { armer } from '../armed.svelte'
  import { mrsOf } from '../ticket-step'
  import { openMr } from '../mr-view.svelte'
  import { mrKey } from '../mr-ref'
  import { meOn } from '../forge'
  import MrTitle from './MrTitle.svelte'
  import { nextStep } from '../rules'
  import { openTimeDraft } from '../timeDraft.svelte'
  import { copy } from '../toast.svelte'
  import type { TicketCommit } from '../bindings/TicketCommit'
  import type { MrRef } from '../mr-ref'
  import { journal, journeyOf, stepLabel, where } from '../journey'
  import { journeyAction } from '../journey-actions'
  import JourneyAction from './JourneyAction.svelte'
  import JourneyStrip from './JourneyStrip.svelte'
  import FoldTitle from './FoldTitle.svelte'

  /**
   * Preview of a ClickUp ticket in a side drawer, without leaving the dashboard: description,
   * checklists, subtasks and comments. It hides nothing else: a click on another
   * identifier replaces its content, ← goes back to the previous one, Escape closes it.
   * `phone` (the remote): the whole screen, Back to the list (or the ticket before), larger type
   * and targets; at the bottom its first agent action and the sheet of all its actions.
   */
  let { phone = false }: { phone?: boolean } = $props()
  const t = $derived(ticketView.task)
  const day = (iso: string | null) => (iso ? new Date(iso).toLocaleDateString(locale(), { day: 'numeric', month: 'short', year: 'numeric' }) : '—')
  const stamp = (iso: string | null) => (iso ? new Date(iso).toLocaleString(locale(), { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }) : '')
  const done = $derived(t ? t.checklists.flatMap((c) => c.items).filter((i) => i.resolved).length : 0)
  const total = $derived(t ? t.checklists.flatMap((c) => c.items).length : 0)
  /** Its dependencies, from the tickets Prism reads (ClickUp's, both ways). */
  const read = $derived(t ? byTaskId(t.task_id) : undefined)
  const deps = $derived([
    { key: 'waiting', label: tr('Waiting on'), list: read?.waiting_on ?? [], tone: 'ko' },
    { key: 'blocking', label: tr('Blocking'), list: read?.blocking ?? [], tone: 'muted' },
  ].filter((d) => d.list.length))
  /**
   * The agent actions of its ⋯ menu (Refine, Implement, Validate…, then Open agent): on the ticket as
   * Prism reads it (its repository, its worktree), with the panel's fresher status; a ticket Prism
   * doesn't read is taken from the panel, in the primary repository.
   */
  const row = $derived.by((): Ticket | undefined => {
    if (!t) return undefined
    if (read) return { ...read, status: t.status, closed: t.closed }
    const { task_id, id, name, task_type, task_type_icon, status, closed, priority, assignees, parent, url, created_at, updated_at, time_estimate } = t
    return { task_id, id, name, task_type, task_type_icon, status, closed, priority, assignees, parent, url, created_at, updated_at, time_estimate, list: null, repo: null, description: null, fields: [], waiting_on: [], blocking: [] }
  })
  const actions = $derived(row ? [...ticketActions(row), openAgentAction(row)] : [])
  /**
   * Its journey (journey.ts): where it stands from refinement to done, why, and the one action that
   * moves it on (the review's feedback applied, its red CI fixed, its validation…), at the top;
   * then its journal, each event Prism reads for it by stage, the steps ahead dashed. The phone's
   * bar offers the same action; failing one, its first agent action.
   */
  const journey = $derived(row && t && live.snapshot ? journeyOf(live.snapshot, row, t.statuses) : null)
  const nextAction = $derived(journey && live.snapshot ? journeyAction(journey, live.snapshot) : null)
  const inSprint = $derived(t && live.snapshot?.sprint?.tickets.some((x) => x.task_id === t.task_id) ? live.snapshot.sprint.name : null)
  const stages = $derived(journey && row && live.snapshot ? journal(live.snapshot, row, journey, inSprint) : [])
  const shortDay = (iso: string) => new Date(iso).toLocaleDateString(locale(), { day: 'numeric', month: 'short' })
  const mainAction = $derived(actions.find((a) => a.href))
  let stepBusy = $state(false)
  async function runStep() {
    const p = nextAction
    if (!p || !('run' in p) || stepBusy) return
    stepBusy = true
    try {
      await p.run()
    } catch (e) {
      ticketView.saveError = String(e)
    } finally {
      stepBusy = false
    }
  }
  let sheet = $state(false)
  const sheetActions = $derived(row && t ? flatten(ticketMenu(row, live.snapshot?.tracker_me ?? null, t.statuses, actions, true)).filter((a) => a.key !== 'filter') : [])
  $effect(() => {
    void ticketView.id
    sheet = false
  })

  /**
   * Add it to the current sprint (it keeps its list), after a second click; offered while it isn't
   * in it. The button goes when the snapshot that follows reads it in the sprint.
   */
  const sprintName = $derived(t ? sprintToAdd(t.task_id) : null)
  const sprintArm = armer(4000)
  let sprintBusy = $state(false)
  async function toSprint() {
    if (!t || sprintBusy || !sprintArm.confirm()) return
    sprintBusy = true
    ticketView.saveError = null
    try {
      await addToSprint(t.task_id)
    } catch (e) {
      ticketView.saveError = String(e)
    } finally {
      sprintBusy = false
    }
  }

  let body: HTMLElement | undefined = $state()
  // New ticket: start again from the top.
  $effect(() => {
    void ticketView.id
    body?.scrollTo({ top: 0 })
  })

  /** Enlarged image over the drawer (attachment or description image). */
  let zoomed = $state<{ url: string; title: string; open?: string } | null>(null)
  const images = $derived(t?.attachments.filter((a) => a.image) ?? [])
  const files = $derived(t?.attachments.filter((a) => !a.image) ?? [])
  function size(n: number | null) {
    if (!n) return ''
    return n < 1024 ? `${n} B` : n < 1048576 ? `${Math.round(n / 1024)} KB` : `${(n / 1048576).toFixed(1)} MB`
  }
  // Another ticket: the enlarged image closes.
  $effect(() => {
    void ticketView.id
    zoomed = null
  })

  /**
   * Its commits in the local clones (ticket_commits.rs): named in their message, or on its
   * branches; read again with each ticket. Merged ones in the main or a release branch, the others
   * with their branch.
   */
  let commits = $state<TicketCommit[] | null>(null)
  let allCommits = $state(false)
  const ticketId = $derived(t?.id)
  $effect(() => {
    const id = ticketId
    commits = null
    allCommits = false
    // asTicket reads the snapshot: tracked, each refresh would empty the list and read it again.
    if (!id || !hasCore() || !untrack(() => asTicket(id))) return
    invoke<TicketCommit[]>('ticket_commits', { id })
      .then((c) => {
        if (ticketView.task?.id === id) commits = c
      })
      .catch(() => (commits = []))
  })
  const COMMITS_SHOWN = 8
  const shownCommits = $derived(commits ? (allCommits ? commits : commits.slice(0, COMMITS_SHOWN)) : [])
  const commitRepos = $derived(new Set(commits?.map((c) => c.repo)).size)
  const unmerged = $derived(commits?.filter((c) => !c.merged).length ?? 0)

  /**
   * Its MRs in the snapshot (open first, then merged), each with its next step. A click opens it
   * in the MR panel, which lies under this one: the ticket closes (⌘⇧T, or the MR's ticket, brings
   * it back).
   */
  const mrs = $derived(t ? mrsOf(t.id, row?.repo).map((m) => ({ m, step: nextStep(m, meOn(live.snapshot, m), live.snapshot?.merge_queue_label) })) : [])
  const mrRepos = $derived(new Set(mrs.map(({ m }) => m.repo)).size)
  function showMr(m: MrRef) {
    closeTicket()
    openMr(m)
  }

  /** Comment being edited (mine). */
  let editing = $state<string | null>(null)
  const mine = (c: { author: { id: number } | null }) => !!c.author && c.author.id === live.snapshot?.tracker_me?.id

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !ticketView.id) return
    // Escape in a comment in progress cancels it (CommentBox) without closing the drawer.
    if ((e.target as HTMLElement | null)?.closest?.('textarea, input')) return
    // Before the window shortcuts (clear the filter, exit zoom).
    e.stopImmediatePropagation()
    if (zoomed) zoomed = null
    else closeTicket()
  }

  let drawerEl: HTMLElement | undefined = $state()
  /** A click outside the drawer closes it (see drawers.ts). */
  function onOutside(e: PointerEvent) {
    if (ticketView.id && !phone && clickedOutside(e, drawerEl)) {
      closeTicket()
    }
  }
  /** The Journey section, folded until you open it (kept for every ticket). */
  let journeyOpen = $state(false)
</script>

<svelte:window onkeydowncapture={onKey} onpointerdowncapture={onOutside} />

{#if ticketView.id}
  <aside class="drawer" class:phone bind:this={drawerEl} data-drawer="50" data-pinned={isPinned('ticket')} aria-label={tr('Ticket {id}', { id: ticketView.id ?? '' })}>
    <header>
      {#if phone}<button class="back" onclick={ticketView.trail.length ? backTicket : closeTicket}><Icon name="chevronLeft" />{ticketView.trail.at(-1) ?? tr('Back')}</button>
      {:else if ticketView.trail.length}<button class="icon" onclick={backTicket} title={tr('Back to {name}', { name: ticketView.trail.at(-1) ?? '' })} aria-label={tr('Back')}><Icon name="back" /></button>{/if}
      {#if phone}<span class="id mono">{t?.id ?? ticketView.id}</span>
      {:else}<button class="id mono" onclick={() => setQuery(t?.id ?? ticketView.id!)} title={tr('Filter the dashboard on {id}', { id: t?.id ?? ticketView.id ?? '' })}>{t?.id ?? ticketView.id}</button>{/if}
      {#if t}
        {#if t.statuses.length && offers('status')}
          <label class="status picker" class:error={!!ticketView.saveError} style="--c: {t.status_color ?? 'var(--text-muted)'}" title={ticketView.saveError ? tr('Status change failed: {error}', { error: ticketView.saveError }) : tr('Change the status in {tracker}', { tracker: trackerName() })}>
            <select class="bare" value={t.status} disabled={ticketView.saving} onchange={(e) => setStatus(e.currentTarget.value)} aria-label={tr('Status')}>
              {#each t.statuses as s (s.status)}<option value={s.status}>{s.status}</option>{/each}
            </select>
            <span class="caret"><Icon name="chevronDown" /></span>
            {#if ticketView.saving}<small>{tr('saving…')}</small>{/if}
          </label>
        {:else}
          <span class="status" style="--c: {t.status_color ?? 'var(--text-muted)'}">{t.status}</span>
        {/if}
        <PrioritySelect taskId={t.task_id} priority={t.priority} />
      {/if}
      <span class="grow"></span>
      <!-- The Time form, its ticket filled in: on the Mac, where it lives. -->
      {#if t && !phone && sprintName}<button class="log" class:armed={sprintArm.is()} onclick={toSprint} disabled={sprintBusy} title={sprintArm.is() ? tr('Click again: {id} is added to {sprint} and stays in its list', { id: t.id, sprint: sprintName }) : tr('Add {id} to {sprint} (it keeps its list)', { id: t.id, sprint: sprintName })}><Icon name="add" />{sprintArm.is() ? tr('Confirm: add to {sprint}', { sprint: sprintName }) : tr('Add to {sprint}', { sprint: sprintName })}</button>{/if}
      {#if t && !phone && offers('time')}<button class="log" onclick={() => openTimeDraft(t.id, '')} title={tr('Log time on {id} (T in its menu)', { id: t.id })}><Icon name="timer" />{tr('Log time')}</button>{/if}
      <button class="icon" onclick={refreshTicket} disabled={ticketView.loading} title={tr('Reload from {tracker}', { tracker: trackerName() })} aria-label={tr('Reload')}><Icon name="refresh" /></button>
      {#if t && !phone}<button class="icon" onclick={() => copy(t.url, tr('Link to {name} copied', { name: t.id }))} title={tr('Copy link')} aria-label={tr('Copy link')}><Icon name="copy" /></button>{/if}
      {#if t && !phone}<a class="icon" href={t.url} target="_blank" rel="noreferrer" title={tr('Open in {app}', { app: trackerName() })} aria-label={tr('Open in {app}', { app: trackerName() })}><Icon name="external" /></a>{/if}
      {#if !phone}<PinButton kind="ticket" />{/if}
      {#if !phone}<button class="icon" onclick={closeTicket} title={tr('Close (Esc)')} aria-label={tr('Close')}><Icon name="close" /></button>{/if}
    </header>

    <div class="body" bind:this={body}>
      {#if ticketView.error}
        <p class="error">{ticketView.error}</p>
      {:else if !t}
        <p class="muted">{tr('Loading {id}…', { id: ticketView.id ?? '' })}</p>
      {:else}
        <h2><TaskType type={t.task_type} glyph={t.task_type_icon} />{t.name}</h2>
        {#if journey && journey.step !== 'done' && journey.why}
          <div class="nextcard" class:ko={journey.failed}>
            <div class="what">
              <b>{journey.why}</b>
              <small>{where(journey)}{#if journey.since}{' '}· {tr('since {ago}', { ago: ago(journey.since) })}{/if}</small>
            </div>
            {#if nextAction && !phone && conventions().next_action !== false}<JourneyAction primary={nextAction} />{/if}
          </div>
        {/if}
        <dl>
          {#if offers('assignees')}
            <dt>{tr('Assignees')}</dt>
            <dd>{t.assignees.map((a) => a.name).join(', ') || tc('ticket', 'Unassigned')}</dd>
          {/if}
          {#if t.list}<dt>{tr('List')}</dt><dd>{t.list}</dd>{/if}
          {#if t.tags.length}<dt>{tr('Tags')}</dt><dd class="tags">{#each t.tags as tag (tag)}<span>{tag}</span>{/each}</dd>{/if}
          {#each t.fields ?? [] as f (f.name)}
            {#if f.estimate}
              <dt>{f.name}</dt><dd><EstimateInput taskId={t.task_id} field={f.name} hours={hoursOf(f)} label={f.name} /></dd>
            {:else}
              <dt>{f.name}</dt><dd class="tags">{#each f.values as v (v.label)}<span class="field" style:--c={v.color}>{v.label}</span>{/each}</dd>
            {/if}
          {/each}
          {#if conventions().estimate_builtin}<dt>{tc('noun', 'Estimate')}</dt><dd><EstimateInput taskId={t.task_id} field={null} hours={t.time_estimate ? t.time_estimate / 3_600_000 : null} label={tc('noun', 'Estimate')} /></dd>
          {:else if t.time_estimate}<dt>{tc('noun', 'Estimate')}</dt><dd>{span(t.time_estimate)}</dd>{/if}
          {#if t.due_at}<dt>{tr('Due')}</dt><dd>{day(t.due_at)}</dd>{/if}
          <dt>{tc('tickets', 'Created')}</dt>
          <dd>{day(t.created_at)}{#if t.creator}{' '}· {t.creator.name}{/if}</dd>
          <dt>{tc('tickets', 'Updated')}</dt>
          <dd>{tr('{ago} ago', { ago: ago(t.updated_at) })}</dd>
          {#if t.parent}<dt>{tr('Parent')}</dt><dd><button class="link" onclick={() => openTicket(t.parent!)}>{tr('Open parent task')}</button></dd>{/if}
        </dl>

        {#if actions.length}
          <section>
            <h3>{actorName()}</h3>
            <div class="acts">
              {#each actions as a (a.id)}
                {#if a.href}<a class="btn claude" href={a.href} onclick={claudeClick(a)} title={`${a.prompt || a.note || a.label}\n${keys('⇧-click')}: ${tr('launch at once')}`}>{a.label}</a>{:else}<span class="btn off" title={a.why}>{a.label}</span>{/if}
              {/each}
            </div>
          </section>
        {/if}

        {#if journey && stages.length}
          <section>
            <FoldTitle label={tr('Journey')} key="ticket.journeyOpen" bind:open={journeyOpen} hint={stepLabel(journey.step)} />
            {#if journeyOpen}
            <JourneyStrip {journey} />
            <ol class="journal">
              {#each stages as g (g.stage)}
                <li class="stage">{g.label}</li>
                {#each g.events as e (e.key)}
                  <li class={e.tone}>
                    {#if e.at}<span class="when" title={stamp(e.at)}>{shortDay(e.at)}</span>{/if}
                    {#if e.mr}{@const m = e.mr}<button class="link plain" onclick={() => showMr(m)} title={tr('Open {name}', { name: mrKey(m) })}>{e.text}</button>{:else}<span>{e.text}</span>{/if}
                    {#if e.detail}<small>{e.detail}</small>{/if}
                  </li>
                {/each}
              {/each}
            </ol>
            {/if}
          </section>
        {/if}
        <TicketActual id={t.id} estimate={estimateOf(t)} spentMs={t.time_spent ?? read?.time_spent} {phone} />

        <section>
          <h3>{tr('Description')}</h3>
          {#if t.description}<Markdown text={t.description} onimage={(url, title) => (zoomed = { url, title })} />{:else}<p class="muted">{tr('No description.')}</p>{/if}
        </section>

        {#if total}
          <section>
            <h3>{tr('Checklists')} <small>{done}/{total}</small></h3>
            {#each t.checklists as c (c.name)}
              {#if t.checklists.length > 1}<p class="sub">{c.name}</p>{/if}
              <ul class="check">
                {#each c.items as item, n (n)}<li class:on={item.resolved}><span><Icon name={item.resolved ? 'checked' : 'unchecked'} /></span>{item.name}</li>{/each}
              </ul>
            {/each}
          </section>
        {/if}

        {#if t.attachments.length}
          <section>
            <h3>{tr('Attachments')} <small>{t.attachments.length}</small></h3>
            {#if images.length}
              <div class="thumbs">
                {#each images as a (a.url)}
                  <figure>
                    <RemoteImage url={a.thumbnail ?? a.url} alt={a.title} onopen={() => (zoomed = { url: a.url, title: a.title, open: a.url })} />
                    <figcaption title={a.title}>{a.title}</figcaption>
                  </figure>
                {/each}
              </div>
            {/if}
            {#each files as a (a.url)}
              <a class="file" href={a.url} target="_blank" rel="noreferrer" title={tr('Open {name} (in the browser, with your ClickUp session)', { name: a.title })}>
                <span class="ext">{a.extension ?? tr('file')}</span><span class="fname">{a.title}</span><span class="fsize">{size(a.size)}</span>
              </a>
            {/each}
          </section>
        {/if}

        {#each deps as g (g.label)}
          <section>
            <h3><span class="dot {g.tone}"></span>{g.label} <small>{g.key === 'waiting' ? tr('{n} open / {total}', { n: g.list.filter((d) => !dependencyDone(d)).length, total: g.list.length }) : g.list.length}</small></h3>
            <ul class="subtasks deps">
              {#each g.list as d (d.task_id)}
                <li class:closed={dependencyDone(d)}>
                  {#if d.id}<button class="link" onclick={() => openTicket(d.id)} title={tr('Preview {id}', { id: d.id })}><span class="mono">{d.id}</span> {d.name}</button>
                  {:else}<a href={d.url || undefined} class="link muted" title={tr('Not read from {tracker} (deleted, or no access)', { tracker: trackerName() })}>{tr('a ticket not read')}</a>{/if}
                  {#if d.status}<span class="pill" style:--sc={statusColor(d.status, t.statuses)}>{d.status}</span>{/if}
                </li>
              {/each}
            </ul>
          </section>
        {/each}

        {#if t.subtasks.length}
          <section>
            <h3>{tr('Subtasks')} <small>{t.subtasks.filter((s) => s.closed).length}/{t.subtasks.length}</small></h3>
            <ul class="subtasks">
              {#each t.subtasks as s (s.task_id)}
                <li class:closed={s.closed}>
                  <button class="link" onclick={() => openTicket(asTicket(s.id) ? s.id : s.task_id)} title={tr('Preview this subtask')}>{s.name}</button>
                  <span class="pill">{s.status}</span>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if mrs.length}
          <section>
            <h3>{tr('Merge requests')} <small>{mrs.length}</small></h3>
            <ul class="mrs">
              {#each mrs as { m, step } (mrKey(m))}
                <li class={step.tone} class:merged={m.state === 'merged'}>
                  <button class="link mono sha" onclick={() => showMr(m)} title={tr('Open {name}', { name: mrKey(m) })}>{mrKey(m)}</button>
                  <button class="link subject" onclick={() => showMr(m)} title={m.title}><MrTitle mr={m} /></button>
                  <span class="cmeta">{#if mrRepos > 1}<span class="crepo">{m.repo ?? ''}</span>{/if}<span class="next">{step.label}</span></span>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if commits?.length}
          <section>
            <h3>{tr('Commits')} <small>{commits.length}{#if unmerged}{' '}· {tr('{n} not merged', { n: unmerged })}{/if}</small></h3>
            <ul class="commits">
              {#each shownCommits as c (c.repo + c.sha)}
                <li class:open={!c.merged}>
                  <span class="cdot" title={c.merged ? tr('In the main or a release branch') : c.branch ? tr('On {branch}, not merged', { branch: c.branch }) : tr('Not in the main nor a release branch')}></span>
                  {#if c.url}<a class="mono sha" href={c.url} target="_blank" rel="noreferrer" title={tr('Open {sha} on the forge', { sha: c.short })}>{c.short}</a>{:else}<span class="mono sha">{c.short}</span>{/if}
                  <span class="subject" title={c.subject}>{c.subject}</span>
                  <span class="cmeta">{#if commitRepos > 1}<span class="crepo">{c.repo}</span>{/if}{#if c.branch}<span class="mono cbranch" title={tr('Its branch')}>{c.branch}</span>{/if}<span title={stamp(c.date)}>{c.author.split(' ')[0]} · {ago(c.date)}</span></span>
                </li>
              {/each}
            </ul>
            {#if commits.length > COMMITS_SHOWN}<button class="link more" onclick={() => (allCommits = !allCommits)}>{allCommits ? tr('Show fewer') : tr('Show all {n}', { n: commits.length })}</button>{/if}
          </section>
        {/if}

        {#if offers('comment') || t.comments.length}
        <section>
          <h3>{tr('Comments')} <small>{t.comments.length}</small></h3>
          {#each t.comments as c, n (c.id || n)}
            <article class="comment">
              <p class="who">
                {#if c.author || !onObsidian()}<b>{c.author?.name ?? tr('Someone')}</b>{/if}<span>{stamp(c.date)}</span>{#if c.replies}<span>· {tn(c.replies, '{n} reply in ClickUp', '{n} replies in ClickUp')}</span>{/if}
                {#if hasCore() && offers('comment_edit') && mine(c) && c.id && editing !== c.id}<button class="edit" onclick={() => (editing = c.id)} title={tr('Edit your comment (rich formatting is replaced by plain text)')}>{tr('Edit')}</button>{/if}
              </p>
              {#if editing === c.id}
                <CommentBox value={c.text} label={tr('Save')} accent="var(--src-clickup)" placeholder={keys(tr('Edit your comment… (⌘↵ to save, Esc to cancel)'))} autofocus onsend={(text) => editComment(t.task_id, c.id, text)} oncancel={() => (editing = null)} />
              {:else}
                <Markdown text={c.text} plain />
              {/if}
            </article>
          {:else}
            <p class="muted">{tr('No comment.')}</p>
          {/each}
          {#if hasCore() && offers('comment')}
            {#key t.task_id}
              <CommentBox label={tr('Comment')} accent="var(--src-clickup)" placeholder={keys(tr('Comment on {id}… (⌘↵ to send)', { id: t.id }))} onsend={(text) => addComment(t.task_id, text)} />
            {/key}
          {/if}
        </section>
        {/if}
      {/if}
    </div>
    {#if phone && t}
      <div class="mainbar">
        {#if nextAction && 'href' in nextAction}<a class="primary" href={nextAction.href} title={nextAction.title}>{nextAction.label}</a>
        {:else if nextAction}<button class="primary" disabled={stepBusy} onclick={runStep} title={nextAction.title}>{stepBusy ? '…' : nextAction.label}</button>
        {:else if mainAction}<a class="primary" href={mainAction.href} onclick={claudeClick(mainAction)}>{mainAction.label}</a>{/if}
        <button onclick={() => (sheet = true)}>{tr('Actions')}</button>
      </div>
    {/if}
    {#if phone && sheet && t}<ActionSheet title="{t.id} {t.name}" actions={sheetActions} onclose={() => (sheet = false)} />{/if}
    {#if zoomed}
      <!-- Enlarged image: click anywhere or Escape to close. -->
      <div class="lightbox" role="dialog" aria-label={zoomed.title} tabindex="-1" onclick={() => (zoomed = null)} onkeydown={() => {}}>
        <RemoteImage url={zoomed.url} alt={zoomed.title} />
        <p class="caption">{zoomed.title}{#if zoomed.open}{' '}· <a href={zoomed.open} target="_blank" rel="noreferrer" onclick={linkOnly}>{tc('tickets', 'open')} <Icon name="external" /></a>{/if}</p>
      </div>
    {/if}
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 50; width: min(560px, 92vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid color-mix(in srgb, var(--src-clickup) 45%, var(--border));
    box-shadow: var(--shadow-drawer);
  }
  header {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0;
    border-bottom: 1px solid var(--border);
    background: var(--surface); box-shadow: inset 0 2px var(--src-clickup);
  }
  .grow { flex: 1; }
  .log { display: inline-flex; align-items: center; gap: 4px; height: 24px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta, 11px); white-space: nowrap; cursor: pointer; }
  .log:hover { border-color: var(--accent); color: var(--accent); }
  .log.armed { border-color: var(--warn); color: var(--warn); }
  .log:disabled { opacity: 0.6; cursor: default; }
  .icon { display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: 15px; line-height: 1; cursor: pointer; }
  .icon:hover { background: var(--surface-muted); color: var(--text); }
  .icon:disabled { opacity: 0.4; cursor: progress; }
  .id { border: none; background: none; padding: 0; color: var(--info); font-size: var(--fs-mono); font-weight: 600; cursor: pointer; }
  .mono { font-family: var(--mono); }
  .status { display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.3px; color: var(--text-muted); white-space: nowrap; }
  .status::before { content: ''; width: 8px; height: 8px; border-radius: var(--r2); background: var(--c); }
  .picker { position: relative; padding: 1px 4px 1px 6px; border: 1px solid var(--border); border-radius: var(--r5); cursor: pointer; }
  .picker:hover { border-color: var(--c); color: var(--text); }
  .picker.error { border-color: var(--ko); }
  .picker select { appearance: none; border: none; background: none; padding: 0 12px 0 0; color: inherit; font: inherit; text-transform: inherit; letter-spacing: inherit; cursor: pointer; outline: none; }
  .picker .caret { position: absolute; right: 5px; pointer-events: none; }
  .picker select:disabled { cursor: progress; }
  .picker small { font-size: var(--fs-tag); text-transform: none; }
  .body { flex: 1; min-height: 0; overflow: auto; padding: 12px 16px 24px; }
  h2 { margin: 0 0 10px; font-size: 15px; font-weight: 600; line-height: 1.35; }
  h3 { margin: 0 0 8px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  h3 small { font-weight: 400; margin-left: 4px; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 3px 14px; margin: 0 0 14px; font-size: var(--fs-meta); }
  dt { color: var(--text-muted); }
  dd { margin: 0; }
  .tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .tags span { padding: 0 6px; border-radius: var(--r8); background: var(--surface-muted); font-size: var(--fs-tag); }
  .tags .field { background: color-mix(in srgb, var(--c, var(--text-muted)) 16%, transparent); color: var(--c, var(--text)); }
  section { padding-top: 12px; margin-top: 12px; border-top: 1px solid var(--border); }
  /* The agent actions, as in the MR panel. */
  .acts { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .btn { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r5); background: none; font: inherit; font-size: var(--fs-meta); white-space: nowrap; text-decoration: none; }
  .btn.claude { color: var(--src-claude); border-color: color-mix(in srgb, var(--src-claude) 40%, var(--border)); cursor: pointer; }
  .btn.claude:hover { border-color: var(--src-claude); text-decoration: none; }
  .btn.off { color: var(--text-muted); cursor: not-allowed; }
  .sub { margin: 6px 0 4px; font-size: var(--fs-meta); font-weight: 500; }
  ul { list-style: none; margin: 0; padding: 0; font-size: var(--fs-body); }
  .check li { display: flex; gap: 6px; padding: 1px 0; }
  .check li span { color: var(--text-muted); }
  .check li.on { color: var(--text-muted); text-decoration: line-through; }
  .subtasks li { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 3px 0; }
  .subtasks li.closed { opacity: 0.55; }
  .commits { margin: 0; padding: 0; list-style: none; }
  .commits li { display: grid; grid-template-columns: 8px auto minmax(0, 1fr) auto; gap: 8px; align-items: center; padding: 3px 0; }
  .cdot { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); }
  .commits li.open .cdot { background: transparent; border: 1.5px solid var(--warn); width: 8px; height: 8px; box-sizing: border-box; }
  .sha { font-size: var(--fs-tag); color: var(--info); text-decoration: none; }
  .sha:hover { text-decoration: underline; }
  .subject { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cmeta { display: inline-flex; gap: 6px; align-items: center; color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .crepo { padding: 0 5px; border-radius: var(--r9); background: var(--surface-muted); }
  .cbranch { max-width: 140px; overflow: hidden; text-overflow: ellipsis; color: var(--warn); }
  .mrs li { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 8px; align-items: center; padding: 3px 0; }
  .mrs .subject { display: block; min-width: 0; color: var(--text); }
  .mrs .next { padding: 0 6px; border-radius: var(--r9); background: color-mix(in srgb, var(--c, var(--text-muted)) 16%, transparent); color: var(--c, var(--text-muted)); }
  .mrs .blocked { --c: var(--ko); } .mrs .todo { --c: var(--warn); } .mrs .wait { --c: var(--info); } .mrs .ready { --c: var(--ok); } .mrs .auto { --c: var(--accent); }
  .mrs li.merged { opacity: 0.7; }
  .more { margin-top: 4px; font-size: var(--fs-meta); }
  .deps li.closed .link { text-decoration: line-through; }
  .deps .pill { background: color-mix(in srgb, var(--sc) 20%, transparent); color: color-mix(in srgb, var(--sc) 75%, var(--text)); }
  h3 .dot { display: inline-block; width: 6px; height: 6px; margin-right: 6px; border-radius: 50%; vertical-align: 1px; background: var(--text-muted); }
  h3 .dot.ko { background: var(--ko); }
  .link { border: none; background: none; padding: 0; font: inherit; color: var(--info); text-align: left; cursor: pointer; }
  .link:hover { text-decoration: underline; }
  .edit { margin-left: auto; border: none; background: none; padding: 0; color: var(--info); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .edit:hover { text-decoration: underline; }
  .comment { padding: 8px 10px; margin-bottom: 8px; border-radius: var(--r7); background: color-mix(in srgb, var(--surface-muted) 60%, transparent); }
  .who { display: flex; gap: 6px; margin: 0 0 4px; font-size: var(--fs-meta); color: var(--text-muted); }
  .who b { color: var(--text); font-weight: 500; }
  .error { color: var(--ko); font-size: var(--fs-body); white-space: pre-wrap; }
  .muted { color: var(--text-muted); font-size: var(--fs-body); }
  .thumbs { display: grid; grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); gap: 8px; margin-bottom: 8px; }
  figure { margin: 0; min-width: 0; }
  figure :global(.img img) { width: 100%; height: 90px; object-fit: cover; }
  figcaption { margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-tag); color: var(--text-muted); }
  .file { display: flex; align-items: center; gap: 8px; padding: 4px 0; font-size: var(--fs-body); color: var(--text); }
  .file:hover .fname { color: var(--info); }
  .ext { flex-shrink: 0; min-width: 34px; padding: 0 4px; border-radius: var(--r4); background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); text-align: center; text-transform: uppercase; }
  .fname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fsize { flex-shrink: 0; font-size: var(--fs-tag); color: var(--text-muted); }
  /* The phone (the remote): the whole screen, larger type and targets, as the MR panel. */
  .drawer.phone { left: 0; width: auto; border-left: none; box-shadow: none; font-size: 14px; }
  .phone header { flex-wrap: wrap; row-gap: 6px; padding: calc(8px + env(safe-area-inset-top)) 8px 8px 4px; }
  .phone .icon { width: 36px; height: 36px; font-size: 17px; }
  .phone .id { font-size: 14px; }
  .phone .picker { padding: 4px 6px 4px 8px; }
  .phone .body { padding: 14px 14px calc(96px + env(safe-area-inset-bottom)); }
  .phone h2 { font-size: 18px; }
  .phone dl { font-size: 14px; gap: 6px 14px; }
  .phone section { margin-top: 16px; }
  .phone .acts { gap: 10px; }
  .phone .btn { padding: 6px 12px; font-size: 14px; }
  .phone .subtasks li, .phone .commits li, .phone .mrs li { padding: 6px 0; }
  .phone .subject { white-space: normal; }
  .back { display: inline-flex; align-items: center; gap: 2px; max-width: 40%; padding: 4px 6px; overflow: hidden; border: 0; background: none; color: var(--accent); font: inherit; font-size: 15px; white-space: nowrap; text-overflow: ellipsis; }
  .mainbar { position: absolute; left: 0; right: 0; bottom: 0; z-index: 3; display: flex; gap: 8px; padding: 10px 12px calc(10px + env(safe-area-inset-bottom)); border-top: 1px solid var(--border); background: color-mix(in srgb, var(--surface) 92%, transparent); backdrop-filter: blur(8px); }
  .mainbar > * { flex: 1; display: flex; align-items: center; justify-content: center; padding: 12px; border: 1px solid var(--border); border-radius: var(--r12); background: var(--surface); color: var(--text); font: inherit; font-size: 15px; text-decoration: none; }
  .mainbar .primary { flex: 2; border-color: var(--accent); background: var(--accent); color: var(--on-accent); font-weight: 600; }
  /* The next action, at the top; the journal, by stage, the steps ahead dashed. */
  .nextcard { display: flex; align-items: center; gap: 10px; margin: 0 0 12px; padding: 8px 10px; border: 1px solid color-mix(in srgb, var(--accent) 50%, var(--border)); border-radius: var(--r8); background: color-mix(in srgb, var(--accent) 8%, var(--surface)); }
  .nextcard.ko { border-color: color-mix(in srgb, var(--ko) 55%, var(--border)); background: color-mix(in srgb, var(--ko) 8%, var(--surface)); }
  .nextcard .what { flex: 1; min-width: 0; }
  .nextcard b { display: block; font-weight: 600; font-size: var(--fs-body); }
  .nextcard small { color: var(--text-muted); font-size: var(--fs-meta); }
  .journal { position: relative; margin: 10px 0 0; padding: 0; list-style: none; font-size: var(--fs-body); }
  .journal::before { content: ''; position: absolute; left: 5px; top: 6px; bottom: 6px; width: 2px; background: var(--border); }
  .journal li { position: relative; padding: 0 0 7px 20px; }
  .journal li::before { content: ''; position: absolute; left: 0; top: 4px; width: 8px; height: 8px; border-radius: 50%; border: 2px solid var(--border); background: var(--surface-muted); }
  .journal li.ok::before { border-color: var(--ok); background: var(--ok); }
  .journal li.ko::before { border-color: var(--ko); background: var(--ko); }
  .journal li.now::before { border-color: var(--accent); background: var(--accent); }
  .journal li.todo { color: var(--text-muted); }
  .journal li.todo::before { border-style: dashed; background: var(--surface); }
  .journal li.stage { padding: 4px 0 5px 20px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .journal li.stage::before { display: none; }
  .journal .when { float: right; margin-left: 8px; color: var(--text-muted); font-size: var(--fs-tag); }
  .journal small { display: block; color: var(--text-muted); font-size: var(--fs-meta); }
  .journal .plain { color: inherit; }
  .lightbox { position: fixed; inset: 0; z-index: 60; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; padding: 24px; background: color-mix(in srgb, #000 82%, transparent); cursor: zoom-out; }
  .lightbox :global(.plain) { max-width: 100%; max-height: calc(100vh - 120px); border-radius: var(--r6); }
  .caption { margin: 0; color: #ddd; font-size: var(--fs-meta); }
  .caption a { color: #9ecbff; }
</style>
