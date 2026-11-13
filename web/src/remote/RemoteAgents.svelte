<script lang="ts">
  import OverlapChip from '../components/OverlapChip.svelte'
  import AsideChip from '../components/AsideChip.svelte'
  import ProposalChip from '../components/ProposalChip.svelte'
  import { overlapsOfAgent } from '../overlaps'
  import QuickAnswers from '../components/QuickAnswers.svelte'
  import RoutineReview from '../components/RoutineReview.svelte'
  import { t, tn, tc } from '../i18n.svelte'
  import { allowAgent, interruptAgent, modelName, openAgents, openLauncher, resumeAgent, startPrepared } from '../agents.svelte'
  import { cliOf } from '../agent-cli'
  import { isWorking, waitsForYou } from '../rules'
  import { isArchived, setArchived } from '../agent-archive.svelte'
  import { canMarkRead, markRead } from '../session-read'
  import Usage from '../components/Usage.svelte'
  import Icon from '../components/Icon.svelte'
  import { hasWindows, peak, planSections, sectionKey, sectionName, usageLevel } from '../usage'
  import { mrKey } from '../mr-ref'
  import { ago } from '../time'
  import { toast } from '../toast.svelte'
  import { agentPorts } from '../agent-ports'
  import { openDevServer } from '../dev-preview.svelte'
  import type { Agent, Snapshot } from '../types'
  import { agentRepo } from '../repositories'
  import { agentEntry, type Group } from '../agents-list'
  import { projectsOf, type Project } from '../projects'
  import { implement, newAgentIn, nextOf, reopenLeft } from '../project-actions'
  import { namedByQuickAction, quickActionsHere, runQuickAction } from '../quick-actions'
  import { pref, setPref } from '../layout.svelte'
  import { openQuickActionDraft } from '../quick-action-draft.svelte'
  import { socketState } from './socket.svelte'

  /**
   * Prism's agents on a phone (not Claude Desktop's sessions): what waits for you first — the
   * question it asks, or the command it wants to run — with its quick answers, Allow and the keys
   * at hand; then those working, then the others; the stopped ones folded, Resume picking their
   * conversation up again. A tap opens its terminal; each of its open ports (found by the Dev servers block)
   * opens in the remote's preview.
   * At the top, the plan's usage (the status bar's on the computer: a tap shows its windows and resets,
   * a section per CLI when agents run on several), then a new agent: New opens the sheet (`PhoneLauncher`).
   * By project (a shared pref): a card per repository instead, its agents in it (those waiting for
   * you first), where it was left with Reopen, its next ticket, a + for a new agent in it; its +, its
   * ticket's ▶ and its quick actions open the same sheet, prefilled.
   */
  let { snap, acts }: { snap: Snapshot; acts: boolean } = $props()
  /** Critical actions on (the passkey each time): a quick action is saved to config.toml. */
  const critical = $derived(socketState.hello?.classes.includes('critical') ?? false)

  /** By status (the default) or by project, shared with the computer's phone prefs. */
  let by = $state<'status' | 'project'>(pref<'status' | 'project'>('agents.phoneBy', 'status'))
  const setBy = (b: 'status' | 'project') => setPref('agents.phoneBy', (by = b))

  const all = $derived([...(snap.agents ?? [])].sort((a, b) => (b.last_activity ?? b.created_at).localeCompare(a.last_activity ?? a.created_at)))
  /** Archived ones (`agent-archive.svelte.ts`) fold apart, until they wait for you. */
  const agents = $derived(all.filter((a) => !isArchived(a)))
  const archived = $derived(all.filter(isArchived))
  let showArchived = $state(false)
  const live = $derived(agents.filter((a) => a.state === 'running'))
  /** Waiting for you: a question, a permission, a plan to approve. */
  const waiting = $derived(live.filter((a) => a.status === 'needs_you' || a.status === 'waiting_tool' || a.status === 'review_ready'))
  const working = $derived(live.filter((a) => isWorking(a.status)))
  const idle = $derived(live.filter((a) => !waiting.includes(a) && !working.includes(a)))
  const stopped = $derived(agents.filter((a) => a.state !== 'running'))
  let showStopped = $state(false)
  let showUsage = $state(false)
  const sections = $derived(planSections(snap))
  const lead = $derived(sections.find(hasWindows) ?? null)
  const rest = $derived(sections.filter((p) => p !== lead))
  const restLevel = $derived(usageLevel({ utilization: Math.max(0, ...rest.map(peak)), resets_at: null }))
  const five = $derived(lead?.usage?.five_hour ?? null)
  const seven = $derived(lead?.usage?.seven_day ?? null)

  const STATE: Record<string, [string, string]> = { needs_you: [t('Asks'), 'asks'], waiting_tool: [t('Permission'), 'tool'], running: [t('Working'), 'run'], background: [t('In the background'), 'run'], review_ready: [t('Plan ready'), 'asks'], completed: [tc('infra', 'Done'), ''], idle: [t('Idle'), ''] }
  /** Projects: the agents of each repository, those with an agent waiting for you, then working, first. */
  const repos = $derived((snap.repos ?? []).map((r) => r.name))
  const urgency = (g: Group) => Math.min(...g.items.map((e) => (waiting.some((a) => a.id === e.id) ? 0 : working.some((a) => a.id === e.id) ? 1 : 2)))
  const projects = $derived.by(() => {
    const groups: Group[] = repos.map((name) => ({ name, items: agents.filter((a) => agentRepo(a, snap) === name).map(agentEntry) })).filter((g) => g.items.length)
    groups.sort((x, y) => urgency(x) - urgency(y))
    return projectsOf({ groups, repos, left: snap.left ?? [], next: nextOf, now: Date.now(), kept: namedByQuickAction })
  })
  /** Agents outside every repository. */
  const elsewhere = $derived(agents.filter((a) => !agentRepo(a, snap)))
  let showResting = $state(false)

  const label = (a: Agent) => (a.state === 'running' && a.gate ? [t('Waits for your trust'), 'trust'] : a.state === 'running' && a.read ? [t('Stopped'), ''] : a.state === 'running' ? (STATE[a.status] ?? ['', '']) : [a.state === 'prepared' ? t('Prepared') : t('Stopped'), ''])
  const ctx = (a: Agent) => (a.context?.size ? `ctx ${Math.round((100 * a.context.used) / a.context.size)} %` : '')

  let busy = $state<string | null>(null)
  async function act(id: string, run: () => Promise<unknown>) {
    if (busy) return
    busy = id
    try {
      await run()
    } catch (e) {
      toast(String(e), 6000)
    } finally {
      busy = null
    }
  }

  /**
   * A new agent: the phone's sheet (`PhoneLauncher`), empty or with what was typed here as its
   * prompt; it picks the repository (the last one launched in) and the place.
   */
  let prompt = $state('')
  function start() {
    const text = prompt.trim()
    openLauncher({ title: '', prompt: text })
    prompt = ''
  }
