<script lang="ts">
  import { baseName } from '../platform'
  import type { Aside } from '../types'
  import { live } from '../live.svelte'
  import { t, tn } from '../i18n.svelte'
  import { hasCore } from '../bridge'
  import { offers } from '../tracker'
  import { openAgents } from '../agents.svelte'
  import { openTicket } from '../ticket-view.svelte'
  import { toast } from '../toast.svelte'
  import { agentBranch, asideDismiss, asideDone, asideForget, asideLater, asideReopen, asideSession, asideThen, asideTicket, isOpen, launchAside, mainAction, originOf, sessionOf, STATE_LABEL } from '../asides.svelte'
  import { ago } from '../time'
  import Icon from './Icon.svelte'
  import KindBadge from './KindBadge.svelte'

  /**
   * A task an agent flagged in passing: in its Chat where it saw it (`chat`), in the « In passing »
   * list, the block and the phone. Its title, what and why, its files, who saw it; Launch (the
   * launcher, prefilled), from the base or its agent's branch, In this session (its agent does it
   * after its turn, resumed first if it exited), After this agent, Ticket, Later,
   * its prompt unfolded, Dismiss with a reason told to the agent. Never blocks: not a question.
   */
  let { a, chat = false, withdrawn = false }: { a: Aside; chat?: boolean; withdrawn?: boolean } = $props()

  const origin = $derived(originOf(a, live.snapshot))
  const branch = $derived(agentBranch(a))
  /** From its agent's branch: preselected when it depends on its changes. */
  let fromBranch = $state<boolean | null>(null)
  const useBranch = $derived(!!branch && (fromBranch ?? a.needs_current_branch))
  const canTicket = $derived(hasCore() && offers('create') && !!live.snapshot?.sprint?.boards?.length)
  const canThen = $derived(!!origin && origin.state === 'running')
  /** Its agent's session takes it: running (after its turn), or exited (resumed first). */
  const session = $derived(sessionOf(a, live.snapshot))
  const sessionLabel = $derived(session?.state === 'exited' ? t('Resume the session') : t('In this session'))
  const sessionTitle = $derived(session?.state === 'exited' ? t('{agent} resumed, then given it: it does it in its session', { agent: session.title }) : t('{agent} does it in its session, once its current turn is over', { agent: session?.title ?? '' }))
  /** Where it is done: the agent named on the line, as it is now. */
  const doer = $derived(a.agent ? live.snapshot?.agents?.find((x) => x.id === a.agent) : undefined)
  const main = $derived(mainAction(a, canTicket))
  const desktop = $derived(a.seen.some((s) => s.cli === 'desktop'))
  const byAgent = $derived(a.state === 'dismissed' && a.dismissed_by === 'agent')
  let showPrompt = $state(false)
  let dismissing = $state(false)
  let why = $state('')
  let busy = $state(false)

  async function run(f: () => Promise<unknown>, done?: string) {
    busy = true
    try {
      await f()
      if (done) toast(done)
    } catch (e) {
      toast(String(e), 6000)
    } finally {
      busy = false
    }
  }
  const launch = () => run(() => launchAside(a, useBranch))
  // An agent wrote its prompt, which runs unattended once its agent is done: read first (the first
  // click shows it), chained at the second.
  const then = () => {
    if (!showPrompt) {
      showPrompt = true
      return
    }
    return run(() => asideThen(a, useBranch), t('Runs after {agent}', { agent: origin?.title ?? '' }))
  }
  const ticket = () =>
    run(async () => {
      const key = await asideTicket(a)
      toast(t('Ticket {key} created', { key }), 6000, { label: t('Open it'), run: () => openTicket(key) })
    })
  const toSession = () => run(() => asideSession(a), t('Sent to {agent}: done after its turn', { agent: session?.title ?? '' }))
  const dismiss = () => run(() => asideDismiss(a, why)).then(() => ((dismissing = false), (why = '')))
  const seenBy = $derived(a.seen.map((s) => s.name).filter(Boolean))
</script>

