<script lang="ts">
  import { t as tr, tc } from '../i18n.svelte'
  import { clock } from '../clock.svelte'
  import { locale } from '../locale'
  import { armer } from '../armed.svelte'
  import BlockControls from './BlockControls.svelte'
  import Icon from './Icon.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import { SORTS, STATUS_LABEL, addNote, allRepos, allTags, isDue, notes, parseStatus, removeNote, reschedule, setDone, setRepo, setStatus, splitTags, storage, tagsOf, updateNote, type Note, type NoteKind, type NoteSort, type NoteStatus } from '../notes.svelte'
  import { ago } from '../time'
  import WhenInput from './WhenInput.svelte'
  import { linkify } from '../linkify'
  import { filter, selectedDay, setQuery } from '../filter.svelte'
  import type { Agent, Snapshot } from '../types'
  import { shortcutLabel } from '../shortcut'
  import { quickNotesKey } from '../quick-notes.svelte'
  import type { IconName } from '../icons'
  import { anchored, dismissable, exclusive } from '../popup'
  import { hasCore, isDesktop } from '../bridge'
  import { nameOf } from '../agent-cli'
  import { agentsView } from '../agents.svelte'
  import { isReady as ready, sendNote } from '../note-send'
  import { agentRepo } from '../repositories'

  /**
   * Notes in three kinds (notes.svelte.ts): reminders to do first (overdue ones in red),
   * then the notes of the day selected in the header (today by default), then the
   * sticky notes. The input's kind is remembered under `notes.kind`, done reminders are
   * collapsed (`notes.showDone`). The snapshot resolves references (`ABC-…`, `!…`).
   * Tags (`#infra` in the text): a click on one filters the notes (`notes.tag`), and they can
   * order the notes (`notes.sort`) or group them in sections (`notes.group`). A note may belong to
   * a repository: its badge filters on it (`notes.repo`, which the Repos menu sets too), they can
   * be grouped by it, and its agents come first in its ➤.
   * `floating`: quick input (QuickNotes), outside the grid — no block controls and no
   * collapsing; each change of `opened` puts focus back in the input. `phoneTyping`: on a phone,
   * whether it may type to an agent (`[remote] typing`), which sending a note does.
   */
  let { snap, floating = false, opened = 0, phoneTyping = false }: { snap: Snapshot | null; floating?: boolean; opened?: number; phoneTyping?: boolean } = $props()
  let draftEl = $state<HTMLTextAreaElement>()
  $effect(() => {
    if (!floating || !draftEl) return
    void opened
    draftEl.focus()
  })

  let draft = $state('')
  let kind = $state<NoteKind>(pref<NoteKind>('notes.kind', 'sticky'))
  function setKind(k: NoteKind) {
    kind = k
    setPref('notes.kind', k)
  }
  /** Due time of the reminder being entered; none: in 1 h. */
  let whenAt = $state<Date | null>(null)
  let editing = $state<string | null>(null)
  let editText = $state('')
  let search = $state('')
  /** The search field, opened from the footer (or kept open while it holds a query). */
  let searching = $state(false)
  /** The tag the notes are filtered on (`#` before it in the text), empty: all. */
  let tagFilter = $state(pref<string>('notes.tag', ''))
  function setTag(t: string) {
    tagFilter = tagFilter === t ? '' : t
    setPref('notes.tag', tagFilter)
  }
  /** The repository the notes are filtered on (`notes.repo`, set by the Repos menu too), empty: all. */
  const repoFilter = $derived(pref<string>('notes.repo', ''))
  const setRepoFilter = (r: string) => setPref('notes.repo', repoFilter === r ? '' : r)
  /** Notes ordered by status (in progress first), name, date or tag; grouped by kind, tag or repository. */
  // svelte-ignore state_referenced_locally
  let sortBy = $state<NoteSort>(((s: string) => (s in SORTS ? (s as NoteSort) : 'status'))(pref<string>('notes.sort', 'status')))
  type GroupBy = 'kind' | 'tag' | 'repo'
  let groupBy = $state(pref<GroupBy>('notes.group', 'kind'))
  function setSort(s: NoteSort) {
    sortBy = s
    setPref('notes.sort', sortBy)
  }
  function setGroup(g: GroupBy) {
    groupBy = g
    setPref('notes.group', groupBy)
  }
  let showDone = $state(pref<boolean>('notes.showDone', false))
  function toggleDone() {
    showDone = !showDone
    setPref('notes.showDone', showDone)
  }

  const now = $derived(clock(30_000).now)

  const day = $derived(selectedDay())
  const isToday = $derived(!filter.date)
  const dayLabel = $derived(isToday ? tr('Today') : new Date(`${day}T12:00`).toLocaleDateString(locale(), { weekday: 'short', day: 'numeric', month: 'short' }))

  const q = $derived(search.trim().toLowerCase())
  const tags = $derived(allTags())
  /** A filter on a tag no note carries any more is dropped. */
  const activeTag = $derived(tags.some((t) => t.tag === tagFilter) ? tagFilter : '')
  /** The configured repositories, and those notes are about (one since removed from the config included). */
  const repoNames = $derived((snap?.repositories ?? []).map((r) => r.name))
  const repos = $derived(allRepos())
  const activeRepo = $derived(repos.some((r) => r.repo === repoFilter) || repoNames.includes(repoFilter) ? repoFilter : '')
  /** The repository of the note being written: the filtered one, unless picked otherwise. */
  let draftRepo = $derived(activeRepo)
  const matches = (n: Note) => (!q || n.text.toLowerCase().includes(q)) && (!activeTag || tagsOf(n.text).includes(activeTag)) && (!activeRepo || n.repo === activeRepo)
  const order = $derived(SORTS[sortBy].compare)
  const reminders = $derived(notes.filter((n) => n.kind === 'reminder' && !n.done_at && matches(n)).sort((a, b) => (a.remind_at ?? '').localeCompare(b.remind_at ?? '')))
  /** Collapsed together: done reminders, notes moved to DONE (those of the selected day, and the sticky ones). */
  const isDoneNote = (n: Note) => n.status === 'done' && (n.kind === 'sticky' || (n.kind === 'day' && n.day === day))
  const doneAt = (n: Note) => (n.kind === 'reminder' ? n.done_at : n.status_at) ?? ''
  const done = $derived(notes.filter((n) => ((n.kind === 'reminder' && !!n.done_at) || isDoneNote(n)) && matches(n)).sort((a, b) => doneAt(b).localeCompare(doneAt(a))))
  const ofDay = $derived(notes.filter((n) => n.kind === 'day' && n.day === day && n.status !== 'done' && matches(n)).sort(order))
  const sticky = $derived(notes.filter((n) => n.kind === 'sticky' && n.status !== 'done' && matches(n)).sort(order))
  /** Grouped by tag: the day's and the sticky notes under each of their tags, untagged last. */
  const byTagGroups = $derived.by(() => {
    const open = [...ofDay, ...sticky].sort(order === SORTS.tag.compare ? SORTS.status.compare : order)
    const groups = allTags(open).map(({ tag }) => ({ tag, notes: open.filter((n) => tagsOf(n.text).includes(tag)) }))
    const untagged = open.filter((n) => !tagsOf(n.text).length)
    return untagged.length ? [...groups, { tag: '', notes: untagged }] : groups
  })
  /** Grouped by repository: the day's and the sticky notes under theirs, by name, those of none last. */
  const byRepoGroups = $derived.by(() => {
    const open = [...ofDay, ...sticky].sort(order)
    const names = [...new Set(open.map((n) => n.repo).filter((r): r is string => !!r))].sort((a, b) => a.localeCompare(b))
    const groups = names.map((repo) => ({ repo, notes: open.filter((n) => n.repo === repo) }))
    const none = open.filter((n) => !n.repo)
    return none.length ? [...groups, { repo: '', notes: none }] : groups
  })
  const doing = $derived([...ofDay, ...sticky].filter((n) => n.status === 'doing').length)
  const due = $derived(reminders.filter((n) => isDue(n, now)).length)

  const short = $derived(!floating && isShort('notes'))


  function submit() {
    const text = draft.trim()
    if (!text) return
    if (kind === 'reminder') {
      const at = whenAt ?? new Date(Date.now() + 60 * 60_000)
      addNote(activeTag && !tagsOf(text).includes(activeTag) ? `${text} #${activeTag}` : text, 'reminder', { remind_at: at.toISOString(), repo: draftRepo || null })
      whenAt = null
    } else {
      const parsed = parseStatus(text)
      // Written while a tag is filtered: it carries the tag, and stays in sight.
      if (activeTag && !tagsOf(parsed.text).includes(activeTag)) parsed.text += ` #${activeTag}`
      addNote(parsed.text, kind, { ...(kind === 'day' ? { day } : {}), status: parsed.status, repo: draftRepo || null })
    }
    draft = ''
  }

  /** Enter submits, Shift+Enter inserts a line break. */
  function onDraftKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      submit()
    }
  }

  function startEdit(id: string, text: string) {
    editing = id
    editText = text
  }
  function commitEdit() {
    if (!editing) return
    const text = editText.trim()
    if (text) updateNote(editing, text)
    editing = null
  }
  function onEditKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      commitEdit()
    } else if (e.key === 'Escape') {
      editing = null
    }
  }

  /**
   * Two-click deletion: the first arms the button, the second deletes. No
   * `confirm()`: Tauri's webview does not show it and always answers no.
   */
  const armed = armer(4000)
  function remove(id: string) {
    if (armed.confirm(id)) removeNote(id)
  }

  /** "in 25 min", "in 3 h", "Tue 9:00"; overdue: "12 min late". */
  function until(iso: string) {
    const ms = new Date(iso).getTime() - now
    const at = new Date(iso)
    if (ms <= 0) {
      const late = Math.round(-ms / 60_000)
      return late < 1 ? tr('now') : late < 60 ? tr('{n} min late', { n: late }) : late < 1440 ? tr('{n} h late', { n: Math.round(late / 60) }) : tr('{n} d late', { n: Math.round(late / 1440) })
    }
    const min = Math.round(ms / 60_000)
    if (min < 60) return tr('in {n} min', { n: min })
    const time = at.toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
    if (at.toDateString() === new Date(now).toDateString()) return tr('today {time}', { time })
    return `${at.toLocaleDateString(locale(), { weekday: 'short', day: at.getTime() - now > 6 * 86_400_000 ? 'numeric' : undefined, month: at.getTime() - now > 6 * 86_400_000 ? 'short' : undefined })} ${time}`
  }

  /** The menu open on a note, one at a time: its status, its repository or the agent to send it to; and its button. */
  let menu = $state<{ kind: 'status' | 'repo' | 'send'; id: string; anchor: HTMLElement } | null>(null)
  function openMenu(kind: 'status' | 'repo' | 'send', id: string, anchor: HTMLElement) {
    menu = menu?.kind === kind && menu.id === id ? null : { kind, id, anchor }
  }
  const closeMenu = () => (menu = null)
  const picking = $derived(menu?.kind === 'status' ? menu.id : null)
  const repoing = $derived(menu?.kind === 'repo' ? menu.id : null)
  const sending = $derived(menu?.kind === 'send' ? menu.id : null)
  const STATUS_OPTIONS: { id: NoteStatus | null; label: string }[] = [
    { id: 'todo', label: tr('To do') },
    { id: 'doing', label: tr('Doing') },
    { id: 'done', label: tc('tickets', 'Done') },
    { id: null, label: tr('A note, no status') },
  ]
  function pick(status: NoteStatus | null) {
    if (picking) setStatus(picking, status)
    closeMenu()
  }
  function pickRepo(repo: string | null) {
    if (repoing) setRepo(repoing, repo)
    closeMenu()
  }

  let snoozing = $state<string | null>(null)
  /** The new due time picked in the editor, starting from the current one. */
  let snoozeAt = $state<Date | null>(null)
  function openSnooze(n: Note) {
    snoozeAt = n.remind_at ? new Date(n.remind_at) : null
    snoozing = n.id
  }
  function snooze(id: string, at: Date) {
    reschedule(id, at)
    snoozing = null
  }

  const focus = (el: HTMLTextAreaElement) => {
    el.focus()
    el.setSelectionRange(el.value.length, el.value.length)
  }
  /** The input grows with its lines (up to a few), and shrinks back once saved. */
  function autogrow(el: HTMLTextAreaElement) {
    $effect(() => {
      void draft
      el.style.height = 'auto'
      el.style.height = `${Math.min(el.scrollHeight, 120)}px`
    })
  }
  const KINDS: { id: NoteKind; icon: IconName; label: () => string; hint: string }[] = [
    { id: 'sticky', icon: 'pin', label: () => tr('Sticky'), hint: tr('always shown, whatever the day') },
    { id: 'day', icon: 'calendar', label: () => dayLabel, hint: tr('only shown when this day is selected in the header') },
    { id: 'reminder', icon: 'alarm', label: () => tr('Reminder'), hint: tr('a system notification at the time you pick') },
  ]

  /**
   * A note sent as a prompt to a running agent (`note-send.ts`), picked from its send button: those
   * working in its repository first, then the one selected in the Agents view, then those waiting.
   */
  const phone = $derived(hasCore() && !isDesktop())
  const targets = $derived.by(() => {
    if (!hasCore()) return []
    return (snap?.agents ?? []).filter((a) => a.state === 'running').map((a) => ({ a, repo: agentRepo(a, snap) }))
  })
  const targetsFor = (n: Note | undefined) => {
    const rank = (t: { a: Agent; repo: string | null }) => (n?.repo && t.repo === n.repo ? 0 : 4) + (t.a.id === agentsView.selected ? 0 : ready(t.a) ? 1 : 2)
    return [...targets].sort((x, y) => rank(x) - rank(y))
  }
  /** Agents in more than one repository: each says its own. */
  const multiRepo = $derived(new Set(targets.map((t) => t.repo)).size > 1)
  /** From a phone, its `[remote] typing` (RemoteApp says). */
  const typing = $derived(!phone || phoneTyping)
  function sendTo(n: Note, a: Agent) {
    closeMenu()
    void sendNote(n, a)
  }
