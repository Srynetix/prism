<script lang="ts">
  import { suggest } from '../suggest'
  import { t as tr } from '../i18n.svelte'
  import { dateFormat } from '../locale'
  import { meOn } from '../forge'
  import { armer } from '../armed.svelte'
  import { asTicket, conventions, hasTickets, mrTickets, ticketExample } from '../conventions'
  import Logo from './Logo.svelte'
  import { trackerLogo, trackerName } from '../tracker'
  import Icon from './Icon.svelte'
  import type { Snapshot, TimeEntry } from '../types'
  import { filter, selectedDay, setDate, todayKey } from '../filter.svelte'
  import { timeDraft } from '../timeDraft.svelte'
  import { exclusive } from '../popup'
  import { parseDuration } from '../time'
  import { entryDay, hours, isWorkday, tone as toneOf, total, weekOf } from '../timesheet'
  import { pref, setPref } from '../layout.svelte'
  import TimeReport from './TimeReport.svelte'
  import { hasCore, invoke } from '../bridge'

  let { snap }: { snap: Snapshot } = $props()

  /** Logging target: `[clickup] day_hours` per working day, five days a week. */
  const DAILY_TARGET_MS = $derived(conventions().day_hours * 3600_000)
  const WEEK_TARGET_MS = $derived(5 * DAILY_TARGET_MS)
  /** Start time of an entry when nothing is logged yet that day. */
  const DAY_START = '09:00'

  const day = $derived(selectedDay())
  const isToday = $derived(day === todayKey())

  /** Day (entry, the day's entries), Week (timesheet) or Month (calendar): kept between openings. */
  type Mode = 'day' | 'week' | 'month'
  const MODES: { id: Mode; label: string }[] = [{ id: 'day', label: tr('Day') }, { id: 'week', label: tr('Week') }, { id: 'month', label: tr('Month') }]
  const mode = $derived(pref<Mode>('time.view', 'day'))
  const setMode = (m: Mode) => setPref('time.view', m)
  /** A day of the Week or Month view: the Day view on it. */
  function openDay(k: string) {
    setDate(k)
    setMode('day')
  }

  const dayEntries = $derived(snap.time.entries.filter((e) => entryDay(e) === day))
  const dayMs = $derived(total(dayEntries))
  const dayPct = $derived(Math.min(100, (dayMs / DAILY_TARGET_MS) * 100))

  /** Week (Monday → Sunday) of the day viewed. */
  const week = $derived(weekOf(day))
  const weekMs = $derived(total(snap.time.entries.filter((e) => week.includes(entryDay(e)))))

  const target = (ms: number) => `${Math.round(ms / 3600_000)}h`
  const label = (k: string, style: 'short' | 'long' = 'short') =>
    dateFormat(style === 'short' ? { weekday: 'short', day: 'numeric' } : { weekday: 'short', day: 'numeric', month: 'short' }).format(new Date(`${k}T12:00`))
  const tone = (k: string, ms: number) => toneOf(k, ms, DAILY_TARGET_MS, todayKey())

  /** The entry starts right after the day's last one, otherwise at the start of the day. */
  const nextStart = $derived.by(() => {
    const ends = dayEntries.map((e) => new Date(e.start).getTime() + e.duration_ms)
    return ends.length ? new Date(Math.max(...ends)) : new Date(`${day}T${DAY_START}`)
  })
  const time = (d: Date) => dateFormat({ hour: '2-digit', minute: '2-digit' }).format(d)

  /** Tickets offered for logging: sprint, my open MRs, my sessions, cross-cutting tasks. */
  const suggestions = $derived.by(() => {
    const names = new Map<string, string>()
    for (const t of snap.sprint?.tickets ?? []) if (!t.closed) names.set(t.id, t.name)
    for (const m of snap.merge_requests) if (m.author === meOn(snap, m) && m.state === 'opened') for (const id of mrTickets(m)) if (!names.has(id)) names.set(id, m.title)
    for (const c of snap.sessions) if (c.ticket && !names.has(c.ticket)) names.set(c.ticket, c.title)
    for (const t of snap.time_tasks?.tickets ?? []) if (!names.has(t.id)) names.set(t.id, t.name)
    return [...names.entries()]
  })

  /** The global filter's ticket, if any, pre-fills the entry. */
  const filtered = $derived(asTicket(filter.query) ?? '')
  let duration = $state('')
  let busy = $state(false)
  let error = $state('')
  let durationInput: HTMLInputElement | undefined = $state()

  function toggle() {
    timeDraft.open = !timeDraft.open
    if (timeDraft.open && !timeDraft.ticket) timeDraft.ticket = filtered
    error = ''
  }

  async function log() {
    const ticket = timeDraft.ticket.trim().toUpperCase()
    if (!hasTickets()) return void (error = tr('Logging time needs ticket ids: set the Ticket prefix in Settings › Work › Tracker.'))
    if (!asTicket(ticket)) return void (error = tr('Enter a ticket id like {example}.', { example: ticketExample() }))
    const minutes = parseDuration(duration)
    if (!minutes) return void (error = tr('Enter a duration like 1h30, 45m or 1.5.'))
    if (!hasCore()) return void (error = tr('Time entry is only available in the Prism app.'))
    busy = true
    error = ''
    try {
      await invoke('log_time', { entry: { ticket, start: nextStart.toISOString(), minutes, description: timeDraft.description } })
      // Logged: the form starts over, the next entry is likely another ticket's.
      timeDraft.ticket = ''
      duration = ''
      timeDraft.description = ''
    } catch (e) {
      error = typeof e === 'string' ? e : (e as Error).message
    } finally {
      busy = false
    }
  }

  /** Cross-cutting tasks: collapsed by default, parents followed by their subtasks. */
  const RECENT = 5
  let showTransverse = $state(false)
  let search = $state('')
  const transverse = $derived(snap.time_tasks?.tickets ?? [])
  const recent = $derived(
    snap.time.recent_tickets
      .map((id) => transverse.find((t) => t.id === id))
      .filter((t): t is NonNullable<typeof t> => !!t)
      .slice(0, RECENT),
  )
  const tree = $derived.by(() => {
    const needle = search.trim().toLowerCase()
    const byTask = new Map(transverse.map((t) => [t.task_id, t]))
    const matches = (t: (typeof transverse)[number]) => !needle || t.name.toLowerCase().includes(needle) || t.id.toLowerCase().includes(needle)
    const roots = transverse.filter((t) => !t.parent || !byTask.has(t.parent)).sort((a, b) => a.name.localeCompare(b.name))
    return roots.flatMap((r) => {
      const children = transverse.filter((c) => c.parent === r.task_id).sort((a, b) => a.name.localeCompare(b.name))
      const kept = children.filter(matches)
      // A parent stays shown if one of its subtasks matches, to keep the context.
      if (!matches(r) && kept.length === 0) return []
      return [{ t: r, child: false }, ...(matches(r) ? children : kept).map((c) => ({ t: c, child: true }))]
    })
  })
  const tip = (t: (typeof transverse)[number]) => `${t.id} · ${t.name}${t.description ? `\n\n${t.description}` : ''}`

  function pick(id: string) {
    timeDraft.ticket = id
    timeDraft.description = ''
    error = ''
    durationInput?.focus()
  }

  /**
   * Two-click deletion of an entry: the first arms the cross for a few seconds, the
   * second deletes the entry in the tracker.
   */
  const arming = armer()
  let deleting = $state<string | null>(null)
  let deleteError = $state<{ id: string; message: string } | null>(null)
  async function remove(e: TimeEntry) {
    if (deleting) return
    if (!arming.confirm(e.id)) {
      deleteError = null
      return
    }
    deleting = e.id
    try {
      await invoke('delete_time', { id: e.id })
    } catch (err) {
      deleteError = { id: e.id, message: String(err) }
    } finally {
      deleting = null
    }
  }

  /**
   * Editing an entry, on its row: ticket, start time (the day does not change),
   * duration, description. Pencil or double-click; Enter saves, Escape cancels. A cleared
   * ticket keeps the entry's task.
   */
  type Edit = { id: string; day: string; ticket: string; at: string; duration: string; description: string }
  let editing = $state<Edit | null>(null)
  let saving = $state(false)
  let editError = $state('')
  function startEdit(e: TimeEntry) {
    if (!hasCore() || saving) return
    arming.disarm()
    editError = ''
    editing = { id: e.id, day: entryDay(e), ticket: e.ticket ?? '', at: clock24(new Date(e.start)), duration: hours(e.duration_ms), description: e.description }
  }
  async function saveEdit() {
    const ed = editing
    if (!ed || saving) return
    const ticket = ed.ticket.trim().toUpperCase()
    if (ticket && !asTicket(ticket)) return void (editError = tr('Enter a ticket id like {example}, or leave it empty to keep the task.', { example: ticketExample() }))
    const minutes = parseDuration(ed.duration)
    if (!minutes) return void (editError = tr('Enter a duration like 1h30, 45m or 1.5.'))
    const at = startTime(ed.at)
    const start = new Date(`${ed.day}T${at}`)
    if (!at || Number.isNaN(start.getTime())) return void (editError = tr('Enter a start time like 09:30.'))
    saving = true
    editError = ''
    try {
      await invoke('update_time', { id: ed.id, entry: { ticket, start: start.toISOString(), minutes, description: ed.description.trim() } })
      editing = null
    } catch (err) {
      editError = String(err)
    } finally {
      saving = false
    }
  }
  function onEditKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault()
      void saveEdit()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      editing = null
    }
  }
  const focusFirst = (el: HTMLInputElement) => el.focus()
  /** The start time, typed rather than in the system's time field (not the same on macOS and Linux): 24 h, `HH:MM`. */
  const clock24 = (d: Date) => `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
  /** `9:30`, `09:30` or `930` → `09:30`; null when it is not a time. */
  function startTime(typed: string): string | null {
    const m = /^(\d{1,2}):?(\d{2})$/.exec(typed.trim())
    if (!m || Number(m[1]) > 23 || Number(m[2]) > 59) return null
    return `${m[1].padStart(2, '0')}:${m[2]}`
  }

  let root: HTMLElement | undefined = $state()
  /**
   * A press outside the button and the popup closes it; the ticket and description in
   * progress stay in `timeDraft`. A row's "Log time" menu, which opens it, is not
   * affected: the popup is only opened after that press.
   */
  function onOutside(e: PointerEvent) {
    const target = e.target as Element | null
    if (!timeDraft.open || !target?.isConnected || root?.contains(target)) return
    timeDraft.open = false
  }
</script>

<svelte:window onpointerdowncapture={onOutside} />

<div class="tt" bind:this={root}>
  <button class="chip" class:done={dayPct >= 100} data-time-chip onclick={toggle} title="{tr('Time tracking')} · {label(day, 'long')}">
    <Logo name={trackerLogo()} size={12} />
    <b>{hours(dayMs)}</b>/{target(DAILY_TARGET_MS)}<span class="day"> {isToday ? tr('today') : label(day)}</span>
    <span class="progress"><i style="width: {dayPct}%"></i></span>
  </button>

  {#if timeDraft.open}
    <div class="drawer" class:wide={mode !== 'day'} role="dialog" aria-label={tr('Time tracking')} {@attach exclusive(() => (timeDraft.open = false))}>
      <div class="head">
        <div class="modes" role="tablist" aria-label={tr('Time view')}>
          {#each MODES as m (m.id)}
            <button role="tab" aria-selected={mode === m.id} class:on={mode === m.id} onclick={() => setMode(m.id)}>{m.label}</button>
          {/each}
        </div>
        {#if mode === 'day'}
          <b>{isToday ? tr('Today') : label(day, 'long')}</b>
          <span class="muted">{hours(dayMs)}/{target(DAILY_TARGET_MS)} · {tr('week')} {hours(weekMs)}/{target(WEEK_TARGET_MS)}</span>
        {:else}
          <span class="muted"></span>
        {/if}
        <button class="x" onclick={() => (timeDraft.open = false)} aria-label={tr('Close')}><Icon name="close" /></button>
      </div>

      {#if mode !== 'day'}
        <TimeReport entries={snap.time.entries} {mode} {day} dayMs={DAILY_TARGET_MS} onDay={openDay} />
      {:else}
      <div class="form">
        <label class="ticket">{tr('Ticket')}
          <input {@attach suggest(() => suggestions.map(([value, label]) => ({ value, label })))} bind:value={timeDraft.ticket} oninput={() => (error = '')} placeholder={hasTickets() ? ticketExample() : tr('no ticket ids')} spellcheck="false" />
        </label>
        <label class="duration">{tr('Duration')}
          <input bind:this={durationInput} bind:value={duration} oninput={() => (error = '')} placeholder="1h30" onkeydown={(e) => e.key === 'Enter' && log()} />
        </label>
        <label class="wide">{tr('Description')}
          <input bind:value={timeDraft.description} placeholder={tr('Optional')} onkeydown={(e) => e.key === 'Enter' && log()} />
        </label>
        <div class="actions">
          <span class="muted hint">{tr('{day}, from {time}', { day: label(day, 'long'), time: time(nextStart) })}</span>
          <button class="primary" onclick={log} disabled={busy}>{tr('Log time')}</button>
        </div>
        {#if error}<div class="error">{error}</div>{/if}
      </div>

      {#if transverse.length}
        <button class="section" onclick={() => (showTransverse = !showTransverse)} aria-expanded={showTransverse}>
          <span class="chev" class:open={showTransverse}><Icon name="chevronRight" /></span> {snap.time_tasks?.name ?? tr('Tasks')} <span class="muted">{transverse.length}</span>
        </button>
        {#if showTransverse}
          <div class="transverse">
            {#if recent.length}
              <div class="recent">
                {#each recent as t (t.id)}
                  <button class="chip-t" onclick={() => pick(t.id)} title={tip(t)}>{t.name}</button>
                {/each}
              </div>
            {/if}
            <input class="search" bind:value={search} placeholder={snap.time_tasks?.name ? tr('Filter {name} tasks…', { name: snap.time_tasks.name }) : tr('Filter these tasks…')} spellcheck="false" />
            <div class="tree">
              {#each tree as { t, child } (t.id)}
                <div class="node" class:child class:on={timeDraft.ticket === t.id}>
                  <button class="name" onclick={() => pick(t.id)} title={tip(t)}>{#if child}<Icon name="subtask" /> {/if}{t.name}</button>
                  <a class="icon" href={t.url} target="_blank" rel="noreferrer" title={tr('Open {name} in {app}', { name: t.id, app: trackerName() })} aria-label={tr('Open {name} in {app}', { name: t.name, app: trackerName() })}><Icon name="external" /></a>
                </div>
              {:else}
                <div class="empty">{tr('No task matches.')}</div>
              {/each}
            </div>
          </div>
        {/if}
      {/if}

      <div class="entries">
        <div class="week">
          {#each week as k (k)}
            {@const ms = total(snap.time.entries.filter((e) => entryDay(e) === k))}
            <button
              class="wd {tone(k, ms)}"
              class:sel={k === day}
              class:off={!isWorkday(k)}
              disabled={k > todayKey()}
              onclick={() => setDate(k)}
              title="{label(k, 'long')} · {hours(ms)}"
            >
              <span>{label(k)}</span>
              <span class="bar"><i style="height: {Math.min(100, (ms / DAILY_TARGET_MS) * 100)}%"></i></span>
              <span class="mono">{ms ? hours(ms) : '—'}</span>
            </button>
          {/each}
        </div>
        {#each dayEntries as e (e.id)}
          {#if editing?.id === e.id}
            <!-- svelte-ignore a11y_no_static_element_interactions : the fields receive Enter and Escape -->
            <div class="entry editing" onkeydown={onEditKey}>
              <input class="e-at mono" type="text" inputmode="numeric" maxlength="5" placeholder="09:30" bind:value={editing.at} aria-label={tr('Start time (24 h)')} />
              <input class="e-ticket mono" {@attach suggest(() => suggestions.map(([value, label]) => ({ value, label })))} bind:value={editing.ticket} placeholder={e.ticket ?? '—'} spellcheck="false" aria-label={tr('Ticket')} />
              <input class="e-desc" bind:value={editing.description} placeholder={tr('Description')} use:focusFirst aria-label={tr('Description')} />
              <input class="e-dur mono" bind:value={editing.duration} placeholder="1h30" aria-label={tr('Duration')} />
              <button class="save" onclick={saveEdit} disabled={saving} title={tr('Save in {tracker} (Enter)', { tracker: trackerName() })} aria-label={tr('Save')}>{#if saving}…{:else}<Icon name="check" />{/if}</button>
              <button class="del cancel" onclick={() => (editing = null)} title={tr('Cancel (Esc)')} aria-label={tr('Cancel')}><Icon name="close" /></button>
            </div>
            {#if editError}<div class="error edit-error">{editError}</div>{/if}
          {:else}
          <!-- svelte-ignore a11y_no_static_element_interactions : the pencil is the keyboard path -->
          <div class="entry" ondblclick={(ev) => !(ev.target as HTMLElement).closest('a, button') && startEdit(e)} title={hasCore() ? tr('Double-click to edit') : undefined}>
            <span class="mono muted">{time(new Date(e.start))}</span>
            <a class="mono" href={e.task_url ?? '#'} target="_blank" rel="noreferrer">{e.ticket ?? '—'}</a>
            <span class="grow" title={e.task_name ?? ''}>{e.description || e.task_name || ''}</span>
            <span class="mono muted">{hours(e.duration_ms)}</span>
            {#if hasCore()}
              <button class="del edit" onclick={() => startEdit(e)} title={tr('Edit this entry')} aria-label={tr('Edit this entry')}><Icon name="edit" /></button>
              <button
                class="del"
                class:armed={arming.is(e.id)}
                class:error={deleteError?.id === e.id}
                disabled={deleting === e.id}
                onclick={() => remove(e)}
                title={deleteError?.id === e.id ? tr('Could not delete: {error}', { error: deleteError.message }) : arming.is(e.id) ? tr('Click again to delete this entry in {tracker}', { tracker: trackerName() }) : tr('Delete this entry')}
                aria-label={arming.is(e.id) ? tr('Confirm: delete this entry') : tr('Delete this entry')}
              >{#if deleting === e.id}…{:else if arming.is(e.id)}{tr('delete?')}{:else}<Icon name="close" />{/if}</button>
            {/if}
          </div>
          {/if}
        {:else}
          <div class="empty">{tr('No time logged on {day}.', { day: label(day, 'long') })}</div>
        {/each}
      </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tt { position: relative; display: flex; align-items: center; flex-shrink: 0; }
  .chip { display: flex; align-items: baseline; gap: 5px; padding: 3px 9px; border-radius: var(--r6); border: 1px solid color-mix(in srgb, var(--accent) 55%, var(--border)); background: var(--surface-muted); color: var(--text-muted); font: inherit; font-size: var(--fs-body); cursor: pointer; white-space: nowrap; }
  /* After `font: inherit`, which would reset the tabular figures. */
  .chip { position: relative; overflow: hidden; font-variant-numeric: tabular-nums; }
  .chip b { color: var(--text); font-weight: 500; font-size: 13px; }
  .chip.done b { color: var(--ok); }
  .progress { position: absolute; left: 0; right: 0; bottom: 0; height: 2px; }
  .progress i { display: block; height: 100%; background: var(--accent); transition: width 0.3s; }
  .chip.done .progress i { background: var(--ok); }
  /* Above the side panes (ticket, MR, logs: 49 to 52), from which a ticket's time is logged. */
  .drawer { position: absolute; top: calc(100% + 8px); right: 0; z-index: 55; width: 400px; max-height: 80vh; display: flex; flex-direction: column; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r10); box-shadow: var(--shadow-pop); }
  .drawer.wide { width: 580px; }
  .modes { display: inline-flex; padding: 2px; border-radius: var(--r6); background: var(--surface-muted); border: 1px solid var(--border); align-self: center; }
  .modes button { padding: 1px 8px; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .modes button.on { background: var(--surface); color: var(--text); box-shadow: 0 0 0 1px var(--border); }
  .head { display: flex; align-items: baseline; gap: 10px; padding: 10px 12px; border-bottom: 1px solid var(--border); }
  .head .muted { flex: 1; font-size: var(--fs-meta); }
  .x { display: inline-flex; align-items: center; border: none; background: none; color: var(--text-muted); font-size: 16px; cursor: pointer; }
  .form { display: grid; grid-template-columns: 1fr 90px; gap: 8px; padding: 10px 12px; border-bottom: 1px solid var(--border); }
  label { display: flex; flex-direction: column; gap: 3px; font-size: var(--fs-meta); color: var(--text-muted); }
  label.wide, .actions, .error { grid-column: 1 / 3; }
  input { font: inherit; font-size: var(--fs-body); padding: 5px 7px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); min-width: 0; }
  input:focus { outline: none; border-color: var(--info); }
  .actions { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .hint { font-size: var(--fs-meta); }
  button.primary { font: inherit; font-size: var(--fs-body); padding: 4px 12px; border-radius: var(--r6); border: none; background: var(--info-bg); color: var(--info); cursor: pointer; }
  button.primary:disabled { opacity: 0.6; cursor: progress; }
  .error { font-size: var(--fs-body); color: var(--ko); }
  .section { display: flex; align-items: center; gap: 6px; width: 100%; padding: 7px 12px; border: none; border-bottom: 1px solid var(--border); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .section:hover { background: var(--surface-muted); }
  .chev { display: inline-flex; color: var(--text-muted); transition: transform 0.15s; }
  .chev.open { transform: rotate(90deg); }
  .transverse { display: flex; flex-direction: column; gap: 6px; padding: 8px 12px; border-bottom: 1px solid var(--border); }
  .recent { display: flex; flex-wrap: wrap; gap: 5px; }
  .chip-t { max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 3px 9px; font: inherit; font-size: var(--fs-mono); border-radius: var(--r12); border: 1px solid var(--border); background: var(--surface-muted); color: var(--text); cursor: pointer; }
  .chip-t:hover { border-color: var(--info); }
  .search { font-size: var(--fs-mono); }
  .tree { max-height: 180px; overflow: auto; border: 1px solid var(--border); border-radius: var(--r6); }
  .node { display: flex; align-items: center; gap: 4px; padding: 0 6px 0 0; border-top: 1px solid var(--border); }
  .node:first-child { border-top: none; }
  .node .name { flex: 1; min-width: 0; padding: 4px 8px; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; cursor: pointer; }
  .node.child .name { padding-left: 18px; color: var(--text-muted); }
  .node:hover { background: var(--surface-muted); }
  .node.on .name { color: var(--info); font-weight: 500; }
  .node .icon { padding: 0 4px; font-size: var(--fs-meta); color: var(--text-muted); opacity: 0; text-decoration: none; }
  .node:hover .icon, .node .icon:focus-visible { opacity: 1; }
  .node .icon:hover { color: var(--info); }
  .entries { overflow: auto; min-height: 0; }
  .week { display: grid; grid-template-columns: repeat(7, 1fr); gap: 4px; padding: 8px 12px; background: var(--surface-muted); border-bottom: 1px solid var(--border); }
  .wd { display: flex; flex-direction: column; align-items: center; gap: 3px; padding: 4px 0; border: 1px solid transparent; border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .wd:disabled { opacity: 0.35; cursor: default; }
  .wd.off { opacity: 0.55; }
  .wd.sel { border-color: var(--info); color: var(--text); }
  .wd .bar { position: relative; width: 6px; height: 26px; border-radius: var(--r3); background: var(--border); overflow: hidden; }
  .wd .bar i { position: absolute; left: 0; right: 0; bottom: 0; background: var(--text-muted); }
  .wd.ok .bar i { background: var(--ok); }
  .wd.warn .bar i { background: var(--warn); }
  .wd.warn .mono { color: var(--warn); }
  .wd.ok .mono { color: var(--ok); }
  .wd .mono { font-size: var(--fs-tag); }
  .entry { display: flex; gap: 8px; align-items: center; padding: 5px 12px; border-top: 1px solid var(--border); font-size: var(--fs-body); }
  .del { flex-shrink: 0; display: inline-flex; align-items: center; justify-content: center; min-width: 20px; height: 20px; padding: 0 4px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; opacity: 0; }
  .entry:hover .del, .del:focus-visible, .del.armed, .del.error { opacity: 1; }
  .del:hover, .del.armed, .del.error { color: var(--ko); border-color: color-mix(in srgb, var(--ko) 40%, transparent); }
  .del.armed { background: color-mix(in srgb, var(--ko) 12%, transparent); }
  .del:disabled { cursor: progress; }
  .del.edit:hover { color: var(--info); border-color: color-mix(in srgb, var(--info) 40%, transparent); }
  .entry.editing { gap: 5px; padding: 4px 8px 4px 12px; background: color-mix(in srgb, var(--info) 6%, var(--surface)); }
  .entry.editing input { padding: 2px 5px; font-size: var(--fs-meta); }
  .entry.editing .e-at { width: 64px; }
  .entry.editing .e-ticket { width: 82px; }
  .entry.editing .e-desc { flex: 1; }
  .entry.editing .e-dur { width: 50px; }
  .entry.editing .del { opacity: 1; }
  /* Not `.ok`: that is the tone of a complete day in the week (`.wd.ok`). */
  .save { flex-shrink: 0; display: inline-flex; align-items: center; justify-content: center; min-width: 20px; height: 20px; padding: 0 4px; border: 1px solid color-mix(in srgb, var(--info) 40%, transparent); border-radius: var(--r5); background: var(--info-bg); color: var(--info); cursor: pointer; }
  .save:disabled { cursor: progress; }
  .edit-error { padding: 2px 12px 6px; font-size: var(--fs-meta); }
  /* Narrow window: the day is already shown in the header's date picker. */
  @media (max-width: 1250px) {
    .chip .day { display: none; }
  }
</style>
