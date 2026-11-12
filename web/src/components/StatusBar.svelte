<script lang="ts">
  import { locale } from '../locale'
  import { anchored, dismissable } from '../popup'
  import { meOn, mrNumber } from '../forge'
  import { agentIcon, agentName } from '../agent-cli'
  import { live } from '../live.svelte'
  import { pref, setPref, showBlock } from '../layout.svelte'
  import { isBusy, openEvent } from '../agenda.svelte'
  import { openMr } from '../mr-view.svelte'
  import { countedSessions, waitingAlert } from '../rules'
  import { planSections } from '../usage'
  import { DEFAULT_ORDER, SIDE_BREAK, STATUS_ITEMS, arrange, fullOrder, overflow, statusItem, type StatusId } from '../statusbar'
  import { statusbarEditing } from '../statusbar-edit.svelte'
  import { appearance } from '../fun.svelte'
  import { saveEdits } from '../settings.svelte'
  import { BLOCK_COMPONENTS } from '../blocks/components'
  import { OWNER } from '../blocks/runtime'
  import ModuleBoundary from '../modules/ModuleBoundary.svelte'
  import type { StatusProps } from '../blocks/block'
  import type { Component } from 'svelte'
  import type { Snapshot } from '../types'
  import AudioControl from './AudioControl.svelte'
  import DigestItem from './DigestItem.svelte'
  import { agentsSettings, openAgents } from '../agents.svelte'
  import { reviewPile } from '../routines.svelte'
  import { asidesOf, badged, showAsides } from '../asides.svelte'
  import { openProposal, proposals } from '../mcp-view.svelte'
  import StatusUsage from './StatusUsage.svelte'
  import RepoMenu from './RepoMenu.svelte'
  import RemoteItem from './RemoteItem.svelte'
  import ShellsItem from './ShellsItem.svelte'
  import ProblemsItem from './ProblemsItem.svelte'
  import AlertsItem from './AlertsItem.svelte'
  import type { Alert } from '../rules'
  import Icon from './Icon.svelte'
  import { isDue, pendingReminders } from '../notes.svelte'
  import { packsReady, updateReady, updates } from '../updates.svelte'
  import { openSettings } from '../settings.svelte'
  import { feedbackOn, newlyShipped, openFeedback } from '../feedback.svelte'
  import { t, tn, tc } from '../i18n.svelte'
  import { initial, profileColor } from '../profiles.svelte'
  import { invoke } from '../bridge'

  /**
   * Status bar, shared by all views (see statusbar.ts). The blocks read the full state,
   * not the filtered view: a filter must not hide the upcoming meeting.
   */
  let { snap, now, status, alerts = [] }: { snap: Snapshot | null; now: number; status: string; alerts?: Alert[] } = $props()

  // The order and the hidden items: `[appearance] statusbar` and `statusbar_hidden`; before them,
  // the hidden items were a pref, read until the first change writes the config.
  let legacy = $state<StatusId[]>(pref<StatusId[]>('statusbarHidden', []))
  const saved = $derived.by(() => {
    const a = appearance()
    const order = a.statusbar ?? []
    const hidden = a.statusbar_hidden ?? []
    return { order, hidden: order.length || hidden.length ? hidden : legacy }
  })
  /** A change saved, until the config reads it back. */
  let local = $state<{ order: string[]; hidden: string[] } | null>(null)
  const same = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i])
  $effect(() => {
    if (local && same(local.order, saved.order) && same(local.hidden, saved.hidden)) local = null
  })
  const current = $derived(local ?? saved)
  const sides = $derived(arrange(current.order, current.hidden))

  async function save(order: string[], hidden: string[]) {
    // The default order is written as none: Prism's later changes to it then apply.
    const full = fullOrder(order)
    const next = { order: same(full, DEFAULT_ORDER) ? [] : full, hidden: STATUS_ITEMS.map((i) => i.id).filter((id) => hidden.includes(id)) }
    local = next
    try {
      await saveEdits([{ path: ['appearance', 'statusbar'], value: next.order }, { path: ['appearance', 'statusbar_hidden'], value: next.hidden }])
      if (legacy.length) {
        legacy = []
        setPref('statusbarHidden', [])
      }
    } catch (e) {
      console.error('status bar', e)
      local = null
    }
  }

  const shows = (id: StatusId) => !current.hidden.includes(id)
  function flip(id: StatusId) {
    void save(current.order, shows(id) ? [...current.hidden, id] : current.hidden.filter((h) => h !== id))
  }
  let menu = $state<{ x: number; y: number } | null>(null)
  function onContext(e: MouseEvent) {
    e.preventDefault()
    menu = { x: e.clientX, y: e.clientY }
  }

  /* Customize: the bar becomes its items' labels, dragged along it or into the tray (hidden). */
  let edit = $state<{ order: string[]; hidden: string[] } | null>(null)
  let editBar = $state<HTMLElement>()
  let editHeight = $state(26)
  let dragged = $state<string | null>(null)
  function startEdit() {
    edit = { order: fullOrder(current.order), hidden: [...current.hidden] }
  }
  $effect(() => {
    if (!statusbarEditing.on) return
    statusbarEditing.on = false
    startEdit()
  })
  function startDrag(e: DragEvent, id: string) {
    dragged = id
    e.dataTransfer?.setData('text/plain', id)
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
  }
  /** Dropped on the bar: before the item under the pointer's left half, otherwise at the end. */
  function dropOnBar(e: DragEvent) {
    e.preventDefault()
    if (!edit || !dragged || !editBar) return
    const id = dragged
    const target = [...editBar.querySelectorAll<HTMLElement>('[data-id]')].find((el) => {
      const r = el.getBoundingClientRect()
      return el.dataset.id !== id && e.clientX < r.left + r.width / 2
    })?.dataset.id
    const order = edit.order.filter((x) => x !== id)
    const at = target ? order.indexOf(target) : order.length
    order.splice(at < 0 ? order.length : at, 0, id)
    edit = { order, hidden: edit.hidden.filter((h) => h !== id) }
    dragged = null
  }
  function dropOnTray(e: DragEvent) {
    e.preventDefault()
    if (!edit || !dragged || dragged === SIDE_BREAK) return
    if (!edit.hidden.includes(dragged)) edit = { ...edit, hidden: [...edit.hidden, dragged] }
    dragged = null
  }
  function finishEdit() {
    if (edit) void save(edit.order, edit.hidden)
    edit = null
  }

  /*
   * Squeezed, the bar moves its least needed items into a ⋯ (`overflow`): each item's width is
   * kept from its last time in the bar, and every change of size (the window, an item saying more)
   * weighs them again.
   */
  let bar = $state<HTMLElement>()
  let remoteEl = $state<HTMLElement>()
  let moreButton = $state<HTMLElement>()
  let moreOpen = $state(false)
  let over = $state<StatusId[]>([])
  const widths = new Map<StatusId, number>()
  const MORE_WIDTH = 30
  const ZONE_WIDTH = 9
  function weigh() {
    if (!bar) return
    for (const el of bar.querySelectorAll<HTMLElement>('.slot[data-id]')) widths.set(el.dataset.id!, el.offsetWidth ? el.offsetWidth + 2 : 0)
    const shown = sides.flatMap((s) => s.zones.flatMap((z) => z.items.map((i) => i.id)))
    const zones = sides.reduce((n, s) => n + s.zones.filter((z) => z.items.some((i) => (widths.get(i.id) ?? 0) > 0)).length, 0)
    const room = bar.clientWidth - 12 - 16 - (remoteEl?.offsetWidth ?? 0) - zones * ZONE_WIDTH
    const next = overflow(shown, widths, room, MORE_WIDTH)
    if (!same(next, over)) over = next
    if (!next.length) moreOpen = false
  }
  let frame = 0
  const sizes = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => {
    cancelAnimationFrame(frame)
    frame = requestAnimationFrame(weigh)
  })
  const observe = (el: HTMLElement) => {
    sizes?.observe(el)
    return () => sizes?.unobserve(el)
  }
  // Another order or other items hidden: weighed again.
  $effect(() => {
    void sides
    cancelAnimationFrame(frame)
    frame = requestAnimationFrame(weigh)
  })

  /** My pipeline: that of one of my open MRs, running first, otherwise failed. */
  const pipeline = $derived.by(() => {
    if (!snap?.me) return null
    const mine = snap.merge_requests.filter((m) => m.state === 'opened' && m.author === meOn(snap, m) && m.head_pipeline)
    const running = mine.find((m) => ['running', 'pending', 'created'].includes(m.head_pipeline!.status))
    const failed = mine.find((m) => m.head_pipeline!.status === 'failed')
    const mr = running ?? failed
    if (!mr) return null
    const p = mr.head_pipeline!
    const stage = p.stages.find((s) => s.status === 'running') ?? p.stages.find((s) => s.finished < s.jobs)
    const jobs = p.stages.reduce((n, s) => n + s.jobs, 0)
    const done = p.stages.reduce((n, s) => n + s.finished, 0)
    return { mr, failed: !running, stage: stage?.name ?? p.status, done, jobs }
  })

  /** Current meeting, otherwise the next one today. */
  const meeting = $derived.by(() => {
    if (snap?.agenda?.access !== 'granted') return null
    const events = snap.agenda.events.filter((e) => !e.all_day && isBusy(e) && new Date(e.end).getTime() > now)
    const e = events.sort((a, b) => a.start.localeCompare(b.start))[0]
    if (!e) return null
    const start = new Date(e.start).getTime()
    const hm = (d: string) => new Date(d).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
    const min = Math.round((start - now) / 60_000)
    const when = start <= now ? t('now · until {time}', { time: hm(e.end) }) : min < 60 ? t('in {min} min', { min }) : t('at {time}', { time: hm(e.start) })
    return { e, when, soon: start > now && min <= 10, ongoing: start <= now }
  })

  /** A block's item (`BlockDef.status`): its component, `<block>:status`. */
  const blockItem = (id: string) => BLOCK_COMPONENTS[`${id}:status`] as unknown as Component<StatusProps> | undefined

  /** Overdue reminders, otherwise the next one if it falls within 12 h. */
  const reminder = $derived.by(() => {
    const pending = pendingReminders()
    const due = pending.filter((n) => isDue(n, now))
    if (due.length) return { due: due.length, text: due[0].text, when: '' }
    const next = pending[0]
    if (!next) return null
    const min = Math.round((new Date(next.remind_at!).getTime() - now) / 60_000)
    if (min > 12 * 60) return null
    const when = min < 60 ? t('in {min} min', { min }) : t('at {time}', { time: new Date(next.remind_at!).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' }) })
    return { due: 0, text: next.text, when }
  })
  const firstLine = (text: string) => text.split('\n')[0]

  const waiting = $derived(snap ? countedSessions(snap).filter(waitingAlert).length : 0)

  const sprintDay = $derived.by(() => {
    const s = snap?.sprint
    if (!s?.start || !s.end) return null
    const DAY = 86_400_000
    const start = new Date(s.start).getTime()
    const total = Math.max(1, Math.round((new Date(s.end).getTime() - start) / DAY))
    return { name: s.name, day: Math.min(total, Math.max(1, Math.floor((now - start) / DAY) + 1)), total }
  })
</script>

{#snippet item(id: StatusId)}
  {#if blockItem(id) && snap}{@const Item = blockItem(id)!}<ModuleBoundary {id} status active={id in OWNER}><Item {snap} {now} /></ModuleBoundary>
  {:else if id === 'live'}
    <span class="sbi" title={status} aria-label={status}><span class="ld" class:on={live.connected}></span></span>
  {:else if id === 'alerts'}
    <AlertsItem {alerts} {now} />
  {:else if id === 'problems'}
    <ProblemsItem {snap} {now} />
  {:else if id === 'repos'}
    <RepoMenu {snap} />
  {:else if id === 'pipeline' && pipeline}
    <button class="sbi gitlab" class:ko={pipeline.failed} onclick={() => openMr(pipeline.mr)} title={pipeline.failed ? t('{title} · pipeline failed', { title: pipeline.mr.title }) : t('{title} · pipeline running', { title: pipeline.mr.title })}>
      <span class="g"><Icon name={pipeline.failed ? 'fail' : 'play'} /></span><b>{mrNumber(pipeline.mr.iid, pipeline.mr.forge)}</b>
      {#if pipeline.failed}{t('failed')}{:else}{pipeline.stage}<span class="mini"><i style:width="{(100 * pipeline.done) / Math.max(1, pipeline.jobs)}%"></i></span>{pipeline.done}/{pipeline.jobs}{/if}
    </button>
  {:else if id === 'meeting' && meeting}
    <button class="sbi calendar" class:soon={meeting.soon} onclick={() => openEvent(meeting.e.id)} title={t('{title} · details', { title: meeting.e.title })}>
      <span class="g"><Icon name="clock" /></span><span class="clip">{meeting.e.title}</span><b>{meeting.when}</b>
    </button>
  {:else if id === 'reminder' && reminder}
    <button class="sbi alert" class:ko={reminder.due > 0} onclick={() => showBlock('notes')} title={reminder.due ? tn(reminder.due, '{n} reminder due · {text}', '{n} reminders due · {text}', { text: reminder.text }) : t('Next reminder {when} · {text}', { when: reminder.when, text: reminder.text })}>
      <span class="g"><Icon name="alarm" /></span>{#if reminder.due}<b>{reminder.due}</b> {t('due')} ·{' '}{/if}<span class="clip">{firstLine(reminder.text)}</span>{#if reminder.when}<b>{reminder.when}</b>{/if}
    </button>
  {:else if id === 'claude' && waiting}
    <button class="sbi claude" onclick={() => showBlock('sessions')} title={t('{agent} sessions waiting for you', { agent: agentName() })}><span class="g fun-beat"><Icon name={agentIcon()} /></span><b>{waiting}</b> {t('{agent} waiting', { agent: agentName() })}</button>
  {:else if id === 'profiles' && snap?.other_profiles?.length}
    <!-- The other profiles open beside this one: who waits there, a click brings its Prism forward. -->
    {#each snap.other_profiles as p (p.id)}
      <button class="sbi prof" class:waits={p.waiting > 0} style:--pc={profileColor(p.color)} onclick={() => void invoke('open_profile', { id: p.id })} title={p.waiting ? tn(p.waiting, '{name}: {n} agent waiting for you ({first}) · show its Prism', '{name}: {n} agents waiting for you, first {first} · show its Prism', { name: p.name, first: p.first ?? '' }) : tn(p.agents, '{name}: {n} agent running · show its Prism', '{name}: {n} agents running · show its Prism', { name: p.name })}>
        <span class="pb">{initial(p.name)}</span>{p.name}{#if p.waiting}{' '}· <b>{p.waiting}</b> {tc('agents', 'waiting')}{:else if p.agents}{' '}· {tn(p.agents, '{n} agent', '{n} agents')}{/if}
      </button>
    {/each}
  {:else if id === 'machines' && snap?.machines?.length}
    <!-- The other computers' Prism (`[[machines]]`): who waits there, a click opens its window. -->
    {#each snap.machines as m (m.name)}
      <button class="sbi prof mach" class:waits={m.waiting.length > 0} class:off={!m.reachable} style:--pc="var(--info)" onclick={() => void invoke('machine_open', { name: m.name })} title={!m.reachable ? t('{name}: {error} · open its window', { name: m.name, error: m.error ?? '' }) : m.waiting.length ? tn(m.waiting.length, '{name}: {n} agent waiting for you ({first}) · open its window', '{name}: {n} agents waiting for you, first {first} · open its window', { name: m.name, first: m.waiting[0].title }) : tn(m.agents, '{name}: {n} agent running · open its window', '{name}: {n} agents running · open its window', { name: m.name })}>
        <span class="pb">{initial(m.name)}</span>{m.name}{#if !m.reachable}{' '}· {t('out of reach')}{:else if m.waiting.length}{' '}· <b>{m.waiting.length}</b> {tc('agents', 'waiting')}{:else if m.agents}{' '}· {tn(m.agents, '{n} agent', '{n} agents')}{/if}
      </button>
    {/each}
  {:else if id === 'shells'}
    <ShellsItem />
  {:else if id === 'digest'}
    <DigestItem {now} />
  {:else if id === 'routines' && reviewPile(snap).length}
    {@const n = reviewPile(snap).length}
    <!-- The routines' runs waiting in "To review", while the pile is not empty. -->
    <button class="sbi" onclick={() => openAgents()} title={t('Routines: {n} to review · the Agents view', { n })}><span class="g"><Icon name="alarm" /></span><b>{n}</b> {t('to review')}</button>
  {:else if id === 'mcp' && (proposals(snap).length || snap?.mcp?.clients.length)}
    {@const ps = proposals(snap)}
    <!-- Agents on Prism's MCP server (`prism mcp`), and their proposals waiting for an answer. -->
    {#if ps.length}
      <!-- In the accent's color while one waits: something asks you, here. -->
      <button class="sbi update asks" onclick={() => openProposal()} title={tn(ps.length, 'An agent proposes a change of Prism: see it, apply or refuse it', '{n} changes of Prism proposed by agents: see them', {})}><span class="g"><Icon name="settings" /></span>MCP · <b>{ps.length}</b> {tn(ps.length, 'proposal', 'proposals')}</button>
    {:else}
      {@const cs = snap!.mcp.clients}
      <button class="sbi" onclick={() => openSettings('prism-mcp')} title={t('Connected to Prism MCP: {who} · Settings › Agents › Prism MCP', { who: cs.map((c) => c.name).join(', ') })}><span class="g"><Icon name="settings" /></span>MCP · {cs.length}</button>
    {/if}
  {:else if id === 'aside' && agentsSettings().aside?.statusbar && asidesOf(snap).some(badged)}
    {@const n = asidesOf(snap).filter(badged).length}
    <!-- The tasks flagged in passing still open ([agents.aside] statusbar): never an alert, never urgent. -->
    <button class="sbi" onclick={() => { openAgents(); showAsides() }} title={t('In passing: {n} open · the Agents view', { n })}><span class="g"><Icon name="aside" /></span><b>{n}</b> {t('in passing')}</button>
  {:else if id === 'usage'}
    <StatusUsage sections={snap ? planSections(snap) : []} {now} working={snap ? countedSessions(snap).some((c) => c.status === 'running') : false} />
  {:else if id === 'sprint' && sprintDay}
    <button class="sbi clickup" onclick={() => showBlock('sprint')} title={t('{name} · day {day} of {total}', { name: sprintDay.name, day: sprintDay.day, total: sprintDay.total })}><span class="g"><Icon name="sprint" /></span><span class="clip">{sprintDay.name}</span>· {t('day')} <b>{sprintDay.day}</b>/{sprintDay.total}</button>
  {:else if id === 'audio'}
    <AudioControl />
  {:else if id === 'update' && updateReady()}
    <button class="sbi update" onclick={() => openSettings('updates')} title={t('Prism {version} is out (this one: {current}) · Settings › Updates', { version: updates.info!.latest!.version, current: updates.info!.current })}><span class="g"><Icon name="up" /></span>Prism <b>{updates.info!.latest!.version}</b></button>
  {:else if id === 'packs' && packsReady().length}
    {@const ps = packsReady()}
    <button class="sbi update" onclick={() => openSettings('packs')} title={tn(ps.length, '{packs} changed at its source · Settings › Packs', '{packs} changed at their sources · Settings › Packs', { packs: ps.map((p) => `${p.name}${p.latest ? ` ${p.latest}` : ''}`).join(', ') })}><span class="g"><Icon name="up" /></span>{#if ps.length > 1}<b>{ps.length}</b> {t('packs')}{:else}Pack <b>{ps[0].name}{#if ps[0].latest} {ps[0].latest}{/if}</b>{/if}</button>
  {:else if id === 'feedback' && feedbackOn()}
    {@const done = newlyShipped()}
    {#if done.length}
      <button class="sbi update" onclick={() => openSettings('feedback')} title={t('{requests} · closed · Settings › Feedback', { requests: done.map((r) => `#${r.number} ${r.title}`).join(' · ') })}><span class="g"><Icon name="check" /></span>{#if done.length > 1}<b>{done.length}</b> {t('requests done')}{:else}<b>#{done[0].number}</b> {tc('core', 'done')}{/if}</button>
    {:else}
      <button class="sbi" onclick={() => openFeedback({ where: 'status bar' })} title={t('Send feedback: an issue on Prism’s repository or a pack’s')}><span class="g"><Icon name="comment" /></span></button>
    {/if}
  {:else if id === 'clock'}
    {@const d = new Date(now)}
    <span class="sbi clock" title={d.toLocaleDateString(locale(), { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' })}>{d.toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })}</span>
  {/if}
{/snippet}

{#if edit}
  {@const shown = edit.order.filter((id) => !edit!.hidden.includes(id))}
  <footer class="statusbar editing" bind:this={editBar} bind:offsetHeight={editHeight} ondragover={(e) => e.preventDefault()} ondrop={dropOnBar}>
    {#each shown as id (id)}
      {#if id === SIDE_BREAK}
        <span class="sep" role="separator" class:dragging={dragged === id} data-id={id} draggable="true" ondragstart={(e) => startDrag(e, id)} ondragend={() => (dragged = null)} title={t('Left | right: drag to move')}></span><span class="grow"></span>
      {:else}
        <span class="chip {statusItem(id)?.zone}" class:dragging={dragged === id} data-id={id} draggable="true" ondragstart={(e) => startDrag(e, id)} ondragend={() => (dragged = null)} role="listitem">{t(statusItem(id)?.label ?? id)}</span>
      {/if}
    {/each}
  </footer>
  <div class="editor" role="dialog" aria-label={t('Customize the status bar')} style:bottom="{editHeight + 8}px" {@attach dismissable(() => (edit = null), { trigger: () => editBar })}>
    <p>{t('Drag the items along the bar; the orange line parts left from right. Drop one here to hide it, back on the bar to show it.')}</p>
    <div class="tray" role="list" ondragover={(e) => e.preventDefault()} ondrop={dropOnTray}>
      {#each edit.hidden as id (id)}
        <span class="chip" class:dragging={dragged === id} data-id={id} draggable="true" ondragstart={(e) => startDrag(e, id)} ondragend={() => (dragged = null)} role="listitem">{t(statusItem(id)?.label ?? id)}</span>
      {:else}
        <span class="empty">{t('Hidden items')}</span>
      {/each}
    </div>
    <div class="acts">
      <button onclick={() => (edit = { order: [...DEFAULT_ORDER], hidden: [] })}>{t('Default order')}</button>
      <span class="grow"></span>
      <button onclick={() => (edit = null)}>{t('Cancel')}</button>
      <button class="pri" onclick={finishEdit}>{t('Done')}</button>
    </div>
  </div>
{:else}
  <footer class="statusbar items" bind:this={bar} {@attach observe} oncontextmenu={onContext} title={t('Right-click to choose the blocks')}>
    <!-- With several profiles, whose window this is: its color and name (each profile in a window of the app). -->
    {#if snap?.profile?.shared}<span class="me" style:--pc={profileColor(snap.profile.color)} title={t('This window: profile {name}', { name: snap.profile.name })}><span class="dot"></span>{snap.profile.name}</span>{/if}
    {#each sides as s (s.side)}
      {#if s.side === 'right'}<span class="grow"></span>{/if}
      <span class="side {s.side}">
        {#if s.side === 'right' && snap?.remote}<span class="fixed" bind:this={remoteEl}><RemoteItem remote={snap.remote} /></span>{/if}
        {#each s.zones as z, i (i)}
          <span class="zone {z.zone}">
            {#each z.items.filter((it) => !over.includes(it.id)) as it (it.id)}
              <span class="slot" data-id={it.id} {@attach observe}>{@render item(it.id)}</span>
            {/each}
          </span>
        {/each}
        {#if s.side === 'right' && over.length}
          <button bind:this={moreButton} class="sbi more" class:open={moreOpen} onclick={(e) => { e.stopPropagation(); moreOpen = !moreOpen }} aria-expanded={moreOpen} title={t('Items without room in the bar')}><Icon name="more" /></button>
        {/if}
      </span>
    {/each}
  </footer>
{/if}

{#if moreOpen && over.length && !edit}
  <div class="overflow items" role="dialog" aria-label={t('Items without room in the bar')} {@attach anchored(() => moreButton, { align: 'end' })} {@attach dismissable(() => (moreOpen = false), { trigger: () => moreButton })}>
    {#each over as id (id)}
      <div class="row">{@render item(id)}</div>
    {/each}
  </div>
{/if}

{#if menu}
  <div class="ctx" role="menu" tabindex="-1" {@attach dismissable(() => (menu = null))} style:left="{Math.min(menu.x, innerWidth - 200)}px" style:bottom="{innerHeight - menu.y + 4}px">
    <p>{t('Status bar')}</p>
    {#each STATUS_ITEMS as b (b.id)}
      <button role="menuitemcheckbox" aria-checked={shows(b.id)} onclick={(e) => { e.stopPropagation(); flip(b.id) }}>
        <span class="check">{#if shows(b.id)}<Icon name="check" />{/if}</span>{t(b.label)}
      </button>
    {/each}
    <hr />
    <button role="menuitem" onclick={() => { menu = null; startEdit() }}><span class="check"></span>{t('Customize…')}</button>
  </div>
{/if}

<style>
  /* Its glass behind it, not on itself: see `.rail::before` (App.svelte). */
  .statusbar::before { content: ''; position: absolute; inset: 0; z-index: -1; backdrop-filter: var(--chrome-blur); -webkit-backdrop-filter: var(--chrome-blur); pointer-events: none; }
  .statusbar { position: relative; flex: 0 0 26px; display: flex; align-items: center; gap: 2px; min-width: 0; padding: 0 6px; overflow: hidden; white-space: nowrap; font-size: var(--fs-meta); color: var(--text-muted); background: color-mix(in srgb, var(--accent) 5%, var(--chrome-bg)); border-top: 1px solid var(--border); }
  .grow { flex: 1; min-width: 8px; }
  /* Sides, zones and slots: a slot keeps its item's width (the ⋯ takes what has no room), an empty
     one (nothing to say) and a zone of empty ones take no room; a thin line parts two zones. */
  .side { display: flex; align-items: center; gap: 2px; height: 100%; min-width: 0; }
  .zone { display: flex; align-items: center; gap: 2px; height: 20px; flex-shrink: 0; }
  .zone:not(:has(> .slot > *)) { display: none; }
  .zone:has(> .slot > *) ~ .zone:has(> .slot > *) { margin-left: 3px; padding-left: 4px; border-left: 1px solid var(--border); border-radius: 0; }
  .zone.you:has(> .slot > *) { background: color-mix(in srgb, var(--warn) 8%, transparent); border-radius: var(--r5); }
  .slot { display: flex; align-items: center; flex-shrink: 0; height: 100%; }
  .slot:not(:has(> *)) { display: none; }
  .fixed { display: flex; flex-shrink: 0; }
  .more.open { background: var(--surface-muted); color: var(--text); }
  .overflow { position: fixed; z-index: 45; display: flex; flex-direction: column; gap: 2px; min-width: 200px; max-width: calc(100vw - 24px); padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); font-size: var(--fs-meta); color: var(--text-muted); }
  .overflow .row { display: flex; align-items: center; min-height: 22px; }
  .overflow .row:not(:has(> *)) { display: none; }
  /* Customize: the labels on the bar, dashed; the line between the sides; the tray of hidden items. */
  .statusbar.editing { flex-basis: auto; flex-wrap: wrap; row-gap: 3px; padding: 3px 6px; background: color-mix(in srgb, var(--accent) 12%, var(--chrome-bg)); }
  .chip { display: flex; align-items: center; flex-shrink: 0; height: 20px; padding: 0 7px; border: 1px dashed color-mix(in srgb, var(--accent) 45%, transparent); border-radius: var(--r5); color: var(--text); cursor: grab; }
  .chip.you { background: color-mix(in srgb, var(--warn) 10%, transparent); }
  .chip.dragging, .sep.dragging { opacity: 0.35; }
  .sep { flex-shrink: 0; width: 6px; height: 18px; margin: 0 2px; border-left: 2px solid var(--accent); cursor: grab; }
  .editor { position: fixed; z-index: 46; left: 50%; transform: translateX(-50%); display: flex; flex-direction: column; gap: 8px; width: min(640px, calc(100vw - 24px)); padding: 10px 12px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r10); box-shadow: var(--shadow-pop); font-size: var(--fs-meta); color: var(--text-muted); }
  .editor p { margin: 0; }
  .tray { display: flex; flex-wrap: wrap; gap: 4px; min-height: 30px; padding: 4px; border: 1px dashed var(--border); border-radius: var(--r8); }
  .tray .chip { border-color: var(--border); color: var(--text-muted); }
  .tray .empty { align-self: center; padding: 0 6px; opacity: 0.7; }
  .acts { display: flex; gap: 6px; }
  .acts button { padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text); font: inherit; cursor: pointer; }
  .acts button:hover { background: var(--surface-muted); }
  .acts .pri { border-color: var(--accent); color: var(--accent); }
  /* Squeezed, a block cuts its text rather than drawing over its neighbour. */
  /* The items' vocabulary, global: a block's item uses it too (`BlockDef.status`). */
  .items :global(.sbi) { display: flex; align-items: center; gap: 5px; height: 20px; min-width: 0; overflow: hidden; padding: 0 7px; border: none; border-radius: var(--r5); background: none; color: inherit; font: inherit; cursor: default; }
  .items :global(button.sbi) { cursor: pointer; }
  .items :global(button.sbi:hover) { background: var(--surface-muted); color: var(--text); }
  .items :global(.sbi b) { font-weight: 600; color: var(--text); font-variant-numeric: tabular-nums; }
  /* The time, for a window in full screen: the system's clock is hidden. */
  .clock { color: var(--text); font-family: var(--mono); font-variant-numeric: tabular-nums; }
  .clip { min-width: 0; max-width: 220px; overflow: hidden; text-overflow: ellipsis; }
  .ld { width: 7px; height: 7px; border-radius: 50%; background: var(--ko); }
  .ld.on { background: var(--ok); }
  .gitlab .g { color: var(--src-gitlab); }
  .calendar .g { color: var(--src-calendar); }
  .claude .g { color: var(--src-claude); }
  .clickup .g { color: var(--src-clickup); }
  .update .g { color: var(--accent); }
  .sbi.asks { background: color-mix(in srgb, var(--accent) 18%, transparent); color: var(--accent); font-weight: 600; }
  .sbi.asks .g { color: inherit; }
  .items :global(.g) { display: inline-flex; flex-shrink: 0; }
  .items :global(.sbi.ko), .items :global(.sbi.ko b), .items :global(.sbi.ko .g) { color: var(--ko); }
  .sbi.soon { background: color-mix(in srgb, var(--src-calendar) 14%, transparent); }
  /* Another profile open beside this one: its initial on its color; tinted while one of its agents waits. */
  .me { flex: none; display: inline-flex; align-items: center; gap: 5px; padding: 0 8px 0 2px; margin-right: 2px; color: var(--text); font-weight: 600; border-right: 1px solid var(--border); }
  .me .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--pc); }
  .sbi.prof { flex: none; }
  .sbi.prof .pb { display: inline-grid; place-items: center; width: 14px; height: 14px; border-radius: var(--r4); border: 1px solid color-mix(in srgb, var(--pc) 55%, transparent); background: color-mix(in srgb, var(--pc) 20%, transparent); color: var(--pc); font-size: 9px; font-weight: 700; }
  .sbi.prof.waits { background: color-mix(in srgb, var(--pc) 14%, transparent); }
  .sbi.prof.waits b { color: var(--warn); }
  /* Another computer's: its badge round, as its dot elsewhere; out of reach, faded. */
  .sbi.mach .pb { border-radius: 50%; }
  .sbi.mach.off { opacity: 0.55; }
  .mini { width: 38px; height: 4px; border-radius: var(--r2); overflow: hidden; background: var(--surface-muted); }
  .mini i { display: block; height: 100%; background: var(--src-gitlab); }
  .ctx { position: fixed; z-index: 50; width: 190px; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); font-size: var(--fs-body); }
  .ctx p { margin: 2px 8px 4px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .ctx button { display: flex; align-items: center; gap: 6px; width: 100%; padding: 4px 8px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .ctx button:hover { background: var(--surface-muted); }
  .ctx hr { margin: 4px 6px; border: none; border-top: 1px solid var(--border); }
  .check { display: inline-flex; width: 12px; color: var(--accent); }
</style>
