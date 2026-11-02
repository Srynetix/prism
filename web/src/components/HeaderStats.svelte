<script lang="ts" module>
  /** Open panel; `i` opens "For me" from the keyboard (App.svelte). */
  export const headerStats = $state({ open: null as 'inbox' | 'health' | null })
  export const toggleInbox = () => (headerStats.open = headerStats.open === 'inbox' ? null : 'inbox')
</script>

<script lang="ts">
  import { branchName } from '../pipeline'
  import { hasCore } from '../bridge'
  import { anchored, dismissable, exclusive } from '../popup'
  import { openMr } from '../mr-view.svelte'
  import { mrKey } from '../mr-ref'
  import MrTitle from './MrTitle.svelte'
  import { agentsEnabled, openAgents } from '../agents.svelte'
  import { agentName } from '../agent-cli'
  import { showBlock } from '../layout.svelte'
  import { ago } from '../time'
  import { waitingAlertHours, type Alert } from '../rules'
  import type { ClaudeSession, MergeRequest, Pipeline, PodView } from '../types'
  import Icon from './Icon.svelte'
  import Odometer from './Odometer.svelte'
  import { t } from '../i18n.svelte'

  /**
   * Header stats, in two clusters. "For me": what awaits an action from me
   * (reviews, Claude sessions, desyncs), shown only if there are any. "CI": a
   * single health indicator, colored only when there is a problem (failed pipeline on a
   * protected branch, runner down). Each cluster opens its detail.
   */
  let { managers, jobs, failures, reviews, waiting, syncs }: {
    managers: PodView[]
    jobs: PodView[]
    failures: Pipeline[]
    reviews: MergeRequest[]
    waiting: ClaudeSession[]
    syncs: Alert[]
  } = $props()

  let inboxEl: HTMLElement | undefined = $state()
  let healthEl: HTMLElement | undefined = $state()

  const todo = $derived(reviews.length + waiting.length + syncs.length)
  const healthy = $derived(managers.filter((p) => p.healthy).length)
  const down = $derived(managers.length - healthy)
  const ko = $derived(failures.length > 0 || down > 0)

  const open = $derived(headerStats.open)
  const close = () => (headerStats.open = null)
  function toggle(e: MouseEvent, which: 'inbox' | 'health') {
    e.stopPropagation()
    headerStats.open = open === which ? null : which
  }
  function go(e: MouseEvent, run: () => void) {
    e.stopPropagation()
    close()
    run()
  }
  const mrFix = (a: Alert) => a.fixes?.find((f) => f.kind === 'mr')
  const openSession = (c: ClaudeSession) => (agentsEnabled() ? openAgents(c.id) : showBlock('sessions'))
</script>

