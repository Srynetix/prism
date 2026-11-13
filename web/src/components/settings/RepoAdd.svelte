<script lang="ts">
  import { baseName } from '../../platform'
  import { untrack } from 'svelte'
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import { canPick, pickPath } from '../../pick'
  import { cloneRepo, createProject, initRepo, inspectFolder, publishRepo, repoAdd, sourceToAdd, type AddMode, type Folder } from '../../clones.svelte'
  import { pref, setPref } from '../../layout.svelte'
  import { forgeLabel, forgeName, profileForge } from '../../forge'
  import type { ForgeProvider } from '../../bindings/ForgeProvider'
  import { mergedRows, setMergedRows, setValue, valueOf } from '../../settings.svelte'

  /**
   * Settings › Repositories: a repository added without typing its row. An existing folder (its
   * project read from its remote; `git init` offered when it is not a repository), a project
   * cloned with glab or gh, or a new project: a folder with a git repository, a README and a first
   * commit, and on request its project on the forge, pushed. Each adds a pending row (path,
   * project, worktrees on), saved with the others; a pack's entry for the same project without a
   * clone gets the path instead. A project kept local is published later (Publish): its project
   * created on the forge, its branch pushed, its row given the project.
   */
  const REPOS = ['repos']
  const rows = $derived(mergedRows(REPOS))

  /** The working folders (`[[workspaces]]`), named (their folder's name by default). */
  type Workspace = { name: string; dir: string; namespace: string; forge: ForgeProvider }
  const workspaces = $derived(
    (valueOf<Record<string, unknown>[] | null>(['workspaces'], null) ?? [])
      .filter((w) => typeof w.dir === 'string' && w.dir.trim())
      .map((w) => {
        const dir = (w.dir as string).trim().replace(/\/+$/, '')
        return { name: (typeof w.name === 'string' && w.name.trim()) || baseName(dir) || dir, dir, namespace: typeof w.namespace === 'string' ? w.namespace.trim() : '', forge: w.forge === 'github' || w.forge === 'gitlab' ? w.forge : profileForge() }
      }),
  )
  /** The working folder a path is in. */
  const workspaceOf = (p: string) => workspaces.find((w) => p === w.dir || p.startsWith(`${w.dir}/`))
  /** Where new clones and projects go: the first working folder, else the last folder used, else where the first clone is. */
  const parentDefault = () => workspaces[0]?.dir || pref<string>('repos.parent', '') || ((rows.find((r) => typeof r.path === 'string' && r.path)?.path as string | undefined)?.replace(/\/[^/]+\/?$/, '') ?? '~')

  let busy = $state('')
  let error = $state('')
  let done = $state('')

  // Folder
  let path = $state('')
  let folder = $state<Folder | null>(null)
  let name = $state('')
  let project = $state('')
  // Clone
  let cloneProject = $state('')
  let parent = $state(parentDefault())
  /** A working folder picked: its folder, and its forge and group for a project created there. */
  function useWorkspace(w: Workspace) {
    parent = w.dir
    namespace = w.namespace
    forge = w.forge
  }
  /** The forge a clone comes from, a project is created or published on: the working folder's, else the profile's. */
  let forge = $state<ForgeProvider>(workspaceOf(parentDefault())?.forge ?? profileForge())
  /** What a project on `f` writes in its row: its MRs followed, on the profile's forge or on `f`, named. */
  const forgeKeys = (proj: string, f: ForgeProvider) => (f === profileForge() ? { project: proj, merge_requests: true } : { project: proj, merge_requests: true, forge: f })
  // New
  let newName = $state('')
  let onForge = $state(false)
  let namespace = $state(workspaceOf(parentDefault())?.namespace ?? '')
  let visibility = $state('private')
  // Publish
  /** The repositories that could go on the forge: a clone, no project. */
  const local = $derived(rows.filter((r) => typeof r.path === 'string' && r.path && !r.project))
  let publishName = $state('')

  function open(m: AddMode | null) {
    repoAdd.mode = repoAdd.mode === m ? null : m
    // A working folder set meanwhile counts.
    if (repoAdd.mode) {
      parent = parentDefault()
      namespace = workspaceOf(parent)?.namespace ?? ''
      forge = workspaceOf(parent)?.forge ?? profileForge()
    }
    error = ''
    done = ''
  }

  const lower = (v: unknown) => (typeof v === 'string' ? v.toLowerCase() : '')
  /** The entry of `proj` without a clone (a pack's, typically): it gets the folder. */
  const twinOf = (proj: string | null) => (proj ? rows.find((r) => !r.path && lower(r.project) === proj.toLowerCase()) : undefined)
  /** Why the row cannot be added: a name or a folder already in the list. */
  function clash(n: string, p: string, proj: string | null): string {
    const byPath = rows.find((r) => r.path === p)
    if (byPath) return t('{path} is “{name}” already.', { path: p, name: String(byPath.name) })
    if (twinOf(proj)) return ''
    if (!n) return t('Give it a name.')
    return rows.some((r) => r.name === n) ? t('“{name}” is in the list already: give it another name.', { name: n }) : ''
  }

  /** Adds the row, or gives its clone to the entry of the same project without one. */
  function add(n: string, p: string, proj: string | null, f: ForgeProvider = profileForge()) {
    const twin = twinOf(proj)
    if (twin) {
      setMergedRows(REPOS, rows.map((r) => (r === twin ? { ...r, path: p } : r)))
      done = t('{path} is now the clone of “{name}”.', { path: p, name: String(twin.name) })
    } else {
      const row: Record<string, unknown> = { name: n, path: p, worktrees: true, ...(proj ? forgeKeys(proj, f) : {}) }
      setMergedRows(REPOS, [...rows, row])
      done = t('“{name}” added ({path}).', { name: n, path: p })
    }
    done += ` ${t('Save: it is read at once.')}`
    repoAdd.mode = null
  }

  async function run(label: string, f: () => Promise<void>) {
    busy = label
    error = ''
    done = ''
    try {
      await f()
    } catch (e) {
      error = String(e)
    } finally {
      busy = ''
    }
  }

  async function pick(current: string, set: (p: string) => void) {
    const p = await pickPath(current || '~')
    if (p) set(p)
  }

  // A source given by Prism's assistant (its « Add the repository » card): the panel prefilled once.
  $effect(() => {
    const source = repoAdd.source
    if (!source) return
    untrack(() => {
      repoAdd.source = ''
      const { mode, value } = sourceToAdd(source)
      repoAdd.mode = mode
      if (mode === 'folder') void readFolder(value)
      else cloneProject = value
    })
  })

  // ---- Folder ----
  const readFolder = (p: string) =>
    run(t('Reading…'), async () => {
      path = p
      folder = await inspectFolder(p)
      path = folder.path
      name = folder.name
      project = folder.project ?? ''
    })
  const initHere = () => run(t('Initializing…'), async () => {
    await initRepo(path)
    folder = await inspectFolder(path)
  })
  function addFolder() {
    const why = clash(name.trim(), path, project.trim() || null)
    if (why) error = why
    else add(name.trim(), path, project.trim() || null)
  }

  // ---- Clone ----
  const cloneName = $derived(cloneProject.trim().replace(/\/+$/, '').split('/').pop() ?? '')
  const cloneAdd = () =>
    run(t('Cloning…'), async () => {
      const proj = cloneProject.trim().replace(/^\/+|\/+$/g, '')
      const why = clash(cloneName, `${parent}/${cloneName}`, proj)
      if (why) throw why
      const p = await cloneRepo(proj, parent, forge)
      setPref('repos.parent', parent)
      add(cloneName, p, proj, forge)
    })

  // ---- New ----
  const create = () =>
    run(onForge ? t('Creating on {forge}…', { forge: forgeLabel(forge) }) : t('Creating…'), async () => {
      const n = newName.trim()
      const why = clash(n, `${parent}/${n}`, null)
      if (why) throw why
      const made = await createProject(parent, n, onForge ? { namespace: namespace.trim(), visibility, forge } : null)
      setPref('repos.parent', parent)
      add(n, made.path, made.project, forge)
      if (made.warning) error = t('Created locally, not on {forge}: {warning}', { forge: forgeLabel(forge), warning: made.warning })
    })

  // A working folder's header opened the panel on it: its folder, group and forge.
  $effect(() => {
    if (!repoAdd.workspace || !repoAdd.mode) return
    const tilde = (p: string) => p.replace(/^(\/Users|\/home)\/[^/]+/, '~')
    const w = workspaces.find((x) => tilde(x.dir) === repoAdd.workspace)
    if (w) useWorkspace(w)
    repoAdd.workspace = ''
    error = ''
    done = ''
  })

  // ---- Publish ----
  // Opened from the Repos menu or the palette: the repository it names, else the first one.
  $effect(() => {
    if (repoAdd.mode === 'publish') publishName = repoAdd.publish || (local[0]?.name as string | undefined) || ''
  })
  // The group of the working folder the repository is in.
  $effect(() => {
    const row = local.find((r) => r.name === publishName)
    if (repoAdd.mode === 'publish' && row) {
      const w = workspaceOf(row.path as string)
      namespace = w?.namespace ?? ''
      forge = w?.forge ?? profileForge()
    }
  })
  const publish = () =>
    run(t('Publishing on {forge}…', { forge: forgeLabel(forge) }), async () => {
      const row = local.find((r) => r.name === publishName)
      if (!row) throw t('Pick a repository.')
      const project = await publishRepo(row.path as string, { namespace: namespace.trim(), visibility, forge })
      setMergedRows(REPOS, rows.map((r) => (r === row ? { ...r, ...forgeKeys(project, forge) } : r)))
      done = `${t('“{name}” is {project} on {forge}.', { name: String(row.name), project, forge: forgeLabel(forge) })} ${t('Save: it is read at once.')}`
      repoAdd.mode = null
    })

  /** A new working folder: its row, open in its header for its keys. */
  function addWorkspace() {
    const list = valueOf<Record<string, unknown>[] | null>(['workspaces'], null) ?? []
    setValue(['workspaces'], [...list, { dir: '' }])
    repoAdd.mode = null
    repoAdd.editing = list.length
    requestAnimationFrame(() => document.getElementById(`ws-${list.length}`)?.scrollIntoView({ block: 'center', behavior: 'smooth' }))
  }

  const MODES: { id: AddMode; label: string; hint: string }[] = [
    { id: 'folder', label: t('Add a folder…'), hint: t('A clone you already have, or any folder (git init offered)') },
    { id: 'clone', label: t('Clone a project…'), hint: t('Clone a {forge} project into a folder (glab or gh, with their login)', { forge: forgeName() }) },
    { id: 'new', label: t('New project…'), hint: t('A new folder with a git repository and a first commit; on request, its project on {forge}', { forge: forgeName() }) },
    { id: 'publish', label: t('Publish on {forge}…', { forge: forgeName() }), hint: t('A repository kept local (no remote): its project created on {forge}, its branch pushed', { forge: forgeName() }) },
  ]
