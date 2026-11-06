<script lang="ts">
  import { threadAttr } from '../thread.svelte'
  import { t, tc, tn } from '../i18n.svelte'
  import { columns, grip } from '../columns.svelte'
  import { tildePath } from '../platform'
  import { armer } from '../armed.svelte'
  import { mrNumber, mrWord } from '../forge'
  import { agentIcon, agentName } from '../agent-cli'
  import { linkOnly } from '../links'
  import BlockControls from './BlockControls.svelte'
  import Icon from './Icon.svelte'
  import ArmedLabel from './ArmedLabel.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import { behindLimit, SESSION_LABEL, describeWorktrees, type WorktreeInfo, type WorktreeOrigin, type WorktreeState } from '../rules'
  import { agentsEnabled, desktopUrl, openAgents, openLauncher } from '../agents.svelte'
  import { deleteWorktree, openWorktree, pruneWorktrees, pullWorktree, size, switchWorktree } from '../worktrees'
  import BranchPicker from './BranchPicker.svelte'
  import { setQuery } from '../filter.svelte'
  import { toast } from '../toast.svelte'
  import { ago } from '../time'
  import type { Snapshot } from '../types'
  import { FILE_MANAGER } from '../platform'
  import { hasCore } from '../bridge'
  import { openWorktreeShell } from '../shell-drawer.svelte'
  import { canArchive, isHidden, setHidden } from '../worktree-archive.svelte'
  import OverlapChip from './OverlapChip.svelte'
  import { overlapsAt } from '../overlaps'

  /**
   * Primary repository worktrees: what each one is linked to, who
   * works in it, whether it is stale, and its deletion when nothing would be lost. Wide (Worktrees
   * view), a table with details on the right; narrow (Dashboard), one row per worktree and
   * details under the chosen row.
   */
  let { snap }: { snap: Snapshot } = $props()

  type Filter = 'all' | WorktreeState | WorktreeOrigin
  const all = $derived(describeWorktrees(snap))
  /** A worktree's overlaps, shown there when no agent works in it (the agent's card has them otherwise). */
  const overlapsOf = (path: string) => overlapsAt(snap, [path])
  const otherBase = (path: string) => snap.overlaps?.other_base.find((x) => x.path === path)
  /** Archived (`worktree-archive.svelte.ts`): out of the list and its counts, unless it shows only them (not remembered). */
  let showArchived = $state(false)
  const archivedCount = $derived(all.filter(isHidden).length)
  const infos = $derived(all.filter((i) => isHidden(i) === showArchived))
  /** Worktrees of several repositories (`[[repos]] worktrees`): each one names its repository. */
  const multi = $derived(new Set(snap.worktrees.map((w) => w.repo)).size > 1)
  let filter = $state<Filter>(pref<Filter>('worktrees.filter', 'all'))
  const setFilter = (f: Filter) => setPref('worktrees.filter', (filter = f === filter ? 'all' : f))
  const matches = (i: WorktreeInfo, f: Filter) => f === 'all' || i.state === f || i.origin === f
  const count = (f: Filter) => infos.filter((i) => matches(i, f)).length

  const GROUPS: [WorktreeState, string][] = [['in_use', t('In use')], ['stale', t('Stale')], ['gone', t('Folder gone')], ['idle', t('Idle')]]
  const groups = $derived(
    GROUPS.map(([st, label]) => ({
      st, label,
      items: infos.filter((i) => i.state === st && matches(i, filter)).sort((a, b) => Number(b.w.is_main) - Number(a.w.is_main) || (b.activity ?? '').localeCompare(a.activity ?? '')),
    })).filter((g) => g.items.length),
  )
  const stale = $derived(infos.filter((i) => i.state === 'stale'))
  const safe = $derived(stale.filter((i) => i.removable))
  const gone = $derived(infos.filter((i) => i.state === 'gone'))
  /** Rows for the short mode. */
  const SHORT = 3
  const disk = $derived(infos.reduce((n, i) => n + (i.w.size_bytes ?? 0), 0))

  let width = $state(0)
  const wide = $derived(width >= 980)
  const compact = $derived(width > 0 && width < 640)
  let selected = $state<string | null>(null)
  const current = $derived(infos.find((i) => i.w.path === selected) ?? (wide ? (groups[0]?.items[0] ?? null) : null))
  const pick = (i: WorktreeInfo) => (selected = !wide && selected === i.w.path ? null : i.w.path)

  let busy = $state(false)
  let error = $state('')
  async function run(f: () => Promise<unknown>) {
    if (busy) return
    busy = true
    error = ''
    try {
      await f()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
  /**
   * Two-click deletion: the first arms the button (which then says what will be lost), the
   * second deletes. No `confirm()`: Tauri's webview does not show it and always answers no.
   */
  const armed = armer(5000)
  function remove(i: WorktreeInfo, force = false) {
    if (!armed.confirm(`${force ? 'force' : 'rm'}:${i.w.path}`)) return
    void run(async () => {
      await deleteWorktree(i.w.path, force)
      toast(t('{name} removed', { name: i.name }))
    })
  }
  /** Pull (fast-forward only) and Change ▾: what they did, said in a toast. */
  const pull = (i: WorktreeInfo) => run(async () => toast(await pullWorktree(i.w.path)))
  const switchTo = (i: WorktreeInfo, branch: string) => run(async () => toast(await switchWorktree(i.w.path, branch)))
  /** Why the branch is not changed from here, or nothing. */
  const noSwitch = (i: WorktreeInfo) => (i.w.changes ? t('Uncommitted changes: commit or stash them first') : i.agents.some((a) => a.state === 'running') ? t('An agent runs in it: stop it first') : '')
  const prune = () => run(async () => {
    await pruneWorktrees()
    toast(tn(gone.length, '{n} worktree pruned', '{n} worktrees pruned'))
  })

  /** Cleanup: the stale worktrees, checked when nothing would be lost. */
  let sheet = $state(false)
  let chosen = $state<Record<string, boolean>>({})
  function openSheet() {
    chosen = Object.fromEntries(safe.map((i) => [i.w.path, true]))
    sheet = true
  }
  /** Stale first; those that would lose work are checked one by one, never by default. */
  const candidates = $derived(stale.filter((i) => i.removable || i.forceable))
  const blocked = $derived(stale.filter((i) => !i.removable && !i.forceable))
  const toRemove = $derived(candidates.filter((i) => chosen[i.w.path]))
  const forced = $derived(toRemove.filter((i) => i.forceable))
  const freed = $derived(toRemove.reduce((n, i) => n + (i.w.size_bytes ?? 0), 0))
  function cleanUp() {
    if (forced.length && !armed.confirm('sheet')) return
    removeMany(toRemove, () => (sheet = false))
  }
  /** One after the other; those that fail are listed, the others go. Forced where work would be lost. */
  function removeMany(list: WorktreeInfo[], done: () => void) {
    void run(async () => {
      const failed: string[] = []
      for (const i of list) {
        try {
          await deleteWorktree(i.w.path, i.forceable)
        } catch (e) {
          failed.push(`${i.name}: ${e}`)
        }
      }
      done()
      toast(tn(list.length - failed.length, '{n} worktree removed', '{n} worktrees removed'))
      if (failed.length) throw new Error(failed.join('\n'))
    })
  }

  /**
   * Selection in bulk: a box on each row whose folder is there (a folder gone is pruned instead), to
   * archive them (a main checkout too), or remove those that can go (not in use, not locked, not main). ⇧-click ticks or unticks
   * the range from the last box clicked. Two clicks remove, the second saying what would be lost.
   */
  let marked = $state<Record<string, boolean>>({})
  let lastMarked = ''
  const selectable = (i: WorktreeInfo) => canArchive(i.w)
  const shown = $derived(groups.flatMap((g) => g.items))
  const markedAll = $derived(infos.filter((i) => marked[i.w.path] && selectable(i)))
  const markedList = $derived(markedAll.filter((i) => i.removable || i.forceable))
  const markedForced = $derived(markedList.filter((i) => i.forceable))
  const markedSize = $derived(markedList.reduce((n, i) => n + (i.w.size_bytes ?? 0), 0))
  function archiveMarked() {
    setHidden(markedAll, !showArchived)
    toast(showArchived ? tn(markedAll.length, '{n} worktree back in the list', '{n} worktrees back in the list') : tn(markedAll.length, '{n} worktree archived', '{n} worktrees archived'))
    marked = {}
  }
  const visibleSelectable = $derived(shown.filter(selectable))
  function mark(i: WorktreeInfo, on: boolean, range: boolean) {
    const from = shown.findIndex((x) => x.w.path === lastMarked)
    const to = shown.findIndex((x) => x.w.path === i.w.path)
    const span = range && from >= 0 ? shown.slice(Math.min(from, to), Math.max(from, to) + 1) : [i]
    for (const x of span.filter(selectable)) marked[x.w.path] = on
    lastMarked = i.w.path
  }
  const markAll = (on: boolean) => { marked = on ? Object.fromEntries(visibleSelectable.map((i) => [i.w.path, true])) : {} }
  function removeMarked() {
    if (!armed.confirm('bulk')) return
    removeMany(markedList, () => (marked = {}))
  }

  const ORIGIN: Record<WorktreeOrigin, string> = { main: t('main repository'), desktop: 'Claude Desktop', agent: t('Prism agent'), manual: t('manual') }
  /** One line for the Claude column: what works in it, otherwise what worked in it. */
  /** `agent`: the linked Prism agent, which the row opens in the Agents view. */
  function claude(i: WorktreeInfo): { dot: string; text: string; agent?: string } | null {
    const a = i.agents.find((a) => a.state === 'running') ?? i.agents[0]
    if (a) return { dot: a.state === 'running' ? a.status : 'gone', text: `${a.title} · ${a.state === 'running' ? SESSION_LABEL[a.status] : t('agent stopped')}`, agent: a.id }
    const c = i.sessions[0]
    if (c) return { dot: c.status, text: `${i.sessions.length > 1 ? t('{n} sessions', { n: i.sessions.length }) : c.title} · ${SESSION_LABEL[c.status]}` }
    if (i.archived.length) return { dot: 'gone', text: t('archived') }
    return null
  }
  const branchOf = (i: WorktreeInfo) => i.w.branch ?? t('detached @ {head}', { head: i.w.head?.slice(0, 7) ?? '?' })
  const whyNot = (i: WorktreeInfo) =>
    i.w.is_main ? t('The repository’s main checkout') : i.w.locked ? t('Locked (git worktree unlock)') : i.busy ? t('In use: {why}', { why: i.busy }) : i.keep ? t('Would lose {what}', { what: i.keep }) : t('Remove the worktree and its local branch')
</script>

<section class="panel" style="--src: var(--src-gitlab)" class:collapsed={isCollapsed('worktrees')}>
  <h2>
    <span class="title"><Icon name="worktree" />Worktrees</span>
    <small>{infos.length} · {t('{n} in use', { n: infos.filter((i) => i.state === 'in_use').length })} · <span class:warn={stale.length}>{t('{n} stale', { n: stale.length })}</span>{#if disk}{' '}· {size(disk)}{/if}</small>
    {#if gone.length}<button class="hb" onclick={prune} disabled={busy} title={t('git worktree prune: forget the worktrees whose folder is gone')}><Icon name="prune" />{t('Prune {n}', { n: gone.length })}</button>{/if}
    {#if stale.length}<button class="hb primary" onclick={openSheet} disabled={busy} title={t('Remove stale worktrees: those holding nothing to lose are ticked, the others only if you tick them')}><Icon name="trash" />{t('Clean up')}{safe.length ? ` ${safe.length}` : ''}…</button>{/if}
    <BlockControls id="worktrees" />
  </h2>
  {#if isShort('worktrees')}
    <!-- Short: what is working, then one row for what needs cleaning up. -->
    <div class="short">
      {#each infos.filter((i) => i.state === 'in_use' && !i.w.is_main).slice(0, SHORT) as i (i.w.path)}
        {@const cl = claude(i)}
        <div class="srow" title={i.w.path}>
          <span class="st {cl?.dot ?? 'idle'}" title={cl?.text}></span>
          <b>{i.name}</b>
          {#if cl?.agent}<button class="ib go" onclick={() => openAgents(cl.agent)} title={`${cl.text}\n${t('Open this agent in the Agents view')}`} aria-label={t('Open the agent')}><Icon name="terminal" /></button>{/if}
          {#if i.w.ticket}<span class="pill tk">{i.w.ticket}</span>{/if}
          {#if i.mr}<a class="mono" href={i.mr.url} target="_blank" rel="noreferrer" title={i.mr.title}>{mrNumber(i.mr.iid)}</a>{/if}
          {#if i.w.behind > behindLimit()}<span class="pill warn">↓{i.w.behind}</span>{/if}
          {#if i.agents.length === 0 && overlapsOf(i.w.path).length}<OverlapChip seen={overlapsOf(i.w.path)} />{/if}
          <span class="grow"></span>
          <span class="muted mono">{ago(i.activity)}</span>
        </div>
      {:else}
        <div class="srow muted">{t('No worktree in use.')}</div>
      {/each}
      {#if stale.length || gone.length}
        <div class="srow sum">
          {#if stale.length}<span class="warn">{t('{n} stale', { n: stale.length })}</span>{#if safe.length}<span class="muted">· {t('{n} safe to remove', { n: safe.length })}</span>{/if}{/if}
          {#if gone.length}<span class="muted">· {t('{n} to prune', { n: gone.length })}</span>{/if}
        </div>
      {/if}
    </div>
  {:else}
    <div class="body" class:wide bind:clientWidth={width}>
      <div class="list">
        <div class="filter">
          {#each [['all', tc('infra', 'All')], ['in_use', t('In use')], ['stale', t('Stale')]] as [f, label] (f)}
            <button class:on={filter === f} onclick={() => setFilter(f as Filter)}>{label} {count(f as Filter)}</button>
          {/each}
          <span class="sep"></span>
          {#each [['agent', 'Prism'], ['desktop', 'Desktop'], ['manual', t('Manual')]] as [f, label] (f)}
            {#if count(f as Filter)}<button class:on={filter === f} onclick={() => setFilter(f as Filter)}>{label} {count(f as Filter)}</button>{/if}
          {/each}
          {#if archivedCount || showArchived}
            <span class="grow"></span>
            <button class="arch" class:on={showArchived} onclick={() => { showArchived = !showArchived; marked = {} }} aria-pressed={showArchived} title={showArchived ? t('Back to the list') : t('The worktrees archived: out of the list until something happens in them')}><Icon name="hide" />{archivedCount}</button>
          {/if}
        </div>
        {#if markedAll.length}
          <div class="bulk">
            <input type="checkbox" checked={markedAll.length === visibleSelectable.length} indeterminate={markedAll.length < visibleSelectable.length} onchange={(e) => markAll(e.currentTarget.checked)} aria-label={t('Select every worktree shown')} />
            <b>{t('{n} selected', { n: markedAll.length })}</b>{#if markedSize}<span class="muted">~{size(markedSize)}</span>{/if}
            {#if markedForced.length}<span class="lose">{t('{n} with unsaved work', { n: markedForced.length })}</span>{/if}
            <span class="grow"></span>
            <button onclick={() => markAll(false)} disabled={busy}>{t('Clear')}</button>
            <button onclick={archiveMarked} disabled={busy} title={showArchived ? t('Back in the list') : t('Out of the list until something happens in them (a commit, a change); nothing is removed')}><Icon name={showArchived ? 'show' : 'hide'} />{showArchived ? t('Unarchive') : t('Archive')} {markedAll.length}</button>
            {#if markedList.length}<button class="danger" class:armed={armed.is('bulk')} onclick={removeMarked} onblur={() => armed.is('bulk') && armed.disarm()} disabled={busy} title="{t('Each worktree folder and its local branch')}{markedForced.length ? `; ${markedForced.map((i) => t('{name} loses {what}', { name: i.name, what: i.keep ?? '' })).join(', ')}` : ''}">
              <Icon name="trash" />{#if busy}{t('Removing…')}{:else}<ArmedLabel armed={armed.is('bulk')} label={t('Remove {n}', { n: markedList.length })} armedLabel={markedForced.length ? t('Confirm: {n} lose work', { n: markedForced.length }) : t('Confirm')} />{/if}
            </button>{/if}
          </div>
        {/if}
        {#if error}<p class="err">{error}</p>{/if}
        <div class="scroll">
          <table class:compact {@attach columns('worktrees', compact)}>
            {#if !compact}
              <thead><tr><th {@attach grip}>{t('Worktree')}</th><th {@attach grip}>{t('Ticket')}</th><th {@attach grip}>MR</th><th {@attach grip}>{agentName()}</th><th {@attach grip}>Git</th><th {@attach grip}>{t('Activity')}</th><th>{t('Status')}</th></tr></thead>
            {/if}
            <tbody>
              {#each groups as g (g.st)}
                <tr class="g"><td colspan="7">{g.label}<span>{g.items.length}</span></td></tr>
                {#each g.items as i (i.w.path)}
                  {@const cl = claude(i)}
                  <tr class="r" class:on={current?.w.path === i.w.path} class:dim={g.st === 'gone' || g.st === 'idle'} data-thread={i.w.is_main || !i.w.branch ? undefined : threadAttr({ branch: i.w.branch, repo: i.w.repo })} onclick={() => pick(i)}>
                    <td>
                      <div class="first">
                      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                      <span class="ck" onclick={(e) => e.stopPropagation()}>{#if selectable(i)}<input type="checkbox" checked={!!marked[i.w.path]} onclick={(e) => mark(i, e.currentTarget.checked, e.shiftKey)} title={i.forceable ? t('Select (removing it would lose {what})', { what: i.keep ?? '' }) : i.removable ? t('Select to archive or remove (⇧-click: a range)') : t('Select to archive (⇧-click: a range)')} aria-label={t('Select {name}', { name: i.name })} />{/if}</span>
                      <div class="nm" title={i.w.path}>
                        <b><span class="kind {i.origin}" title={ORIGIN[i.origin]}></span>{#if multi}<span class="repo">{i.w.repo}</span>{/if}{i.name}{#if i.w.is_main}<i> ({t('main')})</i>{/if}</b>
                        <span class="br" class:det={!i.w.branch}>{branchOf(i)}</span>
                      </div>
                      </div>
                    </td>
                    {#if !compact}
                      <td>{#if i.w.ticket}<span class="pill tk" title={i.ticket?.name}>{i.w.ticket}</span> <span class="muted">{i.ticket?.status ?? ''}</span>{/if}</td>
                      <td>
                        {#if i.mr}
                          <span class="lnk"><a href={i.mr.url} target="_blank" rel="noreferrer" onclick={linkOnly} title={i.mr.title}>{mrNumber(i.mr.iid)}</a>
                          <span class="pill" class:info={i.mr.state === 'opened'} class:merged={i.mr.state === 'merged'}>{i.mr.state === 'merged' ? tc('infra', 'merged') : i.mr.draft ? t('draft') : tc('state', 'open')}</span>
                          {#if i.mr.state === 'opened' && i.mr.pipeline === 'failed'}<span class="pill ko">CI</span>{/if}</span>
                        {/if}
                      </td>
                      <td>
                        {#if cl?.agent}
                          <button class="cl go" onclick={(e) => { e.stopPropagation(); openAgents(cl.agent) }} title={`${cl.text}\n${t('Open this agent in the Agents view')}`}><span class="st {cl.dot}"></span><span class="tx">{cl.text}</span><Icon name="terminal" /></button>
                        {:else if cl}<span class="cl" title={cl.text}><span class="st {cl.dot}"></span><span class="tx">{cl.text}</span></span>{/if}
                      </td>
                      <td>
                        {#if !i.w.prunable}
                          <span class="git">
                            {#if i.w.behind}<span class:b={i.w.behind > behindLimit()} title={t('{n} behind origin/{base}', { n: i.w.behind, base: i.w.base })}>↓{i.w.behind}</span>{/if}
                            {#if i.w.ahead}<span class="a" title={t('{n} ahead of origin/{base}', { n: i.w.ahead, base: i.w.base })}>↑{i.w.ahead}</span>{/if}
                            {#if i.w.changes}<span class="c" title={tn(i.w.changes, '{n} uncommitted change', '{n} uncommitted changes')}>●{i.w.changes}</span>{/if}
                            {#if i.w.untracked}<span class="c" title={t('{n} untracked (outside .claude/)', { n: i.w.untracked })}>?{i.w.untracked}</span>{/if}
                            {#if i.w.unpushed}<span class="u" title={tn(i.w.unpushed, '{n} commit on no remote branch', '{n} commits on no remote branch')}>⇡{i.w.unpushed}</span>{/if}
                            {#if !i.w.changes && !i.w.untracked && !i.w.unpushed && !i.w.behind && !i.w.ahead}<span class="muted">{t('clean')}</span>{/if}
                          </span>
                        {/if}
                      </td>
                      <td class="muted mono">{ago(i.activity)}</td>
                    {/if}
                    <td>
                      <span class="why">
                        {#if compact && cl}<span class="st {cl.dot}" title={cl.text}></span>{/if}
                        {#if g.st === 'gone'}<span class="pill">{t('prunable')}</span>
                        {:else if g.st === 'in_use'}<span class="pill ok">{t('active')}</span>
                        {:else}
                          {#each i.reasons.slice(0, 2) as r (r)}<span class="pill warn">{r}</span>{/each}
                          {#if i.reasons.length > 2}<span class="pill warn" title={i.reasons.join(' · ')}>+{i.reasons.length - 2}</span>{/if}
                          {#if g.st === 'stale'}{#if i.removable}<span class="pill ok">{t('safe')}</span>{:else if i.keep}<span class="pill ko" title={t('Would lose {what}', { what: i.keep })}>{t('keep')}</span>{/if}{/if}
                        {/if}
                        {#if i.w.behind > behindLimit() && g.st !== 'stale' && g.st !== 'gone' && !i.w.is_main}<span class="pill warn">{t('behind {n}', { n: i.w.behind })}</span>{/if}
                        {#if i.agents.length === 0 && overlapsOf(i.w.path).length}<OverlapChip seen={overlapsOf(i.w.path)} />{/if}
                        {#if otherBase(i.w.path)}<span class="pill" title={t('On {base}: its changes are not compared with those of the worktrees on the repository’s base', { base: i.w.base })}>{t('not compared: other base')}</span>{/if}
                      </span>
                    </td>
                  </tr>
                  {#if !wide && selected === i.w.path}
                    <!-- Pinned to the visible width: the table can be wider and scroll sideways. -->
                    <tr class="inline"><td colspan="7"><div class="pinned" style:width="{width}px">{@render detail(i)}</div></td></tr>
                  {/if}
                {/each}
              {:else}
                <tr><td class="empty muted" colspan="7">{#if showArchived}{t('Nothing archived any more.')}{:else}{filter !== 'all' ? t('No worktree for this filter.') : t('No worktree.')}{/if}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
      {#if wide && current}<aside class="side">{@render detail(current)}</aside>{/if}
    </div>
  {/if}

  {#if sheet}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="scrim" onclick={(e) => e.target === e.currentTarget && (sheet = false)}>
      <div class="sheet" role="dialog" aria-label={t('Clean up stale worktrees')}>
        <div class="hd"><Icon name="trash" /><b>{t('Clean up stale worktrees')}</b><span class="grow"></span><button class="ib" onclick={() => (sheet = false)} aria-label={t('Close')}><Icon name="close" /></button></div>
        <div class="bd">
          {#each candidates as i (i.w.path)}
            <label class:risky={i.forceable} title={i.forceable ? t('Would lose {what}', { what: i.keep ?? '' }) : i.w.path}>
              <input type="checkbox" checked={!!chosen[i.w.path]} onchange={(e) => (chosen[i.w.path] = e.currentTarget.checked)} />
              <span><b>{i.name}</b> <span class="muted mono">{branchOf(i)}</span>{#if i.forceable} <span class="lose">· {t('loses {what}', { what: i.keep ?? '' })}</span>{/if}</span>
              <span class="pill {i.forceable ? 'ko' : 'warn'}">{i.forceable ? t('unsaved work') : i.reasons[0]}</span>
            </label>
          {/each}
          {#each blocked as i (i.w.path)}
            <label class="off" title={whyNot(i)}>
              <input type="checkbox" disabled />
              <span><b>{i.name}</b> <span class="muted">· {i.busy ?? t('locked')}</span></span>
              <span class="pill">{t('in use')}</span>
            </label>
          {/each}
        </div>
        <div class="ft">
          <span class="muted">{t('Worktree folder and local branch.')}{#if freed}{' '}{t('~{size} freed.', { size: size(freed) })}{/if}</span>
          <span class="grow"></span>
          <button onclick={() => (sheet = false)}>{t('Cancel')}</button>
          <button class="primary" class:danger={forced.length > 0} class:armed={armed.is('sheet')} disabled={busy || !toRemove.length} onclick={cleanUp}>
            {#if busy}{t('Removing…')}{:else}<ArmedLabel armed={armed.is('sheet')} label={`${t('Remove {n}', { n: toRemove.length })}${forced.length ? ` (${t('{n} with unsaved work', { n: forced.length })})` : ''}`} armedLabel={t('Click again to confirm')} />{/if}
          </button>
        </div>
      </div>
    </div>
  {/if}
</section>

{#snippet detail(i: WorktreeInfo)}
  <div class="detail">
    <div class="sec head">
      <div class="hrow">
        <b><span class="kind {i.origin}"></span>{i.name}</b>
        <span class="tools">
          {#if i.w.prunable}
            <button class="tb txt" onclick={prune} disabled={busy}><Icon name="prune" />{t('Prune')}</button>
          {:else}
            <button class="tb" onclick={() => run(() => openWorktree(i.w.path, 'editor'))} disabled={!snap.open_with?.editor} title={snap.open_with?.editor ? t('Open in {app}', { app: snap.open_with.editor }) : t('No editor in [open_with]')} aria-label={t('Editor')}><Icon name="code" /></button>
            {#if hasCore()}<button class="tb" onclick={() => openWorktreeShell(i.w.path, i.name)} title={t('A shell in Prism, in this folder: tmux keeps it after closing')} aria-label={t('Shell')}><Icon name="terminal" /></button>{/if}
            <button class="tb" onclick={() => run(() => openWorktree(i.w.path, 'terminal'))} title={t('Open the folder in the terminal app')} aria-label={t('Terminal')}><Icon name="external" /></button>
            <button class="tb" onclick={() => run(() => openWorktree(i.w.path, 'finder'))} title={t('Open in {app}', { app: FILE_MANAGER })} aria-label={FILE_MANAGER}><Icon name="folder" /></button>
            {#if agentsEnabled()}<button class="tb txt primary" onclick={() => openLauncher({ title: i.w.ticket ?? i.name, cwd: i.w.path })}><Icon name={agentIcon()} />{t('Agent here')}</button>{/if}
          {/if}
        </span>
      </div>
      <span class="path">{tildePath(i.w.path)}</span>
      <span class="tags"><span class="pill">{ORIGIN[i.origin]}</span>{#if i.w.size_bytes}<span class="pill">{size(i.w.size_bytes)}</span>{/if}{#if i.w.locked}<span class="pill warn">{t('locked')}</span>{/if}</span>
    </div>
    <div class="sec">
      <span class="cap">{t('Linked to')}</span>
      <div class="kv">
        <span>{t('Branch')}</span><span class="line"><span class="mono">{branchOf(i)}</span>{#if hasCore() && !i.w.prunable}<BranchPicker path={i.w.path} disabled={busy || !!noSwitch(i)} why={noSwitch(i)} onpick={(b) => switchTo(i, b)} />{/if}</span>
        {#if i.w.upstream}<span>{t('Upstream')}</span><span class="line"><span class="mono">{i.w.upstream}</span>{#if i.w.upstream_gone}<span class="pill warn">{t('gone')}</span>{:else if i.w.unpulled}<span class="pill warn" title={tn(i.w.unpulled, '{n} commit on origin not here yet', '{n} commits on origin not here yet')}>↓{i.w.unpulled}</span>{#if hasCore()}<button class="pull" onclick={() => pull(i)} disabled={busy} title={t('Bring them here: fast-forward only, nothing rebased nor merged')}><Icon name="down" />{t('Pull')}</button>{/if}{/if}</span>{/if}
        {#if !i.w.prunable}<span>{t('Base')}</span><span>origin/{i.w.base} · ↓{i.w.behind} ↑{i.w.ahead}</span>{/if}
        {#if i.w.ticket}<span>{t('Ticket')}</span><span>{#if i.ticket}<a href={i.ticket.url} target="_blank" rel="noreferrer">{i.w.ticket}</a> · {i.ticket.status} · {i.ticket.name}{:else}<button class="link" onclick={() => setQuery(i.w.ticket!)}>{i.w.ticket}</button>{/if}</span>{/if}
        {#if i.mr}<span>{mrWord()}</span><span><a href={i.mr.url} target="_blank" rel="noreferrer">{mrNumber(i.mr.iid)}</a> · {i.mr.state === 'merged' ? t('merged {ago} ago', { ago: ago(i.mr.merged_at) }) : i.mr.draft ? t('draft') : tc('state', 'open')}{i.mr.pipeline ? ` · CI ${i.mr.pipeline}` : ''}</span>{/if}
        {#if i.w.last_commit}<span>{t('Last commit')}</span><span>{t('{ago} ago', { ago: ago(i.w.last_commit) })}</span>{/if}
      </div>
    </div>
    {#if i.agents.length || i.sessions.length || i.archived.length}
      <div class="sec">
        <span class="cap">{agentName()}</span>
        {#each i.agents as a (a.id)}
          <button class="sess" onclick={() => openAgents(a.id)} title={t('Open in the Agents view')}><span class="st {a.state === 'running' ? a.status : 'gone'}"></span><span class="t">{a.title}</span><span class="pill cl">{t('agent')}</span><span class="muted">{a.state === 'running' ? SESSION_LABEL[a.status] : t('stopped')}</span></button>
        {/each}
        {#each i.sessions as c (c.id)}
          <a class="sess" href={desktopUrl(c.id) ?? undefined}><span class="st {c.status}"></span><span class="t">{c.title}</span><span class="pill">Desktop</span><span class="muted">{ago(c.last_activity)}</span></a>
        {/each}
        {#each i.archived as c (c.id)}
          <span class="sess"><span class="st gone"></span><span class="t">{c.title}</span><span class="pill">Desktop</span><span class="muted">{t('archived')}</span></span>
        {/each}
      </div>
    {/if}
    {#if !i.w.prunable && (!i.w.is_main || canArchive(i.w))}
      <div class="sec">
        {#if !i.w.is_main}
          <span class="cap">{t('Verdict')}</span>
          {#if i.state === 'stale'}<div class="verdict stale"><b>{t('Stale')}</b><span>{i.reasons.join(' · ')}</span></div>{/if}
          {#if i.busy}<div class="verdict busy"><b>{t('In use')}</b><span>{i.busy}</span></div>
          {:else if i.keep}<div class="verdict kept" class:armed={armed.is(`force:${i.w.path}`)}><b>{t('Would lose work')}</b><span>{i.keep}. {armed.is(`force:${i.w.path}`) ? t('Click Confirm to delete it for good.') : t('Remove anyway deletes it for good.')}</span></div>
          {:else if i.removable}<div class="verdict safe"><b>{t('Safe to remove')}</b><span>{i.w.unpushed ? t('Nothing uncommitted; its {n} local commits predate the merge of {mr}.', { n: i.w.unpushed, mr: mrNumber(i.mr?.iid ?? '') }) : t('Nothing uncommitted, every commit is on origin.')}</span></div>{/if}
        {/if}
        <div class="acts">
          <span class="grow"></span>
          {#if canArchive(i.w)}
            {@const hidden = isHidden(i)}
            <button class="quiet" onclick={() => setHidden([i], !hidden)} title={hidden ? t('Back in the list, its archived agents too') : t('Out of the list until something happens in it (a commit, a change); nothing is removed')}><Icon name={hidden ? 'show' : 'hide'} />{hidden ? t('Unarchive') : t('Archive')}</button>
          {/if}
          {#if i.forceable}
            <button class="danger" class:armed={armed.is(`force:${i.w.path}`)} onclick={() => remove(i, true)} onblur={() => armed.is(`force:${i.w.path}`) && armed.disarm()} disabled={busy} title={t('Remove even though {what} would be lost', { what: i.keep ?? '' })}>
              <Icon name="trash" /><ArmedLabel armed={armed.is(`force:${i.w.path}`)} label={t('Remove anyway…')} armedLabel={t('Confirm: lose it')} />
            </button>
          {:else if !i.w.is_main}
            <button class="danger" class:armed={armed.is(`rm:${i.w.path}`)} onclick={() => remove(i)} onblur={() => armed.is(`rm:${i.w.path}`) && armed.disarm()} disabled={busy || !i.removable} title={whyNot(i)}>
              <Icon name="trash" /><ArmedLabel armed={armed.is(`rm:${i.w.path}`)} label={t('Remove')} armedLabel={t('Confirm')} />
            </button>
          {/if}
        </div>
      </div>
    {/if}
  </div>
{/snippet}

<style>
  h2 small .warn { color: var(--warn); }
  .hb { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .hb.primary, .sheet .primary { background: var(--src-gitlab); border-color: var(--src-gitlab); color: #fff; background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--src-gitlab); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
  .hb:disabled { opacity: 0.5; }
  .short { display: flex; flex-direction: column; }
  .srow { display: flex; align-items: center; gap: 6px; min-height: 26px; padding: 3px 12px; border-top: 1px solid var(--border); font-size: var(--fs-body); white-space: nowrap; }
  .srow:first-child { border-top: none; }
  .srow b { font-weight: 550; overflow: hidden; text-overflow: ellipsis; }
  .srow a { font-size: var(--fs-meta); }
  .srow.sum { font-size: var(--fs-meta); }
  .srow .warn { color: var(--warn); }
  .body { flex: 1; min-height: 0; display: flex; }
  .list { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }
  .filter { display: flex; gap: 4px; padding: 7px 12px; flex-wrap: wrap; border-bottom: 1px solid var(--border); }
  .filter button { padding: 0 8px; border-radius: var(--r10); border: 1px solid var(--border); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .filter button.on { background: color-mix(in srgb, var(--src-gitlab) 15%, transparent); border-color: color-mix(in srgb, var(--src-gitlab) 40%, transparent); color: var(--src-gitlab); }
  .filter .sep { width: 1px; background: var(--border); margin: 2px 3px; }
  .filter .grow { flex: 1; }
  .filter .arch { display: inline-flex; align-items: center; gap: 4px; }
  .err { margin: 0; padding: 6px 12px; background: var(--ko-bg); color: var(--ko); font-size: var(--fs-meta); white-space: pre-wrap; }
  table { width: 100%; border-collapse: collapse; font-size: var(--fs-body); }
  thead th { position: sticky; top: 0; z-index: 1; background: var(--surface); text-align: left; font-weight: 500; font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); padding: 5px 8px; border-bottom: 1px solid var(--border); }
  td { padding: 4px 8px; border-top: 1px solid var(--border); vertical-align: middle; white-space: nowrap; }
  td:first-child, th:first-child { padding-left: 12px; }
  tr.g td { padding: 10px 12px 3px; font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); border-top: none; }
  tr.g td span { margin-left: 6px; }
  tr.r { cursor: pointer; }
  tr.r:hover td { background: var(--surface-muted); }
  tr.r.on td { background: color-mix(in srgb, var(--src-gitlab) 11%, transparent); }
  tr.r.on td:first-child { box-shadow: inset 3px 0 0 var(--src-gitlab); }
  tr.r.dim td { opacity: 0.65; }
  tr.inline td { padding: 0; background: var(--bg); }
  .first { display: flex; align-items: center; gap: 6px; }
  .ck { display: inline-flex; width: 14px; flex-shrink: 0; }
  .ck input, .bulk input { margin: 0; cursor: pointer; }
  .bulk { display: flex; align-items: center; gap: 8px; padding: 5px 12px; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--src-gitlab) 8%, transparent); font-size: var(--fs-meta); }
  .bulk .lose { color: var(--ko); }
  .bulk button { display: inline-flex; align-items: center; gap: 5px; padding: 2px 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .bulk button:disabled { opacity: 0.45; cursor: not-allowed; }
  .bulk .danger { color: var(--ko); }
  .bulk .danger:hover:not(:disabled) { border-color: var(--ko); }
  .bulk .danger.armed { background: var(--ko-bg); border-color: var(--ko); color: var(--ko); }
  .nm { display: flex; flex-direction: column; max-width: 260px; min-width: 0; }
  .nm b { font-weight: 550; overflow: hidden; text-overflow: ellipsis; }
  .nm i { font-style: normal; font-weight: 400; color: var(--text-muted); }
  .nm .br { font: var(--fs-meta) var(--mono); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; }
  .nm .br.det { font-style: italic; }
  .compact .nm { max-width: none; }
  .compact td:last-child { width: 1%; }
  /* On the phone: the name takes what the reasons leave them, cut short; the reasons wrap. */
  :global(html.remote) .compact td:first-child { max-width: 0; width: 100%; }
  :global(html.remote) .compact .why { max-width: 150px; }
  .repo { margin-right: 6px; padding: 0 5px; border-radius: var(--r4); background: var(--surface-muted); color: var(--text-muted); font-weight: 400; font-size: var(--fs-meta); }
  .kind { display: inline-block; width: 7px; height: 7px; border-radius: var(--r2); margin-right: 6px; vertical-align: 1px; }
  .kind.agent { background: var(--src-claude); }
  .kind.desktop { border: 1.5px dashed var(--src-claude); }
  .kind.manual { background: var(--text-muted); }
  .kind.main { background: var(--accent); }
  .lnk { display: inline-flex; align-items: center; gap: 4px; }
  .lnk a { font: var(--fs-meta) var(--mono); }
  .pill.tk { background: color-mix(in srgb, var(--src-clickup) 20%, transparent); color: var(--src-clickup); }
  .pill.merged { background: color-mix(in srgb, var(--src-clickup) 18%, transparent); color: var(--src-clickup); }
  .pill.cl { background: color-mix(in srgb, var(--src-claude) 20%, transparent); color: var(--src-claude); }
  .cl { display: inline-flex; align-items: center; gap: 5px; max-width: 220px; min-width: 0; color: var(--text-muted); font-size: var(--fs-meta); }
  /* Linked Prism agent: a click opens it in the Agents view. */
  button.cl { padding: 1px 6px 1px 4px; margin-left: -4px; border: 1px solid transparent; border-radius: var(--r6); background: none; font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  button.cl :global(svg) { flex-shrink: 0; color: var(--src-claude); opacity: 0.6; }
  button.cl:hover { border-color: color-mix(in srgb, var(--src-claude) 45%, transparent); color: var(--text); }
  button.cl:hover :global(svg) { opacity: 1; }
  .ib.go { color: var(--src-claude); }
  /* The session name is truncated (…) in the column, terminal icon included; the full name is in the tooltip. */
  .cl .tx { flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .git { display: inline-flex; gap: 6px; font: var(--fs-meta) var(--mono); color: var(--text-muted); }
  .git .b { color: var(--warn); } .git .a { color: var(--ok); } .git .c { color: var(--warn); } .git .u { color: var(--ko); }
  .why { display: inline-flex; align-items: center; gap: 3px 4px; flex-wrap: wrap; white-space: normal; max-width: 260px; }
  .st { width: 8px; height: 8px; border-radius: 50%; display: inline-block; flex-shrink: 0; }
  .st.running { background: var(--ok); animation: pulse 1.4s infinite; }
  .st.background { background: var(--ok); opacity: 0.6; }
  .st.needs_you { background: var(--warn); }
  .st.waiting_tool { background: var(--ko); }
  .st.review_ready { background: var(--info); }
  .st.completed, .st.idle { background: var(--text-muted); opacity: 0.5; }
  .st.gone { border: 1.5px solid var(--text-muted); }
  .empty { padding: 14px 12px; }

  .side { width: 340px; flex-shrink: 0; border-left: 1px solid var(--border); overflow: auto; }
  .detail { display: flex; flex-direction: column; }
  .sec { display: flex; flex-direction: column; gap: 5px; padding: 9px 12px; border-bottom: 1px solid var(--border); }
  .sec:last-child { border-bottom: none; }
  .head b { font-size: 13.5px; }
  .path { font: var(--fs-meta) var(--mono); color: var(--text-muted); word-break: break-all; }
  .tags { display: flex; gap: 5px; flex-wrap: wrap; }
  .cap { font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .kv { display: grid; grid-template-columns: 84px minmax(0, 1fr); gap: 3px 8px; font-size: var(--fs-meta); }
  .kv > span { overflow-wrap: anywhere; }
  .kv > span:nth-child(odd) { color: var(--text-muted); }
  .link { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; }
  .sess { display: flex; align-items: center; gap: 7px; padding: 4px 7px; border: none; border-radius: var(--r7); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; text-decoration: none; }
  button.sess, a.sess { cursor: pointer; }
  button.sess:hover, a.sess:hover { background: var(--surface-muted); }
  .sess .t { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .verdict { display: flex; flex-direction: column; gap: 2px; padding: 7px 9px; border-radius: var(--r7); font-size: var(--fs-meta); }
  .verdict.stale, .verdict.busy { background: color-mix(in srgb, var(--warn) 10%, transparent); color: var(--warn); }
  .verdict.safe { background: color-mix(in srgb, var(--ok) 10%, transparent); color: var(--ok); }
  .verdict.kept { background: color-mix(in srgb, var(--ko) 10%, transparent); color: var(--ko); }
  .verdict.kept.armed { box-shadow: inset 0 0 0 1px var(--ko); }
  .pinned { position: sticky; left: 0; max-width: 100%; }
  .hrow { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .hrow b { flex: 1; min-width: 0; }
  .tools { display: flex; align-items: center; gap: 2px; }
  .tb { display: inline-grid; place-items: center; width: 26px; height: 24px; border: none; border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .tb.txt { display: inline-flex; align-items: center; gap: 5px; width: auto; padding: 0 9px; }
  .tb:hover:not(:disabled) { background: var(--surface-muted); color: var(--text); }
  .tb:disabled { opacity: 0.4; cursor: not-allowed; }
  .tb.primary { margin-left: 4px; border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent); background: color-mix(in srgb, var(--accent) 14%, var(--surface)); color: var(--text); }
  .line { display: inline-flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .pull { display: inline-flex; align-items: center; gap: 3px; height: 20px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .pull:hover:not(:disabled) { border-color: var(--src-gitlab); }
  .pull:disabled { opacity: 0.45; cursor: not-allowed; }
  .acts { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .acts .quiet { border-color: transparent; background: none; color: var(--text-muted); }
  .acts .quiet:hover { background: var(--surface-muted); color: var(--text); border-color: transparent; }
  .acts button, .ft button { display: inline-flex; align-items: center; gap: 5px; padding: 3px 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .acts button:hover:not(:disabled) { border-color: var(--src-gitlab); }
  .acts button:disabled, .ft button:disabled { opacity: 0.45; cursor: not-allowed; }
  .acts .danger { color: var(--ko); }
  .acts .danger:hover:not(:disabled) { border-color: var(--ko); }
  .acts .danger.armed, .sheet .primary.armed { background: var(--ko-bg); border-color: var(--ko); color: var(--ko); }

  .scrim { position: fixed; inset: 0; z-index: 50; display: flex; align-items: flex-start; justify-content: center; padding-top: 12vh; background: rgba(0, 0, 0, 0.45); }
  .sheet { width: min(560px, 92vw); max-height: 70vh; display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--r12); background: var(--surface); box-shadow: var(--shadow-modal); }
  .sheet .hd { display: flex; align-items: center; gap: 8px; padding: 11px 14px; border-bottom: 1px solid var(--border); }
  .sheet .bd { overflow: auto; padding: 8px 10px; display: flex; flex-direction: column; gap: 2px; }
  .sheet label { display: grid; grid-template-columns: 18px minmax(0, 1fr) auto; gap: 8px; align-items: center; padding: 5px 6px; border-radius: var(--r6); font-size: var(--fs-body); }
  .sheet label:hover { background: var(--surface-muted); }
  .sheet label.off { opacity: 0.55; }
  .sheet .lose { color: var(--ko); }
  .sheet .primary.danger { background: var(--ko-bg); border-color: var(--ko); color: var(--ko); }
  .sheet label > span:nth-child(2) { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sheet .ft { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-top: 1px solid var(--border); font-size: var(--fs-meta); }
  .ib { display: inline-flex; padding: 3px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .grow { flex: 1; }
  .muted { color: var(--text-muted); }
  .mono { font-family: var(--mono); }
</style>
