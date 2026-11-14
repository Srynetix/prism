<script lang="ts">
  import { t, tc, tn } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { keys, tildePath, FILE_MANAGER } from '../platform'
  import { isDesktop, isLocal } from '../bridge'
  import { forgeLabel, forgeUrl } from '../forge'
  import { mrKey } from '../mr-ref'
  import { openMr } from '../mr-view.svelte'
  import { openTicket } from '../ticket-view.svelte'
  import { ago } from '../time'
  import { toast } from '../toast.svelte'
  import { armer } from '../armed.svelte'
  import { cliOf } from '../agent-cli'
  import { agentsAvailable, modelName, openAgents, openLauncher, projectsView, resumeAgent } from '../agents.svelte'
  import { sessionLabel } from '../rules'
  import { canMarkRead, markRead } from '../session-read'
  import { deleteWorktree, openWorktree, pullWorktree } from '../worktrees'
  import { isHidden, setHidden } from '../worktree-archive.svelte'
  import { agentPorts } from '../agent-ports'
  import { repoNotes } from '../notes.svelte'
  import { setPref } from '../layout.svelte'
  import { openQuickNotes } from '../quick-notes.svelte'
  import { openSettings } from '../settings.svelte'
  import { quickActionsHere, runQuickAction } from '../quick-actions'
  import { implement, nextOf, reopenLeft } from '../project-actions'
  import { byActivity, projectSummaries, ticketsFor, waitingAgent, type ProjectSummary, type Stage, type Workstream } from '../workstreams'
  import type { Agent } from '../types'
  import Icon from './Icon.svelte'
  import ArmedLabel from './ArmedLabel.svelte'
  import NewWorkstream from './NewWorkstream.svelte'
  import Worktrees from './Worktrees.svelte'

  /**
   * The Projects page: on the left every project (a repository), grouped by working folder, with
   * its signals (an agent waiting, its main pipeline failed, its workstreams and MRs); on the right
   * the project: its header (branch and gap, pipeline, MRs, notes, ports; editor, terminal, forge),
   * « New workstream », its workstreams as cards by stage (in progress, in review, done · to clean
   * up) or by activity, those to pick up again, and its clone. The Cleanup tab is the Worktrees
   * block over every project. The Agents view answers « who waits for me? », this page « where
   * does my work stand? ».
   */
  const snap = $derived(live.snapshot)
  const projects = $derived(snap ? projectSummaries(snap, { hidden: isHidden }) : [])
  let query = $state('')
  const shown = $derived(query.trim() ? projects.filter((p) => p.name.toLowerCase().includes(query.trim().toLowerCase())) : projects)
  const folders = $derived([...new Set(shown.map((p) => p.workspace))])
  const current = $derived(projects.find((p) => p.name === projectsView.repo) ?? projects.find((p) => p.waiting) ?? projects.find((p) => !p.resting) ?? projects[0])
  const cleanupCount = $derived(projects.reduce((n, p) => n + p.workstreams.filter((w) => w.stage === 'cleanup').length, 0))

  let sort = $state<'stage' | 'activity'>('stage')
  const STAGE_LABEL: Record<Stage, string> = { active: t('In progress'), review: t('In review'), cleanup: t('Done · to clean up'), resume: t('To pick up') }

  let busy = $state(false)
  async function run(f: () => Promise<unknown>) {
    if (busy) return
    busy = true
    try {
      await f()
    } catch (e) {
      toast(String(e), 8000)
    } finally {
      busy = false
    }
  }
  const armed = armer(5000)

  const pick = (p: ProjectSummary) => {
    projectsView.repo = p.name
    projectsView.tab = 'project'
  }
  const editor = $derived(snap?.open_with?.editor ?? null)
  const forgeLink = (p: ProjectSummary) => (p.repo?.project ? forgeUrl(p.repo.project, p.repo.forge) : null)
  function showNotes(name: string) {
    setPref('notes.repo', name)
    openQuickNotes()
  }
  /** Its agents' open ports, each once (an agent can belong to two workstreams: its `adopted`, its `also`). */
  const portsOf = (p: ProjectSummary) => [...new Set(p.workstreams.flatMap((w) => w.agents.filter((a) => a.state === 'running').flatMap((a) => agentPorts(a.id).map((x) => x.port))))]
  const pipeIcon = (status: string | null | undefined) => (status === 'success' ? 'check' : status === 'failed' ? 'fail' : status === 'running' || status === 'pending' ? 'running' : status === 'canceled' ? 'canceled' : 'none')
  const pipeTone = (status: string | null | undefined) => (status === 'success' ? 'ok' : status === 'failed' ? 'ko' : status === 'running' || status === 'pending' ? 'info' : '')

  /** A worktree's removal from its card: two clicks, the second saying what would be lost; forced only when asked. */
  function remove(w: Workstream) {
    const i = w.info
    if (!i || !(i.removable || i.forceable)) return
    if (!armed.confirm(`rm:${w.path}`)) return
    void run(async () => {
      await deleteWorktree(i.w.path, i.forceable)
      toast(t('{name} removed', { name: i.name }))
    })
  }
  function archive(w: Workstream) {
    if (!w.info) return
    setHidden([w.info], true)
    toast(tn(1, '{n} worktree archived', '{n} worktrees archived'))
  }
  const agentHere = (w: Workstream) => openLauncher({ title: w.title, cwd: w.path })
  const agentLine = (a: Agent) => `${cliOf(a.provider).name}${a.model ? ` · ${modelName(a.model)}` : ''}`
  const groupsOf = (p: ProjectSummary) => {
    const live = p.workstreams.filter((w) => w.stage !== 'resume')
    if (sort === 'activity') return [{ stage: null as Stage | null, items: [...live].sort(byActivity) }]
    return (['active', 'review', 'cleanup'] as Stage[]).map((stage) => ({ stage: stage as Stage | null, items: live.filter((w) => w.stage === stage).sort(byActivity) }))
  }
  const toResume = (p: ProjectSummary) => p.workstreams.filter((w) => w.stage === 'resume').sort(byActivity)
