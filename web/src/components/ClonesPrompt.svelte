<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n.svelte'
  import Icon from './Icon.svelte'
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

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => e.target === e.currentTarget && !busy && closeClonesPrompt()}>
  <div class="sheet" role="dialog" aria-label={t('Your local clones')}>
    <header>
      <div>
        <b>{t('Your local clones')}</b>
        <p>{t('These repositories have no clone on this machine: only their merge requests are read. Give them one for the Repos menu, worktrees and agents. Where you keep them is yours: Prism looks only where you point it.')}</p>
      </div>
      <button class="x" onclick={closeClonesPrompt} aria-label={t('Close')}><Icon name="close" /></button>
    </header>
    <div class="body">
      {#if Object.keys(paths).length}
        <ul class="done">
          {#each Object.entries(paths) as [name, path] (name)}<li><Icon name="check" /><b>{name}</b><span>{path}</span></li>{/each}
        </ul>
      {/if}
      <RepoClones items={all.filter((c) => !paths[c.name])} onpath={(name, path) => (paths = { ...paths, [name]: path })} />
      {#if error}<p class="err">{error}</p>{/if}
    </div>
    <footer>
      <span>{t('Settings › Code › Repositories, later.')}</span>
      <button class="btn ghost" onclick={closeClonesPrompt} disabled={busy}>{t('Later')}</button>
      <button class="btn primary" onclick={save} disabled={busy || !cfg.file || !Object.keys(paths).length}>{busy ? t('Saving…') : t('Save')}</button>
    </footer>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 60; display: flex; align-items: flex-start; justify-content: center; padding-top: 10vh; background: rgba(0, 0, 0, 0.5); }
  .sheet { width: min(760px, 94vw); max-height: 80vh; display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--r14); background: var(--surface); box-shadow: var(--shadow-modal); overflow: hidden; }
  header { display: flex; align-items: flex-start; gap: 12px; padding: 16px 18px 12px; border-bottom: 1px solid var(--border); }
  header div { flex: 1; }
  header b { font-size: 15px; }
  header p { margin: 4px 0 0; color: var(--text-muted); font-size: var(--fs-meta); line-height: 1.45; }
  .x { padding: 4px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .body { display: flex; flex-direction: column; gap: 10px; padding: 14px 18px; overflow: auto; }
  .done { display: flex; flex-direction: column; gap: 4px; margin: 0; padding: 0; list-style: none; font-size: var(--fs-meta); }
  .done li { display: flex; align-items: center; gap: 8px; color: var(--ok); }
  .done b { color: var(--text); font-weight: 550; }
  .done span { font-family: var(--mono); color: var(--text-muted); }
  .err { margin: 0; color: var(--ko); font-size: var(--fs-meta); }
  footer { display: flex; align-items: center; gap: 8px; padding: 12px 18px; border-top: 1px solid var(--border); }
  footer span { flex: 1; font-size: var(--fs-meta); color: var(--text-muted); }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 5px 14px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); cursor: pointer; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn.ghost { background: none; color: var(--text-muted); }
  .btn.primary { border-color: var(--accent); background: var(--accent); color: var(--on-accent); background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--accent); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
</style>
