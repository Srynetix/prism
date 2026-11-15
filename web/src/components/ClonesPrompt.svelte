<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n.svelte'
  import Icon from './Icon.svelte'
  import Modal from './ui/Modal.svelte'
  import RepoClones from './settings/RepoClones.svelte'
  import { closeClonesPrompt, missingClones } from '../clones.svelte'
  import { cfg, loadConfig, mergedRows, saveEdits, setMergedRows } from '../settings.svelte'

  /**
   * At launch, once (after the first launch's setup and the pages wizard): the repositories a pack
   * names without a clone here. Their clones are found in a folder the user picks, chosen, or
   * cloned; saving writes their paths to config.toml and relaunches Prism, which reads them at
   * launch. Closing leaves them to Settings › Code › Repositories.
   */
  const REPOS = ['repos']
  const all = missingClones()
  let paths = $state<Record<string, string>>({})
  let busy = $state(false)
  let error = $state('')
  onMount(() => {
    if (!cfg.file) void loadConfig()
  })

  async function save() {
    busy = true
    error = ''
    try {
      setMergedRows(REPOS, mergedRows(REPOS).map((r) => (typeof r.name === 'string' && paths[r.name] ? { ...r, path: paths[r.name] } : r)))
      await saveEdits([{ path: REPOS, value: cfg.edits.repos ?? null }])
      closeClonesPrompt()
    } catch (e) {
      error = String(e)
      busy = false
    }
  }
</script>

<Modal open key="clones" label={t('Your local clones')} title={t('Your local clones')} width={720} onclose={() => !busy && closeClonesPrompt()}>
  <div class="body">
    <p class="intro">{t('These repositories have no clone on this machine: only their merge requests are read. Give them one for the Repos menu, worktrees and agents. Where you keep them is yours: Prism looks only where you point it.')}</p>
    {#if Object.keys(paths).length}
      <ul class="done">
        {#each Object.entries(paths) as [name, path] (name)}<li><Icon name="check" /><b>{name}</b><span>{path}</span></li>{/each}
      </ul>
    {/if}
    <RepoClones items={all.filter((c) => !paths[c.name])} onpath={(name, path) => (paths = { ...paths, [name]: path })} />
    {#if error}<p class="err">{error}</p>{/if}
  </div>
  {#snippet footer()}
    <span class="later">{t('Settings › Code › Repositories, later.')}</span>
    <button class="btn ghost" onclick={closeClonesPrompt} disabled={busy}>{t('Later')}</button>
    <button class="btn primary" class:busy onclick={save} disabled={busy || !cfg.file || !Object.keys(paths).length}>{#if busy}<span class="spinner"></span>{t('Saving…')}{:else}{t('Save')}{/if}</button>
  {/snippet}
</Modal>

<style>
  .body { display: flex; flex-direction: column; gap: 10px; min-height: 0; padding: var(--sp-5) var(--sp-6); overflow: auto; }
  .intro { margin: 0; color: var(--text-muted); font-size: var(--fs-meta); line-height: 1.45; }
  .done { display: flex; flex-direction: column; gap: 4px; margin: 0; padding: 0; list-style: none; font-size: var(--fs-meta); }
  .done li { display: flex; align-items: center; gap: 8px; color: var(--ok); }
  .done b { color: var(--text); font-weight: 550; }
  .done span { font-family: var(--mono); color: var(--text-muted); }
  .body .err { margin: 0; }
  .later { flex: 1; font-size: var(--fs-meta); color: var(--text-muted); }
</style>
