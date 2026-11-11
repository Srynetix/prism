<script lang="ts">
  import { openOverlap } from '../overlap-view.svelte'
  import { openWhatChanged } from '../changes-view.svelte'
  import { forgeName, mrWord } from '../forge'
  import Sprout from './Sprout.svelte'
  import BlockControls from './BlockControls.svelte'
  import { isCollapsed, isShort } from '../layout.svelte'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import type { Alert, Fix, OpenTarget } from '../rules'
  import { isSeen, setSeen } from '../seen.svelte'
  import { hasCore, isDesktop } from '../bridge'
  import { openProposal } from '../mcp-view.svelte'
  import { canPreview, changeStatus, openTicket } from '../ticket-view.svelte'
  import { rebaseMr, retryPipeline, setAutoMerge, setMergeQueue } from '../pipeline-actions'
  import { canShowLogs, openPipeline } from '../logs.svelte'
  import { openMr } from '../mr-view.svelte'
  import { branchPipelineAction, claudeClick, devServerPrompt, investigateAction, mrActions, type ClaudeAction } from '../claude-actions'
  import { openAgents, promptAgent } from '../agents.svelte'
  import QuickAnswers from './QuickAnswers.svelte'
  import { branchName } from '../pipeline'
  import type { Agent, SessionStatus } from '../types'
  import { CLIS, agentName } from '../agent-cli'
  import { openSwitch, suggestedFor, targetName } from '../agent-switch.svelte'
  import { call } from '../blocks/block'
  import { live } from '../live.svelte'
  import { canShowPodLogs, openWorkload } from '../pod-logs.svelte'
  import { mrKey } from '../mr-ref'
  import { openSettings } from '../settings.svelte'
  import { openArgo } from '../argo-view.svelte'
  import { trackerName } from '../tracker'
  import { keys } from '../platform'
  import { t, tn, tc } from '../i18n.svelte'
  import { anchored, dismissable } from '../popup'
  import { clock } from '../clock.svelte'
  import { alertSettings } from '../conventions'
  import { ticketIndex } from '../rules'
  import { ticketKey } from '../tracker'
  import { arrangeAlerts, BAND_LABEL, type AlertGroup, type Band } from '../alert-bands'
  import { alertMinutes, isSnoozed, snooze, snoozeChoices, snoozedUntil, wake, type SnoozeChoice } from '../alert-snooze.svelte'
  import { locale } from '../locale'
  import { used } from '../feature-use.svelte'
  /**
   * `phone`: the remote's view — by band, each alert in full, its actions under it, ✓ always there.
   * `popup`: the status bar's, the bands' list alone.
   */
  let { alerts, phone = false, popup = false }: { alerts: Alert[]; phone?: boolean; popup?: boolean } = $props()

  /**
   * Fixes for an alert (the core's, `prism_core::alerts`), in one click: change the ticket status, take the MR out of
   * draft, rebase, retry or read the pipeline logs, open the MR, or the Claude action. Those
   * that go through the app do not exist in the browser preview. The alert disappears by itself
   * when the data catches up; until then, the button shows ✓.
   */
  interface QuickFix {
    key: string
    label: string
    title: string
    run?: () => Promise<unknown> | unknown
    /** A Claude action: the Agents launcher, or Claude Desktop (`[agents] target`). */
    claude?: ClaudeAction
    /** An agent already on it: the fix gives way to it, a click opens it. */
    agent?: Agent
  }
  const agents = $derived((live.snapshot?.agents ?? []).filter((a) => a.state === 'running'))
  const STATUS: Record<SessionStatus, string> = { running: t('on it'), background: t('on it, in the background'), waiting_tool: t('asks permission'), needs_you: t('waits for you'), review_ready: t('ready for review'), completed: tc('core', 'done'), idle: t('idle') }
  /** The agent working on an alert: launched on the MR's branch, or under the fix's title. */
  const onIt = (match: (a: Agent) => boolean) => agents.find(match)
  const agentFix = (a: Agent, key: string): QuickFix => ({ key: `on-${key}`, label: `${agentName()} ${a.gate ? t('waits for your trust') : STATUS[a.status]}`, title: t('{title} · open it', { title: a.title }), agent: a })
  /** Told now when it waits for a prompt; otherwise typed in its prompt, for you to send once it is free. */
  const waitsForPrompt = (a: Agent) => a.status === 'idle' || a.status === 'background' || a.status === 'completed' || a.status === 'review_ready' || a.status === 'needs_you'
  function quickFixes(a: Alert): QuickFix[] {
    const app = hasCore()
    return (a.fixes ?? []).flatMap((f: Fix): QuickFix[] => {
      switch (f.kind) {
        case 'status':
          return app ? [{ key: `st-${f.status}`, label: `→ ${f.status}`, title: t('Move {ticket} to “{status}”', { ticket: f.ticket, status: f.status }), run: () => changeStatus(f.task_id, f.status, f.statuses) }] : []
        case 'rebase':
          return app ? [{ key: 'rebase', label: t('Rebase'), title: t('{forge} rebases {mr}', { forge: forgeName(), mr: mrKey(f) }), run: () => rebaseMr(f) }] : []
        case 'automerge':
          return app ? [{ key: 'automerge', label: t('Auto-merge'), title: t('{forge} merges {mr} as soon as the pipeline is green', { forge: forgeName(), mr: mrKey(f) }), run: () => setAutoMerge(f, true) }] : []
        case 'retry':
          return app ? [{ key: 'retry', label: t('Retry'), title: t('Retry the failed jobs of {label}', { label: f.label }), run: () => retryPipeline(f.pipeline) }] : []
        case 'logs':
          return canShowLogs() ? [{ key: 'logs', label: t('Logs'), title: t('Open the logs of {label}', { label: f.label }), run: () => openPipeline(f.pipeline, f.label, f.project ?? null) }] : []
        case 'queue': {
          const mr = live.snapshot?.merge_requests.find((m) => (m.repo ?? null) === (f.repo ?? null) && m.iid === f.iid)
          const label = live.snapshot?.merge_queue_label
          return app && mr && label ? [{ key: 'queue', label: t('Merge queue'), title: t('Add the “{label}” label to {mr}: the CI job rebases, then auto-merges', { label, mr: mrKey(f) }), run: () => setMergeQueue(mr, true) }] : []
        }
        case 'mr':
          return app ? [{ key: 'mr', label: mrKey(f), title: t('Open {name}', { name: mrKey(f) }), run: () => openMr(f) }] : []
        case 'podlogs':
          return canShowPodLogs() ? [{ key: 'podlogs', label: t('Pod logs'), title: t('Logs of {name}', { name: f.deployment }), run: () => openWorkload({ context: f.context, namespace: f.namespace, deployment: f.deployment, label: f.label }) }] : []
        case 'proposal':
          return isDesktop() ? [{ key: `proposal-${f.id}`, label: t('See the proposal'), title: t('Its diff, then apply or refuse it'), run: () => openProposal(f.id) }] : []
        case 'settings':
          return [{ key: `settings-${f.section}`, label: f.label, title: t('Open {name}', { name: f.label }), run: () => openSettings(f.section) }]
        case 'claude': {
          const mr = live.snapshot?.merge_requests.find((m) => (m.repo ?? null) === (f.repo ?? null) && m.iid === f.iid)
          const busy = mr && onIt((a) => (a.mr ? a.mr.iid === mr.iid && (a.mr.repo ?? null) === (mr.repo ?? null) : a.branch === mr.source_branch))
          if (busy) return [agentFix(busy, f.action)]
          const action = mr ? mrActions(mr).find((x) => x.id === f.action && (x.href || x.launch)) : undefined
          return action ? [{ key: `claude-${f.action}`, label: action.label, title: titleOf(action), claude: action }] : []
        }
        case 'fix-branch': {
          const p = live.snapshot?.pipelines.find((x) => x.id === f.pipeline)
          if (!p) return []
          const action = branchPipelineAction(p, branchName(p))
          const busy = onIt((a) => a.title === action.launch?.title)
          if (busy) return [agentFix(busy, 'fix-branch')]
          return action.launch || action.href ? [{ key: 'claude-fix-branch', label: action.label, title: titleOf(action), claude: action }] : []
        }
        case 'investigate': {
          const action = investigateAction(f.workload, f.namespace, f.context, f.env, f.issue)
          const busy = onIt((a) => a.title === action.launch?.title)
          if (busy) return [agentFix(busy, 'investigate')]
          return action.launch || action.href ? [{ key: 'claude-investigate', label: action.label, title: titleOf(action), claude: action }] : []
        }
        case 'dev-server': {
          const agent = agents.find((a) => a.id === f.agent)
          if (!app || !agent) return []
          if (agent.status === 'running') return [agentFix(agent, 'dev-server')]
          const text = devServerPrompt(f.command, f.where, f.tail)
          const now = waitsForPrompt(agent)
          return [{ key: 'dev-server', label: t('Tell {agent}', { agent: agentName() }), title: now ? t('Send its agent the error, to fix it:\n\n{text}', { text }) : t('Its agent is busy: the error is typed in its prompt, for you to send.\n\n{text}', { text }), run: async () => { await promptAgent(agent.id, text, now); openAgents(agent.id) } }]
        }
        case 'switch': {
          const agent = live.snapshot?.agents?.find((x) => x.id === f.agent)
          const to = agent ? suggestedFor(agent) : null
          return app && agent ? [{ key: 'switch', label: to ? t('Continue with {cli}…', { cli: targetName(to) }) : t('Continue with…'), title: t('Another CLI takes {title} on, from a relay', { title: agent.title }), run: () => openSwitch(agent, to, 'limit') }] : []
        }
        case 'overlap':
          return [{ key: 'overlap', label: t('See the overlap'), title: t('Both sides’ hunks, side by side'), run: () => openOverlap(f.id, f.side) }]
        case 'changes':
          return [{ key: 'changes', label: t('What changed'), title: t('The timeline of {env} around the crash, the change just before it highlighted', { env: f.env ?? '' }), run: () => openWhatChanged(f.env, f.app) }]
        case 'block':
          return app ? [{ key: `block-${f.block}-${f.command}`, label: f.label, title: f.title, run: () => call(f.block, f.command, f.args) }] : []
      }
    })
  }

  /** A Claude action's tooltip: its prompt, and what Prism adds to it. */
  const titleOf = (a: ClaudeAction) => `${a.prompt}${a.note ? `\n(${a.note})` : ''}`

  /** State of the fixes launched, by alert and fix. */
  let progress = $state<Record<string, 'busy' | 'done' | string>>({})
  async function apply(a: Alert, f: QuickFix) {
    const id = `${a.text}|${f.key}`
    if (progress[id] === 'busy' || !f.run) return
    progress[id] = 'busy'
    // Settings › Cleanup: an alert followed through its fix (agent actions count as theirs, `action:<id>`).
    used(`alert:${a.rule ?? a.kind}`)
    try {
      await f.run()
      // Opening a drawer is not "fixing": the button stays as is.
      progress[id] = f.key === 'logs' || f.key === 'mr' || f.key === 'podlogs' || f.key === 'changes' || f.key.startsWith('settings-') ? '' : 'done'
    } catch (e) {
      progress[id] = String(e)
    }
  }
  /**
   * "Open": the alert's object in the app (ticket preview, MR pane, pipeline logs, ArgoCD drawer);
   * ⌘-click, or a target the app cannot show (browser preview, tracker off), keeps the link.
   */
  function inApp(o: OpenTarget | undefined): (() => void) | null {
    if (!o) return null
    switch (o.kind) {
      case 'ticket':
        return canPreview() ? () => openTicket(o.id) : null
      case 'mr':
        return hasCore() ? () => openMr(o) : null
      case 'pipeline':
        return canShowLogs() ? () => openPipeline(o.pipeline, o.label, o.project ?? null) : null
      case 'argo':
        return () => openArgo(o.cluster, o.app)
    }
  }
  const outside = (o: OpenTarget) => (o.kind === 'ticket' ? trackerName() : o.kind === 'argo' ? 'ArgoCD' : forgeName())
  function openClick(a: Alert) {
    return (e: MouseEvent) => {
      const go = inApp(a.open)
      if (!go || e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return
      e.preventDefault()
      go()
    }
  }
  const stateOf = (a: Alert, f: QuickFix) => progress[`${a.text}|${f.key}`] ?? ''
  const now = $derived(clock(60_000).now)
  const settings = $derived(alertSettings())
  /** Seen items move to a collapsed section at the bottom: they no longer count. Put off (⏾) ones too, in their own. */
  const unseen = $derived(alerts.filter((a) => !isSeen(a)))
  const snoozed = $derived(unseen.filter((a) => isSnoozed(a, now)))
  const seen = $derived(alerts.filter(isSeen))
  /** By band (alert-bands.ts), an object's alerts on one line; the phone shows each in full. */
  const bands = $derived(arrangeAlerts(unseen.filter((a) => !isSnoozed(a, now)), settings, (a) => alertMinutes(a, now), settings.group && !phone))
  const fresh = $derived(bands.flatMap((b) => b.groups.flatMap((g) => g.alerts)))
  const urgent = $derived(bands.some((b) => b.band === 'now'))
  /** A red alert not yet seen: the block's border turns red (orange ones, the lines say it). */
  const critical = $derived(fresh.some((a) => a.severity === 'ko'))
  let showSeen = $state(false)
  let showSnoozed = $state(false)
  /** Short: Now and Today, three lines at most. */
  const SHORT = 3
  const pressing = $derived(bands.filter((b) => b.band !== 'later').flatMap((b) => b.groups))

  /** Groups unfolded, by key. */
  let unfolded = $state<Record<string, boolean>>({})
  /** A group's object, named: its MR's or ticket's id and title. */
  function objectOf(g: AlertGroup): { ref: string; title: string } {
    const o = g.alerts[0].open
    const s = live.snapshot
    switch (o?.kind) {
      case 'mr':
        return { ref: mrKey(o), title: s?.merge_requests.find((m) => (m.repo ?? null) === (o.repo ?? null) && m.iid === o.iid)?.title ?? '' }
      case 'ticket':
        return { ref: o.id, title: (s && [...ticketIndex(s).values()].find((e) => ticketKey(e.t) === o.id)?.t.name) ?? '' }
      case 'pipeline':
        return { ref: o.label, title: '' }
      case 'argo':
        return { ref: o.app, title: '' }
      default:
        return { ref: '', title: g.alerts[0].text }
    }
  }
  /** Its age, short: 40m, 3h, 2d; nothing under a minute or unknown. */
  const age = (m: number | null) => (m === null || m < 1 ? '' : m < 60 ? t('{n}m', { n: Math.round(m) }) : m < 1440 ? t('{n}h', { n: Math.round(m / 60) }) : t('{n}d', { n: Math.round(m / 1440) }))
  const hm = (ms: number) => new Date(ms).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
  /** Until when an alert put off is away, in words. */
  function awayUntil(a: Alert): string {
    const until = snoozedUntil(a)
    if (until === null) return t('until it changes')
    if (until === undefined) return ''
    const day = new Date(until).toDateString() === new Date(now).toDateString()
    return day ? t('until {time}', { time: hm(until) }) : t('until {day} {time}', { day: new Date(until).toLocaleDateString(locale(), { weekday: 'short' }), time: hm(until) })
  }

  /** ⏾ open: the alerts it puts off, and its button. */
  let putting = $state<{ alerts: Alert[]; el: HTMLElement } | null>(null)
  const choices = $derived(putting ? snoozeChoices(live.snapshot, now) : [])
  function putOff(c: SnoozeChoice) {
    if (putting) snooze(putting.alerts, c.until)
    used('alert:snooze')
    putting = null
  }
  /**
   * Under NARROW px (a quarter of the screen), the block's lines become cards: on one line, the
   * fixes left the text no room.
   */
  const NARROW = 560
  let width = $state(0)
  const narrow = $derived(width > 0 && width < NARROW)
  /** The phone's ⏾: the choices under the alert. */
  let phonePutting = $state<string | null>(null)
</script>

<!-- The phone's alert: its kind, its text in full, its fixes and Open under it; ⏾ and ✓ beside. -->
{#snippet prow(a: Alert, marked: boolean, away = false)}
  {@const fixes = quickFixes(a)}
  {@const ag = a.answer ? live.snapshot?.agents?.find((x) => x.id === a.answer) : undefined}
  {@const key = `${a.kind}|${a.text}`}
  <div class="pal {a.severity}" class:seen={marked || away}>
    <span class="sev" aria-hidden="true"></span>
    <div class="pbody">
      <span class="kind">{a.kind === 'MR' ? mrWord() : a.kind === 'Claude' ? agentName() : t(a.kind)}{#if away}{' '}· {awayUntil(a)}{:else if age(alertMinutes(a, now))}{' '}· {age(alertMinutes(a, now))}{/if}</span>
      <p class="ptext">{a.text}</p>
      {#if fixes.length || ag || a.url}
        <div class="pacts">
          {#each fixes as f (f.key)}
            {#if f.agent}
              {@const on = f.agent}
              <button class="pbtn claude" onclick={() => openAgents(on.id)}><span class="pulse" class:live={on.status === 'running'}></span>{f.label}</button>
            {:else if f.claude}
              <a class="pbtn claude" href={f.claude.href} onclick={claudeClick(f.claude)}><Logo name="agent" size={11} />{f.label}</a>
            {:else}
              {@const st = stateOf(a, f)}
              <button class="pbtn" class:done={st === 'done'} class:err={!!st && st !== 'busy' && st !== 'done'} disabled={st === 'busy' || st === 'done'} onclick={() => apply(a, f)}>{#if st === 'done'}<Icon name="check" />{/if}{st === 'busy' ? `${f.label}…` : f.label}</button>
            {/if}
          {/each}
          {#if ag}<QuickAnswers agent={ag} />{/if}
          {#if a.url}<a class="pbtn" href={a.url} target="_blank" rel="noreferrer" onclick={openClick(a)}>{t('Open')}</a>{/if}
        </div>
      {/if}
      {#if phonePutting === key}
        <div class="pacts">
          {#each snoozeChoices(live.snapshot, now) as c (c.key)}
            <button class="pbtn" onclick={() => { snooze([a], c.until); phonePutting = null }}>{c.label}{#if c.detail}{' '}<small>{c.detail}</small>{/if}</button>
          {/each}
        </div>
      {/if}
    </div>
    {#if away}
      <button class="pmark" onclick={() => wake([a])} aria-label={t('Bring it back now')}><Icon name="undo" /></button>
    {:else}
      {#if !marked}<button class="pmark" class:on={phonePutting === key} onclick={() => (phonePutting = phonePutting === key ? null : key)} aria-label={t('Put off')}><Icon name="snooze" /></button>{/if}
      <button class="pmark" onclick={() => setSeen(a, !marked)} aria-label={marked ? t('Mark as new') : t('Mark as seen')}><Icon name={marked ? 'undo' : 'check'} /></button>
    {/if}
  </div>
{/snippet}

<!-- An alert's fixes, in one click, then Open; `lead`: the first stands out (the narrow block's cards). -->
{#snippet fixButtons(a: Alert, lead = false)}
  {#each quickFixes(a) as f, i (f.key)}
    {#if f.agent}
      {@const ag = f.agent}
      <button class="fix claude on" title={f.title} onclick={() => openAgents(ag.id)}><span class="pulse" class:live={ag.status === 'running'}></span>{f.label}</button>
    {:else if f.claude}
      <a class="fix claude" class:lead={lead && i === 0} href={f.claude.href} title={f.title} onclick={claudeClick(f.claude)}><Logo name="agent" size={10} />{f.label}</a>
    {:else}
      {@const st = stateOf(a, f)}
      <button class="fix" class:lead={lead && i === 0} class:busy={st === 'busy'} class:done={st === 'done'} class:err={!!st && st !== 'busy' && st !== 'done'} disabled={st === 'busy' || st === 'done'} title={st && st !== 'busy' && st !== 'done' ? t('Failed: {error}', { error: st }) : f.title} onclick={() => apply(a, f)}>{#if st === 'done'}<Icon name="check" />{f.label}{:else}{st === 'busy' ? `${f.label}…` : f.label}{/if}</button>
    {/if}
  {/each}
  {#if a.answer}{@const ag = live.snapshot?.agents?.find((x) => x.id === a.answer)}{#if ag}<QuickAnswers agent={ag} compact />{/if}{/if}
  {#if a.url}<a href={a.url} target="_blank" rel="noreferrer" onclick={openClick(a)} title={a.open && inApp(a.open) ? keys(t('click: open in Prism, ⌘-click: {where}', { where: outside(a.open) })) : undefined}>{t('Open')}</a>{/if}
{/snippet}

<!-- ⏾: puts these alerts off. -->
{#snippet putOffButton(list: Alert[])}
  <button class="mark" class:on={putting?.alerts === list} onclick={(e) => { const el = e.currentTarget; putting = putting?.el === el ? null : { alerts: list, el } }} title={t('Put off: back at a time, or when it changes')} aria-label={t('Put off')}><Icon name="snooze" /></button>
{/snippet}

{#snippet row(a: Alert, marked: boolean, away = false)}
  <div class="row" class:seen={marked || away}>
    <span class="pill {a.severity}">{a.kind === 'MR' ? mrWord() : a.kind === 'Claude' ? agentName() : t(a.kind)}</span>
    <span class="grow" title={a.text}>{a.text}</span>
    {#if away}<span class="age">{awayUntil(a)}</span>{:else if age(alertMinutes(a, now))}<span class="age" title={t('Flagged for {age}', { age: age(alertMinutes(a, now)) })}>{age(alertMinutes(a, now))}</span>{/if}
    {@render fixButtons(a)}
    {#if away}
      <button class="mark" onclick={() => wake([a])} title={t('Bring it back now')} aria-label={t('Bring it back now')}><Icon name="undo" /></button>
    {:else}
      {#if !marked}{@render putOffButton([a])}{/if}
      <button class="mark" onclick={() => setSeen(a, !marked)} title={marked ? t('Mark as new') : t('Mark as seen, no action needed')} aria-label={marked ? t('Mark as new') : t('Mark as seen')}><Icon name={marked ? 'undo' : 'check'} /></button>
    {/if}
  </div>
{/snippet}

<!-- An object's alerts on one line: its id and title, each alert a pill, the lead's fixes; unfolded, each in full. -->
{#snippet group(g: AlertGroup)}
  {#if g.alerts.length === 1}
    {@render row(g.alerts[0], false)}
  {:else}
    {@const o = objectOf(g)}
    {@const open = !!unfolded[g.key]}
    <div class="row grp">
      <button class="fold" onclick={() => (unfolded[g.key] = !open)} aria-expanded={open} title={open ? t('Fold') : t('Each alert in full')}><span class="chev" class:open>›</span></button>
      <span class="obj">{o.ref}</span>
      <span class="grow" title={g.alerts.map((a) => a.text).join('\n')}>{o.title}</span>
      <span class="pills">{#each g.alerts as a (a.text)}<span class="mini {a.severity}" title={a.text}>{a.short ?? a.text}</span>{/each}</span>
      {#if age(g.minutes)}<span class="age">{age(g.minutes)}</span>{/if}
      {@render fixButtons(g.alerts[0])}
      {@render putOffButton(g.alerts)}
      <button class="mark" onclick={() => g.alerts.forEach((a) => setSeen(a, true))} title={t('Mark them all as seen')} aria-label={t('Mark as seen')}><Icon name="check" /></button>
    </div>
    {#if open}<div class="sub">{#each g.alerts as a (a.text)}{@render row(a, false)}{/each}</div>{/if}
  {/if}
{/snippet}

<!-- The narrow block's alert (under NARROW px): its object and age, then its text on two lines at most, then its fixes on a line of their own. -->
{#snippet card(a: Alert, marked: boolean, away = false)}
  {@const o = objectOf({ key: '', band: 'later', alerts: [a], minutes: null })}
  <div class="card {a.severity}" class:seen={marked || away}>
    <div class="head">
      {#if o.ref}<span class="obj">{o.ref}</span><span class="grow" title={o.title}>{o.title}</span>{:else}<span class="pill {a.severity}">{a.kind === 'MR' ? mrWord() : a.kind === 'Claude' ? agentName() : t(a.kind)}</span><span class="grow"></span>{/if}
      {#if away}<span class="age">{awayUntil(a)}</span>{:else if age(alertMinutes(a, now))}<span class="age" title={t('Flagged for {age}', { age: age(alertMinutes(a, now)) })}>{age(alertMinutes(a, now))}</span>{/if}
      {#if away}
        <button class="mark" onclick={() => wake([a])} title={t('Bring it back now')} aria-label={t('Bring it back now')}><Icon name="undo" /></button>
      {:else}
        {#if !marked}{@render putOffButton([a])}{/if}
        <button class="mark" onclick={() => setSeen(a, !marked)} title={marked ? t('Mark as new') : t('Mark as seen, no action needed')} aria-label={marked ? t('Mark as new') : t('Mark as seen')}><Icon name={marked ? 'undo' : 'check'} /></button>
      {/if}
    </div>
    <p class="text" title={a.text}>{a.text}</p>
    <div class="acts">{@render fixButtons(a, true)}</div>
  </div>
{/snippet}

<!-- An object's alerts as one card: its id and title, each alert a pill (wrapping), the lead's fixes; unfolded, each as its card. -->
{#snippet groupCard(g: AlertGroup)}
  {#if g.alerts.length === 1}
    {@render card(g.alerts[0], false)}
  {:else}
    {@const o = objectOf(g)}
    {@const open = !!unfolded[g.key]}
    {@const sev = g.alerts.some((a) => a.severity === 'ko') ? 'ko' : g.alerts.some((a) => a.severity === 'warn') ? 'warn' : 'info'}
    <div class="card {sev}">
      <div class="head">
        <button class="fold" onclick={() => (unfolded[g.key] = !open)} aria-expanded={open} title={open ? t('Fold') : t('Each alert in full')}><span class="chev" class:open>›</span></button>
        <span class="obj">{o.ref}</span>
        <span class="grow" title={o.title}>{o.title}</span>
        {#if age(g.minutes)}<span class="age">{age(g.minutes)}</span>{/if}
        {@render putOffButton(g.alerts)}
        <button class="mark" onclick={() => g.alerts.forEach((a) => setSeen(a, true))} title={t('Mark them all as seen')} aria-label={t('Mark as seen')}><Icon name="check" /></button>
      </div>
      <div class="pills wrap">{#each g.alerts as a (a.text)}<span class="mini {a.severity}" title={a.text}>{a.short ?? a.text}</span>{/each}</div>
      <div class="acts">{@render fixButtons(g.alerts[0], true)}</div>
    </div>
    {#if open}<div class="sub">{#each g.alerts as a (a.text)}{@render card(a, false)}{/each}</div>{/if}
  {/if}
{/snippet}

{#snippet bandHead(band: Band, n: number)}
  <p class="band {band}"><span class="dot"></span>{BAND_LABEL[band]}<span class="n">{n}</span></p>
{/snippet}

<!-- The bands, then the alerts put off and those seen, folded. -->
{#snippet list()}
  {#each bands as b (b.band)}
    {@render bandHead(b.band, b.groups.reduce((n, g) => n + g.alerts.length, 0))}
    {#each b.groups as g (g.key)}{#if narrow}{@render groupCard(g)}{:else}{@render group(g)}{/if}{/each}
  {:else}
    <div class="empty"><Sprout />{seen.length || snoozed.length ? t('Nothing new.') : t('Nothing to report.')}</div>
  {/each}
  {#if snoozed.length}
    <button class="seen-toggle" onclick={() => (showSnoozed = !showSnoozed)} aria-expanded={showSnoozed}>
      <span class="chev" class:open={showSnoozed}>›</span> {tc('alerts', 'Put off')} · {snoozed.length}
    </button>
    {#if showSnoozed}{#each snoozed as a}{#if narrow}{@render card(a, false, true)}{:else}{@render row(a, false, true)}{/if}{/each}{/if}
  {/if}
  {#if seen.length}
    <button class="seen-toggle" onclick={() => (showSeen = !showSeen)} aria-expanded={showSeen}>
      <span class="chev" class:open={showSeen}>›</span> {t('Seen')} · {seen.length}
    </button>
    {#if showSeen}{#each seen as a}{#if narrow}{@render card(a, true)}{:else}{@render row(a, true)}{/if}{/each}{/if}
  {/if}
{/snippet}

{#if phone}
  <div class="phone">
    {#each bands as b (b.band)}
      <p class="pgrp {b.band}">{BAND_LABEL[b.band]} · {b.groups.length}</p>
      {#each b.groups as g (g.key)}{@render prow(g.alerts[0], false)}{/each}
    {/each}
    {#if !fresh.length}<div class="empty pempty"><Sprout />{seen.length || snoozed.length ? t('Nothing new.') : t('Nothing to report.')}</div>{/if}
    {#if snoozed.length}
      <button class="pfold" onclick={() => (showSnoozed = !showSnoozed)} aria-expanded={showSnoozed}><span>{tc('alerts', 'Put off')} · {snoozed.length}</span><span>{showSnoozed ? '▾' : '▸'}</span></button>
      {#if showSnoozed}{#each snoozed as a}{@render prow(a, false, true)}{/each}{/if}
    {/if}
    {#if seen.length}
      <button class="pfold" onclick={() => (showSeen = !showSeen)} aria-expanded={showSeen}><span>{t('Seen')} · {seen.length}</span><span>{showSeen ? '▾' : '▸'}</span></button>
      {#if showSeen}{#each seen as a}{@render prow(a, true)}{/each}{/if}
    {/if}
  </div>
{:else if popup}
  <div class="popup">{@render list()}</div>
{:else}
<section class="panel" style="--src: var(--src-alert)" class:hot={critical} class:collapsed={isCollapsed('todo')}>
  <h2><span class="title"><Logo name="alert" />{t('Alerts')}</span> <small class:urgent={isCollapsed('todo') && urgent}>{isCollapsed('todo') ? tn(fresh.length, '{n} item', '{n} items') : t('by urgency')}</small><BlockControls id="todo" /></h2>
  {#if isShort('todo')}
    <div class="short-body">
      {#each pressing.slice(0, SHORT) as g (g.key)}{@render group(g)}{:else}<span class="more">{t('Nothing urgent.')}</span>{/each}
      {#if bands.reduce((n, b) => n + b.groups.length, 0) > Math.min(SHORT, pressing.length)}<span class="more">{t('+{n} more', { n: bands.reduce((n, b) => n + b.groups.length, 0) - Math.min(SHORT, pressing.length) })}</span>{/if}
    </div>
  {:else}
  <div class="scroll" bind:clientWidth={width}>{@render list()}</div>
  {/if}
</section>
{/if}

{#if putting}
  {@const target = putting.el}
  <div class="putoff" role="menu" {@attach anchored(() => target, { align: 'end' })} {@attach dismissable(() => (putting = null), { trigger: () => target })}>
    {#each choices as c (c.key)}
      <button role="menuitem" onclick={() => putOff(c)}><span>{c.label}</span>{#if c.detail}<small>{c.detail}</small>{/if}</button>
    {/each}
    <hr />
    <button role="menuitem" class="muted" onclick={() => { putting = null; openSettings('alerts') }}><span>{t('Band of this kind of alert…')}</span></button>
  </div>
{/if}

<style>
  /* The phone's view. */
  /* Edge to edge: the remote's page pads its views. */
  .phone { width: calc(100% + 20px); max-width: none; margin: 0 -10px; padding-bottom: 8px; }
  .pgrp { margin: 0; padding: 14px 14px 6px; color: var(--text-muted); font-size: 11px; letter-spacing: 0.05em; text-transform: uppercase; }
  .pgrp.ko { color: var(--ko); } .pgrp.warn { color: var(--warn); }
  .pal { display: flex; gap: 10px; padding: 11px 14px; border-bottom: 1px solid var(--border); --sev: var(--info); }
  .pal.ko { --sev: var(--ko); } .pal.warn { --sev: var(--warn); }
  .sev { flex: none; width: 3px; border-radius: 2px; background: var(--sev); }
  .pbody { flex: 1; min-width: 0; }
  .kind { color: var(--text-muted); font-size: 11px; }
  .ptext { margin: 2px 0 0; font-size: 14px; line-height: 1.35; overflow-wrap: anywhere; }
  .pacts { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
  .pacts :global(.qa button), .pbtn { display: inline-flex; align-items: center; gap: 5px; padding: 6px 12px; border: 1px solid var(--border); border-radius: 16px; background: var(--surface); color: var(--text); font: inherit; font-size: 13px; text-decoration: none; }
  .pbtn.claude { border-color: color-mix(in srgb, var(--src-claude) 60%, var(--border)); }
  .pbtn.done { border-color: transparent; background: var(--ok-bg); color: var(--ok); }
  .pbtn.err { border-color: var(--ko); color: var(--ko); }
  .pbtn:disabled { opacity: 0.7; }
  .pmark { flex: none; display: grid; place-items: center; width: 34px; height: 34px; border: 1px solid var(--border); border-radius: 50%; background: none; color: var(--text-muted); }
  .pfold { display: flex; justify-content: space-between; width: calc(100% - 28px); margin: 10px 14px; padding: 10px 12px; border: 0; border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font: inherit; font-size: 13px; }
  .pempty { padding: 24px 14px; justify-content: center; color: var(--text-muted); }
  /* Empty: Prism's sprout (Sprout) to the left of the message. */
  .empty { display: flex; align-items: center; gap: 8px; }
  .hot { border-color: var(--ko); }
  .short-body .row { padding: 0; }
  .urgent { color: var(--warn) !important; font-weight: 500; }
  a { font-size: var(--fs-body); }
  /* Fixes: small buttons before "Open". */
  .fix { display: inline-flex; flex-shrink: 0; align-items: center; gap: 4px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; cursor: pointer; text-decoration: none; }
  .fix:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
  .fix.claude { border-color: color-mix(in srgb, var(--src-claude) 60%, var(--border)); }
  .fix.claude:hover { background: color-mix(in srgb, var(--src-claude) 12%, var(--surface)); color: var(--text); }
  .fix.on { gap: 5px; background: color-mix(in srgb, var(--src-claude) 10%, var(--surface)); }
  .pulse { position: relative; width: 6px; height: 6px; border-radius: 50%; background: var(--src-claude); }
  /* A ring scaled and faded (the GPU's), not a shadow animated (a repaint a frame). */
  .pulse.live::after { content: ''; position: absolute; inset: -3px; border-radius: 50%; background: color-mix(in srgb, var(--src-claude) 30%, transparent); opacity: 0; transform: scale(0.5); animation: pulse 1.6s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 1; transform: scale(1); } }
  @media (prefers-reduced-motion: reduce) { .pulse.live::after { animation: none; } }
  .fix.busy { cursor: progress; opacity: 0.7; }
  .fix.done { border-color: transparent; background: var(--ok-bg); color: var(--ok); cursor: default; }
  .fix.err { border-color: var(--ko); color: var(--ko); }
  /* Discreet "seen" button, revealed on row hover. */
  .mark { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 20px; height: 20px; border: none; border-radius: var(--r5); background: none; padding: 0; color: var(--text-muted); font: inherit; font-size: var(--fs-body); cursor: pointer; opacity: 0; }
  .row:hover .mark, .mark:focus-visible, .seen .mark { opacity: 1; }
  .mark:hover { background: var(--surface-muted); color: var(--ok); }
  .seen { opacity: 0.55; }
  .seen-toggle { display: flex; align-items: center; gap: 6px; width: 100%; padding: 4px 12px; border: none; border-top: 1px solid var(--border); background: var(--surface-muted); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .chev { display: inline-block; transition: transform 0.15s; }
  .chev.open { transform: rotate(90deg); }
  /* Bands: Now, Today, When you can, each under its line. */
  .band { display: flex; align-items: center; gap: 6px; margin: 0; padding: 6px 12px 2px; color: var(--text-muted); font-size: var(--fs-tag); letter-spacing: 0.05em; text-transform: uppercase; }
  .band .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--info); }
  .band.now .dot { background: var(--ko); }
  .band.today .dot { background: var(--warn); }
  .band .n { margin-left: 2px; opacity: 0.8; }
  .pgrp.now { color: var(--ko); } .pgrp.today { color: var(--warn); }
  .age { flex-shrink: 0; color: var(--text-muted); font-size: var(--fs-tag); font-variant-numeric: tabular-nums; white-space: nowrap; }
  /* A group: its object, its alerts as pills, the lead's fixes. */
  .fold { display: inline-flex; flex-shrink: 0; width: 14px; padding: 0; border: none; background: none; color: var(--text-muted); font: inherit; cursor: pointer; }
  .obj { flex-shrink: 0; font-family: var(--mono); font-size: var(--fs-meta); color: var(--accent); }
  .pills { display: inline-flex; flex-shrink: 1; gap: 3px; min-width: 0; overflow: hidden; }
  .mini { flex-shrink: 0; max-width: 140px; overflow: hidden; text-overflow: ellipsis; padding: 0 6px; border: 1px solid var(--border); border-radius: 9px; color: var(--text-muted); font-size: var(--fs-tag); line-height: 16px; white-space: nowrap; }
  .mini.ko { border-color: color-mix(in srgb, var(--ko) 55%, var(--border)); color: var(--ko); }
  .mini.warn { border-color: color-mix(in srgb, var(--warn) 55%, var(--border)); color: var(--warn); }
  .sub { padding-left: 18px; }
  .mark.on { opacity: 1; color: var(--accent); }
  .pmark.on { border-color: var(--accent); color: var(--accent); }
  .pbtn small { color: var(--text-muted); }
  .putoff { position: fixed; z-index: 50; min-width: 210px; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); font-size: var(--fs-body); }
  .putoff button { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 8px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .putoff button span { flex: 1; }
  .putoff button small { color: var(--text-muted); font-size: var(--fs-meta); }
  .putoff button:hover { background: var(--surface-muted); }
  .putoff button.muted { color: var(--text-muted); }
  .putoff hr { margin: 4px 6px; border: none; border-top: 1px solid var(--border); }
  /* The narrow block's cards: a bar of their severity, their object, their text, their fixes. */
  .card { position: relative; padding: calc(6px * var(--space)) 12px calc(7px * var(--space)) 15px; border-top: 1px solid var(--border); font-size: var(--fs-body); --sev: var(--info); }
  .card:first-of-type { border-top: none; }
  .card.ko { --sev: var(--ko); } .card.warn { --sev: var(--warn); }
  .card::before { content: ''; position: absolute; left: 6px; top: 8px; bottom: 8px; width: 3px; border-radius: 2px; background: var(--sev); }
  .card .head { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .card .head .grow { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .card:hover .mark { opacity: 1; }
  .card .text { margin: 2px 0 0; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; }
  .card .pills.wrap { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px; overflow: visible; }
  .card .pills.wrap .mini { max-width: 100%; }
  .card .acts { display: flex; flex-wrap: wrap; align-items: center; gap: 5px; margin-top: 5px; }
  .card .acts:empty { display: none; }
  .fix.lead { border-color: color-mix(in srgb, var(--accent) 55%, var(--border)); background: color-mix(in srgb, var(--accent) 14%, var(--surface)); }
  .popup { display: flex; flex-direction: column; max-height: min(480px, 70vh); overflow: auto; }
</style>
