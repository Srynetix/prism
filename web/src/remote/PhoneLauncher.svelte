<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import { live } from '../live.svelte'
  import { afterLaunch, agentsSettings, closeLauncherDrawer, filesOf, kindNames, launcherDrawer, openAgents, prepareAgent, slug, spawnAgent, toastLaunched, type Launch } from '../agents.svelte'
  import { Attachments, nameOf, newStep, newWorktree, takeDraft, tasksFor, testTask, titleOf, watchPlan, watchTasks } from '../launch-form.svelte'
  import { hasWorktrees, primaryRepo } from '../repositories'
  import { CLIS, cliOf, providerOf } from '../agent-cli'
  import { choices, effortsKey, loadEfforts, loadModels } from '../settings.svelte'
  import { STEP_LABEL, WHEN_LABEL, type Step, type StepKind } from '../agent-chains.svelte'
  import { pref, setPref } from '../layout.svelte'
  import { tildePath, baseName } from '../platform'
  import { t } from '../i18n.svelte'
  import type { AgentProvider } from '../types'

  /**
   * A new agent on a phone: a sheet from the bottom, made for a thumb, in place of the computer's
   * launcher (`AgentLauncher`). The prompt first, then where (a repository in chips: a new
   * worktree where agents make them, its clone otherwise), with which CLI, in which mode — only
   * those a phone may launch in (`phone_launch`: Plan, Ask, Edits alone, never `auto`) — and
   * Launch under the thumb, Prepare beside it. More options unfolds the rest (worktree, branch,
   * base, model, effort, kind, tags, Then). Every way in opens it (New, a project's +, a ticket's ▶,
   * a quick action, an action's agent), prefilled with its draft (`launcherDrawer`). Its repository
   * and its mode are the last ones launched from the phone (prefs), the draft's when it names them.
   * Launched, the agent's Chat opens.
   */

  type Mode = 'plan' | 'default' | 'acceptEdits'
  type Where = 'new' | 'existing' | 'clone' | 'folder'
  const MODES: [Mode, string, string][] = [
    ['plan', t('Plan'), t('You approve its plan before it touches the code.')],
    ['default', t('Ask'), t('The mode of your CLI’s settings: usually it asks before each edit and each command.')],
    ['acceptEdits', t('Edits alone'), t('It edits files on its own, and asks before a command.')],
  ]
  const isMode = (m: unknown): m is Mode => m === 'plan' || m === 'default' || m === 'acceptEdits'
  const STEP_KINDS: StepKind[] = ['mr', 'task', 'agent', 'prompt', 'notify']

  const snap = $derived(live.snapshot)
  /** The repositories with a clone here, those a phone can launch in. */
  const repos = $derived((snap?.repos ?? []).filter((r) => r.path && !r.error))

  let prompt = $state('')
  let repo = $state('')
  let whereTo = $state<Where>('new')
  /** A folder the draft names that is neither a worktree nor a clone (an agent's): kept as is. */
  let folder = $state('')
  let wtPath = $state('')
  let name = $state('')
  let branch = $state('')
  let base = $state('')
  let picked = $state<AgentProvider | ''>('')
  /** The model and the effort picked for each CLI (a model is one CLI's); empty: its own. */
  let models = $state<Partial<Record<AgentProvider, string>>>({})
  let efforts = $state<Partial<Record<AgentProvider, string>>>({})
  /** Other repositories its new worktree is made in too (a draft's `worktree.also`). */
  let also = $state<string[]>([])
  let mode = $state<Mode>('plan')
  let category = $state('')
  let tags = $state<string[]>([])
  let steps = $state<Step[]>([])
  let from = $state('')
  let prismMcp = $state(false)
  let title = $state('')
  let more = $state(false)
  let showRepos = $state(false)
  let busy = $state(false)
  let error = $state('')

  /** The repository first offered: the draft's, failing that the last one launched in, the primary one. */
  const lastRepo = () => pref<string>('phone.launch.repo', '')
  /** The repository offered when the draft names none: the last one launched in, the primary one, the first; among those making worktrees when `worktrees`. */
  function defaultRepo(worktrees = false) {
    const ok = repos.filter((r) => !worktrees || hasWorktrees(r.name))
    const pool = ok.length ? ok : repos
    return (pool.find((r) => r.name === lastRepo()) ?? pool.find((r) => r.name === primaryRepo(live.snapshot)?.name) ?? pool[0])?.name ?? ''
  }
  function placeOf(d: Launch) {
    const s = live.snapshot
    if (d.worktree) {
      // Its repository when it has a clone here (a phone launches in nothing else), failing that the default one.
      repo = repos.some((r) => r.name === d.worktree?.repo) ? d.worktree.repo! : defaultRepo(true)
      // A repository that makes no worktree: its clone, said in the Where line.
      whereTo = hasWorktrees(repo) ? 'new' : 'clone'
      return
    }
    const wt = d.cwd && s?.worktrees.find((w) => w.path === d.cwd)
    if (wt && !wt.is_main) {
      repo = wt.repo ?? ''
      whereTo = 'existing'
      wtPath = wt.path
      return
    }
    const clone = d.cwd && repos.find((r) => r.path === d.cwd)
    if (clone) {
      repo = clone.name
      whereTo = 'clone'
      return
    }
    if (d.cwd) {
      folder = d.cwd
      repo = ''
      whereTo = 'folder'
      return
    }
    repo = defaultRepo()
    whereTo = hasWorktrees(repo) ? 'new' : 'clone'
  }

  /** Its draft, once, each time it opens (as the computer's launcher). */
  takeDraft(() => launcherDrawer, (d) => {
    prompt = d.prompt ?? ''
    title = d.title ?? ''
    error = ''
    folder = ''
    att.clear()
    placeOf(d)
    name = d.worktree?.name ? slug(d.worktree.name) : ''
    branch = d.worktree?.branch ?? ''
    base = d.worktree?.base ?? ''
    picked = d.provider ?? ''
    const cli = d.provider ?? providerOf(repo || undefined)
    models = d.model ? { [cli]: d.model } : {}
    efforts = d.effort ? { [cli]: d.effort } : {}
    also = d.worktree?.also ?? []
    const last = pref<string>('phone.launch.mode', '')
    mode = isMode(d.permission_mode) ? d.permission_mode : isMode(last) ? last : 'plan'
    category = d.category ?? ''
    tags = d.tags ?? []
    steps = d.then ? structuredClone($state.snapshot(d.then)) : []
    from = d.from ?? ''
    prismMcp = d.prism_mcp ?? false
    launcherDrawer.replacing = null
    launcherDrawer.sameAs = null
    launcherDrawer.quick = false
    more = false
    showRepos = false
  })

  function pickRepo(r: string) {
    repo = r
    folder = ''
    whereTo = hasWorktrees(r) ? 'new' : 'clone'
    wtPath = ''
    name = branch = base = ''
    also = also.filter((x) => x !== r)
  }
  /** The repositories in chips: the one picked first, then the last one launched in, then the rest. */
  const ordered = $derived.by(() => {
    const last = lastRepo()
    const rank = (n: string) => (n === repo ? 0 : n === last ? 1 : 2)
    return [...repos].sort((a, b) => rank(a.name) - rank(b.name))
  })
  const SHOWN = 4
  const chips = $derived(showRepos ? ordered : ordered.slice(0, SHOWN))

  const repoPath = $derived(repos.find((r) => r.name === repo)?.path ?? '')
  const worktrees = $derived((snap?.worktrees ?? []).filter((w) => !w.is_main && !w.prunable && w.repo === repo))
  $effect(() => {
    if (whereTo === 'existing' && !worktrees.some((w) => w.path === wtPath)) wtPath = worktrees[0]?.path ?? ''
  })

  const provider = $derived(picked || providerOf(repo || undefined))
  const cliModel = $derived(agentsSettings()[provider]?.model ?? null)
  const takesEffort = $derived(CLIS[provider].takes_effort)
  const model = $derived(models[provider] ?? '')
  const effort = $derived(efforts[provider] ?? '')
  $effect(() => {
    if (more) loadModels(provider)
  })
  const effortModel = $derived(CLIS[provider].efforts_by_model ? model.trim() || cliModel : null)
  $effect(() => {
    if (more && takesEffort) loadEfforts(provider, effortModel)
  })
  const effortChoices = $derived(choices.efforts[effortsKey(provider, effortModel)] ?? [])
  const kinds = $derived(kindNames())

  const autoTitle = $derived(titleOf(title, prompt))
  const autoName = $derived(nameOf(prompt))
  /** Its other repositories, while its CLI can work in several folders. */
  const alsoIn = $derived(CLIS[provider].takes_dirs ? also.filter((x) => x !== repo) : [])
  const worktree = $derived(newWorktree({ name, fallback: slug(title) || autoName, branch, base, repo, also: alsoIn }))

  /** The new worktree as Prism will make it (its name, branch and base). */
  const planned = watchPlan(() => (whereTo === 'new' ? { title: autoTitle, worktree } : null))
  const plan = $derived(planned.value)

  const cwd = $derived(whereTo === 'existing' ? wtPath : whereTo === 'clone' ? repoPath : folder)
  const launch = $derived<Launch>({
    title: autoTitle,
    prompt,
    permission_mode: mode === 'default' ? undefined : mode,
    category: category || undefined,
    tags: tags.length ? tags : undefined,
    provider: picked && picked !== providerOf(repo || undefined) ? picked : undefined,
    model: model.trim() || undefined,
    effort: (takesEffort && effort) || undefined,
    then: steps.length ? $state.snapshot(steps) : undefined,
    from: from || undefined,
    prism_mcp: prismMcp || undefined,
    ...(whereTo === 'new' ? { worktree } : { cwd }),
  })

  /** What Launch does, in a line under it. */
  const summary = $derived(
    [
      whereTo === 'folder' ? tildePath(folder) : repo,
      whereTo === 'new' ? t('new worktree') : whereTo === 'existing' ? baseName(wtPath) : whereTo === 'clone' ? t('the clone') : '',
      cliOf(provider).name,
      MODES.find(([m]) => m === mode)?.[1] ?? '',
    ]
      .filter(Boolean)
      .join(' · '),
  )
  const whereLine = $derived(
    whereTo === 'new'
      ? branch
        ? t('new worktree on {branch}', { branch })
        : t('new worktree from {base}', { base: base.trim() || plan?.base || 'main' }) + (alsoIn.length ? ` · ${t('also in {repos}', { repos: alsoIn.join(', ') })}` : '')
      : whereTo === 'existing'
        ? t('in the worktree {name}', { name: baseName(wtPath) || '—' })
        : whereTo === 'clone'
          ? t('in the clone itself')
          : tildePath(folder),
  )

  /* ---------- Photos and files, attached to the prompt ---------- */
  const att = new Attachments()
  let photoInput = $state<HTMLInputElement>()
  let fileInput = $state<HTMLInputElement>()
  async function attach(files: File[]) {
    error = ''
    try {
      prompt = await att.add(files, prompt)
    } catch (e) {
      error = String(e)
    }
  }
  const detach = (i: number) => (prompt = att.remove(i, prompt))

  /* ---------- Then: its steps, those a phone may add (`phone_then`) ---------- */
  const where = $derived(whereTo === 'new' ? { repo: repo || null } : { cwd })
  const hasTaskStep = $derived(steps.some((st) => st.kind === 'task'))
  const taskList = watchTasks(() => (more || hasTaskStep ? where : null))
  const tasks = $derived(taskList.list)
  /** `tasks` are the folder's now picked (not still loading, nor another folder's). */
  const tasksReady = $derived(tasksFor(taskList, where))
  // Its task steps on the folder's tasks: loaded after the step was added, or another folder picked.
  $effect(() => {
    if (!tasksReady) return
    for (const st of steps) if (st.kind === 'task' && !tasks.includes(st.command ?? '')) st.command = testTask(tasks)
  })
  /** A task step names one of the folder's tasks (`phone_then` refuses any other). */
  const stepsOk = $derived(!hasTaskStep || (tasksReady && steps.every((st) => st.kind !== 'task' || (!!st.command && tasks.includes(st.command)))))
  const ready = $derived((whereTo === 'new' ? !!repo && !plan?.error : !!cwd) && stepsOk)
  const addStep = (kind: StepKind) => steps.push(newStep(kind, testTask(tasks)))
  function addTag(input: HTMLInputElement) {
    const tag = input.value.trim().replace(/^#/, '')
    input.value = ''
    if (tag && !tags.includes(tag)) tags = [...tags, tag]
  }

  /** Launched (its Chat opens), or prepared for the computer (said by a toast). */
  async function go(prepared = false) {
    if (busy || !ready) return
    busy = true
    error = ''
    try {
      const id = await (prepared ? prepareAgent : spawnAgent)(launch)
      setPref('phone.launch.mode', mode)
      if (repo) setPref('phone.launch.repo', repo)
      const effect = launcherDrawer.onLaunch
      launcherDrawer.onLaunch = null
      att.clear()
      closeLauncherDrawer()
      if (prepared) toastLaunched(id, launch.title, true)
      else openAgents(id)
      void afterLaunch(effect, prepared ? t('Agent prepared') : t('Agent started'))
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  /** The keyboard out at once: the prompt is what it opens for. */
  const focus = (el: HTMLTextAreaElement) => {
    el.focus()
    const end = el.value.length
    el.setSelectionRange(end, end)
  }
</script>

<!-- A tap beside it does not close it: a prompt being written is not lost to a stray thumb. -->
<div class="scrim" role="presentation"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-label={t('New session')}>
  <div class="grab"></div>
  <header>
    <b>{t('New session')}</b>
    {#if launcherDrawer.origin}<span class="origin">{launcherDrawer.origin}</span>{/if}
    <span class="grow"></span>
    <button class="x" onclick={closeLauncherDrawer} aria-label={t('Close')}><Icon name="close" /></button>
  </header>
  <div class="scroll">
    <div class="prompt">
      <textarea bind:value={prompt} {@attach focus} placeholder={t('What should it do?')} rows="3" enterkeyhint="enter" onpaste={(e) => { const files = filesOf(e.clipboardData?.files); if (files.length) { e.preventDefault(); void attach(files) } }}></textarea>
      {#if att.list.length}
        <div class="thumbs">
          {#each att.list as a, i (a.path)}
            <span class="thumb">{#if a.url}<img src={a.url} alt={a.name} />{:else}<Icon name="files" />{/if}<span>{a.name}</span><button type="button" onclick={() => detach(i)} aria-label={t('Remove {name}', { name: a.name })}>×</button></span>
          {/each}
        </div>
      {/if}
      <div class="tools">
        <button type="button" disabled={att.busy} onclick={() => photoInput?.click()} aria-label={t('Take a photo')}><Icon name="camera" /></button>
        <button type="button" disabled={att.busy} onclick={() => fileInput?.click()} aria-label={t('Attach photos or files')}><Icon name="attach" /></button>
        <span class="grow"></span>
        {#if att.busy}<small>{t('Attaching…')}</small>{:else}<small>{t('The keyboard’s mic dictates')}</small>{/if}
      </div>
      <input bind:this={photoInput} type="file" accept="image/*" capture="environment" hidden onchange={(e) => { void attach(filesOf(e.currentTarget.files)); e.currentTarget.value = '' }} />
      <input bind:this={fileInput} type="file" multiple hidden onchange={(e) => { void attach(filesOf(e.currentTarget.files)); e.currentTarget.value = '' }} />
    </div>

    <section>
      <div class="lbl">{t('Where')}</div>
      <div class="chips">
        {#each chips as r (r.name)}<button type="button" class="chip" class:on={whereTo !== 'folder' && repo === r.name} onclick={() => pickRepo(r.name)}>{r.name}</button>{/each}
        {#if ordered.length > SHOWN}<button type="button" class="chip" onclick={() => (showRepos = !showRepos)} aria-expanded={showRepos}>{showRepos ? t('Fewer') : `+${ordered.length - SHOWN}`}</button>{/if}
        {#if whereTo === 'folder'}<span class="chip on">{baseName(folder)}</span>{/if}
      </div>
      <div class="where"><Icon name={whereTo === 'new' ? 'branch' : 'folder'} size="13px" /><span>{whereLine}</span>{#if whereTo !== 'folder'}<button type="button" onclick={() => (more = true)}>{t('Change')}</button>{/if}</div>
      {#if whereTo === 'new' && plan?.taken}<small class="muted">{t('{reason}: {name} instead', { reason: plan.taken, name: plan.name })}</small>{/if}
    </section>

    <section>
      <div class="lbl">{t('With')}</div>
      <div class="chips">
        {#each Object.entries(CLIS) as [p, c] (p)}
          <button type="button" class="chip cli" class:on={provider === p} style:--c={c.color} onclick={() => (picked = p === providerOf(repo || undefined) ? '' : (p as AgentProvider))}><i></i>{c.name}{#if provider === p && (model.trim() || cliModel)}<small>{model.trim() || cliModel}</small>{/if}</button>
        {/each}
      </div>
    </section>

    <section>
      <div class="lbl">{t('Mode')}</div>
      <div class="seg" role="radiogroup">
        {#each MODES as [m, label] (m)}<button type="button" role="radio" aria-checked={mode === m} class:on={mode === m} onclick={() => (mode = m)}>{label}</button>{/each}
      </div>
      <p class="hint">{MODES.find(([m]) => m === mode)?.[2]}</p>
    </section>

    <button type="button" class="more" onclick={() => (more = !more)} aria-expanded={more}>
      <span>{t('More options')}</span><span class="grow"></span><span class="muted">{more ? '' : t('worktree · model · kind · tags · then')}</span><Icon name={more ? 'chevronDown' : 'chevronRight'} />
    </button>

    {#if more}
      <div class="adv">
        {#if whereTo !== 'folder'}
          <div>
            <div class="lbl">{t('Worktree')}</div>
            <div class="seg">
              <button type="button" class:on={whereTo === 'new'} disabled={!hasWorktrees(repo)} onclick={() => (whereTo = 'new')}>{t('New')}</button>
              <button type="button" class:on={whereTo === 'existing'} disabled={!worktrees.length} onclick={() => (whereTo = 'existing')}>{t('Existing')}</button>
              <button type="button" class:on={whereTo === 'clone'} onclick={() => (whereTo = 'clone')}>{t('The clone')}</button>
            </div>
          </div>
        {/if}
        {#if whereTo === 'new'}
          <label class="f"><span>{t('Name')}</span><input bind:value={name} placeholder={plan?.name ?? (slug(title) || autoName || 'agent')} spellcheck="false" autocapitalize="off" autocomplete="off" /></label>
          <label class="f"><span>{t('Branch')}</span><input bind:value={branch} placeholder={plan?.branch ?? t('new')} spellcheck="false" autocapitalize="off" autocomplete="off" /></label>
          {#if !branch}<label class="f"><span>{t('From')}</span><input bind:value={base} list="pl-bases" placeholder={plan?.base ?? 'main'} spellcheck="false" autocapitalize="off" autocomplete="off" /></label>
            <datalist id="pl-bases">{#each plan?.bases ?? [] as b (b)}<option value={b}></option>{/each}</datalist>{/if}
        {:else if whereTo === 'existing'}
          <label class="f"><span>{t('Worktree')}</span><select bind:value={wtPath}>{#each worktrees as w (w.path)}<option value={w.path}>{baseName(w.path)} · {w.branch ?? t('detached')}{w.dirty ? ` · ${t('changes')}` : ''}</option>{/each}</select></label>
        {/if}
        <label class="f"><span>{t('Model')}</span><input value={model} oninput={(e) => (models[provider] = e.currentTarget.value)} list="pl-models" placeholder={cliModel ?? t('{cli}’s default', { cli: cliOf(provider).name })} spellcheck="false" autocapitalize="off" autocomplete="off" /></label>
        <datalist id="pl-models">{#each choices.models[provider] ?? [] as m (m)}<option value={m}></option>{/each}</datalist>
        {#if takesEffort}
          <label class="f"><span>{t('Effort')}</span>
            <select value={effort} onchange={(e) => (efforts[provider] = e.currentTarget.value)}>
              <option value="">{agentsSettings()[provider]?.effort ?? t('{cli}’s default', { cli: cliOf(provider).name })}</option>
              {#each effort && !effortChoices.includes(effort) ? [...effortChoices, effort] : effortChoices as e (e)}<option value={e}>{e}</option>{/each}
            </select>
          </label>
        {/if}
        {#if kinds.length}
          <div>
            <div class="lbl">{t('Kind')}</div>
            <div class="chips wrap">
              <button type="button" class="chip" class:on={!category} onclick={() => (category = '')}>{t('Auto')}</button>
              {#each kinds as k (k)}<button type="button" class="chip" class:on={category === k} onclick={() => (category = k)}>{k}</button>{/each}
            </div>
          </div>
        {/if}
        <div>
          <div class="lbl">{t('Tags')}</div>
          <div class="chips wrap">
            {#each tags as tag (tag)}<button type="button" class="chip on" onclick={() => (tags = tags.filter((x) => x !== tag))} aria-label={t('Remove tag {tag}', { tag })}>#{tag} ×</button>{/each}
            {#each (agentsSettings().tags ?? []).filter((tag) => !tags.includes(tag)) as tag (tag)}<button type="button" class="chip" onclick={() => (tags = [...tags, tag])}>+ #{tag}</button>{/each}
            <input class="tag" placeholder={t('+ tag')} spellcheck="false" autocapitalize="off" enterkeyhint="done" onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addTag(e.currentTarget) } }} onchange={(e) => addTag(e.currentTarget)} />
          </div>
        </div>
        <div>
          <div class="lbl">{t('Then')}</div>
          {#each steps as s, i (i)}
            <div class="step">
              <div class="sh"><span class="n">{i + 1}</span><span class="grow">{STEP_LABEL[s.kind]}</span><button type="button" onclick={() => steps.splice(i, 1)} aria-label={t('Remove step {n}', { n: i + 1 })}>×</button></div>
              {#if s.kind === 'task'}<select bind:value={s.command} aria-label={t('Task')}>{#each tasks as tk (tk)}<option value={tk}>{tk}</option>{:else}<option value="">{t('No task in this folder')}</option>{/each}</select>
              {:else if s.kind === 'prompt' || s.kind === 'agent'}<input bind:value={s.text} placeholder={s.kind === 'agent' ? t('Its prompt: {mr}, {ticket}, {branch} filled in', { mr: '{mr}', ticket: '{ticket}', branch: '{branch}' }) : t('Sent to this agent when it waits for you')} spellcheck="false" aria-label={t('Prompt')} />{/if}
              <select bind:value={s.when} aria-label={t('When')}>{#each Object.entries(WHEN_LABEL) as [w, label] (w)}<option value={w}>{label}</option>{/each}</select>
            </div>
          {/each}
          <div class="chips wrap">{#each STEP_KINDS as k (k)}<button type="button" class="chip" onclick={() => addStep(k)}>+ {STEP_LABEL[k]}</button>{/each}</div>
          {#if steps.length}<p class="hint">{t('The computer runs them; a task needs critical actions on.')}</p>{/if}
          {#if tasksReady && !stepsOk}<p class="err">{t('No task in this folder')}</p>{/if}
        </div>
      </div>
    {/if}
    {#if whereTo === 'new' && plan?.error}<p class="err">{plan.error}</p>{/if}
    {#if error}<p class="err">{error}</p>{/if}
  </div>
  <footer>
    <button type="button" class="prep" disabled={busy || !ready} onclick={() => go(true)} title={t('Kept in the list, started from the computer or here later')}>{t('Prepare')}</button>
    <button type="button" class="launch" disabled={busy || !ready} onclick={() => go()}>{busy ? t('Launching…') : t('Launch')}<small>{summary}</small></button>
  </footer>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 60; background: rgb(0 0 0 / 0.45); animation: fade 0.18s ease-out; }
  .sheet { position: fixed; left: 0; right: 0; bottom: 0; z-index: 61; display: flex; flex-direction: column; max-height: calc(100dvh - 24px - env(safe-area-inset-top)); border-radius: 18px 18px 0 0; background: var(--bg); box-shadow: 0 -8px 30px rgb(0 0 0 / 0.25); animation: rise 0.22s ease-out; font-size: 15px; }
  @keyframes rise { from { transform: translateY(40px); opacity: 0; } }
  @keyframes fade { from { opacity: 0; } }
  :global(html[data-reduce-motion]) .sheet, :global(html[data-reduce-motion]) .scrim { animation: none; }
  .grab { width: 38px; height: 5px; margin: 8px auto 0; border-radius: 3px; background: var(--border); }
  header { display: flex; align-items: center; gap: 8px; padding: 6px 10px 6px 16px; }
  header b { font-size: 17px; }
  .origin { padding: 2px 8px; border-radius: 10px; background: var(--surface-muted); color: var(--info); font: 12px var(--mono); }
  .grow { flex: 1; min-width: 0; }
  .x { display: inline-flex; padding: 10px; border: none; background: none; color: var(--text-muted); }
  .scroll { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; display: flex; flex-direction: column; gap: 16px; padding: 4px 16px 16px; }
  .prompt { border: 1px solid var(--border); border-radius: 14px; background: var(--surface); }
  .prompt:focus-within { border-color: var(--accent); }
  .prompt textarea { display: block; width: 100%; min-height: 84px; max-height: 40dvh; field-sizing: content; resize: none; padding: 12px 14px 4px; border: none; background: none; color: var(--text); font: inherit; font-size: 16px; line-height: 1.4; outline: none; }
  .tools { display: flex; align-items: center; gap: 4px; padding: 2px 6px 6px; color: var(--text-muted); }
  .tools button { display: inline-flex; padding: 9px; border: none; border-radius: 10px; background: none; color: var(--text-muted); }
  .tools button:disabled { opacity: 0.5; }
  .tools small { padding-right: 6px; font-size: 12px; }
  .thumbs { display: flex; gap: 6px; padding: 4px 10px; overflow-x: auto; scrollbar-width: none; }
  .thumb { flex: none; display: inline-flex; align-items: center; gap: 6px; max-width: 180px; padding: 3px 4px 3px 3px; border: 1px solid var(--border); border-radius: 10px; font-size: 12px; }
  .thumb img { width: 32px; height: 32px; border-radius: 7px; object-fit: cover; }
  .thumb span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .thumb button { padding: 4px 6px; border: none; background: none; color: var(--text-muted); font-size: 16px; }
  section { display: flex; flex-direction: column; gap: 8px; }
  .lbl { color: var(--text-muted); font-size: 12px; letter-spacing: 0.05em; text-transform: uppercase; }
  .chips { display: flex; gap: 8px; overflow-x: auto; scrollbar-width: none; margin: 0 -16px; padding: 0 16px; }
  .chips.wrap { flex-wrap: wrap; overflow: visible; margin: 0; padding: 0; }
  .chip { flex: none; display: inline-flex; align-items: center; gap: 6px; min-height: 40px; padding: 0 14px; border: 1px solid var(--border); border-radius: 20px; background: var(--surface); color: var(--text); font: inherit; font-size: 15px; }
  .chip.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, var(--surface)); font-weight: 600; }
  .chip small { color: var(--text-muted); font-weight: 400; font-size: 12px; }
  .chip.cli i { width: 9px; height: 9px; border-radius: 50%; background: var(--c); }
  .where { display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: 13px; }
  .where span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .where button { flex: none; padding: 4px 2px; border: none; background: none; color: var(--accent); font: inherit; font-size: 13px; }
  .seg { display: flex; padding: 3px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); }
  .seg button { flex: 1; min-height: 38px; padding: 0 6px; border: none; border-radius: 9px; background: none; color: var(--text-muted); font: inherit; font-size: 14px; }
  .seg button.on { background: var(--bg); color: var(--text); font-weight: 600; box-shadow: 0 1px 3px rgb(0 0 0 / 0.15); }
  .seg button:disabled { opacity: 0.4; }
  .hint { margin: 0; color: var(--text-muted); font-size: 13px; }
  .more { display: flex; align-items: center; gap: 8px; min-height: 46px; padding: 0 12px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); color: var(--text); font: inherit; font-size: 15px; }
  .more > span:first-child { flex: none; white-space: nowrap; }
  .more .muted { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12.5px; }
  .muted { color: var(--text-muted); }
  .adv { display: flex; flex-direction: column; gap: 14px; }
  .f { display: flex; align-items: center; gap: 10px; min-height: 46px; padding: 0 12px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); }
  .f span { flex: none; width: 74px; color: var(--text-muted); font-size: 14px; }
  .f input, .f select { flex: 1; min-width: 0; height: 44px; border: none; background: none; color: var(--text); font: inherit; font-size: 16px; outline: none; }
  .tag { width: 110px; min-height: 40px; padding: 0 12px; border: 1px dashed var(--border); border-radius: 20px; background: none; color: var(--text); font: inherit; font-size: 16px; }
  .step { display: flex; flex-direction: column; gap: 6px; margin-bottom: 8px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); }
  .step .sh { display: flex; align-items: center; gap: 8px; font-size: 14px; }
  .step .n { display: inline-grid; place-items: center; width: 20px; height: 20px; border-radius: 50%; background: var(--surface-muted); font-size: 12px; }
  .step .sh button { padding: 4px 8px; border: none; background: none; color: var(--text-muted); font-size: 18px; }
  .step select, .step input { min-height: 40px; padding: 0 8px; border: 1px solid var(--border); border-radius: 9px; background: var(--bg); color: var(--text); font: inherit; font-size: 16px; }
  .err { margin: 0; color: var(--ko); font-size: 13px; overflow-wrap: anywhere; }
  footer { display: flex; gap: 10px; padding: 10px 16px calc(10px + env(safe-area-inset-bottom)); border-top: 1px solid var(--border); background: var(--bg); }
  footer button { min-height: 54px; border-radius: 14px; font: inherit; }
  .prep { flex: none; padding: 0 16px; border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 15px; }
  .launch { flex: 1; min-width: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 1px; padding: 0 12px; border: none; background: var(--accent); color: var(--on-accent, #fff); font-size: 17px; font-weight: 650; }
  .launch small { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; font-weight: 400; opacity: 0.85; }
  footer button:disabled { opacity: 0.5; }
</style>
