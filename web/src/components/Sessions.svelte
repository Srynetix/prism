<script lang="ts">
  import ProposalChip from './ProposalChip.svelte'
  import { threadAttr } from '../thread.svelte'
  import { mrNumber } from '../forge'
  import BlockControls from './BlockControls.svelte'
  import QuickAnswers from './QuickAnswers.svelte'
  import OverlapChip from './OverlapChip.svelte'
  import { overlapsOfAgent } from '../overlaps'
  import { isCollapsed, isShort } from '../layout.svelte'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import type { Agent, Snapshot } from '../types'
  import { behindLimit, isWorking, SESSION_LABEL, SESSION_RANK, waitsForYou } from '../rules'
  import { ago } from '../time'
  import { setQuery } from '../filter.svelte'
  import { agentsEnabled, openAgents } from '../agents.svelte'
  import { isArchived } from '../agent-archive.svelte'
  import { keys } from '../platform'
  import { t } from '../i18n.svelte'

  /**
   * Agents block: the agents started by Prism, what awaits me first. Claude
   * Desktop sessions stay in the Agents view (Desktop filter). Plan usage is in the status bar.
   */
  let { snap }: { snap: Snapshot } = $props()

  const STOPPED: Record<Agent['state'], string> = { running: '', exited: t('exited'), gone: t('stopped'), prepared: t('prepared') }
  const agents = $derived(
    // Archived ones are out (the Agents view lists them), until they wait for you.
    [...(snap.agents ?? [])].filter((a) => !isArchived(a)).sort((a, b) =>
      Number(b.state === 'running') - Number(a.state === 'running')
      || SESSION_RANK[a.status] - SESSION_RANK[b.status]
      || (b.last_activity ?? b.created_at).localeCompare(a.last_activity ?? a.created_at)),
  )
  const isWaiting = (a: Agent) => a.state === 'running' && waitsForYou(a.status)
  const running = $derived(agents.filter((a) => a.state === 'running' && isWorking(a.status)).length)
  const waiting = $derived(agents.filter(isWaiting).length)

  const mrOf = (a: Agent) =>
    a.branch ? (snap.merge_requests.find((m) => !m.repo && m.source_branch === a.branch && m.state === 'opened') ?? snap.merge_requests.find((m) => !m.repo && m.source_branch === a.branch)) : undefined
  const worktreeOf = (a: Agent) => snap.worktrees.find((w) => w.path === (a.worktree ?? a.cwd))
  /** Row tooltip: what no longer fits on a compact row. */
  const tooltip = (a: Agent) =>
    [a.title, `${a.state === 'running' ? SESSION_LABEL[a.status] : STOPPED[a.state]} · ${a.branch ?? a.cwd}`, a.needs_action ? `→ ${a.needs_action}` : '', t('Open in the Agents view')]
      .filter(Boolean)
      .join('\n')
</script>

