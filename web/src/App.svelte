<script lang="ts">
  import { inField, inLayer } from './layers.svelte'
  import { reopenLastPanel } from './recent-panels.svelte'
  // Agents' chains run while Prism does, the Agents view open or not.
  import { clock } from './clock.svelte'
  import { onMount, tick, untrack } from 'svelte'
  import { toast } from './toast.svelte'
  import type { Snippet } from 'svelte'
  import { keyedAction, watchMenu } from './app-menu'
  import { t, tn, watchLanguage, tc } from './i18n.svelte'
  import { connect, live } from './live.svelte'
  import { meOn } from './forge'
  import { alerts as computeAlerts, blockAlerts, hasExternalAlerts, awaitsMyReview, countedSessions, describeWorktrees, podDown, waitingAlert, waitsForYou } from './rules'
  import { alertIsMine, alertMatches, apply, applyMine, filter, mineFocus, resolve, setQuery, toggleMine } from './filter.svelte'
  import Alerts from './components/Alerts.svelte'
  import { addColumn, bandsOf, columnsOf, currentView, editing, ensureSizes, showBlock, modeOf, moveBand, moveBlock, resetLayout, setView, shift, sizesOf, stepView, toggleEditing, toggleZen, zen, zoom } from './layout.svelte'
  import LayoutMenu from './components/LayoutMenu.svelte'
  import ViewIcon from './components/ViewIcon.svelte'
  import { MAX_COLUMNS, bandRanges, hiddenViews, isCustom, views, type BlockId, type View } from './views'
  import { look, movePage, pageUi, renamePage, startRename, startStyling } from './pages.svelte'
  import PageMenus from './components/PageMenus.svelte'
  import CustomBlock from './blocks/custom/CustomBlock.svelte'
  import { customBlock } from './blocks/custom/types'
  import { BLOCK_COMPONENTS } from './blocks/components'
  import { BLOCK_DEFS } from './blocks'
  import { OWNER } from './blocks/runtime'
  import ModuleBoundary from './modules/ModuleBoundary.svelte'
  import { moduleFailure } from './modules/modules.svelte'
  import { stateOf } from './blocks/block'
  import { alertSettings, hasTickets, ticketExample } from './conventions'
  import { ruleBand } from './alert-bands'
  import { isSnoozed, noteAlerts } from './alert-snooze.svelte'
  import Gutter from './components/Gutter.svelte'
  import TimeTracker from './components/TimeTracker.svelte'
  import PrismLogo from './components/PrismLogo.svelte'
  import ThemeMenu from './components/ThemeMenu.svelte'
  import KeyHelp from './components/KeyHelp.svelte'
  import { followPointer, handleKey, reset as resetKeys } from './keys.svelte'
  import { syncDesktop } from './desktop'
  import { isSeen } from './seen.svelte'
  import DatePicker from './components/DatePicker.svelte'
  import MergeRequests from './components/MergeRequests.svelte'
  import Pods from './components/Pods.svelte'
  import Mail from './components/Mail.svelte'
  import Sessions from './components/Sessions.svelte'
  import Worktrees from './components/Worktrees.svelte'
  import PagesWizard from './components/PagesWizard.svelte'
  import ProfileMenu from './components/ProfileMenu.svelte'
  import BlockOff from './components/BlockOff.svelte'
  import MachineBar from './components/MachineBar.svelte'
  import { GRACE, machineState } from './machine.svelte'
  import { wizard, wizardDue } from './wizard.svelte'
  import { pageShown } from './feature-use.svelte'
  import Notes from './components/Notes.svelte'
  import Sprint from './components/Sprint.svelte'
  import MySprint from './components/MySprint.svelte'
  import CiHealth from './components/CiHealth.svelte'
  import Vulnerabilities from './components/Vulnerabilities.svelte'
  import Environments from './components/Environments.svelte'
  import ArgoCd from './components/ArgoCd.svelte'
  import Renovate from './components/Renovate.svelte'
  import MergeQueue from './components/MergeQueue.svelte'
  import InProgress from './components/InProgress.svelte'
  import RoutinesBlock from './components/RoutinesBlock.svelte'
  import AsidesBlock from './components/AsidesBlock.svelte'
  import { reviewPile } from './routines.svelte'
  import TicketPanel from './components/TicketPanel.svelte'
  import Refinement from './components/Refinement.svelte'
  import LogPanel from './components/LogPanel.svelte'
  import PodLogPanel from './components/PodLogPanel.svelte'
  import Agenda from './components/Agenda.svelte'
  import Pickup from './components/Pickup.svelte'
  import { startMeetingAlerts } from './agenda.svelte'
  import { startDigest } from './digest.svelte'
  import { watchUpdates } from './updates.svelte'
  import { watchProblems } from './problems.svelte'
  import { watchFeedback } from './feedback.svelte'
  import Feedback from './components/Feedback.svelte'
  import AgentSwitch from './components/AgentSwitch.svelte'
  import Palette from './components/Palette.svelte'
  import { paletteKey, paletteOn, togglePalette } from './palette.svelte'
  import EventPanel from './components/EventPanel.svelte'
  import ThreadPanel from './components/ThreadPanel.svelte'
  import ArgoPanel from './components/ArgoPanel.svelte'
  import OverlapPanel from './components/OverlapPanel.svelte'
  import ChangesDrawer from './blocks/changes/ChangesDrawer.svelte'
  import McpProposal from './components/McpProposal.svelte'
  import { watchProposals } from './mcp-view.svelte'
  import { assistantOn, assistantView, proposalsOf as assistantProposals, toggleAssistant, watchRepoAdds } from './assistant.svelte'
  import AssistantPanel from './components/assistant/AssistantPanel.svelte'
  import ShellDrawer from './components/ShellDrawer.svelte'
  import MrPanel from './components/MrPanel.svelte'
  import MrChoice from './components/MrChoice.svelte'
  import LauncherDrawer from './components/LauncherDrawer.svelte'
  import { qaDraft } from './quick-action-draft.svelte'
  import NewTicketDialog from './components/NewTicketDialog.svelte'
  import { newTicketDialog } from './new-ticket.svelte'
  import StatusBar from './components/StatusBar.svelte'
  import Tooltip from './components/Tooltip.svelte'
  import ContextMenu from './components/ContextMenu.svelte'
  import SelectMenu from './components/SelectMenu.svelte'
  import Toast from './components/Toast.svelte'
  import Confetti from './components/Confetti.svelte'
  import Console from './components/Console.svelte'
  import QuickNotes from './components/QuickNotes.svelte'
  import { quickNotesKey, toggleQuickNotes } from './quick-notes.svelte'
  import Boot from './components/Boot.svelte'
  import FirstLaunch from './components/FirstLaunch.svelte'
  import ClonesPrompt from './components/ClonesPrompt.svelte'
  import { clonesDue, clonesPrompt } from './clones.svelte'
  import Screensaver from './components/Screensaver.svelte'
  import ThemeScene from './components/ThemeScene.svelte'
  import { watchArrivals } from './arrivals'
  import Settings from './components/Settings.svelte'
  import Help from './components/Help.svelte'
  import { closeHelp, helpView } from './help-view.svelte'
  import { openHelp, seeTour, toggleHelp, tour } from './help.svelte'
  import { closeSettings, quitApp, settingsView, toggleFullscreen, toggleSettings } from './settings.svelte'
  import AgentsView from './components/AgentsView.svelte'
  import ProjectsView from './components/ProjectsView.svelte'
  import { waitingWorkstreams } from './workstreams'
  import { agentsEnabled, agentsView, newAgent, openAgents, openProjects, projectsView, terminalKeeps } from './agents.svelte'
  import { agentsDigit as agentsDigitOf, hit, projectsKey as projectsKeyOf, shortcutKeys } from './shortcuts'
  import { BLOCK_HELP } from './block-help'
  import { chord, inTerminal, keys, pageDigit } from './platform'
  import { invoke, isDesktop, isLocal, machine, profileWindow } from './bridge'
  import { consoleKey, toggleConsole } from './console.svelte'
  import { matches, worksWhileTyping } from './shortcut'
  import { watchCelebrations } from './celebrations.svelte'
  import HeaderStats, { headerStats, toggleInbox } from './components/HeaderStats.svelte'
  import { appearance, featureOn, moves, watchAnimations, watchIdle } from './fun.svelte'
  import { pageSwipe } from './page-swipe'
  import { watchTheme } from './theme.svelte'
  import { watchFonts } from './fonts.svelte'
  import Icon from './components/Icon.svelte'
  import { initAudio } from './audio.svelte'
  import { startReminders } from './notes.svelte'
  import { isArchived, isHidden } from './worktree-archive.svelte'

  // First: the commands that fail from the start are kept too.
  onMount(watchProblems)
  onMount(connect)
  onMount(startMeetingAlerts)
  onMount(startDigest)
  onMount(initAudio)
  onMount(startReminders)
  onMount(watchCelebrations)
  onMount(watchAnimations)
  onMount(() => watchArrivals(document.body))
  onMount(watchLanguage)
  onMount(watchIdle)
  onMount(watchTheme)
  onMount(watchFonts)
  onMount(watchMenu)
  onMount(watchUpdates)
  onMount(watchFeedback)

  const now = $derived(clock(1000).now)

  const full = $derived(live.snapshot)
  // Several profiles: the Dock icon carries this one's letter.
  // An agent's proposal (`prism mcp`) arriving: a toast to open it.
  $effect(() => watchProposals(live.snapshot))
  $effect(() => watchRepoAdds(live.snapshot))
  /** The assistant's proposals waiting for a click: a count on its rail button. */
  const assistantWaiting = $derived(assistantProposals(live.snapshot).length)
  const focus = $derived(full ? resolve(full, filter.query) : null)
  /** All blocks read the filtered view; the alerts, the core's on the full state, are filtered. */
  const snap = $derived(full ? apply(filter.mine ? applyMine(full) : full, focus) : null)
  /** Every alert of the full state (the core's, `Snapshot.alerts`), for the view and the menu bar. */
  const allAlerts = $derived(full ? computeAlerts(full) : [])
  // The alerts of the modules' blocks, told to the core as they change (its routines
  // and Changes read them): only with such blocks, from this computer's window.
  let externalSent = ''
  $effect(() => {
    if (!full || !hasExternalAlerts() || !isLocal()) return
    const list = blockAlerts(full)
    const key = JSON.stringify(list)
    if (key === externalSent) return
    externalSent = key
    invoke('alerts_external', { alerts: list }).catch(() => (externalSent = ''))
  })
  const alerts = $derived.by(() => {
    if (!full) return []
    const titles = full.sessions.filter((c) => focus?.sessions.has(c.id)).map((c) => c.title)
    const mine = filter.mine ? mineFocus(full) : null
    return allAlerts.filter((a) => alertMatches(a, focus, titles) && (!mine || alertIsMine(a, mine)))
  })
  /**
   * Menu bar and notifications: on the full state, not on the filtered view,
   * so that an active filter does not hide an alert. Only once GitLab is loaded.
   */
  $effect(() => {
    if (!full?.me) return
    // An item marked as seen, put off or of a hidden rule no longer counts in the menu bar.
    const settings = alertSettings()
    // A put off alert comes back on the minute, not at each second's tick.
    const minute = clock(60_000).now
    syncDesktop(full, allAlerts.filter((a) => !isSeen(a) && !isSnoozed(a, minute) && ruleBand(a, settings) !== null))
  })
  // Since when each alert holds, for its age and its band (alert-snooze.svelte.ts).
  $effect(() => {
    if (full) noteAlerts(allAlerts)
  })

  /** A collapsed block keeps only its title line; its neighbors share the space. */
  /** A short or collapsed block takes the height of its content; normal blocks share the rest. */
  const grow = (id: string, size: number) => (modeOf(id) !== 'normal' ? 'flex: 0 0 auto' : `flex: ${size} 1 0`)
  /** A handle only makes sense between two open blocks. */
  const both = (a: string, b: string) => modeOf(a) === 'normal' && modeOf(b) === 'normal'

  const view = $derived(currentView())
  const sizes = $derived(sizesOf(view))
  /** Enlarged block, if it belongs to the displayed view. */
  const columns = $derived(columnsOf(view))
  /** The `[start, end)` columns of each band, top to bottom. */
  const bands = $derived(bandRanges(bandsOf(view)))
  /**
   * A band of only short or collapsed blocks, among several, takes the height of its content;
   * the others share the rest.
   */
  const fitted = (b: number) => bands.length > 1 && columns.slice(...bands[b]).flat().every((id) => modeOf(id) !== 'normal')
  // A page shown counts its blocks as used (Settings › General › Cleanup), once each time it comes up (its
  // blocks known: at launch, once the snapshot is there).
  const shownPage = $derived(columns.length ? view.id : '')
  $effect(() => {
    if (shownPage) untrack(() => pageShown(columns.flat()))
  })
  // First launch without pages in the config: the wizard offers a starting point.
  $effect(() => {
    if (wizardDue()) wizard.open = true
  })
  // Then, once, the repositories a pack names without a clone here.
  $effect(() => {
    if (clonesDue()) clonesPrompt.open = true
  })
  // The proportions of the displayed view are remembered before being adjusted with the mouse.
  $effect(() => ensureSizes(view))
  const zoomed = $derived(columns.flat().find((id) => id === zoom.id))

  /** Drag and drop in "Edit layout": the moved block, and the hovered column. */
  let dragged = $state<BlockId | null>(null)
  let over = $state<number | null>(null)
  function drop(e: DragEvent, c: number) {
    e.preventDefault()
    over = null
    if (!dragged) return
    // Before the first block whose middle is under the pointer.
    const blocks = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>(':scope > [data-block]')]
    const before = blocks.find((b) => b.dataset.block !== dragged && e.clientY < b.getBoundingClientRect().top + b.offsetHeight / 2)
    moveBlock(dragged, c, (before?.dataset.block as BlockId | undefined) ?? null)
    dragged = null
  }
  /** Fun: the block being moved tilts and lifts (tilted drag image). */
  function tilt(e: DragEvent) {
    const el = e.currentTarget as HTMLElement
    if (!moves() || !e.dataTransfer) return
    const ghost = el.cloneNode(true) as HTMLElement
    ghost.style.cssText = `position:fixed;top:-3000px;left:0;width:${el.offsetWidth}px;height:${el.offsetHeight}px;transform:rotate(-2.5deg) scale(.96);box-shadow: var(--shadow-modal);border-radius:var(--r10);overflow:hidden;pointer-events:none`
    document.body.appendChild(ghost)
    e.dataTransfer.setDragImage(ghost, e.offsetX, e.offsetY)
    setTimeout(() => ghost.remove())
  }
  /** Easter egg: seven quick clicks on the logo, it spins and casts a rainbow along the rail. */
  let logoClicks = 0
  let logoTimer: ReturnType<typeof setTimeout> | undefined
  let rainbow = $state(0)
  function logoClick() {
    if (!featureOn('easter_eggs')) return
    clearTimeout(logoTimer)
    logoTimer = setTimeout(() => (logoClicks = 0), 1500)
    if (++logoClicks < 7) return
    logoClicks = 0
    rainbow++
  }
  /** Fun: ⌥ held alone for a moment, each block shows its identifier and its source (X-rays). */
  let xray = $state(false)
  let xrayTimer: ReturnType<typeof setTimeout> | undefined
  function onAlt(e: KeyboardEvent, down: boolean) {
    clearTimeout(xrayTimer)
    if (down && e.key === 'Alt' && !e.metaKey && !e.ctrlKey && !e.shiftKey && featureOn('easter_eggs')) xrayTimer = setTimeout(() => (xray = true), 250)
    else xray = false
  }
  const xrayLabel = (id: BlockId) => `${id} · ${BLOCK_HELP[id]?.source.split(/[,.;(]/)[0] ?? ''}`
  /** Alert badge on a view's tab, so as not to miss an incident out of view. */
  const viewAlert = (v: View) => columnsOf(v).flat().includes('pods') && (snap?.pods.some(podDown) ?? false)

  let input: HTMLInputElement | undefined = $state()
  /** Typing in a field (filter, note, time entry…): no shortcut. */
  const typing = (e: KeyboardEvent) => inField(e.target)
  /** The Agents view comes after the grid views: ⌘5 with four views. */
  const agentsDigit = $derived(agentsDigitOf())
  /** The Projects page, after the Agents view: shown once a repository is configured. */
  const projectsOn = $derived((full?.repositories?.length ?? 0) > 0)
  const projectsKey = $derived(projectsKeyOf())

  /** Zen mode: the filter shown while `/` puts the cursor in it, until it loses it. */
  let filterPeek = $state(false)
  /** A page being dragged along the rail, and the place it would take. */
  let draggedPage = $state<string | null>(null)
  // A name being typed where it is no longer shown (Zen mode, Edit layout left) is given up: Esc is the app's again.
  $effect(() => {
    if (pageUi.renaming && (zen.on || (pageUi.renaming.at === 'header' && !editing.on))) pageUi.renaming = null
  })
  /** Where the page dragged would go: before rail page `dropAt` (after the last one: their count). */
  let dropAt = $state<number | null>(null)
  /** Before page `i`, or after it over its lower half. */
  function dropSlot(e: DragEvent, i: number): number {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    return e.clientY > r.top + r.height / 2 ? i + 1 : i
  }
  /** The page dragged, put at `slot` (its own place counted out once it goes). */
  function dropPage(slot: number) {
    const from = views().findIndex((v) => v.id === draggedPage)
    if (draggedPage && from >= 0) movePage(draggedPage, slot > from ? slot - 1 : slot)
    draggedPage = null
    dropAt = null
  }
  /** In Zen mode, the header is back while the filter or the For me panel is in use. */
  const headerShown = $derived(!zen.on || !!filter.query || filterPeek || !!headerStats.open)

  function onKey(e: KeyboardEvent) {
    // Escape inside a drawer, a dialog or a menu is the stack's (layers.svelte.ts), not the page's.
    if (e.key === 'Escape' && inLayer(e.target)) return
    // A shortcut an agent's terminal keeps (`[agents] terminal_keys`): Prism leaves it there, and
    // ignores the menu bar's echo of it (⌘N, ⌘,).
    if (inTerminal(e) && terminalKeeps(e)) {
      const echo = (['new-agent', 'settings', 'zen'] as const).find((id) => hit(id, e))
      if (echo) keyedAction(echo)
      return
    }
    // The command palette (`[palette] key`, ⌘K), over anything.
    if (paletteOn() && matches(e, paletteKey()) && (worksWhileTyping(paletteKey()) || !typing(e))) {
      e.preventDefault()
      togglePalette()
      return
    }
    // The chords below: shortcuts.ts declares them (and the help lists them from there).
    // ⌘⇧F: Zen mode, the rail and the header folded away.
    if (hit('zen', e)) {
      e.preventDefault()
      keyedAction('zen')
      toggleZen()
      if (zen.on) toast(t('Zen mode · {keys} to leave it', { keys: shortcutKeys('zen') }))
      return
    }
    // ⌘, : Settings, as in any macOS app; closed, back to where it was opened from.
    if (hit('settings', e)) {
      e.preventDefault()
      keyedAction('settings')
      toggleSettings()
      return
    }
    // Ctrl+Q and F11 where no menu bar does it (Linux): quit, full screen.
    if (isDesktop() && hit('quit', e)) {
      e.preventDefault()
      void quitApp()
      return
    }
    if (isDesktop() && hit('fullscreen', e)) {
      e.preventDefault()
      void toggleFullscreen()
      return
    }
    // ⌘? or F1: Help.
    if (hit('help', e)) {
      e.preventDefault()
      toggleHelp()
      return
    }
    // F2: renames the page shown, in the rail (not in Zen mode: the rail is not there); the Agents view renames its agent.
    if (hit('rename', e) && !zen.on && !typing(e) && !inTerminal(e) && !agentsView.open && !projectsView.open && !settingsView.open && !helpView.open) {
      e.preventDefault()
      startRename(view.id)
      return
    }
    // A page's menu, grid, « + » or name open: Esc is theirs.
    if (e.key === 'Escape' && (pageUi.menu || pageUi.styling || pageUi.adding || pageUi.hiddenMenu || pageUi.renaming)) return
    // Typing in an agent's terminal: everything goes to it (Esc, ⌃1…), except app chords.
    if (inTerminal(e) && !chord(e)) return
    if (e.key === 'Escape' && settingsView.open && !typing(e)) {
      closeSettings()
      return
    }
    // In Help, Esc closes an enlarged image, clears the search, then closes it.
    if (e.key === 'Escape' && helpView.open) {
      if (helpView.zoom) helpView.zoom = null
      else if (helpView.query) helpView.query = ''
      else closeHelp()
      return
    }
    // The Prism console, on the config's shortcut (`[console] key`, ` by default).
    const ck = consoleKey()
    if (matches(e, ck) && (worksWhileTyping(ck) || !typing(e))) {
      e.preventDefault()
      toggleConsole()
      return
    }
    // Quick note entry, over any view (`[notes] key`, ⌘⇧N by default).
    const nk = quickNotesKey()
    if (matches(e, nk) && (worksWhileTyping(nk) || !typing(e))) {
      e.preventDefault()
      toggleQuickNotes()
      return
    }
    // ⌘1, ⌘2…: switch view; the next digit opens the agents, ⌘N launches one.
    const digit = pageDigit(e)
    if (digit === agentsDigit && agentsEnabled()) {
      e.preventDefault()
      openAgents()
      return
    }
    if (digit === projectsKey && projectsOn) {
      e.preventDefault()
      openProjects()
      return
    }
    if (hit('new-agent', e) && agentsEnabled()) {
      e.preventDefault()
      keyedAction('new-agent')
      newAgent()
      return
    }
    // ⌘⇧T: the panel closed last comes back (recent-panels.svelte.ts).
    if (hit('reopen', e)) {
      e.preventDefault()
      reopenLastPanel()
      return
    }
    if (digit && views()[Number(digit) - 1]) {
      e.preventDefault()
      closeSettings()
      closeHelp()
      setView(views()[Number(digit) - 1].id)
      resetKeys()
      return
    }
    // "/" puts the cursor in the filter, Esc clears it (Settings has its own search).
    if (e.key === '/' && !typing(e) && !settingsView.open) {
      e.preventDefault()
      filterPeek = true
      void tick().then(() => input?.focus())
    } else if (e.key === 'm' && !typing(e) && !e.metaKey && !e.ctrlKey) {
      toggleMine()
    } else if (e.key === 'i' && !typing(e) && !e.metaKey && !e.ctrlKey && snap && !settingsView.open && !helpView.open) {
      // The header's "For me" panel (absent from the Settings and Help views).
      toggleInbox()
    } else if (e.key === 'Escape' && (filter.query || document.activeElement === input)) {
      setQuery('')
      input?.blur()
    } else if (e.key === 'Escape' && editing.on) {
      toggleEditing()
    } else if (e.key === 'Escape' && zoom.id) {
      zoom.id = null
    } else {
      handleKey(e)
    }
  }
  const managers = $derived(snap?.pods.filter((p) => p.role === 'runner') ?? [])
  const jobs = $derived(snap?.pods.filter((p) => p.role === 'job' && p.status === 'Running') ?? [])
  // Only protected branches' pipelines are collected, those of every repository with `[[repos]] pipelines`.
  const protectedFailures = $derived(snap?.pipelines.filter((p) => p.status === 'failed') ?? [])
  const reviews = $derived(snap?.merge_requests.filter((m) => awaitsMyReview(m, meOn(snap, m))) ?? [])
  const waiting = $derived(snap ? countedSessions(snap).filter(waitingAlert) : [])
  /** The Agents view has been opened: it stays mounted afterwards (see the template). */
  let agentsShown = $state(false)
  $effect(() => {
    if (agentsView.open) agentsShown = true
  })
  /** Rail badge: the agents waiting for me (question or permission). */
  /** Worktrees view badge: the stale worktrees. */
  const staleWorktrees = $derived(full ? describeWorktrees(full).filter((i) => i.state === 'stale' && !isHidden(i)).length : 0)
  /** The rail badges of the blocks on a page (broken things), once per source. */
  const blockBadges = (v: View) => {
    const shown = columnsOf(v).flat()
    const sources = new Map<string, { count: number; title: string }>()
    for (const b of BLOCK_DEFS) {
      // Custom blocks: their badge on the pages showing any of them.
      const state = b.badge && (shown.includes(b.id as BlockId) || (b.id === 'custom_blocks' && shown.some(isCustom))) ? stateOf(b, full) : undefined
      const badge = state !== undefined && b.badge!(state as never)
      if (badge && badge.count) sources.set(b.source ?? b.id, badge)
    }
    return [...sources.values()]
  }
  const agentsWaiting = $derived(full?.agents?.filter((a) => waitsForYou(a.status)).length ?? 0)
  /** Projects badge: the workstreams with an agent waiting for me, as the page counts them. */
  const workstreamsWaiting = $derived(projectsOn && full ? waitingWorkstreams(full, isArchived) : 0)
  /** The stale worktrees, on the Projects entry when no page of the rail shows the Worktrees block. */
  const staleOnProjects = $derived(!views().some((v) => columnsOf(v).flat().includes('worktrees')) ? staleWorktrees : 0)
  /** The routines' runs waiting in "To review": a badge of their own on the Agents tab. */
  const toReview = $derived(reviewPile(full).length)
  const syncs = $derived(alerts.filter((a) => a.kind === 'Sync' && !isSeen(a)))
  const ago = $derived(snap?.updated_at ? Math.max(0, Math.round((now - new Date(snap.updated_at).getTime()) / 1000)) : null)
  /** The live dot's tooltip: connected, and the age of the snapshot. */
  const status = $derived(!live.connected ? t('Reconnecting…') : ago !== null ? t('Live · {age} ago', { age: ago < 60 ? `${ago}s` : `${Math.floor(ago / 60)}m` }) : t('Live'))
