<script lang="ts">
  import { baseName } from '../platform'
  import { t as tr, tn, tc } from '../i18n.svelte'
  import { clickedOutside } from '../drawers'
  import { isPinned } from '../drawer-pins'
  import PinButton from './PinButton.svelte'
  import { live } from '../live.svelte'
  import { closeThread, resolveThread, threadView, type Thread } from '../thread.svelte'
  import { openTicket, canPreview } from '../ticket-view.svelte'
  import { openMr } from '../mr-view.svelte'
  import { canShowLogs, openPipeline } from '../logs.svelte'
  import { openAgents } from '../agents.svelte'
  import { openEvent } from '../agenda.svelte'
  import { openWorktree } from '../worktrees'
  import { ticketKey } from '../tracker'
  import { mrKey } from '../mr-ref'
  import { mrNumber, mrWord } from '../forge'
  import MrTitle from './MrTitle.svelte'
  import { ago, span } from '../time'
  import { locale } from '../locale'
  import { agentName } from '../agent-cli'
  import { waitsForYou } from '../rules'
  import { hasCore } from '../bridge'
  import Icon from './Icon.svelte'
  import type { IconName } from '../icons'
  import type { SessionStatus } from '../types'

  /**
   * A piece of work's thread in a side drawer: from its ticket to production, each link in a
   * row (ticket, worktree, agent, MR, pipeline, environments), the one missing flagged; then what
   * Prism sees wrong about it, and its figures (time logged, agents' cost, meetings, blockers).
   * Each link opens its own drawer or view. Escape or a click outside closes it.
   */
  const now = $derived(live.snapshot ? Date.now() : 0)
  const th = $derived<Thread | null>(threadView.ref && live.snapshot ? resolveThread(threadView.ref, live.snapshot, now) : null)
  const app = hasCore()

  type Tone = 'on' | 'ko' | 'warn' | 'miss' | 'todo'
  interface Node {
    key: string
    icon: IconName
    label: string
    value: string
    state: string
    tone: Tone
    title: string
    open?: () => void
  }
  const AGENT: Record<SessionStatus, string> = { running: tc('tickets', 'working'), background: tr('works in the background'), waiting_tool: tr('asks permission'), needs_you: tr('waits for you'), review_ready: tr('ready for review'), completed: tc('tickets', 'done'), idle: tr('idle') }

  const nodes = $derived.by((): Node[] => {
    if (!th) return []
    const t = th.ticket
    const w = th.worktree
    const a = th.agents[0]
    const m = th.mr
    const p = th.pipeline
    const out: Node[] = [
      t
        ? { key: 'ticket', icon: 'kanban', label: tr('Ticket'), value: t.id, state: t.status, tone: 'on', title: `${t.id} · ${t.name}`, open: canPreview() ? () => openTicket(ticketKey(t)) : undefined }
        : { key: 'ticket', icon: 'kanban', label: tr('Ticket'), value: '—', state: tr('none found'), tone: 'todo', title: tr('No ticket found in its branch or MR') },
      w
        ? { key: 'worktree', icon: 'worktree', label: tr('Worktree'), value: baseName(w.path) || w.path, state: w.dirty ? tr('{n} changed', { n: w.changes }) : w.ahead ? tr('{n} ahead', { n: w.ahead }) : tr('clean'), tone: 'on', title: `${w.path}\n${w.branch ?? ''}${app ? `\n${tr('Click: open in the editor')}` : ''}`, open: app ? () => void openWorktree(w.path, 'editor') : undefined }
        : { key: 'worktree', icon: 'worktree', label: tr('Worktree'), value: th.branch ? tr('none') : '—', state: th.missing.has('worktree') ? tr('in progress, no branch') : th.branch ? th.branch : '', tone: th.missing.has('worktree') ? 'miss' : 'todo', title: th.missing.has('worktree') ? tr('The ticket is in progress, but no worktree, branch or agent works on it') : tr('No local worktree') },
      a
        ? { key: 'agent', icon: 'claude', label: agentName(), value: th.agents.length > 1 ? tr('{n} agents', { n: th.agents.length }) : a.state === 'running' ? AGENT[a.status] : tr('stopped'), state: a.title, tone: a.state !== 'running' ? 'todo' : waitsForYou(a.status) ? 'warn' : 'on', title: th.agents.map((x) => `${x.title} · ${x.state === 'running' ? AGENT[x.status] : tr('stopped')}`).join('\n'), open: app ? () => openAgents(a.id) : undefined }
        : { key: 'agent', icon: 'claude', label: agentName(), value: '—', state: tr('none'), tone: 'todo', title: tr('No agent on it') },
      m
        ? { key: 'mr', icon: 'mr', label: mrWord(), value: mrNumber(m.iid), state: m.state === 'merged' ? tr('merged {ago}', { ago: ago(m.merged_at) }) : m.draft ? tr('draft') : m.approvals_left ? tn(m.approvals_left, '{n} approval left', '{n} approvals left') : tr('approved'), tone: m.state === 'merged' || !m.approvals_left ? 'on' : 'warn', title: `${mrKey(m)} · ${m.title}`, open: () => openMr(m) }
        : { key: 'mr', icon: 'mr', label: mrWord(), value: th.missing.has('mr') ? tr('none') : '—', state: th.missing.has('mr') ? tr('commits, no MR') : tr('not opened'), tone: th.missing.has('mr') ? 'miss' : 'todo', title: th.missing.has('mr') ? tr('The branch has commits but no MR') : tr('No MR yet') },
      p
        ? { key: 'pipeline', icon: 'running', label: 'Pipeline', value: `#${p.id}`, state: p.status === 'failed' && p.failed_jobs.length ? tr('{job} failed', { job: p.failed_jobs[0] }) : p.status, tone: p.status === 'failed' ? 'ko' : p.status === 'success' ? 'on' : 'warn', title: `${p.status}${p.failed_jobs.length ? `: ${p.failed_jobs.join(', ')}` : ''}${canShowLogs() ? `\n${tr('Click: its logs')}` : ''}`, open: canShowLogs() && m ? () => void openPipeline(p.id, mrKey(m), m.project ?? null) : undefined }
        : { key: 'pipeline', icon: 'running', label: 'Pipeline', value: m?.state === 'merged' ? '✓' : '—', state: m?.state === 'merged' ? tr('merged') : '', tone: m?.state === 'merged' ? 'on' : 'todo', title: m?.state === 'merged' ? tc('tickets', 'Merged') : tr('No pipeline yet') },
    ]
    for (const e of th.envs) {
      const miss = th.missing.has(`env:${e.env}`)
      out.push({
        key: `env:${e.env}`,
        icon: 'rocket',
        label: e.env,
        value: !m?.merged_at ? '—' : e.since ? tr('updated') : tr('not yet'),
        state: !m?.merged_at ? tr('not merged') : e.since ? ago(e.at) : miss ? tr('waits {ago}', { ago: ago(m.merged_at) }) : '',
        tone: !m?.merged_at ? 'todo' : e.since ? 'on' : miss ? 'miss' : 'todo',
        title: !m?.merged_at ? tr('Not merged yet') : e.since ? tr('Its workloads were updated {ago}, after the merge ({merged}): a deploy happened since. Prism compares the times, not the commits.', { ago: ago(e.at), merged: ago(m.merged_at) }) : tr('Not updated since the merge'),
      })
    }
    return out
  })

  const hm = (iso: string) => new Date(iso).toLocaleString(locale(), { weekday: 'short', hour: '2-digit', minute: '2-digit' })
  const waiting = $derived((th?.ticket?.waiting_on ?? []).filter((d) => !d.closed))
  const blocking = $derived((th?.ticket?.blocking ?? []).filter((d) => !d.closed))

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !threadView.ref) return
    e.stopImmediatePropagation()
    closeThread()
  }
  let drawerEl: HTMLElement | undefined = $state()
  function onOutside(e: PointerEvent) {
    if (threadView.ref && clickedOutside(e, drawerEl)) {
      closeThread()
    }
  }