<section class="panel" style="--src: var(--src-claude)" class:collapsed={isCollapsed('sessions')}>
  <h2><span class="title"><Logo name="agent" />Agents</span> <small>{t('{running} running · {waiting} waiting', { running, waiting })}</small><BlockControls id="sessions" /></h2>
  {#if isShort('sessions')}
    <!-- Short: only the agents waiting for me. -->
    <div class="short-body">
      {#each agents.filter(isWaiting) as a (a.id)}
        <div class="srow"><span class="st {a.status}"></span><button class="t" onclick={() => openAgents(a.id)} title={tooltip(a)}>{a.title}</button><span class="qa"><QuickAnswers agent={a} compact /></span><span class="pill {a.status === 'waiting_tool' ? 'ko' : 'warn'}">{a.status === 'waiting_tool' ? t('tool') : t('asks')}</span></div>
      {:else}
        <div class="srow more">{t('No agent waiting for you.')}</div>
      {/each}
    </div>
  {:else}
  <div class="scroll">
    {#each agents as a (a.id)}
      {@const mr = mrOf(a)}
      {@const wt = worktreeOf(a)}
      {@const status = a.state === 'running' ? a.status : 'completed'}
      <div class="s" class:dim={status === 'completed' || status === 'idle'} data-thread={threadAttr({ agent: a.id })}>
        <span class="st {status}" title={a.state === 'running' ? SESSION_LABEL[a.status] : STOPPED[a.state]}></span>
        <button class="t" onclick={() => openAgents(a.id)} title={tooltip(a)}><Icon name="terminal" />{a.title}</button>
        {#if isWaiting(a) || a.status === 'review_ready'}<span class="qa"><QuickAnswers agent={a} compact /></span>{/if}
        {#if isWaiting(a)}<span class="pill {a.status === 'waiting_tool' ? 'ko' : 'warn'} asks" title={a.needs_action ?? SESSION_LABEL[a.status]}>{a.status === 'waiting_tool' ? t('tool') : t('asks')}</span>{/if}
        <ProposalChip agent={a} />
        {#if a.state !== 'running'}<span class="pill">{STOPPED[a.state]}</span>{/if}
        {#if overlapsOfAgent(snap, a).length}<OverlapChip seen={overlapsOfAgent(snap, a)} short />{/if}
        {#if a.ticket}<button class="pill info" onclick={() => setQuery(a.ticket!)} title={t('Filter on {ticket}', { ticket: a.ticket })}>{a.ticket}</button>{/if}
        {#if mr}<a class="mono mr" href={mr.url} target="_blank" rel="noreferrer" title="{mr.title}{mr.state === 'merged' ? ` ${t('(merged)')}` : mr.draft ? ` ${t('(draft)')}` : ''}">{mrNumber(mr.iid)}</a>{/if}
        {#if mr?.state === 'opened' && mr.pipeline === 'failed'}<span class="pill ko">CI</span>{/if}
        {#if wt && !wt.is_main && wt.behind > behindLimit() && mr?.state !== 'merged'}<span class="pill warn" title={t('{n} commits behind {base}', { n: wt.behind, base: wt.base })}><Icon name="down" />{wt.behind}</span>{/if}
        <span class="mono muted when">{ago(a.last_activity ?? a.created_at)}</span>
      </div>
    {:else}
      <div class="empty">{t('No agent.')}{#if agentsEnabled()} <b>{keys('⌘N')}</b> {t('starts one.')}{/if}</div>
    {/each}
  </div>
  {/if}
</section>

<style>
  /* One row per session: the details (branch, pending question) are in the tooltip. */
  .s { display: flex; align-items: center; gap: 6px; padding: 3px 12px; border-top: 1px solid var(--border); font-size: var(--fs-body); min-height: 26px; }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--text); display: inline-flex; align-items: center; gap: 5px; padding: 0; border: none; background: none; font: inherit; text-align: left; cursor: pointer; }
  .t:hover { color: var(--accent); text-decoration: underline; text-underline-offset: 2px; }
  button.t :global(svg) { color: var(--src-claude); }
  .s .pill { flex-shrink: 0; }
  .mr { font-size: var(--fs-meta); flex-shrink: 0; }
  .when { width: 28px; text-align: right; flex-shrink: 0; font-size: var(--fs-meta); }
  .asks { cursor: help; }
  .st { width: 8px; height: 8px; border-radius: 50%; display: inline-block; flex-shrink: 0; }
  .st.running { background: var(--ok); animation: pulse 1.4s infinite; }
  .st.background { background: var(--ok); opacity: 0.6; }
  .st.needs_you { background: var(--warn); }
  .st.waiting_tool { background: var(--ko); }
  .st.review_ready { background: var(--info); }
  .st.completed, .st.idle { background: var(--text-muted); opacity: 0.5; }
  .dim { opacity: 0.6; }
  button.pill { border: none; cursor: pointer; font: inherit; font-size: var(--fs-tag); }
  /* An agent's quick answers: on the row's hover, as its other actions, over its right end: out of
     the row's flow, they never take the title's room. */
  .s, .srow { position: relative; }
  .qa { position: absolute; right: 8px; top: 50%; translate: 0 -50%; display: inline-flex; padding: 2px 0 2px 8px; background: linear-gradient(to right, transparent, var(--surface) 8px); opacity: 0; pointer-events: none; transition: opacity 0.1s; }
  .srow:hover .qa, .s:hover .qa, .qa:focus-within { opacity: 1; pointer-events: auto; }
</style>
