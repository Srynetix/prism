<script lang="ts">
  import { suggest } from '../suggest'
  import { isMac, keys, tildePath, baseName } from '../platform'
  import { hasCore, invoke } from '../bridge'
  import Icon from './Icon.svelte'
  import { live } from '../live.svelte'
  import { afterLaunch, agentsAvailable, agentsSettings, agentsView, allTags, slug, spawnAgent, prepareAgent, closeAgent, draggedFiles, filesOf, type Launch, type LaunchSlot, closeLauncherDrawer, toastLaunched, kindOf } from '../agents.svelte'
  import { primaryRepo, worktreeRepos } from '../repositories'
  import { canPick, pickPath } from '../pick'
  import { toast } from '../toast.svelte'
  import { choices, effortsKey, loadEfforts, loadModels } from '../settings.svelte'
  import { CLIS, cliOf, providerOf } from '../agent-cli'
  import type { AgentProvider } from '../types'
  import { RECIPES, STEP_LABEL, WHEN_LABEL, type Step, type StepKind } from '../agent-chains.svelte'
  import { t } from '../i18n.svelte'
  import { pref, setPref } from '../layout.svelte'
  import DictateButton from './chat/DictateButton.svelte'
  import DictationBar from './chat/DictationBar.svelte'
  import { Attachments, nameOf, newStep, newWorktree, takeDraft, titleOf, watchPlan, watchTasks } from '../launch-form.svelte'

  /**
   * The New agent form of the Agents view. Mounted once and hidden when an agent shows: what is
   * typed in it stays until it is launched.
   */
  /**
   * `slot`: where its draft comes from (the Agents view's, or the drawer's: each consumes its own);
   * `drawer`: in the drawer over a panel, the form compact (kind, tags, Then and the command
   * under More options), the launch leaving the view as it is, said by a toast; `onclose`: Cancel.
   */
  let { kinds, withDesktop, canCancel, slot = agentsView, drawer = false, onclose }: { kinds: string[]; withDesktop: boolean; canCancel: boolean; slot?: LaunchSlot; drawer?: boolean; onclose?: () => void } = $props()
  /** Prefix of the ids of its fields' suggestions: two launchers may be open (view and drawer). */
  const uid = $props.id()
  /** In the drawer: the less used fields shown. */
  let more = $state(false)
  /**
   * Opened from a project's + (`slot.quick`): the short form, its prompt first, where it runs in a
   * line, the title and the worktree's name taken from the prompt; More options shows it all.
   */
  let quick = $state(false)

  /**
   * Launcher: a new worktree (default: Prism creates it next to the repository, through its hook
   * for Claude Code, on `<prefix><name>` from the release base), an existing worktree, or a folder
   * as is (a repository, or any folder chosen: outside the repositories too).
   */
  type Where = 'new' | 'existing' | 'folder'
  type Mode = 'default' | 'acceptEdits' | 'plan'
  /** The repositories where agents create worktrees (`[[repos]] worktrees`), the primary one first. */
  const wtRepos = $derived(worktreeRepos(live.snapshot))
  const checkouts = $derived((live.snapshot?.worktrees ?? []).filter((w) => w.is_main))
  const existing = $derived((live.snapshot?.worktrees ?? []).filter((w) => !w.is_main && !w.prunable))
  const plain = $derived.by(() => {
    const s = live.snapshot
    const seen = new Set<string>()
    const primary = primaryRepo(s)?.name
    return (s?.repos ?? [])
      .map((r) => ({ path: r.path, label: r.name === primary ? t('{name} (primary repo)', { name: r.name }) : r.name }))
      .filter((f) => !seen.has(f.path) && seen.add(f.path))
  })

  let title = $state('')
  let prompt = $state('')
  let promptField = $state<HTMLTextAreaElement>()
  let whereTo = $state<Where>('new')
  let name = $state('')
  /** Existing branch to check out in the new worktree (an MR's); empty: `<prefix><name>`. */
  let wtBranch = $state('')
  /** Branch on origin the new one starts from; empty: the repository's base. */
  let wtBase = $state('')
  let wtPath = $state('')
  /** Repository of the new worktree; empty: the first one. */
  let wtRepo = $state('')
  /**
   * Other repositories it works in too (`worktree.also`): a worktree of the same name in each, on
   * the same branch, given to its CLI (`--add-dir`); Claude Code and Codex only.
   */
  let alsoRepos = $state<string[]>([])
  const toggleAlso = (r: string) => (alsoRepos = alsoRepos.includes(r) ? alsoRepos.filter((x) => x !== r) : [...alsoRepos, r])
  let folder = $state('')
  /** The mode from `[agents] permission_mode`, each time the launcher opens. */
  let mode = $state<Mode>(agentsSettings().permission_mode)
  /** Its kind; empty: guessed from its prompt's command (`kindOf`), none otherwise. */
  let category = $state('')
  const guessed = $derived(kindOf(prompt))
  /** The new agent's tags, picked among the suggested ones or typed. */
  let launchTags = $state<string[]>([])
  function addLaunchTag(input: HTMLInputElement) {
    const tag = input.value.trim()
    input.value = ''
    if (tag && !launchTags.includes(tag)) launchTags = [...launchTags, tag]
  }
  /** The action that opened the launcher, kept with the launch; the agent it repeats (Launch again). */
  let from = $state('')
  /** Prism's MCP server given to it whatever `[agents] prism_mcp` says (Configure Prism). */
  let prismMcp = $state(false)
  let sameAs = $state('')
  /** The prepared agent being edited, which the next launch (or Prepare) replaces. */
  let replacing = $state('')
  /** Then…: the steps of its chain. */
  let steps = $state<Step[]>([])
  /** Launch again, then not: the form back to an empty launch. */
  function startBlank() {
    ;[title, prompt, name, wtBranch, wtBase, category, from, sameAs, picked, replacing] = ['', '', '', '', '', '', '', '', '', '']
    launchTags = []
    alsoRepos = []
    steps = []
    models = {}
    efforts = {}
    mode = agentsSettings().permission_mode
  }
  let busy = $state(false)
  let error = $state('')
  $effect(() => {
    if (!folder && plain.length) folder = plain[0].path
    if (!wtPath && existing.length) wtPath = existing[0].path
    if (!wtRepos.some((r) => r.name === wtRepo)) wtRepo = wtRepos[0]?.name ?? ''
    if (!wtRepos.length && whereTo === 'new') whereTo = 'folder'
  })
  /**
   * From Folder to a worktree, the repository follows: the folder's (the deepest repository
   * holding it) becomes the new worktree's, and an existing worktree of it is picked when the one
   * selected is another repository's.
   */
  function goTo(to: Where) {
    const repo = whereTo === 'folder' ? (live.snapshot?.repos ?? []).filter((r) => folder === r.path || folder.startsWith(`${r.path}/`)).sort((a, b) => b.path.length - a.path.length)[0]?.name : undefined
    if (repo && to === 'new' && wtRepos.some((r) => r.name === repo)) wtRepo = repo
    if (repo && to === 'existing' && existing.find((w) => w.path === wtPath)?.repo !== repo) wtPath = existing.find((w) => w.repo === repo)?.path ?? wtPath
    whereTo = to
  }
  /** Folder: any one, chosen in the system's picker (outside the repositories too). */
  async function chooseFolder() {
    const p = await pickPath(folder || '~')
    if (p) folder = p
  }
  /** Main checkout of the new worktree's repository: where it goes next to, and its base. */
  const main = $derived(checkouts.find((w) => w.repo === wtRepo) ?? null)
  /** Without a title, the prompt's first line; its first words name the worktree. */
  const autoTitle = $derived(titleOf(title, prompt))
  const autoName = $derived(title.trim() ? '' : nameOf(prompt))
  const wtName = $derived(slug(name) || (wtBranch ? slug(wtBranch.split('/').pop() ?? '') : '') || slug(title) || autoName || 'agent')

  /** An action opens the launcher prefilled: its title, its prompt, its location. */
  takeDraft(() => slot, (d) => {
    title = d.title
    prompt = d.prompt ?? ''
    error = ''
    wtBranch = ''
    wtBase = ''
    if (d.worktree) {
      whereTo = 'new'
      name = d.worktree.name ? slug(d.worktree.name) : ''
      wtBranch = d.worktree.branch ?? ''
      wtBase = d.worktree.base ?? ''
      if (d.worktree.repo) wtRepo = d.worktree.repo
      alsoRepos = d.worktree.also ?? []
    } else if (d.cwd && existing.some((w) => w.path === d.cwd)) {
      whereTo = 'existing'
      wtPath = d.cwd
    } else {
      whereTo = 'folder'
      if (d.cwd) folder = d.cwd
    }
    quick = !!slot.quick
    slot.quick = false
    if (quick) more = false
    // The short form: the mode last launched with in this repository, failing that the settings'.
    const lastMode = quick && d.worktree?.repo ? pref<Mode | ''>(`agents.mode.${d.worktree.repo}`, '') : ''
    mode = (d.permission_mode as Mode) || lastMode || agentsSettings().permission_mode
    category = d.category ?? ''
    launchTags = d.tags ?? []
    picked = d.provider ?? ''
    models = d.model ? { [d.provider ?? providerOf(launchRepo)]: d.model } : {}
    efforts = d.effort ? { [d.provider ?? providerOf(launchRepo)]: d.effort } : {}
    steps = d.then ? structuredClone(d.then) : []
    from = d.from ?? ''
    prismMcp = d.prism_mcp ?? false
    sameAs = slot.sameAs ?? ''
    slot.sameAs = null
    replacing = slot.replacing ?? ''
    slot.replacing = null
  })
  /** The repository the agent will run in, whose CLI it gets (`[[repos]] agent`); none: `[agents] provider`. */
  const launchRepo = $derived(whereTo === 'new' ? wtRepo : whereTo === 'existing' ? existing.find((w) => w.path === wtPath)?.repo : live.snapshot?.repos?.find((r) => r.path === folder)?.name)
  /** Its repository's CLI, failing that `[agents] provider`: what the agent runs unless another is picked. */
  const defaultProvider = $derived(providerOf(launchRepo))
  /** The CLI picked for this agent; empty: the default, which follows the repository. */
  let picked = $state<AgentProvider | ''>('')
  const launchProvider = $derived(picked || defaultProvider)
  const launchCli = $derived(cliOf(launchProvider))
  /** Its CLI can work in several folders (`--add-dir`). */
  const takesDirs = $derived(CLIS[launchProvider].takes_dirs)
  /** The other repositories ticked, besides the new worktree's own, while its CLI can take them. */
  const also = $derived(whereTo === 'new' && takesDirs ? alsoRepos.filter((r) => r !== wtRepo && wtRepos.some((x) => x.name === r)) : [])
  /** The model typed for each CLI (a model is one CLI's); empty: its own (`[agents.<cli>] model`), failing that its default. */
  let models = $state<Partial<Record<AgentProvider, string>>>({})
  const cliModel = $derived(agentsSettings()[launchProvider]?.model ?? null)
  const launchModel = $derived(models[launchProvider]?.trim() || '')
  $effect(() => {
    loadModels(launchProvider)
  })
  /** The effort picked for each CLI (its levels are its own); empty: its own (`[agents.<cli>] effort`), failing that its default. */
  let efforts = $state<Partial<Record<AgentProvider, string>>>({})
  const takesEffort = $derived(CLIS[launchProvider].takes_effort)
  const cliEffort = $derived(agentsSettings()[launchProvider]?.effort ?? null)
  const launchEffort = $derived(takesEffort ? efforts[launchProvider] || '' : '')
  /** Its levels, for the model it will run (Codex's differ by model). */
  const effortModel = $derived(CLIS[launchProvider].efforts_by_model ? launchModel || cliModel : null)
  $effect(() => {
    if (takesEffort) loadEfforts(launchProvider, effortModel)
  })
  const effortChoices = $derived(choices.efforts[effortsKey(launchProvider, effortModel)] ?? [])
  const wtRoot = $derived(agentsSettings().worktrees_dir ?? (main ? `${main.path}.worktrees` : ''))
  const prefix = $derived(agentsSettings().branch_prefix)
  const launchSpec = $derived<Launch>({
    title: autoTitle,
    prompt,
    permission_mode: mode === 'default' ? undefined : mode,
    category: category || undefined,
    tags: launchTags.length ? launchTags : undefined,
    provider: picked && picked !== defaultProvider ? picked : undefined,
    model: launchModel || undefined,
    effort: launchEffort || undefined,
    then: steps.length ? $state.snapshot(steps) : undefined,
    from: from || undefined,
    prism_mcp: prismMcp || undefined,
    ...(whereTo === 'new' ? { worktree: newWorktree({ name, fallback: autoName, branch: wtBranch, base: wtBase, repo: wtRepo, also }) } : { cwd: whereTo === 'existing' ? wtPath : folder }),
  })
  /**
   * What the new worktree will be, as Prism will create it: asked while typing (a short pause),
   * so a name already taken or an MR's branch already checked out shows before launching. Null:
   * not asked yet, or in the demo (the form's own guess then).
   */
  const planned = watchPlan(() => (whereTo === 'new' ? { title: autoTitle, worktree: newWorktree({ name, fallback: autoName, branch: wtBranch, base: wtBase, repo: wtRepo }) } : null))
  const plan = $derived(planned.value)
  const planName = $derived(plan?.name ?? wtName)
  /**
   * The command, as Prism will run it (the name may get a suffix if already taken): built by Prism
   * from what the form says so far (`agent_command_line`), the session's id and Prism's wiring
   * shown as `…`; what it refuses (a mode the CLI lacks) said in its place.
   */
  let command = $state('')
  $effect(() => {
    const preview = { provider: launchProvider, title: autoTitle || 'Agent', permission_mode: mode, model: launchModel || null, effort: launchEffort || null, worktree: whereTo === 'new' ? planName : null, also, prompt: prompt.trim() || null, prism_mcp: prismMcp }
    let gone = false
    const timer = setTimeout(() => {
      invoke<string>('agent_command_line', { preview }).then(
        (c) => !gone && (command = c),
        (e) => !gone && (command = String(e)),
      )
    }, 150)
    return () => {
      gone = true
      clearTimeout(timer)
    }
  })

  /* ---------- Then…: the steps of its chain (`steps`, declared with the form) ---------- */

  const STEP_KINDS: [StepKind, string][] = [['task', t('Task')], ['prompt', t('Prompt')], ['mr', t('Draft MR')], ['agent', t('Review agent')], ['notify', t('Notify')]]
  function addStep(kind: StepKind) {
    steps.push(newStep(kind))
  }
  /**
   * The tasks of the folder it will run in (a new worktree: its repository's), while Then shows:
   * a task step's suggestions.
   */
  const thenShown = $derived((!drawer && !quick) || more)
  const taskList = watchTasks(() => (thenShown ? (whereTo === 'new' ? { repo: wtRepo || null } : { cwd: whereTo === 'existing' ? wtPath : folder }) : null))
  const tasks = $derived(taskList.list)
  /** A recipe: its steps, as they are. */
  function useRecipe(r: (typeof RECIPES)[number]) {
    steps = structuredClone(r.steps)
  }

  /* ---------- Files attached to a prompt ---------- */

  /** Files attached to the launcher's prompt: their path (`@path` in the prompt) and a preview. */
  const att = new Attachments()
  let fileInput: HTMLInputElement | undefined = $state()
  async function attachToLaunch(files: File[]) {
    error = ''
    try {
      prompt = await att.add(files, prompt)
    } catch (e) {
      error = String(e)
    }
  }
  const detach = (i: number) => (prompt = att.remove(i, prompt))
  const kb = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1000))} kB`)

  /**
   * Launch, or (`prepared`) only declare it: it waits in the list for Start, its steps with it. The
   * effect of the action that opened the launcher (a ticket moved to Validation, assigned to me)
   * applies either way, at once: the work is taken. A prepared agent being edited is replaced by
   * the new one.
   */
  async function launch(prepared = false) {
    if (busy || !folder || (whereTo === 'new' && plan?.error)) return
    busy = true
    error = ''
    try {
      const id = await (prepared ? prepareAgent : spawnAgent)(launchSpec)
      if (quick && launchRepo) setPref(`agents.mode.${launchRepo}`, mode)
      steps = []
      if (replacing) await closeAgent(replacing).catch(() => null)
      replacing = ''
      // From the drawer, the view stays as it is: a toast leads to the agent.
      if (drawer) {
        closeLauncherDrawer()
        toastLaunched(id, launchSpec.title, prepared)
      } else {
        agentsView.selected = id
        agentsView.creating = false
      }
      // The effect of the action that opened the launcher (ticket moved to Validation…), once the agent is off.
      const effect = slot.onLaunch
      slot.onLaunch = null
      // Prepared too: the work is taken now, whenever the agent starts.
      void afterLaunch(effect, prepared ? t('Agent prepared') : t('Agent started'))
      title = ''
      prompt = ''
      quick = false
      att.clear()
      name = ''
      category = ''
      launchTags = []
      alsoRepos = []
      from = ''
      prismMcp = false
      sameAs = ''
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  /** A new agent from the New button: its launcher's own branch and error cleared. */
  export function fresh() {
    quick = false
    wtBranch = ''
    wtBase = ''
    error = ''
  }
</script>

{#if !agentsAvailable()}
  <p class="muted empty">{t('Agents run in the Prism app only (tmux server and terminals), not in the browser preview.')}</p>
{:else}
  <form class="launch" class:inside={drawer} onsubmit={(e) => { e.preventDefault(); void launch() }}>
    {#if quick}<h3><Icon name="terminal" />{t('New agent in {repo}', { repo: launchRepo ?? tildePath(folder) })}</h3>
    {:else if !drawer}<h3><Icon name="terminal" />{t('New agent')}</h3>{/if}
    {#if sameAs}<div class="same"><Icon name="retry" /><span>{#if replacing}{t('Editing the prepared')} <b>{sameAs}</b>{t(': Launch or Prepare replaces it')}{:else}{t('Same as')} <b>{sameAs}</b>{/if}</span><span class="grow"></span><button type="button" onclick={startBlank}>{t('Start blank')}</button></div>{/if}
    {#if !quick}<label><span>{t('Title')}</span><input bind:value={title} placeholder={autoTitle || t('Agent')} spellcheck="false" /></label>{/if}
    <label><span>{t('Prompt')}</span>
      <textarea bind:this={promptField} bind:value={prompt} {@attach (el: HTMLTextAreaElement) => { if (quick) el.focus() }} rows="3" spellcheck="false" writingsuggestions="false" placeholder={t('Optional. Sent as is: a prompt or a /plugin:skill command. Drop or paste images and files here.')} onkeydown={(e) => { if (e.key === 'Enter' && (isMac ? e.metaKey : e.ctrlKey)) { e.preventDefault(); void launch() } }}
        ondragover={(e) => { if (draggedFiles(e)) e.preventDefault() }}
        ondrop={(e) => { const files = filesOf(e.dataTransfer?.files); if (files.length) { e.preventDefault(); void attachToLaunch(files) } }}
        onpaste={(e) => { const files = filesOf(e.clipboardData?.files); if (files.length) { e.preventDefault(); void attachToLaunch(files) } }}></textarea>
    </label>
    <DictationBar key="launcher" />
    <div class="attach">
      <DictateButton key="launcher" field={() => promptField} text={() => prompt} write={(v) => (prompt = v)} hints={() => [launchRepo ?? '', title]} />
      {#each att.list as a, i (a.path)}
        <span class="thumb" title={a.path}>
          {#if a.url}<img src={a.url} alt={a.name} />{:else}<span class="file"><Icon name="files" /></span>{/if}
          <span class="nm">{a.name}<small>{kb(a.size)}</small></span>
          <button type="button" class="x" onclick={() => detach(i)} aria-label={t('Remove {name}', { name: a.name })}>×</button>
        </span>
      {/each}
      <button type="button" class="drop" disabled={att.busy} onclick={() => fileInput?.click()}
        ondragover={(e) => { if (draggedFiles(e)) e.preventDefault() }}
        ondrop={(e) => { const files = filesOf(e.dataTransfer?.files); if (files.length) { e.preventDefault(); void attachToLaunch(files) } }}>
        {att.busy ? t('Attaching…') : t('Attach images or files: drop, paste (⌘V) or click')}
      </button>
      <input bind:this={fileInput} type="file" multiple hidden onchange={(e) => { void attachToLaunch(filesOf(e.currentTarget.files)); e.currentTarget.value = '' }} />
    </div>
    {#if quick}
      <div class="field"><span>{t('Where')}</span>
        <div class="short">
          {#if whereTo === 'new'}<code>{planName}</code> <i>{wtBranch ? t('(existing)') : t('new worktree, from {base}', { base: wtBase.trim() || plan?.base || main?.base || 'main' })}</i>{#if also.length}{' '}<i>{t('also in {repos}', { repos: also.join(', ') })}</i>{/if}
          {:else if whereTo === 'existing'}<code>{tildePath(wtPath)}</code>
          {:else}<code>{tildePath(folder)}</code> <i>{t('the checkout itself')}</i>{/if}
          <button type="button" class="more" onclick={() => (quick = false)}>{t('Change')}</button>
        </div>
      </div>
    {:else}
    <div class="field"><span>{t('Where')}</span>
      <div>
        <div class="seg">
          <button type="button" class:on={whereTo === 'new'} disabled={!wtRepos.length} onclick={() => goTo('new')}>{t('New worktree')}</button>
          <button type="button" class:on={whereTo === 'existing'} disabled={!existing.length} onclick={() => goTo('existing')}>{t('Existing worktree')}</button>
          <button type="button" class:on={whereTo === 'folder'} onclick={() => goTo('folder')}>{t('Folder')}</button>
        </div>
        {#if whereTo === 'new'}
          <div class="where">
            {#if wtRepos.length > 1}
              <span>{t('Repository')}</span><select bind:value={wtRepo}>{#each wtRepos as r (r.name)}<option value={r.name}>{r.name}</option>{/each}</select>
            {/if}
            <span>{t('Name')}</span><input bind:value={name} placeholder={wtName} spellcheck="false" />
            {#if plan?.taken}<small class="taken">{t('{reason}: {name} instead', { reason: plan.taken, name: plan.name })}</small>{/if}
            <span>{t('Folder')}</span><code>{plan ? tildePath(plan.dir) : `${tildePath(wtRoot)}/${wtName}`}</code>
            {#if wtBranch}
              <span>{t('Branch')}</span><code>{wtBranch} <i>{t('(existing)')}</i></code>
            {:else}
              <span>{t('Branch')}</span><code>{plan?.branch ?? `${prefix}${wtName}`} <i>{t('(new)')}</i></code>
              <label for="{uid}-base">{t('From')}</label>
              <div class="base">
                <span>origin/</span><input id="{uid}-base" bind:value={wtBase} {@attach suggest(() => plan?.bases ?? [])} placeholder={plan?.base ?? main?.base ?? 'main'} spellcheck="false" autocomplete="off" autocapitalize="off" title={t('The branch the new one starts from; empty: the repository’s base')} />
              </div>
            {/if}
            {#if wtRepos.length > 1}
              <span>{t('Also in')}</span>
              <div class="also">
                {#each wtRepos.filter((r) => r.name !== wtRepo) as r (r.name)}
                  <button type="button" class:on={also.includes(r.name)} disabled={!takesDirs} onclick={() => toggleAlso(r.name)} aria-pressed={also.includes(r.name)} title={takesDirs ? t('A worktree of {repo} too, same name and branch: the agent works in both', { repo: r.name }) : t('{cli} works in one folder only: Claude Code or Codex for several repositories', { cli: launchCli.name })}>{r.name}</button>
                {/each}
              </div>
              {#if also.length}<small class="alsonote">{t('{name} in each, from its own base; an MR per repository at Draft MR, linked to one another.', { name: planName })}</small>{/if}
            {/if}
          </div>
        {:else if whereTo === 'existing'}
          <select bind:value={wtPath}>{#each existing as w (w.path)}<option value={w.path}>{wtRepos.length > 1 ? `${w.repo} · ` : ''}{baseName(w.path)} · {w.branch ?? t('detached')}{w.dirty ? ` · ${t('changes')}` : ''}</option>{/each}</select>
        {:else}
          <div class="pick">
            <select bind:value={folder}>
              {#each plain as f (f.path)}<option value={f.path}>{f.label}</option>{/each}
              {#if folder && !plain.some((f) => f.path === folder)}<option value={folder}>{tildePath(folder)}</option>{/if}
            </select>
            {#if canPick()}<button type="button" onclick={chooseFolder} title={t('Any folder, outside the repositories too')}>{t('Choose…')}</button>{/if}
          </div>
          {#if !canPick()}<input class="typed" placeholder={t('Other folder: ~/path or /path')} spellcheck="false" autocapitalize="off" onchange={(e) => { const v = e.currentTarget.value.trim(); if (v) folder = v; e.currentTarget.value = '' }} />{/if}
          {#if checkouts.some((w) => w.path === folder)}<small class="muted">{t('Shared checkout: best for read-only work (refine, review, investigate).')}</small>{/if}
        {/if}
      </div>
    </div>
    {/if}
    <div class="field"><span>{t('Agent')}</span>
      <div class="seg clis">
        {#each Object.entries(CLIS) as [p, c] (p)}
          {@const isDefault = p === defaultProvider}
          <button type="button" class:on={launchProvider === p} onclick={() => (picked = isDefault ? '' : (p as AgentProvider))} title={isDefault ? `${c.cli}: ${launchRepo && live.snapshot?.repositories?.find((r) => r.name === launchRepo)?.agent ? t('{repo}’s CLI', { repo: launchRepo }) : t('the default CLI')}` : t('{cli}, for this agent only', { cli: c.cli })}><Icon name={c.icon} />{c.name}{#if isDefault}<i class="def">{t('default')}</i>{/if}</button>
        {/each}
      </div>
    </div>
    {#if !quick}
    <label><span>{t('Model')}</span><input value={models[launchProvider] ?? ''} oninput={(e) => (models[launchProvider] = e.currentTarget.value)} placeholder={cliModel ? t('{value} (Settings)', { value: cliModel }) : t('{cli}’s default', { cli: launchCli.name })} spellcheck="false" autocapitalize="off" {@attach suggest(() => choices.models[launchProvider] ?? [])} title={t('For this agent, kept on resume: any name its CLI takes')} /></label>
    {#if takesEffort}
      <label><span>{t('Effort')}</span>
        <select value={launchEffort} onchange={(e) => (efforts[launchProvider] = e.currentTarget.value)} title={t('Its reasoning effort, kept on resume: higher thinks longer and uses your limits faster')}>
          <option value="">{cliEffort ? t('{value} (Settings)', { value: cliEffort }) : t('{cli}’s default', { cli: launchCli.name })}</option>
          {#each launchEffort && !effortChoices.includes(launchEffort) ? [...effortChoices, launchEffort] : effortChoices as e (e)}<option value={e}>{e}</option>{/each}
        </select>
      </label>
    {/if}
    {/if}
    <div class="field"><span>{t('Permissions')}</span>
      <div class="seg">
        <button type="button" class:on={mode === 'default'} onclick={() => (mode = 'default')} title={t('The mode of your {cli} settings', { cli: launchCli.cli })}>{t('Default')}</button>
        <button type="button" class:on={mode === 'acceptEdits'} onclick={() => (mode = 'acceptEdits')}>{t('Accept edits')}</button>
        <button type="button" class:on={mode === 'plan'} onclick={() => (mode = 'plan')}>{t('Plan first')}</button>
      </div>
    </div>
    {#if quick}<button type="button" class="more" onclick={() => { quick = false; more = true }} aria-expanded="false">{t('More options')}<small> · {t('Title, where, model, Then, kind, tags')}</small></button>
    {:else if drawer}<button type="button" class="more" onclick={() => (more = !more)} aria-expanded={more}>{more ? t('Fewer options') : t('More options')}<small> · {t('Then, kind, tags, command')}</small></button>{/if}
    {#if thenShown}
    <div class="field"><span>{t('Then')}</span>
      <div class="then">
        {#each steps as s, i (i)}
          <div class="step">
            <span class="n">{i + 1}</span>
            <span class="k">{STEP_LABEL[s.kind]}</span>
            {#if s.kind === 'task'}<input bind:value={s.command} {@attach suggest(() => tasks)} placeholder={t('Command, e.g. npm test')} spellcheck="false" aria-label={t('Command')} />
            {:else if s.kind === 'prompt' || s.kind === 'agent'}<input bind:value={s.text} placeholder={s.kind === 'agent' ? t('Its prompt: {mr}, {ticket}, {branch} filled in', { mr: '{mr}', ticket: '{ticket}', branch: '{branch}' }) : t('Sent to this agent when it waits for you')} spellcheck="false" writingsuggestions="false" aria-label={t('Prompt')} />
            {:else}<span class="grow"></span>{/if}
            <select bind:value={s.when} aria-label={t('When')}>{#each Object.entries(WHEN_LABEL) as [w, label] (w)}<option value={w}>{label}</option>{/each}</select>
            <button type="button" class="x" onclick={() => steps.splice(i, 1)} aria-label={t('Remove step {n}', { n: i + 1 })}>×</button>
          </div>
        {/each}
        <div class="adds">
          {#each STEP_KINDS as [k, label] (k)}<button type="button" onclick={() => addStep(k)}>+ {label}</button>{/each}
          <span class="muted">{t('or')}</span>
          {#each RECIPES as r (r.name)}<button type="button" class="recipe" onclick={() => useRecipe(r)} title={r.steps.map((s) => STEP_LABEL[s.kind]).join(' → ')}>{r.name}</button>{/each}
        </div>
        {#if steps.length}<small class="muted">{t('After its work: each step once the one before has finished. A failed task goes back to the agent once. Nothing merges; no push to a protected branch.')}</small>{/if}
      </div>
    </div>
    {#if kinds.length}
      <div class="field"><span>{t('Kind')}</span>
        <div class="seg">
          <button type="button" class:on={!category} onclick={() => (category = '')} title={t('Guessed from the command the prompt starts with (Settings › Agents › Kinds)')}>{t('Auto')}{#if guessed}<small> · {guessed}</small>{/if}</button>
          {#each kinds as c (c)}<button type="button" class:on={category === c} onclick={() => (category = c)}>{c}</button>{/each}
        </div>
      </div>
    {/if}
    <div class="field"><span>{t('Tags')}</span>
      <div class="tags">
        {#each launchTags as tag (tag)}
          <span class="pill tag">#{tag}<button type="button" class="untag" onclick={() => (launchTags = launchTags.filter((x) => x !== tag))} title={t('Remove #{tag}', { tag })} aria-label={t('Remove tag {tag}', { tag })}><Icon name="close" size={9} /></button></span>
        {/each}
        {#each (agentsSettings().tags ?? []).filter((tag) => !launchTags.includes(tag)) as tag (tag)}
          <button type="button" class="sugg" onclick={() => (launchTags = [...launchTags, tag])} title={t('Add #{tag}', { tag })}>+ #{tag}</button>
        {/each}
        <!-- Enter adds the tag, it does not launch. -->
        <input class="addtag" {@attach suggest(() => allTags().filter((tag) => !launchTags.includes(tag)))} placeholder={t('+ tag')} spellcheck="false" aria-label={t('Add a tag')} onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addLaunchTag(e.currentTarget) } }} onchange={(e) => addLaunchTag(e.currentTarget)} />
      </div>
    </div>
    <div class="field"><span>{t('Command')}</span><code class="cmd">{command}</code></div>
    {/if}
    {#if whereTo === 'new' && plan?.error}<p class="err">{plan.error}</p>{/if}
    {#if error}<p class="err">{error}</p>{/if}
    <div class="row">
      <!-- Claude Desktop as an alternative only where its sessions are shown: only with Claude Code. -->
      {#if slot.desktopHref && withDesktop && !quick}<a class="desk" href={slot.desktopHref} onclick={() => { const effect = slot.onLaunch; slot.onLaunch = null; void afterLaunch(effect, t('Claude Desktop opened')) }} title={t('The former behaviour: a Claude Desktop session, prompt not sent')}>{t('Open in Claude Desktop instead')}</a>{/if}
      <span class="grow"></span>
      {#if canCancel}<button type="button" onclick={() => (onclose ? onclose() : (agentsView.creating = false))}>{t('Cancel')}</button>{/if}
      <button type="button" disabled={busy || (whereTo === 'folder' && !folder) || (whereTo === 'existing' && !wtPath) || (whereTo === 'new' && !!plan?.error)} onclick={() => void launch(true)} title={slot.onLaunch ? t('Declare it without starting it: it waits in the list for Start, its worktree made then. The action’s effect (ticket moved…) applies now') : t('Declare it without starting it: it waits in the list for Start, its worktree made then')}>{t('Prepare')}</button>
      <button type="submit" class="primary" disabled={busy || (whereTo === 'folder' && !folder) || (whereTo === 'existing' && !wtPath) || (whereTo === 'new' && !!plan?.error)}>{busy ? t('Starting…') : t('Launch')} <kbd>{keys('⌘↩')}</kbd></button>
    </div>
  </form>
{/if}

<style>
  .grow { flex: 1; }
  .empty { margin: auto; max-width: 420px; padding: 14px 10px; font-size: var(--fs-meta); line-height: 1.5; text-align: center; }

  button { display: inline-flex; align-items: center; gap: 5px; padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); cursor: pointer; white-space: nowrap; }
  button:hover:not(:disabled) { border-color: var(--src-claude); }
  button:disabled { opacity: 0.5; cursor: default; }
  button.primary { background: var(--src-claude); border-color: var(--src-claude); color: #fff; background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--src-claude); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
  kbd { font: 600 10px var(--mono); opacity: 0.8; }
  .err { margin: 0; color: var(--ko); font-size: var(--fs-meta); white-space: pre-wrap; }

  .launch { width: min(620px, 92%); margin: 40px auto; display: flex; flex-direction: column; gap: 12px; padding: 16px 18px; border: 1px solid var(--border); border-radius: var(--r12); background: var(--surface); }
  /* In the drawer: the panel's width, no frame of its own. */
  .launch.inside { width: auto; margin: 0; padding: 12px 14px 16px; border: none; border-radius: 0; background: none; }
  .more { align-self: flex-start; padding: 0; border: none; background: none; color: var(--info); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .more small { color: var(--text-muted); font-size: inherit; }
  .launch h3 { margin: 0; display: flex; align-items: center; gap: 8px; font-size: 14px; }
  .launch h3 :global(svg) { color: var(--src-claude); }
  .same { display: flex; align-items: center; gap: 7px; padding: 6px 9px; border-radius: var(--r7); background: var(--info-bg); color: var(--info); font-size: var(--fs-meta); }
  .same .grow { flex: 1; }
  .same button { padding: 0; border: none; background: none; color: inherit; font: inherit; text-decoration: underline; cursor: pointer; }
  .launch label { display: grid; grid-template-columns: 70px minmax(0, 1fr); gap: 10px; align-items: start; }
  .launch label > span { padding-top: 5px; color: var(--text-muted); font-size: var(--fs-meta); }
  .launch input, .launch select, .launch textarea { min-width: 0; padding: 5px 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text); font: inherit; outline: none; }
  .launch textarea { font-family: var(--mono); resize: vertical; max-height: 60vh; }
  .launch input:focus, .launch select:focus, .launch textarea:focus { border-color: var(--src-claude); }
  .row { display: flex; align-items: center; justify-content: flex-end; gap: 8px; }
  .row .desk { font-size: var(--fs-meta); color: var(--text-muted); }
  .launch .field { display: grid; grid-template-columns: 70px minmax(0, 1fr); gap: 10px; align-items: start; }
  .launch .field > span { padding-top: 4px; color: var(--text-muted); font-size: var(--fs-meta); }
  .launch .field small { display: block; margin-top: 5px; font-size: var(--fs-meta); }
  .launch .field select { margin-top: 6px; width: 100%; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r7); overflow: hidden; }
  /* The phone: the CLIs wrap rather than run off the screen; larger targets. */
  :global(html.remote) .seg { flex-wrap: wrap; }
  :global(html.remote) .launch button { min-height: 36px; }
  :global(html.remote) .launch input, :global(html.remote) .launch select, :global(html.remote) .launch textarea { font-size: 16px; }
  .seg button { border: none; border-radius: 0; background: var(--surface); color: var(--text-muted); }
  .seg button + button { border-left: 1px solid var(--border); }
  .clis button { display: inline-flex; align-items: center; gap: 5px; }
  .clis .def { font-style: normal; font-size: var(--fs-tag); opacity: 0.7; }
  .seg button.on { background: var(--src-claude); color: #fff; }
  .pick { display: flex; gap: 6px; align-items: center; }
  .pick select { flex: 1; min-width: 0; }
  .pick button { margin-top: 6px; flex-shrink: 0; }
  .typed { margin-top: 6px; width: 100%; }
  /* The short form's Where: a line, Change for the whole field. */
  .short { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 6px; padding-top: 4px; font-size: var(--fs-meta); color: var(--text-muted); }
  .short code { font-family: var(--mono); color: var(--text); overflow-wrap: anywhere; }
  .short i { font-style: normal; }
  .short .more { margin-left: 4px; }
  .where { margin-top: 8px; display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 4px 10px; align-items: center; font-size: var(--fs-meta); color: var(--text-muted); }
  .where input { padding: 3px 8px; }
  .where .taken { grid-column: 1 / -1; color: var(--warn); }
  /* The other repositories it works in too: toggles. */
  .where .also { display: flex; flex-wrap: wrap; gap: 4px; }
  .where .also button { padding: 0 8px; border-radius: var(--r9); font-family: var(--mono); font-size: var(--fs-meta); color: var(--text-muted); }
  .where .also button.on { border-color: var(--src-claude); background: color-mix(in srgb, var(--src-claude) 13%, transparent); color: var(--src-claude); }
  .where .alsonote { grid-column: 1 / -1; margin: 0; color: var(--text-muted); }
  .where code { font-family: var(--mono); color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .where i { color: var(--text-muted); font-style: normal; }
  .where .base { display: flex; align-items: center; min-width: 0; }
  .where .base span { font-family: var(--mono); color: var(--text-muted); }
  .where .base input { flex: 1; min-width: 0; font-family: var(--mono); }
  .cmd { padding: 7px 9px; border-radius: var(--r7); background: var(--bg); border: 1px solid var(--border); font: var(--fs-meta) var(--mono); color: var(--text-muted); white-space: pre-wrap; word-break: break-all; }
  .seg { display: inline-flex; flex-shrink: 0; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { padding: 1px 8px; border: none; border-right: 1px solid var(--border); border-radius: 0; background: none; color: var(--text-muted); font-size: var(--fs-meta); }
  .seg button:last-child { border-right: none; }
  .seg button.on { background: color-mix(in srgb, var(--src-claude) 13%, transparent); color: var(--src-claude); }
  .pill.tag { background: var(--info-bg); color: var(--info); }
  .tags { display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; }
  .tags .pill.tag { display: inline-flex; align-items: center; gap: 2px; padding-right: 2px; }
  .untag { display: inline-flex; padding: 1px; border: none; border-radius: 50%; background: none; color: inherit; opacity: 0.6; cursor: pointer; }
  .untag:hover { opacity: 1; background: color-mix(in srgb, var(--info) 18%, transparent); }
  .addtag { width: 58px; padding: 0 6px; border: 1px dashed var(--border); border-radius: var(--r9); background: none; color: var(--text); font: inherit; font-size: var(--fs-tag); outline: none; }
  .addtag:focus { width: 100px; border-style: solid; border-color: var(--info); }
  .tags { flex-wrap: wrap; }
  .sugg { padding: 0 7px; border: 1px dashed var(--border); border-radius: var(--r9); background: none; color: var(--info); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .sugg:hover { border-style: solid; border-color: var(--info); }
  .attach { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; margin: -2px 0 4px; }
  .attach .thumb { position: relative; display: inline-flex; align-items: center; gap: 6px; max-width: 220px; padding: 3px 22px 3px 3px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); font-size: var(--fs-tag); }
  .attach img, .attach .file { width: 34px; height: 34px; flex-shrink: 0; border-radius: var(--r4); object-fit: cover; background: var(--surface-muted); display: grid; place-items: center; }
  .attach .nm { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .attach .nm small { display: block; color: var(--text-muted); }
  .attach .x { position: absolute; top: 2px; right: 3px; padding: 0 3px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .attach .drop { flex: 1; min-width: 200px; padding: 6px 10px; border: 1.5px dashed color-mix(in srgb, var(--src-claude) 45%, var(--border)); border-radius: var(--r7); background: color-mix(in srgb, var(--src-claude) 5%, transparent); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .then { display: flex; flex-direction: column; gap: 5px; min-width: 0; flex: 1; }
  .then .step { display: grid; grid-template-columns: 20px auto minmax(0, 1fr) auto 18px; gap: 6px; align-items: center; padding: 4px 6px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); font-size: var(--fs-meta); }
  .then .n { display: grid; place-items: center; width: 18px; height: 18px; border-radius: 50%; background: var(--surface-muted); font-size: 10px; font-weight: 700; }
  .then .k { white-space: nowrap; }
  .then input, .then select { min-width: 0; padding: 2px 6px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  .then .x { padding: 0; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .then .adds { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; }
  .then .adds button { padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r9); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .then .adds button:hover { border-color: var(--src-claude); color: var(--text); }
  .then .adds .recipe { border-style: dashed; }
  .then .grow { flex: 1; }
</style>
