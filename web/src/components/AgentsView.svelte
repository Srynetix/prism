<script lang="ts">
  import { suggest } from '../suggest'
  import { chord, chordKey, extraShift, inTerminal, keys, tildePath } from '../platform'
  import Icon from './Icon.svelte'
  import Terminal from './Terminal.svelte'
  import QuickAnswers from './QuickAnswers.svelte'
  import ActionMenu from './ActionMenu.svelte'
  import AgentSide from './AgentSide.svelte'
  import AgentChat from './AgentChat.svelte'
  import AgentLabels from './AgentLabels.svelte'
  import KindBadge from './KindBadge.svelte'
  import Gutter from './Gutter.svelte'
  import { live } from '../live.svelte'
  import { TERMINAL_FONT, zoomTerminal, zoomChat, agentsAvailable, agentsSettings, agentsView, allowAgent, modelName, desktopUrl, interruptAgent, justLaunched, allTags, renameAgent, startPrepared, launchAgain, openLauncher, resumeAgent, setAgentCategory, setAgentTags, kindNames, kindColor, linkAgentTicket, linkAgentMr, stopAgent, attachFiles, mention, draggedFiles, insertInAgent, filesOf, layoutKey, paneArrow, terminalKeeps, plans, type AgentsLayout, type SideTab } from '../agents.svelte'
  import { pref, setPref } from '../layout.svelte'
  import { filter as query, resolve } from '../filter.svelte'
  import { AGENT_STATE, agentStateLabel, desktopShown, isWorking, SESSION_LABEL, ticketIndex, waitsForYou } from '../rules'
  import { asTicket } from '../conventions'
  import { mrView, openMr } from '../mr-view.svelte'
  import { agentPorts, serverTitle } from '../agent-ports'
  import { openDevServer } from '../dev-preview.svelte'
  import { mrKey, sameMr } from '../mr-ref'
  import { ago } from '../time'
  import { toast } from '../toast.svelte'
  import { cliEffort, effortOf, effortsOf, loadEffortsOf, pickEffort, takesEffort } from '../agent-effort'
  import { canPreview, openTicket, ticketView } from '../ticket-view.svelte'
  import type { Agent } from '../types'
  import { agentCli, cliOf, nameOf } from '../agent-cli'
  import ContextGauge from './ContextGauge.svelte'
  import ContextDetail from './ContextDetail.svelte'
  import AgentUsageChip from './AgentUsage.svelte'
  import AgentChain from './AgentChain.svelte'
  import AgentCliChip from './AgentCliChip.svelte'
  import AgentLimitBanner from './AgentLimitBanner.svelte'
  import AgentLauncher from './AgentLauncher.svelte'
  import { agentHead, agentMenu, openMrOf } from '../menus'
  import { AgentPanes, LAYOUTS, layoutHint, TEMPLATE } from '../agent-panes.svelte'
  import { agentRepo } from '../repositories'
  import { isArchived, setArchived } from '../agent-archive.svelte'
  import { setPollsShown } from '../polls.svelte'
  import { settingsView } from '../settings.svelte'
  import { helpView } from '../help-view.svelte'
  import { agentEntry, desktopEntry, groupEntries, groupLabel, GROUP_BY, shortOf, SORTS, tagsOf, taskOf, TASK_ICON, where, type Entry, type Filter, type GroupBy, type SortBy } from '../agents-list'
  import { t, tn, tc } from '../i18n.svelte'
  import { projectsOf, type Project } from '../projects'
  import { implement, newAgentIn, nextOf, reopenLeft } from '../project-actions'
  import { namedByQuickAction, quickActionsHere, runQuickAction } from '../quick-actions'

  /**
   * Agents view: the agents launched by Prism and the desktop
   * sessions, grouped by status (what is waiting on me first), the chosen agent's terminal on the
   * right, and the "New agent" form. A desktop session has no terminal here:
   * a card opens it in the Claude app.
   */

  // Kept mounted under the other views (App): its panels' polls wait meanwhile.
  setPollsShown(() => agentsView.open && !settingsView.open && !helpView.open)

  const agents = $derived(live.snapshot?.agents ?? [])
  /** Claude Desktop sessions, unless hidden (`[agents.claude] show_desktop`, only with Claude Code). */
  const withDesktop = $derived(desktopShown(live.snapshot?.agents_settings))
  const desktop = $derived(withDesktop ? (live.snapshot?.sessions ?? []).filter((c) => c.origin !== 'agent') : [])
  const entries = $derived<Entry[]>([...agents.map(agentEntry), ...desktop.map(desktopEntry)])

  let filter = $state<Filter>(pref<Filter>('agents.filter', 'all'))
  const setFilter = (f: Filter) => setPref('agents.filter', (filter = f))
  /**
   * The header's filter (a ticket, an MR, a word): the agents and sessions it leads to, through
   * the same ticket ↔ MR ↔ branch links as the other blocks.
   */
  const queried = $derived(live.snapshot ? resolve(live.snapshot, query.query) : null)
  const matching = $derived(queried ? entries.filter((e) => queried.sessions.has(e.id)) : entries)
  /** Only the agents with a tag (`agents.tag`); a tag no agent carries any more no longer filters. */
  let tagFilter = $state(pref<string>('agents.tag', ''))
  const setTagFilter = (tag: string) => setPref('agents.tag', (tagFilter = tag))
  const tagOn = $derived(allTags().includes(tagFilter) ? tagFilter : '')
  /** Archived (`agent-archive.svelte.ts`): out of the list, unless it shows only them (not remembered). */
  const archivedEntry = (e: Entry) => isArchived(e.kind === 'agent' ? e.a : e.c)
  let showArchived = $state(false)
  const archivedCount = $derived(matching.filter(archivedEntry).length)
  // Without the desktop sessions, the filter has nothing to sort: everything is a Prism agent.
  const shown = $derived(matching.filter((e) => archivedEntry(e) === showArchived && (!withDesktop || filter === 'all' || e.kind === filter) && (!tagOn || tagsOf(e).includes(tagOn))))
  const count = (f: Filter) => matching.filter((e) => archivedEntry(e) === showArchived && (f === 'all' || e.kind === f)).length

  /** Beyond this, finished sessions fold behind "Show more". */
  const FINISHED_SHOWN = 8
  let allFinished = $state(false)

  /** Order within each group (`agents-list.ts`), remembered like the filter (`agents.sortBy`). */
  let sortBy = $state<SortBy>(pref<SortBy>('agents.sortBy', 'activity'))
  const setSortBy = (s: SortBy) => setPref('agents.sortBy', (sortBy = s))

  /** Grouping by status, kind, tag or repository (`agents-list.ts`), remembered (`agents.groupBy`; a former one, by stage: by status). */
  let groupBy = $state<GroupBy>(((g) => (GROUP_BY.some(([x]) => x === g) ? g : 'status'))(pref<GroupBy>('agents.groupBy', 'status')))
  const setGroupBy = (g: GroupBy) => setPref('agents.groupBy', (groupBy = g))
  const categories = $derived(kindNames())
  let launcher: ReturnType<typeof AgentLauncher> | undefined = $state()
  /** An action on the shown agent failed. */
  let error = $state('')
  /** The configured repositories, in their order: the groups by repository. */
  const repos = $derived((live.snapshot?.repositories ?? []).map((r) => r.name))
  const repoOf = (e: Entry) => agentRepo(e.kind === 'agent' ? e.a : { cwd: e.c.folder, worktree: e.c.worktree }, live.snapshot)
  const groups = $derived(groupEntries(shown, { groupBy, sortBy, categories, tags: allTags(), repos, repoOf }))
  /**
   * By repository, each one of the config as a project (`projects.ts`): its agents, or where it was
   * left and its next ticket, a + for a new agent in it; those resting folded. Not while the list
   * is narrowed (a tag, the archived, Desktop only, the header's filter): only what it finds then.
   */
  const projects = $derived(groupBy === 'repo' && !tagOn && !showArchived && filter !== 'desktop' && !queried ? projectsOf({ groups: groups.filter((g) => repos.includes(g.name)), repos, left: live.snapshot?.left ?? [], next: nextOf, now: Date.now(), kept: namedByQuickAction }) : null)
  /** Beside the projects: agents outside every repository, Claude Desktop's. */
  const others = $derived(projects ? groups.filter((g) => !repos.includes(g.name)) : groups)
  let showResting = $state(false)

  /**
   * Groups folded in the list (`agents.folded`), per grouping: `category:Review`, `tag:#api`…
   * A folded group keeps its count, and a dot when one of its agents waits for me.
   */
  let folded = $state<string[]>(pref<string[]>('agents.folded', []))
  const foldKey = (name: string) => `${groupBy}:${name}`
  const isFolded = (name: string) => folded.includes(foldKey(name))
  function toggleFold(name: string) {
    const k = foldKey(name)
    setPref('agents.folded', (folded = folded.includes(k) ? folded.filter((x) => x !== k) : [...folded, k]))
  }

  const current = $derived(entries.find((e) => e.id === agentsView.selected) ?? null)
  /** With no choice, or if the chosen item is gone: the first in the list (not an agent still on its way). */
  $effect(() => {
    const first = groups[0]?.items[0]
    if (!agentsView.creating && !current && first && !justLaunched(agentsView.selected)) agentsView.selected = first.id
  })

  /** Files dragged over a running agent's terminal: dropped, saved in its folder, their paths typed in its prompt. */
  let dropping = $state(false)
  async function dropOnAgent(e: DragEvent) {
    dropping = false
    const a = shownAgent
    const files = filesOf(e.dataTransfer?.files)
    if (!a || a.state !== 'running' || !files.length) return
    e.preventDefault()
    e.stopPropagation()
    try {
      const paths = await attachFiles(files, a.id)
      await insertInAgent(a.id, ` ${paths.map(mention).join(' ')} `)
      toast(tn(paths.length, '{n} file attached to its prompt (not sent)', '{n} files attached to its prompt (not sent)'))
    } catch (err) {
      error = String(err)
    }
  }

  function select(id: string) {
    agentsView.creating = false
    agentsView.selected = id
    error = ''
  }
  /** The list gives way to Manage: the kinds and tags of `[agents]`. */
  let managing = $state(false)
  /** A tag typed (or picked among the suggestions) in the agent's header. */
  function addTag(a: Agent, input: HTMLInputElement) {
    const tag = input.value.trim()
    input.value = ''
    const tags = a.tags ?? []
    if (tag && !tags.includes(tag)) void act(() => setAgentTags(a.id, [...tags, tag]))
  }
  /**
   * A ticket (`ABC-123`) or an MR (`!123`, `helm!214`, `#123` on GitHub) linked by hand: it replaces
   * the one found in the agent's title or branch, until unlinked.
   */
  const MR_REF = /^([\w.-]+)?[!#](\d+)$/
  function addLink(a: Agent, input: HTMLInputElement) {
    const text = input.value.trim()
    input.value = ''
    if (!text) return
    const mr = text.match(MR_REF)
    const ticket = asTicket(text)
    if (mr) void act(() => linkAgentMr(a.id, { repo: mr[1] ?? null, iid: Number(mr[2]) }))
    else if (ticket) void act(() => linkAgentTicket(a.id, ticket))
    else error = t('“{text}” is neither a ticket id nor an MR ({example})', { text, example: mrKey({ iid: 123 }) })
  }
  /** What the link field suggests: the tickets Prism knows, and the open MRs. */
  const linkChoices = $derived.by(() => {
    const s = live.snapshot
    if (!s) return []
    const tickets = [...ticketIndex(s).values()].filter(({ t: tk }) => !tk.closed).map(({ t: tk }) => ({ value: tk.id, label: tk.name }))
    const mrs = s.merge_requests.filter((m) => m.state === 'opened').map((m) => ({ value: mrKey(m), label: m.title }))
    return [...new Map([...tickets, ...mrs].map((c) => [c.value, c])).values()]
  })
  async function act(f: () => Promise<unknown>) {
    error = ''
    try {
      await f()
    } catch (e) {
      error = String(e)
    }
  }
  /** Picked in its header: Claude Code at its prompt takes it at once, otherwise from its next resume. */
  const chooseEffort = (a: Agent, effort: string) => act(async () => toast(await pickEffort(a, effort || null), 5000))
  /** The agent's state in words: Claude's while it runs, otherwise why it stopped. */
  const SIDE_SHORT: Record<SideTab, string> = { files: t('Files'), diff: t('Diff'), web: t('Web'), tasks: t('Tasks'), shell: t('Shell'), activity: t('Activity'), launch: tc('tab', 'Launch'), notes: t('Notes'), plan: t('Plan') }


  /**
   * ⌘[ / ⌘]: previous / next agent in the list; ⌘.: interrupt Claude (Escape);
   * ⌘+ / ⌘- / ⌘0: terminal text size.
   */
  function onKey(e: KeyboardEvent) {
    // Kept by the terminal it was pressed in (`[agents] terminal_keys`).
    if (inTerminal(e) && terminalKeeps(e)) return
    if (agentsView.open && managing && e.key === 'Escape' && !(e.target as HTMLElement | null)?.closest?.('textarea, .xterm')) {
      e.preventDefault()
      managing = false
      return
    }
    // Esc restores a full-width panel, unless it is meant for a field (or the terminal, hidden then).
    if (agentsView.open && full && e.key === 'Escape' && !(e.target as HTMLElement | null)?.closest?.('input, textarea, select, .xterm')) {
      e.preventDefault()
      setFull(false)
      return
    }
    if (agentsView.open) {
      const l = layoutKey(e)
      if (l) {
        e.preventDefault()
        grid.setLayout(l)
        return
      }
      const arrow = paneArrow(e)
      if (arrow) {
        e.preventDefault()
        grid.move(arrow)
        return
      }
    }
    if (!agentsView.open || !chord(e) || e.altKey) return
    // Shift added only to leave the terminal (Linux, Windows) does not count.
    const shift = e.shiftKey && !extraShift(e)
    const k = chordKey(e)
    const zoom = k === '=' || k === '+' ? TERMINAL_FONT.step * 2 : k === '-' || k === '_' ? -TERMINAL_FONT.step * 2 : k === '0' && !shift ? 0 : null
    if (zoom !== null) {
      e.preventDefault()
      // The agent shown reads as its Chat: its text; otherwise the terminals'.
      if (current?.kind === 'agent' && chatting(current.a.id)) zoomChat(zoom)
      else zoomTerminal(zoom)
      return
    }
    if (shift) return
    if (k === 'enter' && grid.count > 1 && current?.kind === 'agent') {
      e.preventDefault()
      grid.zoomed = !grid.zoomed
      return
    }
    if (k === 'b') {
      e.preventDefault()
      toggleCollapsed()
      return
    }
    if (k === '[' || k === ']') {
      // Grouped by tag, an agent may show in several groups: once in the order.
      const order = groups.filter((g) => !isFolded(g.name)).flatMap((g) => g.items).filter((x, i, all) => all.findIndex((y) => y.id === x.id) === i)
      const i = order.findIndex((x) => x.id === current?.id)
      const next = order[(i + (k === ']' ? 1 : -1) + order.length) % order.length]
      if (!next) return
      e.preventDefault()
      select(next.id)
    } else if (k === '.' && current?.kind === 'agent' && current.a.state === 'running') {
      e.preventDefault()
      void act(() => interruptAgent(current!.id))
    }
  }

  /** Rename: double-click the title or the pencil; ↩ or leaving the field saves, Escape cancels. */
  let renaming = $state<string | null>(null)
  let newTitle = $state('')
  function startRename(a: Agent) {
    renaming = a.id
    newTitle = a.title
  }
  function endRename(a: Agent, save: boolean) {
    if (renaming !== a.id) return
    renaming = null
    const next = newTitle.trim()
    if (save && next && next !== a.title) void act(() => renameAgent(a.id, next))
  }
  const focus = (el: HTMLInputElement) => {
    el.focus()
    el.select()
  }

  /**
   * Right panel of an agent (files, diff, Web view, tasks, shell). Which one is open, and its width
   * (set with the handle, double-click to reset it), are each agent's: see `sides`.
   */
  const SIDE_W = 520
  const SIDE: [SideTab, string, string][] = [
    ['files', 'files', t('Files of its folder')],
    ['diff', 'diff', t('Git diff of its folder')],
    ['web', 'web', t('Web view: a page of its folder, or a local server')],
    ['tasks', 'play', t('Tasks of its folder: justfile, Taskfile, package.json scripts')],
    ['shell', 'terminal', t('A shell in its folder')],
    ['activity', 'activity', t('What it did: files read and written, commands, waits')],
    ['launch', 'rocket', t('What it was launched with, and Launch again')],
    ['notes', 'pin', t('The notes of its repository, each sent to it in a click')],
    ['plan', 'queue', t('Its plan (plan mode): read, commented on passage by passage, answered')],
  ]
  /** Its panels: Plan only once it plans (or the one shown). */
  const sideOf = (a: Agent, shown: string | null) => SIDE.filter(([tb]) => tb !== 'plan' || shown === 'plan' || plans(a))

  /**
   * Mounted terminals: the shown agent's and those of the last ones seen. Attaching a terminal
   * resizes the tmux window, and Claude gains an empty line in its prompt each time:
   * switching from one agent to another must therefore neither detach nor attach. Beyond `KEEP`, the least
   * recent is detached; so is a closed agent, or one whose tmux session is gone.
   */
  const KEEP = 8
  const shownAgent = $derived(!agentsView.creating && current?.kind === 'agent' ? current.a : null)
  /**
   * The panel open for each agent, by id (absent: none): opening the Web view for one leaves the
   * others as they were, and coming back to an agent finds the panel it had. Closed agents leave.
   */
  let sides = $state(pref<Record<string, SideTab>>('agents.sides', {}))
  const side = $derived(shownAgent ? (sides[shownAgent.id] ?? null) : null)
  function setSide(tab: SideTab | null) {
    if (!shownAgent) return
    const { [shownAgent.id]: _, ...others } = sides
    sides = tab ? { ...others, [shownAgent.id]: tab } : others
    setPref('agents.sides', sides)
    if (!tab) setFull(false)
  }
  // A panel asked from outside (a block's "Show"): opened once the agent is the one shown.
  $effect(() => {
    if (agentsView.side && shownAgent && shownAgent.id === agentsView.selected) {
      setSide(agentsView.side)
      agentsView.side = null
    }
  })
  /** Each agent's panel width; an agent never resized takes the last width set. */
  let sideWidths = $state(pref<Record<string, number>>('agents.side.widths', {}))
  let lastWidth = $state(pref<number>('agents.side.width', SIDE_W))
  const sideW = $derived((shownAgent && sideWidths[shownAgent.id]) || lastWidth)
  function setWidth(w: number) {
    lastWidth = w
    setPref('agents.side.width', w)
    if (!shownAgent) return
    sideWidths = { ...sideWidths, [shownAgent.id]: w }
    setPref('agents.side.widths', sideWidths)
  }
  const resizeSide = (delta: number, [left, right]: [number, number]) => setWidth(Math.round(Math.max(280, Math.min(left + right - 320, right - delta))))
  /** The panel over the whole width, for a while (never remembered): the terminal stays attached, at its size. */
  let fullSides = $state<Record<string, boolean>>({})
  const full = $derived(!!shownAgent && !!side && !!fullSides[shownAgent.id])
  function setFull(on: boolean) {
    if (shownAgent) fullSides = { ...fullSides, [shownAgent.id]: on }
  }
  $effect(() => {
    // An empty list may only mean the agents are not read yet (launch): nothing is forgotten then.
    if (!agents.length) return
    const ids = new Set(agents.map((a) => a.id))
    const keep = <T,>(m: Record<string, T>) => Object.fromEntries(Object.entries(m).filter(([id]) => ids.has(id)))
    if (Object.keys(sides).some((id) => !ids.has(id))) setPref('agents.sides', (sides = keep(sides)))
    if (Object.keys(views).some((id) => !ids.has(id))) setPref('agents.views', (views = keep(views)))
    if (Object.keys(sideWidths).some((id) => !ids.has(id))) setPref('agents.side.widths', (sideWidths = keep(sideWidths)))
  })
  let seen = $state<string[]>([])
  $effect(() => {
    const id = shownAgent?.id
    if (id && seen[0] !== id) seen = [id, ...seen.filter((x) => x !== id)].slice(0, KEEP)
  })
  /** The type of an agent dragged from the list onto a pane. */
  const PANE_DRAG = 'application/x-prism-agent'
  const live_ = (id: string) => agents.some((a) => a.id === id && a.state !== 'gone' && a.state !== 'prepared')

  /** Agents side by side (`agent-panes.svelte.ts`): 1, 2 or 4 panes, the focused one the selected agent's. */
  const grid = new AgentPanes(() => agents, select)
  const split = $derived(grid.count > 1 && !grid.zoomed && !agentsView.creating && current?.kind === 'agent')
  $effect(() => {
    agentsView.split = split
  })
  /** A click on a pane's control (its panel picker, a button): the focus stays, or the terminal would take the keyboard from it. */
  const onControl = (ev: PointerEvent) => !!(ev.target as HTMLElement | null)?.closest?.('button, select, input, a, [role="menu"]')
  /** The agents of the panes shown, and the last ones seen: their terminals stay mounted. */
  const mounted = $derived([...new Set([...(grid.count > 1 ? grid.visible.filter((id): id is string => !!id) : []), ...seen])].filter(live_))
  const agentOf = (id: string | null) => (id ? agents.find((a) => a.id === id) : undefined)
  /** A pane's panel, in place of its terminal: the agent's own (see `sides`). */
  function setSideOf(id: string, tab: SideTab | null) {
    const { [id]: _, ...others } = sides
    sides = tab ? { ...others, [id]: tab } : others
    setPref('agents.sides', sides)
  }

  /**
   * An agent read as a Chat rather than its terminal (Claude Code, Codex: those with a transcript):
   * its conversation, its questions as cards, a box to write to it. `[agents] view` by default,
   * each agent's own choice remembered (`agents.views`) when it differs; its terminal stays
   * attached under it.
   */
  let views = $state(pref<Record<string, 'chat' | 'terminal'>>('agents.views', {}))
  const defaultView = $derived(agentsSettings().view === 'terminal' ? 'terminal' : 'chat')
  /** `[agents] header_meta`: its header's second line read, its fields behind Edit (for a while). */
  const readMeta = $derived(agentsSettings().header_meta !== 'edit')
  let editMeta = $state(false)
  const canChat = (a: Agent | undefined) => !!a && cliOf(a.provider).conversation
  const chatting = (id: string) => (views[id] ?? defaultView) === 'chat' && canChat(agentOf(id))
  function setChat(id: string, on: boolean) {
    const { [id]: _, ...others } = views
    const v = on ? 'chat' : 'terminal'
    views = v === defaultView ? others : { ...others, [id]: v }
    setPref('agents.views', views)
  }
  /** A pane's choice: its terminal, its Chat, or one of its panels. */
  function setPaneView(id: string, v: string) {
    setChat(id, v === 'chat')
    setSideOf(id, v && v !== 'chat' ? (v as SideTab) : null)
  }

  /**
   * List collapsed into a rail: one pill per agent,
   * its task's icon above its ticket or MR number, its status in the corner, its title
   * on hover. ‹ / › or ⌘B; state remembered (`agents.collapsed`).
   */
  let collapsed = $state(pref<boolean>('agents.collapsed', false))
  const toggleCollapsed = () => setPref('agents.collapsed', (collapsed = !collapsed))

  /** A prepared agent starts: its launch as kept, its Then steps with it. */
  async function start(a: Agent) {
    await startPrepared(a.id)
  }
  /** A prepared agent back in the launcher: launching or preparing it again replaces it. */
  function edit(a: Agent) {
    openLauncher(launchAgain(a))
    agentsView.sameAs = a.title
    agentsView.replacing = a.id
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- Its open ports (the Dev servers block's, for this agent): a click opens it. -->
{#snippet ports(a: Agent)}
  {#each agentPorts(a.id) as { port, server } (port)}<button class="link port" onclick={() => openDevServer(port)} title={t('Open http://localhost:{port} · {server}', { port, server: serverTitle(server) })}><Icon name="external" size={11} />:{port}</button>{/each}
{/snippet}

{#snippet layoutIcon(l: AgentsLayout)}
  <span class="lic l{l}" aria-hidden="true">{#each { length: l === '4' ? 4 : l === '1' ? 1 : 2 } as _, n (n)}<i></i>{/each}</span>
{/snippet}

<!-- A pane's header, side by side: the agent, the one action of the moment, its panel, zoom, its menu. -->
{#snippet cellHead(a: Agent, i: number, tab: SideTab | null)}
  <div class="chead">
    <span class="n">{i + 1}</span>
    <span class="st {a.status}"></span>
    <!-- svelte-ignore a11y_no_static_element_interactions : ⌘↵ is the keyboard path -->
    <b class="title" ondblclick={() => { grid.focus(i); grid.zoomed = true }} title="{a.title} · {t('double-click: the whole view ({key})', { key: keys('⌘↵') })}">{a.title}</b>
    <span class="pill {waitsForYou(a.status) ? 'warn' : isWorking(a.status) ? 'info' : ''}" class:working={isWorking(a.status)} title={a.needs_action ?? undefined}>{#if isWorking(a.status)}<span class="spin-ascii" aria-hidden="true"></span>{/if}{agentStateLabel(a)}</span>
    <QuickAnswers agent={a} compact />
    {#if a.status === 'waiting_tool'}
      <button class="allow" onclick={() => act(() => allowAgent(a.id))} title="{t('Answers {answer}', { answer: cliOf(a.provider).allow })}{a.needs_action ? `: ${a.needs_action}` : ''}"><Icon name="check" />{t('Allow')}</button>
    {:else if a.state === 'prepared'}
      <button class="primary" onclick={() => act(() => start(a))}><Icon name="play" />{t('Start')}</button>
    {:else if a.state !== 'running'}
      <button class="primary" onclick={() => act(() => resumeAgent(a.id))}><Icon name="retry" />{t('Resume')}</button>
    {/if}
    <span class="grow"></span>
    <select class="ctab" value={tab ?? (chatting(a.id) ? 'chat' : '')} onchange={(ev) => setPaneView(a.id, ev.currentTarget.value)} aria-label={t('Shown in this pane')} title={t('Its terminal, its Chat, or one of its panels')}>
      <option value="">{t('Terminal')}</option>
      {#if canChat(a)}<option value="chat">Chat</option>{/if}
      {#each sideOf(a, tab) as [tb, , label] (tb)}<option value={tb} title={label}>{SIDE_SHORT[tb]}</option>{/each}
    </select>
    <button class="quiet icon" onclick={() => { grid.focus(i); grid.zoomed = true }} aria-label={t('The whole view')} title={keys(t('The whole view, for a while (⌘↵)'))}><Icon name="expand" /></button>
    <ActionMenu sections={agentMenu(a, () => startRename(a))} head={agentHead(a)} label={t('All actions on {name}', { name: a.title })} />
    {#if grid.visible.some((x, j) => x && j !== i)}<button class="quiet icon" onclick={() => grid.close(i)} aria-label={t('Empty this pane')} title={t('Empty this pane (the agent goes on)')}><Icon name="close" /></button>{/if}
  </div>
{/snippet}

{#snippet group(g: { name: string; items: Entry[] }, inProject = false)}
        {@const finished = groupBy === 'status' && g.name === 'Finished'}
        {@const items = finished && !allFinished ? g.items.slice(0, FINISHED_SHOWN) : g.items}
        {@const fold = isFolded(g.name)}
        <div class="gh">
          <button class="grp" onclick={() => toggleFold(g.name)} aria-expanded={!fold} title={fold ? t('Show {name}', { name: groupLabel(g.name) }) : t('Fold {name}', { name: groupLabel(g.name) })}>
            <Icon name={fold ? 'chevronRight' : 'chevronDown'} size={11} />{groupLabel(g.name)}
            {#if fold && g.items.some((e) => waitsForYou(e.status))}<i class="wait" title={t('An agent of this group waits for you')}></i>{/if}
            <span>{g.items.length}</span>
          </button>
          {#if inProject}{@render plus(g.name)}{/if}
        </div>
        {#each fold ? [] : items as e (e.id)}
          {@const at = grid.count > 1 ? grid.visible.indexOf(e.id) : -1}
          <button class="ag {e.kind}" class:on={!agentsView.creating && e.id === current?.id} class:shown={at >= 0} onclick={(ev) => grid.pick(ev, e.id)} draggable={e.kind === 'agent' && grid.count > 1} ondragstart={(ev) => ev.dataTransfer?.setData(PANE_DRAG, e.id)} title={e.kind === 'desktop' ? t('Claude Desktop session') : grid.count > 1 ? keys(t('⌘-click or drag: into another pane')) : undefined}>
            <span class="st {e.status}"></span>
            <span class="t">{#if at >= 0}<span class="slot" title={t('In pane {n}', { n: at + 1 })}>{at + 1}</span>{/if}{e.title}</span>
            <span class="tm">{ago(e.when)}</span>
            {#if e.kind === 'agent' && waitsForYou(e.status)}
              <span class="why">{e.status === 'waiting_tool' ? '⚠' : '?'} {e.a.needs_action ?? (e.status === 'needs_you' ? t('Your turn') : t('Permission requested'))}</span>
            {:else if e.kind === 'desktop' && waitsForYou(e.status) && e.c.needs_action}
              <span class="why">? {e.c.needs_action}</span>
            {/if}
            <span class="m">
              {#if e.kind === 'desktop'}<span class="pill dk">Desktop</span>{#if e.c.group}<span>{e.c.group}</span>{/if}{/if}
              {#if e.kind === 'agent' && e.a.category && groupBy !== 'category'}<KindBadge name={e.a.category} />{/if}
              {#if groupBy !== 'tag'}{#each tagsOf(e) as tag (tag)}<span class="pill tag">#{tag}</span>{/each}{/if}
              {#if e.kind === 'agent' && e.a.looping}<span class="pill loop" title={t('{command} failed {n} times in a row: going in circles? (Activity tab)', { command: e.a.looping.command, n: e.a.looping.count })}>↻ ×{e.a.looping.count}</span>{/if}
              {#if e.kind === 'agent' && e.a.state !== 'running'}<span class="pill">{AGENT_STATE[e.a.state]}</span>{/if}
              {#if e.kind === 'agent' && e.a.worktree}<span class="pill wt">{t('worktree')}</span>{/if}
              {#if e.kind === 'agent' && e.a.model}<span class="pill model" title={t('The model answering: {model}', { model: e.a.model })}>{modelName(e.a.model)}</span>{/if}
              <span class="br">{where(e)}</span>
              {#if e.kind === 'agent'}{#each agentPorts(e.a.id) as { port, server } (port)}<span class="pill port" title={t('Listens on :{port} · {server} · open it from its header', { port, server: serverTitle(server) })}>:{port}</span>{/each}{/if}
              {#if e.kind === 'agent' && (e.a.context || e.a.usage)}<span class="use">{#if e.a.context}<ContextGauge context={e.a.context} compact />{/if}{#if e.a.usage}<AgentUsageChip usage={e.a.usage} provider={e.a.provider} compact />{/if}</span>{/if}
            </span>
          </button>
        {/each}
        {#if finished && !fold && g.items.length > FINISHED_SHOWN}
          <button class="more" onclick={() => (allFinished = !allFinished)}>{allFinished ? t('Show less') : t('Show {n} more', { n: g.items.length - FINISHED_SHOWN })}</button>
        {/if}
{/snippet}

{#snippet plus(repo: string)}
  <button class="padd" onclick={() => newAgentIn(repo)} disabled={!agentsAvailable()} title={t('New agent in {repo}', { repo })} aria-label={t('New agent in {repo}', { repo })}><Icon name="plus" size={11} stroke={2.5} /></button>
{/snippet}

{#snippet project(p: Project)}
  {#if p.group}
    {@render group(p.group, true)}
  {:else}
    <div class="gh"><span class="grp idle">{p.name}</span>{@render plus(p.name)}</div>
  {/if}
  {#if p.left}
    <div class="left">
      <span>{t('Left {ago} ago', { ago: ago(p.left.closed_at) })}{#if p.left.branch}{' '}· <code>{p.left.branch}</code>{/if} · {p.left.title}</span>
      {#if p.left.said}<q>{p.left.said}</q>{/if}
      {#if p.left.resumable}<button onclick={() => act(() => reopenLeft(p.name))} disabled={!agentsAvailable()} title={t('Back in the list, on its conversation ({cli})', { cli: cliOf(p.left.provider).name })}><Icon name="retry" size={11} />{t('Reopen')}</button>{/if}
    </div>
  {/if}
  {@const quick = quickActionsHere(p.name)}
  {#if quick.length}
    <div class="quick">{#each quick as a, i (i)}<button onclick={(ev) => act(() => runQuickAction(a, p.name, ev.shiftKey))} disabled={!agentsAvailable()} title={`${a.prompt}\n${keys(t('⇧-click: change it before it goes'))}`}><Icon name="quick" size={10} />{a.name}</button>{/each}</div>
  {/if}
  {#if p.next}
    <div class="next"><span class="muted">{tc('project', 'Next')}</span><button class="tk" onclick={() => openTicket(p.next!.id)} title={p.next.name}>{p.next.id}</button><span class="nm">{p.next.name}</span><button class="go" onclick={() => implement(p.next!)} title={t('Implement {ticket}', { ticket: p.next.id })} aria-label={t('Implement {ticket}', { ticket: p.next.id })}><Icon name="play" size={10} /></button></div>
  {/if}
{/snippet}

<section class="agents" class:collapsed>
  {#if collapsed}
    <aside class="rail" aria-label={t('Agents')}>
      <button class="ctl" onclick={toggleCollapsed} title={keys(t('Expand the list (⌘B)'))} aria-label={t('Expand the list')}><Icon name="chevronRight" /></button>
      <button class="ctl plus" onclick={() => { agentsView.creating = true; agentsView.desktopHref = null; agentsView.onLaunch = null; launcher?.fresh() }} disabled={!agentsAvailable()} title={keys(t('New agent (⌘N)'))} aria-label={t('New agent')}><Icon name="plus" stroke={2.5} /></button>
      <button class="ctl" onclick={() => grid.setLayout(LAYOUTS[(LAYOUTS.findIndex(([l]) => l === grid.layout) + 1) % LAYOUTS.length][0])} title={t('Layout: {name} · click for the next', { name: LAYOUTS.find(([l]) => l === grid.layout)?.[1] ?? '' })} aria-label={t('Next layout')}>{@render layoutIcon(grid.layout)}</button>
      <div class="rscroll">
        {#each groups as g (g.name)}
          <span class="sep" title={groupLabel(g.name)}></span>
          {#each g.items as e (e.id)}
            {@const task = taskOf(e)}
            <button class="it t-{task} {e.kind}" class:on={!agentsView.creating && e.id === current?.id} onclick={(ev) => grid.pick(ev, e.id)} title="{e.title}{e.kind === 'agent' && waitsForYou(e.status) ? ` — ${e.a.needs_action ?? (e.status === 'needs_you' ? t('Your turn') : t('Permission requested'))}` : ''}{e.kind === 'desktop' ? ' (Claude Desktop)' : ''}" aria-label={e.title}>
              <Icon name={TASK_ICON[task]} size={15} />
              <b>{shortOf(e)}</b>
              <span class="st {e.status}"></span>
            </button>
          {/each}
        {/each}
      </div>
    </aside>
  {:else}
  <aside class="list">
    <div class="top">
      <span class="layouts" role="group" aria-label={t('Agents side by side')}>
        {#each LAYOUTS as [l, label, k] (l)}
          <button class:on={grid.layout === l} onclick={() => grid.setLayout(l)} title="{label}{k ? ` (${layoutHint(k)})` : ''}" aria-pressed={grid.layout === l} aria-label={label}>{@render layoutIcon(l)}</button>
        {/each}
      </span>
      {#if grid.count > 1}<button class="by" class:on={grid.follow} onclick={() => grid.setFollow(!grid.follow)} aria-pressed={grid.follow} title={t('Follow who waits: an agent waiting for you comes into an empty pane (or the last), and leaves it once answered')} aria-label={t('Follow who waits')}><Icon name="alarm" /></button>{/if}
      <span class="grow"></span>
      <button class="fold" onclick={toggleCollapsed} title={keys(t('Collapse the list (⌘B)'))} aria-label={t('Collapse the list')}><Icon name="chevronLeft" /></button>
      <button class="new" onclick={() => { agentsView.creating = true; agentsView.desktopHref = null; agentsView.onLaunch = null; launcher?.fresh() }} disabled={!agentsAvailable()} title={keys(t('New agent (⌘N)'))}><Icon name="plus" size={13} stroke={2.5} />{t('New')}<kbd>{keys('⌘N')}</kbd></button>
    </div>
    <div class="filter" role="tablist">
      {#if withDesktop}
        {#each [['all', t('All')], ['agent', t('Prism agents')], ['desktop', 'Desktop']] as [f, label] (f)}
          <button role="tab" class:on={filter === f} aria-selected={filter === f} onclick={() => setFilter(f as Filter)}>{label} {count(f as Filter)}</button>
        {/each}
      {/if}
      <span class="grow"></span>
      {#if archivedCount || showArchived}<button class="by" class:on={showArchived} onclick={() => (showArchived = !showArchived)} aria-pressed={showArchived} title={showArchived ? t('Back to the list') : t('The agents and sessions archived: out of the list until they wait for you')}><Icon name="hide" />{archivedCount}</button>{/if}
      {#if allTags().length}
        <select class="sort" class:on={!!tagOn} value={tagOn} onchange={(e) => setTagFilter(e.currentTarget.value)} title={t('Only the agents with this tag')} aria-label={t('Filter by tag')}>
          <option value="">{t('All tags')}</option>
          {#each allTags() as tag (tag)}<option value={tag}>#{tag}</option>{/each}
        </select>
      {/if}
      <select class="sort" value={sortBy} onchange={(e) => setSortBy(e.currentTarget.value as SortBy)} title={t('Order within each group')} aria-label={t('Sort agents by')}>
        {#each SORTS as [s, label] (s)}<option value={s}>{label}</option>{/each}
      </select>
      <select class="sort" class:on={groupBy !== 'status'} value={groupBy} onchange={(e) => setGroupBy(e.currentTarget.value as GroupBy)} title={t('Groups of the list')} aria-label={t('Group agents by')}>
        {#each GROUP_BY as [g, label] (g)}<option value={g}>{t('By {what}', { what: t(label).toLowerCase() })}</option>{/each}
      </select>
      <button class="by" class:on={managing} onclick={() => (managing = !managing)} aria-pressed={managing} title={t('Kinds and tags: rename, reorder, add, remove')} aria-label={t('Manage kinds and tags')}><Icon name="settings" /></button>
    </div>
    {#if managing}
      <AgentLabels onclose={() => (managing = false)} />
    {:else}
    <div class="scroll">
      {#if projects}
        {#each projects.active as p (p.name)}{@render project(p)}{/each}
        {#each others as g (g.name)}{@render group(g)}{/each}
        {#if projects.resting.length}
          <button class="grp" onclick={() => (showResting = !showResting)} aria-expanded={showResting}><Icon name={showResting ? 'chevronDown' : 'chevronRight'} size={11} />{t('Resting')}<span>{projects.resting.length}</span></button>
          {#if showResting}{#each projects.resting as p (p.name)}{@render project(p)}{/each}{:else}<button class="rest" onclick={() => (showResting = true)}>{projects.resting.map((p) => p.name).join(', ')}</button>{/if}
        {/if}
      {:else}
      {#each groups as g (g.name)}
        {@render group(g)}
      {:else}
        <p class="muted empty">{#if showArchived}{t('Nothing archived any more.')}{:else if tagOn}{t('No agent tagged #{tag}.', { tag: tagOn })}{:else}{t('No agent yet.')} <b>{t('New')}</b> {t('starts {cli} in a terminal that keeps running when Prism quits.', { cli: agentCli() })}{/if}</p>
      {/each}
      {/if}
    </div>
    {/if}
  </aside>
  {/if}

  <section class="pane">
    <div class="launcher" hidden={!(agentsView.creating || !current)}><AgentLauncher bind:this={launcher} kinds={categories} {withDesktop} canCancel={entries.length > 0} /></div>
    {#if agentsView.creating || !current}
      <!-- The launcher, above: mounted once, what is typed in it stays. -->
    {:else if current.kind === 'desktop'}
      {@const c = current.c}
      {@const hidden = isArchived(c)}
      <header>
        <span class="st {c.status}"></span>
        <b class="title">{c.title}</b>
        <span class="pill {waitsForYou(c.status) ? 'warn' : ''}">{SESSION_LABEL[c.status]}</span>
        <span class="pill dk">Claude Desktop</span>
        {#if c.group}<span class="muted">{c.group}</span>{/if}
        <span class="path">{c.branch ?? c.folder}</span>
        <button class="quiet" onclick={() => setArchived(c, !hidden)} title={hidden ? t('Back in the list') : t('Out of the list until it waits for you; Desktop keeps it as it is')}><Icon name={hidden ? 'show' : 'hide'} />{hidden ? t('Unarchive') : t('Archive')}</button>
      </header>
      <div class="deskpane">
        <div class="deskcard">
          <div class="hd"><span class="ic"><Icon name="external" /></span><div><b>{t('This session runs in Claude Desktop')}</b><p>{t('Prism reads its status from disk, never its conversation.')}</p></div></div>
          {#if c.needs_action}<div class="ask">? {c.needs_action}</div>{/if}
          {#if desktopUrl(c.id)}<a class="open" href={desktopUrl(c.id)}>{t('Open in Claude Desktop')}</a>{/if}
          <p>{t('Its worktree belongs to Desktop: the Worktrees view removes it once the session is archived.')}</p>
        </div>
      </div>
    {:else if !split}
      {@const a = current.a}
      {@const agentMr = openMrOf(a)}
      <header class="two">
        <div class="row">
          <span class="st {a.status}"></span>
          {#if renaming === a.id}
            <input class="rename" bind:value={newTitle} use:focus spellcheck="false" onblur={() => endRename(a, true)} onkeydown={(e) => { if (e.key === 'Enter') endRename(a, true); else if (e.key === 'Escape') { e.stopPropagation(); endRename(a, false) } }} />
          {:else}
            <!-- svelte-ignore a11y_no_static_element_interactions : ⋯ › Rename is the keyboard path -->
            <b class="title" ondblclick={() => startRename(a)} title="{a.title} · {t('double-click to rename')}">{a.title}</b>
          {/if}
          <span class="pill {waitsForYou(a.status) ? 'warn' : isWorking(a.status) ? 'info' : ''}" class:working={isWorking(a.status)} title={a.needs_action ?? undefined}>{#if isWorking(a.status)}<span class="spin-ascii" aria-hidden="true"></span>{/if}{agentStateLabel(a)}</span>
          <!-- The one action of the moment: answer Claude, or bring a stopped agent back. -->
          {#if a.status === 'waiting_tool' && !chatting(a.id)}
            <button class="allow" onclick={() => act(() => allowAgent(a.id))} title="{t('Answers {answer} to {agent}’s permission prompt', { answer: cliOf(a.provider).allow, agent: nameOf(a) })}{a.needs_action ? `: ${a.needs_action}` : ''}"><Icon name="check" />{t('Allow once')}</button>
          {:else if a.state === 'prepared'}
            <button class="primary" onclick={() => act(() => start(a))} title={t('Start it now, as prepared (its worktree is made then)')}><Icon name="play" />{t('Start')}</button>
            <button class="quiet" onclick={() => edit(a)} title={t('Change its launch in the launcher')}><Icon name="edit" />{t('Edit…')}</button>
          {:else if a.state !== 'running'}
            <button class="primary" onclick={() => act(() => resumeAgent(a.id))} title={t('{command}, same folder', { command: cliOf(a.provider).resume })}><Icon name="retry" />{t('Resume')}</button>
          {/if}
          <AgentCliChip {a} />
          {#if takesEffort(a)}
            {@const cli = cliEffort(a)}
            <label class="chip effort" title="{t('Its reasoning effort, kept on resume: higher thinks longer and uses your limits faster')}{a.provider === 'codex' ? t('. Codex takes it from its next resume') : ''}">
              <span class="k">{t('Effort')}</span>
              <select value={effortOf(a) ?? ''} onchange={(e) => chooseEffort(a, e.currentTarget.value)} {@attach () => void loadEffortsOf(a)}>
                <option value="">{cli ? t('{value} (Settings)', { value: cli }) : t('default')}</option>
                {#each effortsOf(a) as l (l)}<option value={l}>{l}</option>{/each}
              </select>
            </label>
          {/if}
          <span class="grow"></span>
          {#if canChat(a)}
            <span class="seg" role="group" aria-label={t('Terminal or Chat')}>
              <button class:on={!chatting(a.id)} onclick={() => setChat(a.id, false)} aria-pressed={!chatting(a.id)} title={t('Its terminal, as it runs')}>{t('Terminal')}</button>
              <button class:on={chatting(a.id)} onclick={() => setChat(a.id, true)} aria-pressed={chatting(a.id)} title={t('Its conversation, read from its transcript: its questions as cards, a box to write to it')}>Chat</button>
            </span>
          {/if}
          <span class="seg" role="group" aria-label={t('Side panel')}>
            {#each sideOf(a, side) as [tb, , label] (tb)}
              <button class:on={side === tb} onclick={() => setSide(side === tb ? null : tb)} title={label} aria-pressed={side === tb}>{SIDE_SHORT[tb]}</button>
            {/each}
          </span>
          {#if grid.zoomed}<button class="quiet icon" onclick={() => (grid.zoomed = false)} aria-label={t('Back to the agents side by side')} title={keys(t('Back to the agents side by side (⌘↵)'))}><Icon name="shrink" /></button>{/if}
          {#if side}
            <button class="quiet icon" onclick={() => setFull(!full)} aria-pressed={full} aria-label={full ? t('Restore the panel') : t('Panel full width')} title={full ? t('Restore the panel (Esc)') : t('Panel full width, for a while')}><Icon name={full ? 'shrink' : 'expand'} /></button>
          {/if}
          {#if a.state === 'running' && !chatting(a.id)}
            <button class="quiet" onclick={() => act(() => stopAgent(a.id))} title={t('Ctrl-C twice: {agent} exits, its last screen stays', { agent: nameOf(a) })}><Icon name="stop" />{t('Stop')}</button>
          {/if}
          <ActionMenu sections={agentMenu(a, () => startRename(a))} head={agentHead(a)} label={t('All actions on {name}', { name: a.title })} />
        </div>
        {#if readMeta && !editMeta}
          <!-- `[agents] header_meta = "read"`: where it works and how it is filed, read; changed behind Edit. -->
          <div class="row meta read">
            {#if a.ticket}
              {#if canPreview()}<button class="link" class:on={ticketView.id === a.ticket} onclick={() => openTicket(a.ticket!)} title={t('Show {ticket}: its preview', { ticket: a.ticket })}><Icon name="kanban" size={11} />{a.ticket}</button>{:else}<span class="tick">{a.ticket}</span>{/if}
              <span class="dot">·</span>
            {/if}
            {#if agentMr}<button class="link mr" class:on={!!mrView.ref && sameMr(mrView.ref, agentMr)} onclick={() => openMr(agentMr)} title={t('Show {mr}', { mr: mrKey(agentMr) })}><Icon name="mr" size={11} />{mrKey(agentMr)}</button><span class="dot">·</span>{/if}
            <span class="path" title="{a.branch ? `${t('Branch {branch}', { branch: a.branch })} · ` : ''}{tildePath(a.worktree ?? a.cwd)}">{a.branch ?? tildePath(a.worktree ?? a.cwd)}</span>
            {#if a.worktree}<span class="pill wt" title={t('Worktree of this agent: {path}', { path: tildePath(a.worktree) })}>{t('worktree')}</span>{/if}
            {@render ports(a)}
            <span class="grow"></span>
            {#if a.category}<KindBadge name={a.category} />{/if}
            {#each a.tags ?? [] as tag (tag)}<span class="filed">#{tag}</span>{/each}
            <button class="edit" onclick={() => (editMeta = true)} title={t('Link a ticket or an MR, change its kind and tags')}>{t('Edit')} <Icon name="down" size={10} /></button>
          </div>
        {:else}
        <div class="row meta">
          <!-- Its ticket and its open MR, a click away: their side panels, over the terminal. -->
          {#if a.ticket}
            {#if canPreview()}<button class="link" class:on={ticketView.id === a.ticket} onclick={() => openTicket(a.ticket!)} title="{t('Show {ticket}: its preview', { ticket: a.ticket })}{a.ticket_linked ? ` ${t('(linked by hand)')}` : ''}"><Icon name="kanban" size={11} />{a.ticket}</button>{:else}<span class="tick">{a.ticket}</span>{/if}
            {#if a.ticket_linked}<button class="untag" onclick={() => act(() => linkAgentTicket(a.id, null))} title={t('Unlink {ticket}: back to the ticket of its title or branch', { ticket: a.ticket })} aria-label={t('Unlink {ticket}', { ticket: a.ticket })}><Icon name="close" size={9} /></button>{/if}
          {/if}
          {#if agentMr}
            <button class="link mr" class:on={!!mrView.ref && sameMr(mrView.ref, agentMr)} onclick={() => openMr(agentMr)} title="{t('Show {mr}', { mr: mrKey(agentMr) })}{'title' in agentMr ? `: ${agentMr.title}` : ''}{a.mr ? ` ${t('(linked by hand)')}` : ''}"><Icon name="mr" size={11} />{mrKey(agentMr)}</button>
            {#if a.mr}<button class="untag" onclick={() => act(() => linkAgentMr(a.id, null))} title={t('Unlink {mr}: back to its branch’s', { mr: mrKey(agentMr) })} aria-label={t('Unlink {mr}', { mr: mrKey(agentMr) })}><Icon name="close" size={9} /></button>{/if}
          {/if}
          <input class="addtag" {@attach suggest(() => linkChoices)} placeholder={t('+ link')} spellcheck="false" title={t('Link a ticket or an MR by hand, over the ones of its title or branch')} aria-label={t('Link a ticket or an MR')} onchange={(e) => addLink(a, e.currentTarget)} />
          {#if a.ticket || agentMr}<span class="dot">·</span>{/if}
          {#if a.branch}<span class="path" title={t('Branch')}>{a.branch}</span><span class="dot">·</span>{/if}
          <span class="path" title={a.worktree ?? a.cwd}>{tildePath(a.worktree ?? a.cwd)}</span>
          {#if a.worktree}<span class="pill wt" title={t('Worktree of this agent')}>{t('worktree')}</span>{/if}
          {#each a.adopted ?? [] as p (p)}<span class="pill wt" title={t('{path}: created by a command of this agent (a skill)', { path: tildePath(p) })}>+ {p.split('/').pop()}</span>{/each}
          {#each a.also ?? [] as p (p)}<span class="pill wt also" title={t('{path}: its worktree in this repository too, same branch (--add-dir)', { path: tildePath(p) })}>+ {agentRepo({ cwd: p, worktree: p }, live.snapshot) ?? p.split('/').pop()}</span>{/each}
          {@render ports(a)}
          <span class="grow"></span>
          <label class="chip" style:--k={a.category ? kindColor(a.category) : null} title={t('Kind: what it does (guessed from its command at launch)')}>
            <span class="k">{t('Kind')}</span>
            <select value={a.category ?? ''} onchange={(e) => act(() => setAgentCategory(a.id, e.currentTarget.value || null))}>
              <option value="">—</option>
              {#each a.category && !categories.includes(a.category) ? [...categories, a.category] : categories as c (c)}<option value={c}>{c}</option>{/each}
            </select>
          </label>
          <span class="tags" title={t('Tags: several per agent')}>
            {#each a.tags ?? [] as tag (tag)}
              <span class="pill tag">#{tag}<button class="untag" onclick={() => act(() => setAgentTags(a.id, (a.tags ?? []).filter((x) => x !== tag)))} title={t('Remove #{tag}', { tag })} aria-label={t('Remove tag {tag}', { tag })}><Icon name="close" size={9} /></button></span>
            {/each}
            <input class="addtag" {@attach suggest(() => allTags().filter((tag) => !(a.tags ?? []).includes(tag)))} placeholder={t('+ tag')} spellcheck="false" aria-label={t('Add a tag')} onchange={(e) => addTag(a, e.currentTarget)} />
          </span>
          {#if readMeta}<button class="edit" onclick={() => (editMeta = false)} title={t('Back to the line to read')}>{t('Done')} <Icon name="up" size={10} /></button>{/if}
        </div>
        {/if}
      </header>
      <!-- In its Chat, its chain is a chip of the rail. -->
      {#if !chatting(a.id)}<AgentChain agent={a} />{/if}
      <AgentLimitBanner {a} />
      {#if a.worktree && a.worktree_note}<p class="ask bar">{t('Worktree kept: {note}', { note: a.worktree_note })}</p>{/if}
      {#if error}<p class="err bar">{error}</p>{/if}
    {:else if error}
      <p class="err bar">{error}</p>
    {/if}
    <!--
      Outside the branches above: terminals stay mounted (and attached) when switching to another
      agent, a desktop session or the launcher; hidden, they keep their size.
    -->
    <div class="body" class:off={!shownAgent} class:full={full && !split} style:grid-template-columns={side && shownAgent && !full && !split ? `minmax(0, 1fr) 6px ${sideW}px` : null}>
      <div class="main" class:split role="region" aria-label={t('Terminal')} style:grid-template={split ? TEMPLATE[grid.layout] : null}
        ondragentercapture={(e) => { if (draggedFiles(e) && shownAgent?.state === 'running') dropping = true }}
        ondragovercapture={(e) => { if (dropping) e.preventDefault() }}
        ondragleavecapture={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node | null)) dropping = false }}
        ondropcapture={(e) => { if (dropping) void dropOnAgent(e) }}>
        {#if dropping && shownAgent && !split}
          <div class="veil"><div><b>{t('Drop to attach to {agent}', { agent: shownAgent.title })}</b><small>{t('Saved in its folder (.prism/attach/, ignored by git), their paths typed in its prompt, not sent.')}</small></div></div>
        {/if}
        {#if !split && shownAgent?.state === 'prepared'}
          <p class="muted empty">{t('Prepared, not started: nothing runs and no worktree exists yet.')} <b>{t('Start')}</b> {t('launches it as prepared (its {tab} tab),', { tab: tc('tab', 'Launch') })} <b>{t('Edit…')}</b> {t('changes it first.')}</p>
        {/if}
        {#if !split && shownAgent?.state === 'gone'}
          <p class="muted empty">{t('The terminal of this agent is gone (the computer restarted).')} <b>{t('Resume')}</b> {t('picks the conversation up again.')}</p>
        {/if}
        {#each mounted as id (id)}
          {@const at = split ? grid.visible.indexOf(id) : -1}
          {@const a = agentOf(id)}
          {@const tab = split ? (sides[id] ?? null) : null}
          {@const chat = !tab && chatting(id)}
          <!-- One element per agent wherever it shows: only its place in the grid changes, its terminal stays attached. -->
          <!-- svelte-ignore a11y_no_static_element_interactions : an agent dropped from the list; ⌘-click there is the keyboard-free path -->
          <div class="tw" class:off={split ? at < 0 : id !== shownAgent?.id} class:cell={split} class:focus={split && at === grid.slot} style:grid-area={at >= 0 ? `p${at}` : null}
            onpointerdowncapture={(ev) => split && !onControl(ev) && grid.focus(at)}
            ondragover={(ev) => { if (split && ev.dataTransfer?.types.includes(PANE_DRAG)) ev.preventDefault() }}
            ondrop={(ev) => { const dragged = ev.dataTransfer?.getData(PANE_DRAG); if (split && dragged) { ev.preventDefault(); grid.placeInto(at, dragged) } }}>
            {#if split && a}{@render cellHead(a, at, tab)}{/if}
            <div class="tslot" class:off={!!tab || chat}><Terminal {id} active={id === shownAgent?.id && !chat} /></div>
            {#if a && chat && (split ? at >= 0 : id === shownAgent?.id)}<div class="cchat"><AgentChat agent={a} onterminal={() => setChat(id, false)} onplan={() => (split ? setSideOf(id, 'plan') : setSide('plan'))} /></div>{/if}
            {#if split && a && tab}<div class="cside">{#key a.id}<AgentSide agent={a} bind:tab={() => tab, (tb) => setSideOf(a.id, tb)} />{/key}</div>{/if}
            {#if split && a && !chat && (a.context || a.usage)}<footer class="ctxbar"><ContextDetail agent={a} /><span class="ctxgrow"></span>{#if a.usage}<AgentUsageChip usage={a.usage} provider={a.provider} />{/if}</footer>{/if}
          </div>
        {/each}
        {#if split}
          {#each grid.visible as id, i (i)}
            {#if !id || !mounted.includes(id)}
              {@const a = agentOf(id)}
              <!-- svelte-ignore a11y_no_static_element_interactions : as above; its picker is the keyboard path -->
              <div class="tw cell" class:focus={i === grid.slot} style:grid-area="p{i}"
                ondragover={(ev) => { if (ev.dataTransfer?.types.includes(PANE_DRAG)) ev.preventDefault() }}
                ondrop={(ev) => { const dragged = ev.dataTransfer?.getData(PANE_DRAG); if (dragged) { ev.preventDefault(); grid.placeInto(i, dragged) } }}
                onpointerdowncapture={(ev) => !onControl(ev) && grid.focus(i)}>
                {#if a}
                  {@render cellHead(a, i, null)}
                  <p class="muted empty">{a.state === 'prepared' ? t('Prepared, not started: Start launches it as prepared.') : t('Its terminal is gone: Resume picks the conversation up again.')}</p>
                {:else}
                  <div class="vacant">
                    <b>{t('Pane {n}', { n: i + 1 })}</b>
                    <span>{keys(t('Drop an agent here, ⌘-click one in the list, or'))}</span>
                    <select onchange={(ev) => ev.currentTarget.value && grid.placeInto(i, ev.currentTarget.value)} aria-label={t('Pick an agent for pane {n}', { n: i + 1 })}>
                      <option value="">{t('pick an agent…')}</option>
                      {#each agents.filter((x) => !grid.visible.includes(x.id)) as x (x.id)}<option value={x.id}>{x.title}</option>{/each}
                    </select>
                    {#if grid.follow}<small>{t('Following: the next agent waiting for you comes here.')}</small>{/if}
                  </div>
                {/if}
              </div>
            {/if}
          {/each}
        {/if}
        <!-- Its Chat has them in its dock. -->
        {#if !split && shownAgent && !chatting(shownAgent.id)}<QuickAnswers agent={shownAgent} bar />{/if}
        {#if !split && shownAgent && !chatting(shownAgent.id) && (shownAgent.context || shownAgent.usage)}<footer class="ctxbar"><ContextDetail agent={shownAgent} /><span class="ctxgrow"></span>{#if shownAgent.usage}<AgentUsageChip usage={shownAgent.usage} provider={shownAgent.provider} />{/if}</footer>{/if}
      </div>
      {#if side && shownAgent && !split}
        {#if !full}<Gutter dir="x" onresize={resizeSide} onreset={() => setWidth(SIDE_W)} />{/if}
        {#key shownAgent.id}<AgentSide agent={shownAgent} bind:tab={() => side ?? 'files', (tb) => setSide(tb)} />{/key}
      {/if}
    </div>
  </section>
</section>

<style>
  .agents { flex: 1; min-height: 0; display: grid; grid-template-columns: 290px minmax(0, 1fr); }
  .agents.collapsed { grid-template-columns: 62px minmax(0, 1fr); }
  /* Rail: the collapsed list. */
  .rail { display: flex; flex-direction: column; align-items: center; gap: 4px; min-height: 0; padding: 8px 0 0; border-right: 1px solid var(--border); background: var(--surface); }
  .rail .ctl { width: 30px; justify-content: center; padding: 2px 0; color: var(--text-muted); }
  .rail .ctl.plus { border-color: color-mix(in srgb, var(--src-claude) 55%, var(--border)); background: color-mix(in srgb, var(--src-claude) 8%, var(--surface)); color: var(--src-claude); }
  .rscroll { flex: 1; min-height: 0; width: 100%; overflow-y: auto; display: flex; flex-direction: column; align-items: center; gap: 5px; padding: 0 0 10px; }
  .rail .sep { flex-shrink: 0; width: 60%; height: 1px; margin: 3px 0 1px; background: var(--border); }
  .it { position: relative; flex-shrink: 0; width: 50px; flex-direction: column; gap: 2px; padding: 5px 0 3px; border: 1px solid transparent; border-radius: var(--r8); background: var(--surface-muted); color: var(--text); }
  .it b { font: 600 10px var(--mono); max-width: 46px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .it :global(svg) { color: var(--k); }
  .it.on { background: color-mix(in srgb, var(--src-claude) 14%, transparent); border-color: color-mix(in srgb, var(--src-claude) 45%, transparent); }
  .it.desktop { border-style: dashed; border-color: color-mix(in srgb, var(--src-claude) 40%, transparent); }
  .it .st { position: absolute; top: -3px; right: -3px; width: 10px; height: 10px; border: 2px solid var(--surface); }
  .t-refine { --k: var(--violet); }
  .t-implement { --k: var(--info); }
  .t-review { --k: var(--src-gitlab); }
  .t-validate { --k: var(--ok); }
  .t-cve { --k: var(--ko); }
  .t-other { --k: var(--text-muted); }
  .top .fold { padding: 1px 4px; border-color: transparent; background: none; color: var(--text-muted); }
  .list { display: flex; flex-direction: column; min-height: 0; border-right: 1px solid var(--border); background: var(--surface); }
  .top { display: flex; align-items: center; gap: 8px; padding: 10px 10px 6px; }
  .top b { font-size: 13px; }
  .grow { flex: 1; }
  .filter { display: flex; gap: 4px; padding: 0 10px 6px; }
  .filter button { padding: 1px 9px; border-radius: var(--r10); font-size: var(--fs-meta); color: var(--text-muted); background: none; }
  .filter button.on { background: color-mix(in srgb, var(--src-claude) 15%, transparent); border-color: color-mix(in srgb, var(--src-claude) 40%, transparent); color: var(--src-claude); }
  .scroll { flex: 1; min-height: 0; overflow: auto; padding: 0 6px 10px; display: flex; flex-direction: column; gap: 2px; }
  .grp { display: flex; align-items: center; gap: 3px; width: calc(100% - 12px); margin: 10px 6px 3px; padding: 0; border: none; background: none; font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); cursor: pointer; }
  .grp:hover { color: var(--text); }
  /* A group's header, with its project's +. */
  .gh { display: flex; align-items: center; gap: 4px; margin-right: 6px; }
  .gh .grp { flex: 1; width: auto; }
  .grp.idle { cursor: default; padding-left: 14px; }
  .padd { display: inline-flex; align-items: center; justify-content: center; width: 18px; height: 18px; margin-top: 7px; padding: 0; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--src-claude); cursor: pointer; }
  .padd:hover:not(:disabled) { border-color: var(--src-claude); }
  .padd:disabled { opacity: 0.4; cursor: default; }
  /* Where a project was left: its last agent closed, its last words, Reopen. */
  .left { display: flex; flex-direction: column; align-items: flex-start; gap: 3px; margin: 0 8px 4px 20px; padding: 2px 0 2px 8px; border-left: 2px solid var(--border); font-size: var(--fs-meta); color: var(--text-muted); }
  .left code { font-family: var(--mono); }
  .left q { display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; color: var(--text); quotes: '« ' ' »'; }
  :global(html[lang='en']) .left q { quotes: '“' '”'; }
  .left button, .next button { display: inline-flex; align-items: center; gap: 4px; padding: 1px 7px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .left button:hover:not(:disabled), .next button:hover { border-color: var(--src-claude); }
  /* Its quick actions: an agent each, in one click. */
  .quick { display: flex; flex-wrap: wrap; gap: 4px; margin: 0 8px 4px 20px; }
  .quick button { display: inline-flex; align-items: center; gap: 4px; max-width: 100%; padding: 1px 7px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .quick button :global(svg) { flex: none; color: var(--src-claude); }
  .quick button:hover:not(:disabled) { border-color: var(--src-claude); }
  .quick button:disabled { opacity: 0.4; cursor: default; }
  /* Its next ticket: Implement at hand. */
  .next { display: flex; align-items: center; gap: 6px; margin: 0 8px 4px 20px; font-size: var(--fs-meta); min-width: 0; }
  .next .tk { padding: 0 5px; border-color: transparent; background: none; color: var(--info); font-family: var(--mono); }
  .next .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .next .go { flex: none; color: var(--src-claude); padding: 2px 5px; }
  .rest { margin: 0 6px 0 20px; padding: 0; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rest:hover { color: var(--text); }
  .grp .wait { width: 6px; height: 6px; margin-left: 4px; border-radius: 50%; background: var(--warn); }
  .grp span { margin-left: auto; font-variant-numeric: tabular-nums; }
  .ag { display: grid; grid-template-columns: 14px minmax(0, 1fr) auto; gap: 2px 8px; padding: 7px 8px; border: 1px solid transparent; border-radius: var(--r8); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .ag:hover { background: var(--surface-muted); }
  .ag.on { background: color-mix(in srgb, var(--src-claude) 13%, transparent); border-color: color-mix(in srgb, var(--src-claude) 35%, transparent); }
  /* Desktop session: dotted edge, square pill, slightly faded title. */
  .ag.desktop { border-left: 2px dashed color-mix(in srgb, var(--src-claude) 55%, transparent); border-radius: 0 var(--r8) var(--r8) 0; }
  .ag.desktop .st { border-radius: var(--r2); }
  .ag.desktop .t { font-weight: 500; color: color-mix(in srgb, var(--text) 85%, var(--text-muted)); }
  .ag .st { grid-row: 1 / span 2; align-self: start; margin-top: 5px; }
  .ag .t { font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ag .tm { font-size: var(--fs-meta); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .ag .why { grid-column: 2 / span 2; font-size: var(--fs-meta); color: var(--warn); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ag .m { grid-column: 2 / span 2; display: flex; flex-wrap: wrap; align-items: center; gap: 3px 6px; min-width: 0; font-size: var(--fs-meta); color: var(--text-muted); }
  /* Too many pills for one line: they wrap; the branch shrinks first, then goes under them. */
  .ag .br { flex: 1 1 0; min-width: 72px; font-family: var(--mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ag .m :global(.ctx) { margin-left: auto; flex: none; }
  .more { margin: 2px 6px; align-self: flex-start; border: none; background: none; color: var(--info); font-size: var(--fs-meta); }
  .pill.dk { background: color-mix(in srgb, var(--src-claude) 20%, transparent); color: var(--src-claude); }
  /* Same status colors as the Claude block. */
  .st { width: 8px; height: 8px; border-radius: 50%; display: inline-block; flex-shrink: 0; }
  .st.running { background: var(--ok); animation: pulse 1.4s infinite; }
  .st.background { background: var(--ok); opacity: 0.6; }
  .st.needs_you { background: var(--warn); }
  .st.waiting_tool { background: var(--ko); }
  .st.review_ready { background: var(--info); }
  .st.completed, .st.idle { background: var(--text-muted); opacity: 0.5; }
  .empty { padding: 14px 10px; font-size: var(--fs-meta); line-height: 1.5; }
  .pane .empty { margin: auto; max-width: 420px; text-align: center; }

  .pane { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  /* Shown, the launcher fills the pane and scrolls in it: a long prompt never pushes it past the window. */
  .launcher:not([hidden]) { flex: 1; min-height: 0; overflow: auto; }
  header { display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--surface); }
  header .title { font-size: 13.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  header .path { font: var(--fs-meta) var(--mono); color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  button { display: inline-flex; align-items: center; gap: 5px; padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); cursor: pointer; white-space: nowrap; }
  button:hover:not(:disabled) { border-color: var(--src-claude); }
  button:disabled { opacity: 0.5; cursor: default; }
  button.primary { background: var(--src-claude); border-color: var(--src-claude); color: #fff; background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--src-claude); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
  /* "+ New ⌘N": thin outline, dark text, as tall as the filters. */
  .new { gap: 4px; padding: 1px 8px 1px 6px; border-color: color-mix(in srgb, var(--src-claude) 55%, var(--border)); background: color-mix(in srgb, var(--src-claude) 8%, var(--surface)); font-size: var(--fs-meta); font-weight: 500; line-height: 18px; }
  /* An icon rather than a font "+": it, at least, is centered on the text height. */
  .new :global(svg) { color: var(--src-claude); vertical-align: 0; }
  .new kbd { margin-left: 3px; color: var(--text-muted); }
  button.allow { background: var(--warn-bg); border-color: transparent; color: var(--warn); font-weight: 600; }
  kbd { font: 600 10px var(--mono); opacity: 0.8; }
  .err { margin: 0; color: var(--ko); font-size: var(--fs-meta); white-space: pre-wrap; }
  .bar { margin: 0; padding: 6px 12px; font-size: var(--fs-meta); }
  .err.bar { background: var(--ko-bg); }
  .ask.bar { background: var(--warn-bg); color: var(--warn); }

  .deskpane { flex: 1; min-height: 0; display: flex; align-items: center; justify-content: center; padding: 20px; background: repeating-linear-gradient(135deg, var(--bg) 0 10px, color-mix(in srgb, var(--bg) 92%, var(--text)) 10px 20px); }
  .deskcard { width: min(460px, 100%); display: flex; flex-direction: column; gap: 10px; padding: 16px 18px; border: 1px dashed color-mix(in srgb, var(--src-claude) 50%, var(--border)); border-radius: var(--r12); background: var(--surface); }
  .deskcard .hd { display: flex; align-items: center; gap: 10px; }
  .deskcard .ic { display: inline-flex; padding: 7px; border-radius: var(--r8); background: color-mix(in srgb, var(--src-claude) 18%, transparent); color: var(--src-claude); }
  .deskcard p { margin: 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .deskcard .ask { padding: 8px 10px; border-radius: var(--r8); background: var(--warn-bg); color: var(--warn); }
  .deskcard .open { align-self: flex-start; padding: 4px 12px; border-radius: var(--r6); background: var(--src-claude); color: #fff; text-decoration: none; font-weight: 500; }

  .row { display: flex; align-items: center; justify-content: flex-end; gap: 8px; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r7); overflow: hidden; }
  .seg button { border: none; border-radius: 0; background: var(--surface); color: var(--text-muted); }
  .seg button + button { border-left: 1px solid var(--border); }
  .seg button.on { background: var(--src-claude); color: #fff; }
  .filter .by { flex-shrink: 0; padding: 1px 6px; border-color: transparent; }
  .filter .sort { flex-shrink: 1; min-width: 0; max-width: 104px; padding: 1px 2px; border: 1px solid transparent; border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .filter .sort:hover, .filter .sort:focus { border-color: var(--border); color: var(--text); outline: none; }
  header .rename { min-width: 0; width: 260px; padding: 2px 7px; border: 1px solid var(--src-claude); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; font-size: 13.5px; font-weight: 600; outline: none; }
  /* An agent's header: row 1 says what to do now,
     row 2, small and muted, where it works and how it is filed. */
  header.two { flex-direction: column; align-items: stretch; gap: 3px; padding: 7px 12px 6px; }
  header.two .row { display: flex; align-items: center; gap: 8px; min-width: 0; }
  header.two .meta { gap: 6px; padding-left: 16px; color: var(--text-muted); font-size: var(--fs-meta); }
  .meta .dot { opacity: 0.5; }
  .meta.read { gap: 6px; }
  .meta .filed { color: var(--text-muted); white-space: nowrap; }
  .meta .edit { display: inline-flex; align-items: center; gap: 3px; padding: 0 4px; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .meta .edit:hover { color: var(--text); }
  .meta .link { display: inline-flex; align-items: center; gap: 4px; padding: 1px 7px; border: 1px solid color-mix(in srgb, var(--src-clickup) 40%, transparent); border-radius: var(--r5); background: color-mix(in srgb, var(--src-clickup) 10%, transparent); color: var(--src-clickup); font: 600 var(--fs-meta) var(--mono); cursor: pointer; }
  .meta .link.port { --c: var(--ok); border-color: color-mix(in srgb, var(--c) 40%, transparent); background: color-mix(in srgb, var(--c) 10%, transparent); color: var(--c); }
  .meta .link.mr { --c: var(--src-gitlab); border-color: color-mix(in srgb, var(--c) 40%, transparent); background: color-mix(in srgb, var(--c) 10%, transparent); color: var(--c); }
  .meta .link:hover, .meta .link.on { background: color-mix(in srgb, currentColor 22%, transparent); }
  .meta .tick { padding: 0; border: none; background: none; color: var(--src-clickup); font: 600 var(--fs-meta) var(--mono); }
  button.tick:hover { text-decoration: underline; }
  .seg { display: inline-flex; flex-shrink: 0; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { padding: 1px 8px; border: none; border-right: 1px solid var(--border); border-radius: 0; background: none; color: var(--text-muted); font-size: var(--fs-meta); }
  .seg button:last-child { border-right: none; }
  .seg button.on { background: color-mix(in srgb, var(--src-claude) 13%, transparent); color: var(--src-claude); }
  button.quiet { border-color: transparent; background: none; color: var(--text-muted); }
  .chip { display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; padding: 0 2px 0 6px; border: 1px dashed transparent; border-radius: var(--r9); color: var(--text-muted); cursor: pointer; }
  .chip:hover, .chip:focus-within { border-color: var(--border); }
  .chip .k { opacity: 0.7; }
  .chip select { padding: 0; border: none; background: none; color: var(--k, var(--text)); font: inherit; font-weight: 600; cursor: pointer; outline: none; }
  .pill.tag { background: var(--surface-muted); color: var(--text-muted); }
  .tags { display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; }
  .tags .pill.tag { display: inline-flex; align-items: center; gap: 2px; padding-right: 2px; }
  .untag { display: inline-flex; padding: 1px; border: none; border-radius: 50%; background: none; color: inherit; opacity: 0.6; cursor: pointer; }
  .untag:hover { opacity: 1; background: color-mix(in srgb, var(--info) 18%, transparent); }
  .addtag { width: 58px; padding: 0 6px; border: 1px dashed var(--border); border-radius: var(--r9); background: none; color: var(--text); font: inherit; font-size: var(--fs-tag); outline: none; }
  .addtag:focus { width: 100px; border-style: solid; border-color: var(--info); }
  .filter .sort.on { color: var(--info); }
  .tags { flex-wrap: wrap; }
  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr); }
  .body.off, .tw.off { display: none; }
  .body .main { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .tslot { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .tslot.off { display: none; }
  .cchat { flex: 1; min-height: 0; position: relative; }
  .cside { flex: 1; min-height: 0; display: grid; grid-template: minmax(0, 1fr) / minmax(0, 1fr); }
  /* Side by side: one cell per pane, the focused one edged. */
  .body .main.split { display: grid; gap: 6px; padding: 6px; background: var(--bg); }
  .tw.cell { display: flex; flex-direction: column; min-width: 0; min-height: 0; border: 1px solid var(--border); border-radius: var(--r8); overflow: hidden; background: var(--surface); }
  .tw.cell.off { display: none; }
  .tw.cell.focus { border-color: var(--src-claude); box-shadow: 0 0 0 1px var(--src-claude); }
  .chead { flex: none; display: flex; align-items: center; gap: 6px; min-width: 0; padding: 4px 6px 4px 8px; border-bottom: 1px solid var(--border); background: var(--surface); }
  .chead .n { font: var(--fs-tag) var(--mono); color: var(--text-muted); }
  .chead .title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-body); cursor: default; }
  .chead button { padding: 1px 7px; font-size: var(--fs-meta); }
  .chead .icon { padding: 2px 4px; }
  .ctab { padding: 1px 4px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  .vacant { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; padding: 12px; background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-meta); text-align: center; }
  .vacant select { padding: 2px 6px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; }
  .layouts { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .layouts button { padding: 3px 6px; border: none; border-right: 1px solid var(--border); border-radius: 0; color: var(--text-muted); }
  .layouts button:last-child { border-right: none; }
  .layouts button.on { background: color-mix(in srgb, var(--src-claude) 14%, var(--surface)); color: var(--src-claude); }
  .lic { display: inline-grid; gap: 1px; width: 14px; height: 10px; }
  .lic i { border-radius: 1px; background: currentColor; opacity: 0.6; }
  .lic.l2 { grid-template-columns: 1fr 1fr; }
  .lic.l2r { grid-template-rows: 1fr 1fr; }
  .lic.l4 { grid-template: 1fr 1fr / 1fr 1fr; }
  .ag .slot { display: inline-block; min-width: 15px; margin-right: 6px; padding: 0 4px; border-radius: var(--r9); background: var(--src-claude); color: var(--on-accent, #fff); font: 600 10px/15px var(--sans); text-align: center; vertical-align: 1px; }
  /* Full-width panel: the terminals stay mounted, hidden (a zero-size frame keeps their size). */
  .body.full .main { display: none; }
  .tw { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  /* The context and the cost, together at the end of the line. */
  .ag .use { flex: none; display: inline-flex; align-items: center; gap: 6px; margin-left: auto; }
  .ctxgrow { flex: 1; }
  /* Narrow (a side panel open): the session's usage goes under the context rather than out of view. */
  .ctxbar { flex: none; display: flex; flex-wrap: wrap; align-items: center; gap: 2px 12px; padding: 4px 12px; border-top: 1px solid var(--border); background: var(--surface); }
  .pill.port { background: color-mix(in srgb, var(--ok) 16%, transparent); color: var(--ok); font-family: var(--mono); }
  .pill.wt { background: color-mix(in srgb, var(--src-gitlab) 18%, transparent); color: var(--src-gitlab); }
  .pill.model { font-family: var(--mono); }
  .pill.loop { background: color-mix(in srgb, var(--warn) 22%, transparent); color: var(--warn); }
  .main { position: relative; }
  .veil { position: absolute; inset: 0; z-index: 20; display: grid; place-items: center; border: 2px dashed var(--src-claude); border-radius: var(--r4); background: color-mix(in srgb, var(--src-claude) 14%, rgba(10, 10, 10, 0.55)); pointer-events: none; }
  .veil > div { max-width: 380px; padding: 12px 18px; border-radius: var(--r10); background: var(--surface); text-align: center; font-size: var(--fs-body); }
  .veil small { display: block; margin-top: 3px; color: var(--text-muted); font-size: var(--fs-meta); }
</style>