</script>

{#snippet text(n: Note)}
  {#if editing === n.id}
    <textarea class="edit" bind:value={editText} onkeydown={onEditKey} onblur={commitEdit} use:focus rows="3"></textarea>
  {:else}
    <!-- The text can be selected with the mouse; a double-click (outside a link) edits it, Enter too from the keyboard. -->
    <div
      class="text selectable"
      role="button"
      tabindex="0"
      title={tr('Double-click to edit')}
      ondblclick={(e) => !(e.target as HTMLElement).closest('a, button') && (window.getSelection()?.removeAllRanges(), startEdit(n.id, n.text))}
      onkeydown={(e) => e.key === 'Enter' && e.target === e.currentTarget && startEdit(n.id, n.text)}
    >
      {#each linkify(n.text, snap) as seg}{#if 'href' in seg && seg.query}{@const q = seg.query}<button class="ref" title="{seg.title ?? q} · {tr('click to filter')}" onclick={() => setQuery(q)}>{seg.text}</button><a class="open" href={seg.href} title={tr('Open {name}', { name: q })} aria-label={tr('Open {name}', { name: q })}><Icon name="external" /></a>{:else if 'href' in seg}<a href={seg.href} title={seg.title ?? seg.href}>{seg.text}</a>{:else}{#each splitTags(seg.text) as part}{#if 'tag' in part}{@const t = part.tag}<button class="tagref" class:on={activeTag === t} onclick={() => setTag(t)} title={activeTag === t ? tr('Show all the notes') : tr('Only the notes tagged #{tag}', { tag: t })}>{part.text}</button>{:else}{part.text}{/if}{/each}{/if}{/each}
    </div>
  {/if}
{/snippet}

{#snippet send(n: Note)}
  {#if targets.length}
    <button class="x go" class:open={sending === n.id} onclick={(e) => openMenu('send', n.id, e.currentTarget)} title={n.status === 'todo' ? tr('Send to an agent · the note goes DOING') : tr('Send to an agent')} aria-label={tr('Send the note to an agent')} aria-haspopup="menu" aria-expanded={sending === n.id}><Icon name="send" /></button>
  {/if}
{/snippet}

{#snippet edit(n: Note)}
  <!-- A phone has no double-click to speak of: its pencil edits. -->
  <button class="x pen" onclick={() => startEdit(n.id, n.text)} aria-label={tr('Edit the note')} title={tr('Edit')}><Icon name="edit" /></button>
{/snippet}

{#snippet repoBadge(n: Note)}
  {#if n.repo}{@const r = n.repo}<button class="reporef" class:on={activeRepo === r} onclick={() => setRepoFilter(r)} title={activeRepo === r ? tr('Show the notes of every repository') : tr('Only the notes of {repo}', { repo: r })}><Icon name="branch" />{r}</button>{/if}
{/snippet}

{#snippet repoBtn(n: Note)}
  {#if repoNames.length}
    <button class="x rp" class:open={repoing === n.id} onclick={(e) => openMenu('repo', n.id, e.currentTarget)} title={tr('Repository of the note')} aria-label={tr('Repository of the note')} aria-haspopup="menu" aria-expanded={repoing === n.id}><Icon name="branch" /></button>
  {/if}
{/snippet}

{#snippet del(n: Note)}
  <button class="x" class:armed={armed.is(n.id)} onclick={() => remove(n.id)} onblur={() => armed.is(n.id) && armed.disarm()} aria-label={armed.is(n.id) ? tr('Click again to delete the note') : tr('Delete note')} title={armed.is(n.id) ? tr('Click again to delete') : tr('Delete')}>{#if armed.is(n.id)}{tr('Delete?')}{:else}<Icon name="close" />{/if}</button>
{/snippet}

{#snippet reminder(n: Note)}
  {@const late = isDue(n, now)}
  <div class="note rem" class:late class:done={!!n.done_at}>
    <button class="box bell" class:checked={!!n.done_at} onclick={() => setDone(n.id, !n.done_at)} title={n.done_at ? tr('Reopen') : tr('Mark as done')} aria-label={n.done_at ? tr('Reopen') : tr('Mark as done')}>{#if n.done_at}<Icon name="check" />{/if}</button>
    {@render text(n)}
    {#if editing !== n.id}
      <span class="meta">
        {@render repoBadge(n)}
        {#if n.done_at}<span class="muted mono" title={tr('Done {time}', { time: new Date(n.done_at).toLocaleString(locale()) })}>{tr('done {ago}', { ago: ago(n.done_at) })}</span>
        {:else if n.remind_at}<button class="when" onclick={() => openSnooze(n)} title="{new Date(n.remind_at).toLocaleString(locale())} · {tr('click to change')}">{until(n.remind_at)}</button>{/if}
        {#if !n.done_at}
          <span class="snz">
            <button class="later" onclick={() => (snoozing === n.id ? (snoozing = null) : openSnooze(n))} title={tr('Change the time')} aria-expanded={snoozing === n.id}>{tr('Later')}</button>
            {#if snoozing === n.id}
              <!-- svelte-ignore a11y_no_static_element_interactions : Escape closes it; the field has its own keys -->
              <div class="pop edit-when" onkeydown={(e) => e.key === 'Escape' && (snoozing = null)}>
                <button class="soon" onclick={() => snooze(n.id, new Date(Date.now() + 10 * 60_000))}>{tr('in 10 min')}</button>
                <WhenInput bind:at={snoozeAt} {now} autofocus onenter={(at) => snooze(n.id, at)} />
                <div class="acts">
                  <button onclick={() => (snoozing = null)}>{tr('Cancel')}</button>
                  <button class="ok" disabled={!snoozeAt} onclick={() => snoozeAt && snooze(n.id, snoozeAt)}>{tr('Set')}</button>
                </div>
              </div>
            {/if}
          </span>
        {/if}
        <span class="acts">{@render edit(n)}{@render repoBtn(n)}{@render send(n)}{@render del(n)}</span>
      </span>
    {/if}
  </div>
{/snippet}

{#snippet plain(n: Note)}
  <div class="note st-{n.status ?? 'none'}">
    <button class="box {n.status ?? 'none'}" class:open={picking === n.id} onclick={(e) => openMenu('status', n.id, e.currentTarget)} title="{n.status ? STATUS_LABEL[n.status] : tr('A note')} · {tr('click to change')}" aria-label={tr('Status: {status}, change', { status: n.status ? STATUS_LABEL[n.status] : tr('none') })} aria-haspopup="menu" aria-expanded={picking === n.id}>{#if n.status === 'done'}<Icon name="check" />{/if}</button>
    {@render text(n)}
    {#if editing !== n.id}
      <span class="meta">
        {@render repoBadge(n)}
        {#if n.status === 'done' && n.status_at}<span class="mono muted" title={tr('Done {time}', { time: new Date(n.status_at).toLocaleString(locale()) })}>{tr('done {ago}', { ago: ago(n.status_at) })}</span>
        {:else}<span class="mono muted" title={new Date(n.updated_at).toLocaleString(locale())}>{ago(n.updated_at)}</span>{/if}
        <span class="acts">{@render edit(n)}{@render repoBtn(n)}{@render send(n)}{@render del(n)}</span>
      </span>
    {/if}
  </div>
{/snippet}

{#if sending}
  {@const n = notes.find((x) => x.id === sending)}
  <!-- Fixed position, over the block: its scroll would clip it. -->
  <div class="pick agents" role="menu" tabindex="-1" {@attach anchored(() => menu?.anchor)} {@attach exclusive(closeMenu)} {@attach dismissable(closeMenu, { trigger: () => menu?.anchor })}>
    <p class="head">{tr('Send to')}</p>
    {#if !typing}<p class="off">{tr('Typing to an agent from a phone is off (Settings › Remote › Exposure)')}</p>{/if}
    {#each targetsFor(n) as { a, repo } (a.id)}
      <button role="menuitem" disabled={!n || !typing} onclick={() => n && sendTo(n, a)} title={phone || ready(a) ? tr('{name}: sent at once', { name: a.title || nameOf(a) }) : tr('{name}: sent at once, read once its turn is over', { name: a.title || nameOf(a) })}>
        <span class="who"><b>{a.title || nameOf(a)}</b><small>{[multiRepo || (n?.repo && repo !== n.repo) ? repo : null, a.ticket, a.branch ?? a.cwd.split('/').pop()].filter(Boolean).join(' · ')}</small></span>
        <span class="st" class:busy={!ready(a)}>{a.id === agentsView.selected && !phone ? `${tr('selected')} · ` : ''}{ready(a) ? tc('tickets', 'waits') : tc('tickets', 'working')}</span>
      </button>
    {/each}
  </div>
{/if}

{#if repoing}
  {@const current = notes.find((x) => x.id === repoing)?.repo ?? null}
  <!-- Fixed position, over the block: its scroll would clip it. -->
  <div class="pick" role="menu" tabindex="-1" {@attach anchored(() => menu?.anchor)} {@attach exclusive(closeMenu)} {@attach dismissable(closeMenu, { trigger: () => menu?.anchor })}>
    {#each [...repoNames, ...(current && !repoNames.includes(current) ? [current] : [])] as r (r)}
      <button role="menuitemradio" aria-checked={current === r} class:on={current === r} onclick={() => pickRepo(r)}><Icon name="branch" />{r}</button>
    {/each}
    <button role="menuitemradio" aria-checked={current === null} class:on={current === null} onclick={() => pickRepo(null)}><Icon name="close" />{tc('tickets', 'No repository')}</button>
  </div>
{/if}

{#if picking}
  {@const current = notes.find((x) => x.id === picking)?.status ?? null}
  <!-- Fixed position, over the block: its scroll would clip it. -->
  <div class="pick" role="menu" tabindex="-1" {@attach anchored(() => menu?.anchor, { align: 'start' })} {@attach exclusive(closeMenu)} {@attach dismissable(closeMenu, { trigger: () => menu?.anchor })}>
    {#each STATUS_OPTIONS as o (o.id ?? 'none')}
      <button role="menuitemradio" aria-checked={current === o.id} class:on={current === o.id} onclick={() => pick(o.id)}><span class="box {o.id ?? 'none'}">{#if o.id === 'done'}<Icon name="check" />{/if}</span>{o.label}</button>
    {/each}
  </div>
{/if}

<section class="panel" style="--src: var(--accent)" class:collapsed={!floating && isCollapsed('notes')} class:floating>
  <h2>
    <span class="title">{tr('Notes')}</span>
    <small>{sticky.length + ofDay.length}{reminders.length ? ` · ${reminders.length} reminder${reminders.length > 1 ? 's' : ''}` : ''}</small>
    {#if activeRepo}<button class="reporef on" onclick={() => setRepoFilter(activeRepo)} title={tr('Filtered on this repository: show the notes of every one')}><Icon name="branch" />{activeRepo} <Icon name="close" /></button>{/if}
    {#if activeTag}<button class="tagref on" onclick={() => setTag(activeTag)} title={tr('Filtered on this tag: show all the notes')}>#{activeTag} <Icon name="close" /></button>{/if}
    {#if doing}<span class="doing" title={tr('Notes in progress')}>{tr('{n} doing', { n: doing })}</span>{/if}
    {#if due}<span class="due" title={tr('Reminders past their time')}>{tr('{n} due', { n: due })}</span>{/if}
    {#if floating}<span class="close-hint"><kbd>Esc</kbd> {tr('to close')}</span>{:else}<BlockControls id="notes" />{/if}
  </h2>
  <div class="compose">
    <div class="line">
      <div class="kinds" role="radiogroup" aria-label={tr('Kind of note')}>
        {#each KINDS as k (k.id)}
          <button role="radio" aria-checked={kind === k.id} aria-label={k.label()} class:on={kind === k.id} class="k-{k.id}" onclick={() => setKind(k.id)} title="{k.label()}: {k.hint}"><Icon name={k.icon} /></button>
        {/each}
      </div>
      {#if repoNames.length}
        <label class="draft-repo" class:on={!!draftRepo} title={draftRepo ? tr('A note of {repo}', { repo: draftRepo }) : tr('A note of no repository: pick one to file it there')}><Icon name="branch" />
          <select bind:value={draftRepo} aria-label={tr('Repository of the note')}>
            <option value="">—</option>
            {#each repoNames as r (r)}<option value={r}>{r}</option>{/each}
          </select>
        </label>
      {/if}
      <textarea bind:this={draftEl} bind:value={draft} onkeydown={onDraftKey} use:autogrow rows="1" placeholder={kind === 'reminder' ? tr('Remind me to…') : kind === 'day' ? (isToday ? tr('A note for today… (todo / doing first: a task)') : tr('A note for {day}… (todo / doing first: a task)', { day: dayLabel })) : tr('Add a note… (todo / doing first: a task)')}></textarea>
      {#if !floating}<kbd title={tr('A note from anywhere')}>{shortcutLabel(quickNotesKey())}</kbd>{/if}
    </div>
    {#if kind === 'reminder' && (draft.trim() || whenAt)}
      <div class="when-pick"><WhenInput bind:at={whenAt} {now} onenter={() => submit()} empty={tr('Default: in 1 h')} /></div>
    {/if}
    {#if storage.failed}<div class="warn">{tr('Notes couldn’t be saved to disk.')}</div>{/if}
  </div>
  {#if (tags.length || repos.length || ofDay.length + sticky.length > 1) && !short}
    <div class="tags" role="toolbar" aria-label={tr('Tags and order')}>
      <span class="chips">
        {#each repos as r (r.repo)}
          <button class="chip repo" class:on={activeRepo === r.repo} onclick={() => setRepoFilter(r.repo)} aria-pressed={activeRepo === r.repo} title={activeRepo === r.repo ? tr('Show the notes of every repository') : tr('Only the notes of {repo}', { repo: r.repo })}><Icon name="branch" />{r.repo}<b>{r.count}</b></button>
        {/each}
        {#each tags as t (t.tag)}
          <button class="chip" class:on={activeTag === t.tag} onclick={() => setTag(t.tag)} aria-pressed={activeTag === t.tag} title={activeTag === t.tag ? tr('Show all the notes') : tr('Only the notes tagged #{tag}', { tag: t.tag })}>#{t.tag}<b>{t.count}</b></button>
        {/each}
      </span>
      <label class="sort" class:on={sortBy !== 'status'} title={tr('Order of the notes: {order}', { order: SORTS[sortBy].title })}><Icon name="sortAz" />
        <select value={sortBy} onchange={(e) => setSort(e.currentTarget.value as NoteSort)} aria-label={tr('Sort the notes by')}>
          {#each Object.entries(SORTS) as [key, s] (key)}<option value={key}>{s.label}</option>{/each}
        </select>
      </label>
      {#if tags.length || repos.length}
        <label class="sort" class:on={groupBy !== 'kind'} title={tr('Sections: sticky and day notes, one per tag, or one per repository')}><Icon name="group" />
          <select value={groupBy} onchange={(e) => setGroup(e.currentTarget.value as GroupBy)} aria-label={tr('Group the notes by')}>
            <option value="kind">{tr('Kind')}</option>
            {#if tags.length || groupBy === 'tag'}<option value="tag">{tr('Tag')}</option>{/if}
            {#if repos.length || groupBy === 'repo'}<option value="repo">{tr('Repository')}</option>{/if}
          </select>
        </label>
      {/if}
    </div>
  {/if}
  <div class="scroll">
    {#if short}
      {#each reminders.slice(0, 2) as n (n.id)}{@render reminder(n)}{/each}
      {#each [...ofDay, ...sticky].slice(0, reminders.length ? 1 : 2) as n (n.id)}{@render plain(n)}{/each}
    {:else}
      {#if reminders.length}
        <p class="sec rem-sec"><span class="dot"></span>{tr('Reminders')} <b>{reminders.length}</b></p>
        {#each reminders as n (n.id)}{@render reminder(n)}{/each}
      {/if}
      {#if groupBy === 'tag' && tags.length}
        {#each byTagGroups as g (g.tag)}
          <p class="sec tag-sec"><span class="dot"></span>{g.tag ? `#${g.tag}` : tr('No tag')} <b>{g.notes.length}</b></p>
          {#each g.notes as n (n.id)}{@render plain(n)}{/each}
        {/each}
      {:else if groupBy === 'repo' && repos.length}
        {#each byRepoGroups as g (g.repo)}
          <p class="sec repo-sec"><span class="dot"></span>{g.repo || tc('tickets', 'No repository')} <b>{g.notes.length}</b></p>
          {#each g.notes as n (n.id)}{@render plain(n)}{/each}
        {/each}
      {:else}
      {#if ofDay.length || kind === 'day'}
        <p class="sec day-sec"><span class="dot"></span>{isToday ? `${tr('Today')} · ${new Date().toLocaleDateString(locale(), { weekday: 'long', day: 'numeric', month: 'long' })}` : dayLabel} <b>{ofDay.length}</b></p>
        {#each ofDay as n (n.id)}{@render plain(n)}{:else}<div class="empty">{tr('No note for this day.')}</div>{/each}
      {/if}
      {#if sticky.length}
        {#if reminders.length || ofDay.length || kind === 'day'}<p class="sec"><span class="dot"></span>{tr('Sticky')} <b>{sticky.length}</b></p>{/if}
        {#each sticky as n (n.id)}{@render plain(n)}{/each}
      {/if}
      {/if}
      {#if done.length && showDone}
        <p class="sec done-sec"><span class="dot"></span>{tc('tickets', 'Done')} <b>{done.length}</b></p>
        {#each done as n (n.id)}{#if n.kind === 'reminder'}{@render reminder(n)}{:else}{@render plain(n)}{/if}{/each}
      {/if}
      {#if !reminders.length && !ofDay.length && !sticky.length && kind !== 'day'}
        <div class="empty">{search || activeTag || activeRepo ? tr('No note matches.') : tr('No note yet.')}</div>
      {/if}
    {/if}
  </div>
  {#if !short && (done.length || notes.length > 5)}
    <div class="foot">
      {#if done.length}<button class="fold" onclick={toggleDone} aria-expanded={showDone}><span class="caret" class:open={showDone}><Icon name="chevronRight" /></span>{tc('tickets', 'Done')} <b>{done.length}</b></button>{/if}
      <span class="grow"></span>
      {#if notes.length > 5}
        {#if searching || search}
          <!-- svelte-ignore a11y_autofocus : opened by a click on Search -->
          <input class="search" bind:value={search} placeholder={tr('Search notes…')} spellcheck="false" autofocus onblur={() => !search && (searching = false)} onkeydown={(e) => e.key === 'Escape' && ((search = ''), (searching = false))} />
        {:else}
          <button class="find" onclick={() => (searching = true)}><Icon name="search" /> {tr('Search')}</button>
        {/if}
      {/if}
    </div>
  {/if}
</section>

<style>
  .close-hint { margin-left: auto; font-size: var(--fs-tag); font-weight: 400; color: var(--text-muted); }
  .close-hint kbd { font: 600 10px var(--mono); }
  /* Composer: one line, the kind as three icons inside it. */
  .compose { display: flex; flex-direction: column; gap: 5px; padding: 8px 10px 6px; }
  .line { display: flex; align-items: flex-start; gap: 6px; padding: 3px 5px 3px 3px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--bg); }
  .line:focus-within { border-color: var(--accent); }
  .line textarea { flex: 1; min-width: 0; margin: 0; padding: 2px 0; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-body); line-height: 18px; resize: none; overflow: auto; }
  .line textarea:focus { outline: none; }
  .line kbd { align-self: center; padding: 0 4px; border: 1px solid var(--border); border-radius: var(--r3); font: 10px var(--mono); color: var(--text-muted); white-space: nowrap; }
  .edit { flex: 1; font: inherit; font-size: var(--fs-body); padding: 4px 6px; border: 1px solid var(--accent); border-radius: var(--r6); background: var(--bg); color: var(--text); resize: vertical; }
  .edit:focus { outline: none; }
  .warn { font-size: var(--fs-meta); color: var(--warn); }
  .doing { padding: 0 6px; border-radius: var(--r9); background: var(--info-bg); color: var(--info); font-size: var(--fs-tag); font-weight: 500; white-space: nowrap; }
  .due { padding: 0 6px; border-radius: var(--r9); background: var(--ko-bg); color: var(--ko); font-size: var(--fs-tag); font-weight: 500; white-space: nowrap; }
  /* Note kind: sticky (accent), daily (calendar), reminder (alert). */
  .kinds { display: inline-flex; flex-shrink: 0; gap: 1px; padding: 1px; border-radius: var(--r6); background: var(--surface-muted); }
  .kinds button { display: grid; place-items: center; width: 22px; height: 20px; padding: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font-size: 12px; cursor: pointer; }
  .kinds button:hover { color: var(--text); }
  .kinds button.on { background: var(--surface); color: var(--kc); box-shadow: inset 0 -2px var(--kc); }
  .k-sticky { --kc: var(--accent); }
  .k-day { --kc: var(--src-calendar); }
  .k-reminder { --kc: var(--src-alert); }
  .when-pick { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; font-size: var(--fs-tag); }
  /* Sections: a coloured dot per kind, light. */
  .sec { display: flex; align-items: center; gap: 6px; margin: 0; padding: 9px 12px 3px; font-size: var(--fs-tag); font-weight: 600; color: var(--text-muted); --dc: var(--accent); }
  .sec .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--dc); }
  .sec b { color: var(--text); font-weight: 600; }
  .rem-sec { --dc: var(--src-alert); }
  .day-sec { --dc: var(--src-calendar); }
  .done-sec { --dc: var(--ok); }
  .tag-sec { --dc: var(--info); }
  .repo-sec { --dc: var(--src-gitlab); }
  /* Repository: a badge on the note, a chip in the bar, a picker in the composer. */
  .reporef { display: inline-flex; align-items: center; gap: 3px; padding: 0 5px; border: none; border-radius: var(--r4); background: var(--surface-muted); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); white-space: nowrap; cursor: pointer; }
  .reporef:hover, .reporef.on { color: var(--text); background: color-mix(in srgb, var(--accent) 16%, transparent); }
  h2 .reporef { font-weight: 500; }
  .chip.repo { gap: 3px; }
  .draft-repo { flex-shrink: 0; display: inline-flex; align-items: center; gap: 2px; max-width: 110px; padding: 0 2px 0 5px; border-radius: var(--r5); color: var(--text-muted); font-size: var(--fs-tag); line-height: 20px; cursor: pointer; }
  .draft-repo:hover { color: var(--text); background: var(--surface-muted); }
  .draft-repo.on { color: var(--text); background: color-mix(in srgb, var(--accent) 14%, transparent); }
  .draft-repo select { min-width: 0; padding: 0; border: none; background: none; color: inherit; font: inherit; text-overflow: ellipsis; cursor: pointer; outline: none; }
  /* Tags: a chip in the text filters on it; the bar lists them, then sort and group. */
  .tagref { display: inline; padding: 0 3px; border: none; border-radius: var(--r4); background: color-mix(in srgb, var(--info) 12%, transparent); color: var(--info); font: inherit; font-size: 0.95em; cursor: pointer; }
  .tagref:hover, .tagref.on { background: color-mix(in srgb, var(--info) 24%, transparent); }
  h2 .tagref { display: inline-flex; align-items: center; gap: 3px; font-size: var(--fs-tag); font-weight: 500; }
  .tags { display: flex; align-items: center; gap: 4px; padding: 0 10px 5px; }
  .chips { flex: 1; min-width: 0; display: flex; gap: 4px; overflow-x: auto; scrollbar-width: none; }
  .chips::-webkit-scrollbar { display: none; }
  .chip { flex-shrink: 0; display: inline-flex; align-items: center; gap: 4px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r10); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; }
  .chip b { font-weight: 600; color: var(--text); }
  .chip:hover { color: var(--text); border-color: var(--info); }
  .chip.on { border-color: var(--info); background: color-mix(in srgb, var(--info) 14%, transparent); color: var(--info); }
  .sort { flex-shrink: 0; display: inline-flex; align-items: center; gap: 2px; padding: 0 2px 0 5px; border-radius: var(--r5); color: var(--text-muted); font-size: var(--fs-tag); cursor: pointer; }
  .sort:hover { color: var(--text); background: var(--surface-muted); }
  .sort.on { color: var(--info); background: color-mix(in srgb, var(--info) 14%, transparent); }
  .sort select { padding: 0 2px; border: none; background: none; color: inherit; font: inherit; cursor: pointer; outline: none; }
  .opt { flex-shrink: 0; display: grid; place-items: center; width: 22px; height: 20px; padding: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .opt:hover { color: var(--text); background: var(--surface-muted); }
  .opt.on { color: var(--info); background: color-mix(in srgb, var(--info) 14%, transparent); }
  /* Footer: the Done fold and the search. */
  .foot { display: flex; align-items: center; gap: 8px; min-height: 28px; padding: 3px 10px 3px 8px; border-top: 1px solid var(--border); font-size: var(--fs-tag); color: var(--text-muted); }
  .fold, .find { display: inline-flex; align-items: center; gap: 4px; padding: 1px 4px; border: none; border-radius: var(--r4); background: none; color: inherit; font: inherit; cursor: pointer; }
  .fold:hover, .find:hover { color: var(--text); background: var(--surface-muted); }
  .fold b { color: var(--text); }
  .search { width: 55%; padding: 2px 6px; border: 1px solid var(--accent); border-radius: var(--r5); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  .search:focus { outline: none; }
  .caret { display: inline-flex; font-size: 10px; transition: transform 0.12s; }
  .caret.open { transform: rotate(90deg); }
  .note { display: flex; gap: 9px; align-items: flex-start; padding: 4px 12px; }
  .note:hover { background: var(--surface-muted); }
  .rem.late { background: color-mix(in srgb, var(--ko) 8%, var(--surface)); }
  .rem.done .text { color: var(--text-muted); text-decoration: line-through; }
  .when { padding: 0 3px; border: 1px dashed transparent; border-radius: var(--r4); background: none; white-space: nowrap; color: var(--text-muted); font: inherit; font-variant-numeric: tabular-nums; cursor: pointer; }
  .when:hover { border-color: var(--border); color: var(--text); }
  .rem.late .when { color: var(--ko); font-weight: 600; }
  .snz { position: relative; }
  .later { padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 16px; cursor: pointer; opacity: 0; }
  .note:hover .later, .later[aria-expanded='true'], .rem.late .later { opacity: 1; }
  .later:hover { color: var(--text); }
  .pop { position: absolute; right: 0; top: calc(100% + 3px); z-index: 20; display: flex; flex-direction: column; min-width: 120px; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r7); box-shadow: var(--shadow-pop); }
  .pop button { padding: 3px 8px; border: none; border-radius: var(--r4); background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; white-space: nowrap; cursor: pointer; }
  .pop button:hover { background: var(--surface-muted); }
  /* Changing a reminder's time: proposed times, the field, then Set. */
  .pop.edit-when { width: 270px; gap: 6px; padding: 8px; }
  .pop.edit-when .soon { align-self: flex-start; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r10); font-size: var(--fs-tag); line-height: 18px; color: var(--text-muted); }
  .pop.edit-when .acts { display: flex; justify-content: flex-end; gap: 4px; }
  .pop.edit-when .acts button { padding: 2px 10px; border: 1px solid var(--border); border-radius: var(--r5); text-align: center; }
  .pop.edit-when .acts .ok { border-color: var(--src-alert); color: var(--src-alert); font-weight: 600; }
  .pop.edit-when .acts .ok:disabled { opacity: 0.5; cursor: default; }
  .text { flex: 1; min-width: 0; color: var(--text); font-size: var(--fs-body); white-space: pre-wrap; overflow-wrap: anywhere; cursor: text; }
  .text:focus-visible { outline: 1px solid var(--accent); border-radius: var(--r3); }
  .text a { color: var(--accent); text-decoration: none; font-family: var(--mono); font-size: var(--fs-mono); }
  .text a:hover { text-decoration: underline; }
  /* Reference: a click filters the dashboard, the arrow opens the ticket or the MR. */
  .text .ref { display: inline; padding: 0; border: none; background: none; color: var(--accent); font-family: var(--mono); font-size: var(--fs-mono); cursor: pointer; }
  .text .ref:hover { text-decoration: underline; }
  .text a.open { margin-left: 1px; font-family: inherit; font-size: var(--fs-tag); opacity: 0.5; }
  .text a.open:hover { opacity: 1; text-decoration: none; }
  /* Status as a round box: empty TODO, half DOING, ticked DONE; a plain note shows a dot, and
     on hover the box that makes it a task. A reminder's box is square, in its colour. */
  .box { flex-shrink: 0; display: grid; place-items: center; width: 14px; height: 14px; margin-top: 2px; padding: 0; border: 1.5px solid var(--text-muted); border-radius: 50%; background: none; color: var(--surface); font-size: 9px; cursor: pointer; }
  .box.todo { border-color: var(--warn); }
  .box.doing { border-color: var(--info); background: linear-gradient(90deg, var(--info) 50%, transparent 50%); }
  .box.done { border-color: var(--ok); background: var(--ok); }
  .box.none { border-color: transparent; }
  .box.none::after { content: ''; width: 5px; height: 5px; border-radius: 50%; background: var(--text-muted); opacity: 0.6; }
  .note:hover .box.none, .box.none:focus-visible { border: 1.5px dashed var(--text-muted); }
  .note:hover .box.none::after { display: none; }
  .box.bell { border-radius: var(--r4); border-color: var(--src-alert); }
  .box.bell.checked { background: var(--src-alert); }
  .box:hover, .box.open { filter: brightness(1.15); }
  .box.open { box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 45%, transparent); }
  /* The status picker. */
  .pick { position: fixed; z-index: 40; display: flex; flex-direction: column; min-width: 160px; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); }
  .pick button { display: flex; align-items: center; gap: 8px; padding: 4px 8px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .pick button:hover { background: var(--surface-muted); }
  .pick button.on { font-weight: 600; }
  .pick .box { margin-top: 0; pointer-events: none; }
  .st-doing .text { color: var(--text); }
  .st-done .text { color: var(--text-muted); text-decoration: line-through; }
  .meta { display: flex; gap: 6px; align-items: center; flex-shrink: 0; margin-top: 1px; font-size: var(--fs-tag); }
  .x { border: none; background: none; color: var(--text-muted); cursor: pointer; font-size: var(--fs-meta); opacity: 0; padding: 0 2px; }
  .note:hover .x, .x:focus-visible { opacity: 1; }
  .x:hover { color: var(--ko); }
  .x.go:hover { color: var(--src-claude); }
  .x.go.open { opacity: 1; color: var(--src-claude); }
  .x.rp:hover, .x.rp.open { opacity: 1; color: var(--accent); }
  .acts { display: contents; }
  .x.pen { display: none; }
  /*
   * On the phone: a note's text over the full width, its time and actions on a line under it,
   * every one of them shown (no hover) and big enough for a thumb; a status box a thumb can hit;
   * the input at 16 px (below that, iOS zooms in on focus), without the computer's shortcut.
   */
  :global(html.remote) .note { flex-wrap: wrap; row-gap: 0; padding: 8px 12px; border-top: 1px solid color-mix(in srgb, var(--border) 60%, transparent); }
  :global(html.remote) .sec + .note { border-top: none; }
  :global(html.remote) .note:hover { background: none; }
  :global(html.remote) .rem.late, :global(html.remote) .rem.late:hover { background: color-mix(in srgb, var(--ko) 8%, var(--surface)); }
  :global(html.remote) .text { flex-basis: calc(100% - 30px); line-height: 1.4; }
  :global(html.remote) .box { position: relative; width: 20px; height: 20px; margin-top: 0; font-size: 12px; }
  :global(html.remote) .box::before { content: ''; position: absolute; inset: -10px; }
  :global(html.remote) .box.none { border: 1.5px dashed color-mix(in srgb, var(--text-muted) 50%, transparent); }
  :global(html.remote) .box.none::after { display: none; }
  :global(html.remote) .meta { flex-basis: calc(100% - 30px); min-width: 0; gap: 4px; margin: -2px 0 -6px 30px; min-height: 32px; }
  :global(html.remote) .acts { display: flex; gap: 2px; margin: 0 -6px 0 auto; }
  :global(html.remote) .x, :global(html.remote) .later { opacity: 1; }
  :global(html.remote) .x { display: grid; place-items: center; min-width: 34px; height: 32px; padding: 0 6px; border-radius: var(--r6); font-size: 15px; }
  :global(html.remote) .x.armed { font-size: var(--fs-meta); }
  :global(html.remote) .later { line-height: 26px; padding: 0 10px; }
  :global(html.remote) .when { padding: 4px 3px; }
  :global(html.remote) .line textarea, :global(html.remote) textarea.edit, :global(html.remote) .search { font-size: 16px; line-height: 22px; }
  :global(html.remote) .line kbd { display: none; }
  :global(html.remote) .kinds button { min-width: 32px; min-height: 30px; }
  :global(html.remote) .pick button { min-height: 40px; padding: 6px 10px; }
  .pick.agents { min-width: 220px; max-width: 320px; max-height: 320px; overflow-y: auto; }
  .pick.agents .head { margin: 2px 8px 4px; font-size: var(--fs-tag); color: var(--text-muted); }
  .pick.agents .off { margin: 0 8px 4px; font-size: var(--fs-tag); color: var(--warn); }
  .pick.agents button { justify-content: space-between; gap: 12px; }
  .pick.agents button:disabled { opacity: 0.5; cursor: default; }
  .pick.agents .who { display: flex; flex-direction: column; min-width: 0; }
  .pick.agents .who b, .pick.agents .who small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pick.agents .who small { color: var(--text-muted); font-size: var(--fs-tag); }
  .pick.agents .st { flex: none; font-size: var(--fs-tag); color: var(--ok); }
  .pick.agents .st.busy { color: var(--info); }
  .x.armed { opacity: 1; padding: 0 6px; border-radius: var(--r4); background: var(--ko-bg); color: var(--ko); font-weight: 500; }
</style>
