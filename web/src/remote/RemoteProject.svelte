<script lang="ts">
  import type { ForgeProject } from '../bindings/ForgeProject'
  import type { Created } from '../bindings/Created'
  import { hasCore, invoke } from '../bridge'
  import { t } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { forgeLabel, profileForge } from '../forge'
  import type { ForgeProvider } from '../bindings/ForgeProvider'
  import Icon from '../components/Icon.svelte'

  /**
   * More › New project on a phone (`[remote] projects`): a folder with a git repository, a README
   * and a first commit in one of the Mac's working folders (`[[workspaces]]`: the phone picks one
   * and names the project, never a folder), on request its project on the forge, pushed. The Mac adds it to
   * the repositories, read when Prism restarts. A project kept local is published later from
   * here (Kept local): its project created on the forge, its branch pushed, written in its
   * `[[repos]]` entry. Or a project of the working folder's forge cloned into it (Clone), found
   * among those you are a member of. Confirmed by the passkey each time.
   */

  /** A working folder of the Mac: `dir` as config.toml writes it (`~/…`), `abs` expanded. */
  interface Workspace {
    name: string
    dir: string
    abs: string
    namespace: string
    /** Its forge, the profile's when it names none. */
    forge: ForgeProvider
  }

  /** The preview (no Mac): folders of its own, nothing created. */
  const preview = !hasCore()
  let workspaces = $state<Workspace[] | undefined>(undefined)
  /** The working folder picked, by name. */
  let picked = $state('')
  const ws = $derived(workspaces?.find((w) => w.name === picked))
  const dir = $derived(ws?.dir ?? '')
  /** The working folder a path is in. */
  const workspaceOf = (p: string) => workspaces?.find((w) => [w.dir, w.abs].some((d) => p === d || p.startsWith(`${d}/`)))
  /** The forge a repository is published on: its working folder's, else the profile's (as the Mac decides). */
  const forgeOf = (p: string): ForgeProvider => workspaceOf(p)?.forge ?? profileForge()
  function pick(w: Workspace) {
    picked = w.name
    namespace = w.namespace
  }
  let loadError = $state<string | null>(null)
  let name = $state('')
  let onForge = $state(false)
  let namespace = $state('')
  let visibility = $state('private')
  let busy = $state(false)
  let error = $state<string | null>(null)
  let made = $state<Created | null>(null)
  /** The project just created here, kept local: in Kept local before Prism restarts. */
  let createdLocal = $state<{ name: string; path: string } | null>(null)

  /** Kept local: the repositories with a clone and no project, the one just created included. */
  const local = $derived.by(() => {
    const names = (live.snapshot?.repos ?? []).filter((r) => !r.project && !r.error).map((r) => ({ name: r.name, path: r.path }))
    const created = createdLocal
    if (created && !names.some((r) => r.name === created.name)) names.unshift(created)
    return names
  })
  /** The repository whose publish form is open. */
  let publishing = $state('')
  let publishBusy = $state(false)
  let publishError = $state<string | null>(null)
  let published = $state<Created | null>(null)
  let publishedForge = $state<ForgeProvider>('gitlab')
  async function publish(name: string) {
    publishBusy = true
    publishError = null
    published = null
    try {
      if (preview) throw new Error(t('Preview: nothing is published.'))
      const path = local.find((r) => r.name === name)?.path ?? ''
      published = await invoke<Created>('remote_project_publish', { name, remote: { namespace: namespace.trim(), visibility } })
      publishedForge = forgeOf(path)
      if (name === createdLocal?.name) createdLocal = null
      publishing = ''
    } catch (e) {
      publishError = e instanceof Error ? e.message : String(e)
    } finally {
      publishBusy = false
    }
  }

  ;(async () => {
    try {
      workspaces = preview
        ? [{ name: 'Work', dir: '~/Development', abs: '/Users/me/Development', namespace: 'example-org', forge: profileForge() }, { name: 'Personal', dir: '~/Perso', abs: '/Users/me/Perso', namespace: '', forge: 'github' }]
        : await invoke<Workspace[]>('workspaces')
      if (workspaces[0]) pick(workspaces[0])
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e)
    }
  })()

  /** As the Mac checks it (`clones::valid_name`): one folder, nothing a shell or git trips on. */
  const why = $derived.by(() => {
    const n = name.trim()
    if (!n) return ''
    if (n.startsWith('.') || n.startsWith('-')) return 'A name cannot start with . or -'
    return /^[\p{L}\p{N}._-]+$/u.test(n) ? '' : t('Letters, digits, - _ and . only')
  })

  /* ---------- Clone: a forge project into the working folder ---------- */

  let query = $state('')
  let found = $state<ForgeProject[] | null>(null)
  let searching = $state(false)
  let searchError = $state<string | null>(null)
  let cloning = $state('')
  let cloneError = $state<string | null>(null)
  let cloned = $state<Created | null>(null)
  /** Asked again a moment after typing, and for another working folder. */
  $effect(() => {
    const q = query.trim()
    const w = picked
    if (!w || preview) return
    let gone = false
    const timer = setTimeout(async () => {
      searching = true
      searchError = null
      try {
        const r = await invoke<ForgeProject[]>('remote_project_search', { workspace: w, query: q })
        if (!gone) found = r
      } catch (e) {
        if (!gone) searchError = e instanceof Error ? e.message : String(e)
      } finally {
        if (!gone) searching = false
      }
    }, 350)
    return () => {
      gone = true
      clearTimeout(timer)
    }
  })
  async function clone(project: string) {
    cloning = project
    cloneError = null
    cloned = null
    try {
      cloned = await invoke<Created>('remote_project_clone', { workspace: picked, project })
      found = found?.map((p) => (p.path === project ? { ...p, added: true } : p)) ?? null
    } catch (e) {
      cloneError = e instanceof Error ? e.message : String(e)
    } finally {
      cloning = ''
    }
  }

  async function create() {
    busy = true
    error = null
    made = null
    try {
      if (preview) throw new Error(t('Preview: nothing is created.'))
      made = await invoke<Created>('remote_project_create', { workspace: picked, name: name.trim(), remote: onForge ? { namespace: namespace.trim(), visibility } : null })
      if (!made.project) createdLocal = { name: name.trim(), path: made.path }
      name = ''
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="project">
  {#if loadError}
    <p class="note ko">{loadError}</p>
  {:else if workspaces === undefined}
    <p class="note">{t('Asking the Mac…')}</p>
  {:else if !workspaces.length}
    <p class="banner">{t('No working folder on the Mac: add one in Settings › Code › Repositories › Working folders there, then come back.')}</p>
  {:else}
    <p class="banner">{t('In')} <code>{dir}</code> {t('on the Mac: a git repository, a README.md and a first commit; added to the repositories. Confirmed by your passkey.')}</p>
    <div class="box">
      {#if workspaces.length > 1}
        <div class="field"><b>{t('Working folder')}</b>
          <div class="seg">{#each workspaces as w (w.name)}<button class:on={picked === w.name} onclick={() => pick(w)}>{w.name}</button>{/each}</div>
        </div>
      {/if}
      <label class="field"><b>{t('Name')}</b>
        <input type="text" bind:value={name} placeholder="my-app" autocapitalize="off" autocorrect="off" spellcheck="false" />
        {#if why}<span class="h ko">{why}</span>{:else if name.trim()}<span class="h">{dir}/{name.trim()}</span>{/if}
      </label>
      <div class="field">
        <div class="l"><b>{t('Also on {forge}', { forge: forgeLabel(ws?.forge) })}</b><button class="sw" class:on={onForge} onclick={() => (onForge = !onForge)} aria-label={t('Also on {forge}', { forge: forgeLabel(ws?.forge) })} aria-pressed={onForge}></button></div>
        <span class="h">{t('Its project created with {cli} (the Mac’s login), set as origin and pushed.', { cli: ws?.forge === 'github' ? 'gh' : 'glab' })}</span>
      </div>
      {#if onForge}
        <label class="field"><b>{ws?.forge === 'github' ? t('Owner') : t('Group')}</b>
          <input type="text" bind:value={namespace} placeholder={t('empty: your own account')} autocapitalize="off" autocorrect="off" spellcheck="false" />
        </label>
        <label class="field"><b>{t('Visibility')}</b>
          <select bind:value={visibility}>
            <option value="private">{t('private')}</option>
            <option value="internal">{t('internal')}</option>
          </select>
        </label>
      {/if}
    </div>
    <button class="pri" onclick={create} disabled={busy || !name.trim() || !!why}>{busy ? (onForge ? t('Creating on {forge}…', { forge: forgeLabel(ws?.forge) }) : t('Creating…')) : onForge ? t('Create here and on {forge}', { forge: forgeLabel(ws?.forge) }) : t('Create')}</button>
    {#if error}<p class="note ko">{error}</p>{/if}
    {#if made}
      <p class="banner ok"><Icon name="check" />{made.project ? t('{path} created, {project} on {forge}.', { path: made.path, project: made.project, forge: forgeLabel(ws?.forge) }) : t('{path} created.', { path: made.path })} {t('In the repositories once Prism restarts (More › Restart Prism).')}</p>
      {#if made.warning}<p class="note ko">{made.warning}</p>{/if}
    {/if}
    <p class="sect">{t('Clone from {forge}', { forge: forgeLabel(ws?.forge) })}</p>
    <div class="box">
      <label class="field"><b>{t('Project')}</b>
        <input type="search" bind:value={query} placeholder={t('search your projects')} autocapitalize="off" autocorrect="off" spellcheck="false" />
        <span class="h plain">{t('Cloned into')} <code>{dir}</code> {t('with {cli} (the Mac’s login), added to the repositories. Confirmed by your passkey.', { cli: ws?.forge === 'github' ? 'gh' : 'glab' })}</span>
      </label>
      {#if searchError}<div class="field"><span class="h ko">{searchError}</span></div>{/if}
      {#each found ?? [] as p (p.path)}
        <div class="field">
          <div class="l"><b class="mono">{p.path}</b>
            {#if p.added}<span class="h plain">{t('added')}</span>{:else}<button class="sm" onclick={() => clone(p.path)} disabled={!!cloning}>{cloning === p.path ? t('Cloning…') : t('Clone')}</button>{/if}
          </div>
          {#if p.description}<span class="h plain">{p.description}</span>{/if}
        </div>
      {:else}
        {#if found && !searching}<div class="field"><span class="h plain">{query.trim() ? t('No project matches.') : t('No project.')}</span></div>{:else if searching}<div class="field"><span class="h plain">{t('Searching…')}</span></div>{/if}
      {/each}
    </div>
    {#if cloneError}<p class="note ko">{cloneError}</p>{/if}
    {#if cloned}
      <p class="banner ok"><Icon name="check" />{cloned.project} cloned into {cloned.path}. In the repositories once Prism restarts (More › Restart Prism).</p>
      {#if cloned.warning}<p class="note ko">{cloned.warning}</p>{/if}
    {/if}
    {#if local.length}
      <p class="sect">{t('Kept local')}</p>
      <div class="box">
        {#each local as r (r.name)}
          <div class="field">
            <div class="l"><b>{r.name}</b>
              <button class="sm" onclick={() => { publishing = publishing === r.name ? '' : r.name; publishError = null; namespace = workspaceOf(r.path)?.namespace ?? '' }} disabled={publishBusy}>{publishing === r.name ? t('Cancel') : t('Publish on {forge}', { forge: forgeLabel(forgeOf(r.path)) })}</button>
            </div>
            <span class="h">{r.path}</span>
            {#if publishing === r.name}
              <label class="sub"><span>{forgeOf(r.path) === 'github' ? t('Owner') : t('Group')}</span>
                <input type="text" bind:value={namespace} placeholder={t('empty: your own account')} autocapitalize="off" autocorrect="off" spellcheck="false" />
              </label>
              <label class="sub"><span>{t('Visibility')}</span>
                <select bind:value={visibility}>
                  <option value="private">{t('private')}</option>
                  <option value="internal">{t('internal')}</option>
                      </select>
              </label>
              <span class="h plain">{t('Named after its folder, set as origin, its current branch pushed. Confirmed by your passkey.')}</span>
              <button class="pri" onclick={() => publish(r.name)} disabled={publishBusy}>{publishBusy ? t('Publishing on {forge}…', { forge: forgeLabel(forgeOf(r.path)) }) : t('Publish')}</button>
            {/if}
          </div>
        {/each}
      </div>
      {#if publishError}<p class="note ko">{publishError}</p>{/if}
    {/if}
    {#if published}
      <p class="banner ok"><Icon name="check" />{t('{path} is {project} on {forge}.', { path: published.path, project: published.project ?? '', forge: forgeLabel(publishedForge) })} {publishedForge === profileForge() ? t('Its MRs once Prism restarts (More › Restart Prism).') : t('Its link once Prism restarts (More › Restart Prism).')}</p>
      {#if published.warning}<p class="note ko">{published.warning}</p>{/if}
    {/if}
  {/if}
</div>

<style>
  .project { display: flex; flex-direction: column; gap: 10px; }
  .banner { display: block; margin: 0; padding: 9px 12px; border-radius: 10px; background: var(--surface-muted); color: var(--text-muted); font-size: 12.5px; }
  .banner.ok { background: color-mix(in srgb, var(--ok-bg) 55%, transparent); color: var(--ok); }
  .banner :global(svg) { vertical-align: -2px; margin-right: 4px; }
  .banner code { font-family: var(--mono); color: var(--text); }
  .note { margin: 12px 4px; color: var(--text-muted); font-size: 13px; text-align: center; }
  .note.ko { color: var(--ko); }
  .sect { margin: 6px 4px -4px; color: var(--text-muted); font-size: 11px; letter-spacing: 0.05em; text-transform: uppercase; }
  .sub { display: flex; flex-direction: column; gap: 4px; margin-top: 4px; color: var(--text-muted); font-size: 12px; }
  .h.plain { font-family: inherit; }
  .seg { display: flex; flex-wrap: wrap; gap: 6px; }
  .seg button { padding: 6px 14px; border: 1px solid var(--border); border-radius: 16px; background: none; color: var(--text); font: inherit; font-size: 14px; }
  .seg button.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); }
  button.sm { padding: 5px 12px; border: 1px solid var(--border); border-radius: 14px; background: var(--surface); color: var(--text); font: inherit; font-size: 13px; }
  .box { border: 1px solid var(--border); border-radius: 12px; background: var(--surface); overflow: hidden; }
  .field { display: flex; flex-direction: column; gap: 5px; padding: 11px 14px; }
  .field + .field { border-top: 1px solid var(--border); }
  .field b { font-weight: 500; font-size: 14.5px; }
  .l { display: flex; align-items: center; gap: 10px; }
  .l b { flex: 1; }
  .h { color: var(--text-muted); font-size: 12px; font-family: var(--mono); overflow-wrap: anywhere; }
  .h.ko { color: var(--ko); font-family: inherit; }
  .mono { font-family: var(--mono); font-size: 13.5px; overflow-wrap: anywhere; }
  input[type='text'], input[type='search'], select { width: 100%; padding: 9px 10px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); color: var(--text); font: 14px var(--mono); }
  input:focus, select:focus { outline: none; border-color: var(--accent); }
  .sw { flex: none; width: 42px; height: 24px; padding: 0; border-radius: 12px; background: var(--surface-muted); border: 1px solid var(--border); position: relative; }
  .sw::after { content: ''; position: absolute; top: 2px; left: 2px; width: 18px; height: 18px; border-radius: 50%; background: var(--text-muted); transition: left 0.12s; }
  .sw.on { background: var(--ok); border-color: var(--ok); }
  .sw.on::after { left: 20px; background: var(--surface); }
  button.pri { align-self: flex-end; padding: 8px 16px; border: 1px solid var(--accent); border-radius: 18px; background: var(--accent); color: #fff; font: inherit; font-size: 14px; }
  button.pri:disabled { opacity: 0.5; }
</style>
