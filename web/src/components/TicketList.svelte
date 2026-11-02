<script lang="ts" module>
  /** Block-specific grouping (board of origin…), offered in addition to status and assignee. */
  export interface Grouping {
    key: string
    label: string
    of: (t: import('../types').Ticket) => { name: string; color: string; rank: number }
  }
</script>

<script lang="ts">
  import { t as tr, tc } from '../i18n.svelte'
  import { clock } from '../clock.svelte'
  import { offers } from '../tracker'
  import type { Snippet } from 'svelte'
  import { pref, setPref } from '../layout.svelte'
  import type { Person, StatusOption, Ticket } from '../types'
  import { isDone, statusColor } from '../rules'
  import { conventions } from '../conventions'
  import TicketRow from './TicketRow.svelte'
  import Icon from './Icon.svelte'
  import { anchored, dismissable, exclusive } from '../popup'

  /**
   * ClickUp ticket list for the Sprint and To pick blocks: one row per ticket (see TicketRow),
   * arranged in collapsible groups — by status, by assignee, or per block (`grouping`). A
   * single toolbar: Open / Unassigned / Done / All, priorities, the values of the custom fields
   * (`[clickup] fields`, those of `filter_fields`), grouping (by status, assignee, or one of those
   * fields) and sorting (priority then status, or any shown field: an estimate in hours); when it is
   * narrow, priorities, fields, grouping and sorting move into a "Filters" menu. "My
   * tickets" goes through the header's global Mine button; `people` adds a filter by assignee
   * (Everyone, me, each assignee, Unassigned), when the tracker has assignees.
   * Settings remembered under `<id>.show`, `<id>.priorities`, `<id>.fields`, `<id>.person`, `<id>.group`, `<id>.sort`, `<id>.desc` and `<id>.folded`.
   */
  let {
    id, tickets: all, statuses = [], statusesOf, me, grouping, tag, toSprint = false, special, people = false,
  }: {
    id: string
    tickets: Ticket[]
    /** The board's statuses, offered by the status picker and the menu. */
    statuses?: StatusOption[]
    /** Statuses offered for a ticket, when tickets come from different boards. */
    statusesOf?: (t: Ticket) => StatusOption[]
    me: Person | null
    /** Block-specific grouping, chosen by default. */
    grouping?: Grouping
    /** Block-specific label on each ticket's context line. */
    tag?: Snippet<[Ticket]>
    /** Offer "Add to sprint" in each ticket's menu ("To pick"). */
    toSprint?: boolean
    /**
     * Separate group for some tickets (tickets outside the sprint), placed just before "done";
     * within it, the tickets grouped as the others, under smaller headers.
     */
    special?: (t: Ticket) => { key: string; name: string; color: string } | null
    /** Offer the filter by assignee (Sprint). */
    people?: boolean
  } = $props()

  const optionsOf = (t: Ticket) => (statusesOf ? statusesOf(t) : statuses)
  const palette = $derived(statuses.length ? statuses : statusesOf ? all.flatMap(statusesOf) : [])
  const rank = (s: string) => {
    return conventions().workflow_statuses.indexOf(s)
  }

  /** ClickUp priorities, from most urgent to no priority. */
  const PRIORITIES = ['urgent', 'high', 'normal', 'low', 'none'] as const
  type Priority = (typeof PRIORITIES)[number]
  const priorityOf = (t: Ticket): Priority => t.priority ?? 'none'

  type Show = 'mine' | 'open' | 'unassigned' | 'done' | 'all'
  type Group = 'status' | 'assignee' | 'none' | string
  /**
   * On the phone (the remote), "Mine" (my open tickets) first, "Open" by default as on the Mac,
   * remembered under `<id>.phoneShow`: this phone's own, the Mac's `<id>.show` untouched.
   */
  const phone = document.documentElement.classList.contains('remote')
  const showKey = phone ? 'phoneShow' : 'show'
  // svelte-ignore state_referenced_locally
  let show = $state(pref<Show>(`${id}.${showKey}`, 'open'))
  // svelte-ignore state_referenced_locally
  let priorities = $state(pref<Priority[]>(`${id}.priorities`, []))
  /** Chosen values of each custom field, by field name; `''`: the tickets without a value. */
  // svelte-ignore state_referenced_locally
  let fieldFilter = $state(pref<Record<string, string[]>>(`${id}.fields`, {}))
  /** `''`: everyone; `me`; `none`: the unassigned tickets; `p:<id>`: a person's. */
  // svelte-ignore state_referenced_locally
  let person = $state(pref<string>(`${id}.person`, ''))
  // svelte-ignore state_referenced_locally
  let group = $state(pref<Group>(`${id}.group`, grouping?.key ?? 'status'))
  /** `''`: priority, then status, then last update; `f:<name>`: a custom field's value. */
  // svelte-ignore state_referenced_locally
  let sort = $state(pref<string>(`${id}.sort`, ''))
  // svelte-ignore state_referenced_locally
  let desc = $state(pref<boolean>(`${id}.desc`, false))
  // svelte-ignore state_referenced_locally
  let folded = $state(pref<string[]>(`${id}.folded`, ['done']))
  function save<T>(key: string, value: T) {
    setPref(`${id}.${key}`, value)
    return value
  }
  const toggle = <T,>(list: T[], v: T) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v])

  /** The custom fields the tickets carry (`[clickup] fields`), in the config's order: all can sort. */
  const shownFields = $derived([...new Set(all.flatMap((t) => (t.fields ?? []).map((f) => f.name)))])
  /** Those that also filter and group (`[clickup] filter_fields`); the others are only shown. */
  const fieldNames = $derived([...new Set(all.flatMap((t) => (t.fields ?? []).filter((f) => f.filter !== false).map((f) => f.name)))])
  const valuesOf = (t: Ticket, name: string) => t.fields?.find((f) => f.name === name)?.values ?? []
  /** Each field's values among the open tickets, in ClickUp's option order, with their count; `none`: tickets without. */
  const fieldCounts = $derived(
    fieldNames.map((name) => {
      const found = new Map<string, { label: string; color: string | null; order: number; n: number }>()
      let none = 0
      for (const t of open) {
        const vs = valuesOf(t, name)
        if (!vs.length) none++
        for (const v of vs) {
          const cur = found.get(v.label) ?? { label: v.label, color: v.color, order: v.order ?? 1e6, n: 0 }
          cur.n++
          found.set(v.label, cur)
        }
      }
      return { name, values: [...found.values()].sort((a, b) => a.order - b.order || a.label.localeCompare(b.label)), none }
    }),
  )
  /** The chosen values still offered (a value gone from the tickets no longer filters). */
  const chosenOf = (name: string) => {
    const c = fieldCounts.find((f) => f.name === name)
    return (fieldFilter[name] ?? []).filter((v) => (v === '' ? !!c?.none : !!c?.values.some((x) => x.label === v)))
  }
  const setField = (name: string, values: string[]) => (fieldFilter = save('fields', { ...fieldFilter, [name]: values }))

  const groupings = $derived<{ key: Group; label: string }[]>([
    ...(grouping ? [{ key: grouping.key, label: grouping.label }] : []),
    { key: 'status', label: tr('status') },
    ...(offers('assignees') ? [{ key: 'assignee', label: tr('assignee') }] : []),
    ...fieldNames.map((name) => ({ key: `f:${name}`, label: name })),
    { key: 'none', label: tr('none') },
  ])
  // A remembered grouping that no longer exists (block changed) falls back to the first.
  const groupBy = $derived(groupings.some((g) => g.key === group) ? group : groupings[0].key)

  const open = $derived(all.filter((t) => !isDone(t)))
  const isMine = (t: Ticket) => !!me && t.assignees.some((a) => a.id === me.id)
  const counts = $derived({ mine: open.filter(isMine).length, open: open.length, unassigned: open.filter((t) => !t.assignees.length).length, done: all.length - open.length, all: all.length })
  /** Without a done ticket, "Open" and "All" are the same: only one of the two, and no "Done". */
  // Without assignees (Obsidian), no Unassigned: every ticket would be.
  const shows = $derived<Show[]>([...(phone && people && me && offers('assignees') ? (['mine'] as Show[]) : []), ...(['open'] as Show[]), ...(offers('assignees') ? (['unassigned'] as Show[]) : []), ...(counts.done ? (['done', 'all'] as Show[]) : [])])
  const showing = $derived(shows.includes(show) ? show : 'open')
  /** The people the tickets are assigned to, by name, with their open tickets; me apart. */
  const persons = $derived.by(() => {
    const m = new Map<number, { p: Person; n: number }>()
    for (const t of all) for (const a of t.assignees) if (a.id !== me?.id && !m.has(a.id)) m.set(a.id, { p: a, n: 0 })
    for (const t of open) for (const a of t.assignees) { const e = m.get(a.id); if (e) e.n++ }
    return [...m.values()].sort((a, b) => a.p.name.localeCompare(b.p.name))
  })
  const mine = $derived(counts.mine)
  const withPeople = $derived(people && offers('assignees'))
  /** A remembered person no longer among the assignees falls back to everyone. */
  const personBy = $derived(
    !withPeople ? '' : person === 'me' ? (me ? 'me' : '') : person.startsWith('p:') ? (persons.some((x) => `p:${x.p.id}` === person) ? person : '') : person,
  )
  const ofPerson = (t: Ticket) =>
    personBy === '' ? true
    : personBy === 'none' ? !t.assignees.length
    : personBy === 'me' ? t.assignees.some((a) => a.id === me?.id)
    : t.assignees.some((a) => `p:${a.id}` === personBy)
  const priorityCount = $derived(PRIORITIES.map((p) => [p, open.filter((t) => priorityOf(t) === p).length] as const).filter(([, n]) => n > 0))

  /** A remembered sort on a field no longer shown falls back to the default. */
  const sortBy = $derived(sort.startsWith('f:') && shownFields.includes(sort.slice(2)) ? sort : '')
  /**
   * By a field: its (first) value, numbers as numbers (`12.5` after `8`), a dropdown's options in
   * ClickUp's order; the tickets without a value last, whatever the direction.
   */
  function byField(a: Ticket, b: Ticket, name: string): number {
    const va = valuesOf(a, name)[0]
    const vb = valuesOf(b, name)[0]
    if (!va || !vb) return va ? -1 : vb ? 1 : 0
    const c = va.order != null && vb.order != null ? va.order - vb.order : va.label.localeCompare(vb.label, undefined, { numeric: true })
    return desc ? -c : c
  }

  const shown = $derived(
    all
      .filter((t) => (showing === 'all' ? true : showing === 'done' ? isDone(t) : showing === 'unassigned' ? !isDone(t) && !t.assignees.length : showing === 'mine' ? !isDone(t) && isMine(t) : !isDone(t)))
      .filter((t) => !priorities.length || priorities.includes(priorityOf(t)))
      .filter(ofPerson)
      .filter((t) =>
        fieldNames.every((name) => {
          const chosen = chosenOf(name)
          if (!chosen.length) return true
          const vs = valuesOf(t, name)
          return vs.length ? vs.some((v) => chosen.includes(v.label)) : chosen.includes('')
        }),
      )
      .sort(
        (a, b) =>
          (sortBy ? byField(a, b, sortBy.slice(2)) : 0) ||
          PRIORITIES.indexOf(priorityOf(a)) - PRIORITIES.indexOf(priorityOf(b)) ||
          rank(b.status) - rank(a.status) ||
          (b.updated_at ?? '').localeCompare(a.updated_at ?? ''),
      ),
  )

  interface Section { key: string; name: string; color: string; rank: number; tickets: Ticket[] }
  /** A ticket's group, special apart; done tickets all go to the "done" group, collapsed by default, unless only they are shown (Done): then grouped like the others, open. */
  function sectionOf(t: Ticket): Omit<Section, 'tickets'> {
    if (groupBy === 'none') return { key: '', name: '', color: '', rank: 0 }
    if (isDone(t) && showing !== 'done') return { key: 'done', name: tc('tickets', 'done'), color: statusColor('done', palette), rank: 1000 }
    if (groupBy === 'status') return { key: `s:${t.status}`, name: t.status, color: statusColor(t.status, palette), rank: -rank(t.status) }
    if (groupBy === 'assignee') {
      const a = t.assignees[0]
      if (!a) return { key: 'a:', name: tc('tickets', 'Unassigned'), color: 'var(--text-muted)', rank: 0 }
      return { key: `a:${a.id}`, name: a.id === me?.id ? `${a.name} ${tr('(you)')}` : a.name, color: a.id === me?.id ? 'var(--accent)' : 'var(--info)', rank: a.id === me?.id ? -1 : 1 }
    }
    // By a custom field: its (first) value, in ClickUp's option order; without one, after them.
    if (groupBy.startsWith('f:')) {
      const name = groupBy.slice(2)
      const v = valuesOf(t, name)[0]
      if (!v) return { key: 'f:', name: tr('No {name}', { name }), color: 'var(--text-muted)', rank: 998 }
      return { key: `f:${v.label}`, name: v.label, color: v.color ?? 'var(--info)', rank: v.order ?? 500 }
    }
    const g = grouping!.of(t)
    return { key: `g:${g.name}`, ...g }
  }

  /** Subtasks follow their parent when it is in the same group; otherwise "↑ parent" on their row. */
  function nest(list: Ticket[]) {
    const here = new Set(list.map((t) => t.task_id))
    const children = new Map<string, Ticket[]>()
    for (const t of list) if (t.parent && here.has(t.parent)) children.set(t.parent, [...(children.get(t.parent) ?? []), t])
    const out: { t: Ticket; depth: number }[] = []
    const visit = (t: Ticket, depth: number) => {
      out.push({ t, depth })
      for (const c of children.get(t.task_id) ?? []) visit(c, depth + 1)
    }
    for (const t of list) if (!t.parent || !here.has(t.parent)) visit(t, 0)
    return out
  }

  type Shown = Section & { rows: { t: Ticket; depth: number }[] }
  function grouped(list: Ticket[]): Shown[] {
    const m = new Map<string, Section>()
    for (const t of list) {
      const s = sectionOf(t)
      const cur = m.get(s.key) ?? { ...s, tickets: [] }
      cur.tickets.push(t)
      m.set(s.key, cur)
    }
    return [...m.values()].sort((a, b) => a.rank - b.rank || a.name.localeCompare(b.name)).map((s) => ({ ...s, rows: nest(s.tickets) }))
  }

  /** The groups; a special one holds its own, by the same grouping (`subs`, empty without grouping). */
  const sections = $derived.by(() => {
    const specials = new Map<string, Section>()
    const plain: Ticket[] = []
    for (const t of shown) {
      const sp = special?.(t)
      if (!sp) {
        plain.push(t)
        continue
      }
      const key = `x:${sp.key}`
      const cur = specials.get(key) ?? { key, name: sp.name, color: sp.color, rank: 999, tickets: [] }
      cur.tickets.push(t)
      specials.set(key, cur)
    }
    const out: (Shown & { subs: Shown[] })[] = [...grouped(plain).map((s) => ({ ...s, subs: [] })), ...[...specials.values()].map((s) => {
      const subs = grouped(s.tickets)
      return { ...s, rows: nest(s.tickets), subs: subs.length > 1 || subs[0]?.key ? subs : [] }
    })]
    return out.sort((a, b) => a.rank - b.rank || a.name.localeCompare(b.name))
  })

  const now = $derived(clock(1000).now)

  /** "Filters" menu of the narrow bar. */
  let filtersOpen = $state(false)
  let filtersButton: HTMLButtonElement | undefined = $state()
  /** Active filters out of view: chosen priorities, grouping other than the default. */
  const fieldFiltered = $derived(fieldNames.reduce((n, name) => n + chosenOf(name).length, 0))
  const activeFilters = $derived(priorities.length + fieldFiltered + (personBy ? 1 : 0) + (groupBy !== groupings[0].key ? 1 : 0) + (sortBy ? 1 : 0))

  const PRIORITY_COLOR: Record<Priority, string> = { urgent: 'var(--ko)', high: 'var(--warn)', normal: 'var(--info)', low: 'var(--text-muted)', none: 'transparent' }
  const LABEL: Record<Show, string> = { mine: tr('Mine'), open: tc('tickets', 'Open'), unassigned: tc('tickets', 'Unassigned'), done: tc('tickets', 'Done'), all: tc('tickets', 'All') }
  const PRIORITY_NAME: Record<Priority, string> = { urgent: tr('Urgent'), high: tr('High'), normal: tc('tickets', 'Normal'), low: tr('Low'), none: tc('tickets', 'None') }
