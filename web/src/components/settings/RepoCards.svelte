<script lang="ts">
  import { baseName } from '../../platform'
  import Icon from '../Icon.svelte'
  import Control from './Control.svelte'
  import CheckBadge from './CheckBadge.svelte'
  import PipelineBranchesField from './PipelineBranchesField.svelte'
  import ArmedLabel from '../ArmedLabel.svelte'
  import { armer } from '../../armed.svelte'
  import { t, tc } from '../../i18n.svelte'
  import { canPick, pickPath } from '../../pick'
  import { live } from '../../live.svelte'
  import { cloneRepo, notCloneOf, repoAdd } from '../../clones.svelte'
  import { forgeLabel, profileForge } from '../../forge'
  import { requestOf } from '../../settings-checks.svelte'
  import { AGENT_CLIS } from '../../settings-schema'
  import { fromPack, mergedRows, setMergedRows, setValue, valueOf } from '../../settings.svelte'
  import { forgeOfRow, readersOf, repoBlockLabel, tilde, workspaceOf } from './repo-blocks'
  import type { ForgeProvider } from '../../bindings/ForgeProvider'

  /**
   * Settings › Repositories: the `[[repos]]` as cards, grouped by working folder (`[[workspaces]]`,
   * the folder their clone is in), then those without a clone (their MRs only) and those whose
   * clone is elsewhere. A card folds to one line, its switches as chips; open, its identity, what
   * Prism reads of it on this computer (needs the clone) and on the forge (needs the project), the
   * blocks that read it and its branches. A working folder's header holds its own keys and adds a
   * clone or a project there.
   */
  type Row = Record<string, unknown>
  const REPOS = ['repos']
  const rows = $derived(mergedRows(REPOS))
  const str = (v: unknown) => (typeof v === 'string' ? v : '')

  /* ---------- Working folders ---------- */
  const WORKSPACES = ['workspaces']
  const wsRows = $derived(valueOf<Row[] | null>(WORKSPACES, null) ?? [])
  const wsName = (w: Row) => str(w.name).trim() || baseName(tilde(str(w.dir))) || t('New folder')
  const wsForge = (w: Row): ForgeProvider => (w.forge === 'github' || w.forge === 'gitlab' ? w.forge : profileForge())
  function setWorkspace(i: number, patch: Row) {
    setValue(WORKSPACES, wsRows.map((w, n) => (n === i ? clean({ ...w, ...patch }) : w)))
  }
  const armed = armer()
  function removeWorkspace(i: number) {
    if (!armed.confirm(`ws:${i}`)) return
    const next = wsRows.filter((_, n) => n !== i)
    setValue(WORKSPACES, next.length ? next : null)
    repoAdd.editing = null
  }
  /** Clone here / New project: the Add panel below, on this folder. */
  function addIn(w: Row, mode: 'clone' | 'new') {
    repoAdd.workspace = tilde(str(w.dir))
    repoAdd.mode = mode
    requestAnimationFrame(() => document.getElementById('repo-add')?.scrollIntoView({ block: 'center', behavior: 'smooth' }))
  }

  /** The groups: each working folder's clones, then those without a clone, then the others. */
  type Group = { key: string; ws?: number; label: string; meta: string; rows: Row[] }
  const groups = $derived.by((): Group[] => {
    if (!wsRows.length) return [{ key: 'all', label: '', meta: '', rows }]
    const out: Group[] = wsRows.map((w, i) => ({ key: `ws${i}`, ws: i, label: wsName(w), meta: '', rows: rows.filter((r) => !!workspaceOf(r) && tilde(str(workspaceOf(r)!.dir)) === tilde(str(w.dir))) }))
    const remote = rows.filter((r) => !str(r.path))
    const elsewhere = rows.filter((r) => str(r.path) && !workspaceOf(r))
    if (remote.length) out.push({ key: 'remote', label: t('Forge only'), meta: t('No clone on this machine: their MRs and pipelines only.'), rows: remote })
    if (elsewhere.length) out.push({ key: 'elsewhere', label: t('Elsewhere'), meta: t('Clones outside your working folders: they work the same.'), rows: elsewhere })
    return out
  })

  /* ---------- A repository ---------- */
  const PRIMARY = ['forge', 'bare_mr_repo']
  /** The repository bare MR references name: `[forge] bare_mr_repo`, else the entry with `primary = true`. */
  const bareRepo = $derived(valueOf<string>(PRIMARY, '') || str(rows.find((r) => r.primary)?.name))
  const info = (name: string) => live.snapshot?.repositories.find((r) => r.name === name)
  const forgeOf = forgeOfRow
  /** On another forge than the profile's: only its MRs are read, with that forge's tool. */
  const foreign = (r: Row) => forgeOf(r) !== profileForge()
  function clean(r: Row): Row {
    return Object.fromEntries(Object.entries(r).filter(([, v]) => v !== null && v !== undefined && v !== ''))
  }
  function update(name: unknown, patch: Row) {
    setMergedRows(REPOS, rows.map((r) => (r.name === name ? clean({ ...r, ...patch }) : r)))
  }
  function remove(r: Row) {
    if (armed.confirm(`repo:${str(r.name)}`)) setMergedRows(REPOS, rows.filter((x) => x !== r))
  }

  let open = $state(new Set<string>())
  function toggle(name: string) {
    const next = new Set(open)
    if (next.has(name)) next.delete(name)
    else next.add(name)
    open = next
  }

  /** The clone given to a repository without one: a folder chosen (checked), or the project cloned. */
  let busy = $state<Record<string, string>>({})
  let errors = $state<Record<string, string>>({})
  async function choose(r: Row) {
    const path = await pickPath('~')
    if (!path) return
    const why = await notCloneOf(path, str(r.project) || null).catch((e) => String(e))
    errors = { ...errors, [str(r.name)]: why ? `${path}: ${why}` : '' }
    if (!why) update(r.name, { path })
  }
  const cloneTarget = $derived(wsRows.find((w) => str(w.dir).trim()))
  async function cloneInto(r: Row) {
    const parent = cloneTarget ? str(cloneTarget.dir) : await pickPath('~')
    if (!parent || !str(r.project)) return
    const name = str(r.name)
    busy = { ...busy, [name]: t('Cloning…') }
    errors = { ...errors, [name]: '' }
    try {
      update(r.name, { path: await cloneRepo(str(r.project), parent, forgeOf(r)) })
    } catch (e) {
      errors = { ...errors, [name]: String(e) }
    } finally {
      const { [name]: _, ...rest } = busy
      busy = rest
    }
  }

  /** What a branch field takes when empty: `[forge]`'s or `[worktrees]`' value, or the remote's. */
  const forgeMain = $derived(valueOf<string>(['forge', 'main_branch'], ''))
  const forgeRelease = $derived(valueOf<string>(['forge', 'release_prefix'], ''))
  const worktreeBase = $derived(valueOf<string>(['worktrees', 'base_branch'], 'auto'))
  const forgePipelineBranches = $derived(valueOf<string[] | null>(['forge', 'pipeline_branches'], null) ?? [])

  const SWITCHES_HERE = [
    { key: 'worktrees', label: t('Worktrees'), hint: t('Listed in Worktrees; agents create theirs there, Claude actions run on its MRs.') },
  ]
  const SWITCHES_FORGE = [
    { key: 'merge_requests', label: t('Merge requests'), hint: t('Its MRs in the Merge requests block.') },
    { key: 'pipelines', label: t('Pipelines'), hint: t('Its followed branches’ pipelines (main and release ones, or those listed in Branches) and waiting jobs: branch banner, CI alerts, header.') },
    { key: 'merge_queue', label: t('Merge queue'), hint: t('The merge queue label on its MRs (a CI job watches it).') },
    { key: 'ticket_required', label: t('Ticket required'), hint: t('Alerts flags its MRs without a ticket id in their title or branch.') },
  ]
  /** A switch a repository on another forge cannot have: Prism reads only its MRs there. */
  const forgeOnly = (key: string) => key === 'pipelines' || key === 'merge_queue'
  const chipsOf = (r: Row) => [...SWITCHES_FORGE, ...SWITCHES_HERE].filter((s) => !!r[s.key] || (s.key === 'merge_requests' && str(r.name) === bareRepo))