</script>

<svelte:window onpointerdown={followPointer} onkeydown={(e) => { onAlt(e, true); onKey(e) }} onkeyup={(e) => onAlt(e, false)} onblur={() => onAlt(new KeyboardEvent('keyup'), false)} />

<!-- View icons in the rail (`VIEW_ICONS`). -->
{#snippet renameInput(v: View)}
  <!-- svelte-ignore a11y_autofocus -->
  <input class="rename" value={v.label} autofocus aria-label={t('Page name')} onfocus={(e) => e.currentTarget.select()}
    onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') renamePage(v.id, e.currentTarget.value); else if (e.key === 'Escape') pageUi.renaming = null }}
    onblur={(e) => { if (pageUi.renaming?.id === v.id) renamePage(v.id, e.currentTarget.value) }} />
{/snippet}
{#snippet icon(id: string)}<ViewIcon {id} />{/snippet}

<!--
  The header of every page: its title on its color, the filter, the stats. The Agents view has
  the same one (the filter applies to its agents), without what concerns a page of blocks.
-->
{#snippet pageHeader(agents: boolean, preview?: Snippet<[BlockId]>)}
  <header style:--view={agents ? 'var(--src-claude)' : undefined} class:folded={!headerShown}>
    {#if !agents && editing.on}
      <!-- Edit layout: the page's icon and color, and its name, changed in place. -->
      <span class="title editable">
        <button class="look" onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); startStyling(view.id, r.left, r.bottom + 4) }} title={t('Icon and color…')} aria-label={t('Icon and color…')}>{@render icon(look(view).icon)}</button>
        {#if pageUi.renaming?.id === view.id && pageUi.renaming.at === 'header'}
          {@render renameInput(view)}
        {:else}
          <button class="name" onclick={() => startRename(view.id, 'header')} title={t('Rename (F2)')}>{t(view.label)}</button>
        {/if}
        {#if view.origin}<span class="packmark" class:changed={view.origin === 'copy'} title={view.origin === 'pack' ? t('A page of a pack, kept up to date with it') : t('A page of a pack, changed here: it no longer follows the pack')}>{view.origin === 'pack' ? t('pack') : t('pack, changed')}</span>{/if}
      </span>
    {:else}
    <span class="title">{#if agents}{@render icon('agents')}{t('Agents')}{:else}{@render icon(look(view).icon)}{t(view.label)}{/if}</span>
    {/if}
    <div class="filter" class:active={!!focus}>
      <input
        bind:this={input}
        value={filter.query}
        oninput={(e) => setQuery(e.currentTarget.value)}
        onblur={() => (filterPeek = false)}
        placeholder={t('Filter: {examples}!1957, text…  ( / )', { examples: hasTickets() ? `${ticketExample(4688)}, ` : '' })}
        spellcheck="false"
      />
      {#if focus}
        <span class="match" class:none={focus.summary === 'no match'}>{focus.summary === 'no match' ? t('no match') : focus.summary}</span>
        <button onclick={() => setQuery('')} aria-label={t('Clear filter')}><Icon name="close" /></button>
      {/if}
    </div>
    {#if !agents}
      <button class="mine" class:on={filter.mine} onclick={toggleMine} aria-pressed={filter.mine} title={t('Only my MRs and my tickets (m)')}>{t('Mine')}</button>
      <DatePicker />
      <LayoutMenu preview={snap ? preview : undefined} />
    {/if}
    {#if snap}
      <HeaderStats {managers} {jobs} failures={protectedFailures} {reviews} {waiting} {syncs} />
    {/if}
    {#if full && full.tracker !== null && full.tracker_features?.time !== false}<TimeTracker snap={full} />{/if}
  </header>
{/snippet}

<div class="app" class:zen={zen.on} class:machine={!!machine()} class:machine-away={!!(machine() || profileWindow()) && !machineState.connected && !!machineState.since && clock(1000).now - machineState.since >= GRACE} style:--view={look(view).color}>
  <div class="shell">
  <!-- Two fingers up or down on the rail: the next or previous page, while one is shown. -->
  <nav class="rail" aria-label={t('Views')} {@attach pageSwipe(stepView, () => appearance().page_swipe && !editing.on && !zoomed && !agentsView.open && !projectsView.open && !settingsView.open && !helpView.open)}>
    <span class="edge" aria-hidden="true"></span>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <span class="brand" title="Prism" onclick={logoClick}>{#key rainbow}<span class:spin={rainbow > 0}><PrismLogo size={30} animated pulse={live.snapshot?.updated_at} /></span>{/key}</span>
    {#key rainbow}{#if rainbow}<span class="rainbow" aria-hidden="true"></span>{/if}{/key}
    {#each views() as v, i (v.id)}
      {@const lk = look(v)}
      <div class="pg" class:drop={dropAt === i} class:drop-after={dropAt === i + 1 && i === views().length - 1} class:dragged={draggedPage === v.id}
        ondragover={(e) => { if (draggedPage) { e.preventDefault(); dropAt = dropSlot(e, i) } }}
        ondragleave={() => { if (dropAt === i || dropAt === i + 1) dropAt = null }}
        ondrop={(e) => { e.preventDefault(); dropPage(dropSlot(e, i)) }}
        role="listitem">
      {#if pageUi.renaming?.id === v.id && pageUi.renaming.at === 'rail'}
        <span class="v on renaming" style:--tint={lk.color}>
          {@render icon(lk.icon)}
          {@render renameInput(v)}
        </span>
      {:else}
      <button class="v" class:on={v.id === view.id && !settingsView.open && !agentsView.open && !projectsView.open && !helpView.open} style:--tint={lk.color} onclick={() => { closeSettings(); closeHelp(); setView(v.id) }} title="{t(v.label)} ({keys(`⌘${i + 1}`)}){v.origin === 'pack' ? ` · ${t('pack')}` : ''}" aria-current={v.id === view.id ? 'page' : undefined}
        draggable="true" ondragstart={(e) => { draggedPage = v.id; e.dataTransfer?.setData('text/plain', v.label) }} ondragend={() => { draggedPage = null; dropAt = null }}
        oncontextmenu={(e) => { e.preventDefault(); pageUi.menu = { id: v.id, x: e.clientX, y: e.clientY } }}>
        {@render icon(lk.icon)}
        <span>{t(v.label)}</span>
        {#if viewAlert(v)}<span class="alert-dot" aria-label={t('Pod down')}></span>{/if}
        {#each blockBadges(v) as b (b.title)}<span class="count ko" title={b.title}>{b.count}</span>{/each}
        {#if columnsOf(v).flat().includes('worktrees') && staleWorktrees}<span class="count stale" title={tn(staleWorktrees, '{n} stale worktree', '{n} stale worktrees')}>{staleWorktrees}</span>{/if}
        {#if editing.on && v.origin}<span class="packmark" class:changed={v.origin === 'copy'} title={v.origin === 'pack' ? t('A page of a pack, kept up to date with it') : t('A page of a pack, changed here: it no longer follows the pack')}>{t('pack')}</span>{/if}
      </button>
      {#if editing.on}<button class="pmore" onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); pageUi.menu = { id: v.id, x: r.right + 4, y: r.top } }} aria-label={t('{page}: its menu', { page: t(v.label) })} title={t('Rename, icon and color, duplicate, move, hide, delete')}><Icon name="more" /></button>{/if}
      {/if}
      </div>
    {/each}
    {#if editing.on}
      <button class="v add" onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); pageUi.adding = { x: r.right + 4, y: r.top } }} title={t('A new page: empty, a copy of this one, or a template’s')} aria-label={t('New page')}><Icon name="plus" /><span>{t('Page')}</span></button>
    {/if}
    {#if hiddenViews().length}
      <button class="hiddenp" onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); pageUi.hiddenMenu = { x: r.right + 4, y: r.top } }} title={tn(hiddenViews().length, '{n} hidden page: show it again', '{n} hidden pages: show them again')} aria-label={t('Hidden pages')}><Icon name="hide" />{hiddenViews().length}</button>
    {/if}
    {#if agentsEnabled()}
    <button class="v" class:on={agentsView.open && !settingsView.open && !helpView.open} style:--tint="var(--src-claude)" onclick={() => openAgents()} title={keys(t('Agents (⌘{digit}) · new agent ⌘N', { digit: agentsDigit }))} aria-current={agentsView.open ? 'page' : undefined}>
      {@render icon('agents')}
      <span>{t('Agents')}</span>
      {#if agentsWaiting}<span class="count" title={t('{n} waiting for you', { n: agentsWaiting })}>{agentsWaiting}</span>{:else if toReview}<span class="count rv" title={t('{n} to review', { n: toReview })}>{toReview}</span>{/if}
    </button>
    {/if}
    {#if projectsOn}
    <button class="v" class:on={projectsView.open && !settingsView.open && !helpView.open} style:--tint="var(--src-gitlab)" onclick={() => openProjects()} title={keys(t('Projects (⌘{digit}): each repository and its workstreams', { digit: projectsKey }))} aria-current={projectsView.open ? 'page' : undefined}>
      {@render icon('worktrees')}
      <span>{t('Projects')}</span>
      {#if workstreamsWaiting}<span class="count" title={tn(workstreamsWaiting, '{n} workstream waits for you', '{n} workstreams wait for you')}>{workstreamsWaiting}</span>{:else if staleOnProjects}<span class="count stale" title={tn(staleOnProjects, '{n} stale worktree', '{n} stale worktrees')}>{staleOnProjects}</span>{/if}
    </button>
    {/if}
    {#if assistantOn()}
    <button class="v" class:on={assistantView.open} style:--tint="var(--info)" onclick={toggleAssistant} title={t('Assistant: ask Prism in your own words (or from the palette, ⌥↵)')} aria-pressed={assistantView.open}><Icon name="sparkles" /><span>{t('Assistant')}</span>{#if assistantWaiting}<span class="count" title={t('{n} proposals to validate', { n: assistantWaiting })}>{assistantWaiting}</span>{/if}</button>
    {/if}
    <span class="grow"></span>
    <button class="v" class:on={helpView.open} onclick={toggleHelp} title={keys(t('Help (⌘? or F1)'))} aria-pressed={helpView.open}><Icon name="help" /><span>{t('Help')}</span></button>
    <ThemeMenu />
    <button class="v settings" class:on={settingsView.open} onclick={toggleSettings} title={keys(t('Settings (⌘,)'))} aria-pressed={settingsView.open}><Icon name="settings" /><span>{t('Settings')}</span></button>
    <ProfileMenu />
  </nav>
  <div class="content">
    {#if machine() || profileWindow()}<MachineBar />{/if}
    <!--
      The Agents view stays mounted once opened, hidden elsewhere: its terminals stay
      attached, and coming back does not re-attach (which added a line to the prompt).
    -->
    {#if agentsView.open && agentsEnabled() && !settingsView.open && !helpView.open}{@render pageHeader(true)}{/if}
    {#if agentsEnabled() && agentsShown}
    <div class="agents-host" hidden={!agentsView.open || settingsView.open || helpView.open}><AgentsView /></div>
    {/if}
    {#if helpView.open}
    <Help />
    {:else if settingsView.open}
    <Settings />
    {:else if agentsView.open && agentsEnabled()}
    <!-- The Agents view, above. -->
    {:else if projectsView.open && projectsOn}
    <ProjectsView />
    {:else}
    {#if !tour.seen && !wizard.open && !clonesPrompt.open}
      <!-- First launch: an invitation to the Welcome guide, until Help is opened or this is dismissed. -->
      <div class="tour" role="note">
        <Icon name="help" /><span><b>{t('New to Prism?')}</b> {t('Take the 3-minute tour: what each page shows, and the keys that matter.')}</span>
        <button class="go" onclick={() => openHelp('welcome')}>{t('Take the tour')}</button>
        <button class="x" onclick={seeTour} aria-label={tc('core', 'Dismiss')} title={keys(t('Dismiss (Help stays in the rail, ⌘?)'))}><Icon name="close" /></button>
      </div>
    {/if}
    {@render pageHeader(false, block)}

  {#snippet block(id: BlockId)}
    {#if id === 'todo'}<Alerts {alerts} />
    {:else if id === 'sessions'}<Sessions snap={snap!} />
    {:else if id === 'worktrees'}<Worktrees snap={full!} />
    {:else if id === 'notes'}<Notes snap={full} />
    {:else if id === 'mrs'}<MergeRequests snap={snap!} />
    {:else if id === 'queue'}<MergeQueue snap={snap!} />
    {:else if id === 'sprint'}<Sprint sprint={snap!.sprint} next={snap!.next_sprint} outside={snap!.outside_sprint} me={snap!.tracker_me} filtered={!!focus} full={full!} />
    {:else if id === 'retro'}<MySprint snap={full!} />
    {:else if id === 'wip'}<InProgress snap={full!} />
    {:else if id === 'routines'}<RoutinesBlock />
    {:else if id === 'aside'}<AsidesBlock />
    {:else if id === 'pickup'}<Pickup pickup={snap!.pickup} me={snap!.tracker_me} filtered={!!focus || filter.mine} />
    {:else if id === 'agenda'}<Agenda agenda={full!.agenda} />
    {:else if id === 'refinement'}<Refinement refinement={snap!.refinement} me={snap!.tracker_me} filtered={!!focus || filter.mine} />
    {:else if id === 'pods'}<Pods pods={snap!.pods} usage={snap!.pod_usage} nodes={full!.pod_nodes ?? []} />
    {:else if id === 'renovate' && full!.features?.renovate === false}<BlockOff {id} title="Renovate" logo="gitlab" color="var(--src-gitlab)" section="renovate" setting="[renovate] enabled" reads="renovate" />
    {:else if id === 'renovate'}<Renovate renovate={full!.renovate} merge_requests={full!.merge_requests} error={full!.errors.renovate} />
    {:else if id === 'argocd'}<ArgoCd argocd={full!.argocd} environments={full!.environments} error={full!.errors.argocd} />
    {:else if id === 'envs'}<Environments environments={full!.environments} argocd={full!.argocd} error={full!.errors.environments} />
    {:else if id === 'vulns' && full!.features?.vulnerabilities === false}<BlockOff {id} title="Vulnerabilities" logo="alert" color="var(--src-alert)" section="vulns" setting="[vulnerabilities] enabled" reads="vulnerabilities" />
    {:else if id === 'vulns'}<Vulnerabilities vulns={full!.vulnerabilities} error={full!.errors.vulnerabilities} />
    {:else if id === 'mail'}<Mail mail={full!.mail} enabled={!!full!.mail} error={full!.errors.mail} />
    {:else if id === 'ci' && full!.features?.ci_health === false}<BlockOff {id} title="CI health" logo="gitlab" color="var(--src-gitlab)" section="forge" setting="[forge] ci_health" reads="ci_health" />
    {:else if id === 'ci'}<CiHealth health={full!.ci_health} error={full!.errors.ci_health} />
    {:else if isCustom(id)}<CustomBlock block={customBlock(full, id.slice('custom:'.length))} pageId={id} />
    {:else if id in BLOCK_COMPONENTS || id in OWNER}{@const def = BLOCK_DEFS.find((b) => b.id === id)}{#if def}{@const Block = BLOCK_COMPONENTS[id]}<ModuleBoundary {id} active={id in OWNER} missing={id in BLOCK_COMPONENTS ? undefined : (moduleFailure(id) ?? '')}>{#if def.off && stateOf(def, full) === undefined}<BlockOff {id} {...def.off} />{:else if Block}<Block state={stateOf(def, full) as never} error={full!.errors[def.source ?? id]} snap={full!} />{/if}</ModuleBoundary>{/if}
    {/if}
  {/snippet}

    {#if snap && zoomed}
      <main><div class="col" style="flex: 1 1 0"><div data-block={zoomed} style="flex: 1 1 0">{@render block(zoomed)}</div></div></main>
    {:else if snap}
      <!-- Grid of the view: its layout (views.ts, or modified in "Edit layout"), proportions in layout; its bands top to bottom. -->
      <main class:editing={editing.on} class:xray>
        {#each bands as [start, end], b (`${view.id}:band${b}`)}
          {#if b > 0}
            {#if fitted(b - 1) || fitted(b)}<div class="band-gap"></div>{:else}<Gutter dir="y" onreset={resetLayout} onresize={(d, px) => shift(sizes.bands ?? [], b - 1, d, px)} />{/if}
          {/if}
          {#if editing.on && bands.length > 1}
            <div class="band-bar">
              <span>{t('Band {n}', { n: b + 1 })}</span>
              <button onclick={() => moveBand(b, -1)} disabled={b === 0} title={t('Move the band up')}><Icon name="up" /></button>
              <button onclick={() => moveBand(b, 1)} disabled={b === bands.length - 1} title={t('Move the band down')}><Icon name="down" /></button>
              <button onclick={() => addColumn(b)} disabled={end - start >= MAX_COLUMNS} title={t('Add an empty column (up to {max})', { max: MAX_COLUMNS })}>{t('+ Column')}</button>
            </div>
          {/if}
          <div class="band" style={fitted(b) ? 'flex: 0 0 auto' : `flex: ${bands.length > 1 ? (sizes.bands?.[b] ?? 1) : 1} 1 0`}>
            {#each columns.slice(start, end) as column, i (`${view.id}:${start + i}`)}
              {@const c = start + i}
              {#if i > 0}<Gutter dir="x" onreset={resetLayout} onresize={(d, px) => shift(sizes.columns, c - 1, d, px)} />{/if}
              <div
                class="col"
                class:over={over === c}
                style="flex: {sizes.columns[c] ?? 1} 1 0"
                role="list"
                ondragover={(e) => { if (dragged) { e.preventDefault(); over = c } }}
                ondragleave={(e) => { if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) over = null }}
                ondrop={(e) => drop(e, c)}
              >
                {#each column as id, r (id)}
                  {#if r > 0 && both(column[r - 1], id)}<Gutter dir="y" onreset={resetLayout} onresize={(d, px) => shift(sizes.rows[c], r - 1, d, px)} />{/if}
                  <div
                    data-block={id}
                    data-xray={xrayLabel(id)}
                    role="listitem"
                    class:dragging={dragged === id}
                    draggable={editing.on}
                    ondragstart={(e) => { dragged = id; e.dataTransfer?.setData('text/plain', id); tilt(e) }}
                    ondragend={() => { dragged = null; over = null }}
                    style={column.length === 1 && modeOf(id) === 'normal' ? 'flex: 1 1 0' : grow(id, sizes.rows[c]?.[r] ?? 1)}
                  >{@render block(id)}</div>
                {:else}
                  <div class="drop">{editing.on ? t('Drop a block here') : t('Empty page: Edit layout, then + Block')}</div>
                {/each}
              </div>
            {/each}
          </div>
        {/each}
      </main>
    {:else}
      <p class="muted" style="padding: 16px">{t('Connecting to the server…')}</p>
    {/if}
    {/if}
  </div>
  </div>
  <StatusBar snap={full} {now} {status} alerts={allAlerts} />
  <Toast />
  <PageMenus />
  <Tooltip />
  <ContextMenu />
  <SelectMenu />
  <Confetti />
  <Console />
  <QuickNotes />
  <Feedback />
  <AgentSwitch />
  <Palette />
  <Screensaver />
  <ThemeScene />
  <Boot />
  <FirstLaunch />
  {#if wizard.open}<PagesWizard />{/if}
  {#if clonesPrompt.open}<ClonesPrompt />{/if}
  {#if newTicketDialog.open}<NewTicketDialog />{/if}
  <!-- Before the other drawers: its Escape is heard first. -->
  {#if assistantView.open}<AssistantPanel />{/if}
  <LauncherDrawer />
  <!-- Create a quick action… from a session: loaded the first time it opens. -->
  {#if qaDraft.open}{#await import('./components/QuickActionDraft.svelte') then { default: QuickActionDraft }}<QuickActionDraft />{/await}{/if}
  <MrPanel />
  <MrChoice />
  <TicketPanel />
  <LogPanel />
  <PodLogPanel />
  <EventPanel />
  <ThreadPanel />
  <ArgoPanel />
  <OverlapPanel />
  <ChangesDrawer />
  <McpProposal />
  <ShellDrawer />
  <KeyHelp />
</div>

<style>
  /*
   * The views rail on the left; on the right, the header then the grid of blocks; the status bar
   * below, across the full width. Each view has its color (`--view`, views.ts): its tab,
   * the title and the header gradient. A stripe in the logo's colors runs along the rail.
   */
  .app { height: 100%; display: flex; flex-direction: column; }
  .shell { flex: 1; min-height: 0; display: flex; }
  .content { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .rail { position: relative; flex: 0 0 64px; display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 10px 0 10px; background: var(--chrome-bg); border-right: 1px solid var(--border); }
  /* The chrome's glass (`--chrome-blur`) on a layer behind it, not on itself: a backdrop-filter would
     make it the containing block and the stacking context of its fixed menus (theme, profile…), which
     then open under the blocks. */
  .rail::before, header::before { content: ''; position: absolute; inset: 0; z-index: -1; backdrop-filter: var(--chrome-blur); -webkit-backdrop-filter: var(--chrome-blur); pointer-events: none; }
  /* The edge's colours on a layer of their own, which a theme's motion slides (app.css). */
  .rail .edge { position: absolute; top: 0; right: -1px; bottom: 0; width: var(--rail-edge-w); overflow: hidden; opacity: 0.85; pointer-events: none; }
  .rail .edge::before { content: ''; position: absolute; inset: 0; background: var(--rail-edge); }
  /* Another computer's Prism (`machine`): its rail's edge in one color, never mistaken for this one's. */
  .app.machine .rail .edge { width: 3px; }
  .app.machine .rail .edge::before { background: var(--info); }
  /* Its machine out of reach: the page is its last state, dimmed (its bar says why). */
  .app.machine-away .content > :not(:global(.machine-bar)) { opacity: 0.6; filter: grayscale(0.6); transition: opacity 0.3s, filter 0.3s; }
  .rail .brand { display: flex; margin-bottom: 10px; }
  .rail .brand > span { display: flex; }
  .rail .brand .spin { animation: logo-spin 0.9s cubic-bezier(0.2, 0.8, 0.3, 1.2); }
  @keyframes logo-spin { 50% { transform: rotate(180deg) scale(1.25); } to { transform: rotate(360deg); } }
  /* Rainbow along the rail, 1.4 s. */
  .rainbow { position: absolute; left: 0; top: 46px; bottom: 0; width: 100%; z-index: 1; pointer-events: none; background: linear-gradient(#ff5f6d, #ffc371, #fff36e, #7cf29a, #5ec8ff, #a78bfa); mix-blend-mode: screen; mask: linear-gradient(transparent, #000 10%, #000 60%, transparent); animation: rainbow 1.6s ease-in-out forwards; }
  @keyframes rainbow { 0% { opacity: 0; transform: scaleY(0); transform-origin: top; } 25% { opacity: 0.55; transform: scaleY(1); } 70% { opacity: 0.55; } 100% { opacity: 0; } }
  /* X-rays (⌥): each block shows its identifier and its source. */
  main.xray [data-block] { position: relative; outline: 1px dashed var(--accent); outline-offset: -1px; border-radius: var(--r10); }
  main.xray [data-block]::before { content: attr(data-xray); position: absolute; right: 8px; top: 6px; z-index: 5; max-width: 70%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 1px 7px; border-radius: var(--r5); background: color-mix(in srgb, var(--accent) 18%, var(--surface)); color: var(--accent); font: 10.5px var(--mono); pointer-events: none; }
  .rail :global(.v) { position: relative; display: flex; flex-direction: column; align-items: center; gap: 3px; width: 56px; padding: 6px 0 5px; border: none; border-radius: var(--r8); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-micro); line-height: 1.1; cursor: pointer; }
  .rail :global(.v:hover) { color: var(--text); background: var(--surface-muted); }
  .rail :global(.v svg) { width: 19px; height: 19px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
  /* Icons tinted with their view's color, even inactive; the active one has a stroke on the left. */
  .rail .v :global(svg) { color: color-mix(in srgb, var(--tint) 80%, var(--text-muted)); }
  .rail .v.on { color: var(--tint); background: color-mix(in srgb, var(--tint) var(--rail-active), transparent); font-weight: 600; }
  .rail .v.on :global(svg) { color: var(--tint); }
  .rail .v.on::before { content: ''; position: absolute; left: -4px; top: 8px; bottom: 8px; width: var(--rail-mark); border-radius: 0 var(--r3) var(--r3) 0; background: var(--tint); }
  /* Theme button (ThemeMenu) open. */
  .rail :global(.v.on) { color: var(--accent); background: color-mix(in srgb, var(--accent) var(--rail-active), transparent); }
  .rail :global(.v span) { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rail .grow { flex: 1; }
  /* Managing the pages from the rail: drag and drop, the name typed in place, ⋯ and « + » in Edit layout. */
  .rail .pg { position: relative; display: flex; flex-direction: column; align-items: center; }
  .rail .pg.drop::before, .rail .pg.drop-after::after { content: ''; position: absolute; top: -3px; left: 8px; right: 8px; height: 2px; border-radius: var(--r1); background: var(--accent); }
  .rail .pg.drop-after::after { top: auto; bottom: -3px; }
  .rail .pg.dragged { opacity: 0.45; }
  .rail .v.renaming { cursor: default; }
  .rail .rename { width: 54px; padding: 1px 3px; border: 1px solid var(--accent); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-micro); text-align: center; }
  .rail .pmore { position: absolute; top: 0; right: -2px; display: grid; place-items: center; width: 16px; height: 16px; padding: 0; border: 1px solid var(--border); border-radius: 50%; background: var(--surface); color: var(--text-muted); cursor: pointer; }
  .rail .pmore :global(svg) { width: 11px; height: 11px; }
  .rail .pmore:hover { color: var(--text); }
  .rail .packmark { position: absolute; bottom: -2px; padding: 0 3px; border-radius: var(--r3); background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-micro); font-weight: 600; letter-spacing: 0.2px; }
  .rail .packmark.changed { color: var(--warn); }
  .rail .v.add { border: 1px dashed var(--border); }
  .rail .hiddenp { display: inline-flex; align-items: center; gap: 3px; margin-top: 2px; padding: 2px 6px; border: none; border-radius: var(--r8); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .rail .hiddenp:hover { background: var(--surface-muted); color: var(--text); }
  .rail .hiddenp :global(svg) { width: 13px; height: 13px; }
  /* Zen mode: the rail and the header fold away (the header comes back for the filter). */
  .app.zen .rail, header.folded { display: none; }
  /* A page as the others: it slides in and out as they do (`pageTransition`). */
  .agents-host { flex: 1; min-height: 0; display: flex; view-transition-name: view-main; }
  .agents-host[hidden] { display: none; }
  .rail .count { position: absolute; top: 2px; right: 9px; min-width: 15px; height: 15px; padding: 0 4px; border-radius: var(--r8); background: var(--src-claude); color: #fff; font-size: var(--fs-micro); font-weight: 700; line-height: 15px; text-align: center; }
  .rail .count.stale { background: var(--warn-bg); color: var(--warn); }
  .rail .count.rv { background: var(--surface); color: var(--src-claude); box-shadow: inset 0 0 0 1px var(--src-claude); }
  .rail .count.ko { background: var(--ko-bg); color: var(--ko); }
  .alert-dot { position: absolute; top: 4px; right: 12px; width: 8px; height: 8px; border-radius: 50%; background: var(--ko); box-shadow: 0 0 0 2px var(--surface); }
  .tour { display: flex; align-items: center; gap: 10px; padding: 7px 12px; background: color-mix(in srgb, var(--accent) 10%, var(--surface)); border-bottom: 1px solid var(--border); color: var(--text); font-size: var(--fs-body); }
  .tour > :global(svg) { width: 15px; height: 15px; flex-shrink: 0; color: var(--accent); }
  .tour span { flex: 1; min-width: 0; }
  .tour .go { flex-shrink: 0; padding: 3px 10px; border: none; border-radius: var(--r6); background: var(--accent); background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--accent); color: var(--on-accent); font: inherit; font-weight: 600; text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); cursor: pointer; }
  .tour .x { display: inline-flex; flex-shrink: 0; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  header { position: relative; display: flex; align-items: center; gap: 10px; padding: 8px 12px; background: linear-gradient(90deg, color-mix(in srgb, var(--view) 11%, var(--chrome-bg)), var(--chrome-bg) 45%); }
  header::after { content: ''; position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: linear-gradient(90deg, var(--view), color-mix(in srgb, var(--view) 25%, transparent) 40%, transparent 75%), var(--border); background-size: 100% 2px, 100% 1px; background-position: 0 0, 0 100%; background-repeat: no-repeat; pointer-events: none; }
  .title { view-transition-name: view-title; display: flex; align-items: center; gap: 6px; flex-shrink: 0; color: var(--view); font-size: var(--fs-title); font-weight: 600; white-space: nowrap; }
  .title.editable { gap: 4px; }
  .title .look, .title .name { display: inline-flex; align-items: center; padding: 2px 5px; border: 1px dashed color-mix(in srgb, var(--view) 50%, var(--border)); border-radius: var(--r6); background: none; color: inherit; font: inherit; cursor: pointer; }
  .title .look:hover, .title .name:hover { background: color-mix(in srgb, var(--view) 10%, transparent); }
  .title .rename { width: 160px; padding: 1px 6px; border: 1px solid var(--view); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; }
  .title .packmark { padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r8); color: var(--text-muted); font-size: var(--fs-tag); font-weight: 500; }
  .title .packmark.changed { color: var(--warn); }
  .title :global(svg) { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
  .filter { display: flex; align-items: center; gap: 6px; padding: 0 6px 0 10px; height: 30px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); flex: 0 1 360px; min-width: 160px; }
  .filter.active { border-color: var(--info); }
  .filter input { flex: 1; min-width: 0; border: none; outline: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-body); }
  .filter .match { font-size: var(--fs-meta); color: var(--info); white-space: nowrap; }
  .filter .match.none { color: var(--warn); }
  .mine { flex-shrink: 0; font: inherit; font-size: var(--fs-body); padding: 3px 10px; border-radius: var(--r6); border: 1px solid var(--border); background: none; color: var(--text-muted); cursor: pointer; }
  .mine.on { background: var(--info-bg); color: var(--info); border-color: transparent; font-weight: 500; }
  .filter button { display: inline-flex; align-items: center; border: none; background: none; color: var(--text-muted); cursor: pointer; font-size: var(--fs-title); line-height: 1; padding: 0 2px; }
  /* Proportions come from layout.svelte.ts; the handles act as spacing. */
  main { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 10px; view-transition-name: view-main; }
  .band { display: flex; min-height: 0; }
  /* Between two bands without a handle (one takes its content's height), the handle's gap. */
  .band-gap { flex: 0 0 10px; }
  /* Edit layout, several bands: each one's name and its moves above it. */
  .band-bar { display: flex; align-items: center; gap: 2px; margin: 0 0 4px; font-size: var(--fs-tag); color: var(--accent); }
  .band-bar span { margin-right: 6px; font-weight: 500; }
  .band-bar button { display: inline-flex; align-items: center; gap: 3px; height: 20px; padding: 0 6px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; cursor: pointer; }
  .band-bar button:hover:not(:disabled) { border-color: var(--border); color: var(--text); }
  .band-bar button:disabled { opacity: 0.35; cursor: default; }
  .col { display: flex; flex-direction: column; min-height: 0; min-width: 0; }
  .col > div { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
  .col > div > :global(.panel) { flex: 1; }
  /* Without a handle between them (a short or collapsed block), two blocks keep the handle's gap. */
  main:not(.editing) .col > [data-block] + [data-block] { margin-top: 10px; }
  /* Free layout: target columns, moved block faded. */
  main.editing .col { gap: 8px; padding: 4px; margin: -4px 0; border-radius: var(--r10); outline: 1px dashed color-mix(in srgb, var(--accent) 45%, transparent); outline-offset: -1px; }
  main.editing [data-block] { cursor: grab; }
  main.editing .col.over { background: color-mix(in srgb, var(--accent) 8%, transparent); }
  .col > .dragging { opacity: 0.4; }
  .drop { flex: 1; min-height: 80px; display: flex; align-items: center; justify-content: center; color: var(--text-muted); font-size: var(--fs-meta); }
  /* Narrow window: the header tightens (the stats lose their labels, see HeaderStats). */
  @media (max-width: 1250px) {
    .title { display: none; }
  }
  @media (max-width: 1100px) {
    .app { height: auto; min-height: 100%; }
    .rail { position: sticky; top: 0; height: calc(100vh - 26px); }
    .app > :global(.statusbar) { position: sticky; bottom: 0; z-index: 5; }
    .band { flex-direction: column; flex: none !important; }
    .col > div { height: 60vh; flex: none !important; }
    main > :global(.gutter), .band > :global(.gutter), .col > :global(.gutter) { display: none; }
  }
</style>
