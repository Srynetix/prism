<script lang="ts">
  import { live } from '../live.svelte'
  import { t, tc, tn, watchLanguage } from '../i18n.svelte'
  import { alerts as computeAlerts } from '../rules'
  import { blockComponent } from '../blocks/components-lazy'
  import { BLOCK_DEFS } from '../blocks'
  import { stateOf } from '../blocks/block'
  import Icon from '../components/Icon.svelte'
  import type { IconName } from '../icons'
  import RemoteHome from './RemoteHome.svelte'
  import { NODES } from '../settings-tree'
  import { BLOCK_CATEGORIES } from '../views'
  import Boot from '../components/Boot.svelte'
  import { replayBoot } from '../fun.svelte'
  import Toast from '../components/Toast.svelte'
  import { openPalette, palette, usePhonePages } from '../palette.svelte'
  import { feedback, openFeedback } from '../feedback.svelte'
  import { switching } from '../agent-switch.svelte'
  import { socketState } from './socket.svelte'
  import { agentsView, closeAgents, launcherDrawer, openAgents } from '../agents.svelte'
  import ProfileSwitch from './ProfileSwitch.svelte'
  import { ALL, phoneProfiles, several, shown, switchProfile } from './phone-profiles.svelte'
  import { profileColor } from '../profiles.svelte'
  /**
   * An agent's or a shell's terminal (xterm and its chat), loaded the first time one opens: not
   * downloaded nor compiled by a phone that only reads its blocks.
   */
  let tty: Promise<typeof import('./TtyView.svelte')> | null = null
  const ttyView = () => (tty ??= import('./TtyView.svelte'))
  /** The same promise each time: loaded once. */
  function once<T>(load: () => Promise<T>): () => Promise<T> {
    let p: Promise<T> | null = null
    return () => (p ??= load())
  }
  /**
   * The views and drawers, each loaded the first time it opens (a view shown, a drawer opened):
   * a phone downloads and compiles what it uses, not every block of the window's.
   */
  const lazy = {
    Alerts: once(() => import('../components/Alerts.svelte')),
    RemoteMrs: once(() => import('./RemoteMrs.svelte')),
    RemoteAgents: once(() => import('./RemoteAgents.svelte')),
    RemoteAgentsAll: once(() => import('./RemoteAgentsAll.svelte')),
    RemoteSettings: once(() => import('./RemoteSettings.svelte')),
    RemoteProject: once(() => import('./RemoteProject.svelte')),
    Notes: once(() => import('../components/Notes.svelte')),
    Sprint: once(() => import('../components/Sprint.svelte')),
    Pickup: once(() => import('../components/Pickup.svelte')),
    MySprint: once(() => import('../components/MySprint.svelte')),
    InProgress: once(() => import('../components/InProgress.svelte')),
    CiHealth: once(() => import('../components/CiHealth.svelte')),
    Pods: once(() => import('../components/Pods.svelte')),
    Environments: once(() => import('../components/Environments.svelte')),
    ArgoCd: once(() => import('../components/ArgoCd.svelte')),
    Vulnerabilities: once(() => import('../components/Vulnerabilities.svelte')),
    Mail: once(() => import('../components/Mail.svelte')),
    Worktrees: once(() => import('../components/Worktrees.svelte')),
    Refinement: once(() => import('../components/Refinement.svelte')),
    Agenda: once(() => import('../components/Agenda.svelte')),
    Renovate: once(() => import('../components/Renovate.svelte')),
    MrPanel: once(() => import('../components/MrPanel.svelte')),
    TicketPanel: once(() => import('../components/TicketPanel.svelte')),
    LogPanel: once(() => import('../components/LogPanel.svelte')),
    PodLogPanel: once(() => import('../components/PodLogPanel.svelte')),
    ThreadPanel: once(() => import('../components/ThreadPanel.svelte')),
    ArgoPanel: once(() => import('../components/ArgoPanel.svelte')),
    LauncherDrawer: once(() => import('../components/LauncherDrawer.svelte')),
    Feedback: once(() => import('../components/Feedback.svelte')),
    AgentSwitch: once(() => import('../components/AgentSwitch.svelte')),
    Palette: once(() => import('../components/Palette.svelte')),
  }
  import { closeShellView, shellView } from './shell-view.svelte'
  import { pushSupported, setNotifications, subscribed } from './notify'
  import { hasCore, invoke } from '../bridge'
  import { armer } from '../armed.svelte'
  import { closeMr, mrView } from '../mr-view.svelte'
  import { backTicket, closeTicket, ticketView } from '../ticket-view.svelte'
  import { closeLogs, logs } from '../logs.svelte'
  import { closePodLogs, podLogs } from '../pod-logs.svelte'
  import { closeThread, threadView } from '../thread.svelte'
  import { argoView, closeArgo } from '../argo-view.svelte'

  /**
   * Prism on a phone: one view at a time, the bottom tabs chosen in `[remote] tabs`, the others
   * under More, by group as in the window's "+ Block" menu. The views are the window's blocks, fed
   * by the same snapshot: Prism's own here, every block of `blocks/` (another repository's
   * included) by its id, those of CI & delivery together under CI.
   */
  type ViewId = string
  type View = { label: string; icon: IconName; category: string }

  const CORE: Record<string, View> = {
    home: { label: t('Home'), icon: 'activity', category: 'Overview' },
    alerts: { label: t('Alerts'), icon: 'warning', category: 'Overview' },
    notes: { label: t('Notes'), icon: 'pin', category: 'Overview' },
    today: { label: t('Today'), icon: 'calendar', category: 'Overview' },
    mail: { label: t('Mail'), icon: 'mail', category: 'Overview' },
    tickets: { label: t('Tickets'), icon: 'kanban', category: 'Tickets' },
    refinement: { label: t('Refinement'), icon: 'subtask', category: 'Tickets' },
    sprint: { label: t('My sprint'), icon: 'sprint', category: 'Tickets' },
    wip: { label: t('In progress'), icon: 'running', category: 'Tickets' },
    mrs: { label: t('MRs'), icon: 'mr', category: 'Code & agents' },
    agents: { label: t('Agents'), icon: 'claude', category: 'Code & agents' },
    worktrees: { label: t('Worktrees'), icon: 'worktree', category: 'Code & agents' },
    ci: { label: t('CI · Delivery'), icon: 'rocket', category: 'CI & delivery' },
    renovate: { label: 'Renovate', icon: 'automerge', category: 'CI & delivery' },
    infra: { label: t('Infra'), icon: 'kube', category: 'Infra' },
    argocd: { label: 'ArgoCD', icon: 'argocd', category: 'Infra' },
  }
  /** The blocks of `blocks/` have no icon of their own: theirs here, else their group's. */
  const BLOCK_ICONS: Record<string, IconName> = {
    trail: 'clock', pet: 'star', shipit: 'rocket', quotes: 'comment', system: 'cpu', docker: 'kube', dev_servers: 'web',
    tokens: 'hash', status: 'activity', data: 'queue', bqcost: 'timer', custom_blocks: 'code',
  }
  const CATEGORY_ICONS: Record<string, IconName> = {
    Overview: 'activity', Tickets: 'kanban', 'Code & agents': 'code', 'CI & delivery': 'rocket', Security: 'security', Infra: 'kube', Data: 'queue', Fun: 'star',
  }
  /** The blocks of `blocks/` with a view of their own: those of CI & delivery are in CI's. */
  const OWN_BLOCKS = BLOCK_DEFS.filter((b) => b.category !== 'CI & delivery' && !(b.id in CORE))
  const VIEWS: Record<string, View> = {
    ...CORE,
    ...Object.fromEntries(OWN_BLOCKS.map((b) => [b.id, { label: t(b.label), icon: BLOCK_ICONS[b.id] ?? CATEGORY_ICONS[b.category ?? ''] ?? 'category', category: b.category ?? 'Other' }])),
  }

  const full = $derived(live.snapshot)
  const hello = $derived(socketState.hello)
  /** A block the computer has off (its `off` placeholder in the window) is not offered. */
  const blockOff = (id: string) => {
    const def = BLOCK_DEFS.find((b) => b.id === id)
    // Custom blocks: offered once the computer has one.
    return (!!def?.off || id === 'custom_blocks') && stateOf(def!, full) === undefined
  }
  const all = $derived(Object.keys(VIEWS).filter((v) => (v !== 'mail' || hello?.mail) && !blockOff(v)))
  const tabs = $derived((hello?.tabs ?? ['home', 'mrs', 'tickets', 'agents']).filter((tab) => all.includes(tab)).slice(0, 4))
  const more = $derived(all.filter((v) => !tabs.includes(v)))
  /** More's groups, in the order of the "+ Block" menu, the others after them. */
  const moreGroups = $derived.by(() => {
    const rank = (c: string) => ((BLOCK_CATEGORIES as readonly string[]).indexOf(c) + 1 || 500)
    const groups = new Map<string, ViewId[]>()
    for (const v of more) groups.set(VIEWS[v].category, [...(groups.get(VIEWS[v].category) ?? []), v])
    return [...groups].sort(([a], [b]) => rank(a) - rank(b) || a.localeCompare(b))
  })

  const saved = (() => {
    try {
      return localStorage.getItem('prism.remote.view') as ViewId | null
    } catch {
      return null
    }
  })()
  // A notification opens the agents, and the agent that woke the phone (`?agent=`), in its profile.
  const params = new URLSearchParams(location.search)
  const asked = params.get('view') as ViewId | null
  let askedAgent = params.get('agent')
  let view = $state<ViewId>([asked, saved].find((v) => v && (v === 'more' || v in VIEWS)) ?? 'home')
  if ((asked || askedAgent || params.has('profile')) && !params.has('preview')) history.replaceState(null, '', '/')
  $effect(() => {
    if (askedAgent && full?.agents?.some((a) => a.id === askedAgent)) {
      openAgents(askedAgent)
      askedAgent = null
    }
  })
  navigator.serviceWorker?.addEventListener('message', (e) => {
    if (e.data?.t === 'view') go(e.data.view)
    else if (e.data?.t === 'open') openWoken(e.data)
  })
  /** A notification tapped with the page open: its agent, in its profile (the page goes there for another's). */
  function openWoken(o: { profile?: string; agent?: string }) {
    const here = phoneProfiles.choice === ALL ? phoneProfiles.own : phoneProfiles.choice
    if (o.profile && several() && o.profile !== here) return switchProfile(o.profile, { view: 'agents', agent: o.agent })
    go('agents')
    if (o.agent) openAgents(o.agent)
  }
  /** All (several profiles): Home and Agents show every profile's agents. */
  const allProfiles = $derived(several() && phoneProfiles.choice === ALL)

  // The language changed on the computer or from here: the page starts again in it.
  $effect(watchLanguage)
  /** This phone's notifications, shown in More when the computer pushes (`[remote] push`). */
  let notifying = $state(false)
  let notifyError = $state<string | null>(null)
  $effect(() => {
    if (hello?.push) subscribed().then((on) => (notifying = on)).catch(() => {})
  })
  async function toggleNotifications() {
    notifyError = null
    try {
      await setNotifications(!notifying, hello!.push!)
      notifying = !notifying
    } catch (e) {
      notifyError = e instanceof Error ? e.message : String(e)
    }
  }
  /**
   * Prism relaunched on the computer, from the phone (`[remote] actions`): two taps. Its agents and
   * notes outlive it; the socket comes back by itself once the new Prism is up.
   */
  const armed = armer(4000)
  let restarting = $state(false)
  let restartError = $state<string | null>(null)
  /** The old Prism has gone: the next connection is the new one's. */
  let wentDown = false
  let restartTimer: ReturnType<typeof setTimeout> | undefined
  async function restartMac() {
    if (!armed.confirm('restart')) return
    restartError = null
    restarting = true
    wentDown = false
    clearTimeout(restartTimer)
    restartTimer = setTimeout(() => {
      if (!restarting) return
      restarting = false
      restartError = t('Prism has not come back within two minutes: check the computer.')
    }, 120_000)
    try {
      await invoke('restart_app')
    } catch (e) {
      restarting = false
      clearTimeout(restartTimer)
      restartError = e instanceof Error ? e.message : String(e)
    }
  }
  /**
   * The computer itself (`[remote] power`): restarted or shut down as from its Apple menu, after a second
   * tap and the passkey. The agents running stop with it.
   */
  let powering = $state<'restart' | 'shutdown' | null>(null)
  let powerDone = $state<string | null>(null)
  let powerError = $state<string | null>(null)
  /** FileVault (`hello.unlock`): the Mac's password for the restart, sent with it, never kept. */
  let macPassword = $state('')
  const runningAgents = $derived(full?.agents?.filter((a) => a.state === 'running').length ?? 0)
  async function power(action: 'restart' | 'shutdown') {
    const unlock = action === 'restart' && hello?.unlock === 'password'
    if (unlock && !macPassword) {
      powerError = t('Type the Mac’s password first: FileVault asks for it at startup.')
      return
    }
    if (!armed.confirm(action)) return
    powering = action
    powerError = null
    powerDone = null
    try {
      await invoke('mac_power', unlock ? { action, password: macPassword } : { action })
      powerDone = action === 'restart' ? t('The computer restarts: Prism comes back once it is up and opened.') : t('The computer shuts down: Prism is out of reach until it is on again.')
    } catch (e) {
      powerError = e instanceof Error ? e.message : String(e)
    } finally {
      powering = null
      macPassword = ''
    }
  }
  $effect(() => {
    if (!restarting) return
    if (!socketState.connected) wentDown = true
    else if (wentDown) {
      restarting = false
      clearTimeout(restartTimer)
      // The new Prism: its sources answering, line by line, as the window shows them.
      replayBoot()
    }
  })

  function go(v: ViewId, back = false) {
    if (v === view) return scrollTo({ top: 0 })
    // A view opened from the tabs or More: Back comes here again.
    if (!back) {
      trail.push(view)
      history.pushState({ prism: true }, '')
    }
    view = v
    try {
      localStorage.setItem('prism.remote.view', v)
    } catch {
      // A private window: the view is not remembered.
    }
    scrollTo({ top: 0 })
  }

  /**
   * Android's Back (the browser's history): one step back in the remote, never out of it while
   * there is somewhere to go. Each panel opened (an agent's terminal, an MR, a ticket and the ones
   * opened from it, a thread, logs, ArgoCD) and each view changed adds a history entry; Back
   * closes the panel on top, a ticket going back to the one before it, otherwise returns to the
   * previous view. A panel closed otherwise (its ×, a click beside it) takes its entry back, so
   * that Back never falls on a dead one.
   */
  const LAYERS: Record<string, { depth: () => number; back: () => void }> = {
    agent: { depth: () => (agentsView.open && agentsView.selected ? 1 : 0), back: closeAgents },
    shell: { depth: () => (shellView.key ? 1 : 0), back: closeShellView },
    mr: { depth: () => (mrView.ref ? 1 : 0), back: closeMr },
    ticket: { depth: () => (ticketView.id ? 1 + ticketView.trail.length : 0), back: () => (ticketView.trail.length ? backTicket() : closeTicket()) },
    thread: { depth: () => (threadView.ref ? 1 : 0), back: closeThread },
    logs: { depth: () => (logs.open ? 1 : 0), back: closeLogs },
    podLogs: { depth: () => (podLogs.open ? 1 : 0), back: closePodLogs },
    argo: { depth: () => (argoView.open ? 1 : 0), back: closeArgo },
  }
  /** The views left, the last one first back. */
  const trail: ViewId[] = []
  const seen: Record<string, number> = {}
  /** The panels in the order they were last opened: Back closes the last one still open. */
  let order: string[] = []
  /** Entries this page takes back itself (a panel closed otherwise): their `popstate` is no Back. */
  let ignore = 0
  /** The change being made by Back: its entry is already gone. */
  let backing = false
  $effect(() => {
    for (const [key, layer] of Object.entries(LAYERS)) {
      const depth = layer.depth()
      const was = seen[key] ?? 0
      if (depth === was) continue
      seen[key] = depth
      if (backing) continue
      if (depth > was) {
        for (let i = was; i < depth; i++) history.pushState({ prism: true }, '')
        order = [...order.filter((k) => k !== key), key]
      } else {
        for (let i = depth; i < was; i++) {
          ignore++
          history.back()
        }
      }
    }
    backing = false
  })
  function onBack() {
    if (ignore) {
      ignore--
      return
    }
    const top = [...order].reverse().find((k) => LAYERS[k].depth() > 0)
    if (top) {
      backing = true
      LAYERS[top].back()
      return
    }
    const previous = trail.pop()
    if (previous) go(previous, true)
  }

  const alerts = $derived(full ? computeAlerts(full) : [])
  const block = (id: string) => {
    const def = BLOCK_DEFS.find((b) => b.id === id)
    return def && full ? { Block: blockComponent(id), def, state: stateOf(def, full) } : null
  }
  /** The team's blocks of CI & delivery (a deploy group), shown with CI health. */
  const delivery = $derived(BLOCK_DEFS.filter((b) => b.category === 'CI & delivery' && full && stateOf(b, full) !== undefined).map((b) => b.id))
  /** More › Settings (`[remote] settings`): `settings`, then `settings:<page>` for a page of its tree. */
  const settingsOn = $derived(!!hello?.settings || !hasCore())
  const settingsPage = $derived(view.startsWith('settings:') ? view.slice('settings:'.length) : null)
  /** More › New project (`[remote] projects`): `project`. */
  const projectsOn = $derived(!!hello?.projects || !hasCore())
  const title = $derived(view === 'more' ? t('More') : view === 'project' ? t('New project') : view === 'settings' ? t('Settings') : settingsPage ? t(NODES.get(settingsPage)?.label ?? 'Settings') : (VIEWS[view]?.label ?? view))
  // The palette goes to the phone's views, not the window's pages.
  $effect(() => void usePhonePages(all.map((v) => ({ id: v, label: VIEWS[v].label, go: () => go(v) }))))