</script>

<div class="add">
  <div class="modes">
    {#each MODES as m (m.id)}
      <button class="btn" class:on={repoAdd.mode === m.id} onclick={() => open(m.id)} title={m.hint} disabled={!!busy}><Icon name={m.id === 'new' ? 'plus' : m.id === 'clone' ? 'down' : m.id === 'publish' ? 'up' : 'folder'} />{m.label}</button>
    {/each}
    <span class="sep" aria-hidden="true"></span>
    <button class="btn" onclick={addWorkspace} title={t('Where your projects live (work, personal…): the list groups them by it, new projects and clones go there, with its group and forge')} disabled={!!busy}><Icon name="group" />{t('Add a working folder…')}</button>
  </div>

  {#if repoAdd.mode === 'folder'}
    <div class="panel">
      <label class="row"><span>{t('Folder')}</span>
        <input class="mono" bind:value={path} placeholder="~/dev/my-app" onchange={() => path.trim() && readFolder(path.trim())} />
        {#if canPick()}<button class="btn" onclick={() => pick(path, readFolder)} disabled={!!busy}>{t('Choose…')}</button>{/if}
      </label>
      {#if folder}
        {#if folder.git === 'repo'}
          <p class="note ok"><Icon name="check" />{folder.project ? t('A git repository, a clone of {project}.', { project: folder.project }) : t('A git repository, without a remote: its MRs cannot be read.')}</p>
        {:else if folder.git === 'worktree'}
          <p class="note warn"><Icon name="warning" />{t('A linked worktree: add its main clone instead (Prism lists its worktrees).')}</p>
        {:else}
          <p class="note warn"><Icon name="warning" />{t('Not a git repository.')}
            <button class="btn" onclick={initHere} disabled={!!busy}>{t('Initialize git here')}</button>
          </p>
        {/if}
        {#if folder.git === 'repo'}
          <label class="row"><span>{t('Name')}</span><input bind:value={name} /></label>
          <label class="row"><span>{t('Project')}</span><input class="mono" bind:value={project} placeholder={t('none: no MRs')} /></label>
          <div class="go"><button class="btn primary" onclick={addFolder} disabled={!!busy}>{t('Add to the list')}</button></div>
        {/if}
      {/if}
    </div>
  {:else if repoAdd.mode === 'clone'}
    <div class="panel">
      <label class="row"><span>{t('Project')}</span><input class="mono" bind:value={cloneProject} placeholder="group/project" /></label>
      <label class="row"><span>{t('Forge')}</span>
        <select bind:value={forge}><option value="gitlab">GitLab</option><option value="github">GitHub</option></select>
      </label>
      {#if workspaces.length}
        <div class="row"><span>{t('Working folder')}</span>
          <div class="chips">{#each workspaces as w (w.name)}<button class="btn" class:on={parent === w.dir} onclick={() => useWorkspace(w)} title={w.dir}>{w.name}</button>{/each}</div>
        </div>
      {/if}
      <label class="row"><span>{t('Into')}</span>
        <input class="mono" bind:value={parent} placeholder="~/dev" />
        {#if canPick()}<button class="btn" onclick={() => pick(parent, (p) => (parent = p))} disabled={!!busy}>{t('Choose…')}</button>{/if}
      </label>
      {#if cloneName}<p class="note">{parent}/{cloneName}</p>{/if}
      <div class="go"><button class="btn primary" onclick={cloneAdd} disabled={!!busy || !cloneName || !parent.trim()}>{busy || t('Clone and add')}</button></div>
    </div>
  {:else if repoAdd.mode === 'new'}
    <div class="panel">
      {#if workspaces.length}
        <div class="row"><span>{t('Working folder')}</span>
          <div class="chips">{#each workspaces as w (w.name)}<button class="btn" class:on={parent === w.dir} onclick={() => useWorkspace(w)} title={w.dir}>{w.name}</button>{/each}</div>
        </div>
      {/if}
      <label class="row"><span>{t('In')}</span>
        <input class="mono" bind:value={parent} placeholder="~/dev" />
        {#if canPick()}<button class="btn" onclick={() => pick(parent, (p) => (parent = p))} disabled={!!busy}>{t('Choose…')}</button>{/if}
      </label>
      <label class="row"><span>{t('Name')}</span><input class="mono" bind:value={newName} placeholder="my-app" /></label>
      {#if newName.trim()}<p class="note">{parent}/{newName.trim()}{t(': git init, a README.md, a first commit.')}</p>{/if}
      <label class="check"><input type="checkbox" bind:checked={onForge} />{t('Also create the project on {forge} and push', { forge: forgeLabel(forge) })}</label>
      {#if onForge}
      <label class="row"><span>{t('Forge')}</span>
        <select bind:value={forge}><option value="gitlab">GitLab</option><option value="github">GitHub</option></select>
      </label>
        <label class="row"><span>{forge === 'github' ? t('Owner') : t('Group')}</span><input class="mono" bind:value={namespace} placeholder={t('empty: your own account')} /></label>
        <label class="row"><span>{t('Visibility')}</span>
          <select bind:value={visibility}>
            <option value="private">private</option>
            <option value="internal">internal</option>
            <option value="public">public</option>
          </select>
        </label>
      {/if}
      <div class="go"><button class="btn primary" onclick={create} disabled={!!busy || !newName.trim() || !parent.trim()}>{busy || (onForge ? t('Create here and on {forge}', { forge: forgeLabel(forge) }) : t('Create and add'))}</button></div>
    </div>
  {:else if repoAdd.mode === 'publish'}
    <div class="panel">
      {#if local.length}
        <label class="row"><span>{t('Repository')}</span>
          <select bind:value={publishName}>
            {#each local as r (r.name)}<option value={r.name}>{r.name} · {r.path}</option>{/each}
          </select>
        </label>
        <label class="row"><span>{t('Forge')}</span>
          <select bind:value={forge}><option value="gitlab">GitLab</option><option value="github">GitHub</option></select>
        </label>
        <label class="row"><span>{forge === 'github' ? t('Owner') : t('Group')}</span><input class="mono" bind:value={namespace} placeholder={t('empty: your own account')} /></label>
        <label class="row"><span>{t('Visibility')}</span>
          <select bind:value={visibility}>
            <option value="private">private</option>
            <option value="internal">internal</option>
            <option value="public">public</option>
          </select>
        </label>
        <p class="note">{t('Its project is named after its folder, set as origin; its current branch is pushed. A repository with a remote already: give its Project in the list instead.')}</p>
        <div class="go"><button class="btn primary" onclick={publish} disabled={!!busy || !publishName}>{busy || t('Publish on {forge}', { forge: forgeLabel(forge) })}</button></div>
      {:else}
        <p class="note">{t('Every repository with a clone has its project already.')}</p>
      {/if}
    </div>
  {/if}

  {#if busy && repoAdd.mode === 'folder'}<p class="note">{busy}</p>{/if}
  {#if error}<p class="err">{error}</p>{/if}
  {#if done}<p class="note ok"><Icon name="check" />{done}</p>{/if}
</div>

<style>
  .add { display: flex; flex-direction: column; gap: 8px; }
  .modes { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .sep { width: 1px; height: 18px; margin: 0 4px; background: var(--border); }
  .panel { display: flex; flex-direction: column; gap: 6px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface-muted); }
  .row { display: grid; grid-template-columns: 90px minmax(0, 1fr) auto; align-items: center; gap: 8px; font-size: var(--fs-meta); color: var(--text-muted); }
  .row input, .row select { min-width: 0; padding: 3px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  .row select { justify-self: start; }
  .mono { font-family: var(--mono) !important; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .check { display: flex; align-items: center; gap: 6px; font-size: var(--fs-meta); }
  .note { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; margin: 0; font-size: var(--fs-meta); color: var(--text-muted); }
  .note.ok { color: var(--ok); }
  .note.warn { color: var(--warn); }
  .err { margin: 0; font-size: var(--fs-meta); color: var(--ko); }
  .go { display: flex; justify-content: flex-end; }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn:hover:not(:disabled), .btn.on { border-color: var(--accent); }
  .btn:disabled { opacity: 0.6; cursor: default; }
  .btn.primary { border-color: var(--accent); background: var(--accent); color: var(--on-accent); }
</style>