</script>

{#snippet prioritiesUI()}
  <span class="prio" role="group" aria-label={tr('Priority')}>
    {#each priorityCount as [p, n] (p)}
      <button class:on={priorities.includes(p)} style="--c: {PRIORITY_COLOR[p]}" onclick={() => (priorities = save('priorities', toggle(priorities, p)))} title={p === 'none' ? tr('Show only tickets without priority') : tr('Show only {level} priority', { level: PRIORITY_NAME[p].toLowerCase() })}><i class:none={p === 'none'}></i>{PRIORITY_NAME[p]}<b>{n}</b></button>
    {/each}
    {#if priorities.length}<button class="clear" onclick={() => (priorities = save('priorities', []))}>{tr('clear')}</button>{/if}
  </span>
{/snippet}

{#snippet fieldsUI(caps: boolean)}
  {#each fieldCounts.filter((f) => f.values.length) as f (f.name)}
    {@const chosen = chosenOf(f.name)}
    {#if caps}<span class="cap">{f.name}</span>{/if}
    <span class="prio" role="group" aria-label={f.name}>
      {#if !caps}<span class="fname">{f.name}</span>{/if}
      {#each f.values as v (v.label)}
        <button class:on={chosen.includes(v.label)} style="--c: {v.color ?? 'var(--info)'}" onclick={() => setField(f.name, toggle(chosen, v.label))} title={tr('Show only the tickets whose {field} is {value}', { field: f.name, value: v.label })}><i></i>{v.label}<b>{v.n}</b></button>
      {/each}
      {#if f.none}<button class:on={chosen.includes('')} onclick={() => setField(f.name, toggle(chosen, ''))} title={tr('Show only the tickets without {field}', { field: f.name })}><i class="none"></i>{tc('tickets', 'None')}<b>{f.none}</b></button>{/if}
      {#if chosen.length}<button class="clear" onclick={() => setField(f.name, [])}>{tr('clear')}</button>{/if}
    </span>
  {/each}
{/snippet}

{#snippet personUI()}
  {#if withPeople}
    <label class="groupby" class:set={!!personBy}>{tr('Person')}
      <select value={personBy} onchange={(e) => (person = save('person', e.currentTarget.value))}>
        <option value="">{tr('everyone')}</option>
        {#if me}<option value="me">{tr('me')} ({mine})</option>{/if}
        {#each persons as x (x.p.id)}<option value="p:{x.p.id}">{x.p.name} ({x.n})</option>{/each}
        <option value="none">{tr('unassigned')} ({counts.unassigned})</option>
      </select>
    </label>
  {/if}
{/snippet}

{#snippet groupByUI()}
  <label class="groupby">{tr('Group by')}
    <select value={groupBy} onchange={(e) => (group = save('group', e.currentTarget.value))}>
      {#each groupings as g (g.key)}<option value={g.key}>{g.label}</option>{/each}
    </select>
  </label>
{/snippet}

{#snippet sortUI()}
  {#if shownFields.length}
    <label class="groupby">{tr('Sort by')}
      <select value={sortBy} onchange={(e) => (sort = save('sort', e.currentTarget.value))}>
        <option value="">{tr('priority')}</option>
        {#each shownFields as name (name)}<option value="f:{name}">{name}</option>{/each}
      </select>
      {#if sortBy}<button class="dir" onclick={() => (desc = save('desc', !desc))} title={desc ? tr('Largest first: click for smallest first') : tr('Smallest first: click for largest first')} aria-label={desc ? tr('Descending') : tr('Ascending')}>{desc ? '↓' : '↑'}</button>{/if}
    </label>
  {/if}
{/snippet}

<div class="toolbar">
  <div class="bar">
    <span class="seg" role="group" aria-label={tr('Tickets shown')}>
      {#each shows as s (s)}
        <button class:on={showing === s} onclick={() => (show = save(showKey, s))}>{LABEL[s]}<b>{counts[s]}</b></button>
      {/each}
    </span>
    <span class="wide">{@render prioritiesUI()}</span>
    {#if fieldNames.length}<span class="wide fields">{@render fieldsUI(false)}</span>{/if}
    <span class="grow"></span>
    <span class="wide">{@render personUI()}</span>
    <span class="wide">{@render sortUI()}</span>
    <span class="wide">{@render groupByUI()}</span>
    <button bind:this={filtersButton} class="filters narrow" class:on={filtersOpen} onclick={() => (filtersOpen = !filtersOpen)} aria-expanded={filtersOpen}>
      {tr('Filters')}{#if activeFilters}<b>·{activeFilters}</b>{/if} <Icon name="chevronDown" />
    </button>
  </div>
</div>
{#if filtersOpen}
  <div class="filters-menu" role="dialog" aria-label={tr('Filters')} {@attach anchored(() => filtersButton)} {@attach exclusive(() => (filtersOpen = false))} {@attach dismissable(() => (filtersOpen = false), { trigger: () => filtersButton })}>
    <span class="cap">{tr('Priority')}</span>
    {@render prioritiesUI()}
    {@render fieldsUI(true)}
    {#if withPeople}<span class="cap">{tr('Person')}</span>{@render personUI()}{/if}
    <span class="cap">{tr('Grouping')}</span>
    {@render groupByUI()}
    {#if shownFields.length}<span class="cap">{tr('Sorting')}</span>{@render sortUI()}{/if}
  </div>
{/if}
<div class="list">
  {#each sections as s (s.key)}
    {#if s.key}
      {@const isFolded = folded.includes(s.key)}
      <button class="grp" class:folded={isFolded} style="--c: {s.color}" onclick={() => (folded = save('folded', toggle(folded, s.key)))} aria-expanded={!isFolded}>
        <span class="name">{s.name}</span><b>{s.tickets.length}</b><span class="chev"><Icon name="chevronDown" /></span>
      </button>
      {#if !isFolded && s.subs.length}
        {#each s.subs as u (u.key)}
          {@const subKey = `${s.key}/${u.key}`}
          {@const subFolded = folded.includes(subKey)}
          <button class="grp sub" class:folded={subFolded} style="--c: {u.color}" onclick={() => (folded = save('folded', toggle(folded, subKey)))} aria-expanded={!subFolded}>
            <span class="name">{u.name}</span><b>{u.tickets.length}</b><span class="chev"><Icon name="chevronDown" /></span>
          </button>
          {#if !subFolded}
            {#each u.rows as { t, depth } (t.task_id)}<TicketRow ticket={t} {me} statuses={optionsOf(t)} {depth} {now} {tag} {toSprint} statusInGroup={u.key.startsWith('s:')} />{/each}
          {/if}
        {/each}
      {:else if !isFolded}
        {#each s.rows as { t, depth } (t.task_id)}<TicketRow ticket={t} {me} statuses={optionsOf(t)} {depth} {now} {tag} {toSprint} statusInGroup={s.key.startsWith('s:')} />{/each}
      {/if}
    {:else}
      {#each s.rows as { t, depth } (t.task_id)}<TicketRow ticket={t} {me} statuses={optionsOf(t)} {depth} {now} {tag} {toSprint} />{/each}
    {/if}
  {:else}
    <p class="empty">{showing === 'mine' && !priorities.length && !fieldFiltered && !personBy ? tr('No open ticket of yours.') : priorities.length || fieldFiltered || personBy || showing !== 'all' ? tr('No ticket matches these filters.') : tr('No ticket.')}</p>
  {/each}
</div>

<style>
  /* The bar measures its own width: when narrow, priorities and grouping move into "Filters". */
  .toolbar { container-type: inline-size; padding: 6px 12px; border-bottom: 1px solid var(--border); }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px; }
  .narrow { display: none; }
  @container (max-width: 460px) {
    .wide { display: none; }
    .narrow { display: inline-flex; }
  }
  .filters { align-items: center; gap: 3px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; }
  .filters.on { border-color: var(--accent); }
  .filters b { color: var(--accent); font-weight: 600; }
  .filters-menu { position: fixed; z-index: 45; display: flex; flex-direction: column; align-items: flex-start; gap: 6px; width: 260px; max-height: 60vh; overflow: auto; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); box-shadow: var(--shadow-pop); }
  .filters-menu .prio { flex-wrap: wrap; }
  .cap { font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.4px; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; font-size: var(--fs-meta); }
  .seg button { border: none; background: none; padding: 1px 9px; color: var(--text-muted); font: inherit; cursor: pointer; }
  .seg button + button { border-left: 1px solid var(--border); }
  .seg button b { margin-left: 4px; font-weight: 600; }
  .seg button:hover { color: var(--text); }
  .seg button.on { background: var(--surface-muted); color: var(--text); font-weight: 500; }
  .prio { display: inline-flex; align-items: center; gap: 2px; }
  .prio button { display: inline-flex; align-items: center; gap: 4px; padding: 1px 6px; border: 1px solid transparent; border-radius: var(--r10); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .prio button:hover { color: var(--text); }
  .prio button.on { border-color: var(--accent); color: var(--text); background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .prio b { font-weight: 600; color: var(--text); }
  .prio i { width: 7px; height: 7px; border-radius: 50%; background: var(--c); }
  .prio i.none { box-shadow: inset 0 0 0 1px var(--text-muted); }
  .prio .clear { color: var(--accent); }
  .fields { display: inline-flex; flex-wrap: wrap; align-items: center; gap: 4px 10px; }
  .fname { margin-right: 2px; font-size: var(--fs-tag); color: var(--text-muted); }
  .grow { flex: 1; }
  .groupby { display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-tag); color: var(--text-muted); }
  .groupby.set select { border-color: var(--accent); }
  .groupby select { border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); padding: 0 4px; }
  .groupby .dir { padding: 0 5px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .groupby .dir:hover { border-color: var(--accent); }
  .list { flex: 1; min-height: 0; overflow: auto; container: tickets / inline-size; }
  .grp { position: sticky; top: 0; z-index: 1; display: flex; align-items: center; gap: 7px; width: 100%; padding: 3px 12px; border: none; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--surface-muted) 70%, var(--surface)); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; text-align: left; cursor: pointer; }
  .grp::before { content: ''; flex-shrink: 0; width: 8px; height: 8px; border-radius: var(--r2); background: var(--c); }
  .grp .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grp b { color: var(--text); }
  .grp:hover { color: var(--text); }
  .chev { display: inline-flex; margin-left: auto; transition: transform 0.15s; }
  .grp.folded .chev { transform: rotate(-90deg); }
  /* A group within a special one (outside the sprint): under its header, which stays on top. */
  .grp.sub { position: static; padding-left: 26px; background: none; }
  /* On the phone: switches, Filters and group headers a thumb can hit. */
  :global(html.remote) .seg { max-width: 100%; overflow-x: auto; scrollbar-width: none; }
  :global(html.remote) .seg button { flex-shrink: 0; min-height: 32px; white-space: nowrap; }
  :global(html.remote) .filters { min-height: 32px; padding: 0 10px; }
  :global(html.remote) .grp { padding-block: 8px; }
  .empty { margin: 12px; color: var(--text-muted); font-size: var(--fs-meta); }
</style>