</script>

<!-- Several profiles: the header's line and the tab shown in the profile's color. -->
<div class="shell" class:tinted={several()} style:--pc={several() ? profileColor(shown()?.color) : null}>
  <header>
    <b>{title}</b>
    <button class="search" onclick={() => openPalette()} aria-label={t('Search: go to, or do')}><Icon name="search" /></button>
    {#if several()}<ProfileSwitch />{/if}
    <!-- A reconnection of a moment goes unseen: out of reach only after a few seconds. -->
    <span class="mac" class:off={socketState.away} title={socketState.away ? t('The computer is out of reach') : t('Connected to the computer')}>
      <span class="dot"></span>{socketState.away ? (socketState.error ?? t('Computer')) : t('Computer')}
    </span>
  </header>

  <main>
    {#if !full}
      <p class="empty">{t('Waiting for the computer…')}</p>
    {:else if view === 'more'}
      {#each moreGroups as [category, views] (category)}
        <p class="sect">{t(category)}</p>
        <ul class="more">
          {#each views as v (v)}
            <li><button onclick={() => go(v)}><Icon name={VIEWS[v].icon} /><span>{VIEWS[v].label}</span><Icon name="next" /></button></li>
          {/each}
        </ul>
      {/each}
      {#if settingsOn}
        <ul class="more">
          <li><button onclick={() => go('settings')}><Icon name="settings" /><span>{t('Settings')}<small>{t('Read on the computer; notifications, alerts and appearance changed from here')}</small></span><Icon name="next" /></button></li>
        </ul>
      {/if}
      {#if projectsOn}
        <ul class="more">
          <li><button onclick={() => go('project')}><Icon name="plus" /><span>{t('New project')}<small>{t('A git repository in the computer’s working folder, on request on the forge; or one of the forge’s cloned')}</small></span><Icon name="next" /></button></li>
        </ul>
      {/if}
      {#if hello?.push && pushSupported()}
        <ul class="more">
          <li><button onclick={toggleNotifications}><Icon name="alarm" /><span>{t('Notifications')}<small>{notifying ? t('On: an agent waiting wakes this phone') : tc('notifications', 'Off')}</small></span><span class="switch" class:on={notifying}></span></button></li>
        </ul>
        {#if notifyError}<p class="note ko">{notifyError}</p>{/if}
      {/if}
      {#if hello?.classes.includes('act')}
        <ul class="more">
          <li><button onclick={() => openFeedback({ where: 'phone remote' })}><Icon name="comment" /><span>{t('Send feedback')}<small>{t('An issue on Prism’s repository or a pack’s, opened by the computer')}</small></span><Icon name="next" /></button></li>
          <li><button onclick={restartMac} disabled={restarting}><Icon name="retry" /><span>{restarting ? t('Restarting Prism…') : armed.is('restart') ? t('Tap again to restart') : t('Restart Prism')}<small>{t('On the computer: settings reread; agents and notes stay')}</small></span></button></li>
        </ul>
        {#if restartError}<p class="note ko">{restartError}</p>{/if}
      {/if}
      {#if hello?.power}
        {@const stops = runningAgents ? ` · ${tn(runningAgents, '{n} agent running stops', '{n} agents running stop')}` : ''}
        <p class="sect">{t('The computer')}</p>
        <ul class="more">
          <li><button onclick={() => power('restart')} disabled={!!powering}><Icon name="refresh" /><span>{powering === 'restart' ? t('Restarting…') : armed.is('restart') ? t('Tap again: restart the computer') : t('Restart the computer')}<small>{t('Confirmed by your passkey')}{stops}</small></span></button></li>
          <li><button class="danger" onclick={() => power('shutdown')} disabled={!!powering}><Icon name="stop" /><span>{powering === 'shutdown' ? t('Shutting down…') : armed.is('shutdown') ? t('Tap again: shut the computer down') : t('Shut down the computer')}<small>{t('Confirmed by your passkey · out of reach until switched on')}{stops}</small></span></button></li>
        </ul>
        {#if hello.unlock === 'password'}
          <label class="unlock"><b>{t('The Mac’s password')}</b>
            <input type="password" bind:value={macPassword} autocomplete="off" autocapitalize="off" autocorrect="off" spellcheck="false" />
            <small>{t('FileVault: for the restart to skip the unlock at startup, so that the Mac comes back by itself. Sent with the restart only, never kept.')}</small>
          </label>
        {:else if hello.unlock === 'stuck'}
          <p class="note ko">{t('FileVault on a Mac without authenticated restarts: after a restart, it waits for its password at startup, out of reach.')}</p>
        {/if}
        {#if powerDone}<p class="note">{powerDone}</p>{/if}
        {#if powerError}<p class="note ko">{powerError}</p>{/if}
      {/if}
      <p class="note">{settingsOn ? '' : `${t('Settings stay on the computer.')} `}{hello ? t('This phone: {device}.', { device: hello.device }) : ''}</p>
    {:else if view === 'project'}{#await lazy.RemoteProject() then { default: RemoteProject }}<RemoteProject />{/await}
    {:else if view === 'settings' || settingsPage}{#await lazy.RemoteSettings() then { default: RemoteSettings }}<RemoteSettings node={settingsPage} open={(id) => go(`settings:${id}`)} />{/await}
    {:else if view === 'home'}<RemoteHome snap={full} {alerts} {go} all={allProfiles} />
    {:else if view === 'alerts'}{#await lazy.Alerts() then { default: Alerts }}<Alerts {alerts} phone />{/await}
    {:else if view === 'mrs'}{#await lazy.RemoteMrs() then { default: RemoteMrs }}<RemoteMrs snap={full} />{/await}
    {:else if view === 'tickets'}
      {#await Promise.all([lazy.Sprint(), lazy.Pickup()]) then [{ default: Sprint }, { default: Pickup }]}
        <Sprint sprint={full.sprint} next={full.next_sprint} outside={full.outside_sprint} me={full.tracker_me} full={full} />
        <Pickup pickup={full.pickup} me={full.tracker_me} />
      {/await}
    {:else if view === 'agents' && allProfiles}{#await lazy.RemoteAgentsAll() then { default: RemoteAgentsAll }}<RemoteAgentsAll snap={full} />{/await}
    {:else if view === 'agents'}{#await lazy.RemoteAgents() then { default: RemoteAgents }}<RemoteAgents snap={full} acts={hello ? hello.classes.includes('act') : !hasCore()} />{/await}
    {:else if view === 'notes'}{#await lazy.Notes() then { default: Notes }}<Notes snap={full} phoneTyping={socketState.hello?.typing ?? false} />{/await}
    {:else if view === 'ci'}
      {#each delivery as id (id)}{@const b = block(id)}{#if b}{#await b.Block then Block}{#if Block}<Block state={b.state as never} error={full.errors[b.def.source ?? id]} snap={full} />{/if}{/await}{/if}{/each}
      {#await lazy.CiHealth() then { default: CiHealth }}<CiHealth health={full.ci_health} error={full.errors.ci_health} />{/await}
    {:else if view === 'infra'}
      {#await Promise.all([lazy.Pods(), lazy.Environments(), lazy.Vulnerabilities()]) then [{ default: Pods }, { default: Environments }, { default: Vulnerabilities }]}
        <Pods pods={full.pods} usage={full.pod_usage} nodes={full.pod_nodes ?? []} />
        <Environments environments={full.environments} argocd={full.argocd} error={full.errors.environments} />
        <Vulnerabilities vulns={full.vulnerabilities} error={full.errors.vulnerabilities} />
      {/await}
    {:else if view === 'argocd'}{#await lazy.ArgoCd() then { default: ArgoCd }}<ArgoCd argocd={full.argocd} environments={full.environments} error={full.errors.argocd} />{/await}
    {:else if view === 'sprint'}{#await lazy.MySprint() then { default: MySprint }}<MySprint snap={full} />{/await}
    {:else if view === 'wip'}{#await lazy.InProgress() then { default: InProgress }}<InProgress snap={full} />{/await}
    {:else if view === 'refinement'}{#await lazy.Refinement() then { default: Refinement }}<Refinement refinement={full.refinement} me={full.tracker_me} />{/await}
    {:else if view === 'worktrees'}{#await lazy.Worktrees() then { default: Worktrees }}<Worktrees snap={full} />{/await}
    {:else if view === 'today'}{#await lazy.Agenda() then { default: Agenda }}<Agenda agenda={full.agenda} />{/await}
    {:else if view === 'renovate' && full.features?.renovate === false}<p class="empty">{t('{name} is off on the computer.', { name: 'Renovate' })}</p>
    {:else if view === 'renovate'}{#await lazy.Renovate() then { default: Renovate }}<Renovate renovate={full.renovate} merge_requests={full.merge_requests} error={full.errors.renovate} />{/await}
    {:else if view === 'mail'}{#await lazy.Mail() then { default: Mail }}<Mail mail={full.mail} enabled={!!full.mail} error={full.errors.mail} />{/await}
    {:else}
      {@const b = block(view)}
      {#if b && (b.state !== undefined || !b.def.off)}{#await b.Block then Block}{#if Block}<Block state={b.state as never} error={full.errors[b.def.source ?? view]} snap={full} />{/if}{/await}{:else}<p class="empty">{t('{name} is off on the computer.', { name: VIEWS[view]?.label ?? view })}</p>{/if}
    {/if}
  </main>

  <nav>
    {#each tabs as tab (tab)}
      <button class:on={view === tab} onclick={() => go(tab)}>
        <Icon name={VIEWS[tab].icon} size="20px" />
        <span>{VIEWS[tab].label}</span>
        {#if (tab === 'alerts' || tab === 'home') && alerts.length}<b class="n">{alerts.length}</b>{/if}
      </button>
    {/each}
    <button class:on={view === 'more' || view === 'project' || more.includes(view as never) || view.startsWith('settings')} onclick={() => go('more')}><Icon name="more" size="20px" /><span>{t('More')}</span></button>
  </nav>
</div>

<svelte:window onpopstate={onBack} />

<!-- An agent opened from the list (Agents, an alert): its terminal, read-only. -->
{#if agentsView.open && agentsView.selected}
  {@const agent = full?.agents.find((a) => a.id === agentsView.selected)}
  {#key agentsView.selected}{#await ttyView() then { default: TtyView }}<TtyView id={agentsView.selected} title={agent?.title ?? agentsView.selected} status={agent?.status} context={agent?.context} usage={agent?.usage} onclose={() => { closeShellView(); closeAgents() }} />{/await}{/key}
{/if}
<!-- One of its shells (Shell, a task run), over it. -->
{#if shellView.key}
  {#key shellView.key}{#await ttyView() then { default: TtyView }}<TtyView id={shellView.key} title={shellView.title} shell onclose={closeShellView} />{/await}{/key}
{/if}
<!-- The drawers, each mounted (and loaded the first time) once opened. -->
{#if feedback.open}{#await lazy.Feedback() then { default: Feedback }}<Feedback phone />{/await}{/if}
{#if switching.open}{#await lazy.AgentSwitch() then { default: AgentSwitch }}<AgentSwitch phone />{/await}{/if}
{#if palette.open}{#await lazy.Palette() then { default: Palette }}<Palette phone />{/await}{/if}
{#if mrView.ref}{#await lazy.MrPanel() then { default: MrPanel }}<MrPanel phone />{/await}{/if}
{#if ticketView.id}{#await lazy.TicketPanel() then { default: TicketPanel }}<TicketPanel phone />{/await}{/if}
{#if logs.open}{#await lazy.LogPanel() then { default: LogPanel }}<LogPanel />{/await}{/if}
{#if podLogs.open}{#await lazy.PodLogPanel() then { default: PodLogPanel }}<PodLogPanel />{/await}{/if}
{#if threadView.ref}{#await lazy.ThreadPanel() then { default: ThreadPanel }}<ThreadPanel />{/await}{/if}
{#if argoView.open}{#await lazy.ArgoPanel() then { default: ArgoPanel }}<ArgoPanel />{/await}{/if}
<!-- Last: an agent action's launcher, over the panel it came from. -->
{#if launcherDrawer.open}{#await lazy.LauncherDrawer() then { default: LauncherDrawer }}<LauncherDrawer />{/await}{/if}
<Toast />
<!-- Prism's startup screen, as the window's (`[appearance] boot`): at the page's opening, and the
     Prism relaunched from here coming back. A tap skips it. -->
<Boot />

<style>
  .shell {
    min-height: 100dvh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    color: var(--text);
  }
  header {
    position: sticky;
    top: 0;
    /* Above the tabs: the profile sheet opens from it and covers them. */
    z-index: 6;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: calc(10px + env(safe-area-inset-top)) 14px 10px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  header b { font-size: 16px; }
  .mac {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 9px;
    border-radius: 10px;
    font-size: 12px;
    background: var(--ok-bg);
    color: var(--ok);
  }
  .mac.off { background: var(--surface-muted); color: var(--text-muted); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: currentColor; }
  main {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 10px 10px calc(76px + env(safe-area-inset-bottom));
    min-width: 0;
  }
  /* The blocks were drawn for columns: on a phone, each takes the width and its natural height. */
  main > :global(*) { min-width: 0; max-width: 100%; flex: none; }
  .empty, .note { color: var(--text-muted); text-align: center; margin: 24px 0; font-size: 13px; }
  ul.more { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: 12px; overflow: hidden; background: var(--surface); }
  ul.more li + li { border-top: 1px solid var(--border); }
  ul.more button { width: 100%; display: flex; align-items: center; gap: 12px; padding: 14px; background: none; border: none; color: var(--text); font-size: 15px; text-align: left; }
  ul.more button span { flex: 1; display: flex; flex-direction: column; }
  ul.more small { color: var(--text-muted); font-size: 12px; }
  .switch { flex: none !important; width: 38px; height: 22px; border-radius: 11px; background: var(--surface-muted); border: 1px solid var(--border); position: relative; }
  .switch::after { content: ''; position: absolute; top: 2px; left: 2px; width: 16px; height: 16px; border-radius: 50%; background: var(--text-muted); }
  .switch.on { background: var(--ok); border-color: var(--ok); }
  .switch.on::after { left: 18px; background: var(--surface); }
  .note.ko { color: var(--ko); margin-top: 0; }
  .unlock { display: flex; flex-direction: column; gap: 6px; margin: 4px 0 12px; font-size: 13px; }
  .unlock input { width: 100%; padding: 9px 10px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); color: var(--text); font: 14px var(--mono); }
  .unlock input:focus { outline: none; border-color: var(--accent); }
  .unlock small { color: var(--text-muted); font-size: 12px; }
  .sect { margin: 6px 4px -4px; color: var(--text-muted); font-size: 11px; letter-spacing: 0.05em; text-transform: uppercase; }
  ul.more button.danger { color: var(--ko); }
  nav {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 5;
    display: flex;
    justify-content: space-around;
    padding: 6px 4px calc(6px + env(safe-area-inset-bottom));
    background: var(--surface);
    border-top: 1px solid var(--border);
  }
  nav button {
    position: relative;
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 4px 0;
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 10.5px;
  }
  nav button.on { color: var(--accent); }
  .tinted header { border-bottom: 2px solid color-mix(in srgb, var(--pc) 70%, transparent); }
  .tinted nav button.on { color: var(--pc); }
  .n { position: absolute; top: 0; left: calc(50% + 6px); min-width: 16px; padding: 0 4px; border-radius: 8px; background: var(--ko); color: #fff; font-size: 9.5px; line-height: 16px; }
  .search { display: inline-flex; margin-left: auto; padding: 6px; border: none; background: none; color: var(--text-muted); }
  .search ~ .mac { margin-left: 0; }
</style>
