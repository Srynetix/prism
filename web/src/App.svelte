<script lang="ts">
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
  import { alerts as computeAlerts, awaitsMyReview, countedSessions, describeWorktrees, podDown, waitingAlert, waitsForYou } from './rules'
  import { alertIsMine, alertMatches, apply, applyMine, filter, mineFocus, resolve, setQuery, toggleMine } from './filter.svelte'
  import Alerts from './components/Alerts.svelte'
  import { addColumn, bandsOf, columnsOf, currentView, editing, ensureSizes, showBlock, modeOf, moveBand, moveBlock, resetLayout, setView, shift, sizesOf, stepView, toggleEditing, toggleZen, zen, zoom } from './layout.svelte'
  import LayoutMenu from './components/LayoutMenu.svelte'
  import { MAX_COLUMNS, bandRanges, views, type BlockId, type View } from './views'
  import { BLOCK_COMPONENTS } from './blocks/components'
  import { BLOCK_DEFS } from './blocks'
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
  import { markDock } from './dock'
  import ProfileSwitch from './components/ProfileSwitch.svelte'
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
  import ShellDrawer from './components/ShellDrawer.svelte'
  import MrPanel from './components/MrPanel.svelte'
  import LauncherDrawer from './components/LauncherDrawer.svelte'
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
  import { agentsEnabled, agentsView, closeAgents, openAgents, terminalKeeps } from './agents.svelte'
  import { BLOCK_HELP } from './block-help'
  import { chord, chordKey, extraShift, inTerminal, isMac, keys, pageDigit } from './platform'
  import { isDesktop, machine } from './bridge'
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
  import { isHidden } from './worktree-archive.svelte'

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
  $effect(() => void markDock(live.snapshot?.profile))
  const focus = $derived(full ? resolve(full, filter.query) : null)
  /** All blocks read the filtered view; alerts are computed on the full state then filtered. */
  const snap = $derived(full ? apply(filter.mine ? applyMine(full) : full, focus) : null)
  /** Every alert of the full state, computed once per snapshot for the view and the menu bar. */
  const allAlerts = $derived(full ? computeAlerts(full) : [])
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
  // A page shown counts its blocks as used (Settings › Cleanup), once each time it comes up (its
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
  const typing = (e: KeyboardEvent) => !!(e.target as HTMLElement | null)?.closest?.('input, textarea, select, [contenteditable]')
  /** The Agents view comes after the grid views: ⌘5 with four views. */
  const agentsDigit = $derived(String(views().length + 1))

  /** Zen mode: the filter shown while `/` puts the cursor in it, until it loses it. */
  let filterPeek = $state(false)
  /** In Zen mode, the header is back while the filter or the For me panel is in use. */
  const headerShown = $derived(!zen.on || !!filter.query || filterPeek || !!headerStats.open)

  function onKey(e: KeyboardEvent) {
    // A shortcut an agent's terminal keeps (`[agents] terminal_keys`): Prism leaves it there, and
    // ignores the menu bar's echo of it (⌘N, ⌘,).
    if (inTerminal(e) && terminalKeeps(e)) {
      const echo = chord(e) ? ({ n: 'new-agent', ',': 'settings' } as Record<string, string>)[chordKey(e)] : undefined
      if (echo) keyedAction(echo)
      else if (chord(e) && e.shiftKey && chordKey(e).toLowerCase() === 'f') keyedAction('zen')
      return
    }
    // The command palette (`[palette] key`, ⌘K), over anything.
    if (paletteOn() && matches(e, paletteKey()) && (worksWhileTyping(paletteKey()) || !typing(e))) {
      e.preventDefault()
      togglePalette()
      return
    }
    // ⌘⇧F: Zen mode, the rail and the header folded away.
    if (chord(e) && e.shiftKey && chordKey(e).toLowerCase() === 'f') {
      e.preventDefault()
      keyedAction('zen')
      toggleZen()
      if (zen.on) toast(t('Zen mode · {keys} to leave it', { keys: keys('⌘⇧F') }))
      return
    }
    // ⌘, : Settings, as in any macOS app.
    if (chord(e) && chordKey(e) === ',') {
      e.preventDefault()
      keyedAction('settings')
      closeAgents()
      toggleSettings()
      return
    }
    // Ctrl+Q and F11 where no menu bar does it (Linux): quit, full screen.
    if (!isMac && isDesktop() && chord(e) && chordKey(e) === 'q') {
      e.preventDefault()
      void quitApp()
      return
    }
    if (!isMac && isDesktop() && e.key === 'F11') {
      e.preventDefault()
      void toggleFullscreen()
      return
    }
    // ⌘? or F1: Help.
    if ((chord(e) && chordKey(e) === '?') || e.key === 'F1') {
      e.preventDefault()
      toggleHelp()
      return
    }
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
    if (chord(e) && chordKey(e) === 'n' && !(e.shiftKey && !extraShift(e)) && agentsEnabled()) {
      e.preventDefault()
      keyedAction('new-agent')
      openAgents()
      agentsView.creating = true
      agentsView.desktopHref = null
      return
    }
    // ⌘⇧T: the panel closed last comes back (recent-panels.svelte.ts).
    if (chord(e) && e.shiftKey && chordKey(e) === 't') {
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
    // "/" puts the cursor in the filter, Esc clears it.
    if (e.key === '/' && !typing(e)) {
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
      const state = b.badge && shown.includes(b.id as BlockId) ? stateOf(b, full) : undefined
      const badge = state !== undefined && b.badge!(state as never)
      if (badge && badge.count) sources.set(b.source ?? b.id, badge)
    }
    return [...sources.values()]
  }
  const agentsWaiting = $derived(full?.agents?.filter((a) => waitsForYou(a.status)).length ?? 0)
  const syncs = $derived(alerts.filter((a) => a.kind === 'Sync' && !isSeen(a)))
  const ago = $derived(snap?.updated_at ? Math.max(0, Math.round((now - new Date(snap.updated_at).getTime()) / 1000)) : null)
  /** The live dot's tooltip: connected, and the age of the snapshot. */
  const status = $derived(!live.connected ? t('Reconnecting…') : ago !== null ? t('Live · {age} ago', { age: ago < 60 ? `${ago}s` : `${Math.floor(ago / 60)}m` }) : t('Live'))
</script>

<svelte:window onpointerdown={followPointer} onkeydown={(e) => { onAlt(e, true); onKey(e) }} onkeyup={(e) => onAlt(e, false)} onblur={() => onAlt(new KeyboardEvent('keyup'), false)} />

<!-- View icons in the rail (`VIEW_ICONS`); an unknown icon takes the dashboard's. -->
{#snippet icon(id: string)}
  <svg viewBox="0 0 24 24" aria-hidden="true">
    {#if id === 'tickets'}<path d="M4 6h16M4 12h11M4 18h7" /><path d="m15.5 19.5 1-3.5 4-4 2.5 2.5-4 4z" />
    {:else if id === 'notes'}<path d="M5 3.5h10l4 4V20.5H5z" /><path d="M14.5 3.5v4.5H19M8.5 12h7M8.5 16h5" />
    {:else if id === 'chart'}<path d="M4 20V4M4 20h16" /><path d="M8 16v-4M12 16V8M16 16v-6" />
    {:else if id === 'star'}<path d="m12 3.2 2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17.2l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z" />
    {:else if id === 'calendar'}<rect x="3.5" y="5" width="17" height="15.5" rx="2" /><path d="M3.5 10h17M8 3v4M16 3v4" />
    {:else if id === 'shield'}<path d="M12 3 4.5 6v5.5c0 4.6 3.2 8 7.5 9.5 4.3-1.5 7.5-4.9 7.5-9.5V6z" /><path d="m9 12 2.2 2.2L15.5 10" />
    {:else if id === 'music'}<path d="M9 18V5.5l11-2V16" /><circle cx="6.5" cy="18" r="2.5" /><circle cx="17.5" cy="16" r="2.5" />
    {:else if id === 'ci'}<circle cx="6" cy="6" r="2.2" /><circle cx="18" cy="12" r="2.2" /><circle cx="6" cy="18" r="2.2" /><path d="M8.2 6H12a3 3 0 0 1 3 3v.8M8.2 18H12a3 3 0 0 0 3-3v-.8" />
    {:else if id === 'agents'}<rect x="2.5" y="4" width="19" height="16" rx="2.5" /><path d="m7 10 3 2.5L7 15M12.5 15H17" />
    {:else if id === 'worktrees'}<path d="M3 7.5A1.5 1.5 0 0 1 4.5 6H9l2 2.5h8.5A1.5 1.5 0 0 1 21 10v8.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5z" /><circle cx="9.5" cy="15.5" r="1.4" /><circle cx="15" cy="12.5" r="1.4" /><path d="M9.5 14v-1.5a1.5 1.5 0 0 1 1.5-1.5h2.6" />
    {:else if id === 'data'}<ellipse cx="12" cy="5.5" rx="7.5" ry="2.8" /><path d="M4.5 5.5v13c0 1.5 3.4 2.8 7.5 2.8s7.5-1.3 7.5-2.8v-13M4.5 12c0 1.5 3.4 2.8 7.5 2.8s7.5-1.3 7.5-2.8" />
    {:else if id === 'infra'}<path d="M12 2.8 20.5 7.5v9L12 21.2 3.5 16.5v-9z" /><path d="M12 12 20.5 7.5M12 12 3.5 7.5M12 12v9.2" />
    {:else}<rect x="3" y="3" width="7.5" height="9" rx="1.6" /><rect x="13.5" y="3" width="7.5" height="5" rx="1.6" /><rect x="13.5" y="11" width="7.5" height="10" rx="1.6" /><rect x="3" y="15" width="7.5" height="6" rx="1.6" />{/if}
  </svg>
{/snippet}

<!--
  The header of every page: its title on its color, the filter, the stats. The Agents view has
  the same one (the filter applies to its agents), without what concerns a page of blocks.
-->
{#snippet pageHeader(agents: boolean, preview?: Snippet<[BlockId]>)}
  <header style:--view={agents ? 'var(--src-claude)' : undefined} class:folded={!headerShown}>
    <span class="title">{#if agents}{@render icon('agents')}{t('Agents')}{:else}{@render icon(view.icon)}{t(view.label)}{/if}</span>
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

<div class="app" class:zen={zen.on} class:machine={!!machine()} class:machine-away={!!machine() && !machineState.connected && !!machineState.since && clock(1000).now - machineState.since >= GRACE} style:--view={view.color}>
  <div class="shell">
  <!-- Two fingers up or down on the rail: the next or previous page, while one is shown. -->
  <nav class="rail" aria-label={t('Views')} {@attach pageSwipe(stepView, () => appearance().page_swipe && !editing.on && !zoomed && !agentsView.open && !settingsView.open && !helpView.open)}>
    <span class="edge" aria-hidden="true"></span>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <span class="brand" title="Prism" onclick={logoClick}>{#key rainbow}<span class:spin={rainbow > 0}><PrismLogo size={30} animated pulse={live.snapshot?.updated_at} /></span>{/key}</span>
    {#key rainbow}{#if rainbow}<span class="rainbow" aria-hidden="true"></span>{/if}{/key}
    {#each views() as v, i (v.id)}
      <button class="v" class:on={v.id === view.id && !settingsView.open && !agentsView.open && !helpView.open} style:--tint={v.color} onclick={() => { closeSettings(); closeHelp(); setView(v.id) }} title="{t(v.label)} ({keys(`⌘${i + 1}`)})" aria-current={v.id === view.id ? 'page' : undefined}>
        {@render icon(v.icon)}
        <span>{t(v.label)}</span>
        {#if viewAlert(v)}<span class="alert-dot" aria-label={t('Pod down')}></span>{/if}
        {#each blockBadges(v) as b (b.title)}<span class="count ko" title={b.title}>{b.count}</span>{/each}
        {#if columnsOf(v).flat().includes('worktrees') && staleWorktrees}<span class="count stale" title={tn(staleWorktrees, '{n} stale worktree', '{n} stale worktrees')}>{staleWorktrees}</span>{/if}
      </button>
    {/each}
    {#if agentsEnabled()}
    <button class="v" class:on={agentsView.open && !settingsView.open && !helpView.open} style:--tint="var(--src-claude)" onclick={() => openAgents()} title={keys(t('Agents (⌘{digit}) · new agent ⌘N', { digit: agentsDigit }))} aria-current={agentsView.open ? 'page' : undefined}>
      {@render icon('agents')}
      <span>{t('Agents')}</span>
      {#if agentsWaiting}<span class="count" title={t('{n} waiting for you', { n: agentsWaiting })}>{agentsWaiting}</span>{/if}
    </button>
    {/if}
    <span class="grow"></span>
    <button class="v" class:on={helpView.open} onclick={toggleHelp} title={keys(t('Help (⌘? or F1)'))} aria-pressed={helpView.open}><Icon name="help" /><span>{t('Help')}</span></button>
    <ThemeMenu />
    <button class="v settings" class:on={settingsView.open} onclick={() => { closeAgents(); toggleSettings() }} title={keys(t('Settings (⌘,)'))} aria-pressed={settingsView.open}><Icon name="settings" /><span>{t('Settings')}</span></button>
    <ProfileMenu />
  </nav>
  <div class="content">
    {#if machine()}<MachineBar />{/if}
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
    {:else if id in BLOCK_COMPONENTS}{@const Block = BLOCK_COMPONENTS[id]}{@const def = BLOCK_DEFS.find((b) => b.id === id)!}{#if def.off && stateOf(def, full) === undefined}<BlockOff {id} {...def.off} />{:else}<Block state={stateOf(def, full) as never} error={full!.errors[def.source ?? id]} snap={full!} />{/if}
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
  <ProfileSwitch />
  <!-- Before the other drawers: its Escape is heard first. -->
  <LauncherDrawer />
  <MrPanel />
  <TicketPanel />
  <LogPanel />
  <PodLogPanel />
  <EventPanel />
  <ThreadPanel />
  <ArgoPanel />
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
  .rail :global(.v) { position: relative; display: flex; flex-direction: column; align-items: center; gap: 3px; width: 56px; padding: 6px 0 5px; border: none; border-radius: var(--r8); background: none; color: var(--text-muted); font: inherit; font-size: 9.5px; line-height: 1.1; cursor: pointer; }
  .rail :global(.v:hover) { color: var(--text); background: var(--surface-muted); }
  .rail :global(.v svg) { width: 19px; height: 19px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
  /* Icons tinted with their view's color, even inactive; the active one has a stroke on the left. */
  .rail .v svg { color: color-mix(in srgb, var(--tint) 80%, var(--text-muted)); }
  .rail .v.on { color: var(--tint); background: color-mix(in srgb, var(--tint) var(--rail-active), transparent); font-weight: 600; }
  .rail .v.on svg { color: var(--tint); }
  .rail .v.on::before { content: ''; position: absolute; left: -4px; top: 8px; bottom: 8px; width: var(--rail-mark); border-radius: 0 var(--r3) var(--r3) 0; background: var(--tint); }
  /* Theme button (ThemeMenu) open. */
  .rail :global(.v.on) { color: var(--accent); background: color-mix(in srgb, var(--accent) var(--rail-active), transparent); }
  .rail :global(.v span) { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rail .grow { flex: 1; }
  /* Zen mode: the rail and the header fold away (the header comes back for the filter). */
  .app.zen .rail, header.folded { display: none; }
  /* A page as the others: it slides in and out as they do (`pageTransition`). */
  .agents-host { flex: 1; min-height: 0; display: flex; view-transition-name: view-main; }
  .agents-host[hidden] { display: none; }
  .rail .count { position: absolute; top: 2px; right: 9px; min-width: 15px; height: 15px; padding: 0 4px; border-radius: var(--r8); background: var(--src-claude); color: #fff; font-size: 9px; font-weight: 700; line-height: 15px; text-align: center; }
  .rail .count.stale { background: var(--warn-bg); color: var(--warn); }
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
  .title :global(svg) { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
  .filter { display: flex; align-items: center; gap: 6px; padding: 0 6px 0 10px; height: 30px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); flex: 0 1 360px; min-width: 160px; }
  .filter.active { border-color: var(--info); }
  .filter input { flex: 1; min-width: 0; border: none; outline: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-body); }
  .filter .match { font-size: var(--fs-meta); color: var(--info); white-space: nowrap; }
  .filter .match.none { color: var(--warn); }
  .mine { flex-shrink: 0; font: inherit; font-size: var(--fs-body); padding: 3px 10px; border-radius: var(--r6); border: 1px solid var(--border); background: none; color: var(--text-muted); cursor: pointer; }
  .mine.on { background: var(--info-bg); color: var(--info); border-color: transparent; font-weight: 500; }
  .filter button { display: inline-flex; align-items: center; border: none; background: none; color: var(--text-muted); cursor: pointer; font-size: 13px; line-height: 1; padding: 0 2px; }
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