</script>

{#snippet signals(p: ProjectSummary)}
  {#if p.waiting}<span class="sig wait" title={tn(p.waiting, '{n} workstream waits for you', '{n} workstreams wait for you')}>{p.waiting}</span>{/if}
  {#if p.mainFailed}<span class="sig ko" title={t('{base} pipeline failed', { base: p.repo?.base ?? 'main' })}><Icon name="fail" size={10} />{p.repo?.base ?? 'main'}</span>{/if}
{/snippet}

{#snippet card(w: Workstream)}
  {@const i = w.info}
  <article class="ws {w.stage}" class:waiting={w.waiting}>
    <header>
      <b class="ttl" title={w.title}>{w.title}</b>
      {#if w.ticketId}<button class="tk" onclick={() => openTicket(w.ticketId!)} title={w.ticket?.status ?? t('Open the ticket')}>{w.ticketId}</button>{/if}
      {#if w.clone}<span class="pill" title={t('In the clone, not a worktree')}>{t('on {branch}', { branch: w.branch ?? '?' })}</span>{/if}
    </header>
    <p class="where mono" title={w.path}>
      {tildePath(w.path)}{#if w.branch && !w.clone}{' '}· {w.branch}{/if}
      {#if w.ahead}<span class="ahead">↑{w.ahead}</span>{/if}{#if w.behind}<span class="behind">↓{w.behind}</span>{/if}
      {#if w.changes}{' '}· {tn(w.changes, '{n} file changed', '{n} files changed')}{:else if w.dirty}{' '}· {t('local changes')}{/if}
    </p>
    {#each w.agents as a (a.id)}
      <div class="agent">
        <span class="st {a.state === 'running' ? a.status : 'gone'}"></span>
        <span class="who">{agentLine(a)}</span>
        <span class="what" class:asks={waitingAgent(a)}>{a.state === 'running' ? (waitingAgent(a) ? (a.needs_action ?? t('Your turn')) : sessionLabel(a)) : a.state === 'prepared' ? t('Prepared') : t('agent stopped')}</span>
        <span class="tm">{ago(a.last_activity ?? a.created_at)}</span>
        <span class="acts">
          <button onclick={() => openAgents(a.id)} title={t('Open it in the Agents view')}>{t('Open')}</button>
          {#if a.state === 'running' && canMarkRead(a) && !a.read}<button onclick={() => run(() => markRead([a.id]))} title={t('Seen: it waits for you no more until its next turn')}>{t('Mark as read')}</button>{/if}
          {#if a.state === 'exited' || a.state === 'gone'}<button onclick={() => run(() => resumeAgent(a.id))} disabled={!agentsAvailable()} title={t('{command}, same folder', { command: cliOf(a.provider).resume })}><Icon name="retry" size={10} />{t('Resume')}</button>{/if}
        </span>
      </div>
    {/each}
    {#if w.mr}
      <div class="mr">
        <button class="key" onclick={() => openMr(w.mr!)} title={w.mr.title}>{mrKey(w.mr)}</button>
        {#if w.mr.state === 'merged'}<span class="pill ok">{tc('state', 'merged')}</span>{:else if w.mr.state === 'opened'}
          {#if w.mr.draft}<span class="pill">{t('draft')}</span>{/if}
          {#if w.mr.pipeline}<span class="pipe {pipeTone(w.mr.pipeline)}" title={t('Pipeline: {status}', { status: w.mr.pipeline })}><Icon name={pipeIcon(w.mr.pipeline)} size={10} />{t('pipeline')}</span>{/if}
          {#if w.mr.open_threads}<span class="pill warn">{tn(w.mr.open_threads, '{n} open thread', '{n} open threads')}</span>{/if}
          {#if w.mr.approved_by.length}<span class="pill ok">{tn(w.mr.approved_by.length, '{n} approval', '{n} approvals')}</span>{/if}
        {:else}<span class="pill">{w.mr.state}</span>{/if}
      </div>
    {/if}
    {#if w.stage === 'cleanup' && i}
      <p class="verdict" class:safe={i.removable}>{i.removable ? t('Nothing to lose') : i.keep ? t('Would lose {what}', { what: i.keep }) : (i.busy ?? '')}{#if i.reasons.length}{' '}· {i.reasons.join(', ')}{/if}</p>
    {/if}
    <footer>
      {#if w.mr}<button onclick={() => openMr(w.mr!)}>{t('View the MR')}</button>{/if}
      {#if w.stage === 'cleanup' && i}
        {#if i.removable || i.forceable}<button class="danger" class:armed={armed.is(`rm:${w.path}`)} onclick={() => remove(w)} disabled={busy} title={i.forceable ? t('Would lose {what}', { what: i.keep ?? '' }) : t('Remove the worktree and its local branch')}><Icon name="trash" size={10} /><ArmedLabel label={t('Remove the worktree')} armedLabel={i.forceable ? t('Lose {what}?', { what: i.keep ?? '' }) : t('Confirm')} armed={armed.is(`rm:${w.path}`)} /></button>{/if}
        <button onclick={() => archive(w)} title={t('Out of the list until a commit or a change')}><Icon name="hide" size={10} />{t('Archive')}</button>
      {/if}
      {#if !w.agents.some((a) => a.state === 'running')}<button onclick={() => agentHere(w)} disabled={!agentsAvailable()} title={t('A new agent in this folder')}><Icon name="plus" size={10} />{t('Agent here')}</button>{/if}
      {#if isDesktop() && editor}<button onclick={() => run(() => openWorktree(w.path, 'editor'))} title={t('Open in {app}', { app: editor })}>{t('Editor')}</button>{/if}
    </footer>
  </article>
{/snippet}

<section class="projects">
  <aside class="list" aria-label={t('Projects')}>
    <div class="lh">
      <b>{t('Projects')}</b>
      <span class="grow"></span>
      <button class="tab" class:on={projectsView.tab === 'cleanup'} onclick={() => (projectsView.tab = projectsView.tab === 'cleanup' ? 'project' : 'cleanup')} title={t('Every project’s worktrees: sizes, verdicts, removal and archiving in bulk')}><Icon name="trash" size={11} />{tc('projects', 'Cleanup')}{#if cleanupCount}<span class="n">{cleanupCount}</span>{/if}</button>
    </div>
    <input class="filter" bind:value={query} placeholder={t('Filter…')} spellcheck="false" aria-label={t('Filter the projects')} />
    {#each folders as folder (folder ?? '')}
      {#if folders.length > 1 || folder}<p class="folder">{folder ?? t('Elsewhere')}</p>{/if}
      {#each shown.filter((p) => p.workspace === folder) as p (p.name)}
        {@const active = p.workstreams.filter((w) => w.stage !== 'resume').length}
        <button class="proj" class:on={projectsView.tab === 'project' && current?.name === p.name} class:resting={p.resting && !p.waiting} onclick={() => pick(p)}>
          <span class="nm">{p.name}</span>
          {@render signals(p)}
          <span class="meta">
            {#if active}{tn(active, '{n} workstream', '{n} workstreams')}{/if}
            {#if p.mrs.length}<span>{tn(p.mrs.length, '{n} MR', '{n} MRs')}</span>{/if}
            {#if p.repo?.behind}<span class="behind">{p.repo.base} ↓{p.repo.behind}</span>{/if}
            {#if !active && !p.mrs.length && p.left}<span>{t('left {ago} ago', { ago: ago(p.left.closed_at) })}</span>{:else if !active && !p.mrs.length}<span>{t('nothing in progress')}</span>{/if}
          </span>
        </button>
      {/each}
    {/each}
    {#if !projects.length}<p class="empty">{t('No repository yet: add one in Settings › Repositories.')}</p>{/if}
  </aside>

  <div class="detail">
    {#if projectsView.tab === 'cleanup' && snap}
      <Worktrees {snap} />
    {:else if current && snap}
      {@const p = current}
      {@const link = forgeLink(p)}
      {@const notes = repoNotes(p.name).length}
      {@const ports = portsOf(p)}
      {@const next = nextOf(p.name)}
      {@const quick = quickActionsHere(p.name)}
      <header class="ph">
        <div class="id">
          <h2>{p.name}</h2>
          <span class="muted">{forgeLabel(p.info?.forge ?? p.repo?.forge)}{#if p.repo?.project}{' '}· {p.repo.project}{/if}</span>
        </div>
        <span class="grow"></span>
        {#if p.repo}
          {#if isDesktop() && editor}<button onclick={() => run(() => openWorktree(p.repo!.path, 'editor'))} title={t('Open in {app}', { app: editor })}>{t('Editor')}</button>{/if}
          {#if isLocal()}
            <button onclick={() => run(() => openWorktree(p.repo!.path, 'terminal'))} title={t('Open in {app}', { app: snap.open_with?.terminal ?? 'Terminal' })}>{t('Terminal')}</button>
            <button onclick={() => run(() => openWorktree(p.repo!.path, 'finder'))} title={keys(t('Show in the Finder'))}>{FILE_MANAGER}</button>
          {/if}
        {/if}
        {#if link}<a class="btn" href={link} title={p.repo?.project}>{forgeLabel(p.repo?.forge)} <Icon name="external" size={10} /></a>{/if}
      </header>
      <div class="sigs">
        {#if p.repo}
          <span class="sig" class:warn={p.repo.behind > 0}><Icon name="branch" size={11} /><code>{p.repo.branch ?? t('detached')}</code>{#if p.repo.behind}{' '}↓{p.repo.behind} {t('to pull')}{/if}{#if p.repo.dirty}{' '}· {t('local changes')}{/if}</span>
        {/if}
        {#if p.mainPipeline}<span class="sig {pipeTone(p.mainPipeline.status)}"><Icon name={pipeIcon(p.mainPipeline.status)} size={11} />{t('{base} pipeline', { base: p.mainPipeline.target })}</span>{/if}
        {#if p.mrs.length}<span class="sig">{tn(p.mrs.length, '{n} open MR', '{n} open MRs')}</span>{/if}
        <button class="sig link" onclick={() => showNotes(p.name)} title={notes ? tn(notes, 'Its {n} open note, and a new one', 'Its {n} open notes, and a new one') : t('A note about it')}><Icon name="pin" size={11} />{notes ? tn(notes, '{n} note', '{n} notes') : t('A note')}</button>
        {#each ports as port (port)}<span class="sig ok">:{port} {t('online')}</span>{/each}
      </div>

      {#if p.repo}{#key p.name}<NewWorkstream repo={p.name} tickets={ticketsFor(snap, p.name, p.workstreams, next)} />{/key}{:else}<p class="muted">{t('No clone on this computer: its MRs and pipelines only.')}</p>{/if}
      {#if next || quick.length}
        <div class="extra">
          {#if next}<span class="muted">{tc('project', 'Next')}</span><button class="tk" onclick={() => openTicket(next.id)} title={next.name}>{next.id}</button><span class="nm">{next.name}</span><button onclick={() => implement(next)} disabled={!agentsAvailable()} title={t('Implement {ticket}', { ticket: next.id })}><Icon name="play" size={10} />{t('Implement')}</button>{/if}
          {#each quick as a, i (i)}<button onclick={(ev) => run(() => runQuickAction(a, p.name, ev.shiftKey))} disabled={!agentsAvailable()} title={`${a.prompt}\n${keys(t('⇧-click: change it before it goes'))}`}><Icon name="quick" size={10} />{a.name}</button>{/each}
        </div>
      {/if}

      {@const groups = groupsOf(p)}
      {@const counts = { active: p.workstreams.filter((w) => w.stage === 'active').length, cleanup: p.workstreams.filter((w) => w.stage === 'cleanup').length }}
      <div class="bar">
        <b>{t('Workstreams')}</b>
        <span class="muted">{t('{n} in progress', { n: counts.active })} · {t('{n} to clean up', { n: counts.cleanup })}</span>
        <span class="grow"></span>
        <span class="seg" role="tablist">
          <button role="tab" class:on={sort === 'stage'} aria-selected={sort === 'stage'} onclick={() => (sort = 'stage')}>{t('By stage')}</button>
          <button role="tab" class:on={sort === 'activity'} aria-selected={sort === 'activity'} onclick={() => (sort = 'activity')}>{t('By activity')}</button>
        </span>
      </div>
      <div class="stages" class:flat={sort === 'activity'}>
        {#each groups as g (g.stage ?? 'all')}
          <div class="stage">
            {#if g.stage}<h3>{STAGE_LABEL[g.stage]} <span>{g.items.length}</span></h3>{/if}
            {#each g.items as w (w.key)}{@render card(w)}{/each}
            {#if !g.items.length}<p class="none">{g.stage === 'active' ? t('No agent at work.') : g.stage === 'review' ? t('No MR open.') : g.stage === 'cleanup' ? t('Nothing to clean up.') : t('Nothing in progress.')}</p>{/if}
          </div>
        {/each}
      </div>

      {@const resume = toResume(p)}
      {#if resume.length || p.left}
        <h3 class="sec">{t('To pick up')}</h3>
        <div class="resume">
          {#if p.left}
            <div class="row">
              <span>{p.left.title} · {t('agent closed {ago} ago', { ago: ago(p.left.closed_at) })}{#if p.left.branch}{' '}· <code>{p.left.branch}</code>{/if}</span>
              {#if p.left.said}<q>{p.left.said}</q>{/if}
              {#if p.left.resumable}<button onclick={() => run(() => reopenLeft(p.name))} disabled={!agentsAvailable()} title={t('Back in the list, on its conversation ({cli})', { cli: cliOf(p.left.provider).name })}><Icon name="retry" size={10} />{t('Reopen')}</button>{/if}
            </div>
          {/if}
          {#each resume as w (w.key)}{@render card(w)}{/each}
        </div>
      {/if}

      {#if p.repo}
        <h3 class="sec">{t('The repository')}</h3>
        <div class="repo">
          <div class="row">
            <span>{t('Clone')} <code>{tildePath(p.repo.path)}</code> {t('on {branch}', { branch: p.repo.branch ?? '?' })}{p.repo.dirty ? ` · ${t('local changes')}` : ` · ${t('clean')}`}</span>
            <button onclick={() => run(async () => toast(await pullWorktree(p.repo!.path)))} disabled={busy || !p.repo.behind} title={t('Fast-forward only')}><Icon name="down" size={10} />{p.repo.behind ? t('Pull ↓{n}', { n: p.repo.behind }) : t('Up to date')}</button>
          </div>
          <div class="row">
            <span class="muted">{t('Worktrees, MRs, pipelines, merge queue')}</span>
            <button onclick={() => openSettings('repos')}><Icon name="settings" size={10} />{t('Settings')}</button>
          </div>
        </div>
      {/if}
    {/if}
  </div>
</section>

<style>
  .projects { flex: 1; display: flex; min-height: 0; overflow: hidden; }
  .list { flex: 0 0 250px; display: flex; flex-direction: column; gap: 2px; padding: 10px 8px; border-right: 1px solid var(--border); overflow-y: auto; background: var(--chrome-bg); }
  .lh { display: flex; align-items: center; gap: 6px; padding: 0 4px 6px; }
  .grow { flex: 1; }
  .tab { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .tab.on { border-color: var(--accent); color: var(--accent); }
  .tab .n { padding: 0 4px; border-radius: var(--r7); background: var(--warn-bg); color: var(--warn); }
  .filter { margin: 0 2px 6px; padding: 4px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; }
  .folder { margin: 8px 6px 2px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .proj { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; padding: 6px 8px; border: none; border-radius: var(--r6); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .proj:hover { background: var(--surface-muted); }
  .proj.on { background: color-mix(in srgb, var(--accent) 14%, transparent); }
  .proj.resting { opacity: 0.55; }
  .proj .nm { font-weight: 600; }
  .proj .meta { flex-basis: 100%; display: flex; flex-wrap: wrap; gap: 6px; color: var(--text-muted); font-size: var(--fs-tag); }
  .sig { display: inline-flex; align-items: center; gap: 4px; }
  .sig.wait { min-width: 16px; padding: 0 5px; border-radius: var(--r7); background: var(--src-claude); color: var(--on-accent); font-size: var(--fs-tag); font-weight: 600; justify-content: center; }
  .sig.ko, .pipe.ko { color: var(--ko); }
  .sig.ok, .pipe.ok { color: var(--ok); }
  .sig.info, .pipe.info { color: var(--info); }
  .sig.warn, .behind { color: var(--warn); }
  .ahead { color: var(--ok); margin-left: 4px; }
  .behind { margin-left: 4px; }
  .empty, .none { color: var(--text-muted); font-size: var(--fs-tag); margin: 6px; }

  .detail { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 10px; padding: 14px 18px 24px; overflow-y: auto; }
  .detail > :global(.panel) { flex: 1; }
  .ph { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .ph h2 { margin: 0; font-size: 18px; }
  .id { display: flex; align-items: baseline; gap: 8px; }
  .muted { color: var(--text-muted); font-size: var(--fs-tag); }
  .sigs { display: flex; flex-wrap: wrap; gap: 6px 14px; font-size: var(--fs-tag); color: var(--text-muted); }
  .sig.link { border: none; background: none; padding: 0; color: inherit; font: inherit; cursor: pointer; }
  .sig.link:hover { color: var(--text); }
  button, .btn { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; cursor: pointer; text-decoration: none; }
  button:hover:not(:disabled), .btn:hover { background: var(--surface-muted); }
  button:disabled { opacity: 0.5; cursor: default; }
  .proj, .sig.link { line-height: inherit; }
  .extra { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; font-size: var(--fs-tag); }
  .extra .nm { color: var(--text-muted); max-width: 300px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tk { font-family: var(--mono); border-color: color-mix(in srgb, var(--src-clickup) 50%, var(--border)); color: var(--src-clickup); }
  .bar { display: flex; align-items: center; gap: 8px; margin-top: 6px; }
  .seg { display: inline-flex; }
  .seg button { border-radius: 0; }
  .seg button:first-child { border-radius: var(--r5) 0 0 var(--r5); }
  .seg button:last-child { border-radius: 0 var(--r5) var(--r5) 0; border-left: none; }
  .seg button.on { background: color-mix(in srgb, var(--accent) 14%, var(--surface)); color: var(--accent); }
  .stages { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 12px; align-items: start; }
  .stages.flat { grid-template-columns: 1fr; }
  .stages.flat .stage { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 8px; }
  .stage { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  h3 { margin: 0; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); font-weight: 600; }
  h3 span { margin-left: 4px; color: var(--text); }
  .sec { margin-top: 8px; }
  .ws { display: flex; flex-direction: column; gap: 5px; padding: 8px 10px; border: 1px solid var(--border); border-left: 3px solid var(--border); border-radius: var(--r8); background: var(--surface); font-size: var(--fs-body); }
  .ws.active { border-left-color: var(--src-claude); }
  .ws.review { border-left-color: var(--info); }
  .ws.cleanup { border-left-color: var(--ok); }
  .ws.waiting { box-shadow: 0 0 0 1px color-mix(in srgb, var(--src-claude) 50%, transparent); }
  .ws header { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .ttl { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pill { padding: 0 6px; border-radius: var(--r-chip); background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .pill.ok { background: var(--ok-bg); color: var(--ok); }
  .pill.warn { background: var(--warn-bg); color: var(--warn); }
  .where { margin: 0; color: var(--text-muted); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mono { font-family: var(--mono); }
  .agent { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; font-size: var(--fs-tag); }
  .agent .st { width: 7px; height: 7px; border-radius: var(--r-dot); background: var(--text-muted); }
  .agent .st.needs_you, .agent .st.waiting_tool, .agent .st.review_ready { background: var(--src-claude); }
  .agent .st.running, .agent .st.background { background: var(--info); }
  .agent .st.gone { background: var(--border); }
  .agent .who { font-weight: 600; }
  .agent .what { color: var(--text-muted); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 260px; }
  .agent .what.asks { color: var(--src-claude); font-weight: 600; }
  .agent .tm { color: var(--text-muted); }
  .agent .acts { margin-left: auto; display: inline-flex; gap: 4px; }
  .mr { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; font-size: var(--fs-tag); }
  .mr .key { font-family: var(--mono); }
  .pipe { display: inline-flex; align-items: center; gap: 3px; }
  .verdict { margin: 0; font-size: var(--fs-tag); color: var(--warn); }
  .verdict.safe { color: var(--ok); }
  .ws footer { display: flex; flex-wrap: wrap; gap: 4px; }
  .danger { color: var(--ko); }
  .danger.armed { background: var(--ko-bg); border-color: var(--ko); }
  .resume, .repo { display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px; font-size: var(--fs-tag); }
  .row q { flex-basis: 100%; color: var(--text-muted); font-style: italic; }
  code { font-family: var(--mono); font-size: var(--fs-tag); }
  @media (max-width: 760px) {
    .projects { flex-direction: column; overflow-y: auto; }
    .list { flex: 0 0 auto; max-height: 40vh; border-right: none; border-bottom: 1px solid var(--border); }
    .detail { overflow: visible; }
  }
</style>
