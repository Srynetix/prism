<script lang="ts">
  import OverlapChip from '../components/OverlapChip.svelte'
  import { overlapsOfAgent } from '../overlaps'
  import { onMount } from 'svelte'
  import { t, tc } from '../i18n.svelte'
  import { Terminal } from '@xterm/xterm'
  import '@xterm/xterm/css/xterm.css'
  import { terminalTheme } from '../terminal-theme'
  import Icon from '../components/Icon.svelte'
  import { invoke } from '../bridge'
  import { socketState } from './socket.svelte'
  import { remoteBase } from './phone-profiles.svelte'
  import AgentTools from './AgentTools.svelte'
  import ContextGauge from '../components/ContextGauge.svelte'
  import AgentUsageChip from '../components/AgentUsage.svelte'
  import type { Agent } from '../types'
  import QuickAnswers from '../components/QuickAnswers.svelte'
  import { live } from '../live.svelte'
  import { parse, type Line } from '../ansi'
  import { cliOf } from '../agent-cli'
  import { plans } from '../agents.svelte'
  import ChatView from '../components/chat/ChatView.svelte'
  import SlashMenu from '../components/SlashMenu.svelte'
  import AttachButton from '../components/chat/AttachButton.svelte'
  import AttachStrip from '../components/chat/AttachStrip.svelte'
  import { cleanLines, cleanText } from './tty-clean'
  import AgentSide from '../components/AgentSide.svelte'
  import AgentChain from '../components/AgentChain.svelte'
  import { armer } from '../armed.svelte'
  import { agentArtifacts, clearAgent, closeAgent, stopAgent, type AgentArtifact, type SideTab } from '../agents.svelte'
  import { unseen } from './web-seen.svelte'
  import { toast } from '../toast.svelte'
  import { cliEffort, effortOf, effortsOf, loadEffortsOf, pickEffort, takesEffort } from '../agent-effort'
  import { CLIS } from '../agent-cli'
  import { installedClis, openSwitch, planUse, targetsOf } from '../agent-switch.svelte'
  import AgentLimitBanner from '../components/AgentLimitBanner.svelte'
  import { agentStateLabel } from '../rules'
  import { draftOf, setDraft } from '../chat-drafts.svelte'
  import { terminalStack } from '../fonts.svelte'

  /**
   * An agent's terminal on a phone, always at the phone's size, in a readable font: watched, by a
   * read-only tmux client whose size the agent's window takes while the phone is its latest
   * client (the desk typing takes it back); taken (`[remote] typing`, confirmed by the passkey),
   * a client that types too, with a keys bar; the desk taking it back, it is watched again. The
   * phone turned, the terminal follows. A Prism too old to size a watching client gives its window's
   * size: fitted to the width, or readable and scrolled sideways (Zoom).
   * Watched, what scrolled off its screen is above it, in the same scroll: scrolled up, it is read,
   * a tap on Live comes back down. An agent's has its tools beside it (Shell, Tasks, Web:
   * AgentTools) and, when its CLI keeps a transcript (Claude Code, Codex), its conversation first
   * (Chat: ChatView), the terminal's last lines under it; `shell`: one of its shells, the same
   * terminal and answer box, without what only an agent understands (quick answers, Allow).
   * Its Chat, calm: its state, branch and stage under its name; Chat, Terminal, Diff as tabs, the
   * others behind More (a sheet); its chain a chip of the Chat's rail; under it one dock: its
   * context as a thin line on its edge (a tap: the detail), its permission as a card, its quick
   * answers at its prompt, its menus' keys (1 2 3, Esc) behind ⌨︎, a round Send.
   */
  let { id, title, status, onclose, shell = false, context = null, usage = null }: {
    id: string
    title: string
    status?: string
    onclose: () => void
    shell?: boolean
    /** Its context window and what it consumed: the bar under its terminal, as on the computer. */
    context?: Agent['context']
    usage?: Agent['usage']
  } = $props()
  /** Its agent, for the quick answers. */
  const quickAgent = $derived(live.snapshot?.agents?.find((x) => x.id === id))
  /** Its CLI exited, or its terminal is gone (the computer restarted): Resume picks the conversation up again. */
  const stopped = $derived(!shell && (quickAgent?.state === 'exited' || quickAgent?.state === 'gone'))
  /** Prepared, not started: no terminal yet; Start launches it as prepared (its Then steps with it, on the computer). */
  const prepared = $derived(!shell && quickAgent?.state === 'prepared')
  type Tab = 'chat' | 'terminal' | 'shell' | 'tasks' | 'web' | 'files' | 'diff' | 'activity' | 'launch' | 'notes' | 'plan'
  /** The panels the window's AgentSide shows on the phone too; Shell, Tasks and Web are the phone's own (AgentTools). */
  const SIDE_TABS: Tab[] = ['files', 'diff', 'activity', 'launch', 'notes', 'plan']
  const isSide = (tb: Tab) => SIDE_TABS.includes(tb)
  /** Its CLI keeps a transcript the Chat tab reads. */
  const chats = $derived(!shell && !!quickAgent && cliOf(quickAgent.provider).conversation)
  /** The artifacts its CLI wrote since the Web tab was last open, counted on the tab (read every 10 s). */
  let artifacts = $state<AgentArtifact[]>([])
  const newArtifacts = $derived(unseen(id, artifacts))
  $effect(() => {
    if (shell) return
    const read = () => agentArtifacts(id).then((list) => (artifacts = list), () => {})
    void read()
    const timer = setInterval(read, 10_000)
    return () => clearInterval(timer)
  })
  const TABS = $derived<[Tab, string][]>([...(chats ? [['chat', t('Chat')] as [Tab, string]] : []), ['terminal', t('Terminal')], ['diff', t('Diff')], ...(quickAgent && plans(quickAgent) ? [['plan', t('Plan')] as [Tab, string]] : []), ['files', t('Files')], ['shell', t('Shell')], ['tasks', t('Tasks')], ['web', newArtifacts ? `${t('Web')} · ${newArtifacts}` : t('Web')], ['activity', t('Activity')], ['launch', tc('infra', 'Launch')], ['notes', t('Notes')]])
  /** The tab picked; until then, its conversation when it has one. */
  let picked = $state<Tab | null>(null)
  const tab = $derived<Tab>(picked ?? (chats ? 'chat' : 'terminal'))
  /** The tabs always shown (and the one picked among the others); the others behind More. */
  const MAIN: Tab[] = ['chat', 'terminal', 'diff']
  const shownTabs = $derived(TABS.filter(([tb]) => MAIN.includes(tb) || tb === tab))
  let more = $state(false)
  /** Its Chat: the calm dock (permission card, keys behind ⌨︎, context as a line). */
  const calm = $derived(tab === 'chat')
  let keysOpen = $state(false)
  let ctxOpen = $state(false)
  const stateWords = $derived(quickAgent ? agentStateLabel(quickAgent, true) : '')

  const typing = $derived(socketState.hello?.typing ?? false)
  /**
   * `[remote] actions`: the phone answers the agent (a prompt, a choice, Esc, a permission). A
   * shell the same way, but every key in it is typing (`[remote] typing`, its passkey): a shell
   * runs anything.
   */
  const acts = $derived(shell ? typing : (socketState.hello?.classes.includes('act') ?? false))
  /** Its draft: kept when you leave it, until sent. */
  const text = $derived(draftOf(id))
  const write = (t: string) => setDraft(id, t)
  let field = $state<HTMLTextAreaElement>()
  let busy = $state(false)
  let error = $state<string | null>(null)
  let note = $state<string | null>(null)

  async function act(command: string, args: Record<string, unknown> = {}) {
    busy = true
    error = null
    try {
      await invoke(command, { id, ...args })
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
  /**
   * The text, typed as a paste then submitted, as on the computer (even while the agent works): the
   * phone's keyboard, dictation and autocorrect. Typing (`[remote] typing`): the passkey confirms
   * it, once per agent for `[remote] step_up_minutes`, the same confirmation as Take.
   */
  let sent = $state(false)
  async function send() {
    const t = text.trim()
    if (!t || !typing) return
    await act('agent_type', { text: t })
    if (!error) {
      if (draftOf(id).trim() === t) setDraft(id, '')
      sent = true
      setTimeout(() => (sent = false), 3000)
    }
  }

  /**
   * A shell's last command again: Up then Enter, as at the desk (its history, whoever typed it),
   * typing like any key there (its passkey once for a while).
   */
  async function again() {
    await act('agent_key', { key: 'Up' })
    if (!error) await act('agent_key', { key: 'Enter' })
  }

  let host: HTMLDivElement
  let screen: HTMLDivElement
  let term: Terminal | null = null
  let ws: WebSocket | null = null
  let size = $state<{ cols: number; rows: number } | null>(null)
  let control = $state(false)
  /** Its context as a line on the dock's edge, its bar behind a tap; without the dock, the bar. */
  const ctxLine = $derived(calm && !!context && acts && !stopped && !control)
  /** The computer sized this terminal to the phone (watched or typed in). */
  let sized = $state(false)
  let height = $state(0)
  let zoomed = $state(false)
  let width = $state(0)
  /** Ctrl pressed on the keys bar: the next key is sent as Ctrl-<key>. */
  let ctrl = $state(false)

  /** The font that fits the agent's columns in the phone's width (a monospace cell ≈ 0.6 em). */
  const fit = $derived(size && width ? Math.max(4, Math.min(13, Math.floor((width / size.cols / 0.602) * 10) / 10)) : 10)
  /** Typing: a readable font, the terminal sized to the phone. */
  const TYPING_FONT = 11
  /** The bundled Nerd Font, as on the computer by default: a prompt's icons are in it, not in the phone's fonts. */
  const TERM_FONT = terminalStack()

  const encoder = new TextEncoder()
  function key(data: string) {
    if (!ws || ws.readyState !== WebSocket.OPEN || !control) return
    if (ctrl && data.length === 1) {
      const c = data.toUpperCase().charCodeAt(0)
      if (c >= 64 && c <= 95) data = String.fromCharCode(c & 0x1f)
      ctrl = false
    }
    ws.send(encoder.encode(data))
  }

  /** The terminal cells the screen holds, in the readable font. */
  const cells = () => ({
    cols: Math.max(20, Math.floor((screen.clientWidth - 8) / (TYPING_FONT * 0.602))),
    rows: Math.max(8, Math.floor((screen.clientHeight - 12) / (TYPING_FONT * 1.1 * 1.18))),
  })

  /** Opens the terminal at the phone's size: watched, or typed in. */
  function connect(typed: boolean) {
    refused = false
    ws?.close()
    term?.dispose()
    term = null
    size = null
    const params = new URLSearchParams({ agent: id })
    const { cols, rows } = cells()
    params.set('cols', String(cols))
    params.set('rows', String(rows))
    if (typed) params.set('control', 'true')
    const sock = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}${remoteBase()}/api/tty?${params}`)
    ws = sock
    heard = Date.now()
    sock.binaryType = 'arraybuffer'
    sock.onmessage = (e) => {
      heard = Date.now()
      if (typeof e.data === 'string') {
        const m = JSON.parse(e.data)
        if (m.t === 'error') {
          error = m.error
          refused = true
        }
        if (m.t === 'size') {
          retries = 0
          note = note === RECONNECTING ? null : note
          size = { cols: m.cols, rows: m.rows }
          control = !!m.control
          sized = !!m.sized || control
          term = new Terminal({
            cols: m.cols, rows: m.rows, fontSize: sized ? TYPING_FONT : fit, fontFamily: TERM_FONT,
            lineHeight: 1.1, theme: terminalTheme(), disableStdin: !control, cursorBlink: control, scrollback: 2000,
          })
          term.open(host)
          // Still loading, the font was measured as its fallback: measured again once there.
          if (!document.fonts.check(`16px ${TERM_FONT}`)) {
            const opened = term
            void document.fonts.load(`16px ${TERM_FONT}`).then(() => {
              if (term === opened) opened.options.fontFamily = `${TERM_FONT}, monospace`
            })
          }
          if (control) {
            term.onData(key)
            term.focus()
          } else if (term.textarea) {
            // Watched: a tap must not bring the phone's keyboard up.
            term.textarea.readOnly = true
            term.textarea.tabIndex = -1
            term.textarea.setAttribute('inputmode', 'none')
          }
        }
        return
      }
      term?.write(new Uint8Array(e.data as ArrayBuffer))
    }
    sock.onclose = (e) => {
      if (ws !== sock) return
      if (e.code === 4401) error = t('Session ended: unlock again.')
      else if (control && (e.code === 4409 || e.code === 4408)) {
        // Back to watching.
        note = e.code === 4409 ? t('The desk took the terminal back.') : t('Given back after a while without a key.')
        control = false
        connect(false)
      } else if (!refused) {
        // Dropped (the network, the phone asleep): back on its own, as long as it takes, taken
        // again if it was and may still be; a drop of a moment goes unseen.
        retries++
        if (retries > 1) note = RECONNECTING
        setTimeout(() => ws === sock && connect(control && retries < 3), Math.min(1000 * retries, 5000))
      }
    }
  }

  /**
   * A socket can die without a word (the phone asleep, a network change): the computer's sign of life
   * every 10 s missing for 25, the terminal connects again; so it does on coming back to the
   * foreground after a while, or when the network returns.
   */
  const RECONNECTING = t('Reconnecting…')
  let heard = 0
  let retries = 0
  /** The computer refused this terminal (said why): trying again would not help. */
  let refused = false
  let hiddenAt = 0
  function revive() {
    if (!ws) return
    note = RECONNECTING
    connect(control)
  }
  $effect(() => {
    const watchdog = setInterval(() => {
      if (document.visibilityState === 'visible' && ws && Date.now() - heard > 25_000) revive()
    }, 5000)
    const visible = () => {
      if (document.visibilityState === 'hidden') hiddenAt = Date.now()
      else if (Date.now() - hiddenAt > 5000 || Date.now() - heard > 12_000) revive()
    }
    document.addEventListener('visibilitychange', visible)
    addEventListener('online', revive)
    return () => {
      clearInterval(watchdog)
      document.removeEventListener('visibilitychange', visible)
      removeEventListener('online', revive)
    }
  })

  async function take() {
    busy = true
    error = null
    note = null
    try {
      // The passkey confirms it (biometrics), unless it did within [remote] step_up_minutes.
      await invoke('remote_control', { agent: id })
      connect(true)
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  /** Relaunched on its conversation, in its folder: watched again. */
  async function resume() {
    await act('agent_resume')
    if (error) return
    note = null
    retries = 0
    connect(false)
  }

  /** A prepared agent started, under the same id: its terminal watched once it exists. */
  async function start() {
    await act('agent_start')
    if (error) return
    note = null
    retries = 0
    if (!ws) connect(false)
  }

  /**
   * Ending it from the phone (its ⋯ menu): Stop (Ctrl-C twice, its last screen stays) and Close
   * (stopped, its worktree removed when nothing would be lost), as in the window's. Critical
   * actions (`[remote] critical`, the passkey each time); a second tap confirms.
   */
  const critical = $derived(socketState.hello?.classes.includes('critical') ?? false)
  let menu = $state(false)
  /** Its effort's levels, under its chip. */
  let effortOpen = $state(false)
  const armed = armer(4000)
  /** Its CLI clears from Prism (Claude Code, Codex), at its prompt. */
  const clearable = $derived(chats && quickAgent?.state === 'running')
  async function clear() {
    if (!armed.confirm('clear')) return
    busy = true
    error = null
    try {
      await clearAgent(id)
      menu = false
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  /** Its reasoning effort, picked under its chip: at once for Claude Code at its prompt, otherwise from its next resume. */
  $effect(() => {
    if (effortOpen && quickAgent && takesEffort(quickAgent)) loadEffortsOf(quickAgent)
  })
  async function setEffort(effort: string | null) {
    if (!quickAgent) return
    busy = true
    error = null
    try {
      toast(await pickEffort(quickAgent, effort), 5000)
      effortOpen = false
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  async function stop() {
    if (!armed.confirm('stop')) return
    busy = true
    error = null
    try {
      await stopAgent(id)
      menu = false
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
  async function close() {
    if (!armed.confirm('close')) return
    busy = true
    error = null
    try {
      const kept = await closeAgent(id)
      if (kept) toast(kept, 5000)
      onclose()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  function release() {
    note = null
    control = false
    connect(false)
  }

  // A prepared agent has no terminal yet: watched once started (here or on the computer).
  let waited = false
  $effect(() => {
    if (prepared) waited = true
    else if (waited && !ws) connect(false)
  })
  onMount(() => {
    if (!prepared) connect(false)
    return () => {
      const sock = ws
      ws = null
      sock?.close()
      term?.dispose()
    }
  })

  // Watched at the window's size (an older Prism): fitted to the width, or readable and scrolled sideways.
  $effect(() => {
    const px = zoomed ? 11 : fit
    if (size && term && !sized && term.options.fontSize !== px) term.options.fontSize = px
  })

  // The phone turned (or its keyboard came up): the terminal takes the new size, a moment after.
  $effect(() => {
    void width
    void height
    // On a tool's tab, the screen has no height to measure.
    if (!sized || !term || !size || height < 60) return
    const t = setTimeout(() => {
      const next = cells()
      if (!term || !size || !ws || ws.readyState !== WebSocket.OPEN || (next.cols === size.cols && next.rows === size.rows)) return
      size = next
      term.resize(next.cols, next.rows)
      ws.send(JSON.stringify({ t: 'resize', ...next }))
    }, 250)
    return () => clearTimeout(t)
  })

  /**
   * History: what scrolled off the terminal's screen, read from tmux (a watching client has none
   * of it), cleaned of the CLI's frame, above the live screen in the same scroll; read again every
   * few seconds, kept at the bottom unless scrolled up.
   */
  let past = $state<Line[]>([])
  let pastText = ''
  let scroller: HTMLDivElement | undefined = $state()
  let atBottom = $state(true)
  const withHistory = $derived(tab === 'terminal' && !control && !zoomed)
  async function readHistory() {
    try {
      const text = await invoke<string>('agent_history', { id, lines: 3000, screen: false })
      if (text === pastText) return
      pastText = text
      past = cleanLines(parse(text.replace(/\n+$/, '')))
      if (atBottom) requestAnimationFrame(toLive)
    } catch (e) {
      error = String(e)
    }
  }
  $effect(() => {
    if (!withHistory) return
    atBottom = true
    void readHistory()
    const t = setInterval(readHistory, 3000)
    return () => clearInterval(t)
  })
  const toLive = () => scroller && (scroller.scrollTop = scroller.scrollHeight)
  const onscroll = () => scroller && (atBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 24)

  /** The Chat tab's few lines of the terminal, live: its screen's last ones, from its buffer. */
  let tail = $state<string[]>([])
  function readTail() {
    if (!term) return
    const b = term.buffer.active
    const lines: string[] = []
    for (let i = b.baseY; i < b.baseY + term.rows; i++) lines.push(b.getLine(i)?.translateToString(true) ?? '')
    // The CLI's empty input line says nothing.
    const next = cleanText(lines).filter((l) => !/^\s*[>›❯]?\s*$/.test(l)).slice(-5)
    if (next.join('\n') !== tail.join('\n')) tail = next
  }
  $effect(() => {
    if (tab !== 'chat') return
    readTail()
    const t = setInterval(readTail, 1000)
    return () => clearInterval(t)
  })

  const KEYS: [string, string][] = [['Esc', '\x1b'], ['Tab', '\t'], ['↑', '\x1b[A'], ['↓', '\x1b[B'], ['←', '\x1b[D'], ['→', '\x1b[C'], ['⏎', '\r'], ['^C', '\x03']]
</script>

<div class="tty">
  <header>
    <button class="back" onclick={onclose} aria-label={t('Back')}><Icon name="back" /></button>
    {#if !shell && quickAgent}
      <span class="tt">
        <b>{title}</b>
        <small><span class="st {quickAgent.status}"></span>{stateWords}{#if quickAgent.branch}{` · ${quickAgent.branch}`}{/if}{#if quickAgent.category}{` · ${quickAgent.category}`}{/if}</small>
      </span>
    {:else}
      <b>{title}</b>
    {/if}
    {#if !shell && quickAgent && live.snapshot && overlapsOfAgent(live.snapshot, quickAgent).length}<OverlapChip seen={overlapsOfAgent(live.snapshot, quickAgent)} short />{/if}
    {#if !shell && quickAgent && takesEffort(quickAgent)}
      {@const level = effortOf(quickAgent) ?? cliEffort(quickAgent)}
      <button class="effchip" class:on={effortOpen} onclick={() => { effortOpen = !effortOpen; menu = false }} aria-expanded={effortOpen} aria-label={t('Effort')}><small>{t('Effort')}</small>{level ?? t('Default')}<Icon name="down" size={10} /></button>
    {/if}
    {#if !shell && quickAgent}<button class="more" class:on={menu} onclick={() => { menu = !menu; effortOpen = false }} aria-expanded={menu} aria-label={t('Clear, stop or close the agent')}><Icon name="more" /></button>{/if}
  </header>
  {#if !shell && quickAgent && typing}<AgentLimitBanner a={quickAgent} />{/if}
  <!-- Its effort: a setting, under its chip in the header, over its Chat. -->
  {#if effortOpen && quickAgent && takesEffort(quickAgent)}
    {@const own = effortOf(quickAgent)}
    {@const cli = cliEffort(quickAgent)}
    <button class="scrim clear" onclick={() => (effortOpen = false)} aria-label={t('Close')} tabindex="-1"></button>
    <div class="effort" role="group" aria-label={t('Effort')}>
      <span class="lbl">{t('Effort')}<small>{t('Higher thinks longer and uses your limits faster; kept on resume')}</small></span>
      <div class="levels">
        <button class:on={!own} onclick={() => setEffort(null)} disabled={busy || !acts}>{cli ? t('{effort} (Settings)', { effort: cli }) : t('Default')}</button>
        {#each effortsOf(quickAgent) as l (l)}<button class:on={own === l} onclick={() => setEffort(l)} disabled={busy || !acts}>{l}</button>{/each}
      </div>
    </div>
  {/if}
  <!-- Its ⋯: what ends or resets it, an action sheet from the bottom. -->
  {#if menu && quickAgent}
    <button class="scrim" onclick={() => (menu = false)} aria-label={t('Close')} tabindex="-1"></button>
    <div class="actions" role="dialog" aria-label={t('Clear, stop or close the agent')}>
      <div class="group">
        <p class="head">{title}</p>
        {#if !critical}<p class="off">{t('Stopping or closing an agent from a phone is off (Settings › Remote › Exposure: critical actions)')}</p>{/if}
        {#if clearable}
          {@const working = quickAgent.status === 'running' || quickAgent.status === 'waiting_tool'}
          <button class:armed={armed.is('clear')} onclick={clear} disabled={busy || !acts || working}>{armed.is('clear') ? t('Tap again to clear') : t('Clear')}<small>{working ? t('Once its turn is over') : t('A new conversation; this one stays in its transcript')}</small></button>
        {/if}
        {#if quickAgent.state === 'running'}
          <button class:armed={armed.is('stop')} onclick={stop} disabled={busy || !critical}>{armed.is('stop') ? t('Tap again to stop') : t('Stop')}<small>{t('{cli} exits; Resume picks it up', { cli: cliOf(quickAgent.provider).name })}</small></button>
        {/if}
        <button class="ko" class:armed={armed.is('close')} onclick={close} disabled={busy || !critical}>{armed.is('close') ? (quickAgent.state === 'running' ? t('Tap again to stop and close') : t('Tap again to close')) : t('Close')}<small>{quickAgent.worktree ? t('Stops it, removes its worktree if nothing would be lost') : t('Stops it; the conversation stays')}</small></button>
      </div>
      {#if quickAgent.state !== 'prepared'}
        {@const found = installedClis()}
        <div class="group">
          <p class="head">{t('Continue with')}</p>
          {#if !typing}<p class="off">{t('The relay is typed to the agent: typing from a phone is off (Settings › Remote › Exposure)')}</p>{/if}
          {#each targetsOf(quickAgent) as p (p)}
            {@const use = planUse(p)}
            <button onclick={() => { menu = false; openSwitch(quickAgent!, p) }} disabled={!typing || (!!found && !found.includes(p))}>{CLIS[p].cli}<small>{use !== null ? t('{pct} % of its plan used', { pct: Math.round(use) }) : t('from a relay: what was done, where it is')}</small></button>
          {/each}
        </div>
      {/if}
      <button class="cancel" onclick={() => (menu = false)}>{t('Cancel')}</button>
    </div>
  {/if}
  <!-- In its Chat, its chain is a chip of the rail. -->
  {#if !shell && quickAgent && !calm}<AgentChain agent={quickAgent} />{/if}
  {#if !shell}
    <nav class="tabs" aria-label={t('The agent’s tools')}>
      {#each shownTabs as [tb, label] (tb)}<button class:on={tab === tb} onclick={() => (picked = tb)} aria-pressed={tab === tb}>{label}</button>{/each}
      <button class="moretab" class:on={more} onclick={() => (more = !more)} aria-expanded={more}>{t('More')} <Icon name="down" size={11} /></button>
    </nav>
  {/if}
  {#if more}
    <button class="scrim" onclick={() => (more = false)} aria-label={t('Close')} tabindex="-1"></button>
    <div class="sheet" role="dialog" aria-label={t('Its tools')}>
      <div class="grab"></div>
      <div class="grid">
        {#each TABS.filter(([tb]) => !MAIN.includes(tb)) as [tb, label] (tb)}
          <button class:on={tab === tb} onclick={() => { picked = tb; more = false }}>{label}</button>
        {/each}
      </div>
    </div>
  {/if}
  {#if isSide(tab)}
    {#if quickAgent}<div class="side"><AgentSide agent={quickAgent} bind:tab={() => tab as SideTab, (tb) => (picked = tb)} /></div>{/if}
  {:else if tab === 'shell' || tab === 'tasks' || tab === 'web'}
    <AgentTools {id} {tab} onask={(text) => { write(text); picked = chats ? 'chat' : 'terminal' }} />
  {/if}
  <div class="term" class:hidden={tab !== 'terminal' && tab !== 'chat'}>
  {#if tab === 'chat'}
    <!-- The terminal stays under the conversation, sized: its last lines are the Chat's. -->
  {:else if control}
    <div class="banner rw"><Icon name="edit" /> {shell ? t('Typing · the shell follows this size') : t('Typing · the agent’s window follows this size')}<span class="right"><button onclick={release}>{t('Release')}</button></span></div>
  {:else}
    <div class="banner">
      <Icon name="show" /> {acts ? t('Answering through the box below') : t('Watching, read-only')}
      <span class="right">
        {#if withHistory && !atBottom}<button onclick={toLive}>↓ {t('Live')}</button>{/if}
        {#if !sized}<button onclick={() => (zoomed = !zoomed)}>{zoomed ? t('Fit') : tc('infra', 'Zoom')}</button>{/if}
        {#if typing}<button onclick={take} disabled={busy} title={t('Type in the terminal, confirmed by this phone’s passkey')}>{t('Take')}</button>{/if}
      </span>
    </div>
  {/if}
  {#if stopped}
    <div class="banner off">
      {quickAgent?.state === 'gone' ? t('Its terminal is gone') : t('Stopped')}
      {#if acts}<span class="right"><button onclick={resume} disabled={busy} title={t('{command}, same folder', { command: cliOf(quickAgent?.provider).resume })}>{busy ? '…' : `↻ ${t('Resume')}`}</button></span>{/if}
    </div>
  {/if}
  {#if prepared}
    <div class="banner off">
      {t('Prepared, not started: Start launches it as prepared.')}
      {#if acts}<span class="right"><button onclick={start} disabled={busy} title={t('Start it now, as prepared (its worktree is made then)')}>{busy ? '…' : `▶ ${t('Start')}`}</button></span>{/if}
    </div>
  {/if}
  {#if error}<p class="error">{error}</p>{:else if note}<p class="note">{note}</p>{/if}
  <div class="stage">
    <div class="scroller" class:under={tab === 'chat'} bind:this={scroller} {onscroll}>
      {#if withHistory && past.length}
        <div class="past">
          {#each past as l, i (i)}<div class="pl">{#each l.segs as s, j (j)}<span class={s.fg} class:b={s.bold} style:color={s.color}>{s.text}</span>{/each}{#if !l.segs.length}&nbsp;{/if}</div>{/each}
        </div>
        <div class="seam">{t('live')}</div>
      {/if}
      <div class="screen" class:zoomed={zoomed && !control} class:watched={!control} bind:this={screen} bind:clientWidth={width} bind:clientHeight={height}>
        <div bind:this={host} class="host"></div>
      </div>
    </div>
    {#if tab === 'chat'}
      <div class="over"><ChatView {id} agent={quickAgent} working={quickAgent?.status === 'running'} acts={acts && !stopped} {typing} passkey {tail} onterminal={() => (picked = 'terminal')} onplan={() => (picked = 'plan')} /></div>
    {/if}
  </div>
  {#if !shell && (context || usage) && (!ctxLine || ctxOpen)}
    <footer class="ctxbar">{#if context}<ContextGauge {context} fill />{:else}<span class="grow"></span>{/if}{#if usage}<AgentUsageChip {usage} provider={quickAgent?.provider} short />{/if}</footer>
  {/if}
  {#if control}
    <div class="keys">
      <button class:on={ctrl} onclick={() => (ctrl = !ctrl)}>Ctrl</button>
      {#if shell}<button onclick={() => key('\x1b[A\r')} title={t('Its last command again (↑ then Enter)')}>↻</button>{/if}
      {#each KEYS as [label, seq] (label)}<button onclick={() => key(seq)}>{label}</button>{/each}
    </div>
  {:else if acts && !stopped}
    <div class="answer" class:calm>
      {#if ctxLine && context}
        <!-- Its context, a line on the dock's edge: a tap, the detail. -->
        {@const pct = context.size ? Math.min(100, (context.used / context.size) * 100) : 0}
        <button class="ctxline" class:warn={pct >= 70} onclick={() => (ctxOpen = !ctxOpen)} aria-expanded={ctxOpen} aria-label={t('Context {pct}%: the detail', { pct: Math.round(pct) })}><i style:width="{pct}%"></i></button>
      {/if}
      {#if calm && status === 'waiting_tool'}
        <div class="perm" role="group" aria-label={t('Its permission prompt')}>
          <b><Icon name="warning" /> {t('It asks')}</b>
          {#if quickAgent?.needs_action}<code>{quickAgent.needs_action}</code>{/if}
          <div class="pr">
            <button class="allow" onclick={() => act('agent_allow')} disabled={busy}><Icon name="check" /> {t('Allow once')}</button>
            <button onclick={() => (picked = 'terminal')}>{t('Terminal')}</button>
          </div>
        </div>
      {/if}
      {#if quickAgent && !shell}<div class="qrow"><QuickAnswers agent={quickAgent} /></div>{/if}
      <div class="quick" class:hidden={calm && !keysOpen}>
        {#if status === 'waiting_tool' && !calm}<button class="allow" onclick={() => act('agent_allow')} disabled={busy}><Icon name="check" /> {t('Allow once')}</button>{/if}
        <!-- A permission's other choices ("don't ask again"…) are answered at the desk. -->
        {#each status === 'waiting_tool' && !shell ? [] : ['1', '2', '3'] as n (n)}<button onclick={() => act('agent_key', { key: n })} disabled={busy} title={shell ? t('Choice {n} of its prompt', { n }) : t('Choice {n} of the agent’s question', { n })}>{n}</button>{/each}
        {#if shell}
          <button onclick={() => act('agent_key', { key: 'Escape' })} disabled={busy} title="Esc">Esc</button>
          <button onclick={() => act('agent_key', { key: 'C-c' })} disabled={busy} title={t('Stop what the shell runs (Ctrl-C)')}>^C</button>
          <button class="again" onclick={again} disabled={busy} title={t('Run its last command again (↑ then Enter)')}><Icon name="retry" /> {t('Again')}</button>
        {:else}
          <button onclick={() => act('agent_interrupt')} disabled={busy} title={t('Interrupt the agent’s turn (Esc): the agent stays')}><Icon name="pause" /> {t('Interrupt')}</button>
        {/if}
      </div>
      {#if !shell}<AttachStrip {id} {text} {write} />{/if}
      <!-- Return adds a line: only the button sends. -->
      <form class="compose" onsubmit={(e) => { e.preventDefault(); send() }}>
        {#if !shell}<SlashMenu {id} bind:text={() => text, write} {field} />{/if}
        <!-- Images: sent with the words, so only while typing is on. -->
        {#if !shell}<AttachButton {id} {text} {write} disabled={!typing} onerror={(e) => (error = e)} phone />{/if}
        <textarea bind:this={field} bind:value={() => text, write} rows="1" disabled={!typing} placeholder={typing ? (shell ? t('Type in the shell…') : t('Answer the agent…')) : t('Typing from a phone is off (Settings › Remote)')} enterkeyhint="enter" oninput={() => (sent = false)}></textarea>
        {#if calm}<button type="button" class="kbd" class:on={keysOpen} onclick={() => (keysOpen = !keysOpen)} aria-pressed={keysOpen} aria-label={t('Its menus’ keys: 1 2 3, Esc')}>⌨︎</button>{/if}
        <button type="submit" class:round={calm} disabled={busy || !typing || !text.trim()} aria-label={t('Send, confirmed by this phone’s passkey')}><Icon name="lock" />{#if !calm}{t('Send')}{/if}</button>
        {#if calm && sent}<span class="sent">✓ {t('Sent')}</span>{/if}
      </form>
      {#if typing && !calm}<p class="hint" class:ok={sent}>{sent ? `✓ ${t('Sent')}` : shell ? t('Your passkey confirms it, then the shell runs it at once') : t('Your passkey confirms it, then the agent gets it at once')}</p>{/if}
    </div>
  {/if}
  </div>
</div>

<style>
  /* Under the drawers (48–53): an MR, a ticket or a log opened from its Chat shows over it. Its own sheets and menus stay within it. */
  .tty { position: fixed; inset: 0; z-index: 47; display: flex; flex-direction: column; background: var(--surface); color: var(--text); }
  header { display: flex; align-items: center; gap: 8px; padding: calc(10px + env(safe-area-inset-top)) 12px 10px; background: var(--surface); color: var(--text); border-bottom: 1px solid var(--border); }
  header b { font-size: 15px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  header .tt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  header small { display: flex; align-items: center; gap: 5px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11.5px; color: var(--text-muted); }
  header .st { flex: none; width: 7px; height: 7px; border-radius: 50%; background: var(--text-muted); }
  header .st.running { background: var(--warn); animation: pulse 1.2s infinite; }
  header .st.waiting_tool, header .st.waiting_input { background: var(--warn); }
  @keyframes pulse { 50% { opacity: 0.3; } }
  .back { background: none; border: none; color: var(--info); font-size: 18px; padding: 0 4px; }
  .more { margin-left: auto; display: grid; place-items: center; width: 34px; height: 30px; border: 1px solid transparent; border-radius: 8px; background: none; color: var(--text-muted); font-size: 16px; }
  .more.on { border-color: var(--border); background: var(--surface-muted); color: var(--text); }
  /* Its effort, a chip by its ⋯: the level it runs at, always in sight. */
  .effchip { flex: none; display: inline-flex; align-items: center; gap: 5px; padding: 4px 9px; border: 1px solid var(--border); border-radius: 14px; background: var(--surface-muted); color: var(--text); font: inherit; font-size: 12.5px; }
  .effchip small { font-size: 11px; color: var(--text-muted); }
  .effchip.on { border-color: var(--accent); }
  .effchip + .more { margin-left: 0; }
  header .effchip { margin-left: auto; }
  .scrim.clear { background: none; }
  .effort { position: fixed; left: 10px; right: 10px; top: calc(60px + env(safe-area-inset-top)); z-index: 71; display: flex; flex-direction: column; gap: 8px; padding: 12px; border: 1px solid var(--border); border-radius: 14px; background: var(--surface); color: var(--text); box-shadow: 0 12px 30px rgba(0, 0, 0, 0.3); }
  .effort .lbl { display: flex; flex-direction: column; gap: 2px; font-size: 14px; font-weight: 600; }
  .effort .lbl small { font-size: 12px; font-weight: 400; color: var(--text-muted); }
  .effort .levels { display: flex; flex-wrap: wrap; gap: 6px; }
  .effort .levels button { padding: 7px 12px; border: 1px solid var(--border); border-radius: 16px; background: var(--surface-muted); color: var(--text); font: inherit; font-size: 13px; }
  .effort .levels button.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 16%, var(--surface)); font-weight: 600; }
  .effort .levels button:disabled { opacity: 0.5; }
  /* Its ⋯: an action sheet, each action one line and a short why. */
  .actions { position: fixed; left: 10px; right: 10px; bottom: calc(10px + env(safe-area-inset-bottom)); z-index: 71; display: flex; flex-direction: column; gap: 8px; }
  .actions .group { display: flex; flex-direction: column; border-radius: 14px; overflow: hidden; background: var(--surface); color: var(--text); }
  .actions .head { margin: 0; padding: 10px 14px; text-align: center; font-size: 12px; color: var(--text-muted); border-bottom: 1px solid var(--border); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .actions .off { margin: 0; padding: 8px 14px; font-size: 12px; color: var(--warn); border-bottom: 1px solid var(--border); }
  .actions button { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 12px 14px; border: none; border-bottom: 1px solid var(--border); background: none; color: var(--info); font: inherit; font-size: 16px; }
  .actions .group button:last-child { border-bottom: none; }
  .actions button small { font-size: 11.5px; color: var(--text-muted); }
  .actions button.ko { color: var(--ko); }
  .actions button.armed { background: var(--ko-bg); color: var(--ko); font-weight: 600; }
  .actions button:disabled { opacity: 0.5; }
  .actions .cancel { border-radius: 14px; border: none; background: var(--surface); font-weight: 600; }
  /* The window's panels (Files, Diff, Activity, Launch), the full width. */
  .side { flex: 1; min-height: 0; display: grid; grid-template: minmax(0, 1fr) / minmax(0, 1fr); background: var(--surface); color: var(--text); }
  .side :global(.side) { border-left: none; }
  .tabs { display: flex; background: var(--surface); border-bottom: 1px solid var(--border); overflow-x: auto; scrollbar-width: none; }
  .tabs::-webkit-scrollbar { display: none; }
  .tabs button { flex: 1 0 auto; padding: 8px 12px; border: none; border-bottom: 2px solid transparent; background: none; color: var(--text-muted); font: inherit; font-size: 13px; }
  .tabs button.on { color: var(--text); border-bottom-color: var(--accent); font-weight: 600; }
  .tabs .moretab { display: inline-flex; align-items: center; justify-content: center; gap: 3px; }
  /* More: the other tools, a sheet from the bottom. */
  .scrim { position: fixed; inset: 0; z-index: 70; border: none; padding: 0; background: rgba(0, 0, 0, 0.3); }
  .sheet { position: fixed; left: 0; right: 0; bottom: 0; z-index: 71; padding: 8px 14px calc(16px + env(safe-area-inset-bottom)); border-radius: 18px 18px 0 0; background: var(--surface); color: var(--text); box-shadow: 0 -8px 30px rgba(0, 0, 0, 0.25); }
  .grab { width: 38px; height: 5px; border-radius: 3px; background: var(--border); margin: 0 auto 12px; }
  .grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
  .grid button { padding: 14px 0; border: 1px solid var(--border); border-radius: 12px; background: var(--surface-muted); color: var(--text); font: inherit; font-size: 14px; }
  .grid button.on { border-color: var(--accent); color: var(--accent); font-weight: 600; }
  .term { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .term.hidden { display: none; }
  .banner { display: flex; align-items: center; gap: 6px; padding: 7px 12px; font-size: 12px; background: var(--info-bg); color: var(--info); }
  .banner.rw { background: var(--warn-bg); color: var(--warn); }
  .banner.off { background: var(--surface-muted); color: var(--text-muted); }
  .banner .right { margin-left: auto; display: flex; gap: 6px; }
  .banner button { background: none; border: 1px solid currentColor; border-radius: 8px; color: inherit; font-size: 12px; padding: 2px 10px; }
  .error, .note { margin: 0; padding: 8px 12px; font-size: 13px; }
  .error { color: var(--ko); }
  .note { color: var(--text-muted); }
  .stage { flex: 1; min-height: 0; position: relative; display: flex; flex-direction: column; }
  /* The history above the screen, one scroll; the screen one stage tall, measured even under the Chat. */
  .scroller { flex: 1; min-height: 0; overflow-y: auto; -webkit-overflow-scrolling: touch; overscroll-behavior: contain; }
  .over { position: absolute; inset: 0; display: flex; flex-direction: column; z-index: 1; }
  /* Under its Chat, the terminal stays attached and laid out, but unseen: a glass theme's Chat (Synthwave) would show it through. */
  .scroller.under { visibility: hidden; }
  .screen { height: 100%; overflow: hidden; padding: 6px 4px; }
  .screen.zoomed { overflow: auto; }
  .seam { display: flex; align-items: center; gap: 8px; margin: 4px 10px; color: var(--info); font: 600 10px var(--sans); letter-spacing: 0.05em; text-transform: uppercase; }
  .seam::before, .seam::after { content: ''; flex: 1; height: 1px; background: color-mix(in srgb, var(--info) 35%, transparent); }
  .ctxbar { display: flex; align-items: center; gap: 8px; padding: 4px 10px; background: var(--surface); color: var(--text); border-top: 1px solid var(--border); font-size: 11px; }
  .ctxbar .grow { flex: 1; }
  /* Watched: the terminal takes no tap (no keyboard, no focus); the screen still scrolls sideways when zoomed. */
  .screen.watched .host { pointer-events: none; }
  .past { padding: 8px 10px 0; font-size: 11px; line-height: 1.45; font-family: 'RecMonoCasual Nerd Font Mono', var(--mono); color: var(--text); }
  .pl { white-space: pre-wrap; overflow-wrap: anywhere; min-height: 1.45em; }
  .past .b { font-weight: 700; }
  .host { width: max-content; }
  .keys { display: flex; gap: 5px; padding: 6px 8px calc(6px + env(safe-area-inset-bottom)); background: var(--surface); border-top: 1px solid var(--border); overflow-x: auto; }
  .keys button { flex: none; min-width: 38px; padding: 7px 8px; border-radius: 7px; border: 1px solid var(--border); background: var(--surface-muted); color: var(--text); font: 12px ui-monospace, monospace; }
  .keys button.on { background: var(--info-bg); color: var(--info); border-color: var(--info); }
  .answer { display: flex; flex-direction: column; gap: 6px; padding: 8px 8px calc(8px + env(safe-area-inset-bottom)); background: var(--surface); border-top: 1px solid var(--border); }
  .quick { display: flex; gap: 6px; flex-wrap: wrap; }
  .quick button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-width: 38px; padding: 6px 10px; border-radius: 8px; border: 1px solid var(--border); background: var(--surface-muted); color: var(--text); font: 13px ui-monospace, monospace; }
  .quick .allow { display: inline-flex; align-items: center; gap: 4px; background: var(--ok-bg); color: var(--ok); border-color: color-mix(in srgb, var(--ok) 40%, transparent); font-family: inherit; font-weight: 600; }
  .quick .again { display: inline-flex; align-items: center; gap: 4px; margin-left: auto; font-family: inherit; font-weight: 600; color: var(--info); border-color: color-mix(in srgb, var(--info) 40%, transparent); }
  .compose { position: relative; display: flex; gap: 6px; align-items: flex-end; }
  .compose textarea { flex: 1; resize: none; max-height: 120px; padding: 9px 12px; border-radius: 18px; border: 1px solid var(--border); background: var(--surface-muted); color: var(--text); font: 15px/1.35 inherit; }
  .compose button { display: inline-flex; align-items: center; gap: 5px; height: 38px; padding: 0 14px; border-radius: 19px; border: none; background: var(--accent); color: var(--on-accent); font: inherit; font-size: 14px; font-weight: 600; }
  .compose button:disabled { opacity: 0.45; }
  .compose textarea { field-sizing: content; min-height: 38px; }
  .compose textarea:disabled { opacity: 0.6; }
  .hint { margin: 0 4px; color: var(--text-muted); font-size: 11.5px; }
  /* Its Chat's dock. */
  .answer.calm { position: relative; padding-top: 10px; }
  .ctxline { position: absolute; left: 0; right: 0; top: 0; height: 8px; padding: 0; border: none; background: none; }
  .ctxline::before { content: ''; position: absolute; inset: 0 0 auto; height: 2px; background: var(--surface-muted); }
  .ctxline i { position: absolute; left: 0; top: 0; height: 2px; background: var(--info); }
  .ctxline.warn i { background: var(--warn); }
  .perm { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border-radius: 14px; background: var(--warn-bg); color: var(--warn); font-size: 13px; }
  .perm b { display: inline-flex; align-items: center; gap: 6px; }
  .perm code { display: block; padding: 6px 9px; border-radius: 8px; background: var(--surface); color: var(--text); font: 12.5px ui-monospace, 'SF Mono', Menlo, monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
  .perm .pr { display: flex; gap: 8px; }
  .perm .pr button { flex: 1; display: inline-flex; align-items: center; justify-content: center; gap: 5px; height: 40px; border-radius: 20px; border: 1px solid var(--border); background: var(--surface); color: var(--text); font: inherit; font-size: 14px; font-weight: 600; }
  .perm .pr .allow { background: var(--warn); border-color: var(--warn); color: var(--surface); }
  .quick.hidden { display: none; }
  .compose .kbd { flex: none; width: 38px; padding: 0; justify-content: center; background: var(--surface-muted); color: var(--text-muted); border: 1px solid var(--border); font-size: 17px; font-weight: 400; }
  .compose .kbd.on { background: var(--info-bg); color: var(--info); border-color: var(--info); }
  .compose .round { width: 38px; padding: 0; justify-content: center; }
  .sent { position: absolute; right: 50px; top: -28px; padding: 2px 9px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); color: var(--ok); font-size: 12px; }
  .hint.ok { color: var(--ok); }
  button:disabled { opacity: 0.5; }
  /* Quick answers: one line, scrolled sideways. */
  .qrow { overflow-x: auto; }
  .qrow :global(.qa) { flex-wrap: nowrap; }
  .qrow :global(.qa button) { padding: 5px 12px; font-size: var(--fs-body); }
</style>