</script>

{#snippet archiving(a: Agent)}
  {@const hidden = isArchived(a)}
  <button class="arch" onclick={() => setArchived(a, !hidden)} aria-label={hidden ? t('Unarchive {title}', { title: a.title }) : t('Archive {title}', { title: a.title })} title={hidden ? t('Back in the list') : t('Out of the list until it waits for you')}><Icon name={hidden ? 'show' : 'hide'} size="14px" /></button>
{/snippet}

{#snippet card(a: Agent)}
  {@const [state, tone] = label(a)}
  {@const ports = agentPorts(a.id)}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions : its terminal opens from the title, a button -->
  <div class="ag" class:off={a.state !== 'running'} onclick={(e) => !(e.target as HTMLElement).closest('button') && openAgents(a.id)}>
    <div class="h"><span class="dot {tone}"></span><span class="st {tone}">{state}</span><span class="grow"></span><span>{cliOf(a.provider).name.toLowerCase()}{#if a.model}{` · ${modelName(a.model)}`}{/if} · {ago(a.last_activity ?? a.created_at)}</span>{@render archiving(a)}</div>
    <button class="title" onclick={() => openAgents(a.id)}>{a.title}</button>
    {#if a.state === 'running' && a.needs_action && waitsForYou(a.status)}
      <p class="q" class:tool={a.status === 'waiting_tool'}>{a.needs_action}</p>
    {/if}
    <div class="meta">
      {#if a.ticket}<span class="tk">{a.ticket}</span>{/if}
      {#if a.mr}<span>{mrKey(a.mr)}</span>{:else if a.branch}<span class="br">{a.branch}</span>{/if}
      {#if ctx(a)}<span>{ctx(a)}</span>{/if}
      <ProposalChip agent={a} />
      <AsideChip agent={a} />
      {#if overlapsOfAgent(snap, a).length}<OverlapChip seen={overlapsOfAgent(snap, a)} />{/if}
    </div>
    {#if ports.length}
      <div class="ports">
        {#each ports as { port, server } (port)}<button class="port" onclick={() => openDevServer(port)} aria-label={t('Open :{port} ({command})', { port, command: server.command })}><Icon name="external" size="12px" />:{port}<small>{server.docker ? t('container') : server.program}</small></button>{/each}
      </div>
    {/if}
    {#if acts && a.state === 'running'}
      {#if a.status === 'waiting_tool'}
        <div class="chips">
          <button class="btn allow" disabled={busy === a.id} onclick={() => act(a.id, () => allowAgent(a.id))}>✓ {t('Allow once')}</button>
          <button class="btn" disabled={busy === a.id} onclick={() => act(a.id, () => interruptAgent(a.id))} title={t('Refuse and interrupt its turn (Esc): the agent stays')}><Icon name="pause" /> {t('Interrupt')}</button>
        </div>
      {:else if a.status !== 'running'}
        <div class="chips"><QuickAnswers agent={a} />{#if canMarkRead(a)}<button class="btn" disabled={busy === a.id} onclick={() => act(a.id, () => markRead(a.id))} title={t('Seen: it waits for you no more until its next turn')}><Icon name="check" /> {t('Mark as read')}</button>{/if}</div>
      {/if}
    {:else if acts && a.state === 'prepared'}
      <!-- Its Then steps: started by the computer with it (remote.rs, agent_start). -->
      <div class="chips"><button class="btn" disabled={busy === a.id} onclick={() => act(a.id, () => startPrepared(a.id))} title={t('Start it now, as prepared (its worktree is made then)')}><Icon name="play" /> {t('Start')}</button></div>
    {:else if acts}
      <div class="chips"><button class="btn" disabled={busy === a.id} onclick={() => act(a.id, () => resumeAgent(a.id))} title={t('{command}, same folder', { command: cliOf(a.provider).resume })}>↻ {t('Resume')}</button></div>
    {/if}
  </div>
{/snippet}

{#snippet project(p: Project)}
  <section class="proj">
    <header><span class="dot {p.group ? (urgency(p.group) === 0 ? 'asks' : urgency(p.group) === 1 ? 'run' : '') : ''}"></span><b>{p.name}</b><span class="grow"></span>{#if acts}<button class="plus" onclick={() => newAgentIn(p.name)} aria-label={t('New agent in {repo}', { repo: p.name })}>+</button>{/if}</header>
    {#if p.group}{#each p.group.items as e (e.id)}{#if e.kind === 'agent'}{@render card(e.a)}{/if}{/each}{/if}
    {#if p.left}
      <div class="left">
        <p>{t('Left {ago} ago', { ago: ago(p.left.closed_at) })}{#if p.left.branch}{' '}· <code>{p.left.branch}</code>{/if} · {p.left.title}</p>
        {#if p.left.said}<q>{p.left.said}</q>{/if}
        {#if acts}<div class="chips">{#if p.left.resumable}<button class="btn" disabled={busy === p.name} onclick={() => act(p.name, () => reopenLeft(p.name))}>↻ {t('Reopen')}</button>{/if}{#if critical}<button class="btn" onclick={() => void openQuickActionDraft({ left: p.name })} title={t('A quick action from this session: its prompt and settings, to read over')}><Icon name="quick" /> {t('Quick action…')}</button>{/if}</div>{/if}
      </div>
    {/if}
    {#if acts}
      {@const quick = quickActionsHere(p.name)}
      {#if quick.length}<div class="chips quick">{#each quick as a, i (i)}<button class="btn" onclick={() => runQuickAction(a, p.name, true)} title={a.prompt}><Icon name="quick" /> {a.name}</button>{/each}</div>{/if}
    {/if}
    {#if p.next}
      <div class="next"><span class="muted">{tc('project', 'Next')}</span><code>{p.next.id}</code><span class="nm">{p.next.name}</span>{#if acts}<button class="btn" onclick={() => implement(p.next!)} aria-label={t('Implement {ticket}', { ticket: p.next.id })}>▶</button>{/if}</div>
    {/if}
  </section>
{/snippet}

<div class="agents">
  <!-- The routines' runs waiting for you ("To review"), at the head, as on the computer. -->
  <RoutineReview next={false} />
  {#if lead}
    <div class="plan" class:stale={!!lead.error} style:--cli-fill={cliOf(lead.provider).color}>
      <button class="sum" onclick={() => (showUsage = !showUsage)} aria-expanded={showUsage}>
        <Icon name={cliOf(lead.provider).icon} />
        <span class="t">{rest.length ? t('Usage') : t('{cli} usage', { cli: sectionName(lead, cliOf(lead.provider).name) })}</span>
        {#if five}<span class="w {usageLevel(five)}"><small>5h</small><span class="bar"><i style:width="{Math.min(100, five.utilization)}%"></i></span><b>{Math.round(five.utilization)}%</b></span>{/if}
        {#if seven}<span class="w {usageLevel(seven)}"><small>{t('wk')}</small><b>{Math.round(seven.utilization)}%</b></span>{/if}
        {#if rest.length}<span class="more {restLevel}">+{#each rest as p (sectionKey(p))}<span style:color={cliOf(p.provider).color}><Icon name={cliOf(p.provider).icon} /></span>{/each}</span>{/if}
        <Icon name={showUsage ? 'chevronDown' : 'chevronRight'} />
      </button>
      {#if showUsage}
        {#if rest.length}
          {#each sections as p (sectionKey(p))}
            <section class="cli" style:--cli-fill={cliOf(p.provider).color}>
              <header><Icon name={cliOf(p.provider).icon} /><b>{sectionName(p, cliOf(p.provider).name)}</b>{#if p.configured}<span class="def">{t('default')}</span>{/if}<span class="n">{p.agents ? tn(p.agents, '{n} agent', '{n} agents') : t('no agent')}</span></header>
              {#if !cliOf(p.provider).plan}<p class="none">{t('No plan: it runs on your own providers’ keys.')}</p>{:else}<Usage usage={p.usage} error={p.error} />{/if}
            </section>
          {/each}
        {:else}
          <Usage usage={lead.usage} error={lead.error} />
        {/if}
      {/if}
    </div>
  {/if}
  <div class="by" role="tablist">
    <button role="tab" class:on={by === 'status'} aria-selected={by === 'status'} onclick={() => setBy('status')}>{t('By status')}</button>
    <button role="tab" class:on={by === 'project'} aria-selected={by === 'project'} onclick={() => setBy('project')}>{t('By project')}</button>
  </div>
  {#if acts && by === 'status'}
    <form class="new" onsubmit={(e) => { e.preventDefault(); start() }}>
      <textarea bind:value={prompt} rows="1" placeholder={t('New agent: what should it do?')} enterkeyhint="enter"></textarea>
      <button type="submit">{prompt.trim() ? t('Next') : t('New')}</button>
    </form>
  {/if}
  {#if by === 'project'}
    {#each projects.active as p (p.name)}{@render project(p)}{/each}
    {#if elsewhere.length}<p class="grp">{t('Outside the repositories')} · {elsewhere.length}</p>{#each elsewhere as a (a.id)}{@render card(a)}{/each}{/if}
    {#if projects.resting.length}
      <button class="fold" onclick={() => (showResting = !showResting)} aria-expanded={showResting}><span>{tn(projects.resting.length, '{n} project resting', '{n} projects resting')}</span><span>{showResting ? '▾' : '▸'}</span></button>
      {#if showResting}{#each projects.resting as p (p.name)}{@render project(p)}{/each}{/if}
    {/if}
  {:else}
  {#if waiting.length}<p class="grp warn">{t('Waiting for you')} · {waiting.length}</p>{#each waiting as a (a.id)}{@render card(a)}{/each}{/if}
  {#if working.length}<p class="grp">{t('Working')} · {working.length}</p>{#each working as a (a.id)}{@render card(a)}{/each}{/if}
  {#if idle.length}<p class="grp">{t('Idle')} · {idle.length}</p>{#each idle as a (a.id)}{@render card(a)}{/each}{/if}
  {#if !live.length}<p class="empty">{t('No agent running.')}</p>{/if}
  {#if stopped.length}
    <button class="fold" onclick={() => (showStopped = !showStopped)} aria-expanded={showStopped}><span>{t('Stopped')} · {stopped.length}</span><span>{showStopped ? '▾' : '▸'}</span></button>
    {#if showStopped}{#each stopped as a (a.id)}{@render card(a)}{/each}{/if}
  {/if}
  {/if}
  {#if archived.length}
    <button class="fold" onclick={() => (showArchived = !showArchived)} aria-expanded={showArchived}><span>{t('Archived')} · {archived.length}</span><span>{showArchived ? '▾' : '▸'}</span></button>
    {#if showArchived}{#each archived as a (a.id)}{@render card(a)}{/each}{/if}
  {/if}
</div>

<style>
  .arch { display: inline-flex; margin: -6px -6px -6px 2px; padding: 6px; border: none; background: none; color: var(--text-muted); }
  /* Edge to edge: the remote's page pads its views. */
  .agents { width: calc(100% + 20px); max-width: none; margin: 0 -10px; padding-bottom: 8px; }
  .plan { margin: 0 10px 8px; border: 1px solid var(--border); border-radius: 12px; overflow: hidden; background: var(--surface); }
  .plan.stale { opacity: 0.75; }
  .sum { width: 100%; display: flex; align-items: center; gap: 10px; padding: 11px 14px; border: none; background: none; color: var(--text); font: inherit; font-size: 14px; text-align: left; }
  .sum > :global(svg:first-child) { color: var(--cli-fill); }
  .sum .t { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sum > :global(svg:last-child) { color: var(--text-muted); }
  .w { display: inline-flex; align-items: center; gap: 5px; --fill: var(--cli-fill); }
  .w.warn { --fill: var(--warn); }
  .w.ko { --fill: var(--ko); }
  .w small { color: var(--text-muted); font-size: 12px; }
  .w b { font: 600 13px var(--mono); color: var(--fill); }
  .w:not(.warn):not(.ko) b { color: var(--text); }
  .bar { width: 44px; height: 6px; border-radius: 3px; background: var(--border); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--fill); }
  /* The other CLIs in use, as on the computer: tinted once one of them nears a limit. */
  .more { display: inline-flex; align-items: center; gap: 3px; padding: 1px 7px; border-radius: 10px; background: var(--surface-muted); font-size: 12px; color: var(--text-muted); }
  .more.warn { background: color-mix(in srgb, var(--warn) 16%, transparent); color: var(--warn); }
  .more.ko { background: color-mix(in srgb, var(--ko) 16%, transparent); color: var(--ko); }
  .more :global(svg) { width: 12px; height: 12px; }
  /* A section per CLI, in its colour. */
  .cli { border-top: 1px solid var(--border); background: color-mix(in srgb, var(--cli-fill) 6%, transparent); }
  .cli :global(.usage) { border-top: none; background: none; padding-top: 4px; }
  .cli header { display: flex; align-items: center; gap: 8px; padding: 10px 14px 0; font-size: 14px; }
  .cli header > :global(svg) { color: var(--cli-fill); }
  .cli header b { color: var(--text); }
  .cli .def { font-size: 12px; color: var(--text-muted); }
  .cli .n { margin-left: auto; font-size: 12px; color: var(--text-muted); }
  .cli .none { margin: 0; padding: 6px 14px 12px; font-size: 13px; color: var(--text-muted); }
  /* The status bar's detail, made for a thumb: the window's name, its reset and % on a line, its bar under them. */
  .plan :global(.usage) { gap: 12px; padding: 12px 14px 14px; }
  .plan :global(.u:not(.off)) { grid-template-columns: minmax(0, 1fr) auto auto; row-gap: 6px; font-size: 13px; }
  .plan :global(.u .name) { order: 0; }
  .plan :global(.u .reset) { order: 1; font-size: 12px; }
  .plan :global(.u .pct) { order: 2; min-width: 40px; }
  .plan :global(.u > .bar), .plan :global(.u > .days) { order: 3; grid-column: 1 / -1; }
  .plan :global(.u .bar7) { height: 10px; }
  .plan :global(.u .labels span) { font-size: 10.5px; }
  .new { display: flex; gap: 8px; align-items: flex-end; padding: 4px 14px 6px; }
  .new textarea { flex: 1; field-sizing: content; min-height: 38px; max-height: 140px; resize: none; padding: 9px 12px; border: 1px solid var(--border); border-radius: 18px; background: var(--surface); color: var(--text); font: inherit; font-size: 14px; }
  .new button { height: 38px; padding: 0 16px; border: 1px solid var(--accent); border-radius: 19px; background: none; color: var(--text); font: inherit; font-size: 14px; }
  .new button:disabled { opacity: 0.45; }
  .grp { margin: 0; padding: 14px 14px 6px; color: var(--text-muted); font-size: 11px; letter-spacing: 0.05em; text-transform: uppercase; }
  .grp.warn { color: var(--warn); }
  .ag { padding: 12px 14px; border-bottom: 1px solid var(--border); }
  .ag.off { opacity: 0.6; }
  .h { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 12px; }
  .dot { flex: none; width: 9px; height: 9px; border-radius: 50%; background: var(--text-muted); }
  .dot.asks { background: var(--warn); } .dot.trust { background: var(--violet); }
  .st.trust { color: var(--violet); } .dot.tool { background: var(--ko); } .dot.run { background: var(--info); }
  .st { font-weight: 600; }
  .st.asks { color: var(--warn); } .st.tool { color: var(--ko); } .st.run { color: var(--info); }
  .grow { flex: 1; }
  .title { display: block; width: 100%; margin: 4px 0; padding: 0; border: 0; background: none; color: var(--text); font: inherit; font-size: 15px; font-weight: 550; line-height: 1.3; text-align: left; }
  .q { margin: 6px 0 8px; padding: 8px 10px; border-left: 3px solid var(--warn); border-radius: 4px; background: var(--surface); font-size: 13px; line-height: 1.4; white-space: pre-wrap; overflow-wrap: anywhere; }
  .q.tool { border-color: var(--ko); font-family: var(--mono); font-size: 12px; }
  .meta { display: flex; flex-wrap: wrap; gap: 4px 10px; color: var(--text-muted); font-size: 12px; }
  .meta .tk { color: var(--info); font-family: var(--mono); }
  .meta .br { overflow: hidden; max-width: 100%; font-family: var(--mono); text-overflow: ellipsis; white-space: nowrap; }
  /* Its open ports: a chip per port, what holds it beside, scrolled sideways rather than wrapped. */
  .ports { display: flex; gap: 6px; margin-top: 8px; overflow-x: auto; scrollbar-width: none; }
  .port { flex: none; display: inline-flex; align-items: center; gap: 5px; min-height: 30px; padding: 0 10px; border: 1px solid color-mix(in srgb, var(--ok) 40%, transparent); border-radius: 15px; background: color-mix(in srgb, var(--ok) 10%, transparent); color: var(--ok); font: 600 13px var(--mono); }
  .port small { max-width: 110px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); font: 400 11.5px var(--sans); }
  .chips { display: flex; gap: 6px; margin-top: 8px; overflow-x: auto; scrollbar-width: none; }
  .chips :global(.qa) { flex-wrap: nowrap; }
  .chips :global(.qa button), .btn { flex: none; padding: 6px 12px; border: 1px solid var(--border); border-radius: 16px; background: var(--surface); color: var(--text); font: inherit; font-size: 13px; }
  .btn.allow { border-color: var(--ok); color: var(--ok); }
  .btn:disabled { opacity: 0.5; }
  .btn { display: inline-flex; align-items: center; gap: 5px; }
  .empty { margin: 24px 0; color: var(--text-muted); font-size: 13px; text-align: center; }
  /* By status or by project. */
  .by { display: flex; gap: 6px; padding: 2px 14px 8px; }
  .by button { padding: 5px 12px; border: 1px solid var(--border); border-radius: 15px; background: none; color: var(--text-muted); font: inherit; font-size: 13px; }
  .by button.on { border-color: var(--text); background: var(--surface); color: var(--text); font-weight: 600; }
  /* A project: its header with +, its agents' cards, where it was left, its next ticket. */
  .proj { margin: 8px 10px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); overflow: hidden; }
  .proj header { display: flex; align-items: center; gap: 8px; padding: 9px 10px 8px 12px; }
  .proj header b { font-size: 15px; }
  .proj .plus { width: 32px; height: 32px; border: 1px solid var(--border); border-radius: 9px; background: none; color: var(--src-claude); font: 400 20px/1 var(--sans); }
  .proj .ag { border-top: 1px solid var(--border); border-bottom: none; }
  .proj .left { margin: 0 12px 10px; padding: 2px 0 2px 9px; border-left: 2px solid var(--border); font-size: 12.5px; color: var(--text-muted); }
  .proj .left p { margin: 0; }
  .proj .left code { font-family: var(--mono); }
  .proj .left q { display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; margin-top: 4px; color: var(--text); quotes: '« ' ' »'; }
  .proj .quick { margin: 0 12px 10px; }
  .proj .next { display: flex; align-items: center; gap: 7px; padding: 8px 12px 10px; border-top: 1px solid var(--border); font-size: 13px; }
  .proj .next code { color: var(--info); font-family: var(--mono); }
  .proj .next .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .proj .next .btn { padding: 4px 11px; color: var(--src-claude); }
  .muted { color: var(--text-muted); }
  .fold { display: flex; justify-content: space-between; width: calc(100% - 28px); margin: 10px 14px; padding: 10px 12px; border: 0; border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font: inherit; font-size: 13px; }
</style>