</script>

<svelte:window onkeydowncapture={onKey} onpointerdowncapture={onOutside} />

{#if threadView.ref}
  <aside class="drawer" bind:this={drawerEl} data-drawer="48" data-pinned={isPinned('thread')} aria-label={tr('Thread')}>
    <header>
      <Icon name="branch" />
      <span class="grow">{tr('Thread')}{th?.branch ? ` · ${th.branch}` : ''}</span>
      <PinButton kind="thread" />
      <button class="icon" onclick={closeThread} title={tr('Close (Esc)')} aria-label={tr('Close')}><Icon name="close" /></button>
    </header>
    <div class="body">
      {#if !th}
        <p class="muted">{tr('Nothing found around it: no ticket, MR, worktree or agent Prism knows.')}</p>
      {:else}
        <h2>
          {#if th.ticket}<span class="tid mono">{th.ticket.id}</span>{th.ticket.name}{:else if th.mr}{th.mr.title}{:else}{th.branch}{/if}
        </h2>
        {#if th.ticket}<p class="sub muted">{th.ticket.status}{th.ticket.assignees.length ? ` · ${th.ticket.assignees.map((a) => a.name).join(', ')}` : ''}{th.ticket.created_at ? ` · ${tr('created {ago}', { ago: ago(th.ticket.created_at) })}` : ''}</p>{/if}

        <ol class="chain" style:--n={nodes.length}>
          {#each nodes as n (n.key)}
            <li class={n.tone}>
              <svelte:element this={n.open ? 'button' : 'div'} class="bub" role={n.open ? undefined : 'img'} onclick={n.open} title={n.title} aria-label="{n.label}: {n.value} {n.state}">
                <Icon name={n.icon} />
              </svelte:element>
              <span class="lb">{n.label}</span>
              <span class="vl mono" title={n.title}>{n.value}</span>
              <span class="st" title={n.state}>{n.state}</span>
            </li>
          {/each}
        </ol>

        {#each th.drift as a (a.text)}
          <p class="drift {a.severity}"><Icon name="warning" />{a.text}</p>
        {/each}

        <div class="facts">
          {#if th.logged}<div class="fact"><b>{span(th.logged)}</b>{tr('time logged')}</div>{/if}
          {#if th.cost}<div class="fact"><b>${th.cost.toFixed(2)}</b>{tr('agents’ cost')} · {tn(th.agents.length, '{n} agent', '{n} agents')}</div>{/if}
          {#each th.meetings.slice(0, 2) as e (e.id)}
            <button class="fact link" onclick={() => openEvent(e.id)} title={e.title}><b>{hm(e.start)}</b>{e.title}</button>
          {/each}
          {#if waiting.length}<div class="fact"><b>{tr('Waiting on')}</b>{waiting.map((d) => d.id || d.name).join(', ')}</div>{/if}
          {#if blocking.length}<div class="fact"><b>{tc('tickets', 'Blocks')}</b>{blocking.map((d) => d.id || d.name).join(', ')}</div>{/if}
        </div>

        {#if th.mrs.length > 1}
          <section>
            <h3>{mrWord()}s <small>{th.mrs.length}</small></h3>
            {#each th.mrs as m (mrKey(m))}
              <button class="line" onclick={() => openMr(m)}><span class="mono">{mrKey(m)}</span><span class="grow"><MrTitle mr={m} draft /></span><span class="muted">{m.state === 'merged' ? tr('merged') : m.pipeline ?? ''}</span></button>
            {/each}
          </section>
        {/if}
        {#if th.agents.length > 1}
          <section>
            <h3>{agentName()}s <small>{th.agents.length}</small></h3>
            {#each th.agents as a (a.id)}
              <button class="line" onclick={() => openAgents(a.id)} disabled={!app}><span class="grow">{a.title}</span><span class="muted">{a.state === 'running' ? AGENT[a.status] : tr('stopped')}</span></button>
            {/each}
          </section>
        {/if}
      {/if}
    </div>
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 48; width: min(620px, 94vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border));
    box-shadow: var(--shadow-drawer);
  }
  header {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; border-bottom: 1px solid var(--border);
    box-shadow: inset 0 2px var(--accent); font-size: var(--fs-meta); color: var(--text-muted);
  }
  .grow { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .icon { width: 24px; height: 24px; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover { background: var(--surface-muted); color: var(--text); }
  .body { flex: 1; min-height: 0; overflow: auto; padding: 12px 16px 24px; }
  h2 { margin: 0 0 2px; font-size: 15px; font-weight: 600; line-height: 1.35; }
  .tid { margin-right: 8px; color: var(--info); font-size: var(--fs-meta); font-weight: 500; }
  .sub { margin: 0; font-size: var(--fs-meta); }
  .chain { display: grid; grid-template-columns: repeat(var(--n), minmax(0, 1fr)); margin: 16px 0 12px; padding: 0; list-style: none; }
  .chain li { position: relative; display: flex; flex-direction: column; align-items: center; gap: 1px; padding: 0 3px; text-align: center; min-width: 0; }
  .chain li::before { content: ''; position: absolute; top: 13px; left: -50%; width: 100%; height: 2px; background: var(--ok); }
  .chain li:first-child::before { display: none; }
  .chain li.todo::before { background: var(--border); }
  .chain li.miss::before { background: repeating-linear-gradient(90deg, var(--warn) 0 5px, transparent 5px 9px); }
  .chain li.ko::before { background: var(--ko); }
  .bub { position: relative; z-index: 1; display: grid; place-items: center; width: 28px; height: 28px; margin-bottom: 4px; padding: 0; border: 2px solid var(--ok); border-radius: 50%; background: var(--surface); color: var(--text); font: inherit; }
  button.bub { cursor: pointer; }
  button.bub:hover { background: var(--surface-muted); }
  .todo .bub { border-color: var(--border); color: var(--text-muted); }
  .warn .bub { border-color: var(--warn); }
  .ko .bub { border-color: var(--ko); color: var(--ko); }
  .miss .bub { border-color: var(--warn); border-style: dashed; color: var(--warn); }
  .lb { font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .vl, .st { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vl { font-size: var(--fs-meta); }
  .st { font-size: var(--fs-tag); color: var(--text-muted); }
  .miss .st { color: var(--warn); }
  .ko .st { color: var(--ko); }
  .drift { display: flex; align-items: center; gap: 6px; margin: 0 0 6px; padding: 5px 9px; border-left: 3px solid var(--warn); border-radius: var(--r4); background: var(--warn-bg); font-size: var(--fs-meta); }
  .drift.ko { border-left-color: var(--ko); background: var(--ko-bg); }
  .facts { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 8px; margin-top: 10px; }
  .fact { padding: 6px 9px; border: none; border-radius: var(--r6); background: var(--surface-muted); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fact b { display: block; color: var(--text); font-size: var(--fs-body); font-weight: 500; }
  .fact.link { cursor: pointer; }
  .fact.link:hover { background: color-mix(in srgb, var(--accent) 10%, var(--surface-muted)); }
  section { padding-top: 12px; margin-top: 12px; border-top: 1px solid var(--border); }
  h3 { margin: 0 0 6px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  h3 small { font-weight: 400; margin-left: 6px; }
  .line { display: flex; gap: 8px; width: 100%; padding: 3px 4px; border: none; border-radius: var(--r4); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .line:hover { background: var(--surface-muted); }
  .muted { color: var(--text-muted); }
</style>