<div class="stats">
  {#if todo}
    <button bind:this={inboxEl} class="inbox" class:on={open === 'inbox'} onclick={(e) => toggle(e, 'inbox')} title={t('Waiting for me (i)')} aria-expanded={open === 'inbox'}>
      <span class="k">{t('For me')}</span>
      {#if reviews.length}<span class="c" style:--g="var(--src-gitlab)"><Icon name="review" /><b><Odometer value={reviews.length} /></b><span class="lbl">{t('reviews')}</span></span>{/if}
      {#if waiting.length}<span class="c" style:--g="var(--src-claude)"><Icon name="more" /><b><Odometer value={waiting.length} /></b><span class="lbl">{agentName()}</span></span>{/if}
      {#if syncs.length}<span class="c" style:--g="var(--src-clickup)"><Icon name="sync" /><b><Odometer value={syncs.length} /></b><span class="lbl">{t('sync')}</span></span>{/if}
    </button>
  {:else}
    <button bind:this={inboxEl} class="clear" onclick={(e) => toggle(e, 'inbox')} title={t('No review, {agent} session or sync waiting for you (i)', { agent: agentName() })} aria-expanded={open === 'inbox'}><Icon name="check" /><span class="lbl">{t('Nothing waiting')}</span></button>
  {/if}

  <button bind:this={healthEl} class="health" class:ko class:on={open === 'health'} onclick={(e) => toggle(e, 'health')} title={t('CI health')} aria-expanded={open === 'health'}>
    <span class="dot"></span>
    {#if failures.length}<b>main ✕ <Odometer value={failures.length} /></b>
    {:else if down}<b>{t('runner down')}</b>
    {:else}<span>CI</span>{/if}
    <span class="sep"></span>
    <Icon name="kube" /><b><Odometer value={healthy} />/<Odometer value={managers.length} /></b>
    <span class="lbl">· <Odometer value={jobs.length} /> {t('jobs')}</span>
  </button>
</div>

{#if open === 'inbox'}
  <div class="pop" role="dialog" aria-label={t('Waiting for me')} {@attach anchored(() => inboxEl, { gap: 6 })} {@attach exclusive(close)} {@attach dismissable(close, { trigger: () => inboxEl, resize: true })}>
    {#if !todo}<div class="empty">{t('Nothing waiting for you.')}</div>{/if}
    {#if reviews.length}
      <div class="sec" style:--g="var(--src-gitlab)"><i></i>{t('Reviews for me')} · {reviews.length}</div>
      {#each reviews as m (mrKey(m))}
        {#if hasCore()}
          <button class="row" onclick={(e) => go(e, () => openMr(m))} title={m.title}><span class="m">{mrKey(m)}</span><span class="t"><MrTitle mr={m} draft /></span><span class="m">{m.author_name || m.author}</span></button>
        {:else}
          <a class="row" href={m.url} target="_blank" rel="noreferrer" title={m.title}><span class="m">{mrKey(m)}</span><span class="t"><MrTitle mr={m} draft /></span><span class="m">{m.author_name || m.author}</span></a>
        {/if}
      {/each}
    {/if}
    {#if waiting.length}
      <div class="sec" style:--g="var(--src-claude)"><i></i>{t('{agent} waiting', { agent: agentName() })} · {waiting.length}</div>
      {#each waiting as c (c.id)}
        <button class="row" onclick={(e) => go(e, () => openSession(c))} title={c.needs_action ?? c.title}><span class="t">{c.title}</span><span class="m">{ago(c.last_activity)}</span></button>
      {/each}
    {/if}
    {#if syncs.length}
      <div class="sec" style:--g="var(--src-clickup)"><i></i>{t('Out of sync')} · {syncs.length}</div>
      {#each syncs as a, i (i)}
        {@const fix = mrFix(a)}
        {#if fix && fix.kind === 'mr' && hasCore()}
          <button class="row" onclick={(e) => go(e, () => openMr(fix))}><span class="t">{a.text}</span></button>
        {:else if a.url}
          <a class="row" href={a.url} target="_blank" rel="noreferrer"><span class="t">{a.text}</span></a>
        {:else}
          <button class="row" onclick={(e) => go(e, () => showBlock('todo'))}><span class="t">{a.text}</span></button>
        {/if}
      {/each}
    {/if}
    <div class="foot">{t('{agent} sessions: last {hours} h · sync: not marked as seen', { agent: agentName(), hours: waitingAlertHours() })}</div>
  </div>
{/if}

{#if open === 'health'}
  <div class="pop" role="dialog" aria-label={t('CI health')} {@attach anchored(() => healthEl, { gap: 6 })} {@attach exclusive(close)} {@attach dismissable(close, { trigger: () => healthEl, resize: true })}>
    {#if failures.length}
      <div class="sec" style:--g="var(--ko)"><i></i>{t('Failed on main/release')} · {failures.length}</div>
      {#each failures as p (p.id)}
        <a class="row" href={p.url} target="_blank" rel="noreferrer"><span class="s ko"></span><span class="m">{branchName(p)}</span><span class="t">{p.failed_jobs.join(', ') || `#${p.id}`}</span><span class="m">{ago(p.updated_at)}</span></a>
      {/each}
    {/if}
    <div class="sec" style:--g="var(--src-k8s)"><i></i>Runners · {healthy}/{managers.length}</div>
    {#each managers as p (p.name)}
      <button class="row" onclick={(e) => go(e, () => showBlock('pods'))}><span class="s" class:ko={!p.healthy}></span><span class="t">{p.name}</span><span class="m">{p.healthy ? t('healthy') : p.status}</span></button>
    {/each}
    <div class="sec" style:--g="var(--info)"><i></i>{t('Active CI jobs')} · {jobs.length}</div>
    {#each jobs as p (p.name)}
      {#if p.job_url}
        <a class="row" href={p.job_url} target="_blank" rel="noreferrer"><span class="s run"></span><span class="t">{p.job_name ?? p.name}</span><span class="m">{p.job_ref ?? ''}</span></a>
      {:else}
        <div class="row"><span class="s run"></span><span class="t">{p.job_name ?? p.name}</span><span class="m">{p.job_ref ?? ''}</span></div>
      {/if}
    {/each}
    <button class="foot link" onclick={(e) => go(e, () => showBlock('pods'))}>{t('Open the Pods block')}</button>
  </div>
{/if}

<style>
  .stats { display: flex; align-items: center; gap: 8px; margin-left: auto; min-width: 0; }
  .stats :global(svg) { width: 12px; height: 12px; flex-shrink: 0; }
  b { font-size: 13px; font-weight: 600; font-variant-numeric: tabular-nums; }

  /* For me: the counters to handle, on a shared background. */
  .inbox { display: flex; align-items: center; gap: 2px; padding: 2px; border: none; border-radius: var(--r7); background: var(--surface-muted); font: inherit; cursor: pointer; white-space: nowrap; }
  .inbox .k { padding: 0 6px 0 7px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); }
  .c { display: inline-flex; align-items: center; gap: 5px; padding: 2px 8px; border-radius: var(--r5); background: var(--surface); box-shadow: 0 0 0 1px var(--border); font-size: var(--fs-mono); color: var(--g); }
  .c .lbl { color: var(--text-muted); }
  .inbox:hover .c, .inbox.on .c { box-shadow: 0 0 0 1px var(--g); }
  .clear { display: inline-flex; align-items: center; gap: 5px; padding: 3px 8px; border: none; border-radius: var(--r6); background: none; font: inherit; font-size: var(--fs-mono); color: var(--ok); white-space: nowrap; cursor: pointer; }
  .clear:hover { background: var(--surface-muted); }
  .empty { padding: 10px 12px 2px; color: var(--text-muted); }

  /* CI: an indicator light, discreet while all is well. */
  .health { display: inline-flex; align-items: center; gap: 6px; padding: 3px 9px 3px 7px; border: 1px solid var(--border); border-radius: var(--r7); background: none; font: inherit; font-size: var(--fs-mono); color: var(--text-muted); cursor: pointer; white-space: nowrap; }
  .health:hover, .health.on { background: var(--surface-muted); }
  .health b { color: var(--text); }
  .health :global(svg) { color: var(--src-k8s); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 3px var(--ok-bg); flex-shrink: 0; }
  .sep { width: 1px; align-self: stretch; background: var(--border); }
  .health.ko { border-color: transparent; background: var(--ko-bg); color: var(--ko); }
  .health.ko b, .health.ko :global(svg) { color: inherit; }
  .health.ko .dot { position: relative; background: var(--ko); box-shadow: 0 0 0 3px color-mix(in srgb, var(--ko) 25%, transparent); }
  /* Its ring widens and fades: a layer scaled (the GPU's), not a shadow animated (a repaint a frame). */
  .health.ko .dot::after { content: ''; position: absolute; inset: -3px; border-radius: 50%; background: color-mix(in srgb, var(--ko) 25%, transparent); animation: pulse 1.6s infinite; }
  @keyframes pulse { 50% { transform: scale(1.3); opacity: 0; } }
  :global(html[data-reduce-motion]) .health.ko .dot::after { animation: none; }
  /* Detail of a cluster. */
  .pop { position: fixed; z-index: 45; width: 360px; max-height: 70vh; overflow: auto; padding-bottom: 2px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r10); box-shadow: var(--shadow-pop); font-size: var(--fs-body); }
  .sec { display: flex; align-items: center; gap: 6px; padding: 8px 12px 3px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); }
  .sec i { width: 6px; height: 6px; border-radius: 50%; background: var(--g); }
  .row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 12px; border: none; background: none; color: var(--text); font: inherit; text-align: left; text-decoration: none; cursor: pointer; }
  .row:hover { background: var(--surface-muted); }
  .row .t { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row .m { flex-shrink: 0; font-family: var(--mono); font-size: var(--fs-meta); color: var(--text-muted); }
  .s { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); flex-shrink: 0; }
  .s.ko { background: var(--ko); }
  .s.run { background: var(--info); }
  .foot { display: block; width: 100%; margin-top: 6px; padding: 6px 12px; border: none; border-top: 1px solid var(--border); background: none; font: inherit; font-size: var(--fs-meta); color: var(--text-muted); text-align: left; }
  .foot.link { cursor: pointer; }
  .foot.link:hover { color: var(--text); }

  @media (max-width: 1500px) {
    .lbl, .inbox .k { display: none; }
  }
</style>
