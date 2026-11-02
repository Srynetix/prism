<script lang="ts">
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import { canPick, pickPath } from '../../pick'
  import { cloneRepo, findClones, notCloneOf, type CloneItem } from '../../clones.svelte'
  import { mergedRows, setMergedRows } from '../../settings.svelte'

  /**
   * The repositories without a local clone, and three ways to give them one: find them in a folder
   * the user picks (only there, a few levels down, by their remote), choose a folder (checked to
   * be a clone of the project), or clone the project with glab or gh. Without `items`, those of
   * Settings › Repositories, whose paths become pending edits; the launch prompt passes its own.
   */
  let { items, onpath }: { items?: CloneItem[]; onpath?: (name: string, path: string) => void } = $props()

  const REPOS = ['repos']
  const list = $derived(
    items ??
      mergedRows(REPOS)
        .filter((r) => !r.path && typeof r.name === 'string' && r.name)
        .map((r) => ({ name: r.name as string, project: (r.project as string | undefined) ?? null, note: t('no local clone') })),
  )
  function setPath(name: string, path: string) {
    if (onpath) return onpath(name, path)
    setMergedRows(REPOS, mergedRows(REPOS).map((r) => (r.name === name ? { ...r, path } : r)))
  }

  let busy = $state<Record<string, string>>({})
  let errors = $state<Record<string, string>>({})
  let summary = $state('')

  async function findIn() {
    const root = await pickPath('~')
    if (!root) return
    const wanted = list.filter((c) => c.project)
    busy = { ...busy, '*': t('Looking…') }
    summary = ''
    try {
      const found = await findClones(root, wanted.map((c) => c.project!))
      for (const f of found) {
        const c = wanted.find((w) => w.project!.toLowerCase() === f.project.toLowerCase())
        if (c) setPath(c.name, f.path)
      }
      const left = wanted.filter((w) => !found.some((f) => f.project.toLowerCase() === w.project!.toLowerCase()))
      summary = left.length ? t('{found} of {wanted} found in {root}; not there: {names}.', { found: found.length, wanted: wanted.length, root, names: left.map((c) => c.name).join(', ') }) : t('{found} of {wanted} found in {root}.', { found: found.length, wanted: wanted.length, root })
    } catch (e) {
      summary = String(e)
    } finally {
      const { '*': _, ...rest } = busy
      busy = rest
    }
  }

  async function choose(c: CloneItem) {
    const path = await pickPath('~')
    if (!path) return
    errors = { ...errors, [c.name]: '' }
    const why = await notCloneOf(path, c.project).catch((e) => String(e))
    if (why) errors = { ...errors, [c.name]: `${path}: ${why}` }
    else setPath(c.name, path)
  }

  async function cloneInto(c: CloneItem) {
    if (!c.project) return
    const parent = await pickPath('~')
    if (!parent) return
    busy = { ...busy, [c.name]: t('Cloning…') }
    errors = { ...errors, [c.name]: '' }
    try {
      setPath(c.name, await cloneRepo(c.project, parent))
    } catch (e) {
      errors = { ...errors, [c.name]: String(e) }
    } finally {
      const { [c.name]: _, ...rest } = busy
      busy = rest
    }
  }
</script>

{#if list.length}
  <div class="clones">
    <div class="top">
      <span><b>{t('Without a local clone')}</b> · {t('only their MRs are read. Your clones’ place is yours: a pack never sets it.')}</span>
      {#if canPick() && list.some((c) => c.project)}<button class="btn" onclick={findIn} disabled={!!busy['*']} title={t('Pick the folder where you keep your repositories: Prism looks there only, a few levels down, for clones of these projects (by their remote)')}><Icon name="folder" />{busy['*'] ?? t('Find in a folder…')}</button>{/if}
    </div>
    {#if summary}<p class="sum">{summary}</p>{/if}
    {#each list as c (c.name)}
      <div class="item">
        <b>{c.name}</b>
        <span class="proj">{c.project ?? t('no project')}</span>
        <small>{c.note}</small>
        {#if canPick()}
          <span class="acts">
            <button class="btn" onclick={() => choose(c)} disabled={!!busy[c.name]} title={t('Pick its clone: checked against the project’s remote')}>{t('Choose…')}</button>
            {#if c.project}<button class="btn" onclick={() => cloneInto(c)} disabled={!!busy[c.name]} title={t('Clone {project} into a folder you pick (glab or gh, with their login)', { project: c.project })}>{busy[c.name] ?? t('Clone into…')}</button>{/if}
          </span>
        {/if}
        {#if errors[c.name]}<p class="err">{errors[c.name]}</p>{/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .clones { display: flex; flex-direction: column; gap: 6px; padding: 10px 12px; border: 1px dashed var(--border); border-radius: var(--r8); }
  .top { display: flex; align-items: center; gap: 10px; font-size: var(--fs-meta); color: var(--text-muted); }
  .top span { flex: 1; }
  .top b { color: var(--text); font-weight: 550; }
  .sum { margin: 0; font-size: var(--fs-meta); color: var(--text-muted); }
  .item { display: grid; grid-template-columns: 120px minmax(0, 1fr) auto auto; align-items: center; gap: 4px 10px; }
  .proj { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: var(--fs-meta) var(--mono); color: var(--text-muted); }
  .item small { font-size: var(--fs-meta); color: var(--warn); }
  .acts { display: flex; gap: 6px; }
  .err { grid-column: 1 / -1; margin: 0; font-size: var(--fs-meta); color: var(--ko); }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: 0.6; cursor: default; }
</style>