<div class="aside" class:chat class:struck={withdrawn || byAgent} class:done={!isOpen(a)}>
  <div class="hd">
    <span class="tag"><Icon name="aside" size={11} />{t('In passing')}</span>
    {#if a.state !== 'open'}<span class="st {a.state}">{byAgent ? t('withdrawn by the agent') : STATE_LABEL[a.state]}</span>{/if}
    {#if desktop}<span class="pill" title={t('Seen in a Claude Desktop session: its own chip stays in Desktop')}>Desktop</span>{/if}
    {#if a.files_changed}<span class="pill warn" title={t('A commit of another branch changed its files since: maybe done already')}>{t('files changed since')}</span>{/if}
    <span class="grow"></span>
    {#if !chat && seenBy.length}<span class="by" title={a.seen.map((s) => `${s.name}: ${s.tldr}`).join('\n')}>{seenBy.length > 1 ? tn(seenBy.length, 'seen by {n} agent', 'seen by {n} agents') : t('seen by {name}', { name: seenBy[0] })}</span>{/if}
    <span class="age" title={new Date(a.at).toLocaleString()}>{ago(a.at)}</span>
  </div>
  <b class="title">{a.title}</b>
  {#if a.tldr}<p class="tldr">{a.tldr}</p>{/if}
  {#if a.files.length || a.kind}
    <div class="meta">
      {#each a.files.slice(0, 4) as f (f)}<code title={f}>{baseName(f)}</code>{/each}
      {#if a.files.length > 4}<span class="more" title={a.files.slice(4).join('\n')}>+{a.files.length - 4}</span>{/if}
      {#if a.kind}<KindBadge name={a.kind} />{/if}
    </div>
  {/if}
  {#if a.state === 'session'}
    <div class="out"><Icon name="next" size={11} />{t('in the session of')} <button class="lnk" onclick={() => a.agent && openAgents(a.agent)}>{doer?.title ?? a.agent_title ?? t('its agent')}</button>{t(', after its turn')}</div>
  {:else if a.state === 'done'}
    <div class="out"><Icon name="check" size={11} />{t('done by')} <button class="lnk" onclick={() => a.agent && openAgents(a.agent)}>{doer?.title ?? a.agent_title ?? t('its agent')}</button>{#if a.why}: {a.why}{/if}</div>
  {:else if a.state === 'launched' && a.agent}
    <div class="out"><Icon name="next" size={11} />{t('launched in')} <button class="lnk" onclick={() => openAgents(a.agent!)}>{a.agent_title ?? t('its agent')}</button></div>
  {:else if a.state === 'then'}
    <div class="out"><Icon name="next" size={11} />{t('runs once {agent} is done, from {from}', { agent: origin?.title ?? t('its agent'), from: a.from_branch ? t('its branch') : t('the base') })}</div>
  {:else if a.state === 'ticket' && a.ticket}
    <div class="out"><Icon name="kanban" size={11} />{t('ticket')} <button class="lnk" onclick={() => openTicket(a.ticket!)}>{a.ticket}</button></div>
  {:else if a.state === 'dismissed' && a.why}
    <div class="out">{a.why}</div>
  {/if}
  {#if showPrompt}<pre class="prompt">{a.prompt}</pre>{/if}
  {#if hasCore()}
    {#if isOpen(a) && !withdrawn}
      {#if dismissing}
        <div class="acts">
          <!-- svelte-ignore a11y_autofocus : the reason is what this row is for -->
          <input bind:value={why} placeholder={t('Why (told to the agent, optional)')} autofocus onkeydown={(e) => { if (e.key === 'Enter') dismiss(); else if (e.key === 'Escape') { e.stopPropagation(); dismissing = false } }} />
          <button class="b ko" disabled={busy} onclick={dismiss}>{t('Dismiss')}</button>
          <button class="b ghost" onclick={() => (dismissing = false)}>{t('Cancel')}</button>
        </div>
      {:else}
        <div class="acts">
          {#if main === 'session' && session}
            <button class="b pri" disabled={busy} onclick={toSession} title={sessionTitle}>{sessionLabel}</button>
            <button class="b" onclick={launch} title={t('The launcher, prefilled: Enter launches it')}>{t('Launch…')}</button>
          {:else if main === 'then' && canThen}
            <button class="b pri" disabled={busy} onclick={then} title={t('A “Then” step of its chain: another agent does it once it is done')}>{showPrompt ? t('Chain it after this agent') : t('After this agent')}</button>
            <button class="b" onclick={launch} title={t('The launcher, prefilled: Enter launches it')}>{t('Launch…')}</button>
          {:else if main === 'ticket'}
            <button class="b pri" disabled={busy} onclick={ticket}>{t('Ticket')}</button>
            <button class="b" onclick={launch} title={t('The launcher, prefilled: Enter launches it')}>{t('Launch…')}</button>
          {:else}
            <button class="b pri" onclick={launch} title={t('The launcher, prefilled: Enter launches it')}>{t('Launch…')}</button>
          {/if}
          {#if branch && a.repo}
            <select class="from" value={useBranch ? 'branch' : 'base'} onchange={(e) => (fromBranch = e.currentTarget.value === 'branch')} title={t('Where its worktree starts: the repository’s base, or its agent’s branch (it depends on its changes). After this agent: the branch as it is then.')} aria-label={t('Start from')}>
              <option value="base">{t('from the base')}</option>
              <option value="branch">{t('from {branch}', { branch })}</option>
            </select>
          {/if}
          {#if main !== 'then' && canThen}<button class="b" disabled={busy} onclick={then} title={t('A “Then” step of its chain: another agent does it once it is done')}>{showPrompt ? t('Chain it after this agent') : t('After this agent')}</button>{/if}
          {#if main !== 'ticket' && canTicket}<button class="b" disabled={busy} onclick={ticket} title={t('A ticket of the tracker: its title, its tldr and prompt in a comment')}>{t('Ticket')}</button>{/if}
          {#if a.state === 'open'}<button class="b ghost" disabled={busy} onclick={() => run(() => asideLater(a))} title={t('Kept in the list, without its badge')}>{t('Later')}</button>{/if}
          <button class="b ghost" class:on={showPrompt} onclick={() => (showPrompt = !showPrompt)} aria-pressed={showPrompt} title={t('What the new agent is given')}>{t('Prompt')}</button>
          <span class="grow"></span>
          <button class="b ghost x" onclick={() => (dismissing = true)} title={t('Dismiss: the reason is told to the agent, which will not propose it again')} aria-label={t('Dismiss')}><Icon name="close" size={12} /></button>
        </div>
      {/if}
    {:else if !chat && !withdrawn}
      <div class="acts">
        <button class="b ghost" class:on={showPrompt} onclick={() => (showPrompt = !showPrompt)} aria-pressed={showPrompt}>{t('Prompt')}</button>
        {#if a.state === 'session'}<button class="b" disabled={busy} onclick={() => run(() => asideDone(a))} title={t('Done in its session, its agent not having said so')}>{t('Mark done')}</button>{/if}
        {#if a.state === 'dismissed' || a.state === 'then' || a.state === 'session'}<button class="b ghost" disabled={busy} onclick={() => run(() => asideReopen(a))} title={t('Back among the open ones')}>{t('Reopen')}</button>{/if}
        <span class="grow"></span>
        {#if a.state !== 'then' && a.state !== 'session'}<button class="b ghost" disabled={busy} onclick={() => run(() => asideForget(a))} title={t('Out of the list for good')}>{t('Remove')}</button>{/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .aside { display: flex; flex-direction: column; gap: 4px; padding: 8px 10px; border: 1px solid color-mix(in srgb, var(--info) 35%, var(--border)); border-left: 3px solid var(--info); border-radius: var(--r6); background: color-mix(in srgb, var(--info) 5%, var(--surface)); font-size: var(--fs-body); }
  .aside.done { border-color: var(--border); border-left-color: var(--border); background: var(--surface); }
  .aside.chat { margin: 2px 0; max-width: 720px; }
  .struck .title, .struck .tldr { text-decoration: line-through; color: var(--text-muted); }
  .hd { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; font-size: var(--fs-tag); color: var(--text-muted); }
  .tag { display: inline-flex; align-items: center; gap: 3px; color: var(--info); font-weight: 600; }
  .st { padding: 0 6px; border-radius: var(--r9); background: var(--surface-muted); }
  .st.launched, .st.then, .st.session, .st.done { color: var(--ok); background: var(--ok-bg); }
  .st.ticket { color: var(--info); background: var(--info-bg, color-mix(in srgb, var(--info) 12%, transparent)); }
  .pill { padding: 0 6px; border-radius: var(--r9); background: var(--surface-muted); }
  .pill.warn { color: var(--warn); background: var(--warn-bg); }
  .grow { flex: 1; }
  .by { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 50%; }
  .title { font-weight: 600; }
  .tldr { margin: 0; color: var(--text-muted); line-height: 1.4; }
  .meta { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; }
  code { padding: 0 5px; border-radius: var(--r5); background: var(--surface-muted); font-size: var(--fs-tag); }
  .more { color: var(--text-muted); font-size: var(--fs-tag); }
  .out { display: flex; align-items: center; gap: 4px; color: var(--text-muted); font-size: var(--fs-meta); }
  .lnk { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; text-decoration: underline; }
  .prompt { margin: 2px 0; max-height: 220px; overflow: auto; padding: 6px 8px; border-radius: var(--r5); background: var(--surface-muted); font-size: var(--fs-meta); white-space: pre-wrap; }
  .acts { display: flex; flex-wrap: wrap; align-items: center; gap: 5px; margin-top: 2px; }
  .b { display: inline-flex; align-items: center; gap: 4px; height: 22px; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .b.pri { border-color: transparent; background: var(--info); color: var(--surface); font-weight: 600; }
  .b.ko { border-color: transparent; background: var(--ko); color: #fff; }
  .b.ghost { border-color: transparent; background: none; color: var(--text-muted); }
  .b.ghost.on { color: var(--text); background: var(--surface-muted); }
  .b:disabled { opacity: 0.5; cursor: default; }
  .b.x { padding: 0 5px; }
  .from { height: 22px; max-width: 200px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  input { flex: 1; min-width: 140px; height: 22px; padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  :global(html.remote) .aside .b, :global(html.remote) .aside .from, :global(html.remote) .aside input { height: 34px; font-size: 14px; }
</style>