</script>

{#snippet card(r: Row)}
  {@const name = str(r.name)}
  {@const isOpen = open.has(name)}
  {@const pack = fromPack(REPOS, r)}
  {@const path = str(r.path)}
  {@const project = str(r.project)}
  {@const readers = readersOf(name, bareRepo)}
  {@const bare = name === bareRepo}
  {@const missing = info(name)?.clone_missing}
  <div class="repo" class:open={isOpen}>
    <button class="head" onclick={() => toggle(name)} aria-expanded={isOpen}>
      <span class="glyph" class:bare title={bare ? t('Bare MR references (!1294) name this repository') : undefined}><Icon name={bare ? 'star' : 'branch'} /></span>
      <span class="rname">
        <b>{name || t('(no name)')}</b>
        <small class="mono">{path ? (workspaceOf(r) ? tilde(path).slice(tilde(str(workspaceOf(r)!.dir)).length + 1) || tilde(path) : tilde(path)) : t('no clone: MRs only')}</small>
      </span>
      <span class="chips">
        {#if project}<span class="chip forge {forgeOf(r)}">{#if foreign(r)}{forgeLabel(forgeOf(r))}{' · '}{/if}{project}</span>{/if}
        {#each chipsOf(r) as s (s.key)}<span class="chip">{s.label}</span>{/each}
        {#if r.agent}<span class="chip">{t('Agent: {cli}', { cli: str(r.agent) })}</span>{/if}
        {#each readers as b (b)}<span class="chip blk">{repoBlockLabel(b)}</span>{/each}
        {#if pack}<span class="chip pack">pack</span>{/if}
        {#if missing}<span class="chip warn" title={t('Its folder does not exist: moved, or never cloned there')}>{t('Clone missing')}</span>
        {:else if pack && !path}<span class="chip warn">{t('Clone to find')}</span>{/if}
      </span>
      <span class="car" class:on={isOpen}><Icon name="chevronRight" /></span>
    </button>
    {#if isOpen}
      <div class="body">
        <div class="cols">
          <section>
            <h5>{t('Identity')}</h5>
            <p class="req">{t('Where it is, on this machine and on the forge.')}</p>
            <label class="id"><span>{t('Name')}</span>
              {#if pack}<span class="mono fixed" title={t('A pack’s repository keeps its name: config.toml’s entry merges with it by name')}>{name}</span>
              {:else}<Control kind="text" value={name} onchange={(v) => update(r.name, { name: v })} compact />{/if}
            </label>
            <div class="id"><span>{t('Clone')}</span>
              <span class="ctl"><Control kind="path" value={path || null} onchange={(v) => update(r.name, { path: v })} optional placeholder={t('none: MRs only')} compact /><CheckBadge req={requestOf('git_repo', path || null, (k) => r[k])} compact /></span>
            </div>
            <div class="id"><span>{t('Project')}</span>
              <span class="ctl"><Control kind="text" value={project || null} onchange={(v) => update(r.name, { project: v })} optional placeholder="group/project" compact />{#if !foreign(r)}<CheckBadge req={requestOf('gitlab_project', project || null, (k) => r[k])} compact />{/if}</span>
            </div>
            <div class="id"><span>{t('Forge')}</span>
              <Control kind="select" value={(r.forge as string | undefined) ?? null} onchange={(v) => update(r.name, { forge: v })} options={['gitlab', 'github']} optional placeholder={workspaceOf(r) ? t('{forge} · from {folder}', { forge: forgeLabel(forgeOf(r)), folder: wsName(workspaceOf(r)!) }) : t('{forge} · the profile’s', { forge: forgeLabel(profileForge()) })} compact />
            </div>
          </section>

          <section class:locked={!path}>
            <h5><Icon name="code" /> {t('On this machine')}</h5>
            <p class="req">{path ? t('Needs the clone ✓') : t('Needs the clone')}</p>
            {#each SWITCHES_HERE as s (s.key)}
              <div class="sw"><Control kind="toggle" value={!!r[s.key]} onchange={(v) => update(r.name, { [s.key]: v || null })} /><span><b>{s.label}</b><small>{s.hint}</small></span></div>
            {/each}
            <div class="sw"><Control kind="select" value={(r.agent as string | undefined) ?? null} onchange={(v) => update(r.name, { agent: v })} options={[...AGENT_CLIS]} optional placeholder={tc('settings', 'default')} compact /><span><b>{t('Agent CLI')}</b><small>{t('Of the agents launched in its clone or its worktrees; default: Agents › Agent CLI.')}</small></span></div>
            {#if !path}
              <div class="lock">
                {t('No clone on this machine.')}
                {#if canPick()}<button class="btn" onclick={() => choose(r)}>{t('Choose a folder…')}</button>{/if}
                {#if project}<button class="btn" onclick={() => cloneInto(r)} disabled={!!busy[name]}>{busy[name] ?? (cloneTarget ? t('Clone into {folder}', { folder: wsName(cloneTarget) }) : t('Clone…'))}</button>{/if}
              </div>
            {/if}
            {#if errors[name]}<p class="err">{errors[name]}</p>{/if}
          </section>

          <section class:locked={!project}>
            <h5><Icon name="mr" /> {t('On the forge')}</h5>
            <p class="req">{project ? t('Needs the project ✓') : t('Needs the project')}</p>
            {#each SWITCHES_FORGE as s (s.key)}
              {@const forced = s.key === 'merge_requests' && bare}
              {@const barred = foreign(r) && forgeOnly(s.key)}
              <div class="sw" class:barred>
                {#if forced}<span class="forced" title={t('Always read: bare MR references name this repository')}><Control kind="toggle" value={true} onchange={() => {}} /></span>
                {:else}<Control kind="toggle" value={!!r[s.key] && !barred} onchange={(v) => !barred && update(r.name, { [s.key]: v || null })} />{/if}
                <span><b>{s.label}</b><small>{forced ? t('Always on: bare MR references (!1294) name this repository.') : barred ? t('Not on {forge}: Prism reads only the MRs of a project on another forge than the profile’s.', { forge: forgeLabel(forgeOf(r)) }) : s.hint}</small></span>
              </div>
            {/each}
            {#if !project}<div class="lock">{t('No project: type its path above (group/project, owner/repo).')}</div>{/if}
          </section>
        </div>

        <div class="row2">
          <h5><Icon name="chart" /> {t('Read by')}</h5>
          <span class="chips">
            {#if bare}<span class="chip bare"><Icon name="star" /> {t('Bare MR references')}</span>{/if}
            {#each readers as b (b)}<span class="chip blk">{repoBlockLabel(b)}</span>{:else}{#if !bare}<span class="none">{t('no block: see “Blocks that read a repository” below')}</span>{/if}{/each}
          </span>
        </div>

        <div class="row2">
          <h5><Icon name="branch" /> {t('Branches')} <small>{t('empty: the value shown, from Forge and Worktrees, or the remote’s')}</small></h5>
          <div class="branches">
            <label><span>{t('Main branch')}</span><Control kind="text" value={(r.main_branch as string | undefined) ?? null} onchange={(v) => update(r.name, { main_branch: v })} optional placeholder={r.main_branch === '' ? t('the remote’s default, set here') : forgeMain ? t('{branch} · from Forge', { branch: forgeMain }) : t('the remote’s default')} compact /></label>
            <label><span>{t('Release prefix')}</span><Control kind="text" value={(r.release_prefix as string | undefined) ?? null} onchange={(v) => update(r.name, { release_prefix: v })} optional placeholder={r.release_prefix === '' ? t('none, set here') : forgeRelease ? t('{prefix} · from Forge', { prefix: forgeRelease }) : t('none')} compact /></label>
            <label><span>{t('Worktree base')}</span><Control kind="text" value={(r.base_branch as string | undefined) ?? null} onchange={(v) => update(r.name, { base_branch: v })} optional placeholder={worktreeBase === 'auto' ? t('auto: latest release, else main') : t('{branch} · from Worktrees', { branch: worktreeBase })} compact /></label>
          </div>
          {#if r.pipelines && project && !foreign(r)}
            <PipelineBranchesField repo={name} value={(r.pipeline_branches as string[] | undefined) ?? null} fallback={forgePipelineBranches} onchange={(v) => update(r.name, { pipeline_branches: v })} />
          {/if}
        </div>

        <div class="foot">
          {#if r.primary && !valueOf<string>(PRIMARY, '')}<span class="note">{t('Named by primary = true, the former key: bare MR references name it.')}</span>{/if}
          <span class="grow"></span>
          <button class="btn danger" class:armed={armed.is(`repo:${name}`)} onclick={() => remove(r)} title={pack ? t('Drops what config.toml changes in this pack’s repository') : t('Remove this repository (click twice)')}><ArmedLabel label={pack ? t('Back to the pack’s values') : t('Remove')} armedLabel={t('Click again')} armed={armed.is(`repo:${name}`)} /></button>
        </div>
      </div>
    {/if}
  </div>
{/snippet}

<div class="cards">
  {#each groups as g (g.key)}
    <div class="group" id={g.ws !== undefined ? `ws-${g.ws}` : undefined} class:ws={g.ws !== undefined} class:plain={g.ws === undefined && g.key !== 'all'}>
      {#if g.ws !== undefined}
        {@const w = wsRows[g.ws]}
        <div class="ghead">
          <Icon name="folder" />
          <span class="gt">
            <b>{g.label}{#if g.ws === 0}<span class="def">{t('default')}</span>{/if}</b>
            <small><span class="mono">{tilde(str(w.dir)) || t('no folder yet')}</span> · {str(w.namespace) ? str(w.namespace) : t('your account')} · <span class="chip forge {wsForge(w)}">{forgeLabel(wsForge(w))}{#if !w.forge}{' '}{t('(profile)')}{/if}</span></small>
          </span>
          <button class="btn ghost" class:on={repoAdd.editing === g.ws} onclick={() => (repoAdd.editing = repoAdd.editing === g.ws ? null : g.ws!)} title={t('Edit this working folder')}><Icon name="edit" /></button>
          {#if str(w.dir)}
            <button class="btn" onclick={() => addIn(w, 'clone')}><Icon name="down" />{t('Clone here')}</button>
            <button class="btn" onclick={() => addIn(w, 'new')}><Icon name="plus" />{t('New project')}</button>
          {/if}
        </div>
        {#if repoAdd.editing === g.ws}
          {@const i = g.ws}
          <div class="wsedit">
            <label><span>{t('Name')}</span><Control kind="text" value={str(w.name) || null} onchange={(v) => setWorkspace(i, { name: v })} optional placeholder={t('folder name')} compact /></label>
            <label><span>{t('Folder')}</span><Control kind="path" value={str(w.dir) || null} onchange={(v) => setWorkspace(i, { dir: v })} placeholder="~/Development" compact /></label>
            <label><span>{t('Group')}</span><Control kind="text" value={str(w.namespace) || null} onchange={(v) => setWorkspace(i, { namespace: v })} optional placeholder={t('your account')} compact /></label>
            <label><span>{t('Forge')}</span><Control kind="select" value={(w.forge as string | undefined) ?? null} onchange={(v) => setWorkspace(i, { forge: v })} options={['gitlab', 'github']} optional placeholder={t('profile’s')} compact /></label>
            <p class="hint">{t('New projects and clones go in a working folder (the first one by default), the only folders a phone creates projects in. Group: the forge group or owner of the projects created or published from there. Forge: where they go, and that of the clones in it unless they name their own.')}</p>
            <button class="btn danger" class:armed={armed.is(`ws:${i}`)} onclick={() => removeWorkspace(i)}><ArmedLabel label={t('Remove this folder')} armedLabel={t('Click again')} armed={armed.is(`ws:${i}`)} /></button>
          </div>
        {/if}
      {:else if g.label}
        <div class="ghead"><Icon name={g.key === 'remote' ? 'web' : 'more'} /><span class="gt"><b>{g.label}</b><small>{g.meta}</small></span></div>
      {/if}
      <div class="list">
        {#each g.rows as r, i (`${i}:${str(r.name)}`)}{@render card(r)}{:else}<p class="none">{g.ws !== undefined ? t('No repository in this folder yet.') : t('None.')}</p>{/each}
      </div>
    </div>
  {/each}
</div>

<style>
  .cards { display: flex; flex-direction: column; gap: 14px; }
  .group.ws > .list { margin-left: 9px; padding-left: 14px; border-left: 2px solid var(--border); }
  .ghead { display: flex; align-items: center; gap: 8px; padding: 2px 2px 8px; color: var(--text-muted); }
  .gt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .gt b { color: var(--text); font-size: var(--fs-title); font-weight: 600; }
  .group.plain .gt b { color: var(--text-muted); }
  .gt small { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; font-size: var(--fs-meta); }
  .def { margin-left: 6px; padding: 0 6px; border-radius: var(--r9); background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); font-size: var(--fs-tag); font-weight: 500; }
  .list { display: flex; flex-direction: column; gap: 6px; }
  .none { margin: 2px 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .wsedit { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px 10px; margin: 0 0 10px 9px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); }
  .wsedit label { display: flex; flex-direction: column; gap: 3px; min-width: 0; font-size: var(--fs-meta); color: var(--text-muted); }
  .wsedit .hint { grid-column: 1 / -1; margin: 0; font-size: var(--fs-meta); color: var(--text-muted); }
  .wsedit .btn { justify-self: start; }

  .repo { border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); }
  .head { display: grid; grid-template-columns: 24px minmax(120px, 210px) minmax(0, 1fr) 16px; gap: 10px; align-items: center; width: 100%; padding: 8px 12px; border: none; border-radius: var(--r10); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .head:hover { background: var(--surface-muted, color-mix(in srgb, var(--text) 4%, transparent)); }
  .repo.open .head { border-bottom: 1px solid var(--border); border-radius: var(--r10) var(--r10) 0 0; }
  .glyph { display: grid; place-items: center; width: 24px; height: 24px; border-radius: var(--r6); background: color-mix(in srgb, var(--text) 7%, transparent); color: var(--text-muted); }
  .glyph.bare { background: var(--accent); color: var(--on-accent); }
  .rname { display: flex; flex-direction: column; min-width: 0; }
  .rname b { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rname small { color: var(--text-muted); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; min-width: 0; }
  .chip { display: inline-flex; align-items: center; gap: 4px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r9); font-size: var(--fs-tag); line-height: 17px; white-space: nowrap; color: var(--text); }
  .chip.forge::before { content: ''; width: 7px; height: 7px; border-radius: 50%; background: var(--src-gitlab); }
  .chip.forge.github::before { background: var(--text); }
  .chip.blk { border-color: transparent; background: var(--info-bg); color: var(--info); }
  .chip.pack { border-color: var(--info); color: var(--info); }
  .chip.warn { border-color: transparent; background: var(--warn-bg); color: var(--warn); }
  .chip.bare { border-color: var(--accent); color: var(--accent); }
  .car { color: var(--text-muted); transition: transform 0.15s; }
  .car.on { transform: rotate(90deg); }

  .body { padding: 12px 14px; }
  .cols { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1fr); gap: 16px; }
  h5 { display: flex; align-items: center; gap: 6px; margin: 0 0 2px; font-size: var(--fs-body); font-weight: 600; }
  h5 small { font-weight: 400; font-size: var(--fs-meta); color: var(--text-muted); }
  .req { margin: 0 0 8px; font-size: var(--fs-meta); color: var(--text-muted); }
  section.locked .sw { opacity: 0.45; pointer-events: none; }
  .id { display: grid; grid-template-columns: 64px minmax(0, 1fr); gap: 8px; align-items: center; padding: 3px 0; font-size: var(--fs-meta); color: var(--text-muted); }
  .ctl { display: flex; align-items: center; gap: 4px; min-width: 0; }
  .fixed { color: var(--text); }
  .sw { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 8px; align-items: start; padding: 4px 0; }
  .sw b { font-weight: 550; }
  .sw small { display: block; font-size: var(--fs-meta); color: var(--text-muted); }
  .sw.barred b { color: var(--text-muted); }
  .forced { opacity: 0.55; pointer-events: none; }
  .lock { margin-top: 6px; padding: 6px 8px; border-radius: var(--r6); background: var(--warn-bg); color: var(--warn); font-size: var(--fs-meta); display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .err { margin: 6px 0 0; color: var(--ko); font-size: var(--fs-meta); }
  .row2 { margin-top: 12px; padding-top: 10px; border-top: 1px dashed var(--border); }
  .row2 h5 { margin-bottom: 6px; }
  .branches { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
  .branches label { display: flex; flex-direction: column; gap: 3px; font-size: var(--fs-meta); color: var(--text-muted); }
  .foot { display: flex; align-items: center; gap: 8px; margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--border); }
  .foot .note { font-size: var(--fs-meta); color: var(--text-muted); }
  .grow { flex: 1; }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 2px 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn.ghost { border-color: transparent; background: none; color: var(--text-muted); }
  .btn.ghost.on { color: var(--accent); }
  .btn.danger { color: var(--ko); }
  .btn.danger.armed { border-color: var(--ko); background: var(--ko-bg); }
  .link { align-self: flex-start; display: inline-flex; align-items: center; gap: 6px; padding: 0; border: none; background: none; color: var(--info); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  @media (max-width: 760px) {
    .cols, .branches, .wsedit { grid-template-columns: minmax(0, 1fr); }
    .head { grid-template-columns: 24px minmax(0, 1fr) 16px; }
    .head .chips { grid-column: 2 / -1; grid-row: 2; }
  }
</style>
