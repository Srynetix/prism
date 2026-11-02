<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from './Icon.svelte'
  import Logo from './Logo.svelte'
  import { BLOCK_HELP } from '../block-help'
  import { STARTER_PACK, columnsOfConfig, templates, type Template } from '../templates'
  import { BLOCK_LABELS, viewColor, views, type BlockId } from '../views'
  import { forgetViews, setView } from '../layout.svelte'
  import { cfg, closeSettings, fileValue, importPack, loadConfig, mergedRows, openSettings, packBusy, saveEdits, valueOf } from '../settings.svelte'
  import TokenField from './settings/TokenField.svelte'
  import ForgeLogin from './settings/ForgeLogin.svelte'
  import { TOKEN_HINTS, TOKEN_PAGES, tokenCheck } from '../credentials'
  import { live } from '../live.svelte'
  import { closeWizard, detectSetup } from '../wizard.svelte'
  import { CLIS } from '../agent-cli'
  import { createProfile, loadProfiles, openProfile, profiles } from '../profiles.svelte'
  import { openLink } from '../links'
  import { missingFor, pickAgents, pickForge, pickRepos, pickTracker, planPages, repoRows, workspaceDirs, type ForgeChoice, type Missing, type SetupChoices, type TrackerChoice } from '../setup-plan'
  import type { SetupDetection } from '../bindings/SetupDetection'
  import type { AgentProvider } from '../types'
  import { keys } from '../platform'
  import { t, tn, tc } from '../i18n.svelte'

  /**
   * Pages wizard, in five steps: Forge → Agents → Repositories → Tickets → Your page. Each starts
   * from what the machine has (`detect_setup`: glab/gh signed in, agent CLIs, repositories of the
   * usual folders, kubectl, trackers), every one skippable: a skipped step keeps what the config
   * says. The last one picks a template of the packs (Prism's starter pack without one) and shows
   * its pages without the blocks whose source is missing, so that none is empty and in error on the
   * first day; a strip under the other steps shows them as they come. "Start" writes `[[views]]`
   * and the steps' settings to config.toml; "Open the demo" opens the demo profile next to it.
   */
  const STEPS = ['forge', 'agents', 'repos', 'tickets', 'page'] as const
  type Step = (typeof STEPS)[number]
  const STEP_LABELS: Record<Step, string> = { forge: t('Forge'), agents: t('Agents'), repos: t('Repositories'), tickets: t('Tickets'), page: t('Your page') }
  let step = $state<Step>('forge')
  const at = $derived(STEPS.indexOf(step))
  /** Steps passed over: they write nothing, the config keeps its own. */
  let skipped = $state<Step[]>([])
  const go = (s: Step) => (step = s)
  const next = (skip = false) => {
    skipped = skip ? [...new Set([...skipped, step])] : skipped.filter((s) => s !== step)
    step = STEPS[Math.min(at + 1, STEPS.length - 1)]
  }

  const FORGES = { gitlab: 'GitLab', github: 'GitHub' } as const
  const FORGE_IDS = ['gitlab', 'github'] as const
  const TRACKER_IDS = ['clickup', 'jira', 'obsidian', 'none'] as const
  const AGENT_IDS = Object.keys(CLIS) as AgentProvider[]
  const FORGE_CLI = { gitlab: 'glab', github: 'gh' } as const
  const TRACKERS: Record<TrackerChoice, string> = { clickup: 'ClickUp', jira: 'Jira', obsidian: 'Obsidian', none: t('No tracker') }
  const AGENT_LOGO = { claude: 'claude', 'command-code': 'commandcode', opencode: 'opencode', codex: 'codex' } as const
  /** Where to get what is missing. */
  const INSTALL: Record<string, string> = {
    gitlab: 'https://gitlab.com/gitlab-org/cli', github: 'https://cli.github.com',
    claude: 'https://docs.claude.com/en/docs/claude-code/setup', 'command-code': 'https://commandcode.ai', opencode: 'https://opencode.ai', codex: 'https://github.com/openai/codex',
  }

  /* ---------- What the config says today: a skipped step keeps it ---------- */
  const nowForge: ForgeChoice = live.snapshot?.gitlab === false ? 'none' : (live.snapshot?.forge ?? 'gitlab')
  const nowProvider: AgentProvider = live.snapshot?.agents_settings?.provider ?? 'claude'
  const nowAgents: AgentProvider[] = live.snapshot?.agents_settings?.enabled === false ? [] : [nowProvider]
  const nowTracker: TrackerChoice = live.snapshot?.tracker === undefined ? 'clickup' : (live.snapshot.tracker ?? 'none')
  /** Opened again from Settings, over pages: the steps start from the config, not the machine. */
  const configured = !!live.snapshot?.views?.length

  /* ---------- What the machine has ---------- */
  let found = $state<SetupDetection | null>(null)
  let detecting = $state(false)
  let forge = $state<ForgeChoice>(nowForge)
  let agents = $state<AgentProvider[]>(nowAgents)
  let ticked = $state<string[]>([])
  let tracker = $state<TrackerChoice>(nowTracker)
  let vault = $state('')
  let calendar = $state(live.snapshot?.calendar !== false)

  /** The repositories the config has already: their paths, not offered again. */
  const ownRows = () => (cfg.file ? mergedRows(['repos']) : [])
  const knownPaths = () => ownRows().map((r) => String(r.path ?? ''))

  async function detect(prefill: boolean) {
    detecting = true
    try {
      const d = await detectSetup()
      found = d
      if (!prefill) return
      if (!configured) {
        forge = pickForge(d)
        agents = pickAgents(d)
        tracker = pickTracker(d)
      }
      ticked = pickRepos(d, forge, knownPaths())
      vault = d.trackers.obsidian_vaults[0] ?? ''
    } catch (e) {
      error = String(e)
    } finally {
      detecting = false
    }
  }
  onMount(() => {
    void (async () => {
      if (!cfg.file) await loadConfig()
      await detect(true)
    })()
  })

  const forgeFound = (p: 'gitlab' | 'github') => found?.forges.find((f) => f.provider === p)
  const login = $derived(forge === 'none' ? null : (forgeFound(forge)?.login ?? null))
  const agentFound = (p: AgentProvider) => found?.agents.find((a) => a.provider === p)
  function toggleAgent(p: AgentProvider) {
    agents = agents.includes(p) ? agents.filter((a) => a !== p) : [...agents, p]
  }
  function toggleRepo(path: string) {
    ticked = ticked.includes(path) ? ticked.filter((p) => p !== path) : [...ticked, path]
  }
  const repos = $derived((found?.repos ?? []).filter((r) => !knownPaths().includes(r.path)))
  const tickedRepos = $derived(repos.filter((r) => ticked.includes(r.path)))
  // Another forge picked: its repositories ticked instead.
  let lastForge: ForgeChoice = nowForge
  $effect(() => {
    if (forge === lastForge || !found) return
    lastForge = forge
    ticked = pickRepos(found, forge, knownPaths())
  })

  /* ---------- The choices, a skipped step's being the config's ---------- */
  const kube = $derived(!!found?.kube_contexts.length)
  const choices = $derived<SetupChoices>({
    forge: skipped.includes('forge') ? nowForge : forge,
    // The config's forge, kept: as signed in as it is today.
    signedIn: skipped.includes('forge') ? nowForge !== 'none' : !!login,
    agents: skipped.includes('agents') ? nowAgents : agents,
    tmux: !found || !!found.tmux,
    repos: ownRows().length + (skipped.includes('repos') ? 0 : tickedRepos.length),
    tracker: skipped.includes('tickets') ? nowTracker : tracker,
    calendar,
    kube,
  })

  let picked = $state<string | null>(null)
  const template = $derived<Template | null>(templates().find((x) => x.id === picked) ?? templates()[0] ?? null)
  const plan = $derived(planPages(template?.views ?? [], choices))
  const pages = $derived(plan.pages)
  const current = $derived(live.snapshot?.views?.length ? views() : null)

  /** Why a block is left out, in words. */
  function why(m: Missing): string {
    switch (m) {
      case 'no-forge': return t('no forge')
      case 'not-signed-in': return t('{cli} not signed in', { cli: choices.forge === 'none' ? 'glab' : FORGE_CLI[choices.forge] })
      case 'no-repo': return t('no repository')
      case 'no-agent': return t('no agent CLI')
      case 'no-tmux': return t('tmux missing')
      case 'no-tracker': return t('no tracker')
      case 'no-calendar': return t('Calendar off')
      case 'no-kube': return found?.kubectl ? t('no kubectl context') : t('kubectl missing')
    }
  }
  const label = (b: string) => (b in BLOCK_LABELS ? t(BLOCK_LABELS[b as BlockId]) : b)
  /** A kept block's source, said under its chip: the forge's CLI and repositories, the agent… */
  function source(b: string): string {
    if (b === 'mrs' && choices.forge !== 'none') return `${FORGE_CLI[choices.forge]} · ${tn(choices.repos, '{n} repo', '{n} repos')}`
    if (b === 'sessions' && choices.agents.length) return CLIS[choices.agents[0]].cli
    if (b === 'worktrees') return tn(choices.repos, '{n} repo', '{n} repos')
    if (['sprint', 'refinement', 'pickup', 'retro'].includes(b) && choices.tracker !== 'none') return TRACKERS[choices.tracker]
    return ''
  }

  /* ---------- Start ---------- */
  /** The tracker's settings: its provider when it changes, the ClickUp switch when it is ClickUp or none. */
  function trackerEdits() {
    const edits: { path: string[]; value: unknown }[] = []
    if (tracker !== nowTracker) {
      const provider = tracker === 'none' ? 'clickup' : tracker
      if (provider !== (nowTracker === 'none' ? 'clickup' : nowTracker)) edits.push({ path: ['tracker', 'provider'], value: provider })
      if (provider === 'clickup') edits.push({ path: ['clickup', 'enabled'], value: tracker === 'clickup' })
    }
    if (tracker === 'obsidian' && vault && vault !== valueOf<string>(['obsidian', 'vault'], '')) edits.push({ path: ['obsidian', 'vault'], value: vault })
    return edits
  }
  function forgeEdits() {
    if (skipped.includes('forge')) return []
    const edits: { path: string[]; value: unknown }[] = [{ path: ['forge', 'enabled'], value: forge !== 'none' }]
    if (forge !== 'none' && forge !== (live.snapshot?.forge ?? 'gitlab')) edits.push({ path: ['forge', 'provider'], value: forge })
    // A self-hosted instance the repositories point to, signed in there.
    const host = forgeFound('gitlab')?.host
    if (forge === 'gitlab' && host && host !== valueOf<string>(['gitlab', 'host'], 'gitlab.com')) edits.push({ path: ['gitlab', 'host'], value: host })
    return edits
  }
  function agentEdits() {
    if (skipped.includes('agents')) return []
    return [{ path: ['agents', 'enabled'], value: agents.length > 0 }, ...(agents.length && agents[0] !== nowProvider ? [{ path: ['agents', 'provider'], value: agents[0] }] : [])]
  }
  function repoEdits() {
    if (skipped.includes('repos') || !tickedRepos.length) return []
    const own = (fileValue(['repos']) as Record<string, unknown>[] | undefined) ?? []
    const rows = repoRows(tickedRepos, forge, ownRows().map((r) => String(r.name ?? '')))
    const workspaces = (valueOf<unknown[] | null>(['workspaces']) ?? []).length ? [] : [{ path: ['workspaces'], value: workspaceDirs(tickedRepos).map((dir) => ({ dir })) }]
    return [{ path: ['repos'], value: [...own, ...rows] }, ...workspaces]
  }

  let busy = $state(false)
  let error = $state('')
  async function start() {
    if (!template) return
    busy = true
    error = ''
    try {
      await saveEdits([
        { path: ['views'], value: pages.map((v) => JSON.parse(JSON.stringify(v))) },
        ...forgeEdits(),
        ...agentEdits(),
        ...repoEdits(),
        ...(skipped.includes('tickets') ? [] : trackerEdits()),
        // Clusters only when the template reads them and kubectl has a context.
        { path: ['clusters', 'enabled'], value: template.cluster && kube },
        { path: ['calendar', 'enabled'], value: calendar },
        { path: ['data', 'enabled'], value: template.data },
      ])
      // The template's pages start again from their blocks, not from a previous layout.
      forgetViews(pages.map((v) => v.id!).filter(Boolean))
      closeSettings()
      if (pages[0]?.id) setView(pages[0].id)
      closeWizard()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  /** No template in the packs: Prism's starter pack brings the Developer one. */
  async function importStarter() {
    error = ''
    if (!(await importPack(STARTER_PACK))) error = cfg.error || t('The starter pack could not be imported.')
  }

  /** The demo profile, in its own Prism next to this one: created the first time. */
  let demoBusy = $state(false)
  async function openDemo() {
    demoBusy = true
    error = ''
    try {
      await loadProfiles()
      const demo = profiles.list.find((p) => p.id === 'demo') ?? (await createProfile(t('Demo'), 'purple', null, true))
      await openProfile(demo)
    } catch (e) {
      error = String(e)
    } finally {
      demoBusy = false
    }
  }
</script>

{#snippet verdict(text: string, ok: boolean)}<span class={ok ? 'ok' : 'no'}>{#if ok}<i class="led"></i>{/if}{text}</span>{/snippet}

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => e.target === e.currentTarget && closeWizard()}>
  <div class="sheet" role="dialog" aria-label={t('Set up your pages')}>
    <header>
      <div>
        <b>{t('Set up your pages')}</b>
        <p>{t('Prism starts from what this machine already has. Every step can be skipped; Settings changes everything afterwards.')}</p>
      </div>
      {#if detecting}<span class="muted small">{t('Looking at this machine…')}</span>{:else if found}<button class="ghost" onclick={() => detect(false)} title={t('Look again: after signing in, installing a CLI…')}><Icon name="retry" />{t('Check again')}</button>{/if}
      <button class="x" onclick={closeWizard} aria-label={t('Close')}><Icon name="close" /></button>
    </header>

    <nav class="steps">
      {#each STEPS as s, i (s)}
        <button class:done={i < at} class:now={s === step} class:skip={skipped.includes(s)} onclick={() => go(s)}>{STEP_LABELS[s]}</button>
      {/each}
    </nav>

    <div class="body">
      {#if step === 'forge'}
        <h3>{t('Where is your code?')}</h3>
        <p class="sub">{t('Merge requests, CI and repositories come from your forge, through its own CLI: Prism never sees a token.')}</p>
        {#each FORGE_IDS as p (p)}
          {@const f = forgeFound(p)}
          <button class="found" class:on={forge === p} role="radio" aria-checked={forge === p} onclick={() => (forge = p)}>
            <span class="radio" class:on={forge === p}></span>
            <span class="ic"><Logo name={p} size={16} /></span>
            <span class="lb"><b>{FORGES[p]}</b>
              <small>{#if !found}{t('checking…')}{:else if !f?.installed}{t('{cli} is not installed', { cli: FORGE_CLI[p] })}{:else if f.login}{t('{cli} on {host}', { cli: FORGE_CLI[p], host: f.host })}{:else}{t('{cli} installed, not signed in on {host}', { cli: FORGE_CLI[p], host: f.host })}{/if}</small>
            </span>
            {#if f?.login}{@render verdict(t('signed in as {login}', { login: f.login }), true)}
            {:else if found && !f?.installed}<span class="link" role="link" tabindex="-1" onclick={(e) => { e.stopPropagation(); openLink(INSTALL[p]) }}>{t('How to install it')} ↗</span>{/if}
          </button>
          {#if forge === p && f?.installed && !f.login}<div class="inline"><ForgeLogin forge={p} host={f.host} /></div>{/if}
        {/each}
        <button class="found" class:on={forge === 'none'} role="radio" aria-checked={forge === 'none'} onclick={() => (forge = 'none')}>
          <span class="radio" class:on={forge === 'none'}></span>
          <span class="ic"><Icon name="close" /></span>
          <span class="lb"><b>{t('No forge for now')}</b><small>{t('No merge request nor CI blocks; worktrees and agents still work on local clones.')}</small></span>
        </button>
      {:else if step === 'agents'}
        <h3>{t('Which agents do you use?')}</h3>
        <p class="sub">{t('Found on this machine. The first one ticked is the default; a repository can run another.')}</p>
        {#each AGENT_IDS as p (p)}
          {@const a = agentFound(p)}
          <button class="found" class:on={agents.includes(p)} role="checkbox" aria-checked={agents.includes(p)} onclick={() => toggleAgent(p)}>
            <span class="box" class:on={agents.includes(p)}>{#if agents.includes(p)}<Icon name="check" />{/if}</span>
            <span class="ic"><Logo name={AGENT_LOGO[p]} size={16} /></span>
            <span class="lb"><b>{CLIS[p].cli}{#if agents[0] === p}<span class="pill">{t('default')}</span>{/if}</b>
              <small class="mono">{#if !found}{t('checking…')}{:else if a?.path}{a.path}{#if a.version}{' '}· {a.version}{/if}{:else}{t('not installed')}{/if}</small>
            </span>
            {#if a?.path}{@render verdict(t('found'), true)}
            {:else if found}<span class="link" role="link" tabindex="-1" onclick={(e) => { e.stopPropagation(); openLink(INSTALL[p]) }}>{t('How to install it')} ↗</span>{/if}
          </button>
        {/each}
        {#if found && !found.tmux}<p class="warn"><Icon name="warning" />{t('tmux is missing: the agents run in it. Install it, then Check again.')}</p>{/if}
      {:else if step === 'repos'}
        <h3>{t('Which repositories do you work on?')}</h3>
        <p class="sub">{#if found?.folders.length}{t('Found in {folders}. Their merge requests are read and their worktrees listed.', { folders: found.folders.join(', ') })}{:else}{t('Their merge requests are read and their worktrees listed.')}{/if}</p>
        {#if ownRows().length}<p class="muted small">{tn(ownRows().length, 'The config has {n} repository already: it stays.', 'The config has {n} repositories already: they stay.')}</p>{/if}
        <div class="list">
          {#each repos as r (r.path)}
            {@const other = forge !== 'none' && r.forge && r.forge !== forge}
            <button class="found row" class:on={ticked.includes(r.path)} role="checkbox" aria-checked={ticked.includes(r.path)} onclick={() => toggleRepo(r.path)}>
              <span class="box" class:on={ticked.includes(r.path)}>{#if ticked.includes(r.path)}<Icon name="check" />{/if}</span>
              <span class="lb"><b>{r.name}</b><small class="mono">{r.path}{#if r.project}{' '}· {r.project}{/if}</small></span>
              {#if r.forge}<span class="tag" class:other title={other ? t('On another forge: only its merge requests are read, with that forge’s own login') : ''}><Logo name={r.forge} size={11} />{FORGES[r.forge]}</span>{:else}<span class="tag">{t('local')}</span>{/if}
            </button>
          {:else}
            <p class="muted">{found ? t('No git repository in the usual folders (~/Development, ~/src, ~/code…). Add them later in Settings › Code › Repositories.') : t('checking…')}</p>
          {/each}
        </div>
      {:else if step === 'tickets'}
        <h3>{t('Where are your tickets?')}</h3>
        <p class="sub">{t('Sprint, refinement and tickets to pick come from your tracker.')}</p>
        {#each TRACKER_IDS as k (k)}
          <button class="found" class:on={tracker === k} role="radio" aria-checked={tracker === k} onclick={() => (tracker = k)}>
            <span class="radio" class:on={tracker === k}></span>
            <span class="ic">{#if k === 'none'}<Icon name="mr" />{:else}<Logo name={k} size={16} />{/if}</span>
            <span class="lb"><b>{TRACKERS[k]}</b>
              <small>
                {#if k === 'clickup'}{found?.trackers.clickup_env ? t('CLICKUP_TOKEN is set') : t('An API token, kept in the system keychain.')}
                {:else if k === 'jira'}{found?.trackers.jira_env ? t('JIRA_TOKEN is set') : t('Jira Cloud: a site, an account and an API token.')}
                {:else if k === 'obsidian'}{found?.trackers.obsidian_vaults.length ? tn(found.trackers.obsidian_vaults.length, '{n} vault found', '{n} vaults found') : t('A vault with Kanban boards; no token.')}
                {:else}{t('The tickets named in your merge request titles and branches, and nothing else.')}{/if}
              </small>
            </span>
            {#if (k === 'clickup' && found?.trackers.clickup_env) || (k === 'jira' && found?.trackers.jira_env) || (k === 'obsidian' && found?.trackers.obsidian_vaults.length)}{@render verdict(t('found'), true)}{/if}
          </button>
          {#if tracker === k && k === 'clickup' && !found?.trackers.clickup_env}
            <div class="inline"><TokenField tracker="clickup" label={t('ClickUp token')} hint={TOKEN_HINTS.clickup} create={TOKEN_PAGES.clickup} check={tokenCheck('clickup')} /></div>
          {:else if tracker === k && k === 'jira'}
            <div class="inline"><span class="muted">{t('Name the site and the account in Settings › Work › Tracker › Jira, then paste the token there.')}</span><button onclick={() => { closeWizard(); openSettings('jira') }}>{t('Settings')} › Jira</button></div>
          {:else if tracker === k && k === 'obsidian'}
            <div class="inline">
              {#if found?.trackers.obsidian_vaults.length}
                <label class="sel"><span>{t('Vault')}</span><select bind:value={vault}>{#each found.trackers.obsidian_vaults as v (v)}<option value={v}>{v}</option>{/each}</select></label>
              {/if}
              <span class="muted">{t('Its boards are named in Settings › Work › Tracker › Obsidian.')}</span>
            </div>
          {/if}
        {/each}
      {:else}
        <div class="split">
          <div class="templates" role="radiogroup" aria-label={t('Template')}>
            {#each templates() as p (p.id)}
              <button role="radio" aria-checked={template?.id === p.id} class:on={template?.id === p.id} onclick={() => (picked = p.id)}>
                <b>{p.label}</b>
                <span>{p.summary}</span>
                <span class="tags"><span class="pill">{tn(p.views.length, '{n} page', '{n} pages')}</span>{#if p.cluster}<span class="pill k8s">{t('clusters')}</span>{/if}{#if p.data}<span class="pill k8s">GCP</span>{/if}</span>
              </button>
            {:else}
              <div class="none">
                <b>{t('No template yet')}</b>
                <span>{t('Templates come from the packs: your team’s brings those of its roles (Settings › General › Packs). Prism’s starter pack brings the Developer one: tickets, merge requests, worktrees and agents.')}</span>
                <button class="primary" onclick={importStarter} disabled={!!packBusy.name}>{packBusy.name ? t('Importing…') : t('Import the starter pack')}</button>
              </div>
            {/each}
            <label class="opt">
              <input type="checkbox" bind:checked={calendar} />
              <span><b>{t('Read my macOS Calendar (macOS only)')}</b><small>{t('Today block, meeting countdown and reminders. macOS asks for access once. Off: never read.')}</small></span>
            </label>
          </div>
          <div class="preview">
            <p class="cap">{t('Pages created')}</p>
            {#each pages as v, i (v.id ?? v.name)}
              <div class="page" style:--tint={viewColor(v.color) ?? 'var(--accent)'}>
                <div class="ph"><span class="dot"></span><b>{v.name}</b><span class="muted">{keys(`⌘${i + 1}`)}</span></div>
                <div class="grid">
                  {#each columnsOfConfig(v) as col, c (c)}
                    <div class="col">
                      {#each col as b (b)}<span class="blk" title={BLOCK_HELP[b]?.purpose}>{t(BLOCK_LABELS[b])}</span>{/each}
                    </div>
                  {/each}
                </div>
              </div>
            {:else}
              <p class="muted">{template ? t('No page: every block of this template misses its source. Go back a step, or start from an empty Dashboard.') : t('Import a pack to see its pages here, or skip: one empty Dashboard, to fill with “+ Block”.')}</p>
            {/each}
            {#if plan.blocks.some((b) => b.missing)}
              <p class="cap">{t('Left out')}</p>
              <div class="mini">{#each plan.blocks.filter((b) => b.missing) as b (b.id)}<span class="off">{label(b.id)}<small>{why(b.missing!)}</small></span>{/each}</div>
              <p class="muted small">{t('A block whose source is missing is not added: no empty block in error on the first day. “+ Block” adds it once its source is set.')}</p>
            {/if}
          </div>
        </div>
      {/if}
    </div>

    {#if step !== 'page' && plan.blocks.length}
      <div class="strip">
        <span class="muted"><b>{t('Your first page')}</b>{t(', from what is found so far:')}</span>
        <div class="mini">
          {#each plan.blocks as b (b.id)}
            <span class:off={!!b.missing} title={b.missing ? why(b.missing) : (BLOCK_HELP[b.id as BlockId]?.purpose ?? '')}>{label(b.id)}<small>{b.missing ? why(b.missing) : source(b.id)}</small></span>
          {/each}
        </div>
      </div>
    {/if}

    <footer>
      {#if step === 'page'}
        <span class="muted">
          {#if current}{tn(current.length, 'Replaces your page ({names}); the layout of this page starts over.', 'Replaces your {n} pages ({names}); the layout of these pages starts over.', { names: current.map((v) => v.label).join(', ') })}{:else}{t('Everything changes later in Settings.')}{/if}
        </span>
      {:else}
        <span class="muted">{t('Everything changes later in Settings.')}</span>
      {/if}
      {#if error}<span class="err">{error}</span>{/if}
      <span class="grow"></span>
      {#if at > 0}<button class="ghost" onclick={() => go(STEPS[at - 1])}>{t('Back')}</button>{/if}
      {#if step === 'page'}
        <button onclick={openDemo} disabled={demoBusy} title={t('Prism filled with a fictitious team’s work, in its own window: nothing read, nothing written here')}>{demoBusy ? t('Opening…') : t('Open the demo')}</button>
        <button class="primary" onclick={start} disabled={busy || !pages.length}>{busy ? t('Applying…') : tc('wizard', 'Start')}</button>
      {:else}
        <button onclick={() => next(true)} title={t('Keep what the config says for this step')}>{t('Skip')}</button>
        <button class="primary" onclick={() => next()}>{t('Next')}</button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 60; display: flex; align-items: flex-start; justify-content: center; padding-top: 7vh; background: rgba(0, 0, 0, 0.5); }
  .sheet { width: min(860px, 94vw); max-height: 86vh; display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--r14); background: var(--surface); box-shadow: var(--shadow-modal); overflow: hidden; }
  header { display: flex; align-items: flex-start; gap: 12px; padding: 16px 18px 10px; }
  header b { font-size: 15px; }
  header p { margin: 4px 0 0; color: var(--text-muted); font-size: var(--fs-meta); }
  header div { flex: 1; }
  .x { padding: 4px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .steps { display: flex; gap: 6px; padding: 0 18px 12px; border-bottom: 1px solid var(--border); }
  .steps button { flex: 1; justify-content: flex-start; padding: 6px 0 0; border: none; border-top: 3px solid var(--border); border-radius: 0; background: none; color: var(--text-muted); font-size: var(--fs-meta); }
  .steps button.done { border-top-color: var(--ok); color: var(--text); }
  .steps button.skip { border-top-style: dashed; color: var(--text-muted); }
  .steps button.now { border-top-color: var(--accent); color: var(--text); font-weight: 600; }
  .body { flex: 1; min-height: 0; overflow: auto; padding: 16px 20px; }
  h3 { margin: 0 0 4px; font-size: 16px; }
  .sub { margin: 0 0 12px; color: var(--text-muted); font-size: var(--fs-meta); }
  .found { display: flex; align-items: center; gap: 12px; width: 100%; margin-top: 8px; padding: 9px 12px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--bg); color: var(--text); font: inherit; text-align: left; white-space: normal; cursor: pointer; }
  .found.row { margin-top: 4px; padding: 6px 10px; }
  .found:hover { border-color: color-mix(in srgb, var(--accent) 45%, var(--border)); }
  .found.on { border-color: color-mix(in srgb, var(--accent) 65%, var(--border)); }
  .found .ic { width: 28px; height: 28px; flex-shrink: 0; display: grid; place-items: center; border-radius: var(--r7); background: var(--surface); }
  .found .lb { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; font-size: var(--fs-body); }
  .found .lb b { display: flex; align-items: center; gap: 6px; }
  .found .lb small { color: var(--text-muted); font-size: var(--fs-meta); overflow: hidden; text-overflow: ellipsis; }
  .radio { width: 16px; height: 16px; flex-shrink: 0; border: 2px solid var(--border); border-radius: 50%; }
  .radio.on { border-color: var(--accent); box-shadow: inset 0 0 0 3px var(--bg), inset 0 0 0 8px var(--accent); }
  .box { width: 16px; height: 16px; flex-shrink: 0; display: grid; place-items: center; border: 2px solid var(--border); border-radius: var(--r4); color: var(--on-accent, #fff); }
  .box.on { border-color: var(--accent); background: var(--accent); }
  .box :global(svg) { width: 11px; height: 11px; }
  .ok { display: inline-flex; align-items: center; gap: 5px; color: var(--ok); font-size: var(--fs-meta); white-space: nowrap; }
  .led { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); }
  .link { color: var(--accent); font-size: var(--fs-meta); white-space: nowrap; cursor: pointer; }
  .tag { display: inline-flex; align-items: center; gap: 4px; padding: 0 6px; border-radius: var(--r5); background: var(--surface); color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .tag.other { color: var(--warn); }
  .inline { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 6px 0 0 40px; font-size: var(--fs-meta); }
  .inline :global(.field) { padding: 0; }
  .sel { display: flex; align-items: center; gap: 6px; }
  .warn { display: flex; align-items: center; gap: 6px; margin: 10px 0 0; color: var(--warn); font-size: var(--fs-meta); }
  .list { display: flex; flex-direction: column; }
  .pill { padding: 0 6px; border-radius: var(--r10); background: color-mix(in srgb, var(--accent) 18%, transparent); color: var(--accent); font-size: var(--fs-tag); font-weight: 500; }
  .muted { color: var(--text-muted); }
  .small { font-size: var(--fs-meta); }
  .mono { font-family: var(--mono); }
  .strip { padding: 10px 20px 12px; border-top: 1px dashed var(--border); font-size: var(--fs-meta); }
  .mini { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
  .mini span { display: flex; flex-direction: column; min-width: 92px; padding: 5px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); font-size: var(--fs-meta); }
  .mini span small { color: var(--text-muted); font-size: var(--fs-tag); }
  .mini span.off { opacity: 0.55; border-style: dashed; color: var(--text-muted); }
  .split { display: grid; grid-template-columns: 260px minmax(0, 1fr); gap: 16px; }
  .templates { display: flex; flex-direction: column; gap: 8px; }
  .templates button { display: flex; flex-direction: column; align-items: stretch; white-space: normal; gap: 4px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--bg); color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .templates button.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 10%, var(--bg)); }
  .templates button span { color: var(--text-muted); font-size: var(--fs-meta); line-height: 1.4; }
  .tags { display: flex; gap: 5px; margin-top: 2px; }
  .tags .pill.k8s { background: color-mix(in srgb, var(--src-k8s) 20%, transparent); color: var(--src-k8s); }
  .none { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border: 1px dashed var(--border); border-radius: var(--r10); font-size: var(--fs-body); }
  .none span { color: var(--text-muted); font-size: var(--fs-meta); line-height: 1.4; }
  .none button { align-self: flex-start; }
  .opt { display: flex; gap: 8px; align-items: flex-start; padding: 10px 4px 0; border-top: 1px solid var(--border); cursor: pointer; }
  .opt input { margin-top: 3px; --tick: var(--src-calendar); }
  .opt span { display: flex; flex-direction: column; gap: 2px; font-size: var(--fs-body); }
  .opt small { color: var(--text-muted); font-size: var(--fs-meta); line-height: 1.4; }
  .preview { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .cap { margin: 0; font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .page { border: 1px solid var(--border); border-top: 2px solid var(--tint); border-radius: var(--r9); background: var(--bg); }
  .ph { display: flex; align-items: center; gap: 7px; padding: 6px 10px; border-bottom: 1px solid var(--border); font-size: var(--fs-body); }
  .ph .dot { width: 8px; height: 8px; border-radius: var(--r2); background: var(--tint); }
  .ph .muted { margin-left: auto; font: var(--fs-meta) var(--mono); }
  .grid { display: flex; gap: 6px; padding: 8px; }
  .col { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 5px; }
  .blk { padding: 7px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); font-size: var(--fs-meta); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; cursor: help; }
  footer { display: flex; align-items: center; gap: 8px; padding: 11px 16px; border-top: 1px solid var(--border); font-size: var(--fs-meta); }
  .err { color: var(--ko); }
  .grow { flex: 1; }
  button { display: inline-flex; align-items: center; gap: 5px; padding: 4px 12px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); cursor: pointer; white-space: nowrap; }
  button.ghost { border-color: transparent; background: none; color: var(--text-muted); }
  button.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--accent); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
  button:disabled { opacity: 0.5; cursor: default; }
  @media (max-width: 720px) { .split { grid-template-columns: minmax(0, 1fr); } }
</style>
